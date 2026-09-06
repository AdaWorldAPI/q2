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
pub mod gremlin;
pub mod hydration;
pub mod mri;
pub mod ontology;
pub mod orchestrator;
pub mod osint_audit;
pub mod reasoning;
#[cfg(feature = "orchestrator")]
pub mod thinking;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use arrow::array::{
    ArrayRef, Float64Builder, Int64Builder, RecordBatch, StringBuilder, StringDictionaryBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, UInt16Type};
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
        let result = block_on_sync(thinking::execute_think(query))??;
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
        QueryLanguage::Gremlin | QueryLanguage::Sparql => execute_graph_query(source, language),
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

/// Drive a future to completion from a **synchronous** entry point that may (or
/// may not) already be running inside a Tokio runtime.
///
/// `execute` is a synchronous API, but it is reached from the cockpit server's
/// Axum request handlers, which run on Tokio worker threads. The previous
/// `Runtime::new().block_on(..)` path panicked there with "Cannot start a
/// runtime from within a runtime" — a thread that is already driving async
/// tasks cannot spin up a second runtime and block on it. When an ambient
/// runtime is present we reuse it via `block_in_place` (which parks the current
/// worker so the scheduler keeps making progress on other tasks); otherwise we
/// build a private runtime exactly as before.
///
/// The ambient path assumes a multi-thread runtime, which is what every caller
/// uses (`#[tokio::main]` defaults to multi-thread, as does `Runtime::new()`).
fn block_on_sync<F: std::future::Future>(fut: F) -> Result<F::Output, String> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => Ok(tokio::task::block_in_place(move || handle.block_on(fut))),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            Ok(rt.block_on(fut))
        }
    }
}

/// Execute a Cypher query against the aiwar Arrow datasets via lance-graph's
/// real DataFusion path. Shared by the Cypher cell and the Gremlin transpiler.
fn run_cypher_on_aiwar(source: &str) -> Result<RecordBatch, String> {
    block_on_sync(async {
        let (datasets, config) = load_aiwar_datasets()?;
        let query = CypherQuery::new(source)
            .map_err(|e| format!("Cypher parse error: {e}"))?
            .with_config(config.clone());
        query
            .execute(datasets.clone(), None)
            .await
            .map_err(|e| format!("lance-graph execution error: {e}"))
    })?
}

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

    let t0 = Instant::now();
    let result = run_cypher_on_aiwar(source)?;
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

        let lang_name = match language {
            QueryLanguage::Gremlin => "Gremlin",
            QueryLanguage::Sparql => "SPARQL",
            _ => "Unknown",
        };

        // Real execution: transpile the supported Gremlin subset to Cypher and
        // run it through lance-graph's DataFusion path. On any miss (unsupported
        // shape or execution error) fall through to the graph echo below — the
        // demo never breaks, and supported traversals return real rows.
        if language == QueryLanguage::Gremlin
            && let Some(cypher) = gremlin::gremlin_to_cypher(source)
        {
            match run_cypher_on_aiwar(&cypher) {
                Ok(batch) => {
                    let elapsed_ms = t0.elapsed().as_millis() as u64;
                    return Ok(QueryResult {
                        language,
                        raw_output: format!(
                            "{lang_name} → Cypher: {cypher}\n\n{}",
                            batch_to_text(&batch)
                        ),
                        html: Some(format!(
                            "<div class=\"query-executed\">\
                             <div class=\"lang-badge\">{lang_name} → Cypher</div>\
                             <pre>{cypher}</pre>{}</div>",
                            batch_to_html(&batch)
                        )),
                        graph_json: aiwar_graph_json().ok(),
                        elapsed_ms,
                        planner_info,
                    });
                }
                Err(e) => {
                    eprintln!(
                        "[gremlin] transpiled `{cypher}` failed: {e} — falling back to graph echo"
                    );
                }
            }
        }

        // Fallback: graph echo + planner metadata (unchanged behavior).
        let graph_json = aiwar_graph_json().ok();
        let elapsed_ms = t0.elapsed().as_millis() as u64;

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
    use lance_graph_planner::api::Planner;
    // The 12-family orchestration space is `StyleFamily`; `ThinkingStyle` is now a
    // deprecated alias for it. This block only constructs variants, so the extension
    // trait `PlannerStyleExt` is not needed here.
    use lance_graph_planner::thinking::style::StyleFamily;

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
            "analytical" => StyleFamily::Analytical,
            "convergent" => StyleFamily::Convergent,
            "systematic" => StyleFamily::Systematic,
            "creative" => StyleFamily::Creative,
            "divergent" => StyleFamily::Divergent,
            "exploratory" => StyleFamily::Exploratory,
            "focused" => StyleFamily::Focused,
            "diffuse" => StyleFamily::Diffuse,
            "peripheral" => StyleFamily::Peripheral,
            "intuitive" => StyleFamily::Intuitive,
            "deliberate" => StyleFamily::Deliberate,
            "metacognitive" => StyleFamily::Metacognitive,
            _ => StyleFamily::Analytical, // default fallback
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

/// Lenient deserializer for the aiwar `year` field: usually an integer, but a
/// couple of records carry `"n.d."` (no-date) as a string. Non-numeric → None,
/// so the typed Arrow loader (and `aiwar_graph_json`) don't choke on real data.
fn de_lenient_year<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => Ok(n.as_i64()),
        serde_json::Value::String(s) => Ok(s.trim().parse::<i64>().ok()),
        _ => Ok(None),
    }
}

/// Lenient deserializer for optional string fields that the aiwar data sometimes
/// encodes as a number (e.g. edge `hover` is `1` in ~95 records) or bool.
/// Numbers/bools are stringified; null → None.
fn de_lenient_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

#[derive(Debug, Deserialize)]
struct SystemJson {
    id: String,
    name: String,
    #[serde(default, deserialize_with = "de_lenient_year")]
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
    #[serde(default, deserialize_with = "de_lenient_string")]
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
    #[serde(default, deserialize_with = "de_lenient_string")]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CivicJson {
    id: String,
    name: String,
    #[serde(default, deserialize_with = "de_lenient_year")]
    year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<String>,
    #[serde(rename = "type", default)]
    system_type: Option<String>,
    #[serde(default, deserialize_with = "de_lenient_string")]
    hover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HistoricalJson {
    id: String,
    name: String,
    #[serde(default, deserialize_with = "de_lenient_year")]
    year: Option<i64>,
    #[serde(rename = "currentStatus", default)]
    current_status: Option<String>,
    #[serde(rename = "type", default)]
    system_type: Option<String>,
    #[serde(rename = "militaryUse", default)]
    military_use: Option<String>,
    #[serde(rename = "civicUse", default)]
    civic_use: Option<String>,
    #[serde(default, deserialize_with = "de_lenient_string")]
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
    #[serde(default, deserialize_with = "de_lenient_string")]
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
    #[serde(default, deserialize_with = "de_lenient_string")]
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
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {path}: {e}"))?;
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
        // Inheritance root: every node IS an Entity, so untyped traversal
        // targets bind to a table (fixes multi-hop "No field named n1__id").
        datasets.insert("Entity".to_string(), entity_to_batch(&data)?);
        // Relationship inheritance root: every edge IS an Edge, so untyped
        // out()/outE() traversals bind to a table.
        datasets.insert("Edge".to_string(), all_edges_to_batch(&data)?);

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
        // Inheritance root — every subtype also resolves as Entity, the
        // one-to-many / many-to-one join target for traversals.
        .with_node_label("Entity", "id")
        // Edges
        .with_relationship("CONNECTED_TO", "source", "target")
        .with_relationship("DEVELOPED_BY", "source", "target")
        .with_relationship("DEPLOYED_BY", "source", "target")
        .with_relationship("USED_IN", "source", "target")
        .with_relationship("PERSON_LINK", "source", "target")
        .with_relationship("HIERARCHICAL", "source", "target")
        // Relationship inheritance root — untyped traversals resolve as Edge.
        .with_relationship("Edge", "source", "target")
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
    let mut system_type = StringDictionaryBuilder::<UInt16Type>::new();
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
        append_opt_dict(&mut system_type, &s.system_type);
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
        Field::new(
            "type",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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
    let mut stype = StringDictionaryBuilder::<UInt16Type>::new();
    let mut airo = StringDictionaryBuilder::<UInt16Type>::new();
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for s in items {
        id.append_value(&s.id);
        name.append_value(&s.name);
        append_opt_dict(&mut stype, &s.stakeholder_type);
        append_opt_dict(&mut airo, &s.airo_type);
        append_opt(&mut hover, &s.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new(
            "type",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
        Field::new(
            "airotype",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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
    let mut stype = StringDictionaryBuilder::<UInt16Type>::new();
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for c in items {
        id.append_value(&c.id);
        name.append_value(&c.name);
        match c.year {
            Some(y) => year.append_value(y),
            None => year.append_null(),
        }
        append_opt(&mut current_status, &c.current_status);
        append_opt_dict(&mut stype, &c.system_type);
        append_opt(&mut hover, &c.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("year", DataType::Int64, true),
        Field::new("currentstatus", DataType::Utf8, true),
        Field::new(
            "type",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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
    let mut stype = StringDictionaryBuilder::<UInt16Type>::new();
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
        append_opt_dict(&mut stype, &h.system_type);
        append_opt(&mut military_use, &h.military_use);
        append_opt(&mut civic_use, &h.civic_use);
        append_opt(&mut hover, &h.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("year", DataType::Int64, true),
        Field::new("currentstatus", DataType::Utf8, true),
        Field::new(
            "type",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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
    let mut ptype = StringDictionaryBuilder::<UInt16Type>::new();
    let mut airo = StringDictionaryBuilder::<UInt16Type>::new();
    let mut hover = StringBuilder::with_capacity(len, len * 64);

    for p in items {
        id.append_value(&p.id);
        name.append_value(&p.name);
        append_opt_dict(&mut ptype, &p.person_type);
        append_opt_dict(&mut airo, &p.airo_type);
        append_opt(&mut hover, &p.hover);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new(
            "type",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
        Field::new(
            "airotype",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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

/// The inheritance root. Every aiwar node IS an `Entity`: one union table of
/// `(id, name, label)` where `label` is the subtype codebook (System /
/// Stakeholder / Civic / Historical / Person). This is what "use inherit" buys
/// us — a base every node resolves to, so an untyped traversal target `(b)` can
/// bind to a table. Without it, lance-graph's planner can't project the target
/// of a multi-hop pattern ("No field named n1__id"). Subtype tables remain for
/// specific `hasLabel(System)` scans; `Entity` is the one-to-many / many-to-one
/// join target.
fn entity_to_batch(data: &AiWarGraphJson) -> Result<RecordBatch, String> {
    let total = data.systems.len()
        + data.stakeholders.len()
        + data.civic.len()
        + data.historical.len()
        + data.people.len();
    let mut id = StringBuilder::with_capacity(total, total * 16);
    let mut name = StringBuilder::with_capacity(total, total * 32);
    // `entity_type` carries the canonical EntityTypeId — the contract's
    // BindSpace Column-H u16, resolved once per type from the shared aiwar
    // ontology. NOT a per-column Arrow dictionary (e43630f's duplicate u16).
    let mut entity_type = arrow::array::UInt16Builder::with_capacity(total);

    let sys_t = crate::ontology::label_type_id("System");
    let stk_t = crate::ontology::label_type_id("Stakeholder");
    let civ_t = crate::ontology::label_type_id("Civic");
    let his_t = crate::ontology::label_type_id("Historical");
    let per_t = crate::ontology::label_type_id("Person");

    for s in &data.systems {
        id.append_value(&s.id);
        name.append_value(&s.name);
        entity_type.append_value(sys_t);
    }
    for s in &data.stakeholders {
        id.append_value(&s.id);
        name.append_value(&s.name);
        entity_type.append_value(stk_t);
    }
    for s in &data.civic {
        id.append_value(&s.id);
        name.append_value(&s.name);
        entity_type.append_value(civ_t);
    }
    for s in &data.historical {
        id.append_value(&s.id);
        name.append_value(&s.name);
        entity_type.append_value(his_t);
    }
    for s in &data.people {
        id.append_value(&s.id);
        name.append_value(&s.name);
        entity_type.append_value(per_t);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        // The canonical EntityTypeId codebook column (Foundry Column-H).
        Field::new("entity_type", DataType::UInt16, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(id.finish()) as ArrayRef,
            Arc::new(name.finish()),
            Arc::new(entity_type.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (entity): {e}"))
}

/// The relationship inheritance root. Every edge IS an `Edge`: one union table
/// of `(source, target, reltype)` across all six relation types. Lets an untyped
/// Gremlin `out()` / `outE()` ("any outgoing edge") bind to a table — without it
/// an untyped relationship fails planning the same way an untyped node does.
fn all_edges_to_batch(data: &AiWarGraphJson) -> Result<RecordBatch, String> {
    let total = data.edges_connection.len()
        + data.edges_developed.len()
        + data.edges_deployed.len()
        + data.edges_place.len()
        + data.edges_people.len()
        + data.meta_edges.len();
    let mut source = StringBuilder::with_capacity(total, total * 16);
    let mut target = StringBuilder::with_capacity(total, total * 16);
    // `reltype` is the canonical link-type codebook (the relationship analogue
    // of the `entity_type` Column-H): a `u16` from `ontology::rel_type_id`, NOT
    // a per-column Arrow dictionary. Same id means the same rel type everywhere.
    let mut reltype = arrow::array::UInt16Builder::with_capacity(total);

    for (edges, label) in [
        (&data.edges_connection, "CONNECTED_TO"),
        (&data.edges_developed, "DEVELOPED_BY"),
        (&data.edges_deployed, "DEPLOYED_BY"),
        (&data.edges_place, "USED_IN"),
        (&data.edges_people, "PERSON_LINK"),
    ] {
        let rt = crate::ontology::rel_type_id(label);
        for e in edges {
            source.append_value(&e.source);
            target.append_value(&e.target);
            reltype.append_value(rt);
        }
    }
    let hier = crate::ontology::rel_type_id("HIERARCHICAL");
    for e in &data.meta_edges {
        source.append_value(&e.source);
        target.append_value(&e.target);
        reltype.append_value(hier);
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("source", DataType::Utf8, false),
        Field::new("target", DataType::Utf8, false),
        // The canonical link-type codebook column (relationship Column-H).
        Field::new("reltype", DataType::UInt16, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(source.finish()) as ArrayRef,
            Arc::new(target.finish()),
            Arc::new(reltype.finish()),
        ],
    )
    .map_err(|e| format!("Arrow error (all_edges): {e}"))
}

fn edges_to_batch(edges: &[EdgeJson]) -> Result<RecordBatch, String> {
    let len = edges.len();
    let mut source = StringBuilder::with_capacity(len, len * 16);
    let mut target = StringBuilder::with_capacity(len, len * 16);
    let mut label = StringDictionaryBuilder::<UInt16Type>::new();
    let mut weight = Float64Builder::with_capacity(len);
    let mut hover = StringBuilder::with_capacity(len, len * 64);
    let mut reference = StringBuilder::with_capacity(len, len * 64);

    for e in edges {
        source.append_value(&e.source);
        target.append_value(&e.target);
        append_opt_dict(&mut label, &e.label);
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
        Field::new(
            "label",
            DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8)),
            true,
        ),
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

/// Append into a dictionary (codebook) column: distinct strings are interned
/// once into the dictionary, and each row stores a compact u16 index. This is
/// the "labels → codebook + binary index table" pattern via Arrow's native
/// dictionary encoding (the same idea as classid / Base17 / CAM-PQ). Robust to
/// loosely-typed input since values arrive already coerced to `Option<String>`.
fn append_opt_dict(builder: &mut StringDictionaryBuilder<UInt16Type>, val: &Option<String>) {
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
            let field_names: Vec<&str> =
                schema.fields().iter().map(|f| f.name().as_str()).collect();

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
    use arrow::array::{DictionaryArray, Float64Array, Int64Array, StringArray, UInt16Array};
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
        // Canonical codebook id (entity_type / reltype) — render the raw u16.
        DataType::UInt16 => {
            let arr = col_data.as_any().downcast_ref::<UInt16Array>()?;
            Some(arr.value(row).to_string())
        }
        DataType::Dictionary(_, _) => {
            // codebook column: resolve the u16 index back to its string value.
            let dict = col_data
                .as_any()
                .downcast_ref::<DictionaryArray<UInt16Type>>()?;
            let values = dict.values().as_any().downcast_ref::<StringArray>()?;
            let key = dict.keys().value(row) as usize;
            Some(values.value(key).to_string())
        }
        _ => Some(format!("{:?}", col_data.as_ref())),
    }
}

fn get_json_value(batch: &RecordBatch, col: usize, row: usize) -> Option<serde_json::Value> {
    use arrow::array::{DictionaryArray, Float64Array, Int64Array, StringArray, UInt16Array};
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
        // Canonical codebook id (entity_type / reltype) — emit the raw u16.
        DataType::UInt16 => {
            let arr = col_data.as_any().downcast_ref::<UInt16Array>()?;
            Some(serde_json::json!(arr.value(row)))
        }
        DataType::Dictionary(_, _) => {
            // codebook column: resolve the u16 index back to its string value.
            let dict = col_data
                .as_any()
                .downcast_ref::<DictionaryArray<UInt16Type>>()?;
            let values = dict.values().as_any().downcast_ref::<StringArray>()?;
            let key = dict.keys().value(row) as usize;
            Some(serde_json::Value::String(values.value(key).to_string()))
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

// ── Public API: extract real edges from loaded graph for NARS inference ──

/// Extract all edges from the loaded aiwar graph as TruthEdge values.
///
/// This replaces the 2 hardcoded demo edges with the REAL graph data.
/// Each edge gets a default truth value based on its relationship type:
/// - DEVELOPED_BY, DEPLOYED_BY: high confidence (verified relationships)
/// - CONNECTED_TO: moderate confidence (co-occurrence based)
/// - HIERARCHICAL: lower confidence (structural, possibly outdated)
pub fn extract_graph_truth_edges() -> Result<Vec<reasoning::TruthEdge>, String> {
    use arrow::array::{Array, StringArray};

    let (datasets, _config) = load_aiwar_datasets()?;
    let mut edges = Vec::new();

    let edge_tables = [
        ("CONNECTED_TO", 0.75, 0.60),
        ("DEVELOPED_BY", 0.90, 0.85),
        ("DEPLOYED_BY", 0.88, 0.80),
        ("USED_IN", 0.80, 0.70),
        ("PERSON_LINK", 0.70, 0.55),
        ("HIERARCHICAL", 0.65, 0.50),
    ];

    for (rel_type, default_freq, default_conf) in &edge_tables {
        if let Some(batch) = datasets.get(*rel_type) {
            let schema = batch.schema();
            let src_idx = schema.index_of("source").ok();
            let tgt_idx = schema.index_of("target").ok();

            if let (Some(si), Some(ti)) = (src_idx, tgt_idx) {
                for row in 0..batch.num_rows() {
                    let src = batch
                        .column(si)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .and_then(|a| {
                            if a.is_null(row) {
                                None
                            } else {
                                Some(a.value(row).to_string())
                            }
                        });
                    let tgt = batch
                        .column(ti)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .and_then(|a| {
                            if a.is_null(row) {
                                None
                            } else {
                                Some(a.value(row).to_string())
                            }
                        });

                    if let (Some(source), Some(target)) = (src, tgt) {
                        // Check for weight column
                        let (freq, conf) = if let Ok(wi) = schema.index_of("weight") {
                            get_string_value(batch, wi, row)
                                .and_then(|w| w.parse::<f64>().ok())
                                // `clamp` is not the same as the old
                                // `.min(1.0).max(0.0)` for a non-finite weight:
                                // that chain silently turned NaN into 1.0, i.e.
                                // maximum confidence. Drop non-finite weights
                                // and fall back to the default instead.
                                .filter(|w| w.is_finite())
                                .map(|w| (w.clamp(0.0, 1.0), *default_conf))
                                .unwrap_or((*default_freq, *default_conf))
                        } else {
                            (*default_freq, *default_conf)
                        };

                        edges.push(reasoning::TruthEdge {
                            source,
                            target,
                            rel_type: rel_type.to_string(),
                            truth: reasoning::TruthValue::new(freq, conf),
                            inferred: false,
                            via: vec![],
                            inference_type: None,
                        });
                    }
                }
            }
        }
    }

    Ok(edges)
}

/// Get truth values for all edges in the loaded graph.
///
/// Returns a JSON-serializable summary of edge truth values, not just
/// rendering instructions.
pub fn get_graph_truth_summary(min_confidence: f64) -> Result<serde_json::Value, String> {
    let edges = extract_graph_truth_edges()?;
    let filtered: Vec<&reasoning::TruthEdge> = edges
        .iter()
        .filter(|e| e.truth.confidence >= min_confidence)
        .collect();

    // Summary by relationship type
    let mut by_type: std::collections::HashMap<&str, Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    for e in &filtered {
        by_type
            .entry(&e.rel_type)
            .or_default()
            .push((e.truth.frequency, e.truth.confidence));
    }

    let type_summaries: Vec<serde_json::Value> = by_type
        .iter()
        .map(|(rel_type, values)| {
            let avg_freq = values.iter().map(|(f, _)| f).sum::<f64>() / values.len() as f64;
            let avg_conf = values.iter().map(|(_, c)| c).sum::<f64>() / values.len() as f64;
            serde_json::json!({
                "rel_type": rel_type,
                "count": values.len(),
                "avg_frequency": (avg_freq * 1000.0).round() / 1000.0,
                "avg_confidence": (avg_conf * 1000.0).round() / 1000.0,
                "avg_expectation": ((avg_conf * (avg_freq - 0.5) + 0.5) * 1000.0).round() / 1000.0,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "total_edges": edges.len(),
        "filtered_edges": filtered.len(),
        "min_confidence": min_confidence,
        "by_relationship_type": type_summaries,
        "edge_rendering": {
            "opacity": "frequency",
            "width": "confidence",
            "threshold": min_confidence,
        },
    }))
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

#[cfg(test)]
mod runtime_context_tests {
    use super::block_on_sync;

    // Regression for the "Cannot start a runtime from within a runtime" panic.
    //
    // `block_on_sync` is the synchronous bridge used by `execute` /
    // `run_cypher_on_aiwar` / the `%%think` path. The cockpit server reaches
    // those from Axum request handlers running on Tokio worker threads (e.g.
    // `/api/data/status`, the `cell_execute` MCP tool). The old
    // `Runtime::new().block_on(..)` bridge panicked in that context; this test
    // pins the fix by driving a future to completion from *inside* a running
    // multi-thread runtime and asserting it does not panic.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn does_not_panic_inside_async_runtime() {
        let v = block_on_sync(async { 7u32 }).expect("bridge must not fail");
        assert_eq!(v, 7);
    }

    // The cockpit handlers may additionally offload to `spawn_blocking`; the
    // bridge must stay panic-free there too (blocking-pool threads still report
    // an ambient runtime via `Handle::try_current`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn does_not_panic_inside_spawn_blocking() {
        let v = tokio::task::spawn_blocking(|| block_on_sync(async { 8u32 }))
            .await
            .expect("blocking task joined")
            .expect("bridge must not fail");
        assert_eq!(v, 8);
    }

    // With no ambient runtime (a plain notebook-cell call), the bridge builds
    // its own runtime and still resolves the future.
    #[test]
    fn builds_runtime_when_none_present() {
        let v = block_on_sync(async { 9u32 }).expect("bridge must not fail");
        assert_eq!(v, 9);
    }
}
