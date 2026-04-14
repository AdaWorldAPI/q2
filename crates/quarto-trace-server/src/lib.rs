//! Local HTTP server for the Quarto trace viewer SPA.
//!
//! Serves two kinds of routes:
//!
//! - `GET /*` — the SPA bundle, embedded at compile time from
//!   `trace-viewer/dist/` via `include_dir!`. At runtime the env var
//!   `QUARTO_TRACE_VIEWER_DIR` can override this to serve directly from
//!   disk, enabling UI iteration without Rust rebuilds.
//! - `GET /api/traces` — JSON listing of available traces under
//!   `.quarto/trace/`.
//! - `GET /api/trace/<doc>` — JSON trace for the given document stem.
//!
//! The server binds to `127.0.0.1` by default (loopback only) and emits
//! the URL on stdout so the caller can open a browser.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use include_dir::{Dir, include_dir};
use quarto_trace::read::{list_traces, read_trace};
use serde_json::json;

/// The SPA bundle embedded at build time. See `build.rs` for how the
/// source directory is chosen (real `trace-viewer/dist/` if present, else
/// a placeholder).
static EMBEDDED_SPA: Dir<'_> = include_dir!("$QUARTO_TRACE_VIEWER_EMBED_DIR");

/// Configuration for the trace-viewer server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Root of the `.quarto/trace/` directory tree to serve.
    pub trace_dir: PathBuf,
    /// Host to bind to. Defaults to `127.0.0.1`.
    pub host: String,
    /// Port to bind to. `0` lets the OS pick one.
    pub port: u16,
    /// If set, serve SPA assets from this directory at runtime instead of
    /// the embedded bundle. Useful for UI iteration.
    pub spa_dir_override: Option<PathBuf>,
}

impl ServerConfig {
    /// Construct a config with sensible defaults.
    pub fn new(trace_dir: PathBuf) -> Self {
        Self {
            trace_dir,
            host: "127.0.0.1".to_string(),
            port: 0,
            spa_dir_override: std::env::var("QUARTO_TRACE_VIEWER_DIR")
                .ok()
                .map(PathBuf::from),
        }
    }
}

/// Shared server state.
#[derive(Clone)]
struct AppState {
    trace_dir: Arc<PathBuf>,
    spa_dir_override: Option<Arc<PathBuf>>,
}

/// Build the axum router. Exposed for testing.
pub fn router(config: &ServerConfig) -> Router {
    let state = AppState {
        trace_dir: Arc::new(config.trace_dir.clone()),
        spa_dir_override: config.spa_dir_override.clone().map(Arc::new),
    };

    Router::new()
        .route("/api/traces", get(list_handler))
        .route("/api/trace/{doc}", get(show_handler))
        .fallback(get(spa_handler))
        .with_state(state)
}

/// Run the server until the process is signalled.
///
/// Prints the bound URL to stdout on startup. Returns when the server
/// shuts down (e.g. on Ctrl-C).
pub async fn serve(config: ServerConfig) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .with_context(|| format!("Invalid host:port {}:{}", config.host, config.port))?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    let bound = listener.local_addr()?;
    let url = format!("http://{}", bound);
    println!("Quarto trace viewer listening at {}", url);
    println!("Serving traces from {}", config.trace_dir.display());
    if let Some(override_dir) = &config.spa_dir_override {
        println!("SPA assets from {}", override_dir.display());
    } else {
        println!("SPA assets from embedded bundle");
    }
    println!("Press Ctrl-C to stop.");

    let app = router(&config);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_handler(State(state): State<AppState>) -> impl IntoResponse {
    let trace_dir = state.trace_dir.as_path();
    let listings = list_traces(trace_dir);
    let entries: Vec<_> = listings
        .into_iter()
        .map(|l| {
            json!({
                "doc": l.doc_stem,
                "path": l.latest_path.display().to_string(),
            })
        })
        .collect();
    Json(json!({
        "trace_dir": trace_dir.display().to_string(),
        "traces": entries,
    }))
}

async fn show_handler(State(state): State<AppState>, AxumPath(doc): AxumPath<String>) -> Response {
    let candidate = state.trace_dir.join(&doc).join("latest.json");
    // Refuse any path that escapes trace_dir — belt-and-suspenders against
    // path traversal. Canonicalize both sides for a robust comparison;
    // fall back to best-effort string prefix check if canonicalization
    // fails (e.g. file missing).
    if !is_within(&state.trace_dir, &candidate) {
        return (StatusCode::BAD_REQUEST, "invalid doc").into_response();
    }
    if !candidate.is_file() {
        return (StatusCode::NOT_FOUND, "no such trace").into_response();
    }
    match read_trace(&candidate) {
        Ok(doc) => Json(doc).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn spa_handler(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let path = req.uri().path();
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    if let Some(override_dir) = state.spa_dir_override.as_deref() {
        return serve_from_disk(override_dir, rel).await;
    }

    // Embedded bundle.
    if let Some(file) = EMBEDDED_SPA.get_file(rel) {
        return asset_response(rel, file.contents().to_vec());
    }
    // SPA fallback: any non-asset path gets index.html.
    if let Some(index) = EMBEDDED_SPA.get_file("index.html") {
        return asset_response("index.html", index.contents().to_vec());
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

async fn serve_from_disk(root: &Path, rel: &str) -> Response {
    let candidate = root.join(rel);
    if !is_within(root, &candidate) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }
    if candidate.is_file() {
        match tokio::fs::read(&candidate).await {
            Ok(bytes) => return asset_response(rel, bytes),
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }
    // Fallback to index.html for client-side routing.
    let index = root.join("index.html");
    if index.is_file() {
        match tokio::fs::read(&index).await {
            Ok(bytes) => asset_response("index.html", bytes),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, "not found").into_response()
    }
}

fn asset_response(path: &str, bytes: Vec<u8>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type_for(path)),
    );
    (StatusCode::OK, headers, bytes).into_response()
}

fn content_type_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    // Try canonicalization first; fall back to normalized lexical check.
    match (root.canonicalize(), candidate.canonicalize()) {
        (Ok(r), Ok(c)) => c.starts_with(&r),
        _ => {
            // Lexical fallback: reject any `..` component that would climb
            // above the root.
            let mut depth: i32 = 0;
            for comp in candidate
                .strip_prefix(root)
                .unwrap_or(candidate)
                .components()
            {
                use std::path::Component;
                match comp {
                    Component::ParentDir => depth -= 1,
                    Component::Normal(_) => depth += 1,
                    _ => {}
                }
                if depth < 0 {
                    return false;
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;
    use http_body_util::BodyExt;
    use quarto_trace::{RenderInfo, StageStatus, TraceDocument, TraceEntry, write::write_trace};
    use tower::ServiceExt;

    fn fixture_dir(label: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir()
            .join(format!("qts-test-{}-{}-{}", label, std::process::id(), ts))
            .join("trace");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_fixture(root: &Path, stem: &str) {
        let dir = root.join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let mut doc = TraceDocument::new(RenderInfo {
            input_path: Some(format!("{}.qmd", stem)),
            format_target: Some("html".into()),
            ..Default::default()
        });
        doc.pipeline.push(TraceEntry {
            stage: "parse".into(),
            index: 0,
            data_kind: Some("DocumentAst".into()),
            data: Some(json!({"blocks": []})),
            duration_ms: Some(1.0),
            status: StageStatus::Ok,
            error: None,
        });
        write_trace(&doc, &dir.join("latest.json")).unwrap();
    }

    #[tokio::test]
    async fn api_traces_lists_fixtures() {
        let root = fixture_dir("list");
        write_fixture(&root, "a");
        write_fixture(&root, "b");

        let app = router(&ServerConfig::new(root.clone()));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/traces")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let traces = v["traces"].as_array().unwrap();
        assert_eq!(traces.len(), 2);
    }

    #[tokio::test]
    async fn api_trace_returns_trace() {
        let root = fixture_dir("show");
        write_fixture(&root, "only");

        let app = router(&ServerConfig::new(root));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/trace/only")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["pipeline"][0]["stage"], "parse");
    }

    #[tokio::test]
    async fn api_trace_missing_returns_404() {
        let root = fixture_dir("missing");
        let app = router(&ServerConfig::new(root));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/trace/nope")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_trace_refuses_path_traversal() {
        let root = fixture_dir("traversal");
        write_fixture(&root, "inside");
        let app = router(&ServerConfig::new(root.clone()));
        // Axum normalizes `..` in the path; this test defends against any
        // way a user might try to escape the root via a decoded stem.
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/trace/..%2F..%2Fetc")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            res.status() == StatusCode::BAD_REQUEST || res.status() == StatusCode::NOT_FOUND,
            "got {}",
            res.status()
        );
    }

    #[tokio::test]
    async fn spa_root_serves_index_html() {
        let root = fixture_dir("spa");
        // Use an override dir so we don't depend on the embedded bundle.
        let spa_dir = std::env::temp_dir().join(format!("qts-spa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&spa_dir);
        std::fs::create_dir_all(&spa_dir).unwrap();
        std::fs::write(
            spa_dir.join("index.html"),
            "<!doctype html><title>test</title>",
        )
        .unwrap();

        let mut config = ServerConfig::new(root);
        config.spa_dir_override = Some(spa_dir);

        let app = router(&config);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get(header::CONTENT_TYPE).unwrap();
        assert!(ct.to_str().unwrap().starts_with("text/html"));
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains("<title>test"));
    }

    #[tokio::test]
    async fn spa_unknown_path_falls_back_to_index() {
        let root = fixture_dir("spa-fallback");
        let spa_dir = std::env::temp_dir().join(format!(
            "qts-spa-fb-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&spa_dir);
        std::fs::create_dir_all(&spa_dir).unwrap();
        std::fs::write(spa_dir.join("index.html"), "FALLBACK").unwrap();

        let mut config = ServerConfig::new(root);
        config.spa_dir_override = Some(spa_dir);

        let app = router(&config);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/some/spa/route")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(std::str::from_utf8(&body).unwrap(), "FALLBACK");
    }
}
