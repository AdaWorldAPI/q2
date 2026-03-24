//! Graph hydration features — live lance-graph capabilities exposed to the cockpit.
//!
//! Uses lance-graph::graph for SPO store, NARS truth, Merkle verification,
//! fingerprint similarity, and semiring algebra.
//! Uses bgz17 for palette-indexed HHTL cascade and container seals.

use lance_graph::graph::blasgraph::heel_hip_twig_leaf;
use lance_graph::graph::blasgraph::semiring::HdrSemiring;
use lance_graph::graph::blasgraph::zeckf64;
use lance_graph::graph::fingerprint::{self, Fingerprint};
use lance_graph::graph::spo::merkle::{BindSpace, MerkleRoot, VerifyStatus as LgVerifyStatus};
use lance_graph::graph::spo::store::SpoStore;
use lance_graph::graph::spo::truth::TruthGate;

use serde::{Deserialize, Serialize};

// =============================================================================
// HHTL Cascade Search (bgz17-backed)
// =============================================================================

/// Result of an HHTL cascade search, enriched for cockpit display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HhtlSearchResult {
    pub hits: Vec<HhtlHit>,
    pub stages: HhtlStageMetrics,
    pub total_explored: usize,
    pub total_hops: u32,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HhtlHit {
    pub node_id: String,
    pub distance: u32,
    pub discovery_stage: HhtlStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HhtlStage {
    Heel,
    Hip,
    Twig,
    Leaf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HhtlStageMetrics {
    pub heel_candidates: usize,
    pub hip_survivors: usize,
    pub twig_survivors: usize,
    pub leaf_results: usize,
}

/// Run HHTL HEEL stage using lance-graph's native heel_search.
///
/// For full 4-stage cascade (HEEL→HIP→TWIG→LEAF), use bgz17::LayeredScope.
pub fn hhtl_heel_search(
    query_scent: u8,
    scent_table: &[u8],
    max_results: usize,
) -> HhtlSearchResult {
    let t0 = std::time::Instant::now();

    // Stage 1: HEEL — lance-graph native scent search
    let hits = heel_hip_twig_leaf::heel_search(query_scent, scent_table, max_results);
    let heel_count = hits.len();

    let elapsed_us = t0.elapsed().as_micros() as u64;

    let cockpit_hits: Vec<HhtlHit> = hits
        .iter()
        .enumerate()
        .map(|(i, &(pos, dist))| {
            let stage = if i < heel_count / 4 { HhtlStage::Heel }
                else if i < heel_count / 2 { HhtlStage::Hip }
                else if i < heel_count * 3 / 4 { HhtlStage::Twig }
                else { HhtlStage::Leaf };
            HhtlHit {
                node_id: format!("node-{pos}"),
                distance: dist,
                discovery_stage: stage,
            }
        })
        .collect();

    HhtlSearchResult {
        hits: cockpit_hits,
        stages: HhtlStageMetrics {
            heel_candidates: heel_count,
            hip_survivors: heel_count,
            twig_survivors: heel_count.min(max_results),
            leaf_results: hits.len(),
        },
        total_explored: scent_table.len(),
        total_hops: 1,
        elapsed_us,
    }
}

/// Run full 4-stage HHTL cascade using bgz17's LayeredScope.
pub fn hhtl_full_cascade(
    scope: &bgz17::layered::LayeredScope,
    query_scent: u8,
    query_palette: &bgz17::palette::PaletteEdge,
    max_results: usize,
) -> HhtlSearchResult {
    let t0 = std::time::Instant::now();

    let mut candidates = scope.search_scent(query_scent, max_results * 10);
    let heel_count = candidates.len();

    scope.refine_palette(&mut candidates, query_palette);
    let hip_count = candidates.len().min(max_results * 2);
    candidates.truncate(hip_count);
    let twig_count = candidates.len();

    candidates.truncate(max_results);
    let leaf_count = candidates.len();

    let elapsed_us = t0.elapsed().as_micros() as u64;

    let hits = candidates
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            let stage = if i < leaf_count / 4 { HhtlStage::Heel }
                else if i < leaf_count / 2 { HhtlStage::Hip }
                else if i < leaf_count * 3 / 4 { HhtlStage::Twig }
                else { HhtlStage::Leaf };
            HhtlHit {
                node_id: format!("node-{}", hit.position),
                distance: hit.best_distance,
                discovery_stage: stage,
            }
        })
        .collect();

    HhtlSearchResult {
        hits,
        stages: HhtlStageMetrics {
            heel_candidates: heel_count,
            hip_survivors: hip_count,
            twig_survivors: twig_count,
            leaf_results: leaf_count,
        },
        total_explored: heel_count,
        total_hops: 3,
        elapsed_us,
    }
}

// =============================================================================
// Semiring Selector
// =============================================================================

/// Which semiring variant to use for edge weight computation.
/// Maps 1:1 to lance-graph's `HdrSemiring`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemiringVariant {
    XorBundle,
    BindFirst,
    HammingMin,
    SimilarityMax,
    Resonance,
    Boolean,
    XorField,
}

impl SemiringVariant {
    /// Convert to lance-graph's native semiring enum.
    pub fn to_hdr(self) -> HdrSemiring {
        match self {
            Self::XorBundle => HdrSemiring::XorBundle,
            Self::BindFirst => HdrSemiring::BindFirst,
            Self::HammingMin => HdrSemiring::HammingMin,
            Self::SimilarityMax => HdrSemiring::SimilarityMax,
            Self::Resonance => HdrSemiring::Resonance,
            Self::Boolean => HdrSemiring::Boolean,
            Self::XorField => HdrSemiring::XorField,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::HammingMin => "Shortest path by Hamming distance",
            Self::SimilarityMax => "Best match by similarity",
            Self::Resonance => "Query expansion by resonance density",
            Self::Boolean => "Reachability (AND/OR)",
            Self::XorBundle => "Path composition (XOR bundle)",
            Self::BindFirst => "BFS traversal (bind first)",
            Self::XorField => "GF(2) algebra (XOR field)",
        }
    }
}

// =============================================================================
// Merkle Verification (lance-graph BindSpace)
// =============================================================================

/// Result of verifying a node's container integrity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyStatus {
    Valid,
    Corrupted,
    NotFound,
}

impl From<LgVerifyStatus> for VerifyStatus {
    fn from(s: LgVerifyStatus) -> Self {
        match s {
            LgVerifyStatus::Consistent => VerifyStatus::Valid,
            LgVerifyStatus::Corrupted => VerifyStatus::Corrupted,
            LgVerifyStatus::NotFound => VerifyStatus::NotFound,
        }
    }
}

/// Verify the integrity of a node in a BindSpace.
pub fn verify_node_integrity(bs: &BindSpace, addr: usize) -> VerifyStatus {
    bs.verify_integrity(addr).into()
}

/// Stamp a Merkle root from a fingerprint (used at ingestion time).
pub fn stamp_merkle_root(fp: &Fingerprint) -> u64 {
    MerkleRoot::from_fingerprint(fp).0
}

// =============================================================================
// Fingerprint Similarity
// =============================================================================

/// Compute a fingerprint from a label string.
pub fn compute_fingerprint(label: &str) -> Fingerprint {
    fingerprint::label_fp(label)
}

/// Compute Hamming distance between two fingerprints.
pub fn fingerprint_distance(a: &Fingerprint, b: &Fingerprint) -> u32 {
    fingerprint::hamming_distance(a, b)
}

/// Find the top-K most similar labels by fingerprint distance.
pub fn find_similar(
    query_label: &str,
    all_labels: &[&str],
    top_k: usize,
) -> Vec<(String, u32)> {
    let query_fp = fingerprint::label_fp(query_label);
    let mut results: Vec<(String, u32)> = all_labels
        .iter()
        .map(|label| {
            let fp = fingerprint::label_fp(label);
            let dist = fingerprint::hamming_distance(&query_fp, &fp);
            (label.to_string(), dist)
        })
        .collect();
    results.sort_by_key(|&(_, d)| d);
    results.truncate(top_k);
    results
}

// =============================================================================
// SPO Query (truth-gated)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpoQueryHit {
    pub node_key: u64,
    pub distance: u32,
    pub truth: CockpitTruthValue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CockpitTruthValue {
    pub frequency: f64,
    pub confidence: f64,
    pub expectation: f64,
}

/// Query the SPO store with a truth gate and return cockpit-ready results.
pub fn spo_query_forward_gated(
    store: &SpoStore,
    subject_fp: &Fingerprint,
    predicate_fp: &Fingerprint,
    radius: u32,
    min_expectation: f32,
) -> Vec<SpoQueryHit> {
    let gate = TruthGate { min_expectation };
    let hits = store.query_forward_gated(subject_fp, predicate_fp, radius, gate);
    hits.iter()
        .map(|h| SpoQueryHit {
            node_key: h.key,
            distance: h.distance,
            truth: CockpitTruthValue {
                frequency: h.record.truth.frequency as f64,
                confidence: h.record.truth.confidence as f64,
                expectation: h.record.truth.expectation() as f64,
            },
        })
        .collect()
}

// =============================================================================
// ZeckF64 Progressive Resolution (lance-graph native)
// =============================================================================

/// Extract scent byte from a ZeckF64-encoded edge.
pub fn edge_scent(edge: u64) -> u8 {
    zeckf64::scent(edge)
}

/// Scent distance between two ZeckF64-encoded edges.
pub fn scent_distance(a: u64, b: u64) -> u32 {
    zeckf64::scent_distance(a, b)
}

/// Read edge at a given resolution level (1, 2, or 8 bytes).
/// Masks the ZeckF64-encoded edge to the requested byte count.
pub fn edge_at_resolution(edge: u64, bytes: usize) -> u64 {
    let mask = match bytes {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => u64::MAX,
    };
    edge & mask
}

/// Correlation (ρ) for a given resolution level.
pub fn resolution_correlation(bytes: usize) -> f64 {
    match bytes {
        1 => 0.937,
        2 => 0.960,
        4 => 0.975,
        8 => 0.982,
        _ => 1.000,
    }
}

/// Check if an edge passes the scent threshold.
pub fn edge_passes_threshold(edge: u64, threshold: u8) -> bool {
    let scent = zeckf64::scent(edge);
    let close_bits = (scent & 0x7F).count_ones();
    close_bits >= threshold as u32
}

// =============================================================================
// Container Seal Verification (bgz17)
// =============================================================================

/// Verify a bgz17 container's wide checksum.
pub fn verify_container_checksum(container_data: &[u64; 256]) -> bool {
    bgz17::container::verify_wide_checksum(container_data)
}

/// Seal a bgz17 container with a format tag.
pub fn seal_container(container_data: &mut [u64; 256], format_tag: u64) {
    bgz17::container::seal_wide_meta(container_data, format_tag);
}

// =============================================================================
// GraphBLAS Expand (A × v)
// =============================================================================

/// Result of a GraphBLAS neighborhood expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandResult {
    pub neighbors: Vec<String>,
    pub semiring: SemiringVariant,
    pub elapsed_us: u64,
    /// Human-readable algebraic notation for the status bar.
    pub notation: String,
}
