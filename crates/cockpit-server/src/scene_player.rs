//! Scene player: Cypher-file → CypherStream { codebook_indices, source, timestamp }.
//!
//! This module replaces the fabricated `hash(content) % 4096` codebook indices
//! that lived in `shader_stream.rs` with real ones derived from the lance-graph
//! Cypher parser:
//!
//!   1. `discover_acts(dir)` walks `*.cypher` files in version order and tags
//!      each with a confidence score derived from the filename.
//!   2. `cypher_preview(content)` extracts a preview line for SSE display.
//!   3. `cypher_to_stream(content, ts)` parses the Cypher text with
//!      `lance_graph::parser::parse_cypher_query`, harvests identifier tokens
//!      (node labels, relationship types, property names, variables), and maps
//!      each via stable `DefaultHasher % 4096` into codebook indices.
//!      On parse failure it falls back to a regex-style identifier extractor
//!      so that pre-release / partial Cypher snippets still produce a real
//!      StreamDto.
//!
//! The mapping is DETERMINISTIC: same token text → same codebook index
//! across processes / runs. `source: "AriGraph"` flags the provenance so
//! downstream sensors can attribute the perturbation correctly.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use lance_graph::ast::{
    BooleanExpression, CypherQuery, GraphPattern, NodePattern, OrderByClause, PathPattern,
    PropertyRef, PropertyValue, ReadingClause, RelationshipPattern, ReturnClause, ReturnItem,
    UnwindClause, ValueExpression, WhereClause, WithClause,
};
use lance_graph::parser::parse_cypher_query;

/// Output of `cypher_to_stream` — the codebook indices a Cypher act produced,
/// plus provenance. This is a local struct so cockpit-server can drop its
/// `thinking-engine` dep entirely; the canonical R1 surface uses
/// `lance_graph_contract::cognitive_shader::*` for the cycle DTOs, and the
/// driver consumes raw `&[u16]` indices via `MockShaderDriver::perturb`.
#[derive(Clone, Debug)]
pub struct CypherStream {
    /// Provenance label ("AriGraph" for graph-derived perturbations).
    pub source: &'static str,
    pub codebook_indices: Vec<u16>,
    pub timestamp: u64,
}

/// Codebook size — keep in sync with `lance_graph_contract::codebook` width.
const CODEBOOK_SIZE: u32 = 4096;

// ── Public types ────────────────────────────────────────────────────────────

/// A single Cypher act discovered on disk.
#[derive(Clone, Debug)]
pub struct SceneAct {
    pub name: String,
    pub cypher_text: String,
    pub confidence: f32,
}

// ── Discovery ───────────────────────────────────────────────────────────────

/// Discover Cypher enrichment files in `dir`, version-ordered, tagged with
/// confidence derived from the filename.
pub fn discover_acts(dir: &str) -> Vec<SceneAct> {
    let path = Path::new(dir);
    if !path.exists() {
        return Vec::new();
    }
    let mut acts: Vec<SceneAct> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()?.to_str()? == "cypher" {
                let name = p.file_stem()?.to_string_lossy().into_owned();
                let cypher_text = std::fs::read_to_string(&p).ok()?;
                let confidence = confidence_from_name(&name);
                Some(SceneAct { name, cypher_text, confidence })
            } else {
                None
            }
        })
        .collect();

    // Version-ordered: v0 < v31 < v40 < v43.
    acts.sort_by(|a, b| a.name.cmp(&b.name));
    acts
}

/// Confidence score from filename (higher for verified/corrected files).
pub fn confidence_from_name(name: &str) -> f32 {
    if name.contains("corrections") || name.contains("verified") {
        0.92
    } else if name.contains("patch") {
        0.78
    } else if name.contains("allin") {
        0.85
    } else if name.contains("enriched") || name.contains("full") {
        0.70
    } else {
        0.65
    }
}

/// First non-empty, non-comment line of Cypher text, capped at 120 chars.
pub fn cypher_preview(content: &str) -> String {
    content
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .unwrap_or("// empty")
        .chars()
        .take(120)
        .collect()
}

// ── Cypher → StreamDto ──────────────────────────────────────────────────────

/// Parse `content` as Cypher with the lance-graph parser; on success harvest
/// identifier tokens from the AST. On parse failure, fall back to a regex-style
/// identifier extractor so partial / pre-release Cypher still produces a real
/// StreamDto.
///
/// Each token is mapped via stable `DefaultHasher % 4096` to a codebook index.
/// Source is `AriGraph` (these are graph-derived perturbations).
pub fn cypher_to_stream(content: &str, ts: u64) -> CypherStream {
    let mut tokens: Vec<String> = Vec::new();

    match parse_cypher_query(content) {
        Ok(query) => collect_tokens_from_query(&query, &mut tokens),
        Err(_) => collect_tokens_fallback(content, &mut tokens),
    }

    // If the AST walk produced nothing useful (degenerate query, comment-only,
    // etc.) widen the net with the fallback so we never emit an empty stream.
    if tokens.is_empty() {
        collect_tokens_fallback(content, &mut tokens);
    }

    // Map → codebook indices, dedup while preserving order.
    let mut seen: std::collections::HashSet<u16> = std::collections::HashSet::new();
    let mut indices: Vec<u16> = Vec::with_capacity(tokens.len());
    for tok in &tokens {
        let idx = stable_codebook_index(tok);
        if seen.insert(idx) {
            indices.push(idx);
        }
    }

    CypherStream {
        source: "AriGraph",
        codebook_indices: indices,
        timestamp: ts,
    }
}

// ── Token harvesting (AST walk) ─────────────────────────────────────────────

fn collect_tokens_from_query(query: &CypherQuery, out: &mut Vec<String>) {
    // Reading clauses (MATCH / UNWIND).
    for clause in &query.reading_clauses {
        collect_from_reading_clause(clause, out);
    }
    if let Some(w) = &query.where_clause {
        collect_from_where(w, out);
    }
    if let Some(wc) = &query.with_clause {
        collect_from_with(wc, out);
    }
    for clause in &query.post_with_reading_clauses {
        collect_from_reading_clause(clause, out);
    }
    if let Some(w) = &query.post_with_where_clause {
        collect_from_where(w, out);
    }
    collect_from_return(&query.return_clause, out);
    if let Some(ob) = &query.order_by {
        collect_from_order_by(ob, out);
    }
}

fn collect_from_reading_clause(clause: &ReadingClause, out: &mut Vec<String>) {
    match clause {
        ReadingClause::Match(m) => {
            for pat in &m.patterns {
                collect_from_pattern(pat, out);
            }
        }
        ReadingClause::Unwind(u) => collect_from_unwind(u, out),
    }
}

fn collect_from_pattern(pat: &GraphPattern, out: &mut Vec<String>) {
    match pat {
        GraphPattern::Node(n) => collect_from_node(n, out),
        GraphPattern::Path(p) => collect_from_path(p, out),
    }
}

fn collect_from_node(n: &NodePattern, out: &mut Vec<String>) {
    if let Some(v) = &n.variable {
        out.push(v.clone());
    }
    for label in &n.labels {
        out.push(label.clone());
    }
    for (key, val) in &n.properties {
        out.push(key.clone());
        collect_from_property_value(val, out);
    }
}

fn collect_from_path(p: &PathPattern, out: &mut Vec<String>) {
    collect_from_node(&p.start_node, out);
    for seg in &p.segments {
        collect_from_relationship(&seg.relationship, out);
        collect_from_node(&seg.end_node, out);
    }
}

fn collect_from_relationship(r: &RelationshipPattern, out: &mut Vec<String>) {
    if let Some(v) = &r.variable {
        out.push(v.clone());
    }
    for t in &r.types {
        out.push(t.clone());
    }
    for (key, val) in &r.properties {
        out.push(key.clone());
        collect_from_property_value(val, out);
    }
}

fn collect_from_unwind(u: &UnwindClause, out: &mut Vec<String>) {
    collect_from_value_expr(&u.expression, out);
    out.push(u.alias.clone());
}

fn collect_from_where(w: &WhereClause, out: &mut Vec<String>) {
    collect_from_bool_expr(&w.expression, out);
}

fn collect_from_bool_expr(b: &BooleanExpression, out: &mut Vec<String>) {
    match b {
        BooleanExpression::Comparison { left, right, .. } => {
            collect_from_value_expr(left, out);
            collect_from_value_expr(right, out);
        }
        BooleanExpression::And(l, r) | BooleanExpression::Or(l, r) => {
            collect_from_bool_expr(l, out);
            collect_from_bool_expr(r, out);
        }
        BooleanExpression::Not(inner) => collect_from_bool_expr(inner, out),
        BooleanExpression::Exists(p) => collect_from_property_ref(p, out),
        BooleanExpression::In { expression, list } => {
            collect_from_value_expr(expression, out);
            for v in list {
                collect_from_value_expr(v, out);
            }
        }
        BooleanExpression::Like { expression, .. }
        | BooleanExpression::ILike { expression, .. }
        | BooleanExpression::Contains { expression, .. }
        | BooleanExpression::StartsWith { expression, .. }
        | BooleanExpression::EndsWith { expression, .. }
        | BooleanExpression::IsNull(expression)
        | BooleanExpression::IsNotNull(expression) => {
            collect_from_value_expr(expression, out);
        }
    }
}

fn collect_from_value_expr(v: &ValueExpression, out: &mut Vec<String>) {
    match v {
        ValueExpression::Variable(name) => out.push(name.clone()),
        ValueExpression::Property(p) => collect_from_property_ref(p, out),
        ValueExpression::Literal(pv) => collect_from_property_value(pv, out),
        ValueExpression::ScalarFunction { name, args }
        | ValueExpression::AggregateFunction { name, args, .. } => {
            out.push(name.clone());
            for a in args {
                collect_from_value_expr(a, out);
            }
        }
        ValueExpression::Arithmetic { left, right, .. } => {
            collect_from_value_expr(left, out);
            collect_from_value_expr(right, out);
        }
        ValueExpression::VectorDistance { left, right, .. }
        | ValueExpression::VectorSimilarity { left, right, .. } => {
            collect_from_value_expr(left, out);
            collect_from_value_expr(right, out);
        }
        ValueExpression::Parameter(name) => out.push(name.clone()),
        ValueExpression::VectorLiteral(_) => {} // numeric vector — no identifiers
    }
}

fn collect_from_property_ref(p: &PropertyRef, out: &mut Vec<String>) {
    out.push(p.variable.clone());
    out.push(p.property.clone());
}

fn collect_from_property_value(pv: &PropertyValue, out: &mut Vec<String>) {
    match pv {
        PropertyValue::String(s) => {
            // String literals can carry domain-meaningful tokens (e.g. label names);
            // treat each whitespace-separated identifier-shaped chunk as a token.
            for tok in tokenize_identifier_chunks(s) {
                out.push(tok);
            }
        }
        PropertyValue::Parameter(name) => out.push(name.clone()),
        PropertyValue::Property(p) => collect_from_property_ref(p, out),
        PropertyValue::Integer(_) | PropertyValue::Float(_)
        | PropertyValue::Boolean(_) | PropertyValue::Null => {}
    }
}

fn collect_from_with(wc: &WithClause, out: &mut Vec<String>) {
    for item in &wc.items {
        collect_from_return_item(item, out);
    }
    if let Some(ob) = &wc.order_by {
        collect_from_order_by(ob, out);
    }
}

fn collect_from_return(r: &ReturnClause, out: &mut Vec<String>) {
    for item in &r.items {
        collect_from_return_item(item, out);
    }
}

fn collect_from_return_item(item: &ReturnItem, out: &mut Vec<String>) {
    collect_from_value_expr(&item.expression, out);
    if let Some(a) = &item.alias {
        out.push(a.clone());
    }
}

fn collect_from_order_by(ob: &OrderByClause, out: &mut Vec<String>) {
    for item in &ob.items {
        collect_from_value_expr(&item.expression, out);
    }
}

// ── Fallback identifier extractor (regex-style) ─────────────────────────────

/// Split on non-identifier characters, keep only valid Cypher identifier shapes,
/// drop reserved keywords.
fn collect_tokens_fallback(content: &str, out: &mut Vec<String>) {
    for tok in tokenize_identifier_chunks(content) {
        if !is_cypher_keyword(&tok) {
            out.push(tok);
        }
    }
}

fn tokenize_identifier_chunks(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            if is_valid_identifier(&current) {
                tokens.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if !current.is_empty() && is_valid_identifier(&current) {
        tokens.push(current);
    }
    tokens
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_cypher_keyword(s: &str) -> bool {
    matches!(
        s.to_ascii_uppercase().as_str(),
        "MATCH" | "WHERE" | "RETURN" | "WITH" | "AND" | "OR" | "NOT"
        | "AS" | "ORDER" | "BY" | "LIMIT" | "SKIP" | "UNWIND" | "MERGE"
        | "CREATE" | "DELETE" | "DETACH" | "SET" | "REMOVE" | "OPTIONAL"
        | "DISTINCT" | "TRUE" | "FALSE" | "NULL" | "IS" | "IN"
        | "STARTS" | "ENDS" | "CONTAINS" | "EXISTS" | "ASC" | "DESC"
        | "CASE" | "WHEN" | "THEN" | "ELSE" | "END" | "CALL" | "YIELD"
        | "LIKE" | "ILIKE"
    )
}

// ── Hashing ─────────────────────────────────────────────────────────────────

/// Stable codebook index for `tok`: `DefaultHasher(tok.to_lowercase()) % 4096`.
/// Lowercasing makes the mapping case-insensitive (`Person` and `person` map
/// to the same atom).
fn stable_codebook_index(tok: &str) -> u16 {
    let mut hasher = DefaultHasher::new();
    tok.to_ascii_lowercase().hash(&mut hasher);
    (hasher.finish() % CODEBOOK_SIZE as u64) as u16
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_tiers() {
        assert!((confidence_from_name("v40_verified") - 0.92).abs() < 1e-6);
        assert!((confidence_from_name("v31_corrections") - 0.92).abs() < 1e-6);
        assert!((confidence_from_name("v15_patch") - 0.78).abs() < 1e-6);
        assert!((confidence_from_name("v22_allin") - 0.85).abs() < 1e-6);
        assert!((confidence_from_name("v05_full") - 0.70).abs() < 1e-6);
        assert!((confidence_from_name("v05_enriched") - 0.70).abs() < 1e-6);
        assert!((confidence_from_name("v00_baseline") - 0.65).abs() < 1e-6);
    }

    #[test]
    fn preview_skips_blank_and_comments() {
        let txt = "\n// header comment\n\nMATCH (n:Person) RETURN n\n";
        assert_eq!(cypher_preview(txt), "MATCH (n:Person) RETURN n");
    }

    #[test]
    fn preview_caps_at_120_chars() {
        let long = "MATCH ".to_string() + &"a".repeat(500);
        assert_eq!(cypher_preview(&long).chars().count(), 120);
    }

    #[test]
    fn stable_hash_is_deterministic() {
        let a = stable_codebook_index("Person");
        let b = stable_codebook_index("Person");
        assert_eq!(a, b);
        assert!((a as u32) < CODEBOOK_SIZE);
    }

    #[test]
    fn stable_hash_is_case_insensitive() {
        assert_eq!(stable_codebook_index("Person"), stable_codebook_index("person"));
        assert_eq!(stable_codebook_index("KNOWS"), stable_codebook_index("knows"));
    }

    #[test]
    fn cypher_to_stream_real_query() {
        let cypher = "MATCH (p:Person)-[r:KNOWS]->(c:City) WHERE p.age > 30 RETURN p.name, c.name";
        let stream = cypher_to_stream(cypher, 12345);
        assert_eq!(stream.source, "AriGraph");
        assert_eq!(stream.timestamp, 12345);
        assert!(!stream.codebook_indices.is_empty(), "expected indices for Person/KNOWS/City/age/name");
        // All indices in range.
        for idx in &stream.codebook_indices {
            assert!((*idx as u32) < CODEBOOK_SIZE);
        }
        // Person and KNOWS should produce DIFFERENT indices with overwhelming probability.
        let person_idx = stable_codebook_index("Person");
        let knows_idx = stable_codebook_index("KNOWS");
        assert!(stream.codebook_indices.contains(&person_idx));
        assert!(stream.codebook_indices.contains(&knows_idx));
    }

    #[test]
    fn cypher_to_stream_falls_back_on_parse_error() {
        // Malformed Cypher — fallback should still extract identifiers.
        let bad = "this is not :: valid cypher but Person KNOWS City should still extract";
        let stream = cypher_to_stream(bad, 99);
        assert_eq!(stream.source, "AriGraph");
        assert!(!stream.codebook_indices.is_empty());
        assert!(stream.codebook_indices.contains(&stable_codebook_index("Person")));
        assert!(stream.codebook_indices.contains(&stable_codebook_index("City")));
    }

    #[test]
    fn cypher_to_stream_dedupes_indices() {
        // Same identifier appearing many times should produce one index.
        let cypher = "MATCH (a:Foo)-[:R]->(b:Foo)-[:R]->(c:Foo) RETURN a, b, c";
        let stream = cypher_to_stream(cypher, 1);
        let foo_idx = stable_codebook_index("Foo");
        let count = stream.codebook_indices.iter().filter(|i| **i == foo_idx).count();
        assert_eq!(count, 1, "Foo should appear exactly once after dedup");
    }

    #[test]
    fn keyword_filter_excludes_reserved() {
        assert!(is_cypher_keyword("MATCH"));
        assert!(is_cypher_keyword("match"));
        assert!(is_cypher_keyword("Where"));
        assert!(!is_cypher_keyword("Person"));
    }

    #[test]
    fn discover_acts_handles_missing_dir() {
        let acts = discover_acts("/nonexistent/path/that/does/not/exist");
        assert!(acts.is_empty());
    }
}
