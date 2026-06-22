//! `/mri` — AGI Brain MRI: plasticity, activation, and NARS reasoning scan.
//!
//! A real-time "functional MRI" of the cognitive pipeline. Shows which thinking
//! styles are active, which NARS inference chains fired, which graph regions
//! have high/low plasticity, and which causal paths are hot vs frozen.
//!
//! Three scan modes:
//! - **Structural**: graph topology, entity count, edge distribution
//! - **Functional**: which pipeline stages activated, latency, throughput
//! - **Diffusion**: NARS inference chains, evidence flow, truth propagation
//!
//! Exposed at `/mri` (full scan) and `/api/mri/scan` (JSON API).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::osint_audit::{osint_registry, OsintCounterSnapshot};
use super::reasoning::{
    nars_abduction, nars_deduction, nars_induction, InferenceType, TruthEdge, TruthValue,
};

// ============================================================================
// Brain Region Model
// ============================================================================

/// A brain "region" — a functional area of the cognitive pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainRegion {
    /// Region name (e.g., "perception", "reasoning", "memory", "action").
    pub name: String,
    /// Current activation level [0.0, 1.0] — how busy this region is.
    pub activation: f32,
    /// Plasticity [0.0, 1.0] — how much this region is learning/changing.
    pub plasticity: f32,
    /// Temperature — how "hot" the region is (call frequency / time window).
    pub temperature: f32,
    /// Sub-regions with their own activation.
    pub sub_regions: Vec<SubRegion>,
}

/// A sub-region within a brain region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubRegion {
    pub name: String,
    pub activation: f32,
    pub calls: u64,
    pub avg_latency_us: u64,
    pub status: RegionStatus,
}

/// Status of a brain region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionStatus {
    /// Actively processing.
    Active,
    /// Idle but responsive.
    Idle,
    /// Learning / adapting.
    Plastic,
    /// Frozen — no longer updating.
    Frozen,
    /// Dead — never activated.
    Dead,
}

// ============================================================================
// NARS Inference Trace
// ============================================================================

/// A single NARS inference step — one deduction/abduction/induction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStep {
    /// Inference type.
    pub rule: String,
    /// Premise A.
    pub premise_a: String,
    /// Premise B.
    pub premise_b: String,
    /// Conclusion.
    pub conclusion: String,
    /// Truth value of the conclusion.
    pub truth: TruthValue,
    /// Confidence gain/loss from this inference.
    pub confidence_delta: f64,
}

/// A complete NARS reasoning chain — multiple steps forming a logical argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    pub id: u32,
    pub steps: Vec<InferenceStep>,
    /// Final truth value at the end of the chain.
    pub final_truth: TruthValue,
    /// Total confidence accumulated.
    pub total_confidence_gain: f64,
    /// Chain depth (number of inference steps).
    pub depth: usize,
}

// ============================================================================
// Plasticity Map
// ============================================================================

/// Per-entity plasticity — how much an entity's truth values have changed recently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityPlasticity {
    pub entity: String,
    /// Number of triplets involving this entity.
    pub triplet_count: usize,
    /// Average truth confidence across triplets.
    pub avg_confidence: f32,
    /// Number of revisions applied to triplets involving this entity.
    pub revisions: u64,
    /// Number of contradictions detected involving this entity.
    pub contradictions: u64,
    /// Plasticity classification.
    pub state: PlasticityState,
}

/// CausalEdge64 plasticity states (bits 49-51).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlasticityState {
    /// Actively learning — confidence changing rapidly.
    Hot,
    /// Stable — confidence settled, occasional updates.
    Warm,
    /// Frozen — high confidence, no recent changes.
    Frozen,
    /// Contradicted — conflicting evidence, needs resolution.
    Conflicted,
}

// ============================================================================
// Thinking Style Activation
// ============================================================================

/// Activation snapshot for one thinking style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingStyleActivation {
    pub style: String,
    pub cluster: String,
    /// How many times this style was selected in the current window.
    pub activations: u64,
    /// Average quality score when this style was used.
    pub avg_quality: f32,
    /// NARS truth value for "this style is effective".
    pub effectiveness_truth: TruthValue,
    /// Neighboring styles that co-activate (from topology edges).
    pub co_activations: Vec<(String, f32)>,
}

// ============================================================================
// Full MRI Scan Result
// ============================================================================

/// The complete Brain MRI — structural + functional + diffusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainMri {
    /// Scan mode that produced this result.
    pub scan_mode: ScanMode,
    /// Timestamp (Unix millis).
    pub timestamp_ms: u64,

    // ── Structural ──
    /// Brain regions with activation levels.
    pub regions: Vec<BrainRegion>,
    /// Total entities in the knowledge graph.
    pub total_entities: usize,
    /// Total active triplets.
    pub total_triplets: usize,

    // ── Functional ──
    /// Pipeline stage activation (from OSINT registry).
    pub pipeline_activation: Vec<(String, OsintCounterSnapshot)>,
    /// Thinking style activation (from topology).
    pub thinking_styles: Vec<ThinkingStyleActivation>,

    // ── Diffusion ──
    /// Active NARS reasoning chains.
    pub reasoning_chains: Vec<ReasoningChain>,
    /// Entity plasticity map.
    pub plasticity_map: Vec<EntityPlasticity>,

    // ── Summary ──
    /// Overall brain health score [0.0, 1.0].
    pub health_score: f32,
    /// Dominant thinking mode.
    pub dominant_mode: String,
    /// Key findings.
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    /// Quick structural scan.
    Structural,
    /// Functional activation scan.
    Functional,
    /// Full diffusion tensor scan (slowest, most detailed).
    Full,
}

// ============================================================================
// Scan Functions
// ============================================================================

/// Run a brain MRI scan.
///
/// Collects activation data from the OSINT registry, builds brain regions
/// from pipeline stages, computes plasticity from graph state, and traces
/// NARS reasoning chains from inferred edges.
pub fn run_brain_mri(
    edges: &[TruthEdge],
    entity_stats: &HashMap<String, EntityStats>,
    thinking_activations: &[(String, String, u64, f32)], // (style, cluster, count, quality)
    scan_mode: ScanMode,
) -> BrainMri {
    let registry = osint_registry();
    let pipeline = registry.snapshot();

    // ── Build brain regions from pipeline stages ──
    let regions = build_brain_regions(&pipeline.stages);

    // ── Thinking style activation ──
    let thinking_styles: Vec<ThinkingStyleActivation> = thinking_activations
        .iter()
        .map(|(style, cluster, count, quality)| {
            let effectiveness = if *count > 0 {
                TruthValue::new(*quality as f64, (*count as f64 / (*count as f64 + 1.0)))
            } else {
                TruthValue::new(0.5, 0.0)
            };
            ThinkingStyleActivation {
                style: style.clone(),
                cluster: cluster.clone(),
                activations: *count,
                avg_quality: *quality,
                effectiveness_truth: effectiveness,
                co_activations: Vec::new(),
            }
        })
        .collect();

    // ── NARS reasoning chains (diffusion scan only) ──
    let reasoning_chains = if scan_mode == ScanMode::Full {
        trace_reasoning_chains(edges)
    } else {
        Vec::new()
    };

    // ── Entity plasticity map ──
    let plasticity_map: Vec<EntityPlasticity> = entity_stats
        .iter()
        .map(|(entity, stats)| {
            let state = if stats.contradictions > 0 {
                PlasticityState::Conflicted
            } else if stats.revisions > 5 {
                PlasticityState::Hot
            } else if stats.avg_confidence > 0.8 {
                PlasticityState::Frozen
            } else {
                PlasticityState::Warm
            };
            EntityPlasticity {
                entity: entity.clone(),
                triplet_count: stats.triplet_count,
                avg_confidence: stats.avg_confidence,
                revisions: stats.revisions,
                contradictions: stats.contradictions,
                state,
            }
        })
        .collect();

    // ── Compute health score ──
    let active_regions = regions.iter().filter(|r| r.activation > 0.1).count();
    let total_regions = regions.len().max(1);
    let region_health = active_regions as f32 / total_regions as f32;

    let conflict_count = plasticity_map
        .iter()
        .filter(|p| p.state == PlasticityState::Conflicted)
        .count();
    let conflict_penalty = (conflict_count as f32 * 0.1).min(0.5);

    let health_score = (region_health - conflict_penalty).clamp(0.0, 1.0);

    // ── Dominant mode ──
    let dominant_mode = thinking_styles
        .iter()
        .max_by_key(|s| s.activations)
        .map(|s| s.style.clone())
        .unwrap_or_else(|| "idle".to_string());

    // ── Findings ──
    let mut findings = Vec::new();
    if conflict_count > 0 {
        findings.push(format!(
            "{} entities have conflicting evidence — run contradiction resolution",
            conflict_count
        ));
    }
    let hot_count = plasticity_map
        .iter()
        .filter(|p| p.state == PlasticityState::Hot)
        .count();
    if hot_count > 0 {
        findings.push(format!("{} entities are actively learning (hot plasticity)", hot_count));
    }
    let frozen_count = plasticity_map
        .iter()
        .filter(|p| p.state == PlasticityState::Frozen)
        .count();
    if frozen_count > entity_stats.len() / 2 {
        findings.push(format!(
            "{}% of entities are frozen — consider new evidence ingestion",
            frozen_count * 100 / entity_stats.len().max(1)
        ));
    }
    if !reasoning_chains.is_empty() {
        let max_depth = reasoning_chains.iter().map(|c| c.depth).max().unwrap_or(0);
        findings.push(format!(
            "{} reasoning chains active, max depth {}",
            reasoning_chains.len(),
            max_depth
        ));
    }
    if findings.is_empty() {
        findings.push("Brain is healthy — all regions within normal parameters".to_string());
    }

    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    BrainMri {
        scan_mode,
        timestamp_ms,
        regions,
        total_entities: entity_stats.len(),
        total_triplets: edges.len(),
        pipeline_activation: pipeline.stages,
        thinking_styles,
        reasoning_chains,
        plasticity_map,
        health_score,
        dominant_mode,
        findings,
    }
}

/// Per-entity statistics for plasticity computation.
#[derive(Debug, Clone)]
pub struct EntityStats {
    pub triplet_count: usize,
    pub avg_confidence: f32,
    pub revisions: u64,
    pub contradictions: u64,
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Map pipeline stages to brain regions.
fn build_brain_regions(stages: &[(String, OsintCounterSnapshot)]) -> Vec<BrainRegion> {
    // Group stages into 4 brain regions
    let perception_stages = ["extraction", "xai_api_call"];
    let reasoning_stages = ["deduction", "contradiction", "revision"];
    let memory_stages = ["episodic_store", "episodic_retrieve", "graph_bfs", "spatial_path"];
    let action_stages = ["refinement", "planning", "classification"];

    let build_region = |name: &str, stage_names: &[&str]| {
        let sub_regions: Vec<SubRegion> = stages
            .iter()
            .filter(|(n, _)| stage_names.contains(&n.as_str()))
            .map(|(name, snap)| {
                let status = if snap.calls == 0 {
                    RegionStatus::Dead
                } else if snap.failures > snap.successes {
                    RegionStatus::Frozen
                } else if snap.triplets_produced > 0 {
                    RegionStatus::Plastic
                } else {
                    RegionStatus::Active
                };
                SubRegion {
                    name: name.clone(),
                    activation: if snap.calls > 0 { 1.0 } else { 0.0 },
                    calls: snap.calls,
                    avg_latency_us: snap.avg_latency_us,
                    status,
                }
            })
            .collect();

        let total_calls: u64 = sub_regions.iter().map(|s| s.calls).sum();
        let active_sub = sub_regions.iter().filter(|s| s.calls > 0).count();
        let activation = if sub_regions.is_empty() {
            0.0
        } else {
            active_sub as f32 / sub_regions.len() as f32
        };

        // Plasticity = proportion of sub-regions that produced new knowledge
        let plastic_sub = sub_regions
            .iter()
            .filter(|s| s.status == RegionStatus::Plastic)
            .count();
        let plasticity = if sub_regions.is_empty() {
            0.0
        } else {
            plastic_sub as f32 / sub_regions.len() as f32
        };

        BrainRegion {
            name: name.to_string(),
            activation,
            plasticity,
            temperature: total_calls as f32 / 100.0, // normalize to ~[0,1] for 100 calls
            sub_regions,
        }
    };

    vec![
        build_region("perception", &perception_stages),
        build_region("reasoning", &reasoning_stages),
        build_region("memory", &memory_stages),
        build_region("action", &action_stages),
    ]
}

/// Trace NARS reasoning chains from inferred edges.
fn trace_reasoning_chains(edges: &[TruthEdge]) -> Vec<ReasoningChain> {
    let inferred: Vec<&TruthEdge> = edges.iter().filter(|e| e.inferred).collect();
    let mut chains = Vec::new();

    for (id, edge) in inferred.iter().enumerate() {
        let rule = match edge.inference_type {
            Some(InferenceType::Deduction) => "deduction",
            Some(InferenceType::Abduction) => "abduction",
            Some(InferenceType::Induction) => "induction",
            None => "unknown",
        };

        let via_str = if edge.via.is_empty() {
            "direct".to_string()
        } else {
            edge.via.join(" → ")
        };

        let step = InferenceStep {
            rule: rule.to_string(),
            premise_a: format!("{} → {}", edge.source, edge.via.first().unwrap_or(&edge.target)),
            premise_b: format!(
                "{} → {}",
                edge.via.last().unwrap_or(&edge.source),
                edge.target
            ),
            conclusion: format!("{} → {} (via {})", edge.source, edge.target, via_str),
            truth: edge.truth,
            confidence_delta: edge.truth.confidence,
        };

        chains.push(ReasoningChain {
            id: id as u32,
            steps: vec![step],
            final_truth: edge.truth,
            total_confidence_gain: edge.truth.confidence,
            depth: 1 + edge.via.len(),
        });
    }

    chains
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_edges() -> Vec<TruthEdge> {
        vec![
            TruthEdge {
                source: "Palantir".into(),
                target: "US_DoD".into(),
                rel_type: "DEVELOPED_BY".into(),
                truth: TruthValue::new(0.95, 0.87),
                inferred: false,
                via: vec![],
                inference_type: None,
            },
            TruthEdge {
                source: "Palantir".into(),
                target: "Gotham".into(),
                rel_type: "DEPLOYED_BY".into(),
                truth: TruthValue::new(0.85, 0.72),
                inferred: true,
                via: vec!["US_DoD".into()],
                inference_type: Some(InferenceType::Deduction),
            },
        ]
    }

    fn sample_entity_stats() -> HashMap<String, EntityStats> {
        let mut map = HashMap::new();
        map.insert(
            "Palantir".into(),
            EntityStats {
                triplet_count: 5,
                avg_confidence: 0.9,
                revisions: 2,
                contradictions: 0,
            },
        );
        map.insert(
            "US_DoD".into(),
            EntityStats {
                triplet_count: 8,
                avg_confidence: 0.4,
                revisions: 10,
                contradictions: 1,
            },
        );
        map
    }

    #[test]
    fn test_full_brain_mri() {
        let edges = sample_edges();
        let stats = sample_entity_stats();
        let styles = vec![
            ("Analytical".into(), "Convergent".into(), 5u64, 0.8f32),
            ("Creative".into(), "Divergent".into(), 2u64, 0.6f32),
        ];

        let mri = run_brain_mri(&edges, &stats, &styles, ScanMode::Full);

        assert_eq!(mri.regions.len(), 4);
        assert_eq!(mri.total_entities, 2);
        assert_eq!(mri.total_triplets, 2);
        assert_eq!(mri.dominant_mode, "Analytical");
        assert!(!mri.findings.is_empty());
        // Should detect US_DoD as conflicted (1 contradiction)
        assert!(mri
            .plasticity_map
            .iter()
            .any(|p| p.entity == "US_DoD" && p.state == PlasticityState::Conflicted));
        // Should have reasoning chains (1 inferred edge)
        assert!(!mri.reasoning_chains.is_empty());
    }

    #[test]
    fn test_structural_scan_no_chains() {
        let edges = sample_edges();
        let stats = sample_entity_stats();
        let mri = run_brain_mri(&edges, &stats, &[], ScanMode::Structural);

        // Structural scan should NOT trace reasoning chains (that's Full only)
        assert!(mri.reasoning_chains.is_empty());
    }

    #[test]
    fn test_plasticity_states() {
        let mut stats = HashMap::new();
        stats.insert("hot_entity".into(), EntityStats {
            triplet_count: 10,
            avg_confidence: 0.5,
            revisions: 20,
            contradictions: 0,
        });
        stats.insert("frozen_entity".into(), EntityStats {
            triplet_count: 5,
            avg_confidence: 0.95,
            revisions: 1,
            contradictions: 0,
        });
        stats.insert("conflicted_entity".into(), EntityStats {
            triplet_count: 3,
            avg_confidence: 0.6,
            revisions: 2,
            contradictions: 2,
        });

        let mri = run_brain_mri(&[], &stats, &[], ScanMode::Full);

        let hot = mri.plasticity_map.iter().find(|p| p.entity == "hot_entity").unwrap();
        assert_eq!(hot.state, PlasticityState::Hot);

        let frozen = mri.plasticity_map.iter().find(|p| p.entity == "frozen_entity").unwrap();
        assert_eq!(frozen.state, PlasticityState::Frozen);

        let conflicted = mri.plasticity_map.iter().find(|p| p.entity == "conflicted_entity").unwrap();
        assert_eq!(conflicted.state, PlasticityState::Conflicted);
    }

    #[test]
    fn test_brain_regions() {
        let stages = vec![
            ("extraction".into(), OsintCounterSnapshot {
                calls: 10, successes: 9, failures: 1, avg_latency_us: 500, triplets_produced: 45,
            }),
            ("deduction".into(), OsintCounterSnapshot {
                calls: 5, successes: 5, failures: 0, avg_latency_us: 100, triplets_produced: 12,
            }),
            ("episodic_store".into(), OsintCounterSnapshot {
                calls: 0, successes: 0, failures: 0, avg_latency_us: 0, triplets_produced: 0,
            }),
        ];

        let regions = build_brain_regions(&stages);
        assert_eq!(regions.len(), 4);

        let perception = regions.iter().find(|r| r.name == "perception").unwrap();
        assert!(perception.activation > 0.0);

        let memory = regions.iter().find(|r| r.name == "memory").unwrap();
        // episodic_store has 0 calls, so some sub-regions are dead
        assert!(memory.sub_regions.iter().any(|s| s.status == RegionStatus::Dead));
    }

    #[test]
    fn test_empty_brain() {
        let mri = run_brain_mri(&[], &HashMap::new(), &[], ScanMode::Full);
        assert_eq!(mri.total_entities, 0);
        assert_eq!(mri.health_score, 0.0);
        assert!(mri.findings.iter().any(|f| f.contains("healthy")));
    }

    #[test]
    fn test_reasoning_chain_trace() {
        let edges = vec![
            TruthEdge {
                source: "A".into(),
                target: "C".into(),
                rel_type: "DEDUCED".into(),
                truth: TruthValue::new(0.8, 0.6),
                inferred: true,
                via: vec!["B".into()],
                inference_type: Some(InferenceType::Deduction),
            },
            TruthEdge {
                source: "X".into(),
                target: "Y".into(),
                rel_type: "ABDUCED".into(),
                truth: TruthValue::new(0.5, 0.3),
                inferred: true,
                via: vec!["Z".into()],
                inference_type: Some(InferenceType::Abduction),
            },
        ];

        let chains = trace_reasoning_chains(&edges);
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].depth, 2); // 1 + 1 via node
        assert_eq!(chains[0].steps[0].rule, "deduction");
        assert_eq!(chains[1].steps[0].rule, "abduction");
    }
}
