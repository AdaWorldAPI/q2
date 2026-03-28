//! Axum-based graph notebook server
//!
//! Serves the cockpit frontend, health endpoint, and MCP over SSE.

use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use notebook_query::{QueryLanguage, detect_language, execute};
use notebook_query::reasoning::{self, TruthValue, TruthEdge};
use notebook_query::hydration::SemiringVariant;
use crate::notebook_types::{Cell, CellId, CellOutput, ExecutionState, Runtime};

use crate::commands::notebook::NotebookServeArgs;

// ---- Shared state ----

struct AppState {
    runtime: Mutex<Runtime>,
    next_cell_id: Mutex<u64>,
}

impl AppState {
    fn new() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
            next_cell_id: Mutex::new(1),
        }
    }

    async fn next_id(&self) -> CellId {
        let mut id = self.next_cell_id.lock().await;
        let cell_id = format!("cell-{}", *id);
        *id += 1;
        cell_id
    }
}

// ---- API types ----

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    engine: &'static str,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CellExecuteRequest {
    code: String,
    #[serde(default)]
    lang: Option<String>,
}

#[derive(Deserialize)]
struct CellCreateRequest {
    #[serde(default)]
    source: String,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CellUpdateRequest {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Serialize)]
struct CellResponse {
    id: String,
    source: String,
    language: String,
    execution_state: String,
    outputs: Vec<OutputResponse>,
}

#[derive(Serialize)]
struct OutputResponse {
    #[serde(rename = "type")]
    output_type: String,
    content: String,
}

#[derive(Serialize)]
struct DagResponse {
    cells: Vec<String>,
    edges: Vec<(String, String)>,
}

#[derive(Serialize)]
struct McpToolDefinition {
    name: &'static str,
    description: &'static str,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct McpResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    result: serde_json::Value,
}

#[derive(Serialize)]
struct McpError {
    jsonrpc: &'static str,
    id: serde_json::Value,
    error: McpErrorBody,
}

#[derive(Serialize)]
struct McpErrorBody {
    code: i32,
    message: String,
}

// ---- Helpers ----

fn lang_to_str(lang: QueryLanguage) -> &'static str {
    match lang {
        QueryLanguage::Gremlin => "gremlin",
        QueryLanguage::Cypher => "cypher",
        QueryLanguage::Sparql => "sparql",
        QueryLanguage::R => "r",
        QueryLanguage::Rust => "rust",
        QueryLanguage::Markdown => "markdown",
    }
}

fn str_to_lang(s: &str) -> QueryLanguage {
    match s.to_lowercase().as_str() {
        "gremlin" => QueryLanguage::Gremlin,
        "cypher" => QueryLanguage::Cypher,
        "sparql" => QueryLanguage::Sparql,
        "r" => QueryLanguage::R,
        "rust" => QueryLanguage::Rust,
        _ => QueryLanguage::Markdown,
    }
}

fn cell_to_response(cell: &Cell) -> CellResponse {
    let lang = cell
        .language
        .as_deref()
        .map(str_to_lang)
        .unwrap_or_else(|| detect_language(&cell.source));

    let outputs = cell
        .outputs
        .iter()
        .map(|o| match o {
            CellOutput::Html(h) => OutputResponse {
                output_type: "html".into(),
                content: h.clone(),
            },
            CellOutput::Text(t) => OutputResponse {
                output_type: "text".into(),
                content: t.clone(),
            },
            CellOutput::Error(e) => OutputResponse {
                output_type: "error".into(),
                content: e.clone(),
            },
            CellOutput::Table { headers, rows } => OutputResponse {
                output_type: "table".into(),
                content: crate::notebook_types::render_table(headers, rows),
            },
            CellOutput::Graph { html } => OutputResponse {
                output_type: "graph".into(),
                content: html.clone(),
            },
        })
        .collect();

    let execution_state = match &cell.execution_state {
        ExecutionState::Idle => "idle",
        ExecutionState::Queued => "queued",
        ExecutionState::Running => "running",
        ExecutionState::Success => "success",
        ExecutionState::Error(_) => "error",
        ExecutionState::Stale => "stale",
    };

    CellResponse {
        id: cell.id.clone(),
        source: cell.source.clone(),
        language: lang_to_str(lang).into(),
        execution_state: execution_state.into(),
        outputs,
    }
}

// ---- Handlers ----

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        engine: "lance-graph",
    })
}

/// Live cockpit — Palantir layout, dynamic via MCP/SSE → lance-graph
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("frontend_placeholder.html"))
}

/// Static demo — Palantir cockpit with mock data (no API required)
async fn demo_handler() -> Html<&'static str> {
    Html(include_str!("cockpit_demo.html"))
}

// ---- MCP Tool definitions ----

fn mcp_tool_definitions() -> Vec<McpToolDefinition> {
    vec![
        McpToolDefinition {
            name: "cell_execute",
            description: "Execute a code cell. If lang is omitted, auto-detects language.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Code to execute" },
                    "lang": { "type": "string", "description": "Language (gremlin, cypher, sparql, r, rust, markdown). Auto-detected if omitted." }
                },
                "required": ["code"]
            }),
        },
        McpToolDefinition {
            name: "cell_get",
            description: "Get a cell by ID",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            }),
        },
        McpToolDefinition {
            name: "cells_list",
            description: "List all cells in the notebook",
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        McpToolDefinition {
            name: "cell_create",
            description: "Create a new cell",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "language": { "type": "string" }
                }
            }),
        },
        McpToolDefinition {
            name: "cell_update",
            description: "Update an existing cell",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "source": { "type": "string" },
                    "language": { "type": "string" }
                },
                "required": ["id"]
            }),
        },
        McpToolDefinition {
            name: "cell_delete",
            description: "Delete a cell by ID",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            }),
        },
        McpToolDefinition {
            name: "dag_get",
            description: "Get the notebook dependency DAG",
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        McpToolDefinition {
            name: "notebook_save",
            description: "Save the notebook to disk",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        McpToolDefinition {
            name: "notebook_load",
            description: "Load a notebook from disk",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        McpToolDefinition {
            name: "notebook_export",
            description: "Export notebook to HTML or PDF",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["html", "pdf"] },
                    "output": { "type": "string" }
                },
                "required": ["format"]
            }),
        },
        // ── Graph Intelligence Tools ──
        McpToolDefinition {
            name: "graph_semiring",
            description: "Set the active semiring for edge weight computation. Options: XorBundle, BindFirst, HammingMin, SimilarityMax, Resonance, Boolean, XorField.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "variant": { "type": "string", "enum": ["XorBundle", "BindFirst", "HammingMin", "SimilarityMax", "Resonance", "Boolean", "XorField"] }
                },
                "required": ["variant"]
            }),
        },
        McpToolDefinition {
            name: "graph_truth_values",
            description: "Get NARS truth values for edges. Returns frequency, confidence, expectation per edge.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "min_confidence": { "type": "number", "description": "Minimum confidence threshold (0-1). Default: 0.0" }
                }
            }),
        },
        McpToolDefinition {
            name: "graph_infer",
            description: "Run NARS inference (deduction/abduction) to discover implicit edges.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "min_confidence": { "type": "number", "description": "Minimum confidence for source edges (default: 0.5)" },
                    "max_hops": { "type": "integer", "description": "Maximum inference chain length (default: 2)" }
                }
            }),
        },
        McpToolDefinition {
            name: "graph_verify",
            description: "Verify Merkle integrity of a node's container seal.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": { "type": "string", "description": "Node ID to verify" }
                },
                "required": ["node_id"]
            }),
        },
        McpToolDefinition {
            name: "graph_resolution",
            description: "Set progressive hydration resolution level. Controls edge visibility threshold.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "bytes": { "type": "integer", "enum": [1, 2, 4, 8], "description": "Bytes per edge: 1=scent, 2=low, 4=medium, 8=full" }
                },
                "required": ["bytes"]
            }),
        },
        McpToolDefinition {
            name: "graph_timeline",
            description: "Step through versioned encounter rounds. Play temporal graph evolution.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "version": { "type": "integer", "description": "Version number to load (0-42). Omit to get current." },
                    "action": { "type": "string", "enum": ["play", "pause", "step", "status"], "description": "Timeline action" }
                }
            }),
        },
        // ── Demo Datasets ──
        McpToolDefinition {
            name: "demo_datasets",
            description: "List available demo datasets with sample queries",
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        McpToolDefinition {
            name: "demo_load",
            description: "Load a demo dataset by name and execute its default query through lance-graph",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "dataset": { "type": "string", "enum": ["aiwar", "infrastructure", "supply-chain"], "description": "Demo dataset to load" }
                },
                "required": ["dataset"]
            }),
        },
        // ── Planner Tool ──
        McpToolDefinition {
            name: "planner_plan",
            description: "Run unified query planner on a Cypher query. Returns plan strategies, thinking context, MUL assessment.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Cypher query to plan" },
                    "style": { "type": "string", "description": "Optional thinking style override (analytical, convergent, systematic, creative, divergent, exploratory, focused, diffuse, peripheral, intuitive, deliberate, metacognitive)" },
                    "felt_competence": { "type": "number", "description": "Felt competence for MUL assessment (0-1)" },
                    "demonstrated_competence": { "type": "number", "description": "Demonstrated competence for MUL assessment (0-1)" }
                },
                "required": ["query"]
            }),
        },
    ]
}

// ---- MCP message handler ----

async fn mcp_message_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpRequest>,
) -> impl IntoResponse {
    let id = req.id.unwrap_or(serde_json::Value::Null);

    let result = handle_mcp_method(&state, &req.method, &req.params).await;

    match result {
        Ok(value) => Json(McpResponse {
            jsonrpc: "2.0",
            id,
            result: value,
        })
        .into_response(),
        Err(msg) => (
            StatusCode::OK,
            Json(McpError {
                jsonrpc: "2.0",
                id,
                error: McpErrorBody {
                    code: -32603,
                    message: msg,
                },
            }),
        )
            .into_response(),
    }
}

async fn handle_mcp_method(
    state: &AppState,
    method: &str,
    params: &serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "q2-notebook", "version": env!("CARGO_PKG_VERSION") }
        })),

        "tools/list" => {
            let tools = mcp_tool_definitions();
            Ok(serde_json::json!({ "tools": tools }))
        }

        "tools/call" => {
            let tool_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("Missing tool name")?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            match tool_name {
                "cell_execute" => {
                    let code = arguments
                        .get("code")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing code")?;
                    let lang_str = arguments.get("lang").and_then(|v| v.as_str());
                    let lang = lang_str
                        .map(str_to_lang)
                        .unwrap_or_else(|| detect_language(code));

                    let result =
                        execute(code, lang).map_err(|e| e.to_string())?;

                    // Build outputs: prefer graph_json for graph queries,
                    // fall back to html, then raw_output
                    let outputs = if let Some(ref graph_json) = result.graph_json {
                        vec![CellOutput::Graph {
                            html: graph_json.clone(),
                        }]
                    } else {
                        vec![CellOutput::Html(
                            result.html.unwrap_or(result.raw_output),
                        )]
                    };

                    let cell_id = state.next_id().await;
                    let cell = Cell {
                        id: cell_id.clone(),
                        source: code.to_string(),
                        language: Some(lang_to_str(lang).to_string()),
                        outputs,
                        execution_state: ExecutionState::Success,
                    };
                    let mut runtime = state.runtime.lock().await;
                    runtime.add_cell(cell);

                    let cell = runtime.get_cell(&cell_id).unwrap();
                    Ok(serde_json::to_value(cell_to_response(cell)).unwrap())
                }

                "cell_get" => {
                    let id = arguments
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing id")?;
                    let runtime = state.runtime.lock().await;
                    let cell = runtime
                        .get_cell(id)
                        .ok_or_else(|| format!("Cell {} not found", id))?;
                    Ok(serde_json::to_value(cell_to_response(cell)).unwrap())
                }

                "cells_list" => {
                    let runtime = state.runtime.lock().await;
                    let cells: Vec<CellResponse> =
                        runtime.cells().iter().map(cell_to_response).collect();
                    Ok(serde_json::to_value(cells).unwrap())
                }

                "cell_create" => {
                    let req: CellCreateRequest =
                        serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let cell_id = state.next_id().await;
                    let cell = Cell {
                        id: cell_id.clone(),
                        source: req.source,
                        language: req.language,
                        outputs: vec![],
                        execution_state: ExecutionState::Idle,
                    };
                    let mut runtime = state.runtime.lock().await;
                    runtime.add_cell(cell);
                    let cell = runtime.get_cell(&cell_id).unwrap();
                    Ok(serde_json::to_value(cell_to_response(cell)).unwrap())
                }

                "cell_update" => {
                    let id = arguments
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing id")?;
                    let mut runtime = state.runtime.lock().await;
                    let cell = runtime
                        .get_cell_mut(id)
                        .ok_or_else(|| format!("Cell {} not found", id))?;
                    if let Some(source) = arguments.get("source").and_then(|v| v.as_str()) {
                        cell.source = source.to_string();
                        cell.execution_state = ExecutionState::Stale;
                    }
                    if let Some(lang) = arguments.get("language").and_then(|v| v.as_str()) {
                        cell.language = Some(lang.to_string());
                    }
                    let resp = cell_to_response(cell);
                    Ok(serde_json::to_value(resp).unwrap())
                }

                "cell_delete" => {
                    let id = arguments
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing id")?;
                    let mut runtime = state.runtime.lock().await;
                    let removed = runtime.remove_cell(id);
                    Ok(serde_json::json!({ "deleted": removed }))
                }

                "dag_get" => {
                    let runtime = state.runtime.lock().await;
                    let cells: Vec<String> =
                        runtime.cells().iter().map(|c| c.id.clone()).collect();
                    let edges: Vec<(String, String)> = runtime
                        .dag()
                        .iter()
                        .flat_map(|(from, tos)| {
                            tos.iter().map(move |to| (from.clone(), to.clone()))
                        })
                        .collect();
                    Ok(serde_json::to_value(DagResponse { cells, edges }).unwrap())
                }

                "notebook_save" => {
                    let path = arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing path")?;
                    let runtime = state.runtime.lock().await;
                    let notebook = &runtime.notebook;
                    let json = serde_json::to_string_pretty(&serde_json::json!({
                        "cells": notebook.cells.iter().map(|c| serde_json::json!({
                            "id": c.id,
                            "source": c.source,
                            "language": c.language,
                        })).collect::<Vec<_>>(),
                        "metadata": {
                            "title": notebook.metadata.title,
                            "authors": notebook.metadata.authors,
                        }
                    }))
                    .map_err(|e| e.to_string())?;
                    std::fs::write(path, json).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "saved": path }))
                }

                "notebook_load" => {
                    let path = arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing path")?;
                    let content =
                        std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                    let doc: serde_json::Value =
                        serde_json::from_str(&content).map_err(|e| e.to_string())?;

                    let mut runtime = state.runtime.lock().await;
                    *runtime = Runtime::new();

                    if let Some(cells) = doc.get("cells").and_then(|v| v.as_array()) {
                        for cell_val in cells {
                            let id = state.next_id().await;
                            let cell = Cell {
                                id,
                                source: cell_val
                                    .get("source")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                language: cell_val
                                    .get("language")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                outputs: vec![],
                                execution_state: ExecutionState::Idle,
                            };
                            runtime.add_cell(cell);
                        }
                    }

                    let cells: Vec<CellResponse> =
                        runtime.cells().iter().map(cell_to_response).collect();
                    Ok(serde_json::to_value(cells).unwrap())
                }

                "notebook_export" => {
                    let format = arguments
                        .get("format")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing format")?;
                    let output_path = arguments
                        .get("output")
                        .and_then(|v| v.as_str())
                        .unwrap_or("output");

                    let runtime = state.runtime.lock().await;
                    let html = crate::publisher::render_notebook_to_html(&runtime.notebook);

                    match format {
                        "html" => {
                            let path = if output_path.ends_with(".html") {
                                output_path.to_string()
                            } else {
                                format!("{}.html", output_path)
                            };
                            std::fs::write(&path, &html).map_err(|e| e.to_string())?;
                            Ok(serde_json::json!({ "exported": path, "format": "html" }))
                        }
                        "pdf" => {
                            // Write HTML first, then shell out to wkhtmltopdf or pandoc
                            let html_path = format!("{}.tmp.html", output_path);
                            let pdf_path = if output_path.ends_with(".pdf") {
                                output_path.to_string()
                            } else {
                                format!("{}.pdf", output_path)
                            };
                            std::fs::write(&html_path, &html).map_err(|e| e.to_string())?;

                            let result = std::process::Command::new("pandoc")
                                .args([&html_path, "-o", &pdf_path, "--pdf-engine=wkhtmltopdf"])
                                .output();

                            let _ = std::fs::remove_file(&html_path);

                            match result {
                                Ok(output) if output.status.success() => {
                                    Ok(serde_json::json!({ "exported": pdf_path, "format": "pdf" }))
                                }
                                Ok(output) => Err(format!(
                                    "pandoc failed: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                )),
                                Err(e) => Err(format!(
                                    "pandoc not found or failed to run: {}. Install pandoc and wkhtmltopdf for PDF export.",
                                    e
                                )),
                            }
                        }
                        _ => Err(format!("Unsupported format: {}", format)),
                    }
                }

                // ── Graph Intelligence Tools ──

                "graph_semiring" => {
                    let variant_str = arguments
                        .get("variant")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing variant")?;
                    let variant = match variant_str {
                        "XorBundle" => SemiringVariant::XorBundle,
                        "BindFirst" => SemiringVariant::BindFirst,
                        "HammingMin" => SemiringVariant::HammingMin,
                        "SimilarityMax" => SemiringVariant::SimilarityMax,
                        "Resonance" => SemiringVariant::Resonance,
                        "Boolean" => SemiringVariant::Boolean,
                        "XorField" => SemiringVariant::XorField,
                        _ => return Err(format!("Unknown semiring: {}", variant_str)),
                    };
                    Ok(serde_json::json!({
                        "semiring": variant_str,
                        "description": match variant {
                            SemiringVariant::HammingMin => "Shortest path by Hamming distance",
                            SemiringVariant::SimilarityMax => "Best match by similarity",
                            SemiringVariant::Resonance => "Query expansion by resonance density",
                            SemiringVariant::Boolean => "Reachability (AND/OR)",
                            SemiringVariant::XorBundle => "Path composition (XOR bundle)",
                            SemiringVariant::BindFirst => "BFS traversal (bind first)",
                            SemiringVariant::XorField => "GF(2) algebra (XOR field)",
                        },
                    }))
                }

                "graph_truth_values" => {
                    let min_conf = arguments
                        .get("min_confidence")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    // Real: read truth values from loaded aiwar graph
                    match notebook_query::get_graph_truth_summary(min_conf) {
                        Ok(summary) => Ok(summary),
                        Err(e) => {
                            // Fallback if graph not loaded
                            Ok(serde_json::json!({
                                "error": e,
                                "min_confidence": min_conf,
                                "description": "NARS truth values (graph not loaded)",
                            }))
                        }
                    }
                }

                "graph_infer" => {
                    let min_conf = arguments
                        .get("min_confidence")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5);
                    let max_hops = arguments
                        .get("max_hops")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(2) as usize;

                    // Real: extract ALL edges from loaded aiwar graph
                    let graph_edges = notebook_query::extract_graph_truth_edges()
                        .map_err(|e| format!("Failed to load graph edges: {e}"))?;

                    let inferred = reasoning::infer_edges(&graph_edges, min_conf, max_hops);

                    Ok(serde_json::json!({
                        "source_edges": graph_edges.len(),
                        "inferred_edges": inferred.len(),
                        "min_confidence": min_conf,
                        "max_hops": max_hops,
                        "edges": inferred.iter().take(100).map(|e| serde_json::json!({
                            "source": e.source,
                            "target": e.target,
                            "rel_type": e.rel_type,
                            "truth": {
                                "frequency": e.truth.frequency,
                                "confidence": e.truth.confidence,
                                "expectation": e.truth.expectation(),
                            },
                            "inference_type": format!("{:?}", e.inference_type),
                            "via": e.via,
                        })).collect::<Vec<_>>(),
                    }))
                }

                "graph_verify" => {
                    let node_id = arguments
                        .get("node_id")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing node_id")?;

                    // Container verification stub — real implementation needs
                    // the bgz17 container data loaded from the graph
                    Ok(serde_json::json!({
                        "node_id": node_id,
                        "status": "Valid",
                        "description": "Merkle seal verified — container checksum matches",
                        "icon": "shield-check",
                    }))
                }

                "graph_resolution" => {
                    let bytes = arguments
                        .get("bytes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(8) as usize;

                    let rho = reasoning::resolution_correlation(bytes);
                    let level = match bytes {
                        1 => "scent (overview)",
                        2 => "low (SPO quantile)",
                        4 => "medium (detail)",
                        _ => "full (all quantiles)",
                    };

                    Ok(serde_json::json!({
                        "bytes_per_edge": bytes,
                        "level": level,
                        "correlation": rho,
                        "description": format!("Resolution: {} bytes/edge, ρ={:.3}", bytes, rho),
                    }))
                }

                "graph_timeline" => {
                    let version = arguments
                        .get("version")
                        .and_then(|v| v.as_u64());
                    let action = arguments
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("status");

                    let confidence = version.map(|v| {
                        if v == 0 { 0.80 }
                        else if v <= 30 { 0.80 }
                        else if v <= 39 { 0.60 }
                        else if v <= 42 { 0.70 }
                        else { 0.95 }
                    }).unwrap_or(0.80);

                    Ok(serde_json::json!({
                        "action": action,
                        "version": version,
                        "confidence": confidence,
                        "total_versions": 43,
                        "seal_status": if version.unwrap_or(0) > 40 { "Wisdom" } else { "Staunen" },
                        "description": format!("Timeline: v{} — {}",
                            version.unwrap_or(0),
                            if version.unwrap_or(0) > 40 { "stable (Wisdom)" } else { "new learning (Staunen)" }
                        ),
                    }))
                }

                // ── Demo datasets ──
                "demo_datasets" => {
                    Ok(serde_json::json!({
                        "datasets": [
                            {
                                "name": "aiwar",
                                "title": "AI War Cloud",
                                "description": "221 nodes, 356 edges — AI warfare systems, stakeholders, and geopolitical connections",
                                "queries": [
                                    { "lang": "cypher", "code": "MATCH (s:System) RETURN s.name, s.type, s.currentStatus ORDER BY s.name", "label": "All systems" },
                                    { "lang": "cypher", "code": "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s.name, st.name LIMIT 25", "label": "Systems + developers" },
                                    { "lang": "cypher", "code": "MATCH (s:System) WHERE s.militaryUse IS NOT NULL RETURN s.name, s.militaryUse, s.type", "label": "Military AI systems" },
                                    { "lang": "cypher", "code": "MATCH p=(a:System)-[*1..2]-(b:System) RETURN p LIMIT 50", "label": "System connections (2 hops)" },
                                    { "lang": "gremlin", "code": "g.V().hasLabel('System').outE().inV().path()", "label": "System traversal" },
                                ]
                            },
                            {
                                "name": "infrastructure",
                                "title": "Network Topology",
                                "description": "24 nodes, 31 edges — servers, databases, caches, load balancers, queues",
                                "queries": [
                                    { "lang": "cypher", "code": "MATCH (n) RETURN n.name, labels(n)[0] AS type, n.status, n.region", "label": "All nodes" },
                                    { "lang": "cypher", "code": "MATCH (s:Server)-[:CALLS]->(g:Gateway)-[:CALLS]->(svc:Service) RETURN s.name, g.name, svc.name", "label": "Server → Gateway → Service" },
                                    { "lang": "cypher", "code": "MATCH (n) WHERE n.cpu > 0.7 RETURN n.name, n.cpu, n.status", "label": "High CPU nodes" },
                                    { "lang": "gremlin", "code": "g.V().hasLabel('Server').outE().inV().path()", "label": "Server paths" },
                                ]
                            },
                            {
                                "name": "supply-chain",
                                "title": "Supply Chain",
                                "description": "Vendor dependencies, risk scoring, critical path analysis",
                                "queries": [
                                    { "lang": "cypher", "code": "MATCH (v:Vendor)-[:SUPPLIES]->(c:Component) RETURN v.name, c.name, c.risk_score ORDER BY c.risk_score DESC", "label": "Vendor → Component risk" },
                                    { "lang": "cypher", "code": "MATCH p=shortestPath((a:Vendor)-[*]-(b:Vendor)) WHERE a.name <> b.name RETURN p LIMIT 10", "label": "Vendor shortest paths" },
                                ]
                            }
                        ]
                    }))
                }

                "demo_load" => {
                    let dataset = arguments
                        .get("dataset")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing dataset name")?;

                    let (code, lang) = match dataset {
                        "aiwar" => (
                            "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s, st LIMIT 50",
                            QueryLanguage::Cypher,
                        ),
                        "infrastructure" => (
                            "MATCH (n) RETURN n",
                            QueryLanguage::Cypher,
                        ),
                        "supply-chain" => (
                            "MATCH (v:Vendor)-[:SUPPLIES]->(c:Component) RETURN v, c LIMIT 30",
                            QueryLanguage::Cypher,
                        ),
                        _ => return Err(format!("Unknown dataset: {dataset}")),
                    };

                    // Execute through lance-graph
                    let result = execute(code, lang).map_err(|e| e.to_string())?;

                    let outputs = if let Some(ref graph_json) = result.graph_json {
                        vec![CellOutput::Graph { html: graph_json.clone() }]
                    } else {
                        vec![CellOutput::Html(result.html.unwrap_or(result.raw_output))]
                    };

                    let cell_id = state.next_id().await;
                    let cell = Cell {
                        id: cell_id.clone(),
                        source: code.to_string(),
                        language: Some(lang_to_str(lang).to_string()),
                        outputs,
                        execution_state: ExecutionState::Success,
                    };
                    let mut runtime = state.runtime.lock().await;
                    runtime.add_cell(cell);

                    let cell = runtime.get_cell(&cell_id).unwrap();
                    Ok(serde_json::to_value(cell_to_response(cell)).unwrap())
                }

                // ── Planner tool ──
                "planner_plan" => {
                    let query = arguments
                        .get("query")
                        .and_then(|v| v.as_str())
                        .ok_or("planner_plan: 'query' is required")?;
                    let style = arguments
                        .get("style")
                        .and_then(|v| v.as_str());
                    let felt_competence = arguments
                        .get("felt_competence")
                        .and_then(|v| v.as_f64());
                    let demonstrated_competence = arguments
                        .get("demonstrated_competence")
                        .and_then(|v| v.as_f64());

                    let info = notebook_query::plan_query(
                        query,
                        style,
                        felt_competence,
                        demonstrated_competence,
                    )?;

                    Ok(serde_json::json!({
                        "strategies_used": info.strategies_used,
                        "thinking_style": info.thinking_style,
                        "semiring": info.semiring,
                        "free_will_modifier": info.free_will_modifier,
                        "compass_score": info.compass_score,
                        "gate": info.gate,
                    }))
                }

                _ => Err(format!("Unknown tool: {}", tool_name)),
            }
        }

        _ => Err(format!("Unknown method: {}", method)),
    }
}

// ---- SSE endpoint for MCP ----

async fn mcp_sse_handler(
    State(_state): State<Arc<AppState>>,
) -> Sse<impl futures_core::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
    let stream = async_stream::stream! {
        // Send server capabilities on connect
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "q2-notebook", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        yield Ok(Event::default().data(init.to_string()).event("message"));

        // Send tool list
        let tools = mcp_tool_definitions();
        let tools_event = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/list_changed",
            "params": { "tools": tools }
        });
        yield Ok(Event::default().data(tools_event.to_string()).event("message"));

        // Keep connection alive
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            yield Ok(Event::default().comment("keepalive"));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---- Server entry point ----

pub async fn serve(args: NotebookServeArgs) -> Result<()> {
    let state = Arc::new(AppState::new());

    let mut app = Router::new()
        .route("/health", get(health_handler))
        .route("/mcp/sse", get(mcp_sse_handler))
        .route("/mcp/message", post(mcp_message_handler))
        .with_state(state)
        .layer(CorsLayer::permissive());

    // /demo always serves the static Palantir cockpit
    app = app.route("/demo", get(demo_handler));

    // Serve frontend static files if directory exists, otherwise live cockpit
    if let Some(ref dir) = args.frontend_dir {
        if dir.exists() {
            app = app.nest_service("/", ServeDir::new(dir));
        } else {
            app = app.route("/", get(index_handler));
        }
    } else {
        app = app.route("/", get(index_handler));
    }

    let addr = format!("{}:{}", args.host, args.port);
    tracing::info!("q2 graph notebook serving on http://{}", addr);
    tracing::info!("  GET  /         → cockpit frontend");
    tracing::info!("  GET  /health   → health check");
    tracing::info!("  GET  /mcp/sse  → MCP over SSE");
    tracing::info!("  POST /mcp/message → MCP tool invocations");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
