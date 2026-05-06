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
//!
//! ── NARS truth-value source-of-truth ──────────────────────────────
//! Edge truth values use `lance_graph_contract::exploration::NarsTruth`
//! (frequency, confidence) rather than bare `(f32, f32)` pairs. The
//! inference-type label uses `lance_graph_contract::nars::InferenceType`.
//!
//! `NarsTruth::revision` is provided by the contract. NARS deduction
//! now bridges to `lance_graph_planner::nars::truth::TruthValue::deduction`
//! — the canonical truth-revision algebra. The local arithmetic is
//! unchanged in Phase 2B (same `f = f1*f2, c = f1*f2*c1*c2` formula);
//! the change is *which crate computes it*. Resolves TRUTH-1 ledger row
//! from copy #4 to canonical. See `nars_deduction()` below.
//!
//! Wire JSON keeps the historical field names `truth_f` / `truth_c` so
//! the cockpit React frontend keeps working unchanged.

use std::sync::{Arc, OnceLock};
use std::collections::HashMap;

use serde::Serialize;
use tokio::sync::RwLock;

use lance_graph_contract::exploration::NarsTruth;
use lance_graph_contract::nars::InferenceType;

/// Live graph state — the LazyLock double-buffer.
/// Writer: background thread processing encounter rounds + NARS.
/// Reader: Axum handlers serving cockpit panels.
static LIVE_GRAPH: OnceLock<Arc<RwLock<GraphSnapshot>>> = OnceLock::new();

/// Get or initialize the live graph state.
pub fn live_graph() -> &'static Arc<RwLock<GraphSnapshot>> {
    LIVE_GRAPH.get_or_init(|| Arc::new(RwLock::new(GraphSnapshot::empty())))
}

/// NARS deduction `A→B, B→C ⊢ A→C` via the canonical
/// `lance_graph_planner::nars::truth::TruthValue::deduction`.
///
/// Bridges between the contract's `NarsTruth { frequency, confidence }`
/// (used by GraphEdge in this crate) and the planner's `TruthValue`
/// (which carries the canonical revision/deduction/induction/abduction
/// semiring). Both use the same f,c pair on the wire — the bridge is
/// just a struct conversion, not a math change. Replaces the local
/// fallback that lived here through Phase 2A.
fn nars_deduction(ab: &NarsTruth, bc: &NarsTruth) -> NarsTruth {
    use lance_graph_planner::nars::truth::TruthValue;
    let ab_t = TruthValue::new(ab.frequency, ab.confidence);
    let bc_t = TruthValue::new(bc.frequency, bc.confidence);
    let result = ab_t.deduction(&bc_t);
    NarsTruth::new(result.frequency, result.confidence)
}

/// String label for an `InferenceType` (wire JSON compat).
fn inference_type_label(t: InferenceType) -> &'static str {
    match t {
        InferenceType::Deduction => "Deduction",
        InferenceType::Induction => "Induction",
        InferenceType::Abduction => "Abduction",
        InferenceType::Revision => "Revision",
        InferenceType::Synthesis => "Synthesis",
    }
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

/// A graph edge. Internally carries a contract-side `NarsTruth`; the wire
/// JSON keeps the historical `truth_f`/`truth_c` field names for frontend
/// compatibility via a custom `Serialize` impl below.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    pub truth: NarsTruth,
}

impl GraphEdge {
    /// Convenience: legacy `truth_f` accessor for in-process consumers.
    pub fn truth_f(&self) -> f32 { self.truth.frequency }
    /// Convenience: legacy `truth_c` accessor for in-process consumers.
    pub fn truth_c(&self) -> f32 { self.truth.confidence }
}

impl Serialize for GraphEdge {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("GraphEdge", 5)?;
        st.serialize_field("source", &self.source)?;
        st.serialize_field("target", &self.target)?;
        st.serialize_field("label", &self.label)?;
        st.serialize_field("truth_f", &self.truth.frequency)?;
        st.serialize_field("truth_c", &self.truth.confidence)?;
        st.end()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphHealth {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_inferences: usize,
    pub contradiction_count: usize,
    pub confidence_avg: f32,
}

/// A NARS inference result. Internally typed with `NarsTruth` and
/// `InferenceType`; wire JSON keeps the historical
/// `truth_f` / `truth_c` / `inference_type` (string) fields.
#[derive(Debug, Clone)]
pub struct NarsInference {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub inference_type: InferenceType,
    pub truth: NarsTruth,
    pub via: Vec<String>,
}

impl Serialize for NarsInference {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("NarsInference", 7)?;
        st.serialize_field("source", &self.source)?;
        st.serialize_field("target", &self.target)?;
        st.serialize_field("relation", &self.relation)?;
        st.serialize_field("inference_type", inference_type_label(self.inference_type))?;
        st.serialize_field("truth_f", &self.truth.frequency)?;
        st.serialize_field("truth_c", &self.truth.confidence)?;
        st.serialize_field("via", &self.via)?;
        st.end()
    }
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
                        truth: NarsTruth::new(weight.min(1.0), 0.8),
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
        edges.iter().map(|e| e.truth.confidence).sum::<f32>() / edges.len() as f32
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
/// For every A→B + B→C chain, infer A→C with truth deduction.
///
/// Truth-value algebra is `lance_graph_contract::exploration::NarsTruth`.
/// Deduction is delegated to `nars_deduction()`, which bridges to
/// `lance_graph_planner::nars::truth::TruthValue::deduction` — the
/// canonical NARS-1.x formula `f = f1*f2, c = f1*f2*c1*c2`.
pub async fn run_nars_deduction(min_confidence: f32, max_hops: usize) -> Vec<NarsInference> {
    let graph = live_graph();
    let state = graph.read().await;

    // Build adjacency: source → [(target, label, truth)]
    // Truths are owned `NarsTruth` microcopies (Copy type) per the
    // borrow-strategy doctrine: read once, compute on owned copies.
    let mut adj: HashMap<&str, Vec<(&str, &str, NarsTruth)>> = HashMap::new();
    for e in &state.edges {
        adj.entry(&e.source)
            .or_default()
            .push((&e.target, &e.label, e.truth));
    }

    // Existing edges for dedup
    let existing: std::collections::HashSet<(&str, &str)> = state.edges.iter()
        .map(|e| (e.source.as_str(), e.target.as_str()))
        .collect();

    let mut inferences = Vec::new();

    // 2-hop deduction: A→B→C ⟹ A→C
    if max_hops >= 2 {
        for (a, a_edges) in &adj {
            for &(b, _ab_label, ab_truth) in a_edges {
                if ab_truth.confidence < min_confidence { continue; }
                if let Some(b_edges) = adj.get(b) {
                    for &(c_node, bc_label, bc_truth) in b_edges {
                        if bc_truth.confidence < min_confidence { continue; }
                        if *a == c_node || existing.contains(&(a, c_node)) { continue; }

                        // NARS deduction via canonical planner TruthValue::deduction.
                        let inferred = nars_deduction(&ab_truth, &bc_truth);

                        if inferred.confidence >= min_confidence * 0.5 {
                            inferences.push(NarsInference {
                                source: a.to_string(),
                                target: c_node.to_string(),
                                relation: bc_label.to_string(),
                                inference_type: InferenceType::Deduction,
                                truth: inferred,
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
/// Optional `node_id` filters to inferences involving that node.
pub async fn nars_infer_handler(
    axum::Json(params): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let min_conf = params.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
    let max_hops = params.get("max_hops").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
    let node_id = params.get("node_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut inferences = run_nars_deduction(min_conf, max_hops).await;

    // Filter to node if requested
    if let Some(ref nid) = node_id {
        inferences.retain(|i| i.source == *nid || i.target == *nid);
    }

    // Update the live graph with inferences (unfiltered count stays in health)
    {
        let graph = live_graph();
        let mut state = graph.write().await;
        state.nars_inferences = inferences.clone();
        state.health.total_inferences = inferences.len();
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

#[cfg(test)]
mod nars_planner_bridge_tests {
    use super::*;
    #[test]
    fn deduction_matches_planner_canonical() {
        let ab = NarsTruth::new(0.9, 0.8);
        let bc = NarsTruth::new(0.7, 0.6);
        let result = nars_deduction(&ab, &bc);
        // Canonical NARS: f = f1*f2 = 0.63, c = f1*f2*c1*c2 = 0.3024
        assert!((result.frequency - 0.63).abs() < 0.01, "f={}", result.frequency);
        assert!((result.confidence - 0.3024).abs() < 0.01, "c={}", result.confidence);
    }
}
