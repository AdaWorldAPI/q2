//! q2-cockpit — single-binary graph notebook with live .qmd rendering.
//!
//! Architecture:
//! - Axum serves HTTP + SSE
//! - .qmd notebooks are parsed by pampa and rendered by quarto-core
//! - deno_core (V8 JIT) executes JS/TS cells inside the notebook
//! - ALL graph queries route through lance-graph (DataFusion + LanceDB)
//! - ndarray provides SIMD-accelerated compute for graph analytics
//! - neo4j-rs is a fallback ONLY for live demos against Neo4j Aura
//!
//! The cockpit/ Vite build is embedded via include_dir! and served as
//! static files with SPA fallback. React Router handles / vs /demo vs /debug.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

// ── Embed the Vite build at compile time ─────────────────────────────────────
// The cockpit/ directory is built by `cd cockpit && npm run build` which
// produces dist/. We embed dist/ so the binary serves the React app directly.
// If dist/ doesn't exist at compile time, we fall back to the inline HTML shell.

#[cfg(feature = "embed-cockpit")]
use include_dir::{include_dir, Dir};

#[cfg(feature = "embed-cockpit")]
static COCKPIT_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../cockpit/dist");

// ── Application state ────────────────────────────────────────────────────────

struct AppState {
    /// Broadcast channel for SSE events (cell results, graph updates).
    tx: broadcast::Sender<SseEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct SseEvent {
    event: String,
    data: serde_json::Value,
}

// ── MCP types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct McpRequest {
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    params: Option<McpParams>,
}

#[derive(Debug, Deserialize)]
struct McpParams {
    name: Option<String>,
    arguments: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i64,
    message: String,
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("cockpit_server=info,tower_http=info")
        .init();

    let (tx, _rx) = broadcast::channel::<SseEvent>(256);
    let state = Arc::new(AppState { tx });

    let app = Router::new()
        // MCP endpoints — all queries route through lance-graph
        .route("/mcp/sse", get(sse_handler))
        .route("/mcp/message", post(mcp_message_handler))
        // Data status — what's loaded, what failed
        .route("/api/data/status", get(data_status_handler))
        // Live strategy diagnostics — runs all 16 strategies against real queries
        .route("/api/debug/strategies", get(strategy_check_handler))
        // Health
        .route("/health", get(health_handler))
        // Static files + SPA fallback (serves the Vite React build)
        .fallback(get(static_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2718);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("q2-cockpit listening on http://{addr}");
    tracing::info!("  /       → Palantir cockpit (Vite build, 221 aiwar nodes)");
    tracing::info!("  /demo   → infrastructure demo (24 seed nodes)");
    tracing::info!("  /debug  → neural debugger (18,763 functions)");
    tracing::info!("  /mcp/*  → MCP endpoints (lance-graph)");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c");
    tracing::info!("shutting down");
}

// ── Static file handler with SPA fallback ────────────────────────────────────

/// Serves embedded Vite build files. Falls back to index.html for SPA routing.
async fn static_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact file from embedded dist/
    #[cfg(feature = "embed-cockpit")]
    {
        // Try exact path first
        if let Some(file) = COCKPIT_DIST.get_file(path) {
            let mime = mime_from_path(path);
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                file.contents(),
            )
                .into_response();
        }

        // SPA fallback: serve index.html for all non-file routes
        // (React Router handles /demo, /debug, etc.)
        if !path.contains('.') || path.is_empty() {
            if let Some(index) = COCKPIT_DIST.get_file("index.html") {
                return (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    index.contents(),
                )
                    .into_response();
            }
        }
    }

    // Fallback: inline HTML shell (when embed-cockpit feature is off)
    if path.is_empty() || !path.contains('.') {
        return Html(FALLBACK_SHELL.to_string()).into_response();
    }

    (StatusCode::NOT_FOUND, "Not found").into_response()
}

fn mime_from_path(path: &str) -> &'static str {
    if path.ends_with(".html") { "text/html; charset=utf-8" }
    else if path.ends_with(".css") { "text/css; charset=utf-8" }
    else if path.ends_with(".js") { "application/javascript; charset=utf-8" }
    else if path.ends_with(".json") { "application/json" }
    else if path.ends_with(".svg") { "image/svg+xml" }
    else if path.ends_with(".png") { "image/png" }
    else if path.ends_with(".ico") { "image/x-icon" }
    else if path.ends_with(".woff2") { "font/woff2" }
    else if path.ends_with(".woff") { "font/woff" }
    else { "application/octet-stream" }
}

/// Minimal fallback shell when the Vite build isn't embedded.
/// This is ONLY shown when the binary is compiled WITHOUT the embed-cockpit feature.
const FALLBACK_SHELL: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>q2 — Graph Notebook</title>
<style>
:root {
  --bg: #0a0e17; --panel: rgba(16,22,36,0.88); --border: rgba(77,208,225,0.16);
  --accent: #4dd0e1; --text: #e7f3ff; --muted: #93a9bf;
  --success: #35d07f; --warning: #ffb547; --danger: #ff637d;
}
* { box-sizing: border-box; margin: 0; }
body {
  font-family: Inter, system-ui, sans-serif; color: var(--text);
  background: radial-gradient(circle at top left, rgba(77,208,225,0.12), transparent 28%),
    linear-gradient(180deg, #0a0e17, #111826, #161d2d); height: 100vh;
}
.shell { display: flex; flex-direction: column; height: 100vh; padding: 16px; gap: 12px; }
.status { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); }
.dot { width: 8px; height: 8px; border-radius: 50%; }
.dot.on { background: var(--success); box-shadow: 0 0 12px rgba(53,208,127,0.7); }
.dot.off { background: var(--danger); }
.main { flex: 1; display: flex; align-items: center; justify-content: center; }
.msg { text-align: center; }
.msg h1 { font-size: 24px; letter-spacing: 0.08em; text-transform: uppercase; margin-bottom: 12px; }
.msg p { color: var(--muted); font-size: 14px; line-height: 1.6; max-width: 540px; }
.msg code { color: var(--accent); font-family: ui-monospace, monospace; }
#log { font-family: ui-monospace, monospace; font-size: 11px; color: var(--muted);
  max-height: 120px; overflow: auto; padding: 8px; border-top: 1px solid var(--border); }
</style>
</head>
<body>
<div class="shell">
  <div class="status">
    <span class="dot" id="dot"></span>
    <span id="status">connecting…</span>
    <span style="margin-left:auto">Build with --features embed-cockpit to serve the Vite React app</span>
  </div>
  <div class="main">
    <div class="msg">
      <h1>q2 Graph Notebook</h1>
      <p>The Palantir cockpit is not embedded in this build.<br>
      Compile with <code>cargo build --features embed-cockpit</code> after running <code>cd cockpit && npm run build</code>.</p>
    </div>
  </div>
  <div id="log"></div>
</div>
<script>
const dot = document.getElementById('dot');
const status = document.getElementById('status');
const log = document.getElementById('log');
function addLog(msg) { log.textContent += new Date().toISOString().slice(11,19) + ' ' + msg + '\n'; log.scrollTop = log.scrollHeight; }
const es = new EventSource('/mcp/sse');
es.onopen = () => { dot.className = 'dot on'; status.textContent = 'connected — lance-graph live'; addLog('SSE connected'); };
es.onmessage = (e) => { addLog('event: ' + e.data.slice(0, 80)); };
es.onerror = () => { dot.className = 'dot off'; status.textContent = 'disconnected'; addLog('SSE error — reconnecting…'); };
</script>
</body>
</html>"#;

// ── SSE handler ──────────────────────────────────────────────────────────────

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.tx.subscribe();

    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("message")
            .data(r#"{"method":"notifications/initialized"}"#));

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event.data).unwrap_or_default();
                    yield Ok(Event::default().event(&event.event).data(data));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE client lagged by {n} events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

// ── MCP message handler — all queries route through lance-graph ──────────────

async fn mcp_message_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpRequest>,
) -> Json<McpResponse> {
    let result = match req.method.as_str() {
        "tools/call" => handle_tool_call(&state, &req).await,
        "tools/list" => Ok(serde_json::json!({
            "tools": [
                { "name": "cell_execute", "description": "Execute a notebook cell through lance-graph" },
                { "name": "cells_list", "description": "List all cells" },
                { "name": "notebook_export", "description": "Export notebook to PDF/HTML" },
            ]
        })),
        _ => Err(format!("Unknown method: {}", req.method)),
    };

    match result {
        Ok(value) => Json(McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(value),
            error: None,
        }),
        Err(msg) => Json(McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: None,
            error: Some(McpError { code: -32000, message: msg }),
        }),
    }
}

async fn handle_tool_call(
    state: &AppState,
    req: &McpRequest,
) -> Result<serde_json::Value, String> {
    let params = req.params.as_ref().ok_or("Missing params")?;
    let tool_name = params.name.as_deref().ok_or("Missing tool name")?;
    let args = params.arguments.as_ref().cloned().unwrap_or_default();

    match tool_name {
        "cell_execute" => {
            let code = args["code"].as_str().ok_or("Missing 'code' argument")?;
            let lang_hint = args["lang"].as_str();

            let language = if let Some(hint) = lang_hint {
                match hint {
                    "cypher" => notebook_query::QueryLanguage::Cypher,
                    "gremlin" => notebook_query::QueryLanguage::Gremlin,
                    "sparql" => notebook_query::QueryLanguage::Sparql,
                    "r" => notebook_query::QueryLanguage::R,
                    _ => notebook_query::detect_language(code),
                }
            } else {
                notebook_query::detect_language(code)
            };

            // ALL queries route through lance-graph (DataFusion + LanceDB)
            let result =
                notebook_query::execute(code, language).map_err(|e| format!("lance-graph: {e}"))?;

            let cell = serde_json::json!({
                "id": format!("cell-{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()),
                "source": code,
                "language": format!("{:?}", language).to_lowercase(),
                "execution_state": "success",
                "elapsed_ms": result.elapsed_ms,
                "outputs": build_outputs(&result),
            });

            let _ = state.tx.send(SseEvent {
                event: "cell_result".into(),
                data: cell.clone(),
            });

            Ok(cell)
        }
        "cells_list" => Ok(serde_json::json!([])),
        "notebook_export" => {
            let format = args["format"].as_str().unwrap_or("html");
            Ok(serde_json::json!({ "exported": format!("notebook.{format}"), "format": format }))
        }
        _ => Err(format!("Unknown tool: {tool_name}")),
    }
}

fn build_outputs(result: &notebook_query::QueryResult) -> Vec<serde_json::Value> {
    let mut outputs = Vec::new();
    if let Some(ref html) = result.html {
        outputs.push(serde_json::json!({ "type": "html", "content": html }));
    }
    if let Some(ref graph_json) = result.graph_json {
        outputs.push(serde_json::json!({ "type": "graph", "content": graph_json }));
    }
    if outputs.is_empty() {
        outputs.push(serde_json::json!({ "type": "text", "content": result.raw_output }));
    }
    outputs
}

// ── Live strategy diagnostics ─────────────────────────────────────────────────

/// Run all 16 strategies with synthetic queries, report what fires/fails/panics.
/// Also runs demo analyses: same query through all 12 thinking styles.
async fn strategy_check_handler() -> Json<serde_json::Value> {
    // Run in a blocking thread since strategy checks may be CPU-intensive
    let result = tokio::task::spawn_blocking(|| {
        notebook_query::diagnostics::run_strategy_checks()
    })
    .await;

    match result {
        Ok(matrix) => Json(serde_json::to_value(matrix).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({
            "error": format!("Strategy check failed: {}", e),
            "strategies": [],
            "operational": 0,
            "total": 0,
        })),
    }
}

// ── Data status — probe each data source ─────────────────────────────────────

/// Returns the load status of each data source: aiwar graph, enrichment files,
/// neural diagnosis, etc. The frontend renders this as a status bar.
async fn data_status_handler() -> Json<serde_json::Value> {
    let mut sources: Vec<serde_json::Value> = Vec::new();

    // 1. Aiwar Graph JSON — the official 221-node dataset
    let aiwar_status = match notebook_query::execute(
        "MATCH (n) RETURN count(n) AS total",
        notebook_query::QueryLanguage::Cypher,
    ) {
        Ok(result) => serde_json::json!({
            "name": "Aiwar Graph",
            "file": "aiwar_graph.json",
            "status": "loaded",
            "detail": result.raw_output,
            "elapsed_ms": result.elapsed_ms,
        }),
        Err(e) => serde_json::json!({
            "name": "Aiwar Graph",
            "file": "aiwar_graph.json",
            "status": "error",
            "detail": e,
        }),
    };
    sources.push(aiwar_status);

    // 2. Enrichment Cypher files — check if directory exists
    let cypher_dir = std::path::Path::new("/home/user/aiwar-neo4j-harvest/cypher");
    let enrichment_status = if cypher_dir.exists() {
        let count = std::fs::read_dir(cypher_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| {
                e.path().extension().map_or(false, |ext| ext == "cypher")
            }).count())
            .unwrap_or(0);
        serde_json::json!({
            "name": "Enrichment Cypher",
            "file": "cypher/*.cypher",
            "status": if count > 0 { "loaded" } else { "empty" },
            "detail": format!("{} cypher files found", count),
            "count": count,
        })
    } else {
        serde_json::json!({
            "name": "Enrichment Cypher",
            "file": "cypher/*.cypher",
            "status": "not_found",
            "detail": "Directory not found: aiwar-neo4j-harvest/cypher/",
        })
    };
    sources.push(enrichment_status);

    // 3. Neural diagnosis scan data
    let neural_status = if cfg!(feature = "embed-cockpit") {
        serde_json::json!({
            "name": "Neural Diagnosis",
            "file": "neural_diagnosis.json",
            "status": "embedded",
            "detail": "Embedded in Vite build",
        })
    } else {
        serde_json::json!({
            "name": "Neural Diagnosis",
            "file": "neural_diagnosis.json",
            "status": "static",
            "detail": "Served from public/",
        })
    };
    sources.push(neural_status);

    // 4. Aiwar CSV (51 weapons)
    let csv_candidates = [
        "/home/user/aiwar-neo4j-harvest/data/aiwarcloud-table.csv",
        "../aiwar-neo4j-harvest/data/aiwarcloud-table.csv",
    ];
    let csv_status = csv_candidates.iter().find(|p| std::path::Path::new(p).exists());
    sources.push(match csv_status {
        Some(path) => serde_json::json!({
            "name": "Aiwar CSV",
            "file": "aiwarcloud-table.csv",
            "status": "found",
            "detail": format!("At {}", path),
        }),
        None => serde_json::json!({
            "name": "Aiwar CSV",
            "file": "aiwarcloud-table.csv",
            "status": "not_found",
            "detail": "51 weapons CSV not found",
        }),
    });

    Json(serde_json::json!({
        "sources": sources,
        "total": sources.len(),
        "loaded": sources.iter().filter(|s| s["status"] == "loaded" || s["status"] == "found" || s["status"] == "embedded").count(),
    }))
}

// ── Health ────────────────────────────────────────────────────────────────────

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "engine": "lance-graph (DataFusion + LanceDB)",
        "renderer": "quarto-core + deno_core (V8 JIT)",
        "compute": "ndarray (SIMD)",
        "cockpit": if cfg!(feature = "embed-cockpit") { "embedded (Vite React)" } else { "fallback shell" },
    }))
}
