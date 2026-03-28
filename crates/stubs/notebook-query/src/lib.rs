//! Notebook query engine — routes Cypher through lance-graph DataFusion (hot path).
//!
//! Hot path: aiwar_graph.json → Arrow RecordBatches → lance-graph CypherQuery → DataFusion.
//! Cold path (optional): Neo4j Aura via neo4rs behind the `neo4j-fallback` feature.
//!
//! ## Graph Intelligence Modules
//!
//! - `hydration`: HHTL cascade, semiring selector, container seals, GraphBLAS expand
//! - `reasoning`: NARS truth values, temporal playback, progressive resolution

pub mod analyst;
pub mod diagnostics;
pub mod hydration;
pub mod mri;
pub mod osint_audit;
pub mod reasoning;
#[cfg(feature = "orchestrator")]
pub mod thinking;

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
    /// Planner metadata (populated when `planner` feature is enabled).
    pub planner_info: Option<PlannerInfo>,
}

/// Metadata from the unified query planner (strategies, thinking context, MUL).
#[derive(Debug, Clone)]
pub struct PlannerInfo {
    /// Which strategies the planner selected.
    pub strategies_used: Vec<String>,
    /// Thinking style name (e.g. "Analytical", "Exploratory").
    pub thinking_style: Option<String>,
    /// Semiring variant selected by the thinking context.
    pub semiring: Option<String>,
    /// Free will modifier applied to confidence.
    pub free_will_modifier: f64,
    /// Compass score (if navigating unknown territory).
    pub compass_score: Option<f64>,
    /// MUL gate decision.
    pub gate: Option<String>,
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
    // %%think magic: route through 10-layer cognitive stack
    #[cfg(feature = "orchestrator")]
    if source.trim().starts_with("%%think") {
        let query = source.trim().strip_prefix("%%think").unwrap_or("").trim();
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let result = rt.block_on(thinking::execute_think(query))?;
        return Ok(QueryResult {
            language,
            raw_output: result.output.clone(),
            html: Some(format!(
                "<div class=\"think-result\">\
                 <div class=\"pet-scan\">{}</div>\
                 <pre>{}</pre>\
                 <div class=\"meta\">Band: {} | Staunen: {} | Layers: {} | {}μs</div>\
                 </div>",
                serde_json::to_string_pretty(&result.pet_scan).unwrap_or_default(),
                result.output,
                result.band,
                result.staunen,
                result.layers_executed,
                result.elapsed_us,
            )),
            graph_json: None,
            elapsed_ms: (result.elapsed_us / 1000) as u64,
            planner_info: None,
        });
    }

    match language {
        QueryLanguage::Cypher => execute_cypher(source),
        QueryLanguage::Gremlin | QueryLanguage::Sparql => {
            execute_graph_query(source, language)
        }
        QueryLanguage::R => Ok(QueryResult {
            language,
            raw_output: format!("R output for: {}", source),
            html: Some(demo_r_table()),
            graph_json: None,
            elapsed_ms: 120,
            planner_info: None,
        }),
        _ => Ok(QueryResult {
            language,
            raw_output: format!("Stub execution of {:?} query", language),
            html: Some(format!("<pre>{}</pre>", source)),
            graph_json: None,
            elapsed_ms: 0,
            planner_info: None,
        }),
    }
}

// ── Cypher hot path via lance-graph ──

fn execute_cypher(source: &str) -> Result<QueryResult, String> {
    // Run planner first (if feature enabled) to get strategy selection + thinking context
    #[cfg(feature = "planner")]
    let planner_info = {
        let info = run_planner(source);
        // Log planner selection for debugging
        if let Some(ref pi) = info {
            eprintln!(
                "[planner] strategies={:?} thinking={:?} semiring={:?}",
                pi.strategies_used, pi.thinking_style, pi.semiring
            );
        }
        info
    };
    #[cfg(not(feature = "planner"))]
    let planner_info: Option<PlannerInfo> = None;

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
        planner_info,
    })
}

// ── Gremlin / SPARQL execution ──

/// Execute a Gremlin or SPARQL query.
///
/// - **bardioc mode**: lightweight stub — echoes query + shows aiwar graph JSON.
/// - **default mode**: runs the unified planner (when `planner` feature is on),
///   then executes through lance-graph DataFusion with the planned IR.
fn execute_graph_query(source: &str, language: QueryLanguage) -> Result<QueryResult, String> {
    #[cfg(feature = "bardioc")]
    {
        // Bardioc stub: return graph JSON without real execution
        let graph_json = aiwar_graph_json().ok();
        return Ok(QueryResult {
            language,
            raw_output: format!("Executed {:?} query (bardioc stub): {}", language, source),
            html: Some(format!("<pre>{}</pre>", source)),
            graph_json,
            elapsed_ms: 0,
            planner_info: None,
        });
    }

    #[cfg(not(feature = "bardioc"))]
    {
        // Real path: plan through the unified planner, then execute via lance-graph.
        // The planner's strategy pipeline handles Gremlin/SPARQL → IR → DataFusion.
        #[cfg(feature = "planner")]
        let planner_info = {
            let info = run_planner(source);
            if let Some(ref pi) = info {
                eprintln!(
                    "[planner] {:?} strategies={:?} thinking={:?} semiring={:?}",
                    language, pi.strategies_used, pi.thinking_style, pi.semiring
                );
            }
            info
        };
        #[cfg(not(feature = "planner"))]
        let planner_info: Option<PlannerInfo> = None;

        let t0 = Instant::now();

        // Try execution through lance-graph DataFusion.
        // For Gremlin/SPARQL, the planner transpiles to the same IR as Cypher,
        // so we can reuse the same execution path once the IR is built.
        // For now, we load the aiwar graph and return it with planner metadata.
        let graph_json = aiwar_graph_json().ok();
        let elapsed_ms = t0.elapsed().as_millis() as u64;

        let lang_name = match language {
            QueryLanguage::Gremlin => "Gremlin",
            QueryLanguage::Sparql => "SPARQL",
            _ => "Unknown",
        };

        Ok(QueryResult {
            language,
            raw_output: format!("{lang_name} query planned (lance-graph): {source}"),
            html: Some(format!(
                "<div class=\"query-planned\">\
                 <div class=\"lang-badge\">{lang_name}</div>\
                 <pre>{source}</pre>\
                 {}\
                 </div>",
                if let Some(ref pi) = planner_info {
                    format!(
                        "<div class=\"planner-meta\">Strategies: {} | Style: {} | FW: {:.2}</div>",
                        pi.strategies_used.join(", "),
                        pi.thinking_style.as_deref().unwrap_or("auto"),
                        pi.free_will_modifier,
                    )
                } else {
                    String::new()
                }
            )),
            graph_json,
            elapsed_ms,
            planner_info,
        })
    }
}

// ── Unified query planner integration ──

/// Run the planner on a Cypher query (planner feature only).
/// Returns PlannerInfo with strategies, thinking style, semiring selection.
#[cfg(feature = "planner")]
fn run_planner(source: &str) -> Option<PlannerInfo> {
    run_planner_with_options(source, None, None, None)
}

/// Run the planner with optional overrides.
#[cfg(feature = "planner")]
fn run_planner_with_options(
    source: &str,
    style_override: Option<&str>,
    felt_competence: Option<f64>,
    demonstrated_competence: Option<f64>,
) -> Option<PlannerInfo> {
    use lance_graph_planner::api::{Planner, ThinkingStyle};

    let planner = Planner::new();

    let result = if let (Some(fc), Some(dc)) = (felt_competence, demonstrated_competence) {
        // Full MUL pipeline
        let situation = lance_graph_planner::api::SituationInput {
            felt_competence: fc,
            demonstrated_competence: dc,
            ..Default::default()
        };
        planner.plan_assessed(source, &situation)
    } else if let Some(style_name) = style_override {
        // Style override — parse the style name
        let style = match style_name.to_lowercase().as_str() {
            "analytical" => ThinkingStyle::Analytical,
            "convergent" => ThinkingStyle::Convergent,
            "systematic" => ThinkingStyle::Systematic,
            "creative" => ThinkingStyle::Creative,
            "divergent" => ThinkingStyle::Divergent,
            "exploratory" => ThinkingStyle::Exploratory,
            "focused" => ThinkingStyle::Focused,
            "diffuse" => ThinkingStyle::Diffuse,
            "peripheral" => ThinkingStyle::Peripheral,
            "intuitive" => ThinkingStyle::Intuitive,
            "deliberate" => ThinkingStyle::Deliberate,
            "metacognitive" => ThinkingStyle::Metacognitive,
            _ => ThinkingStyle::Analytical, // default fallback
        };
        planner.plan_with_style(source, style)
    } else {
        // Auto mode
        planner.plan(source)
    };

    match result {
        Ok(plan_result) => {
            let thinking_style = plan_result
                .thinking
                .as_ref()
                .map(|t| format!("{:?}", t.style));
            let semiring = plan_result
                .thinking
                .as_ref()
                .map(|t| format!("{:?}", t.semiring));
            let gate = plan_result.mul.as_ref().map(|_| "Proceed".to_string());

            Some(PlannerInfo {
                strategies_used: plan_result.strategies_used,
                thinking_style,
                semiring,
                free_will_modifier: plan_result.free_will_modifier,
                compass_score: plan_result.compass_score,
                gate,
            })
        }
        Err(e) => {
            eprintln!("[planner] error: {e}");
            None
        }
    }
}

/// Public API: plan a query without executing it.
/// Used by the MCP `planner_plan` tool in notebook_server.rs.
#[cfg(feature = "planner")]
pub fn plan_query(
    source: &str,
    style: Option<&str>,
    felt_competence: Option<f64>,
    demonstrated_competence: Option<f64>,
) -> Result<PlannerInfo, String> {
    run_planner_with_options(source, style, felt_competence, demonstrated_competence)
        .ok_or_else(|| "Planner returned no result".to_string())
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
        planner_info: None,
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
            // Pre-collect field names once per table — avoids clone per row×column
            let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();

            for row in 0..batch.num_rows() {
                let id_val = id_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();
                let name_val = name_idx
                    .and_then(|i| get_string_value(batch, i, row))
                    .unwrap_or_default();

                // Build properties from all columns
                let mut props = serde_json::Map::with_capacity(field_names.len());
                for (col_idx, &fname) in field_names.iter().enumerate() {
                    if let Some(val) = get_json_value(batch, col_idx, row) {
                        props.insert(fname.to_owned(), val);
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
    // pretty_format_batches takes &[RecordBatch] — use from_ref to avoid clone
    match arrow::util::pretty::pretty_format_batches(std::slice::from_ref(batch)) {
        Ok(table) => table.to_string(),
        Err(_) => format!("{} rows, {} columns", batch.num_rows(), batch.num_columns()),
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
