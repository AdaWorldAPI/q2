//! Ingest aiwar-neo4j-harvest data into the lance-graph engine's data model.
//!
//! The graph **API/engine is lance-graph** (Cypher/Gremlin → DataFusion / SPO).
//! This crate is the **ingest layer**: it parses the harvest sources into a
//! plain, engine-agnostic property-graph model (`AiWarGraph`) that the
//! lance-graph-backed consumers hydrate:
//!
//!   * `cockpit-server::graph_engine` — render snapshot (+ NARS truth) for the cockpit
//!   * `notebook-query` — Arrow RecordBatches → lance-graph `CypherQuery` / planner
//!
//! Two sources, both from `AdaWorldAPI/aiwar-neo4j-harvest`:
//!   * `/data/aiwar_graph.json` — the base 221-node / 326-edge property graph
//!   * `/cypher/*.cypher` — 30 versioned enrichment rounds (parsed by
//!     [`encounter_round`], applied via [`AiWarGraph::apply_round`])
//!
//! `neo4j-rs` is intentionally **not** a dependency: it was a Neo4j-flavored
//! *GUI* experiment. Presentation (the neo4j-style cockpit) is the vis-network
//! layer; the graph API is lance-graph. This crate carries no engine.

pub mod encounter_round;

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Source schema mapping (aiwar_graph.json → property graph)
//
// Mirrors the node/edge array names in aiwar-neo4j-harvest/data/aiwar_graph.json
// and the canonical type/relationship labels used by the cockpit legend
// (System · Stakeholder · CivicSystem · HistoricalSystem · Person).
// ─────────────────────────────────────────────────────────────────────────────

/// (`json key`, `node_type`) for each node array in `aiwar_graph.json`.
pub const NODE_ARRAYS: &[(&str, &str)] = &[
    ("N_Systems", "System"),
    ("N_Stakeholders", "Stakeholder"),
    ("N_Civic", "CivicSystem"),
    ("N_Historical", "HistoricalSystem"),
    ("N_People", "Person"),
];

/// (`json key`, `rel_type`) for each edge array in `aiwar_graph.json`.
pub const EDGE_ARRAYS: &[(&str, &str)] = &[
    ("E_isDevelopedBy", "DEVELOPED_BY"),
    ("E_isDeployedBy", "DEPLOYED_BY"),
    ("E_connection", "CONNECTED_TO"),
    ("E_place", "USED_IN"),
    ("E_people", "PERSON_LINK"),
    ("E_hierarchical", "HIERARCHICAL"),
];

// ─────────────────────────────────────────────────────────────────────────────
// Property-graph model (engine-agnostic)
// ─────────────────────────────────────────────────────────────────────────────

/// A node in the AIWAR property graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    /// Stable identifier (the `id` field; falls back to `name`).
    pub id: String,
    /// Human-readable display label (the `name` field; falls back to `id`).
    pub label: String,
    /// Node label / type — one of the canonical cockpit types
    /// (`System`, `Stakeholder`, `CivicSystem`, `HistoricalSystem`, `Person`)
    /// or an enrichment-derived label.
    pub node_type: String,
    /// All remaining fields, preserved verbatim for rendering / querying.
    pub properties: HashMap<String, Value>,
}

/// A directed edge in the AIWAR property graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Relationship type (`DEVELOPED_BY`, `CONNECTED_TO`, …).
    pub rel_type: String,
    /// Edge weight (`weight` field; defaults to `1.0`).
    pub weight: f64,
    /// Remaining edge fields (`label`, `hover`, `reference`, …).
    pub properties: HashMap<String, Value>,
}

/// The ingested AIWAR property graph: plain nodes + edges, ready for a
/// lance-graph-backed consumer to hydrate.
#[derive(Debug, Clone, Default)]
pub struct AiWarGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl AiWarGraph {
    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Whether a node id is already present.
    pub fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n.id == id)
    }

    /// Render to the vis-network JSON shape the cockpit frontend expects:
    /// `{ "nodes": [{id,label,type,properties}], "edges": [{source,target,label}] }`.
    ///
    /// Nodes are de-duplicated by `id`.
    pub fn to_vis_json(&self) -> String {
        let mut seen = std::collections::HashSet::new();
        let nodes: Vec<Value> = self
            .nodes
            .iter()
            .filter(|n| seen.insert(n.id.clone()))
            .map(|n| {
                let props: serde_json::Map<String, Value> = n
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                serde_json::json!({
                    "id": n.id,
                    "label": n.label,
                    "type": n.node_type,
                    "properties": props,
                })
            })
            .collect();
        let edges: Vec<Value> = self
            .edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "source": e.source,
                    "target": e.target,
                    "label": e.rel_type,
                })
            })
            .collect();
        serde_json::json!({ "nodes": nodes, "edges": edges }).to_string()
    }

    /// Merge an enrichment round (parsed from a `/cypher/*.cypher` file) into
    /// this graph. New nodes are added by id; nodes already present keep their
    /// existing properties but gain any new ones. All round edges are appended.
    pub fn apply_round(&mut self, round: &encounter_round::EncounterRound) {
        for cn in &round.nodes {
            if let Some(existing) = self.nodes.iter_mut().find(|n| n.id == cn.id) {
                for (k, v) in &cn.properties {
                    existing
                        .properties
                        .entry(k.clone())
                        .or_insert_with(|| Value::String(v.clone()));
                }
            } else {
                let label = cn
                    .properties
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| cn.id.clone());
                let node_type = cn
                    .labels
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Node".to_string());
                let properties = cn
                    .properties
                    .iter()
                    .filter(|(k, _)| k.as_str() != "id" && k.as_str() != "name")
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                self.nodes.push(GraphNode {
                    id: cn.id.clone(),
                    label,
                    node_type,
                    properties,
                });
            }
        }
        for ce in &round.edges {
            let weight = ce
                .properties
                .get("weight")
                .and_then(|w| w.parse::<f64>().ok())
                .unwrap_or(1.0);
            let properties = ce
                .properties
                .iter()
                .filter(|(k, _)| k.as_str() != "weight")
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            self.edges.push(GraphEdge {
                source: ce.source.clone(),
                target: ce.target.clone(),
                rel_type: ce.rel_type.clone(),
                weight,
                properties,
            });
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("IO error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Ingest
// ─────────────────────────────────────────────────────────────────────────────

/// Load `aiwar_graph.json` from a file path into an [`AiWarGraph`].
pub fn load_from_file(path: &str) -> Result<AiWarGraph, IngestError> {
    let content = std::fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_string(),
        source,
    })?;
    load_from_str(&content)
}

/// Parse `aiwar_graph.json` text into an [`AiWarGraph`].
///
/// The harvest source carries bare `NaN` / `Infinity` literals in numeric
/// property positions (1660 `NaN` in the canonical dump) — valid JavaScript,
/// but the JSON spec forbids them and `serde_json` rejects them outright. They
/// mean "missing / unknown", so they are mapped to `null` (dropped downstream)
/// before parsing. Without this, the only harvest copy that carries the
/// `cypher/` enrichment sibling fails to load and the graph silently falls
/// back to the un-enriched 221-node base.
pub fn load_from_str(text: &str) -> Result<AiWarGraph, IngestError> {
    let sanitized = sanitize_non_finite(text);
    let value: Value = serde_json::from_str(&sanitized)?;
    Ok(load_from_value(&value))
}

/// Replace bare non-finite JSON literals (`NaN`, `Infinity`, `-Infinity`) in
/// value position with `null`. Anchored on a preceding `:` / `[` / `,` so that
/// string *contents* are never rewritten (the harvest has zero quoted `NaN`).
fn sanitize_non_finite(text: &str) -> std::borrow::Cow<'_, str> {
    static NON_FINITE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"([:\[,]\s*)(?:NaN|-?Infinity)\b").expect("valid non-finite regex")
    });
    NON_FINITE.replace_all(text, "${1}null")
}

/// Build an [`AiWarGraph`] from a parsed `aiwar_graph.json` value.
///
/// Generic walk that preserves every node/edge property field verbatim. Nodes
/// and edges with empty source/target ids are skipped.
pub fn load_from_value(data: &Value) -> AiWarGraph {
    let mut graph = AiWarGraph::default();

    for (key, node_type) in NODE_ARRAYS {
        let Some(arr) = data.get(*key).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in arr {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let label = obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let properties: HashMap<String, Value> = obj
                .iter()
                .filter(|(k, _)| k.as_str() != "id" && k.as_str() != "name")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            graph.nodes.push(GraphNode {
                id,
                label,
                node_type: node_type.to_string(),
                properties,
            });
        }
    }

    for (key, rel_type) in EDGE_ARRAYS {
        let Some(arr) = data.get(*key).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in arr {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let source = obj
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let target = obj
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if source.is_empty() || target.is_empty() {
                continue;
            }
            let weight = obj.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let properties: HashMap<String, Value> = obj
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "source" | "target" | "weight"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            graph.edges.push(GraphEdge {
                source,
                target,
                rel_type: rel_type.to_string(),
                weight,
                properties,
            });
        }
    }

    graph
}

/// Load the base graph from `aiwar_graph.json` and apply every enrichment
/// round found in `cypher_dir` (version-ordered). Missing `cypher_dir` is not
/// an error — the base graph is returned unenriched.
pub fn load_with_enrichment(
    graph_json_path: &str,
    cypher_dir: &Path,
) -> Result<AiWarGraph, IngestError> {
    let mut graph = load_from_file(graph_json_path)?;
    if cypher_dir.is_dir() {
        if let Ok(rounds) = encounter_round::load_encounter_rounds(cypher_dir) {
            for round in &rounds {
                graph.apply_round(round);
            }
            tracing::info!(
                "aiwar-ingest: applied {} enrichment rounds from {}",
                rounds.len(),
                cypher_dir.display()
            );
        }
    }
    tracing::info!(
        "aiwar-ingest: loaded {} nodes, {} edges",
        graph.node_count(),
        graph.edge_count()
    );
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "N_Systems": [
            {"id": "Maven", "name": "Project Maven", "year": 2017, "MLTask": "object-detection"},
            {"id": "Lavender", "name": "Lavender"}
        ],
        "N_Stakeholders": [
            {"id": "Palantir", "name": "Palantir Technologies", "type": "company"}
        ],
        "N_People": [
            {"id": "Karp", "name": "Alex Karp"}
        ],
        "E_isDevelopedBy": [
            {"source": "Maven", "target": "Palantir", "weight": 3, "label": "builds"}
        ],
        "E_people": [
            {"source": "Karp", "target": "Palantir"},
            {"source": "", "target": "Palantir"}
        ]
    }"#;

    #[test]
    fn parses_nodes_with_types_and_properties() {
        let g = load_from_str(FIXTURE).expect("valid fixture");
        // 2 systems + 1 stakeholder + 1 person
        assert_eq!(g.node_count(), 4);

        let maven = g.nodes.iter().find(|n| n.id == "Maven").unwrap();
        assert_eq!(maven.node_type, "System");
        assert_eq!(maven.label, "Project Maven");
        // non id/name fields preserved verbatim
        assert_eq!(
            maven.properties.get("year").unwrap(),
            &serde_json::json!(2017)
        );
        assert_eq!(
            maven.properties.get("MLTask").unwrap(),
            &serde_json::json!("object-detection")
        );

        let palantir = g.nodes.iter().find(|n| n.id == "Palantir").unwrap();
        assert_eq!(palantir.node_type, "Stakeholder");
    }

    #[test]
    fn parses_edges_with_rel_type_and_skips_empty_endpoints() {
        let g = load_from_str(FIXTURE).expect("valid fixture");
        // 1 developed_by + 1 valid people edge (the empty-source one is skipped)
        assert_eq!(g.edge_count(), 2);

        let dev = g
            .edges
            .iter()
            .find(|e| e.rel_type == "DEVELOPED_BY")
            .unwrap();
        assert_eq!(dev.source, "Maven");
        assert_eq!(dev.target, "Palantir");
        assert_eq!(dev.weight, 3.0);
        assert_eq!(
            dev.properties.get("label").unwrap(),
            &serde_json::json!("builds")
        );

        assert!(
            g.edges
                .iter()
                .all(|e| !e.source.is_empty() && !e.target.is_empty())
        );
    }

    #[test]
    fn vis_json_has_expected_shape() {
        let g = load_from_str(FIXTURE).expect("valid fixture");
        let v: Value = serde_json::from_str(&g.to_vis_json()).expect("valid vis json");
        let nodes = v.get("nodes").and_then(|x| x.as_array()).unwrap();
        let edges = v.get("edges").and_then(|x| x.as_array()).unwrap();
        assert_eq!(nodes.len(), 4);
        assert_eq!(edges.len(), 2);
        let n0 = &nodes[0];
        for k in ["id", "label", "type", "properties"] {
            assert!(n0.get(k).is_some(), "node missing key {k}");
        }
        let e0 = &edges[0];
        for k in ["source", "target", "label"] {
            assert!(e0.get(k).is_some(), "edge missing key {k}");
        }
    }

    #[test]
    fn apply_round_merges_new_nodes_and_edges() {
        let mut g = load_from_str(FIXTURE).expect("valid fixture");
        let before_nodes = g.node_count();
        let before_edges = g.edge_count();

        let mut props = HashMap::new();
        props.insert("name".to_string(), "Pete Hegseth".to_string());
        let round = encounter_round::EncounterRound {
            version: 31,
            name: "patch".to_string(),
            source_file: "p.cypher".to_string(),
            confidence: 0.6,
            nodes: vec![encounter_round::CypherNode {
                id: "Hegseth".to_string(),
                labels: vec!["Person".to_string()],
                properties: props,
            }],
            edges: vec![encounter_round::CypherEdge {
                source: "Hegseth".to_string(),
                target: "Palantir".to_string(),
                rel_type: "CONNECTED_TO".to_string(),
                properties: HashMap::new(),
            }],
        };
        g.apply_round(&round);

        assert_eq!(g.node_count(), before_nodes + 1);
        assert_eq!(g.edge_count(), before_edges + 1);
        let h = g.nodes.iter().find(|n| n.id == "Hegseth").unwrap();
        assert_eq!(h.node_type, "Person");
        assert_eq!(h.label, "Pete Hegseth");
    }

    /// Integration: load the real aiwar_graph.json if present (vendored in the
    /// repo or in the sibling harvest checkout). Skips cleanly when absent so
    /// the unit suite stays hermetic.
    #[test]
    fn loads_real_aiwar_graph_when_available() {
        let candidates = [
            "cockpit/public/aiwar_graph.json",
            "../../cockpit/public/aiwar_graph.json",
            "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
            "../aiwar-neo4j-harvest/data/aiwar_graph.json",
        ];
        let Some(path) = candidates.iter().find(|p| Path::new(p).exists()) else {
            eprintln!("real aiwar_graph.json not found; skipping integration assertion");
            return;
        };
        let g = load_from_file(path).expect("real graph loads");
        // The canonical AIWAR dataset: 65+114+23+7+12 = 221 nodes.
        assert_eq!(g.node_count(), 221, "expected 221 nodes from {path}");
        assert!(
            g.edge_count() >= 300,
            "expected ~326+ edges, got {}",
            g.edge_count()
        );
        // vis json must round-trip.
        let _: Value = serde_json::from_str(&g.to_vis_json()).expect("vis json round-trips");
    }

    #[test]
    fn sanitizes_non_finite_literals_so_harvest_loads() {
        // The harvest dump uses bare NaN / Infinity in numeric fields (1660 NaN
        // in the canonical dump). Before the sanitizer these made serde_json
        // reject the whole file; the enrichable copy never loaded.
        let json = r#"{
            "N_Systems": [
                {"id": "X", "name": "Sys X", "score": NaN, "ratio": -Infinity, "cap": Infinity}
            ],
            "E_connection": [
                {"source": "X", "target": "X", "weight": NaN}
            ]
        }"#;
        let g = load_from_str(json).expect("non-finite literals sanitized to null");
        assert_eq!(g.node_count(), 1);
        let x = g.nodes.iter().find(|n| n.id == "X").expect("node X present");
        assert_eq!(x.label, "Sys X");
        // the non-finite numeric became JSON null (or was dropped) — not a parse error.
        assert!(matches!(
            x.properties.get("score"),
            None | Some(serde_json::Value::Null)
        ));
    }

    #[test]
    fn sanitizer_leaves_normal_json_untouched() {
        let g = load_from_str(FIXTURE).expect("valid fixture still parses");
        assert_eq!(g.node_count(), 4);
        let maven = g.nodes.iter().find(|n| n.id == "Maven").unwrap();
        // a real number is preserved verbatim (sanitizer only touches non-finite).
        assert_eq!(maven.properties.get("year").unwrap(), &serde_json::json!(2017));
    }
}
