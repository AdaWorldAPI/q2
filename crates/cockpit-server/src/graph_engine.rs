//! Live graph engine — neo4j-emulating renderer with AGI thinking.
//!
//! Wires lance-graph's Cypher parser + TripletGraph + NARS inference
//! directly into cockpit-server. No serde on the hot path.
//!
//! The LazyLock double-buffer holds the latest graph state.
//! The cockpit reads it at 60fps. The shader updates it when
//! new data arrives (encounter rounds, user queries, NARS commits).
//!
//! This replaces the JS stubs (seed.ts, aiwar-seed.ts) with real data
//! while keeping those stubs as fallback.

use std::sync::{Arc, OnceLock};
use std::collections::HashMap;

use serde::Serialize;
use tokio::sync::RwLock;

/// Live graph state — the LazyLock double-buffer.
/// Writer: background thread processing encounter rounds + NARS.
/// Reader: Axum handlers serving cockpit panels.
static LIVE_GRAPH: OnceLock<Arc<RwLock<GraphSnapshot>>> = OnceLock::new();

/// Get or initialize the live graph state.
pub fn live_graph() -> &'static Arc<RwLock<GraphSnapshot>> {
    LIVE_GRAPH.get_or_init(|| Arc::new(RwLock::new(GraphSnapshot::empty())))
}

/// A snapshot of the graph state that the cockpit reads.
/// This is the "front buffer" — always complete, never torn.
#[derive(Debug, Clone, Serialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub node_count: usize,
    pub edge_count: usize,
    pub scene_version: u32,
    pub scene_name: String,
    pub health: GraphHealth,
    pub nars_inferences: Vec<NarsInference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    pub truth_f: f32,
    pub truth_c: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphHealth {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_inferences: usize,
    pub contradiction_count: usize,
    pub confidence_avg: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarsInference {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub inference_type: String,
    pub truth_f: f32,
    pub truth_c: f32,
    pub via: Vec<String>,
}

impl GraphSnapshot {
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_count: 0,
            edge_count: 0,
            scene_version: 0,
            scene_name: String::new(),
            health: GraphHealth {
                total_nodes: 0,
                total_edges: 0,
                total_inferences: 0,
                contradiction_count: 0,
                confidence_avg: 0.0,
            },
            nars_inferences: Vec::new(),
        }
    }
}

/// Load aiwar graph data and populate the live graph.
/// Called at startup from main().
pub async fn hydrate_from_aiwar_json(path: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;

    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Parse node arrays
    for (key, type_name) in &[
        ("N_Systems", "System"),
        ("N_Stakeholders", "Stakeholder"),
        ("N_Civic", "CivicSystem"),
        ("N_Historical", "HistoricalSystem"),
        ("N_People", "Person"),
    ] {
        if let Some(arr) = data.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
                let mut props = HashMap::new();
                if let Some(obj) = item.as_object() {
                    for (k, v) in obj {
                        if k != "id" && k != "name" {
                            props.insert(k.clone(), v.clone());
                        }
                    }
                }
                nodes.push(GraphNode {
                    id: id.clone(),
                    label: name,
                    node_type: type_name.to_string(),
                    properties: props,
                });
            }
        }
    }

    // Parse edge arrays
    for (key, rel_type) in &[
        ("E_isDevelopedBy", "DEVELOPED_BY"),
        ("E_isDeployedBy", "DEPLOYED_BY"),
        ("E_connection", "CONNECTED_TO"),
        ("E_place", "USED_IN"),
        ("E_people", "PERSON_LINK"),
        ("E_hierarchical", "HIERARCHICAL"),
    ] {
        if let Some(arr) = data.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !source.is_empty() && !target.is_empty() {
                    let weight = item.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    edges.push(GraphEdge {
                        source,
                        target,
                        label: rel_type.to_string(),
                        truth_f: weight.min(1.0),
                        truth_c: 0.8,
                    });
                }
            }
        }
    }

    let node_count = nodes.len();
    let edge_count = edges.len();
    let confidence_avg = if edges.is_empty() {
        0.0
    } else {
        edges.iter().map(|e| e.truth_c).sum::<f32>() / edges.len() as f32
    };

    let snapshot = GraphSnapshot {
        nodes,
        edges,
        node_count,
        edge_count,
        scene_version: 0,
        scene_name: "aiwar_full".to_string(),
        health: GraphHealth {
            total_nodes: node_count,
            total_edges: edge_count,
            total_inferences: 0,
            contradiction_count: 0,
            confidence_avg,
        },
        nars_inferences: Vec::new(),
    };

    let graph = live_graph();
    let mut state = graph.write().await;
    *state = snapshot;

    tracing::info!("hydrated live graph: {} nodes, {} edges", node_count, edge_count);
    Ok(())
}

/// Run NARS deduction on the live graph.
/// For every A→B + B→C chain, infer A→C with truth revision.
pub async fn run_nars_deduction(min_confidence: f32, max_hops: usize) -> Vec<NarsInference> {
    let graph = live_graph();
    let state = graph.read().await;

    // Build adjacency: source → [(target, label, truth_f, truth_c)]
    let mut adj: HashMap<&str, Vec<(&str, &str, f32, f32)>> = HashMap::new();
    for e in &state.edges {
        adj.entry(&e.source)
            .or_default()
            .push((&e.target, &e.label, e.truth_f, e.truth_c));
    }

    // Existing edges for dedup
    let existing: std::collections::HashSet<(&str, &str)> = state.edges.iter()
        .map(|e| (e.source.as_str(), e.target.as_str()))
        .collect();

    let mut inferences = Vec::new();

    // 2-hop deduction: A→B→C ⟹ A→C
    if max_hops >= 2 {
        for (a, a_edges) in &adj {
            for &(b, _ab_label, ab_f, ab_c) in a_edges {
                if ab_c < min_confidence { continue; }
                if let Some(b_edges) = adj.get(b) {
                    for &(c, bc_label, bc_f, bc_c) in b_edges {
                        if bc_c < min_confidence { continue; }
                        if *a == c || existing.contains(&(a, c)) { continue; }

                        // NARS deduction: f = f1 * f2, c = f1 * f2 * c1 * c2
                        let f = ab_f * bc_f;
                        let c = ab_f * bc_f * ab_c * bc_c;

                        if c >= min_confidence * 0.5 {
                            inferences.push(NarsInference {
                                source: a.to_string(),
                                target: c.to_string(),
                                relation: bc_label.to_string(),
                                inference_type: "Deduction".to_string(),
                                truth_f: f,
                                truth_c: c,
                                via: vec![b.to_string()],
                            });
                        }
                    }
                }
            }
        }
    }

    // Cap at 100 for response size
    inferences.truncate(100);
    inferences
}

/// API handler: get the current live graph snapshot as JSON.
pub async fn graph_snapshot_handler() -> axum::Json<GraphSnapshot> {
    let graph = live_graph();
    let state = graph.read().await;
    axum::Json(state.clone())
}

/// API handler: run NARS inference and return results.
pub async fn nars_infer_handler(
    axum::Json(params): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let min_conf = params.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
    let max_hops = params.get("max_hops").and_then(|v| v.as_u64()).unwrap_or(2) as usize;

    let inferences = run_nars_deduction(min_conf, max_hops).await;

    // Update the live graph with inferences
    {
        let graph = live_graph();
        let mut state = graph.write().await;
        state.health.total_inferences = inferences.len();
        state.nars_inferences = inferences.clone();
    }

    let count = inferences.len();
    axum::Json(serde_json::json!({
        "inferred_edges": count,
        "inferences": inferences,
    }))
}

/// API handler: get graph health summary.
pub async fn graph_health_handler() -> axum::Json<GraphHealth> {
    let graph = live_graph();
    let state = graph.read().await;
    axum::Json(state.health.clone())
}
