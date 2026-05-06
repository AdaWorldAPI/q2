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

mod openai;
mod graph_engine;
mod scene_player;
mod shader_stream;
mod style_state;
mod dto_bridge;
mod codebook;
mod mock_driver;

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
    /// Shared scene state for /v1/shader/stream + /v1/shader/status.
    scene_state: shader_stream::SharedSceneState,
}

// Shader handlers extract `State<Arc<AppState>>` and read `state.scene_state`
// directly — avoids the orphan rule that forbids `impl FromRef<Arc<AppState>>
// for Arc<RwLock<SceneState>>` (Arc is a foreign type).

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
    let scene_state = shader_stream::new_scene_state();
    let state = Arc::new(AppState { tx, scene_state });

    // OpenAI-compatible model API state
    let openai_state = Arc::new(tokio::sync::Mutex::new(openai::OpenAiState::new()));

    // Build the main router with Arc<AppState> as the state type.
    // Shader-stream routes use per-route `.with_state(scene_state)` to
    // bind their `State<SharedSceneState>` extractor directly, finalizing
    // them to `MethodRouter<Arc<AppState>>` so they fit into the parent
    // router. After `.with_state(state)` the router becomes `Router<()>`,
    // which lets us merge in the openai sub-router (also `Router<()>`)
    // without a state-type collision.
    let scene_state_for_routes = state.scene_state.clone();
    let main_routes: Router<Arc<AppState>> = Router::new()
        // Shader stream — DTO pipeline SSE (Φ StreamDto → Ψ ResonanceDto → B BusDto → Γ ThoughtStruct)
        .route("/v1/shader/stream", get(shader_stream::shader_stream_handler).with_state(scene_state_for_routes.clone()))
        .route("/v1/shader/status", get(shader_stream::shader_status_handler).with_state(scene_state_for_routes))
        // Shader style selection — POST { "style": "Focused" } sets the
        // process-global StyleSelector that shader_stream reads when
        // building each ShaderDispatch (overrides default Auto).
        .route("/v1/shader/style", post(style_handler))
        // MCP endpoints — all queries route through lance-graph
        .route("/mcp/sse", get(sse_handler))
        .route("/mcp/message", post(mcp_message_handler))
        // Data status — what's loaded, what failed
        .route("/api/data/status", get(data_status_handler))
        // Live strategy diagnostics — runs all 16 strategies against real queries
        .route("/api/debug/strategies", get(strategy_check_handler))
        // Live OSINT pipeline audit — AriGraph health, NARS stats, xAI status
        .route("/api/debug/osint", get(osint_audit_handler))
        // Brain MRI — plasticity, activation, NARS reasoning chains
        .route("/mri", get(mri_page_handler))
        .route("/api/mri/scan", get(mri_scan_handler))
        .route("/api/mri/scan/:mode", get(mri_scan_mode_handler))
        // Meta-orchestrator — NARS RL style tuning + transparent fallback
        .route("/api/orchestrator/status", get(orchestrator_status_handler))
        .route("/api/orchestrator/step", post(orchestrator_step_handler))
        // Political analyst — NARS causality chains through analytical buckets
        .route("/api/analyst/buckets", get(analyst_buckets_handler))
        .route("/api/analyst/analyze/:bucket", get(analyst_analyze_handler))
        .route("/api/analyst/full", get(analyst_full_handler))
        // Live graph engine — neo4j-emulating renderer with AGI thinking
        .route("/api/graph/snapshot", get(graph_engine::graph_snapshot_handler))
        .route("/api/graph/infer", post(graph_engine::nars_infer_handler))
        .route("/api/graph/health", get(graph_engine::graph_health_handler))
        // Health
        .route("/health", get(health_handler));

    let app: Router = main_routes
        .with_state(state)
        // OpenAI-compatible endpoints: /v1/models, /v1/completions, /v1/chat/completions, etc.
        .merge(openai::openai_router(openai_state))
        // Static files + SPA fallback (serves the Vite React build)
        .fallback(get(static_handler))
        .layer(CorsLayer::permissive());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2718);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("q2-cockpit listening on http://{addr}");
    tracing::info!("  /       → Palantir cockpit (Vite build, 221 aiwar nodes)");
    tracing::info!("  /demo   → infrastructure demo (24 seed nodes)");
    tracing::info!("  /debug  → neural debugger (18,763 functions)");
    tracing::info!("  /api/debug/osint → live OSINT pipeline audit (AriGraph + NARS + xAI)");
    tracing::info!("  /mri             → AGI Brain MRI (pre-rendered, 500ms refresh, LazyLock double-buffer)");
    tracing::info!("  /mcp/*  → MCP endpoints (lance-graph)");
    tracing::info!("  /v1/*   → OpenAI-compatible API (gpt2, openchat_3.5, stable-diffusion)");

    // Hydrate live graph from aiwar data (if available).
    if let Ok(path) = std::env::var("AIWAR_DATA_PATH") {
        match graph_engine::hydrate_from_aiwar_json(&path).await {
            Ok(()) => tracing::info!("  /api/graph/*     → live graph engine (neo4j-emulating, NARS-enabled)"),
            Err(e) => tracing::warn!("  /api/graph/*     → fallback mode (hydration failed: {e})"),
        }
    } else {
        tracing::info!("  /api/graph/*     → fallback mode (AIWAR_DATA_PATH not set)");
    }

    // Start background MRI pre-render (LazyLock double-buffer, 500ms refresh).
    spawn_mri_prerender();

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

// ── Shader style selection ───────────────────────────────────────────────────

/// POST /v1/shader/style — set the process-global thinking style.
///
/// Request body: `{ "style": "Focused" }` (or any of the 36 canonical
/// `ThinkingStyle` names; "Auto" returns to driver-routed selection).
/// On success the next `ShaderDispatch` built by the SSE loop will use
/// the new selector — see `style_state::current_dispatch`.
async fn style_handler(
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let style_name = body
        .get("style")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "missing 'style' field".to_string()))?;
    let parsed = style_state::parse_style_name(style_name).ok_or((
        StatusCode::BAD_REQUEST,
        format!("unknown style: {style_name}"),
    ))?;
    style_state::set_style(parsed);
    let canonical = style_state::current_style_name();
    Ok(Json(serde_json::json!({
        "style": style_name,
        "canonical": canonical,
        "applied": true,
    })))
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

// ── Live OSINT Pipeline Audit ────────────────────────────────────────────────

/// Real-time health check of the AriGraph OSINT pipeline.
/// Reports: pipeline stage call counts, graph truth distribution,
/// NARS inference stats, episodic memory saturation, xAI API status.
async fn osint_audit_handler() -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(|| {
        // In production these come from the live graph state.
        // For now, report pipeline registry stats + env status.
        notebook_query::osint_audit::run_osint_audit(
            0, // graph_triplet_count — wire to live graph
            0, // graph_active_count
            0, // graph_entity_count
            0, // graph_spatial_edges
            0, // graph_contradictions
            0, // episodic_count
            100, // episodic_capacity
        )
    })
    .await;

    match result {
        Ok(audit) => Json(serde_json::to_value(audit).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({
            "error": format!("OSINT audit failed: {}", e),
        })),
    }
}

// ── Brain MRI — pre-rendered double-buffer (Rust 1.94 LazyLock) ─────────────
//
// Pattern: compute frame N+1 in background while serving frame N.
// LazyLock initializes the cache on first access. A background tokio task
// refreshes every 500ms. The HTTP handler reads the cached pre-rendered
// HTML + JSON — zero compute on the request path.
//
// This is NOT a REST API. /mri serves a LangStudio-style web application.
// The page arrives fully rendered (server-side) and then live-updates via
// the pre-rendered JSON cache. No client-side fetch() on first paint.

use std::sync::LazyLock;
use tokio::sync::RwLock as TokioRwLock;

/// Pre-rendered MRI frame: HTML page + JSON data, computed in background.
struct MriFrame {
    /// Full HTML page with the latest scan data inlined as JSON.
    html: String,
    /// Raw JSON for the /api/mri/scan endpoint.
    json: serde_json::Value,
    /// When this frame was rendered (millis since epoch).
    rendered_at_ms: u64,
}

impl Default for MriFrame {
    fn default() -> Self {
        let empty_json = serde_json::json!({
            "scan_mode": "full",
            "regions": [],
            "plasticity_map": [],
            "reasoning_chains": [],
            "findings": ["Waiting for first scan..."],
            "health_score": 0.0,
            "dominant_mode": "idle",
            "total_entities": 0,
            "total_triplets": 0,
            "pipeline_activation": [],
            "thinking_styles": [],
            "timestamp_ms": 0,
        });
        let html = render_mri_html(&empty_json);
        Self { html, json: empty_json, rendered_at_ms: 0 }
    }
}

/// Global double-buffer: LazyLock ensures one-time init, RwLock allows
/// concurrent readers (HTTP handlers) with exclusive writer (background task).
static MRI_CACHE: LazyLock<TokioRwLock<MriFrame>> =
    LazyLock::new(|| TokioRwLock::new(MriFrame::default()));

/// Static CSS — computed once at startup via LazyLock. Never changes.
static MRI_CSS: LazyLock<String> = LazyLock::new(|| {
    r#"
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: 'SF Mono', 'Fira Code', monospace; background: #0a0a0a; color: #00ff88; padding: 1.5em; }
    h1 { color: #00ccff; margin-bottom: 0.5em; font-size: 1.4em; }
    h2 { color: #888; margin: 1em 0 0.3em; font-size: 1em; border-bottom: 1px solid #222; padding-bottom: 0.2em; }
    .header { display: flex; align-items: center; gap: 1em; margin-bottom: 1em; }
    .header select, .header button { background: #1a1a2e; color: #00ff88; border: 1px solid #333; padding: 0.3em 0.8em; border-radius: 4px; cursor: pointer; }
    .status { color: #ffaa00; font-size: 0.9em; }
    .region { border: 1px solid #333; padding: 0.8em; margin: 0.3em 0; border-radius: 6px; transition: border-color 0.3s; }
    .hot { border-color: #ff4444; background: rgba(255,68,68,0.08); }
    .frozen { border-color: #4488ff; background: rgba(68,136,255,0.08); }
    .active { border-color: #44ff44; background: rgba(68,255,68,0.08); }
    .conflicted { border-color: #ffaa00; background: rgba(255,170,0,0.08); }
    .bar { height: 8px; background: linear-gradient(90deg, #00ff88, #00ccff); border-radius: 3px; transition: width 0.5s ease-out; margin-top: 0.3em; }
    .sub { margin-left: 1.5em; font-size: 0.85em; color: #999; }
    .entity { display: inline-block; padding: 0.2em 0.6em; margin: 0.15em; border-radius: 4px; font-size: 0.8em; }
    .chain { margin: 0.3em 0; padding: 0.5em; border-left: 3px solid #00ccff; }
    .chain-step { margin-left: 1.5em; color: #aaa; font-size: 0.85em; }
    .finding { color: #ffaa00; padding: 0.2em 0; }
    pre { overflow-x: auto; font-size: 0.75em; background: #111; padding: 0.8em; border-radius: 4px; max-height: 300px; }
    .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1em; }
    @media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }
    "#.to_string()
});

/// Render the MRI HTML page with scan data inlined as JSON.
/// The page loads instantly (no fetch on first paint) and then
/// auto-refreshes by fetching the pre-rendered JSON every 500ms.
fn render_mri_html(data: &serde_json::Value) -> String {
    let json_str = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"<!DOCTYPE html>
<html><head><title>AGI Brain MRI</title><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<style>{css}</style></head>
<body>
<div class="header">
  <h1>&#129504; AGI Brain MRI</h1>
  <select id="mode" onchange="scan()">
    <option value="structural">Structural</option>
    <option value="functional">Functional</option>
    <option value="full" selected>Full (DTI)</option>
  </select>
  <span class="status" id="status">Pre-rendered</span>
</div>
<div class="grid">
  <div><h2>Brain Regions</h2><div id="regions"></div></div>
  <div><h2>Plasticity Map</h2><div id="plasticity"></div></div>
</div>
<h2>NARS Reasoning Chains</h2><div id="chains"></div>
<h2>Findings</h2><div id="findings"></div>
<details><summary>Raw JSON</summary><pre id="raw"></pre></details>
<script>
// First frame: inlined by the server (zero network latency on first paint).
let cachedData = {json};
render(cachedData);

function render(data) {{
  document.getElementById('status').textContent =
    'Health: ' + (data.health_score * 100).toFixed(0) + '% | ' +
    data.total_entities + ' entities | ' + data.total_triplets + ' triplets | ' +
    (data.dominant_mode || 'idle');

  let html = '';
  for (const reg of (data.regions || [])) {{
    const cls = reg.plasticity > 0.5 ? 'hot' : reg.activation < 0.1 ? 'frozen' : 'active';
    html += '<div class="region ' + cls + '"><b>' + reg.name + '</b>';
    html += ' &nbsp; activation=' + (reg.activation * 100).toFixed(0) + '%';
    html += ' &nbsp; plasticity=' + (reg.plasticity * 100).toFixed(0) + '%';
    html += ' &nbsp; temp=' + (reg.temperature * 100).toFixed(0);
    html += '<div class="bar" style="width:' + Math.max(2, reg.activation * 100) + '%"></div>';
    for (const sub of (reg.sub_regions || [])) {{
      html += '<div class="sub">' + sub.name + ': ' + sub.calls + ' calls, ' + sub.avg_latency_us + 'us avg, ' + sub.status + '</div>';
    }}
    html += '</div>';
  }}
  document.getElementById('regions').innerHTML = html || '<em>No regions</em>';

  let phtml = '';
  for (const p of (data.plasticity_map || [])) {{
    const cls = p.state === 'conflicted' ? 'conflicted' : p.state === 'hot' ? 'hot' : p.state === 'frozen' ? 'frozen' : 'active';
    phtml += '<div class="entity ' + cls + '">' + p.entity + ' <small>' + p.state;
    phtml += ' c=' + p.avg_confidence.toFixed(2) + ' rev=' + p.revisions + '</small></div>';
  }}
  document.getElementById('plasticity').innerHTML = phtml || '<em>No entities tracked</em>';

  let chtml = '';
  for (const c of (data.reasoning_chains || [])) {{
    chtml += '<div class="chain">Chain #' + c.id + ' (depth ' + c.depth + ', conf=' + c.final_truth.confidence.toFixed(3) + ')';
    for (const s of c.steps) {{
      chtml += '<div class="chain-step">' + s.rule + ': ' + s.conclusion + '</div>';
    }}
    chtml += '</div>';
  }}
  document.getElementById('chains').innerHTML = chtml || '<em>No active reasoning</em>';

  let fhtml = '';
  for (const f of (data.findings || [])) {{
    fhtml += '<div class="finding">&#x2022; ' + f + '</div>';
  }}
  document.getElementById('findings').innerHTML = fhtml;
  document.getElementById('raw').textContent = JSON.stringify(data, null, 2);
}}

// Live refresh: fetch pre-rendered JSON every 500ms.
// The server has already computed it — this is just a cache read.
async function scan() {{
  const mode = document.getElementById('mode').value;
  try {{
    const r = await fetch('/api/mri/scan/' + mode);
    const data = await r.json();
    render(data);
  }} catch(e) {{
    document.getElementById('status').textContent = 'Scan failed: ' + e;
  }}
}}
setInterval(scan, 500);
</script></body></html>"#,
        css = *MRI_CSS,
        json = json_str,
    )
}

/// Spawn the background MRI pre-render task.
/// Call this once during server startup. Refreshes the cache every 500ms.
pub fn spawn_mri_prerender() {
    tokio::spawn(async {
        loop {
            // Compute the scan in a blocking thread (CPU-bound).
            let result = tokio::task::spawn_blocking(|| {
                let edges = Vec::new();
                let entity_stats = std::collections::HashMap::new();
                let thinking_activations = Vec::new();
                let mri = notebook_query::mri::run_brain_mri(
                    &edges, &entity_stats, &thinking_activations,
                    notebook_query::mri::ScanMode::Full,
                );
                serde_json::to_value(mri).unwrap_or_default()
            })
            .await;

            if let Ok(json) = result {
                let html = render_mri_html(&json);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let mut cache = MRI_CACHE.write().await;
                cache.html = html;
                cache.json = json;
                cache.rendered_at_ms = now;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

/// Serve the pre-rendered MRI web application. Zero compute on request path.
async fn mri_page_handler() -> axum::response::Html<String> {
    let cache = MRI_CACHE.read().await;
    axum::response::Html(cache.html.clone())
}

/// JSON API: serve pre-rendered scan data. Zero compute on request path.
async fn mri_scan_handler() -> Json<serde_json::Value> {
    let cache = MRI_CACHE.read().await;
    Json(cache.json.clone())
}

/// JSON API: scan with mode (structural/functional/full).
/// For non-full modes, compute on demand (they're fast enough).
async fn mri_scan_mode_handler(
    axum::extract::Path(mode): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    if mode == "full" {
        // Serve from pre-rendered cache.
        let cache = MRI_CACHE.read().await;
        return Json(cache.json.clone());
    }
    // Structural and functional are lighter — compute on demand.
    let scan_mode = match mode.as_str() {
        "structural" => notebook_query::mri::ScanMode::Structural,
        "functional" => notebook_query::mri::ScanMode::Functional,
        _ => notebook_query::mri::ScanMode::Full,
    };
    let result = tokio::task::spawn_blocking(move || {
        let edges = Vec::new();
        let entity_stats = std::collections::HashMap::new();
        let thinking_activations = Vec::new();
        notebook_query::mri::run_brain_mri(&edges, &entity_stats, &thinking_activations, scan_mode)
    })
    .await;
    match result {
        Ok(mri) => Json(serde_json::to_value(mri).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({
            "error": format!("Brain MRI failed: {}", e),
            "health_score": 0.0,
        })),
    }
}

// ── Meta-Orchestrator — Thinking About Thinking ─────────────────────────────

/// Global orchestrator instance. Persists across requests.
static ORCHESTRATOR: std::sync::OnceLock<std::sync::Mutex<notebook_query::orchestrator::MetaOrchestrator>> =
    std::sync::OnceLock::new();

fn get_orchestrator() -> &'static std::sync::Mutex<notebook_query::orchestrator::MetaOrchestrator> {
    ORCHESTRATOR.get_or_init(|| {
        std::sync::Mutex::new(notebook_query::orchestrator::MetaOrchestrator::new())
    })
}

/// GET /api/orchestrator/status — current mode, topology, efficiency, mode switches.
async fn orchestrator_status_handler() -> Json<serde_json::Value> {
    let orch = get_orchestrator().lock().unwrap();
    Json(serde_json::to_value(orch.snapshot()).unwrap_or_default())
}

/// POST /api/orchestrator/step — execute one orchestration step.
///
/// Request body (optional): `{ "quality": 0.8 }` to record the outcome of the previous step.
/// Response: the next style to execute + why it was chosen.
async fn orchestrator_step_handler(
    body: Option<Json<serde_json::Value>>,
) -> Json<serde_json::Value> {
    let mut orch = get_orchestrator().lock().unwrap();

    // If quality is provided, record the outcome of the previous step.
    if let Some(Json(body)) = body {
        if let Some(quality) = body.get("quality").and_then(|v| v.as_f64()) {
            if let Some(style_name) = body.get("style").and_then(|v| v.as_str()) {
                let style = match style_name {
                    "plan" => notebook_query::orchestrator::AgentStyle::Plan,
                    "act" => notebook_query::orchestrator::AgentStyle::Act,
                    "explore" => notebook_query::orchestrator::AgentStyle::Explore,
                    "reflex" => notebook_query::orchestrator::AgentStyle::Reflex,
                    _ => notebook_query::orchestrator::AgentStyle::Plan,
                };
                orch.record_outcome(style, quality as f32);
            }
        }
    }

    let result = orch.select_next();
    Json(serde_json::to_value(result).unwrap_or_default())
}

// ── Political Analyst Savant ──────────────────────────────────────────────────

/// List available analysis buckets.
async fn analyst_buckets_handler() -> Json<serde_json::Value> {
    // NOTE: `seed_queries()` was removed from AnalysisBucket; the per-bucket
    // query count now lives in the AnalysisResult returned from `analyze()`.
    // We surface 0 here rather than running every analysis just to count.
    let buckets: Vec<serde_json::Value> = notebook_query::analyst::AnalysisBucket::all()
        .iter()
        .map(|b| serde_json::json!({
            "id": serde_json::to_value(b).unwrap_or_default(),
            "label": b.label(),
            "description": b.description(),
            "query_count": 0,
        }))
        .collect();
    Json(serde_json::json!({ "buckets": buckets }))
}

/// Run analysis for a specific bucket.
async fn analyst_analyze_handler(
    axum::extract::Path(bucket_name): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let bucket = match bucket_name.as_str() {
        "economic_review" => notebook_query::analyst::AnalysisBucket::EconomicReview,
        "civil_engineering" => notebook_query::analyst::AnalysisBucket::CivilEngineering,
        "political_dynamics" => notebook_query::analyst::AnalysisBucket::PoliticalDynamics,
        "ai_development_impact" => notebook_query::analyst::AnalysisBucket::AiDevelopmentImpact,
        "kill_chain_analysis" => notebook_query::analyst::AnalysisBucket::KillChainAnalysis,
        "surveillance_ecosystem" => notebook_query::analyst::AnalysisBucket::SurveillanceEcosystem,
        _ => {
            return Json(serde_json::json!({
                "error": format!("Unknown bucket: {}. Available: economic_review, civil_engineering, political_dynamics, ai_development_impact, kill_chain_analysis, surveillance_ecosystem", bucket_name),
            }));
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        notebook_query::analyst::analyze(bucket)
    }).await;

    match result {
        Ok(analysis) => Json(serde_json::to_value(analysis).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("Analysis failed: {}", e) })),
    }
}

/// Run all 6 analysis buckets.
async fn analyst_full_handler() -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(|| {
        notebook_query::analyst::full_analysis()
    }).await;

    match result {
        Ok(analyses) => Json(serde_json::to_value(analyses).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("Full analysis failed: {}", e) })),
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
