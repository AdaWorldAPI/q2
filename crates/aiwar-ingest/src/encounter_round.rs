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
    } else if filename.contains("v40_") || filename.contains("v41_") || filename.contains("v42_")
    {
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
    if let Some(caps) = re.captures(filename) {
        if let Ok(v) = caps[1].parse::<u32>() {
            return v;
        }
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
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // ── Node patterns ──
    // Matches: CREATE (var:Label {props})  or  MERGE (var:Label {props})
    // Also handles multi-label like :Label1:Label2
    // The property block is optional.
    let node_re = Regex::new(
        r"(?i)(?:CREATE|MERGE)\s+\((\w+)((?::\w+)+)\s*(?:\{([^}]*)\})?\s*\)",
    )
    .expect("valid node regex");

    for caps in node_re.captures_iter(text) {
        let var_name = caps[1].to_string();
        let labels_raw = &caps[2]; // e.g. ":Person" or ":SchemaValue"
        let labels: Vec<String> = labels_raw
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let properties = caps
            .get(3)
            .map(|m| parse_property_block(m.as_str()))
            .unwrap_or_default();

        // Use the `id` property if available, otherwise fall back to variable name.
        let id = properties
            .get("id")
            .cloned()
            .unwrap_or_else(|| var_name.clone());

        nodes.push(CypherNode {
            id,
            labels,
            properties,
        });
    }

    // ── Relationship patterns ──
    // Matches patterns like:
    //   MERGE (a)-[r:REL_TYPE]->(b)
    //   MERGE (a)-[:REL_TYPE {props}]->(b)
    //   CREATE (a)-[:REL_TYPE]->(b)
    // The variable on the relationship is optional.
    let edge_re = Regex::new(
        r"(?i)(?:CREATE|MERGE)\s+\((\w+)\)\s*-\[(?:\w+)?:([\w|]+)\s*(?:\{([^}]*)\})?\]\s*->\s*\((\w+)\)",
    )
    .expect("valid edge regex");

    for caps in edge_re.captures_iter(text) {
        let source = caps[1].to_string();
        let rel_type = caps[2].to_string();
        let properties = caps
            .get(3)
            .map(|m| parse_property_block(m.as_str()))
            .unwrap_or_default();
        let target = caps[4].to_string();

        edges.push(CypherEdge {
            source,
            target,
            rel_type,
            properties,
        });
    }

    // ── MATCH ... MERGE relationship patterns ──
    // Many enrichment files use:
    //   MATCH (a {id:'X'}) MATCH (b {id:'Y'}) MERGE (a)-[r:REL]->(b) SET r.prop = 'val';
    // We resolve variable names to ids using a two-pass approach (Rust regex
    // does not support backreferences).
    //
    // Step 1: For each line, extract all MATCH (var {id:'...'}) bindings.
    let match_bind_re = Regex::new(r"(?i)MATCH\s+\((\w+)\s*\{[^}]*id\s*:\s*'([^']*)'[^}]*\}\)")
        .expect("valid match-bind regex");
    // Step 2: Find MERGE (var)-[r:REL]->(var) on the same line.
    let merge_rel_re = Regex::new(
        r"(?i)MERGE\s+\((\w+)\)\s*-\[(?:\w+)?:([\w|]+)\s*(?:\{[^}]*\})?\]\s*->\s*\((\w+)\)(?:\s*SET\s+(.*))?"
    ).expect("valid merge-rel regex");

    for line in text.lines() {
        // Build var -> id map from MATCH clauses on this line.
        let mut var_map: HashMap<String, String> = HashMap::new();
        for caps in match_bind_re.captures_iter(line) {
            var_map.insert(caps[1].to_string(), caps[2].to_string());
        }
        if var_map.is_empty() {
            continue;
        }
        // Look for MERGE relationship on the same line.
        if let Some(caps) = merge_rel_re.captures(line) {
            let src_var = &caps[1];
            let rel_type = caps[2].to_string();
            let tgt_var = &caps[3];

            let source = var_map.get(src_var).cloned().unwrap_or_else(|| src_var.to_string());
            let target = var_map.get(tgt_var).cloned().unwrap_or_else(|| tgt_var.to_string());

            let properties = caps
                .get(4)
                .map(|m| parse_set_clause(m.as_str()))
                .unwrap_or_default();

            edges.push(CypherEdge {
                source,
                target,
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
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "cypher")
        })
        .collect();

    // Sort by version, then by filename for stability.
    entries.sort_by(|a, b| {
        let va = version_for_file(&a.file_name().to_string_lossy());
        let vb = version_for_file(&b.file_name().to_string_lossy());
        va.cmp(&vb)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let mut rounds = Vec::new();
    for entry in entries {
        let filename = entry.file_name().to_string_lossy().to_string();
        let text = std::fs::read_to_string(entry.path())?;
        let version = version_for_file(&filename);
        let confidence = confidence_for_file(&filename);
        let (nodes, edges) = parse_cypher(&text);

        let name = filename
            .trim_end_matches(".cypher")
            .replace('_', " ")
            .to_string();

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
        assert_eq!(confidence_for_file("aiwar_enrichment_grok_verified.cypher"), 0.95);
        assert_eq!(confidence_for_file("aiwar_v43_corrections_evidence_review.cypher"), 0.95);
        assert_eq!(confidence_for_file("aiwar_enrichment_epstein_v31_patch.cypher"), 0.60);
        assert_eq!(confidence_for_file("aiwar_enrichment_epstein_v39_patch.cypher"), 0.60);
        assert_eq!(confidence_for_file("aiwar_enrichment_v40_surveillance_ecosystem.cypher"), 0.70);
        assert_eq!(confidence_for_file("aiwar_enrichment_v41_anduril_europe.cypher"), 0.70);
        assert_eq!(confidence_for_file("aiwar_enrichment_v42_bilderberg_doepfner.cypher"), 0.70);
        assert_eq!(confidence_for_file("aiwar_full.cypher"), 0.80);
        assert_eq!(confidence_for_file("aiwar_enriched.cypher"), 0.80);
    }

    #[test]
    fn test_version_ordering() {
        assert_eq!(version_for_file("aiwar_full.cypher"), 0);
        assert_eq!(version_for_file("aiwar_enriched.cypher"), 1);
        assert_eq!(version_for_file("aiwar_enrichment_grok_verified.cypher"), 2);
        assert_eq!(version_for_file("aiwar_enrichment_epstein_v31_patch.cypher"), 31);
        assert_eq!(version_for_file("aiwar_enrichment_v40_surveillance_ecosystem.cypher"), 40);
        assert_eq!(version_for_file("aiwar_v43_corrections_evidence_review.cypher"), 43);
    }

    #[test]
    fn test_parse_create_node() {
        let cypher = "CREATE (n:System {id: 'Maven', name: 'Project Maven', year: 2017})";
        let (nodes, edges) = parse_cypher(cypher);
        assert_eq!(nodes.len(), 1);
        assert_eq!(edges.len(), 0);
        assert_eq!(nodes[0].id, "Maven");
        assert_eq!(nodes[0].labels, vec!["System"]);
        assert_eq!(nodes[0].properties.get("name").unwrap(), "Project Maven");
        assert_eq!(nodes[0].properties.get("year").unwrap(), "2017");
    }

    #[test]
    fn test_parse_merge_node() {
        let cypher = "MERGE (p:Person {id: 'Hegseth'}) SET p.name = 'Pete Hegseth', p.role = 'SecDef'";
        let (nodes, _) = parse_cypher(cypher);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "Hegseth");
        assert_eq!(nodes[0].labels, vec!["Person"]);
    }

    #[test]
    fn test_parse_create_edge() {
        let cypher = "CREATE (a)-[:DEVELOPED_BY {weight: 3}]->(b)";
        let (_, edges) = parse_cypher(cypher);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "a");
        assert_eq!(edges[0].target, "b");
        assert_eq!(edges[0].rel_type, "DEVELOPED_BY");
        assert_eq!(edges[0].properties.get("weight").unwrap(), "3");
    }

    #[test]
    fn test_parse_merge_edge_with_variable() {
        let cypher = "MERGE (a)-[r:PERSON_LINK]->(b)";
        let (_, edges) = parse_cypher(cypher);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].rel_type, "PERSON_LINK");
    }

    #[test]
    fn test_parse_full_match_merge_edge() {
        let cypher = "MATCH (a {id:'Trump'}) MATCH (b {id:'Hegseth'}) MERGE (a)-[r:PERSON_LINK]->(b) SET r.label='appointed SecDef', r.weight=4";
        let (_, edges) = parse_cypher(cypher);
        // The full-line regex should capture this
        let full_edges: Vec<_> = edges.iter().filter(|e| e.source == "Trump").collect();
        assert!(!full_edges.is_empty(), "should parse MATCH-MATCH-MERGE pattern");
        assert_eq!(full_edges[0].target, "Hegseth");
        assert_eq!(full_edges[0].rel_type, "PERSON_LINK");
        assert_eq!(full_edges[0].properties.get("label").unwrap(), "appointed SecDef");
    }

    #[test]
    fn test_parse_property_block() {
        let props = parse_property_block("id: 'test', name: 'Hello World', weight: 42");
        assert_eq!(props.get("id").unwrap(), "test");
        assert_eq!(props.get("name").unwrap(), "Hello World");
        assert_eq!(props.get("weight").unwrap(), "42");
    }

    #[test]
    fn test_parse_set_clause() {
        let props = parse_set_clause("r.label='test edge', r.weight=5, r.source='Reuters'");
        assert_eq!(props.get("label").unwrap(), "test edge");
        assert_eq!(props.get("weight").unwrap(), "5");
        assert_eq!(props.get("source").unwrap(), "Reuters");
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
