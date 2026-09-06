//! Encounter-round loader for Cypher enrichment files.
//!
//! Parses CREATE/MERGE statements from `.cypher` files, extracts nodes and
//! relationships, and assigns confidence-based truth values derived from the
//! file's provenance tier.

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

// ── Public types ──

/// A single enrichment round parsed from one Cypher file.
#[derive(Debug, Clone)]
pub struct EncounterRound {
    /// Monotonic version number (0 = aiwar_full, 1 = aiwar_enriched, 31..43 for patches).
    pub version: u32,
    /// Human-readable name derived from the filename.
    pub name: String,
    /// Original filename (not the full path).
    pub source_file: String,
    /// Confidence score assigned to every fact in this round.
    pub confidence: f64,
    /// Relationship edges extracted from MATCH/MERGE/CREATE patterns.
    pub edges: Vec<CypherEdge>,
    /// Nodes extracted from CREATE/MERGE patterns.
    pub nodes: Vec<CypherNode>,
}

/// A node extracted from a Cypher CREATE or MERGE statement.
#[derive(Debug, Clone)]
pub struct CypherNode {
    /// The variable-name or `id` property value (e.g. `"Hegseth"`).
    pub id: String,
    /// Neo4j labels (e.g. `["Person"]`).
    pub labels: Vec<String>,
    /// Flat property map extracted from the `{...}` block.
    pub properties: HashMap<String, String>,
}

/// A directed edge extracted from a Cypher relationship pattern.
#[derive(Debug, Clone)]
pub struct CypherEdge {
    /// Source node identifier (variable name from the MATCH clause).
    pub source: String,
    /// Target node identifier.
    pub target: String,
    /// Relationship type (e.g. `PERSON_LINK`, `CONNECTED_TO`).
    pub rel_type: String,
    /// Properties set on the relationship via SET clauses or inline `{...}`.
    pub properties: HashMap<String, String>,
}

// ── Confidence mapping ──

/// Assign a confidence score based on the enrichment file's provenance tier.
pub fn confidence_for_file(filename: &str) -> f64 {
    if filename.contains("grok_verified") || filename.contains("v43_corrections") {
        0.95
    } else if filename.contains("epstein_v3") {
        0.60
    } else if filename.contains("v40_") || filename.contains("v41_") || filename.contains("v42_") {
        0.70
    } else {
        0.80
    }
}

// ── Version extraction ──

/// Derive a sort-key version from a filename.
///
/// - `aiwar_full.cypher`        -> 0
/// - `aiwar_enriched.cypher`    -> 1
/// - `aiwar_enrichment_*.cypher` (no version number) -> 2
/// - `*_v31_*.cypher` .. `*_v43_*.cypher` -> 31..43
fn version_for_file(filename: &str) -> u32 {
    // Check for explicit version numbers v31..v99
    let re = Regex::new(r"v(\d{2,})").expect("valid regex");
    if let Some(caps) = re.captures(filename)
        && let Ok(v) = caps[1].parse::<u32>()
    {
        return v;
    }
    if filename.contains("aiwar_full") {
        0
    } else if filename == "aiwar_enriched.cypher" {
        1
    } else {
        // Other enrichment files without an explicit version get version 2
        2
    }
}

// ── Cypher parser (intentionally simple) ──

/// Parse a single Cypher file's text, returning extracted nodes and edges.
fn parse_cypher(text: &str) -> (Vec<CypherNode>, Vec<CypherEdge>) {
    // A node reference: `(var [:Label[:Label…]] [ {props} ])`. Used both to
    // DECLARE a node (when a label is present) and to BIND a variable to a
    // resolvable id so relationships can name real endpoints, not letters.
    let node_ref_re =
        Regex::new(r"(?i)\((\w+)((?::\w+)*)\s*(?:\{([^}]*)\})?\)").expect("valid node-ref regex");
    // A relationship: `(svar […])-[ [relvar] :REL [ {props} ] ]->(tvar […])`.
    // Endpoint labels/props are tolerated (inline-declared endpoints); only the
    // variable name is captured — resolution happens via the per-statement map.
    let rel_re = Regex::new(
        r"(?i)\((\w+)(?::\w+)*\s*(?:\{[^}]*\})?\)\s*-\[(\w+)?:([\w|]+)\s*(?:\{([^}]*)\})?\]\s*->\s*\((\w+)(?::\w+)*\s*(?:\{[^}]*\})?\)",
    )
    .expect("valid rel regex");
    // A statement-trailing `SET relvar.key = value` clause (edge properties).
    let set_re = Regex::new(r"(?is)\bSET\b\s+(.*)").expect("valid set regex");

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Cypher statements are ';'-terminated and variable bindings are
    // statement-local. Resolving per statement (not over the whole file) is what
    // stops a variable like `a` bound to different ids in different statements
    // from cross-contaminating, and what prevents phantom `a`/`b`/`v` endpoints
    // (the harvest's `MERGE (v:SchemaValue {value:…}) WITH v MATCH (a:SchemaAxis
    // {name:…}) MERGE (v)-[:VALID_FOR]->(a)` pattern).
    for stmt in text.split(';') {
        // 1. Bind every variable in the statement to a resolvable id, and emit a
        //    node for each ref that carries a label. Id resolution order:
        //    `id` → `value` (SchemaValue) → `name` (SchemaAxis). A bare ref like
        //    `(a)` carries nothing and binds nothing.
        let mut var_id: HashMap<String, String> = HashMap::new();
        for caps in node_ref_re.captures_iter(stmt) {
            let var = caps[1].to_string();
            let labels: Vec<String> = caps[2]
                .split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            let properties = caps
                .get(3)
                .map(|m| parse_property_block(m.as_str()))
                .unwrap_or_default();
            // Resolve the identity; treat empty and the pandas missing-data
            // marker `nan` (444 `SchemaValue {value:'nan'}` lines in the harvest)
            // as NO identity — skip, so they never become a phantom hub node.
            let Some(id) = properties
                .get("id")
                .or_else(|| properties.get("value"))
                .or_else(|| properties.get("name"))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("nan"))
                .map(str::to_string)
            else {
                continue;
            };
            var_id.entry(var).or_insert_with(|| id.clone());
            if !labels.is_empty() {
                nodes.push(CypherNode {
                    id,
                    labels,
                    properties,
                });
            }
        }

        // 2. Statement-trailing SET clause (edge properties via `SET r.k = v`).
        let set_props = set_re
            .captures(stmt)
            .map(|c| parse_set_clause(&c[1]))
            .unwrap_or_default();

        // 3. Relationships — resolve BOTH endpoints through the binding map; an
        //    edge whose endpoints don't resolve is SKIPPED (never emitted as a
        //    phantom variable-name edge).
        for caps in rel_re.captures_iter(stmt) {
            let svar = &caps[1];
            let rel_type = caps[3].to_string();
            let mut properties = caps
                .get(4)
                .map(|m| parse_property_block(m.as_str()))
                .unwrap_or_default();
            let tvar = &caps[5];
            let (Some(source), Some(target)) = (var_id.get(svar), var_id.get(tvar)) else {
                continue;
            };
            for (k, v) in &set_props {
                properties.entry(k.clone()).or_insert_with(|| v.clone());
            }
            edges.push(CypherEdge {
                source: source.clone(),
                target: target.clone(),
                rel_type,
                properties,
            });
        }
    }

    // Deduplicate nodes by id (keep the one with more properties).
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut deduped_nodes: Vec<CypherNode> = Vec::new();
    for node in nodes {
        if let Some(&idx) = seen.get(&node.id) {
            if node.properties.len() > deduped_nodes[idx].properties.len() {
                deduped_nodes[idx] = node;
            }
        } else {
            seen.insert(node.id.clone(), deduped_nodes.len());
            deduped_nodes.push(node);
        }
    }

    (deduped_nodes, edges)
}

/// Parse a Cypher property block like `id: 'foo', name: 'bar', weight: 4`.
fn parse_property_block(block: &str) -> HashMap<String, String> {
    let mut props = HashMap::new();
    // Match key: 'value' or key: number patterns.
    let prop_re =
        Regex::new(r"(\w+)\s*:\s*(?:'([^']*)'|(\d+(?:\.\d+)?)|(\w+))").expect("valid prop regex");
    for caps in prop_re.captures_iter(block) {
        let key = caps[1].to_string();
        let value = caps
            .get(2)
            .or(caps.get(3))
            .or(caps.get(4))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        props.insert(key, value);
    }
    props
}

/// Parse a SET clause like `r.label='appointed SecDef', r.weight=4, r.source='WH'`.
fn parse_set_clause(clause: &str) -> HashMap<String, String> {
    let mut props = HashMap::new();
    // Match: var.key = 'value' or var.key = number
    let set_re = Regex::new(r"\w+\.(\w+)\s*=\s*(?:'([^']*(?:''[^']*)*)'|(\d+(?:\.\d+)?)|(\w+))")
        .expect("valid set regex");
    for caps in set_re.captures_iter(clause) {
        let key = caps[1].to_string();
        let value = caps
            .get(2)
            .or(caps.get(3))
            .or(caps.get(4))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        props.insert(key, value);
    }
    props
}

// ── Public API ──

/// Load all `.cypher` files from `cypher_dir`, parse them into encounter rounds,
/// and return them sorted by version order.
pub fn load_encounter_rounds(cypher_dir: &Path) -> Result<Vec<EncounterRound>, std::io::Error> {
    let mut entries: Vec<_> = std::fs::read_dir(cypher_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "cypher"))
        .collect();

    // Sort by version, then by filename for stability.
    entries.sort_by(|a, b| {
        let va = version_for_file(&a.file_name().to_string_lossy());
        let vb = version_for_file(&b.file_name().to_string_lossy());
        va.cmp(&vb).then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let mut rounds = Vec::new();
    for entry in entries {
        let filename = entry.file_name().to_string_lossy().to_string();
        let text = std::fs::read_to_string(entry.path())?;
        let version = version_for_file(&filename);
        let confidence = confidence_for_file(&filename);
        let (nodes, edges) = parse_cypher(&text);

        let name = filename.trim_end_matches(".cypher").replace('_', " ");

        rounds.push(EncounterRound {
            version,
            name,
            source_file: filename,
            confidence,
            edges,
            nodes,
        });
    }

    Ok(rounds)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_mapping() {
        assert_eq!(
            confidence_for_file("aiwar_enrichment_grok_verified.cypher"),
            0.95
        );
        assert_eq!(
            confidence_for_file("aiwar_v43_corrections_evidence_review.cypher"),
            0.95
        );
        assert_eq!(
            confidence_for_file("aiwar_enrichment_epstein_v31_patch.cypher"),
            0.60
        );
        assert_eq!(
            confidence_for_file("aiwar_enrichment_epstein_v39_patch.cypher"),
            0.60
        );
        assert_eq!(
            confidence_for_file("aiwar_enrichment_v40_surveillance_ecosystem.cypher"),
            0.70
        );
        assert_eq!(
            confidence_for_file("aiwar_enrichment_v41_anduril_europe.cypher"),
            0.70
        );
        assert_eq!(
            confidence_for_file("aiwar_enrichment_v42_bilderberg_doepfner.cypher"),
            0.70
        );
        assert_eq!(confidence_for_file("aiwar_full.cypher"), 0.80);
        assert_eq!(confidence_for_file("aiwar_enriched.cypher"), 0.80);
    }

    #[test]
    fn test_version_ordering() {
        assert_eq!(version_for_file("aiwar_full.cypher"), 0);
        assert_eq!(version_for_file("aiwar_enriched.cypher"), 1);
        assert_eq!(version_for_file("aiwar_enrichment_grok_verified.cypher"), 2);
        assert_eq!(
            version_for_file("aiwar_enrichment_epstein_v31_patch.cypher"),
            31
        );
        assert_eq!(
            version_for_file("aiwar_enrichment_v40_surveillance_ecosystem.cypher"),
            40
        );
        assert_eq!(
            version_for_file("aiwar_v43_corrections_evidence_review.cypher"),
            43
        );
    }

    #[test]
    fn test_parse_create_node() {
        let cypher = "CREATE (n:System {id: 'Maven', name: 'Project Maven', year: 2017})";
        let (nodes, edges) = parse_cypher(cypher);
        assert_eq!(nodes.len(), 1);
        assert_eq!(edges.len(), 0);
        assert_eq!(nodes[0].id, "Maven");
        assert_eq!(nodes[0].labels, vec!["System"]);
        assert_eq!(&nodes[0].properties["name"], "Project Maven");
        assert_eq!(&nodes[0].properties["year"], "2017");
    }

    #[test]
    fn test_parse_merge_node() {
        let cypher =
            "MERGE (p:Person {id: 'Hegseth'}) SET p.name = 'Pete Hegseth', p.role = 'SecDef'";
        let (nodes, _) = parse_cypher(cypher);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "Hegseth");
        assert_eq!(nodes[0].labels, vec!["Person"]);
    }

    #[test]
    fn test_parse_create_edge() {
        // Inline-declared endpoints bind a→A, b→B, so the edge resolves to real
        // ids (not the letters `a`/`b`).
        let cypher =
            "CREATE (a:System {id: 'A'})-[:DEVELOPED_BY {weight: 3}]->(b:Stakeholder {id: 'B'})";
        let (nodes, edges) = parse_cypher(cypher);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "A");
        assert_eq!(edges[0].target, "B");
        assert_eq!(edges[0].rel_type, "DEVELOPED_BY");
        assert_eq!(&edges[0].properties["weight"], "3");
        // both endpoints were also declared as nodes
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_unbound_edge_is_skipped_not_phantom() {
        // Bare variables with no binding must NOT become phantom `a`/`b`
        // endpoints — the edge is skipped instead.
        let cypher = "MERGE (a)-[r:PERSON_LINK]->(b)";
        let (_, edges) = parse_cypher(cypher);
        assert!(
            edges.is_empty(),
            "unbound endpoints are skipped, not emitted"
        );
    }

    #[test]
    fn test_schema_value_axis_with_clause_resolves() {
        // The harvest's VALID_FOR pattern: `v` bound by {value}, `a` bound by
        // {name}, across a WITH clause. Endpoints must resolve to the value/name
        // ids — never to the letters `v`/`a`.
        let cypher = "MERGE (v:SchemaValue {value: 'Development'}) WITH v MATCH (a:SchemaAxis {name: 'currentStatus_airo'}) MERGE (v)-[:VALID_FOR]->(a)";
        let (nodes, edges) = parse_cypher(cypher);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "Development");
        assert_eq!(edges[0].target, "currentStatus_airo");
        assert_eq!(edges[0].rel_type, "VALID_FOR");
        assert!(
            nodes
                .iter()
                .any(|n| n.id == "Development" && n.labels == ["SchemaValue"])
        );
        assert!(nodes.iter().any(|n| n.id == "currentStatus_airo"));
        // no phantom single-letter node ids
        assert!(!nodes.iter().any(|n| n.id == "v" || n.id == "a"));
    }

    #[test]
    fn test_nan_and_empty_ids_are_skipped() {
        // pandas exported missing cells as the string 'nan' (444 lines). These
        // must not become a node, and edges to them must be skipped.
        let cypher = "MERGE (v:SchemaValue {value: 'nan'}) WITH v MATCH (a:SchemaAxis {name: 'airo_type'}) MERGE (v)-[:VALID_FOR]->(a)";
        let (nodes, edges) = parse_cypher(cypher);
        assert!(!nodes.iter().any(|n| n.id.eq_ignore_ascii_case("nan")));
        // the real axis still materializes…
        assert!(nodes.iter().any(|n| n.id == "airo_type"));
        // …but the VALID_FOR edge from the missing value is dropped (unresolved).
        assert!(edges.is_empty(), "edge from a 'nan' value is skipped");
    }

    #[test]
    fn test_parse_full_match_merge_edge() {
        let cypher = "MATCH (a {id:'Trump'}) MATCH (b {id:'Hegseth'}) MERGE (a)-[r:PERSON_LINK]->(b) SET r.label='appointed SecDef', r.weight=4";
        let (_, edges) = parse_cypher(cypher);
        // The full-line regex should capture this
        let full_edges: Vec<_> = edges.iter().filter(|e| e.source == "Trump").collect();
        assert!(
            !full_edges.is_empty(),
            "should parse MATCH-MATCH-MERGE pattern"
        );
        assert_eq!(full_edges[0].target, "Hegseth");
        assert_eq!(full_edges[0].rel_type, "PERSON_LINK");
        assert_eq!(&full_edges[0].properties["label"], "appointed SecDef");
    }

    #[test]
    fn test_parse_property_block() {
        let props = parse_property_block("id: 'test', name: 'Hello World', weight: 42");
        assert_eq!(&props["id"], "test");
        assert_eq!(&props["name"], "Hello World");
        assert_eq!(&props["weight"], "42");
    }

    #[test]
    fn test_parse_set_clause() {
        let props = parse_set_clause("r.label='test edge', r.weight=5, r.source='Reuters'");
        assert_eq!(&props["label"], "test edge");
        assert_eq!(&props["weight"], "5");
        assert_eq!(&props["source"], "Reuters");
    }

    #[test]
    fn test_load_encounter_rounds_from_temp_dir() {
        let dir = std::env::temp_dir().join("aiwar_test_cypher");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Write two test files
        std::fs::write(
            dir.join("aiwar_full.cypher"),
            "CREATE (n:System {id: 'Sys1', name: 'System One'})\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("aiwar_enrichment_epstein_v31_patch.cypher"),
            "MERGE (p:Person {id: 'P1'}) SET p.name = 'Person One'\nMATCH (a {id:'Sys1'}) MATCH (b {id:'P1'}) MERGE (a)-[r:CONNECTED_TO]->(b) SET r.label='link'\n",
        )
        .unwrap();

        let rounds = load_encounter_rounds(&dir).unwrap();
        assert_eq!(rounds.len(), 2);

        // First round: aiwar_full (version 0, confidence 0.80)
        assert_eq!(rounds[0].version, 0);
        assert!((rounds[0].confidence - 0.80).abs() < f64::EPSILON);
        assert_eq!(rounds[0].nodes.len(), 1);
        assert_eq!(rounds[0].nodes[0].id, "Sys1");

        // Second round: v31 patch (version 31, confidence 0.60)
        assert_eq!(rounds[1].version, 31);
        assert!((rounds[1].confidence - 0.60).abs() < f64::EPSILON);
        assert_eq!(rounds[1].nodes.len(), 1);
        assert!(!rounds[1].edges.is_empty());

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
