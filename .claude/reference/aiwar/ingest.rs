// aiwar-neo4j-harvest/src/ingest.rs
//
// Neo4j Cypher generation for the AI War Cloud graph.
// 
// NOVEL PATTERNS harvested for Neo4j succession (→ Ada Sigma Graph):
//
// Pattern 1: FACETED MULTI-LABEL NODES
//   Nodes get multiple Neo4j labels based on their taxonomy axes.
//   A System node might be (:System:GenerativeAI:Predict:Intelligence:Operation)
//   This enables O(1) label-indexed queries across any axis.
//
// Pattern 2: SCHEMA ONTOLOGY AS CONSTRAINT GRAPH
//   The Schema rows become (:SchemaValue) nodes with (:VALID_FOR) edges
//   to (:SchemaAxis) nodes. This makes the ontology queryable and evolvable.
//
// Pattern 3: ICON-ADDRESSED VISUAL TOPOLOGY
//   NounKey links create a visual namespace — nodes addressed by their
//   icon/glyph rather than just by ID. Parallel to QHDR.sigma addressing.
//
// Pattern 4: TEMPORAL LAYERING
//   year + currentStatus create a natural temporal partition.
//   Nodes flow through Development → Deployment → Operation → Retirement.

use crate::model::*;

/// Generate CREATE CONSTRAINT statements
pub fn constraints() -> Vec<String> {
    vec![
        "CREATE CONSTRAINT IF NOT EXISTS FOR (s:System) REQUIRE s.id IS UNIQUE".into(),
        "CREATE CONSTRAINT IF NOT EXISTS FOR (s:Stakeholder) REQUIRE s.id IS UNIQUE".into(),
        "CREATE CONSTRAINT IF NOT EXISTS FOR (c:CivicSystem) REQUIRE c.id IS UNIQUE".into(),
        "CREATE CONSTRAINT IF NOT EXISTS FOR (h:HistoricalSystem) REQUIRE h.id IS UNIQUE".into(),
        "CREATE CONSTRAINT IF NOT EXISTS FOR (p:Person) REQUIRE p.id IS UNIQUE".into(),
        "CREATE CONSTRAINT IF NOT EXISTS FOR (sv:SchemaValue) REQUIRE sv.value IS UNIQUE".into(),
        "CREATE INDEX IF NOT EXISTS FOR (s:System) ON (s.year)".into(),
        "CREATE INDEX IF NOT EXISTS FOR (s:System) ON (s.current_status)".into(),
        "CREATE INDEX IF NOT EXISTS FOR (s:System) ON (s.noun_key)".into(),
        "CREATE INDEX IF NOT EXISTS FOR (s:Stakeholder) ON (s.stakeholder_type)".into(),
    ]
}

/// Generate MERGE for a System node with multi-label faceting
pub fn system_cypher(sys: &serde_json::Value) -> String {
    let id = sys["id"].as_str().unwrap_or("unknown");
    let name = escape(sys["name"].as_str().unwrap_or(id));
    let year = sys["year"].as_i64().map(|y| y.to_string()).unwrap_or("null".into());
    let status = sys["currentStatus"].as_str().unwrap_or("");
    let sys_type = sys["type"].as_str().unwrap_or("");
    let ml_task = sys["MLTask"].as_str().unwrap_or("");
    let mil_use = sys["militaryUse"].as_str().unwrap_or("");
    let civ_use = sys["civicUse"].as_str().unwrap_or("");
    let purpose = escape(sys["purpose"].as_str().unwrap_or(""));
    let capacity = escape(sys["capacity"].as_str().unwrap_or(""));
    let technique = escape(sys["vair:technique"].as_str().unwrap_or(""));
    let output = sys["output"].as_str().unwrap_or("");
    let risks = escape(sys["vair:riskSources"].as_str().unwrap_or(""));
    let impact = escape(sys["impact"].as_str().unwrap_or(""));
    let hover = escape(sys["hover"].as_str().unwrap_or(""));
    let image = sys["image"].as_str().unwrap_or("");
    let noun_key = sys["nounKey"].as_str().unwrap_or("");

    // Build dynamic labels from taxonomy axes
    let mut labels = vec!["System".to_string()];
    if !status.is_empty() { labels.push(status.to_string()); }
    if !ml_task.is_empty() { labels.push(format!("MLTask_{}", ml_task)); }

    let label_str = labels.join(":");

    format!(
        r#"MERGE (n:{label_str} {{id: '{id}'}})
SET n.name = '{name}',
    n.year = {year},
    n.current_status = '{status}',
    n.system_type = '{sys_type}',
    n.ml_task = '{ml_task}',
    n.military_use = '{mil_use}',
    n.civic_use = '{civ_use}',
    n.purpose = '{purpose}',
    n.capacity = '{capacity}',
    n.technique = '{technique}',
    n.output = '{output}',
    n.risk_sources = '{risks}',
    n.impact = '{impact}',
    n.hover = '{hover}',
    n.image = '{image}',
    n.noun_key = '{noun_key}';"#,
    )
}

/// Generate MERGE for a Stakeholder node
pub fn stakeholder_cypher(sh: &serde_json::Value) -> String {
    let id = sh["id"].as_str().unwrap_or("unknown");
    let name = escape(sh["name"].as_str().unwrap_or(id));
    let sh_type = sh["type"].as_str().unwrap_or("");
    let airo = sh["airo:type"].as_str().unwrap_or("");
    let hover = escape(sh["hover"].as_str().unwrap_or(""));
    let image = sh["image"].as_str().unwrap_or("");

    let mut labels = vec!["Stakeholder".to_string()];
    if !sh_type.is_empty() { labels.push(sh_type.replace(' ', "").to_string()); }
    if !airo.is_empty() { labels.push(airo.to_string()); }

    let label_str = labels.join(":");

    format!(
        r#"MERGE (n:{label_str} {{id: '{id}'}})
SET n.name = '{name}',
    n.stakeholder_type = '{sh_type}',
    n.airo_type = '{airo}',
    n.hover = '{hover}',
    n.image = '{image}';"#,
    )
}

/// Generate MERGE for a CivicSystem node
pub fn civic_cypher(c: &serde_json::Value) -> String {
    let id = c["id"].as_str().unwrap_or("unknown");
    let name = escape(c["name"].as_str().unwrap_or(id));
    let year = c["year"].as_i64().map(|y| y.to_string()).unwrap_or("null".into());
    let status = c["currentStatus"].as_str().unwrap_or("");
    let ctype = c["type"].as_str().unwrap_or("");
    let hover = escape(c["hover"].as_str().unwrap_or(""));
    let image = c["image"].as_str().unwrap_or("");
    let noun_key = c["nounKey"].as_str().unwrap_or("");

    format!(
        r#"MERGE (n:CivicSystem {{id: '{id}'}})
SET n.name = '{name}',
    n.year = {year},
    n.current_status = '{status}',
    n.system_type = '{ctype}',
    n.hover = '{hover}',
    n.image = '{image}',
    n.noun_key = '{noun_key}';"#,
    )
}

/// Generate MERGE for edges
pub fn edge_cypher(edge: &serde_json::Value, rel_type: &str) -> String {
    let source = edge["source"].as_str().unwrap_or("unknown");
    let target = edge["target"].as_str().unwrap_or("unknown");
    let label = escape(edge["label"].as_str().unwrap_or(rel_type));
    let weight = edge["weight"].as_f64().unwrap_or(1.0);

    // Use generic Node match since source/target can be any node type
    format!(
        r#"MATCH (a {{id: '{source}'}})
MATCH (b {{id: '{target}'}})
MERGE (a)-[r:{rel_type}]->(b)
SET r.label = '{label}',
    r.weight = {weight};"#,
    )
}

/// Generate the schema ontology as nodes + edges
/// This is the NOVEL pattern: schema-as-queryable-graph
pub fn schema_ontology_cypher(schema: &[serde_json::Value]) -> Vec<String> {
    let mut stmts = Vec::new();

    // Create axis nodes
    let axes = [
        "currentStatus", "type", "militaryUse", "civicUse",
        "MLTask", "MLType", "purpose", "capacity", "output",
        "impact", "stakeholder", "airo_type",
    ];
    for axis in &axes {
        stmts.push(format!(
            "MERGE (:SchemaAxis {{name: '{axis}'}});"
        ));
    }

    // For each schema row, create value nodes and link to axes
    let field_map = [
        ("currentStatus:airo", "currentStatus"),
        ("type", "type"),
        ("militaryUse", "militaryUse"),
        ("civicUse", "civicUse"),
        ("MLTask", "MLTask"),
        ("MLType", "MLType"),
        ("purpose:vair", "purpose"),
        ("capacity:airo", "capacity"),
        ("output:airo", "output"),
        ("impact:vair", "impact"),
        ("stakeholder", "stakeholder"),
        ("airo:type", "airo_type"),
    ];

    for row in schema {
        for (json_key, axis_name) in &field_map {
            if let Some(val) = row[json_key].as_str() {
                if !val.is_empty() {
                    let escaped = escape(val);
                    stmts.push(format!(
                        r#"MERGE (v:SchemaValue {{value: '{escaped}'}})
WITH v
MATCH (a:SchemaAxis {{name: '{axis_name}'}})
MERGE (v)-[:VALID_FOR]->(a);"#,
                    ));
                }
            }
        }
    }

    stmts
}

fn escape(s: &str) -> String {
    s.replace('\'', "\\'").replace('\\', "\\\\")
}
