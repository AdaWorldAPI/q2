//! Notebook query engine — routes Cypher through lance-graph DataFusion (hot path).
//!
//! Hot path: aiwar_graph.json → Arrow RecordBatches → lance-graph CypherQuery → DataFusion.
//! Cold path (optional): Neo4j Aura via neo4rs behind the `neo4j-fallback` feature.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use arrow::array::{ArrayRef, Float64Builder, Int64Builder, RecordBatch, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema};
use lance_graph::{CypherQuery, GraphConfig};
use serde::Deserialize;

// ── Public types (unchanged API surface) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryLanguage {
    Gremlin,
    Cypher,
    Sparql,
    R,
    Rust,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub language: QueryLanguage,
    pub raw_output: String,
    pub html: Option<String>,
    /// JSON with `{ "nodes": [...], "edges": [...] }` for graph queries.
    /// The frontend renders this with vis-network.
    pub graph_json: Option<String>,
    pub elapsed_ms: u64,
}

pub fn detect_language(source: &str) -> QueryLanguage {
    let trimmed = source.trim();
    if trimmed.starts_with("g.")
        || trimmed.contains(".hasLabel(")
        || trimmed.contains(".outE(")
        || trimmed.contains(".inV(")
    {
        QueryLanguage::Gremlin
    } else if trimmed.starts_with("MATCH (") || trimmed.starts_with("MATCH(") {
        QueryLanguage::Cypher
    } else if trimmed.starts_with("PREFIX ") || trimmed.starts_with("SELECT ?") {
        QueryLanguage::Sparql
    } else if trimmed.contains("%>%") || trimmed.contains("<-") || trimmed.starts_with("library(") {
        QueryLanguage::R
    } else if trimmed.contains("let ") || trimmed.contains("fn ") {
        QueryLanguage::Rust
    } else {
        QueryLanguage::Markdown
    }
}

// ── Execution entry point ──

pub fn execute(source: &str, language: QueryLanguage) -> Result<QueryResult, String> {
    match language {
        QueryLanguage::Cypher => execute_cypher(source),
        QueryLanguage::Gremlin | QueryLanguage::Sparql => {
            // Gremlin/SPARQL: stub for now, but still show the real aiwar graph
            let graph_json = aiwar_graph_json().ok();
            Ok(QueryResult {
                language,
                raw_output: format!("Executed {:?} query (stub): {}", language, source),
                html: Some(format!("<pre>{}</pre>", source)),
                graph_json,
                elapsed_ms: 0,
            })
        }
        QueryLanguage::R => Ok(QueryResult {
            language,
            raw_output: format!("R output for: {}", source),
            html: Some(demo_r_table()),
            graph_json: None,
            elapsed_ms: 120,
        }),
        _ => Ok(QueryResult {
            language,
            raw_output: format!("Stub execution of {:?} query", language),
            html: Some(format!("<pre>{}</pre>", source)),
            graph_json: None,
            elapsed_ms: 0,
        }),
    }
}

// ── Cypher hot path via lance-graph ──

fn execute_cypher(source: &str) -> Result<QueryResult, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let t0 = Instant::now();

    let result = rt.block_on(async {
        let (datasets, config) = load_aiwar_datasets()?;
        let query = CypherQuery::new(source)
            .map_err(|e| format!("Cypher parse error: {e}"))?
            .with_config(config.clone());
        let batch = query
            .execute(datasets.clone(), None)
            .await
            .map_err(|e| format!("lance-graph execution error: {e}"))?;
        Ok::<RecordBatch, String>(batch)
    })?;

    let elapsed_ms = t0.elapsed().as_millis() as u64;

    // Tabular output
    let raw_output = batch_to_text(&result);
    let html = Some(batch_to_html(&result));

    // Graph JSON from the full aiwar dataset
    let graph_json = aiwar_graph_json().ok();

    Ok(QueryResult {
        language: QueryLanguage::Cypher,
        raw_output,
        html,
        graph_json,
        elapsed_ms,
    })
}

// ── Neo4j cold path (feature-gated) ──

#[cfg(feature = "neo4j-fallback")]
pub async fn execute_cold(source: &str) -> Result<QueryResult, String> {
    let uri = std::env::var("NEO4J_URI").map_err(|e| format!("NEO4J_URI not set: {e}"))?;
    let password =
        std::env::var("NEO4J_PASSWORD").map_err(|e| format!("NEO4J_PASSWORD not set: {e}"))?;
    let graph = neo4rs::Graph::new(&uri, "neo4j", &password)
        .await
        .map_err(|e| format!("Neo4j connection error: {e}"))?;

    let t0 = Instant::now();
    let mut stream = graph
        .execute(neo4rs::query(source))
        .await
        .map_err(|e| format!("Neo4j query error: {e}"))?;

    let mut rows: Vec<String> = Vec::new();
    while let Ok(Some(row)) = stream.next().await {
        rows.push(format!("{:?}", row));
    }
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    Ok(QueryResult {
        language: QueryLanguage::Cypher,
        raw_output: rows.join("\n"),
        html: Some(format!("<pre>{}</pre>", rows.join("\n"))),
        graph_json: None,
        elapsed_ms,
    })
}

// ── aiwar JSON model ──

#[derive(Debug, Deserialize)]
struct AiWarGraphJson {
    #[serde(rename = "N_Systems", default)]
    systems: Vec<SystemJson>,
    #[serde(rename = "N_Stakeholders", default)]
    stakeholders: Vec<StakeholderJson>,
    #[serde(rename = "N_Civic", default)]
    civic: Vec<CivicJson>,
    #[serde(rename = "N_Historical", default)]
    historical: Vec<HistoricalJson>,
    #[serde(rename = "N_People", default)]
    people: Vec<PersonJson>,
    #[serde(rename = "E_isDevelopedBy", default)]
    edges_developed: Vec<EdgeJson>,
    #[serde(rename = "E_isDeployedBy", default)]
    edges_deployed: Vec<EdgeJson>,
    #[serde(rename = "E_connection", default)]
    edges_connection: Vec<EdgeJson>,
    #[serde(rename = "E_place", default)]
    edges_place: Vec<EdgeJson>,
    #[serde(rename = "E_people", default)]
    edges_people: Vec<EdgeJson>,
    #[serde(rename = "E_hierarchical", default)]
    meta_edges: Vec<MetaEdgeJson>,
}

#[derive(Debug, Deserialize)]
struct SystemJson {
    id: String,
    name: String,
    #[serde(default)]
    year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<String>,
    #[serde(rename = "type", default)]
    system_type: Option<String>,
    #[serde(rename = "MLTask", default)]
    ml_task: Option<String>,
    #[serde(rename = "militaryUse", default)]
    military_use: Option<String>,
    #[serde(rename = "civicUse", default)]
    civic_use: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    capacity: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    impact: Option<String>,
    #[serde(default)]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StakeholderJson {
    id: String,
    name: String,
    #[serde(rename = "type", default)]
    stakeholder_type: Option<String>,
    #[serde(rename = "airo:type", default)]
    airo_type: Option<String>,
    #[serde(default)]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CivicJson {
    id: String,
    name: String,
    #[serde(default)]
    year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<String>,
    #[serde(rename = "type", default)]
    system_type: Option<String>,
    #[serde(default)]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoricalJson {
    id: String,
    name: String,
    #[serde(default)]
    year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<String>,
    #[serde(rename = "type", default)]
    system_type: Option<String>,
    #[serde(rename = "militaryUse", default)]
    military_use: Option<String>,
    #[serde(rename = "civicUse", default)]
    civic_use: Option<String>,
    #[serde(default)]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PersonJson {
    id: String,
    name: String,
    #[serde(rename = "type", default)]
    person_type: Option<String>,
    #[serde(rename = "airo:type", default)]
    airo_type: Option<String>,
    #[serde(default)]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EdgeJson {
    source: String,
    target: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    weight: Option<f64>,
    #[serde(default)]
    hover: Option<String>,
    #[serde(default)]
    reference: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MetaEdgeJson {
    source: String,
    target: String,
}

// ── Data loading (cached in OnceLock) ──

type AiwarDatasets = Result<(HashMap<String, RecordBatch>, GraphConfig), String>;

static AIWAR_DATA: OnceLock<AiwarDatasets> = OnceLock::new();

fn load_aiwar_datasets() -> Result<&'static (HashMap<String, RecordBatch>, GraphConfig), String> {
    let result = AIWAR_DATA.get_or_init(|| {
        let path = std::env::var("AIWAR_DATA_PATH")
            .unwrap_or_else(|_| find_aiwar_json().unwrap_or_default());
        if path.is_empty() {
            return Err("Cannot find aiwar_graph.json — set AIWAR_DATA_PATH".to_string());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {path}: {e}"))?;
        let data: AiWarGraphJson =
            serde_json::from_str(&content).map_err(|e| format!("JSON parse error: {e}"))?;

        let mut datasets = HashMap::new();

        // Node tables
        datasets.insert("System".to_string(), systems_to_batch(&data.systems)?);
        datasets.insert(
            "Stakeholder".to_string(),
            stakeholders_to_batch(&data.stakeholders)?,
        );
        datasets.insert("Civic".to_string(), civic_to_batch(&data.civic)?);
        datasets.insert(
            "Historical".to_string(),
            historical_to_batch(&data.historical)?,
        );
        datasets.insert("Person".to_string(), people_to_batch(&data.people)?);

        // Edge tables
        datasets.insert(
            "CONNECTED_TO".to_string(),
            edges_to_batch(&data.edges_connection)?,
        );
        datasets.insert(
            "DEVELOPED_BY".to_string(),
            edges_to_batch(&data.edges_developed)?,
        );
        datasets.insert(
            "DEPLOYED_BY".to_string(),
            edges_to_batch(&data.edges_deployed)?,
        );
        datasets.insert("USED_IN".to_string(), edges_to_batch(&data.edges_place)?);
        datasets.insert(
            "PERSON_LINK".to_string(),
            edges_to_batch(&data.edges_people)?,
        );
        datasets.insert(
            "HIERARCHICAL".to_string(),
            meta_edges_to_batch(&data.meta_edges)?,
        );

        let config = aiwar_graph_config()?;

        Ok((datasets, config))
    });
    result.as_ref().map_err(|e| e.clone())
}

fn find_aiwar_json() -> Option<String> {
    // Search relative to the crate / workspace root
    let candidates = [
        "../aiwar-neo4j-harvest/data/aiwar_graph.json",
        "../../aiwar-neo4j-harvest/data/aiwar_graph.json",
        "../../../aiwar-neo4j-harvest/data/aiwar_graph.json",
        // Absolute fallback for the known dev layout
        "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    None
}

fn aiwar_graph_config() -> Result<GraphConfig, String> {
    GraphConfig::builder()
        // Nodes
        .with_node_label("System", "id")
        .with_node_label("Stakeholder", "id")
        .with_node_label("Civic", "id")
        .with_node_label("Historical", "id")
        .with_node_label("Person", "id")
        // Edges
        .with_relationship("CONNECTED_TO", "source", "target")
        .with_relationship("DEVELOPED_BY", "source", "target")
        .with_relationship("DEPLOYED_BY", "source", "target")
        .with_relationship("USED_IN", "source", "target")
        .with_relationship("PERSON_LINK", "source", "target")
        .with_relationship("HIERARCHICAL", "source", "target")
        .build()
        .map_err(|e| format!("GraphConfig error: {e}"))
}

// ── JSON → Arrow RecordBatch converters ──

fn systems_to_batch(systems: &[SystemJson]) -> Result<RecordBatch, String> {
    let len = systems.len();
    let mut id = StringBuilder::with_capacity(len, len * 16);
    let mut name = StringBuilder::with_capacity(len, len * 32);
    let mut year = Int64Builder::with_capacity(len);
    let mut current_status = StringBuilder::with_capacity(len, len * 16);
    let mut system_type = StringBuilder::with_capacity(len, len * 16);
    let mut ml_task = StringBuilder::with_capacity(len, len * 16);
    let mut military_use = StringBuilder::with_capacity(len, len * 16);
    let mut civic_use = StringBuilder::with_capacity(len, len * 16);
    let mut purpose = StringBuilder::with_capacity(len, len * 32);
    let mut capacity = StringBuilder::with_capacity(len, len * 16);
    let mut output = StringBuilder::with_capacity(len, len * 16);
    let mut impact = StringBuilder::with_capacity(len, len * 16);
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for s in systems {
        id.append_value(&s.id);
        name.append_value(&s.name);
        match s.year {
            Some(y) => year.append_value(y),
            None => year.append_null(),
        }
        append_opt(&mut current_status, &s.current_status);
        append_opt(&mut system_type, &s.system_type);
        append_opt(&mut ml_task, &s.ml_task);
        append_opt(&mut military_use, &s.military_use);
        append_opt(&mut civic_use, &s.civic_use);
        append_opt(&mut purpose, &s.purpose);
        append_opt(&mut capacity, &s.capacity);
        append_opt(&mut output, &s.output);
        append_opt(&mut impact, &s.impact);
        append_opt(&mut hover, &s.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("year", DataType::Int64, true),
        Field::new("currentstatus", DataType::Utf8, true),
        Field::new("type", DataType::Utf8, true),
        Field::new("mltask", DataType::Utf8, true),
        Field::new("militaryuse", DataType::Utf8, true),
        Field::new("civicuse", DataType::Utf8, true),
        Field::new("purpose", DataType::Utf8, true),
        Field::new("capacity", DataType::Utf8, true),
        Field::new("output", DataType::Utf8, true),
        Field::new("impact", DataType::Utf8, true),
        Field::new("hover", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(year.finish()),
            Arc::new(current_status.finish()),
            Arc::new(system_type.finish()),
            Arc::new(ml_task.finish()),
            Arc::new(military_use.finish()),
            Arc::new(civic_use.finish()),
            Arc::new(purpose.finish()),
            Arc::new(capacity.finish()),
            Arc::new(output.finish()),
            Arc::new(impact.finish()),
            Arc::new(hover.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (systems): {e}"))
}

fn stakeholders_to_batch(items: &[StakeholderJson]) -> Result<RecordBatch, String> {
    let len = items.len();
    let mut id = StringBuilder::with_capacity(len, len * 16);
    let mut name = StringBuilder::with_capacity(len, len * 32);
    let mut stype = StringBuilder::with_capacity(len, len * 16);
    let mut airo = StringBuilder::with_capacity(len, len * 16);
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for s in items {
        id.append_value(&s.id);
        name.append_value(&s.name);
        append_opt(&mut stype, &s.stakeholder_type);
        append_opt(&mut airo, &s.airo_type);
        append_opt(&mut hover, &s.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("type", DataType::Utf8, true),
        Field::new("airotype", DataType::Utf8, true),
        Field::new("hover", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(stype.finish()),
            Arc::new(airo.finish()),
            Arc::new(hover.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (stakeholders): {e}"))
}

fn civic_to_batch(items: &[CivicJson]) -> Result<RecordBatch, String> {
    let len = items.len();
    let mut id = StringBuilder::with_capacity(len, len * 16);
    let mut name = StringBuilder::with_capacity(len, len * 32);
    let mut year = Int64Builder::with_capacity(len);
    let mut current_status = StringBuilder::with_capacity(len, len * 16);
    let mut stype = StringBuilder::with_capacity(len, len * 16);
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for c in items {
        id.append_value(&c.id);
        name.append_value(&c.name);
        match c.year {
            Some(y) => year.append_value(y),
            None => year.append_null(),
        }
        append_opt(&mut current_status, &c.current_status);
        append_opt(&mut stype, &c.system_type);
        append_opt(&mut hover, &c.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("year", DataType::Int64, true),
        Field::new("currentstatus", DataType::Utf8, true),
        Field::new("type", DataType::Utf8, true),
        Field::new("hover", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(year.finish()),
            Arc::new(current_status.finish()),
            Arc::new(stype.finish()),
            Arc::new(hover.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (civic): {e}"))
}

fn historical_to_batch(items: &[HistoricalJson]) -> Result<RecordBatch, String> {
    let len = items.len();
    let mut id = StringBuilder::with_capacity(len, len * 16);
    let mut name = StringBuilder::with_capacity(len, len * 32);
    let mut year = Int64Builder::with_capacity(len);
    let mut current_status = StringBuilder::with_capacity(len, len * 16);
    let mut stype = StringBuilder::with_capacity(len, len * 16);
    let mut military_use = StringBuilder::with_capacity(len, len * 16);
    let mut civic_use = StringBuilder::with_capacity(len, len * 16);
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for h in items {
        id.append_value(&h.id);
        name.append_value(&h.name);
        match h.year {
            Some(y) => year.append_value(y),
            None => year.append_null(),
        }
        append_opt(&mut current_status, &h.current_status);
        append_opt(&mut stype, &h.system_type);
        append_opt(&mut military_use, &h.military_use);
        append_opt(&mut civic_use, &h.civic_use);
        append_opt(&mut hover, &h.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("year", DataType::Int64, true),
        Field::new("currentstatus", DataType::Utf8, true),
        Field::new("type", DataType::Utf8, true),
        Field::new("militaryuse", DataType::Utf8, true),
        Field::new("civicuse", DataType::Utf8, true),
        Field::new("hover", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(year.finish()),
            Arc::new(current_status.finish()),
            Arc::new(stype.finish()),
            Arc::new(military_use.finish()),
            Arc::new(civic_use.finish()),
            Arc::new(hover.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (historical): {e}"))
}

fn people_to_batch(items: &[PersonJson]) -> Result<RecordBatch, String> {
    let len = items.len();
    let mut id = StringBuilder::with_capacity(len, len * 16);
    let mut name = StringBuilder::with_capacity(len, len * 32);
    let mut ptype = StringBuilder::with_capacity(len, len * 16);
    let mut airo = StringBuilder::with_capacity(len, len * 16);
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for p in items {
        id.append_value(&p.id);
        name.append_value(&p.name);
        append_opt(&mut ptype, &p.person_type);
        append_opt(&mut airo, &p.airo_type);
        append_opt(&mut hover, &p.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("type", DataType::Utf8, true),
        Field::new("airotype", DataType::Utf8, true),
        Field::new("hover", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(ptype.finish()),
            Arc::new(airo.finish()),
            Arc::new(hover.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (people): {e}"))
}

fn edges_to_batch(edges: &[EdgeJson]) -> Result<RecordBatch, String> {
    let len = edges.len();
    let mut source = StringBuilder::with_capacity(len, len * 16);
    let mut target = StringBuilder::with_capacity(len, len * 16);
    let mut label = StringBuilder::with_capacity(len, len * 32);
    let mut weight = Float64Builder::with_capacity(len);
    let mut hover = StringBuilder::with_capacity(len, len * 64);
    let mut reference = StringBuilder::with_capacity(len, len * 64);

    for e in edges {
        source.append_value(&e.source);
        target.append_value(&e.target);
        append_opt(&mut label, &e.label);
        match e.weight {
            Some(w) => weight.append_value(w),
            None => weight.append_null(),
        }
        append_opt(&mut hover, &e.hover);
        append_opt(&mut reference, &e.reference);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("source", DataType::Utf8, false),
        Field::new("target", DataType::Utf8, false),
        Field::new("label", DataType::Utf8, true),
        Field::new("weight", DataType::Float64, true),
        Field::new("hover", DataType::Utf8, true),
        Field::new("reference", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(source.finish()) as ArrayRef,
            Arc::new(target.finish()),
            Arc::new(label.finish()),
            Arc::new(weight.finish()),
            Arc::new(hover.finish()),
            Arc::new(reference.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (edges): {e}"))
}

fn meta_edges_to_batch(edges: &[MetaEdgeJson]) -> Result<RecordBatch, String> {
    let len = edges.len();
    let mut source = StringBuilder::with_capacity(len, len * 16);
    let mut target = StringBuilder::with_capacity(len, len * 16);

    for e in edges {
        source.append_value(&e.source);
        target.append_value(&e.target);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("source", DataType::Utf8, false),
        Field::new("target", DataType::Utf8, false),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(source.finish()) as ArrayRef,
            Arc::new(target.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (meta_edges): {e}"))
}

fn append_opt(builder: &mut StringBuilder, val: &Option<String>) {
    match val {
        Some(v) => builder.append_value(v),
        None => builder.append_null(),
    }
}

// ── RecordBatch → cockpit JSON ──

/// Build the full aiwar graph in vis-network JSON format from the cached datasets.
fn aiwar_graph_json() -> Result<String, String> {
    let (datasets, _config) = load_aiwar_datasets()?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Collect nodes from each node table
    let node_tables = [
        ("System", "System"),
        ("Stakeholder", "Stakeholder"),
        ("Civic", "Civic"),
        ("Historical", "Historical"),
        ("Person", "Person"),
    ];

    for (key, node_type) in &node_tables {
        if let Some(batch) = datasets.get(*key) {
            let schema = batch.schema();
            let id_idx = schema.index_of("id").ok();
            let name_idx = schema.index_of("name").ok();

            for row in 0..batch.num_rows() {
                let id_val = id_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();
                let name_val = name_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();

                // Build properties from all columns
                let mut props = serde_json::Map::new();
                for (col_idx, field) in schema.fields().iter().enumerate() {
                    if let Some(val) = get_json_value(batch, col_idx, row) {
                        props.insert(field.name().clone(), val);
                    }
                }

                nodes.push(serde_json::json!({
                    "id": id_val,
                    "label": name_val,
                    "type": node_type,
                    "properties": props,
                }));
            }
        }
    }

    // Collect edges from each edge table
    let edge_tables = [
        "CONNECTED_TO",
        "DEVELOPED_BY",
        "DEPLOYED_BY",
        "USED_IN",
        "PERSON_LINK",
        "HIERARCHICAL",
    ];

    for rel_type in &edge_tables {
        if let Some(batch) = datasets.get(*rel_type) {
            let schema = batch.schema();
            let src_idx = schema.index_of("source").ok();
            let tgt_idx = schema.index_of("target").ok();

            for row in 0..batch.num_rows() {
                let src = src_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();
                let tgt = tgt_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();

                edges.push(serde_json::json!({
                    "source": src,
                    "target": tgt,
                    "label": rel_type,
                }));
            }
        }
    }

    Ok(serde_json::json!({ "nodes": nodes, "edges": edges }).to_string())
}

// ── RecordBatch → text/HTML helpers ──

fn batch_to_text(batch: &RecordBatch) -> String {
    if batch.num_rows() == 0 {
        return "(empty result)".to_string();
    }
    // Use arrow's pretty-print
    let mut buf = Vec::new();
    if arrow::util::pretty::pretty_format_batches(&[batch.clone()])
        .map(|table| buf.extend_from_slice(table.to_string().as_bytes()))
        .is_ok()
    {
        String::from_utf8_lossy(&buf).to_string()
    } else {
        format!("{} rows, {} columns", batch.num_rows(), batch.num_columns())
    }
}

fn batch_to_html(batch: &RecordBatch) -> String {
    let schema = batch.schema();
    let mut html = String::from("<table class=\"mini-table\"><thead><tr>");
    for field in schema.fields() {
        html.push_str(&format!("<th>{}</th>", field.name()));
    }
    html.push_str("</tr></thead><tbody>");

    for row in 0..batch.num_rows() {
        html.push_str("<tr>");
        for col in 0..batch.num_columns() {
            let val = get_string_value(batch, col, row).unwrap_or_default();
            html.push_str(&format!("<td>{}</td>", html_escape(&val)));
        }
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table>");
    html
}

fn get_string_value(batch: &RecordBatch, col: usize, row: usize) -> Option<String> {
    use arrow::array::{Float64Array, Int64Array, StringArray};
    let col_data = batch.column(col);
    if col_data.is_null(row) {
        return None;
    }
    match col_data.data_type() {
        DataType::Utf8 => {
            let arr = col_data.as_any().downcast_ref::<StringArray>()?;
            Some(arr.value(row).to_string())
        }
        DataType::Int64 => {
            let arr = col_data.as_any().downcast_ref::<Int64Array>()?;
            Some(arr.value(row).to_string())
        }
        DataType::Float64 => {
            let arr = col_data.as_any().downcast_ref::<Float64Array>()?;
            Some(arr.value(row).to_string())
        }
        _ => Some(format!("{:?}", col_data.as_ref())),
    }
}

fn get_json_value(batch: &RecordBatch, col: usize, row: usize) -> Option<serde_json::Value> {
    use arrow::array::{Float64Array, Int64Array, StringArray};
    let col_data = batch.column(col);
    if col_data.is_null(row) {
        return None;
    }
    match col_data.data_type() {
        DataType::Utf8 => {
            let arr = col_data.as_any().downcast_ref::<StringArray>()?;
            Some(serde_json::Value::String(arr.value(row).to_string()))
        }
        DataType::Int64 => {
            let arr = col_data.as_any().downcast_ref::<Int64Array>()?;
            Some(serde_json::json!(arr.value(row)))
        }
        DataType::Float64 => {
            let arr = col_data.as_any().downcast_ref::<Float64Array>()?;
            Some(serde_json::json!(arr.value(row)))
        }
        _ => None,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Demo R table output
fn demo_r_table() -> String {
    r#"<table class="mini-table">
<tr><td>web-server-01</td><td>0.67</td><td>28.4 GB</td></tr>
<tr><td>web-server-02</td><td>0.54</td><td>24.1 GB</td></tr>
<tr><td>web-server-03</td><td>0.42</td><td>31.2 GB</td></tr>
<tr><td>web-server-04</td><td>0.81</td><td>29.8 GB</td></tr>
</table>"#
        .to_string()
}
