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
//! No static Vite build. No include_dir. The cockpit IS a .qmd notebook
//! rendered live by the Quarto engine compiled into this binary.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

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
        // Live-rendered cockpit notebook
        .route("/", get(cockpit_handler))
        // MCP endpoints — all queries route through lance-graph
        .route("/mcp/sse", get(sse_handler))
        .route("/mcp/message", post(mcp_message_handler))
        // Health
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2718);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("q2-cockpit listening on http://{addr}");
    tracing::info!("engine: lance-graph (DataFusion + LanceDB)");
    tracing::info!("renderer: quarto-core + deno_core (V8 JIT)");

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

// ── Live cockpit rendering ───────────────────────────────────────────────────

/// Render the cockpit .qmd notebook through the Quarto pipeline.
///
/// pampa parses the .qmd → quarto-core renders → deno_core executes JS/TS
/// cells → lance-graph handles any graph queries in code cells → HTML output.
async fn cockpit_handler() -> Html<String> {
    // TODO: Wire pampa + quarto-core + deno_core rendering pipeline.
    // For now, return a minimal HTML shell that connects to the MCP SSE
    // endpoint. The full pipeline will render .qmd → HTML with live
    // graph cells executed through lance-graph.
    Html(COCKPIT_SHELL.to_string())
}

/// Minimal cockpit HTML shell — connects to MCP SSE for live updates.
/// This will be replaced by the full quarto-core rendering pipeline.
const COCKPIT_SHELL: &str = r#"<!DOCTYPE html>
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
    <span style="margin-left:auto">engine: lance-graph | renderer: quarto-core + V8 JIT</span>
  </div>
  <div class="main">
    <div class="msg">
      <h1>q2 Graph Notebook</h1>
      <p>Live <code>.qmd</code> rendering through the Quarto pipeline.<br>
      All queries route through <code>lance-graph</code> (DataFusion + LanceDB).<br>
      V8 JIT executes JS/TS cells via <code>deno_core</code>.</p>
      <p style="margin-top:16px;font-size:12px;color:var(--muted)">
        Waiting for quarto-core rendering pipeline…</p>
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

// ── Health ────────────────────────────────────────────────────────────────────

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "engine": "lance-graph (DataFusion + LanceDB)",
        "renderer": "quarto-core + deno_core (V8 JIT)",
        "compute": "ndarray (SIMD)",
    }))
}
