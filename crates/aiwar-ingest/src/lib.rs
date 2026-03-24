//! Load aiwar_graph.json into neo4j-rs MemoryBackend.
//!
//! Hot path: 221 nodes + 356 edges, all in-memory. Zero network hops.
//! Cypher queries execute against Arrow-speed in-process data.

pub mod encounter_round;

use neo4j_rs::{Graph, PropertyMap, Value};
use neo4j_rs::storage::MemoryBackend;
use serde::Deserialize;

// ── JSON model (mirrors aiwar-neo4j-harvest/src/model.rs) ──

#[derive(Debug, Deserialize)]
pub struct AiWarGraphJson {
    #[serde(rename = "N_Systems", default)]
    pub systems: Vec<SystemJson>,
    #[serde(rename = "N_Stakeholders", default)]
    pub stakeholders: Vec<StakeholderJson>,
    #[serde(rename = "N_Civic", default)]
    pub civic: Vec<CivicJson>,
    #[serde(rename = "N_Historical", default)]
    pub historical: Vec<HistoricalJson>,
    #[serde(rename = "N_People", default)]
    pub people: Vec<PersonJson>,
    #[serde(rename = "E_isDevelopedBy", default)]
    pub edges_developed: Vec<EdgeJson>,
    #[serde(rename = "E_isDeployedBy", default)]
    pub edges_deployed: Vec<EdgeJson>,
    #[serde(rename = "E_connection", default)]
    pub edges_connection: Vec<EdgeJson>,
    #[serde(rename = "E_place", default)]
    pub edges_place: Vec<EdgeJson>,
    #[serde(rename = "E_people", default)]
    pub edges_people: Vec<EdgeJson>,
    #[serde(rename = "E_hierarchical", default)]
    pub meta_edges: Vec<MetaEdgeJson>,
    #[serde(rename = "Schema", default)]
    pub schema: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SystemJson {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    pub current_status: Option<String>,
    #[serde(rename = "type", default)]
    pub system_type: Option<String>,
    #[serde(rename = "MLTask", default)]
    pub ml_task: Option<String>,
    #[serde(rename = "militaryUse", default)]
    pub military_use: Option<String>,
    #[serde(rename = "civicUse", default)]
    pub civic_use: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub capacity: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub hover: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StakeholderJson {
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub stakeholder_type: Option<String>,
    #[serde(rename = "airo:type", default)]
    pub airo_type: Option<String>,
    #[serde(default)]
    pub hover: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CivicJson {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    pub current_status: Option<String>,
    #[serde(rename = "type", default)]
    pub system_type: Option<String>,
    #[serde(default)]
    pub hover: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoricalJson {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    pub current_status: Option<String>,
    #[serde(rename = "type", default)]
    pub system_type: Option<String>,
    #[serde(rename = "militaryUse", default)]
    pub military_use: Option<String>,
    #[serde(rename = "civicUse", default)]
    pub civic_use: Option<String>,
    #[serde(default)]
    pub hover: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PersonJson {
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub person_type: Option<String>,
    #[serde(rename = "airo:type", default)]
    pub airo_type: Option<String>,
    #[serde(default)]
    pub hover: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EdgeJson {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub hover: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MetaEdgeJson {
    pub source: String,
    pub target: String,
}

// ── Error ──

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Graph error: {0}")]
    Graph(#[from] neo4j_rs::Error),
}

// ── Ingest ──

/// Load aiwar_graph.json from a file path into an in-memory neo4j-rs graph.
pub async fn load_from_file(path: &str) -> Result<Graph<MemoryBackend>, IngestError> {
    let content = std::fs::read_to_string(path)?;
    let data: AiWarGraphJson = serde_json::from_str(&content)?;
    load_from_json(data).await
}

/// Load parsed JSON data into an in-memory neo4j-rs graph.
pub async fn load_from_json(data: AiWarGraphJson) -> Result<Graph<MemoryBackend>, IngestError> {
    let graph = Graph::open_memory().await?;

    // ── Systems ──
    for sys in &data.systems {
        let mut props = PropertyMap::new();
        props.insert("name".into(), Value::from(sys.name.as_str()));
        props.insert("id".into(), Value::from(sys.id.as_str()));
        if let Some(y) = sys.year { props.insert("year".into(), Value::from(y)); }
        insert_opt(&mut props, "currentStatus", &sys.current_status);
        insert_opt(&mut props, "type", &sys.system_type);
        insert_opt(&mut props, "MLTask", &sys.ml_task);
        insert_opt(&mut props, "militaryUse", &sys.military_use);
        insert_opt(&mut props, "civicUse", &sys.civic_use);
        insert_opt(&mut props, "purpose", &sys.purpose);
        insert_opt(&mut props, "capacity", &sys.capacity);
        insert_opt(&mut props, "output", &sys.output);
        insert_opt(&mut props, "impact", &sys.impact);
        insert_opt(&mut props, "hover", &sys.hover);
        graph.mutate(
            &format!("CREATE (n:System {{{}}})", props_to_cypher_map(&props)),
            PropertyMap::new(),
        ).await?;
    }

    // ── Stakeholders ──
    for st in &data.stakeholders {
        let mut props = PropertyMap::new();
        props.insert("name".into(), Value::from(st.name.as_str()));
        props.insert("id".into(), Value::from(st.id.as_str()));
        insert_opt(&mut props, "type", &st.stakeholder_type);
        insert_opt(&mut props, "airoType", &st.airo_type);
        insert_opt(&mut props, "hover", &st.hover);
        graph.mutate(
            &format!("CREATE (n:Stakeholder {{{}}})", props_to_cypher_map(&props)),
            PropertyMap::new(),
        ).await?;
    }

    // ── Civic ──
    for c in &data.civic {
        let mut props = PropertyMap::new();
        props.insert("name".into(), Value::from(c.name.as_str()));
        props.insert("id".into(), Value::from(c.id.as_str()));
        if let Some(y) = c.year { props.insert("year".into(), Value::from(y)); }
        insert_opt(&mut props, "currentStatus", &c.current_status);
        insert_opt(&mut props, "type", &c.system_type);
        insert_opt(&mut props, "hover", &c.hover);
        graph.mutate(
            &format!("CREATE (n:Civic {{{}}})", props_to_cypher_map(&props)),
            PropertyMap::new(),
        ).await?;
    }

    // ── Historical ──
    for h in &data.historical {
        let mut props = PropertyMap::new();
        props.insert("name".into(), Value::from(h.name.as_str()));
        props.insert("id".into(), Value::from(h.id.as_str()));
        if let Some(y) = h.year { props.insert("year".into(), Value::from(y)); }
        insert_opt(&mut props, "currentStatus", &h.current_status);
        insert_opt(&mut props, "type", &h.system_type);
        insert_opt(&mut props, "militaryUse", &h.military_use);
        insert_opt(&mut props, "civicUse", &h.civic_use);
        insert_opt(&mut props, "hover", &h.hover);
        graph.mutate(
            &format!("CREATE (n:Historical {{{}}})", props_to_cypher_map(&props)),
            PropertyMap::new(),
        ).await?;
    }

    // ── People ──
    for p in &data.people {
        let mut props = PropertyMap::new();
        props.insert("name".into(), Value::from(p.name.as_str()));
        props.insert("id".into(), Value::from(p.id.as_str()));
        insert_opt(&mut props, "type", &p.person_type);
        insert_opt(&mut props, "airoType", &p.airo_type);
        insert_opt(&mut props, "hover", &p.hover);
        graph.mutate(
            &format!("CREATE (n:Person {{{}}})", props_to_cypher_map(&props)),
            PropertyMap::new(),
        ).await?;
    }

    // ── Edges ──
    // DEVELOPED_BY: System → Stakeholder
    for e in &data.edges_developed {
        create_edge(&graph, &e.source, &e.target, "DEVELOPED_BY", e).await?;
    }
    for e in &data.edges_deployed {
        create_edge(&graph, &e.source, &e.target, "DEPLOYED_BY", e).await?;
    }
    for e in &data.edges_connection {
        create_edge(&graph, &e.source, &e.target, "CONNECTED_TO", e).await?;
    }
    for e in &data.edges_place {
        create_edge(&graph, &e.source, &e.target, "USED_IN", e).await?;
    }
    for e in &data.edges_people {
        create_edge(&graph, &e.source, &e.target, "PERSON_LINK", e).await?;
    }
    for e in &data.meta_edges {
        graph.mutate(
            &format!(
                "MATCH (a {{id: '{}'}}), (b {{id: '{}'}}) CREATE (a)-[:META_EDGE]->(b)",
                escape_cypher(&e.source),
                escape_cypher(&e.target),
            ),
            PropertyMap::new(),
        ).await?;
    }

    let total_nodes = data.systems.len() + data.stakeholders.len()
        + data.civic.len() + data.historical.len() + data.people.len();
    let total_edges = data.edges_developed.len() + data.edges_deployed.len()
        + data.edges_connection.len() + data.edges_place.len()
        + data.edges_people.len() + data.meta_edges.len();
    tracing::info!(
        "aiwar-ingest: loaded {} nodes, {} edges into MemoryBackend",
        total_nodes,
        total_edges,
    );

    Ok(graph)
}

async fn create_edge(
    graph: &Graph<MemoryBackend>,
    source: &str,
    target: &str,
    rel_type: &str,
    edge: &EdgeJson,
) -> Result<(), IngestError> {
    let mut prop_parts = Vec::new();
    if let Some(ref label) = edge.label {
        prop_parts.push(format!("label: '{}'", escape_cypher(label)));
    }
    if let Some(w) = edge.weight {
        prop_parts.push(format!("weight: {}", w));
    }
    if let Some(ref hover) = edge.hover {
        prop_parts.push(format!("hover: '{}'", escape_cypher(hover)));
    }
    let props_str = if prop_parts.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", prop_parts.join(", "))
    };

    graph.mutate(
        &format!(
            "MATCH (a {{id: '{}'}}), (b {{id: '{}'}}) CREATE (a)-[:{}{}]->(b)",
            escape_cypher(source),
            escape_cypher(target),
            rel_type,
            props_str,
        ),
        PropertyMap::new(),
    ).await?;
    Ok(())
}

fn insert_opt(props: &mut PropertyMap, key: &str, val: &Option<String>) {
    if let Some(v) = val {
        props.insert(key.into(), Value::from(v.as_str()));
    }
}

fn escape_cypher(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn props_to_cypher_map(props: &PropertyMap) -> String {
    props.iter()
        .map(|(k, v)| {
            let val_str = match v {
                Value::String(s) => format!("'{}'", escape_cypher(s)),
                Value::Int(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => format!("'{}'", escape_cypher(&format!("{:?}", v))),
            };
            format!("{}: {}", k, val_str)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Convert a neo4j-rs QueryResult into the vis-network JSON format
/// expected by the cockpit frontend.
pub fn query_result_to_vis_json(
    result: &neo4j_rs::QueryResult,
) -> String {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for row in &result.rows {
        for (_col, val) in &row.values {
            match val {
                Value::Node(node) => {
                    let label = node.labels.first().map(|s| s.as_str()).unwrap_or("Node");
                    let name = node.properties.get("name")
                        .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                        .unwrap_or("unknown");
                    let id_fallback = format!("n-{}", node.id.0);
                    let id = node.properties.get("id")
                        .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                        .unwrap_or(&id_fallback);

                    // Build properties object
                    let mut prop_obj = serde_json::Map::new();
                    for (k, v) in &node.properties {
                        prop_obj.insert(k.clone(), value_to_json(v));
                    }

                    nodes.push(serde_json::json!({
                        "id": id,
                        "label": name,
                        "type": label,
                        "properties": prop_obj,
                    }));
                }
                Value::Relationship(rel) => {
                    edges.push(serde_json::json!({
                        "source": format!("n-{}", rel.src.0),
                        "target": format!("n-{}", rel.dst.0),
                        "label": rel.rel_type,
                    }));
                }
                Value::Path(path) => {
                    for node in &path.nodes {
                        let label = node.labels.first().map(|s| s.as_str()).unwrap_or("Node");
                        let name = node.properties.get("name")
                            .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                            .unwrap_or("unknown");
                        let id_str = node.properties.get("id")
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .unwrap_or_else(|| format!("n-{}", node.id.0));

                        let mut prop_obj = serde_json::Map::new();
                        for (k, v) in &node.properties {
                            prop_obj.insert(k.clone(), value_to_json(v));
                        }

                        nodes.push(serde_json::json!({
                            "id": id_str,
                            "label": name,
                            "type": label,
                            "properties": prop_obj,
                        }));
                    }
                    for rel in &path.relationships {
                        edges.push(serde_json::json!({
                            "source": format!("n-{}", rel.src.0),
                            "target": format!("n-{}", rel.dst.0),
                            "label": rel.rel_type,
                        }));
                    }
                }
                _ => {}
            }
        }
    }

    // Dedup nodes by id
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| {
        let id = n.get("id").and_then(|v| v.as_str()).unwrap_or("");
        seen.insert(id.to_string())
    });

    serde_json::json!({ "nodes": nodes, "edges": edges }).to_string()
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::json!(b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::String(s) => serde_json::json!(s),
        Value::List(l) => serde_json::json!(l.iter().map(value_to_json).collect::<Vec<_>>()),
        _ => serde_json::json!(format!("{:?}", v)),
    }
}
