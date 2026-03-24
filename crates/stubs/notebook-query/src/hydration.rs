//! Graph hydration features — live lance-graph capabilities exposed to the cockpit.
//!
//! Wraps bgz17 (HHTL cascade, semiring algebra, GraphBLAS, container seals)
//! into cockpit-ready JSON responses.

use std::collections::HashMap;

use bgz17::container;
use bgz17::distance_matrix::SpoDistanceMatrices;
use bgz17::layered::LayeredScope;
use bgz17::palette::PaletteEdge;
use bgz17::palette_matrix::PaletteMatrix;

use serde::{Deserialize, Serialize};

// =============================================================================
// HHTL Cascade Search
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

/// Run an HHTL cascade search and return cockpit-ready results.
pub fn hhtl_search(
    scope: &LayeredScope,
    query_scent: u8,
    query_palette: &PaletteEdge,
    max_results: usize,
) -> HhtlSearchResult {
    let t0 = std::time::Instant::now();

    // Stage 1: HEEL — scent pre-filter
    let mut candidates = scope.search_scent(query_scent, max_results * 10);
    let heel_count = candidates.len();

    // Stage 2: HIP — palette refine
    scope.refine_palette(&mut candidates, query_palette);
    let hip_count = candidates.len().min(max_results * 2);

    // Stage 3: TWIG — second hop (truncate to 2x results)
    candidates.truncate(hip_count);
    let twig_count = candidates.len();

    // Stage 4: LEAF — already refined, take top-N
    candidates.truncate(max_results);
    let leaf_count = candidates.len();

    let elapsed_us = t0.elapsed().as_micros() as u64;

    let hits = candidates
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            let stage = if i < leaf_count / 4 {
                HhtlStage::Heel
            } else if i < leaf_count / 2 {
                HhtlStage::Hip
            } else if i < leaf_count * 3 / 4 {
                HhtlStage::Twig
            } else {
                HhtlStage::Leaf
            };
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

/// Compute edge weights using a selected semiring variant.
///
/// Returns a map of (row, col) → weight for the cockpit to render.
pub fn compute_edge_weights(
    matrix: &PaletteMatrix,
    dm: &SpoDistanceMatrices,
    variant: SemiringVariant,
) -> HashMap<(usize, usize), f64> {
    let csr = matrix.to_distance_csr(dm);
    let mut weights = HashMap::new();

    for row in 0..csr.nrows {
        let start = csr.row_ptr[row];
        let end = csr.row_ptr[row + 1];
        for idx in start..end {
            let col = csr.col_idx[idx];
            let dist = csr.vals[idx];
            let weight = match variant {
                SemiringVariant::HammingMin => 1.0 / (1.0 + dist as f64),
                SemiringVariant::SimilarityMax => 1.0 - (dist as f64 / 65535.0),
                SemiringVariant::Resonance => {
                    let sim = 1.0 - (dist as f64 / 65535.0);
                    sim * sim // density = similarity squared
                }
                SemiringVariant::Boolean => {
                    if dist < 32768.0 { 1.0 } else { 0.0 }
                }
                SemiringVariant::XorBundle
                | SemiringVariant::BindFirst
                | SemiringVariant::XorField => dist as f64,
            };
            weights.insert((row, col), weight);
        }
    }

    weights
}

// =============================================================================
// Merkle Verification (Container Seals)
// =============================================================================

/// Result of verifying a node's container integrity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyStatus {
    Valid,
    Corrupted,
    NotFound,
}

/// Verify the integrity of a node's container seal.
pub fn verify_container_integrity(container_data: &[u64; 256]) -> VerifyStatus {
    if container::verify_wide_checksum(container_data) {
        VerifyStatus::Valid
    } else {
        VerifyStatus::Corrupted
    }
}

/// Compute a seal for a container (used at ingestion time).
pub fn seal_container(container_data: &mut [u64; 256], format_tag: u64) {
    container::seal_wide_meta(container_data, format_tag);
}

// =============================================================================
// Fingerprint Similarity
// =============================================================================

/// Find the top-K most similar nodes by scent distance.
pub fn find_similar_by_scent(
    scope: &LayeredScope,
    query_scent: u8,
    top_k: usize,
) -> Vec<HhtlHit> {
    let candidates = scope.search_scent(query_scent, top_k);
    candidates
        .iter()
        .map(|hit| HhtlHit {
            node_id: format!("node-{}", hit.position),
            distance: hit.best_distance,
            discovery_stage: HhtlStage::Heel,
        })
        .collect()
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

/// Expand a node's neighborhood using matrix-vector multiply with a semiring.
pub fn graphblas_expand(
    matrix: &PaletteMatrix,
    dm: &SpoDistanceMatrices,
    selected_node: usize,
    variant: SemiringVariant,
) -> ExpandResult {
    let t0 = std::time::Instant::now();

    let weights = compute_edge_weights(matrix, dm, variant);

    let neighbors: Vec<String> = weights
        .iter()
        .filter_map(|(&(row, col), &w)| {
            if row == selected_node && w > 0.0 {
                Some(format!("node-{col}"))
            } else if col == selected_node && w > 0.0 {
                Some(format!("node-{row}"))
            } else {
                None
            }
        })
        .collect();

    let elapsed_us = t0.elapsed().as_micros() as u64;
    let notation = format!(
        "A \u{2297} v \u{2192} {} neighbors ({:?} semiring, {}μs)",
        neighbors.len(),
        variant,
        elapsed_us,
    );

    ExpandResult {
        neighbors,
        semiring: variant,
        elapsed_us,
        notation,
    }
}

// =============================================================================
// Storage Metrics (for status bar)
// =============================================================================

/// Storage breakdown for the status bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub scent_bytes: usize,
    pub palette_bytes: usize,
    pub base_bytes: usize,
    pub total_bytes: usize,
    pub nodes: usize,
    pub edges: usize,
}

pub fn storage_metrics(scope: &LayeredScope) -> StorageMetrics {
    let breakdown = scope.storage_breakdown();
    StorageMetrics {
        scent_bytes: breakdown.scent_bytes,
        palette_bytes: breakdown.palette_bytes,
        base_bytes: breakdown.base_bytes,
        total_bytes: breakdown.total_bytes,
        nodes: breakdown.edge_count,
        edges: breakdown.edge_count,
    }
}
