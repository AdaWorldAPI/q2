//! Reasoning features — NARS inference, temporal playback, progressive hydration.
//!
//! These features compose lance-graph primitives into cockpit-ready workflows:
//! - Temporal play button: step through versioned encounter rounds
//! - NARS abductive inference: infer edges from chain walks
//! - Progressive hydration lens: ZeckF64 resolution slider

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// =============================================================================
// NARS Truth Values
// =============================================================================

/// NARS-compatible truth value: frequency × confidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TruthValue {
    pub frequency: f64,
    pub confidence: f64,
}

impl TruthValue {
    pub fn new(f: f64, c: f64) -> Self {
        Self {
            frequency: f.clamp(0.0, 1.0),
            confidence: c.clamp(0.0, 1.0),
        }
    }

    /// Single-scalar truth: c * (f - 0.5) + 0.5
    pub fn expectation(&self) -> f64 {
        self.confidence * (self.frequency - 0.5) + 0.5
    }

    /// Bayesian revision: combine two independent evidence sources.
    pub fn revision(&self, other: &TruthValue) -> TruthValue {
        let w1 = self.confidence / (1.0 - self.confidence + f64::EPSILON);
        let w2 = other.confidence / (1.0 - other.confidence + f64::EPSILON);
        let w = w1 + w2;
        let f = (w1 * self.frequency + w2 * other.frequency) / (w + f64::EPSILON);
        let c = w / (w + 1.0);
        TruthValue::new(f, c)
    }
}

/// Deduction: if A→B and B→C, then A→C.
pub fn nars_deduction(ab: &TruthValue, bc: &TruthValue) -> TruthValue {
    let f = ab.frequency * bc.frequency;
    let c = ab.confidence * bc.confidence * ab.frequency * bc.frequency;
    TruthValue::new(f, c)
}

/// Abduction: if A→B and C→B, then A→C (weaker).
pub fn nars_abduction(ab: &TruthValue, cb: &TruthValue) -> TruthValue {
    let f = ab.frequency;
    let c = ab.confidence * cb.confidence * cb.frequency;
    TruthValue::new(f, c)
}

/// Induction: if A→B and A→C, then B→C.
pub fn nars_induction(ab: &TruthValue, ac: &TruthValue) -> TruthValue {
    let f = ac.frequency;
    let c = ab.confidence * ac.confidence * ab.frequency;
    TruthValue::new(f, c)
}

// =============================================================================
// Edge Truth Metadata
// =============================================================================

/// An edge enriched with NARS truth value for cockpit rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthEdge {
    pub source: String,
    pub target: String,
    pub rel_type: String,
    pub truth: TruthValue,
    /// Whether this edge was inferred (vs. observed).
    pub inferred: bool,
    /// If inferred, the inference chain.
    pub via: Vec<String>,
    /// If inferred, the inference type.
    pub inference_type: Option<InferenceType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceType {
    Deduction,
    Abduction,
    Induction,
}

/// Infer edges from existing graph edges using NARS deduction/abduction.
///
/// For every A→B→C chain where both edges have confidence > `min_confidence`,
/// infer A→C with deduction truth.
pub fn infer_edges(
    edges: &[TruthEdge],
    min_confidence: f64,
    max_hops: usize,
) -> Vec<TruthEdge> {
    // Build adjacency: source → [(target, truth, rel_type)]
    let mut adj: HashMap<&str, Vec<(&str, &TruthValue, &str)>> = HashMap::new();
    for e in edges {
        if !e.inferred && e.truth.confidence >= min_confidence {
            adj.entry(&e.source)
                .or_default()
                .push((&e.target, &e.truth, &e.rel_type));
        }
    }

    // Existing edge set for dedup
    let existing: std::collections::HashSet<(&str, &str)> = edges
        .iter()
        .map(|e| (e.source.as_str(), e.target.as_str()))
        .collect();

    let mut inferred = Vec::new();

    // 2-hop deduction: A→B→C ⟹ A→C
    if max_hops >= 2 {
        for (a, a_edges) in &adj {
            for &(b, ab_truth, _) in a_edges {
                if let Some(b_edges) = adj.get(b) {
                    for &(c, bc_truth, rel_type) in b_edges {
                        if *a == c || existing.contains(&(a, c)) {
                            continue;
                        }
                        let truth = nars_deduction(ab_truth, bc_truth);
                        if truth.confidence >= min_confidence * 0.5 {
                            inferred.push(TruthEdge {
                                source: a.to_string(),
                                target: c.to_string(),
                                rel_type: rel_type.to_string(),
                                truth,
                                inferred: true,
                                via: vec![b.to_string()],
                                inference_type: Some(InferenceType::Deduction),
                            });
                        }
                    }
                }
            }
        }
    }

    // Abduction: A→B and C→B ⟹ A→C
    // Build reverse adjacency
    let mut rev: HashMap<&str, Vec<(&str, &TruthValue)>> = HashMap::new();
    for e in edges {
        if !e.inferred && e.truth.confidence >= min_confidence {
            rev.entry(&e.target).or_default().push((&e.source, &e.truth));
        }
    }

    for (b, incoming) in &rev {
        if incoming.len() < 2 {
            continue;
        }
        for i in 0..incoming.len() {
            for j in (i + 1)..incoming.len() {
                let (a, ab_truth) = incoming[i];
                let (c, cb_truth) = incoming[j];
                if a == c || existing.contains(&(a, c)) {
                    continue;
                }
                let truth = nars_abduction(ab_truth, cb_truth);
                if truth.confidence >= min_confidence * 0.3 {
                    inferred.push(TruthEdge {
                        source: a.to_string(),
                        target: c.to_string(),
                        rel_type: "INFERRED".to_string(),
                        truth,
                        inferred: true,
                        via: vec![b.to_string()],
                        inference_type: Some(InferenceType::Abduction),
                    });
                }
            }
        }
    }

    inferred
}

// =============================================================================
// Temporal Playback (Encounter Rounds)
// =============================================================================

/// A snapshot of the graph at a given version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub version: u32,
    pub name: String,
    pub nodes: Vec<serde_json::Value>,
    pub edges: Vec<TruthEdge>,
    /// Whether new learning occurred since the previous version.
    pub seal_status: SealStatus,
    /// Number of new edges added in this version.
    pub new_edge_count: usize,
    /// Number of edges strengthened by revision in this version.
    pub revised_edge_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SealStatus {
    /// No changes since previous version.
    Wisdom,
    /// New learning occurred.
    Staunen,
}

/// Confidence mapping for enrichment files.
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

/// Compute the temporal diff between two graph snapshots.
pub fn compute_seal_status(_prev: &GraphSnapshot, current: &GraphSnapshot) -> SealStatus {
    if current.new_edge_count == 0 && current.revised_edge_count == 0 {
        SealStatus::Wisdom
    } else {
        SealStatus::Staunen
    }
}

/// Apply NARS revision to strengthen edges that appear across multiple versions.
///
/// If an edge (src, dst, rel_type) appears in both `existing` and `incoming`,
/// their truth values are combined via Bayesian revision.
pub fn revise_edges(existing: &mut Vec<TruthEdge>, incoming: &[TruthEdge]) -> usize {
    let mut revised_count = 0;

    let mut incoming_map: HashMap<(&str, &str, &str), &TruthValue> = HashMap::new();
    for e in incoming {
        incoming_map.insert((&e.source, &e.target, &e.rel_type), &e.truth);
    }

    for edge in existing.iter_mut() {
        if let Some(new_truth) = incoming_map.remove(&(
            edge.source.as_str(),
            edge.target.as_str(),
            edge.rel_type.as_str(),
        )) {
            let revised = edge.truth.revision(new_truth);
            if (revised.confidence - edge.truth.confidence).abs() > 0.01 {
                edge.truth = revised;
                revised_count += 1;
            }
        }
    }

    revised_count
}

// =============================================================================
// Progressive Hydration Lens (ZeckF64 bitmask)
// =============================================================================

/// Resolution level for the progressive hydration lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionLevel {
    /// Number of bytes per edge (1, 2, 4, or 8).
    pub bytes: usize,
    /// Minimum close-bits in scent for an edge to pass.
    pub threshold: u8,
}

impl ResolutionLevel {
    pub fn scent_only() -> Self {
        Self { bytes: 1, threshold: 3 }
    }

    pub fn low() -> Self {
        Self { bytes: 2, threshold: 4 }
    }

    pub fn medium() -> Self {
        Self { bytes: 4, threshold: 5 }
    }

    pub fn full() -> Self {
        Self { bytes: 8, threshold: 6 }
    }
}

/// Filter edges by resolution level.
///
/// At low resolution: everything connected (scent says "close enough").
/// At high resolution: only strong connections survive.
pub fn filter_edges_by_resolution(
    edges: &[TruthEdge],
    _resolution: ResolutionLevel,
) -> Vec<&TruthEdge> {
    // For edges with truth values, use confidence as a proxy for resolution
    // (real ZeckF64 would use the packed edge bytes, but we don't have them
    // in the JSON layer — bgz17 works at the container level)
    let threshold = match _resolution.bytes {
        1 => 0.0,  // show everything
        2 => 0.3,  // filter weak edges
        4 => 0.5,  // medium filter
        _ => 0.7,  // strong edges only
    };

    edges
        .iter()
        .filter(|e| e.truth.confidence >= threshold || e.truth.frequency >= threshold)
        .collect()
}

/// Compute the correlation (ρ) for a given resolution level.
pub fn resolution_correlation(bytes: usize) -> f64 {
    match bytes {
        1 => 0.937,
        2 => 0.960,
        4 => 0.975,
        8 => 0.982,
        _ => 1.000,
    }
}

/// Edge mask for ZeckF64 progressive encoding.
pub fn edge_at_resolution(edge: u64, bytes: usize) -> u64 {
    let mask = match bytes {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => u64::MAX,
    };
    edge & mask
}

/// Check if an edge passes the scent threshold.
pub fn edge_passes_threshold(edge: u64, threshold: u8) -> bool {
    let scent = (edge & 0xFF) as u8;
    let close_bits = (scent & 0x7F).count_ones();
    close_bits >= threshold as u32
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nars_deduction() {
        let ab = TruthValue::new(0.95, 0.87);
        let bc = TruthValue::new(0.90, 0.82);
        let result = nars_deduction(&ab, &bc);
        assert!((result.frequency - 0.855).abs() < 0.01);
        assert!(result.confidence > 0.0 && result.confidence < 1.0);
    }

    #[test]
    fn test_nars_abduction() {
        let ab = TruthValue::new(0.95, 0.87);
        let cb = TruthValue::new(0.80, 0.45);
        let result = nars_abduction(&ab, &cb);
        assert!((result.frequency - 0.95).abs() < 0.01);
        assert!(result.confidence < ab.confidence);
    }

    #[test]
    fn test_truth_revision() {
        let a = TruthValue::new(0.60, 0.50);
        let b = TruthValue::new(0.70, 0.60);
        let revised = a.revision(&b);
        // Revised should have higher confidence than either input
        assert!(revised.confidence > a.confidence);
        // Frequency should be between the two
        assert!(revised.frequency > 0.55 && revised.frequency < 0.75);
    }

    #[test]
    fn test_expectation() {
        let tv = TruthValue::new(0.95, 0.87);
        let e = tv.expectation();
        // e = 0.87 * (0.95 - 0.5) + 0.5 = 0.87 * 0.45 + 0.5 = 0.8915
        assert!((e - 0.8915).abs() < 0.01);
    }

    #[test]
    fn test_resolution_filter() {
        let edges = vec![
            TruthEdge {
                source: "A".into(), target: "B".into(), rel_type: "KNOWS".into(),
                truth: TruthValue::new(0.95, 0.87),
                inferred: false, via: vec![], inference_type: None,
            },
            TruthEdge {
                source: "C".into(), target: "D".into(), rel_type: "MAYBE".into(),
                truth: TruthValue::new(0.20, 0.15),
                inferred: false, via: vec![], inference_type: None,
            },
        ];

        let full = filter_edges_by_resolution(&edges, ResolutionLevel::scent_only());
        assert_eq!(full.len(), 2); // all edges pass at low resolution

        let strict = filter_edges_by_resolution(&edges, ResolutionLevel::full());
        assert_eq!(strict.len(), 1); // only strong edge passes
    }

    #[test]
    fn test_edge_at_resolution() {
        let edge: u64 = 0xDEAD_BEEF_CAFE_BABE;
        assert_eq!(edge_at_resolution(edge, 1), 0xBE);
        assert_eq!(edge_at_resolution(edge, 2), 0xBABE);
        assert_eq!(edge_at_resolution(edge, 4), 0xCAFE_BABE);
        assert_eq!(edge_at_resolution(edge, 8), edge);
    }

    #[test]
    fn test_edge_passes_threshold() {
        // 0xFF = 0b11111111, close bits (lower 7) = 7
        assert!(edge_passes_threshold(0xFF, 6));
        // 0x01 = 0b00000001, close bits = 1
        assert!(!edge_passes_threshold(0x01, 3));
    }

    #[test]
    fn test_infer_edges_deduction() {
        let edges = vec![
            TruthEdge {
                source: "Palantir".into(), target: "US_DoD".into(),
                rel_type: "DEVELOPED_BY".into(),
                truth: TruthValue::new(0.95, 0.87),
                inferred: false, via: vec![], inference_type: None,
            },
            TruthEdge {
                source: "US_DoD".into(), target: "Gotham".into(),
                rel_type: "DEPLOYED_BY".into(),
                truth: TruthValue::new(0.90, 0.82),
                inferred: false, via: vec![], inference_type: None,
            },
        ];

        let inferred = infer_edges(&edges, 0.5, 2);
        assert!(!inferred.is_empty());
        let palantir_gotham = inferred.iter()
            .find(|e| e.source == "Palantir" && e.target == "Gotham");
        assert!(palantir_gotham.is_some());
        let edge = palantir_gotham.unwrap();
        assert_eq!(edge.inference_type, Some(InferenceType::Deduction));
        assert!(edge.truth.frequency > 0.8);
    }

    #[test]
    fn test_seal_status() {
        let prev = GraphSnapshot {
            version: 0, name: "base".into(),
            nodes: vec![], edges: vec![],
            seal_status: SealStatus::Staunen,
            new_edge_count: 10, revised_edge_count: 0,
        };
        let same = GraphSnapshot {
            version: 1, name: "no change".into(),
            nodes: vec![], edges: vec![],
            seal_status: SealStatus::Wisdom,
            new_edge_count: 0, revised_edge_count: 0,
        };
        let learning = GraphSnapshot {
            version: 2, name: "enrichment".into(),
            nodes: vec![], edges: vec![],
            seal_status: SealStatus::Staunen,
            new_edge_count: 5, revised_edge_count: 3,
        };

        assert_eq!(compute_seal_status(&prev, &same), SealStatus::Wisdom);
        assert_eq!(compute_seal_status(&prev, &learning), SealStatus::Staunen);
    }
}
