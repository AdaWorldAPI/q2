//! Stable token-to-codebook-index mapping.
//!
//! Provides a deterministic mapping from string tokens to `u16` codebook
//! indices in the range `[0, CODEBOOK_SIZE)`. Used by the scene player and
//! shader stream to translate Cypher identifiers (and similar tokens) into
//! the codebook space consumed by the thinking engine.
//!
//! NOTE: `CODEBOOK_SIZE` here MUST match
//! `lance_graph::thinking_engine::engine::CODEBOOK_SIZE` (currently 4096).
//! If the upstream value changes, update this constant in lockstep.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// Codebook width. Mirrors `thinking_engine::engine::CODEBOOK_SIZE`.
///
/// Both this crate and `lance-graph`'s thinking engine assume the same
/// codebook width when exchanging `u16` indices. Keep in sync.
pub const CODEBOOK_SIZE: usize = 4096;

/// Synthetic placeholder distance table for `ThinkingEngine::new`.
///
/// **PHASE 2A STUB.** The real codebook should come from a Jina v5 (or
/// equivalent) precomputed distance matrix loaded from disk via mmap.
/// Until that loader exists, we hand the engine an all-zero table so it
/// can boot. Every distance is identity → all atoms equidistant → the
/// resonance field will reflect only the codebook indices that perturb
/// fired into it. This is a deliberate degraded mode; the
/// DiagnosticsOverlay should surface it as `SYNTHETIC` mode.
///
/// Cost: 4096 × 4096 × 1 byte = 16 MB. Allocated once per cockpit-server
/// boot (per SSE connection in the current handler).
pub fn default_distance_table() -> Vec<u8> {
    vec![0u8; CODEBOOK_SIZE * CODEBOOK_SIZE]
}

/// Cypher keywords excluded from identifier extraction.
const CYPHER_KEYWORDS: &[&str] = &[
    "MATCH", "RETURN", "WHERE", "CREATE", "MERGE", "SET", "DELETE", "AS",
    "AND", "OR", "NOT", "NULL", "TRUE", "FALSE", "OPTIONAL", "WITH",
    "UNWIND", "ORDER", "BY", "ASC", "DESC", "LIMIT", "SKIP", "COUNT",
    "COLLECT", "DISTINCT",
];

/// Maximum identifiers returned by [`extract_cypher_identifiers`] per call.
const MAX_IDENTIFIERS_PER_CALL: usize = 32;

/// Map a token to a stable `u16` codebook index in `[0, CODEBOOK_SIZE)`.
///
/// Uses `DefaultHasher` and reduces modulo `CODEBOOK_SIZE`. The mapping is
/// deterministic within a single process run; callers must not rely on
/// stability across Rust toolchain versions, since `DefaultHasher` is not
/// guaranteed stable across releases.
pub fn token_to_index(token: &str) -> u16 {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    let h = hasher.finish();
    (h % CODEBOOK_SIZE as u64) as u16
}

/// Convert a slice of tokens into deduplicated codebook indices.
///
/// Preserves first-occurrence order: if two distinct tokens hash to the same
/// index, both appear once in input order, but duplicate tokens collapse to
/// a single index in the output.
pub fn tokens_to_indices(tokens: &[&str]) -> Vec<u16> {
    let mut seen: HashSet<&str> = HashSet::with_capacity(tokens.len());
    let mut out: Vec<u16> = Vec::with_capacity(tokens.len());
    for tok in tokens {
        if seen.insert(*tok) {
            out.push(token_to_index(tok));
        }
    }
    out
}

/// Extract identifier-like tokens from a Cypher snippet.
///
/// Splits `content` on any non-`[A-Za-z0-9_]` character, keeps tokens that
/// start with an ASCII letter and have length >= 2, drops Cypher keywords
/// (case-insensitive), deduplicates while preserving first-occurrence
/// order, and caps the output at [`MAX_IDENTIFIERS_PER_CALL`] entries.
pub fn extract_cypher_identifiers(content: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for raw in content.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if out.len() >= MAX_IDENTIFIERS_PER_CALL {
            break;
        }
        if raw.len() < 2 {
            continue;
        }
        let first = match raw.chars().next() {
            Some(c) => c,
            None => continue,
        };
        if !first.is_ascii_alphabetic() {
            continue;
        }
        let upper = raw.to_ascii_uppercase();
        if CYPHER_KEYWORDS.iter().any(|kw| *kw == upper) {
            continue;
        }
        if seen.insert(raw.to_string()) {
            out.push(raw.to_string());
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_to_index_is_stable_within_run() {
        let a = token_to_index("Person");
        let b = token_to_index("Person");
        assert_eq!(a, b, "same token must yield same index within a run");
        assert!((a as usize) < CODEBOOK_SIZE);
    }

    #[test]
    fn token_to_index_in_range() {
        for tok in &["", "a", "Person", "Movie", "ACTED_IN", "node_42", "x"] {
            let idx = token_to_index(tok);
            assert!(
                (idx as usize) < CODEBOOK_SIZE,
                "index for {:?} out of range: {}",
                tok,
                idx
            );
        }
    }

    #[test]
    fn tokens_to_indices_dedupes_preserving_first_occurrence() {
        let toks = ["a", "b", "a", "c", "b"];
        let out = tokens_to_indices(&toks);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], token_to_index("a"));
        assert_eq!(out[1], token_to_index("b"));
        assert_eq!(out[2], token_to_index("c"));
    }

    #[test]
    fn extract_cypher_identifiers_basic() {
        let q = "MATCH (p:Person)-[:ACTED_IN]->(m:Movie) RETURN p, m";
        let ids = extract_cypher_identifiers(q);
        // Keywords filtered out; identifiers and labels kept.
        assert!(ids.iter().any(|s| s == "Person"));
        assert!(ids.iter().any(|s| s == "ACTED_IN"));
        assert!(ids.iter().any(|s| s == "Movie"));
        assert!(!ids.iter().any(|s| s.eq_ignore_ascii_case("MATCH")));
        assert!(!ids.iter().any(|s| s.eq_ignore_ascii_case("RETURN")));
    }

    #[test]
    fn extract_cypher_identifiers_filters_short_and_numeric() {
        // "a" is 1 char (dropped), "1node" starts with digit (dropped),
        // "node_2" is kept.
        let q = "a 1node node_2 _under";
        let ids = extract_cypher_identifiers(q);
        assert!(!ids.iter().any(|s| s == "a"));
        assert!(!ids.iter().any(|s| s == "1node"));
        assert!(!ids.iter().any(|s| s == "_under"));
        assert!(ids.iter().any(|s| s == "node_2"));
    }

    #[test]
    fn extract_cypher_identifiers_dedupes_and_caps() {
        // Generate >32 unique identifiers; ensure the cap holds.
        let mut q = String::new();
        for i in 0..50 {
            q.push_str(&format!("ident{} ", i));
        }
        // Add duplicates to confirm dedup runs before the cap.
        q.push_str("ident0 ident1 ident2");
        let ids = extract_cypher_identifiers(&q);
        assert!(ids.len() <= MAX_IDENTIFIERS_PER_CALL);
        assert_eq!(ids.len(), MAX_IDENTIFIERS_PER_CALL);
        // Dedup check: no repeats.
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }
}
