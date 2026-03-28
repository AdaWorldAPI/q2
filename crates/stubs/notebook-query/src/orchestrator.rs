//! Meta-aware agent orchestrator with NARS reinforcement learning + MUL.
//!
//! Three layers of self-awareness, transparent switching:
//!
//! 1. **MUL (Meta-Uncertainty Layer)**: Before every step, assesses epistemic state.
//!    Dunning-Kruger position gates which styles are allowed. Trust texture modulates
//!    exploration rate. Flow state adjusts patience thresholds. Compass can override
//!    both adaptive and hardcoded modes.
//!
//! 2. **Adaptive** (default): NARS topology learns which thinking style sequences
//!    produce good outcomes. Each style pair (A→B) gets a truth value. High-confidence
//!    edges fire; low-confidence edges get explored. MUL's `free_will_modifier`
//!    scales the topology's expected quality — low free will = distrust the learned weights.
//!
//! 3. **Hardcoded fallback**: When MUL-adjusted efficiency drops below threshold,
//!    transparently switches to the classic plan→act→explore→reflex loop.
//!    The MRI endpoint reports which mode is active, MUL assessment, and why.
//!
//! The meta-awareness monitors BOTH its own efficiency AND its epistemic position:
//! - Mount Stupid detected? → Force sandbox, don't act on false confidence
//! - Valley of Despair? → Increase exploration, the system is learning
//! - Plateau of Mastery? → Full exploit, trust the topology
//! - Compass says Explore? → Override topology, go to unexplored territory
//!
//! # Architecture
//!
//! ```text
//! Observation
//!   ↓
//! MetaOrchestrator::step()
//!   ├── Check mode (adaptive vs fallback)
//!   ├── If adaptive:
//!   │     ├── Select style via NARS topology weights
//!   │     ├── Execute style
//!   │     ├── Measure outcome quality
//!   │     ├── NARS revision on (style_from → style_to) edge
//!   │     ├── If rolling_efficiency < FALLBACK_THRESHOLD → switch to fallback
//!   │     └── Return result
//!   └── If fallback:
//!         ├── Execute hardcoded: plan → act → explore → reflex
//!         ├── Measure outcome quality
//!         ├── If rolling_efficiency > RESTORE_THRESHOLD → re-enable adaptive
//!         └── Return result
//! ```

use super::reasoning::TruthValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

/// Below this rolling efficiency, switch from adaptive to hardcoded fallback.
const FALLBACK_THRESHOLD: f32 = 0.35;

/// Above this rolling efficiency in fallback mode, re-enable adaptive.
const RESTORE_THRESHOLD: f32 = 0.55;

/// Minimum observations before allowing mode switch.
const MIN_OBSERVATIONS: usize = 5;

/// Rolling window size for efficiency calculation.
const WINDOW_SIZE: usize = 20;

/// Base exploration probability when adaptive mode has low confidence.
const BASE_EXPLORATION_RATE: f32 = 0.15;

// ============================================================================
// MUL Assessment — Meta-Uncertainty Layer
// ============================================================================

/// Dunning-Kruger position on the confidence curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DkPosition {
    /// HIGH confidence, LOW experience — DANGEROUS. Block autonomous action.
    MountStupid,
    /// Aware of gaps, cautious. Increase exploration.
    ValleyOfDespair,
    /// Building real competence. Normal operation.
    SlopeOfEnlightenment,
    /// Calibrated confidence. Full exploit.
    PlateauOfMastery,
}

impl DkPosition {
    /// Humility factor: discounts confidence based on DK position.
    pub fn humility_factor(&self) -> f32 {
        match self {
            Self::MountStupid => 0.3,
            Self::ValleyOfDespair => 0.7,
            Self::SlopeOfEnlightenment => 0.85,
            Self::PlateauOfMastery => 1.0,
        }
    }

    /// Exploration rate modifier: how much to explore vs exploit.
    pub fn exploration_modifier(&self) -> f32 {
        match self {
            Self::MountStupid => 0.0,         // Don't explore, you're overconfident
            Self::ValleyOfDespair => 2.0,      // Explore heavily, you're learning
            Self::SlopeOfEnlightenment => 1.0, // Normal
            Self::PlateauOfMastery => 0.5,     // Exploit more, you've earned it
        }
    }

    /// Whether this position is safe for autonomous adaptive action.
    pub fn allows_adaptive(&self) -> bool {
        !matches!(self, Self::MountStupid)
    }
}

/// Trust texture — how much to trust the data sources and environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustTexture {
    /// High reliability, stable environment. Trust topology weights.
    Crystalline,
    /// Moderate reliability. Normal operation.
    Fibrous,
    /// Low reliability, unstable environment. Distrust learned weights.
    Fuzzy,
}

impl TrustTexture {
    /// Trust factor: scales topology confidence.
    pub fn trust_factor(&self) -> f32 {
        match self {
            Self::Crystalline => 1.0,
            Self::Fibrous => 0.7,
            Self::Fuzzy => 0.4,
        }
    }
}

/// Flow state — current cognitive load and engagement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowState {
    /// Optimal: challenge matches skill. Normal thresholds.
    Flow,
    /// Under-stimulated. Widen thresholds, increase exploration.
    Boredom,
    /// Over-stimulated. Tighten thresholds, prefer known-good styles.
    Anxiety,
    /// Disengaged. Minimal processing, fast fallback.
    Apathy,
}

impl FlowState {
    /// Patience modifier: adjusts how long before fallback triggers.
    pub fn patience_modifier(&self) -> f32 {
        match self {
            Self::Flow => 1.0,
            Self::Boredom => 1.5,   // More patient, try more things
            Self::Anxiety => 0.5,   // Less patient, fallback sooner
            Self::Apathy => 0.25,   // Very impatient
        }
    }
}

/// Compass override — when the map runs out, the compass decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompassDecision {
    /// No override. Let topology/hardcoded decide.
    None,
    /// Force exploration regardless of topology weights.
    ForceExplore,
    /// Force sandbox — block all autonomous action.
    ForceSandbox,
}

/// Complete MUL assessment for one orchestrator step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulAssessment {
    pub dk_position: DkPosition,
    pub trust: TrustTexture,
    pub flow: FlowState,
    pub compass: CompassDecision,
    /// Combined modifier: DK humility × trust × flow patience. Range [0.0, 1.5].
    pub free_will_modifier: f32,
    /// Effective exploration rate after MUL adjustment.
    pub effective_exploration_rate: f32,
    /// Effective fallback threshold after flow adjustment.
    pub effective_fallback_threshold: f32,
}

impl MulAssessment {
    /// Compute from raw signals.
    pub fn assess(
        felt_competence: f32,
        demonstrated_competence: f32,
        source_reliability: f32,
        environment_stability: f32,
        challenge_skill_ratio: f32,
    ) -> Self {
        // Dunning-Kruger detection
        let gap = felt_competence - demonstrated_competence;
        let dk_position = if gap > 0.3 && demonstrated_competence < 0.4 {
            DkPosition::MountStupid
        } else if felt_competence < 0.4 && demonstrated_competence < 0.5 {
            DkPosition::ValleyOfDespair
        } else if demonstrated_competence > 0.7 && gap.abs() < 0.15 {
            DkPosition::PlateauOfMastery
        } else {
            DkPosition::SlopeOfEnlightenment
        };

        // Trust texture
        let trust_score = source_reliability * 0.5 + environment_stability * 0.5;
        let trust = if trust_score > 0.8 {
            TrustTexture::Crystalline
        } else if trust_score > 0.5 {
            TrustTexture::Fibrous
        } else {
            TrustTexture::Fuzzy
        };

        // Flow state from challenge/skill ratio
        let flow = if challenge_skill_ratio > 0.4 && challenge_skill_ratio < 0.7 {
            FlowState::Flow
        } else if challenge_skill_ratio < 0.2 {
            FlowState::Boredom
        } else if challenge_skill_ratio > 0.85 {
            FlowState::Anxiety
        } else if challenge_skill_ratio < 0.05 {
            FlowState::Apathy
        } else {
            FlowState::Flow
        };

        // Compass override
        let compass = if dk_position == DkPosition::MountStupid {
            CompassDecision::ForceSandbox
        } else if dk_position == DkPosition::ValleyOfDespair
            && demonstrated_competence < 0.2
        {
            CompassDecision::ForceExplore
        } else {
            CompassDecision::None
        };

        let free_will = dk_position.humility_factor()
            * trust.trust_factor()
            * flow.patience_modifier();

        let exploration_rate =
            BASE_EXPLORATION_RATE * dk_position.exploration_modifier() * trust.trust_factor();

        let fallback_threshold = FALLBACK_THRESHOLD / flow.patience_modifier();

        Self {
            dk_position,
            trust,
            flow,
            compass,
            free_will_modifier: free_will,
            effective_exploration_rate: exploration_rate.clamp(0.0, 0.8),
            effective_fallback_threshold: fallback_threshold.clamp(0.1, 0.8),
        }
    }

    /// Quick assessment from rolling efficiency (when no external signals).
    /// Uses efficiency as a proxy for demonstrated competence, and
    /// topology confidence as felt competence.
    pub fn from_efficiency(efficiency: f32, topology_confidence: f32) -> Self {
        Self::assess(
            topology_confidence,    // felt = how confident the topology is
            efficiency,             // demonstrated = actual rolling efficiency
            0.7,                    // default source reliability
            0.8,                    // default environment stability
            (efficiency - 0.3).abs().clamp(0.0, 1.0), // challenge ~ distance from mediocrity
        )
    }

    /// Self-regulated assessment from live graph signals.
    ///
    /// The graph's own entropy, contradiction rate, revision velocity, and
    /// plasticity distribution become sensory input to MUL. The system's
    /// epistemic position is derived from the knowledge it actually has,
    /// not just from outcome quality.
    ///
    /// This is the self-awareness loop: graph state → DK position → style selection
    /// → graph mutations → graph state changes → DK position shifts → ...
    pub fn from_graph_signals(signals: &GraphSensorium, topology_confidence: f32) -> Self {
        // Demonstrated competence: high when graph is consistent + growing.
        // Low entropy + few contradictions + steady revision = mastery.
        // High entropy + many contradictions + stalled revision = valley.
        let consistency = 1.0 - signals.contradiction_rate;
        let growth = signals.revision_velocity.clamp(0.0, 1.0);
        let demonstrated = consistency * 0.6 + growth * 0.4;

        // Felt competence: topology confidence (how sure the RL thinks it is).
        let felt = topology_confidence;

        // Source reliability: inverse of entropy. Low entropy = reliable, consistent sources.
        let source_reliability = 1.0 - signals.truth_entropy;

        // Environment stability: inverse of plasticity flux.
        // If many entities are Hot (rapidly changing), the environment is unstable.
        let stability = 1.0 - signals.plasticity_flux;

        // Challenge/skill ratio: contradictions are the challenge,
        // revision velocity is the skill to resolve them.
        let challenge = signals.contradiction_rate;
        let skill = signals.revision_velocity;
        let challenge_skill = if skill > 0.01 {
            (challenge / skill).clamp(0.0, 1.0)
        } else if challenge > 0.1 {
            0.95 // High challenge, no skill → Anxiety
        } else {
            0.1 // No challenge, no skill → Boredom
        };

        Self::assess(felt, demonstrated, source_reliability, stability, challenge_skill)
    }
}

// ============================================================================
// Graph Sensorium — real-time signals from the knowledge graph
// ============================================================================

/// Real-time signals from the knowledge graph for MUL self-regulation.
///
/// The graph's own state is the primary sensory input to the meta-awareness layer.
/// These signals drive automatic style balancing: high contradiction rate
/// triggers more Explore/Reflex; low entropy triggers more Plan/Act.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSensorium {
    /// Contradiction rate: contradictions / active_triplets. Range [0, 1].
    /// High = lots of conflicting evidence → Valley of Despair, increase exploration.
    /// Low = consistent knowledge → Slope or Plateau, increase exploitation.
    pub contradiction_rate: f32,

    /// Truth entropy: Shannon entropy of truth confidence distribution.
    /// Normalized to [0, 1] where 0 = all triplets have same confidence,
    /// 1 = uniform distribution across confidence bands.
    /// High = uncertain about everything → increase exploration.
    /// Low = confident knowledge → increase exploitation.
    pub truth_entropy: f32,

    /// Revision velocity: revisions_per_step over rolling window.
    /// Range [0, 1] where 1 = every step produces a revision.
    /// High = actively learning → keep current mode, the system is adapting.
    /// Low = stagnant → either mastery (if consistent) or stuck (if inconsistent).
    pub revision_velocity: f32,

    /// Plasticity flux: fraction of entities in Hot state. Range [0, 1].
    /// High = environment is changing rapidly → lower trust, increase exploration.
    /// Low = stable environment → higher trust, increase exploitation.
    pub plasticity_flux: f32,

    /// Deduction yield: inferred_triplets / deduction_attempts. Range [0, 1].
    /// High = graph structure supports rich inference → Plan/Act more.
    /// Low = sparse graph, few chains → Explore more.
    pub deduction_yield: f32,

    /// Episodic saturation: episodes / capacity. Range [0, 1].
    /// High = memory full → start forgetting or compressing.
    pub episodic_saturation: f32,
}

impl GraphSensorium {
    /// Compute from raw graph statistics.
    pub fn compute(
        active_triplets: usize,
        contradictions: usize,
        confidence_histogram: &[usize; 5], // [certain, strong, moderate, weak, unknown]
        revisions_in_window: usize,
        window_steps: usize,
        hot_entities: usize,
        total_entities: usize,
        deduction_attempts: usize,
        deductions_produced: usize,
        episodic_count: usize,
        episodic_capacity: usize,
    ) -> Self {
        let active = active_triplets.max(1) as f32;

        let contradiction_rate = contradictions as f32 / active;

        // Shannon entropy of confidence distribution
        let total: f32 = confidence_histogram.iter().sum::<usize>() as f32;
        let truth_entropy = if total > 0.0 {
            let mut h = 0.0f32;
            for &count in confidence_histogram {
                if count > 0 {
                    let p = count as f32 / total;
                    h -= p * p.ln();
                }
            }
            // Normalize by max entropy (ln(5) ≈ 1.609)
            (h / 1.609).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let revision_velocity = if window_steps > 0 {
            (revisions_in_window as f32 / window_steps as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let plasticity_flux = if total_entities > 0 {
            hot_entities as f32 / total_entities as f32
        } else {
            0.0
        };

        let deduction_yield = if deduction_attempts > 0 {
            (deductions_produced as f32 / deduction_attempts as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let episodic_saturation = if episodic_capacity > 0 {
            episodic_count as f32 / episodic_capacity as f32
        } else {
            0.0
        };

        Self {
            contradiction_rate,
            truth_entropy,
            revision_velocity,
            plasticity_flux,
            deduction_yield,
            episodic_saturation,
        }
    }

    /// What the graph signals suggest: explore more, exploit more, or panic.
    pub fn suggested_bias(&self) -> GraphBias {
        if self.contradiction_rate > 0.3 {
            GraphBias::Resolve // Too many contradictions — focus on reflex/resolution
        } else if self.truth_entropy > 0.7 {
            GraphBias::Explore // Uncertain about everything — gather more evidence
        } else if self.deduction_yield > 0.5 && self.truth_entropy < 0.3 {
            GraphBias::Exploit // Rich consistent graph — exploit the knowledge
        } else if self.plasticity_flux > 0.5 {
            GraphBias::Adapt // Environment changing — stay flexible
        } else if self.revision_velocity < 0.05 && self.truth_entropy > 0.4 {
            GraphBias::Stagnant // Not learning + still uncertain — shake things up
        } else {
            GraphBias::Balanced // Normal operation
        }
    }
}

/// Graph-suggested cognitive bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphBias {
    /// High contradictions — focus on resolution (Reflex/Metacognitive).
    Resolve,
    /// High entropy — gather evidence (Explore/Divergent).
    Explore,
    /// Rich consistent graph — use the knowledge (Plan/Act/Analytical).
    Exploit,
    /// High plasticity — stay flexible (Creative/Exploratory).
    Adapt,
    /// Low revision + high entropy — stuck, need perturbation.
    Stagnant,
    /// Normal — let topology decide.
    Balanced,
}

// ============================================================================
// NARS Auto-Heal Contingency
// ============================================================================

/// Actions the NARS auto-heal contingency can take to fix an unorganized graph.
///
/// When the graph has low truth scores, high entropy, or unresolved contradictions,
/// the contingency fires automatically before the next style selection.
/// This is the immune system: detect disease → apply remedy → measure recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingAction {
    pub action: HealingType,
    pub reason: String,
    pub triplets_affected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealingType {
    /// Run NARS revision on all triplets with very low confidence.
    /// Sets confidence to `from_evidence(1, 0)` — weak but non-zero.
    BootstrapTruth,
    /// Run contradiction detection + resolution.
    /// Contradicting triplets get their confidence reduced by revision with counter-evidence.
    ResolveContradictions,
    /// Run deduction to fill in missing links.
    /// A→B and B→C produces A→C with deduced truth.
    InferMissingLinks,
    /// Compact soft-deleted triplets (garbage collection).
    CompactDeleted,
    /// Re-normalize truth values: scale all confidences so max = 0.95.
    /// Prevents confidence inflation from repeated self-revision.
    NormalizeTruth,
    /// Reset topology: when the orchestrator's own learning is poisoned
    /// by bad data, wipe the NARS edges and restart from uniform prior.
    ResetTopology,
}

/// Determine what healing actions the graph needs.
///
/// Called automatically by the orchestrator when `update_sensorium()` detects
/// graph health issues. Returns a prioritized list of healing actions.
pub fn diagnose_healing(signals: &GraphSensorium) -> Vec<HealingAction> {
    let mut actions = Vec::new();

    // High contradiction rate → resolve contradictions first.
    if signals.contradiction_rate > 0.15 {
        actions.push(HealingAction {
            action: HealingType::ResolveContradictions,
            reason: format!(
                "Contradiction rate {:.1}% exceeds 15% threshold",
                signals.contradiction_rate * 100.0
            ),
            triplets_affected: 0, // Caller fills this in.
        });
    }

    // High entropy + low revision = unorganized, truth scores not set properly.
    if signals.truth_entropy > 0.6 && signals.revision_velocity < 0.1 {
        actions.push(HealingAction {
            action: HealingType::BootstrapTruth,
            reason: format!(
                "High entropy ({:.2}) + low revision velocity ({:.2}): truth values likely uninitialized",
                signals.truth_entropy, signals.revision_velocity
            ),
            triplets_affected: 0,
        });
    }

    // Low deduction yield with enough data → graph has gaps NARS can fill.
    if signals.deduction_yield < 0.1 && signals.truth_entropy < 0.5 {
        actions.push(HealingAction {
            action: HealingType::InferMissingLinks,
            reason: "Low deduction yield but consistent data — inference can fill gaps".into(),
            triplets_affected: 0,
        });
    }

    // High episodic saturation → compact deleted triplets to free memory.
    if signals.episodic_saturation > 0.85 {
        actions.push(HealingAction {
            action: HealingType::CompactDeleted,
            reason: format!(
                "Episodic saturation {:.0}% — compact to free space",
                signals.episodic_saturation * 100.0
            ),
            triplets_affected: 0,
        });
    }

    // Very high truth entropy suggests truth inflation (everything at max confidence).
    // Normalize to prevent overconfidence.
    if signals.truth_entropy < 0.1 && signals.contradiction_rate < 0.05 {
        actions.push(HealingAction {
            action: HealingType::NormalizeTruth,
            reason: "Very low entropy with few contradictions — possible truth inflation".into(),
            triplets_affected: 0,
        });
    }

    actions
}

impl MetaOrchestrator {
    /// Auto-heal contingency: when graph signals indicate disease,
    /// apply healing actions before the next thinking step.
    ///
    /// Returns the list of healing actions that should be applied.
    /// The caller is responsible for executing them against the actual graph
    /// (since the orchestrator doesn't own the graph).
    pub fn auto_heal(&mut self) -> Vec<HealingAction> {
        let Some(ref signals) = self.sensorium else {
            return Vec::new();
        };

        let mut actions = diagnose_healing(signals);

        // If the orchestrator's own topology is producing consistently bad results
        // AND the graph is in bad shape, reset the topology too.
        if self.rolling_efficiency() < 0.2
            && self.step_count > 20
            && signals.truth_entropy > 0.5
        {
            actions.push(HealingAction {
                action: HealingType::ResetTopology,
                reason: format!(
                    "Orchestrator efficiency {:.2} with high entropy {:.2} after {} steps — topology likely poisoned",
                    self.rolling_efficiency(), signals.truth_entropy, self.step_count
                ),
                triplets_affected: 0,
            });
            // Actually reset the topology.
            self.topology = StyleTopology::new();
            self.temperature = 0.5; // Warm restart with moderate noise.
            self.quality_window.clear();
            self.switch_mode(
                OrchestratorMode::HardcodedFallback,
                "Topology reset by auto-heal — starting from hardcoded baseline".into(),
            );
        }

        actions
    }
}

// ============================================================================
// Thinking Styles (maps to lance-graph-planner ThinkingStyle)
// ============================================================================

/// The 4 agent roles mapped to thinking styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentStyle {
    /// Plan agent → Analytical / Convergent.
    /// Deep sequential reasoning, high confidence chains.
    Plan,
    /// Action agent → Focused / Deductive.
    /// Narrow, precise, single best action selection.
    Act,
    /// Exploration agent → Exploratory / Divergent.
    /// Lateral connections, find surprises, expand knowledge.
    Explore,
    /// Reflex agent → Metacognitive / Revision.
    /// Learn from mistakes, revise beliefs, detect inefficiency.
    Reflex,
}

impl AgentStyle {
    pub fn all() -> &'static [AgentStyle] {
        &[
            AgentStyle::Plan,
            AgentStyle::Act,
            AgentStyle::Explore,
            AgentStyle::Reflex,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Act => "act",
            Self::Explore => "explore",
            Self::Reflex => "reflex",
        }
    }

    /// The hardcoded sequence: plan → act → explore → reflex.
    pub fn hardcoded_sequence() -> &'static [AgentStyle] {
        &[
            AgentStyle::Plan,
            AgentStyle::Act,
            AgentStyle::Explore,
            AgentStyle::Reflex,
        ]
    }
}

// ============================================================================
// NARS Topology — learned style activation weights
// ============================================================================

/// A directed edge in the style topology: "after style A, style B works well."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdge {
    /// Source style.
    pub from: AgentStyle,
    /// Target style.
    pub to: AgentStyle,
    /// NARS truth value: frequency = success rate, confidence = evidence strength.
    pub truth: TruthValue,
    /// Number of times this transition was observed.
    pub observations: u64,
    /// Cumulative quality score for this transition.
    pub total_quality: f64,
}

impl TopologyEdge {
    fn new(from: AgentStyle, to: AgentStyle) -> Self {
        Self {
            from,
            to,
            // Start with weak prior: frequency 0.5 (no bias), low confidence.
            truth: TruthValue::new(0.5, 0.1),
            observations: 0,
            total_quality: 0.0,
        }
    }

    /// NARS revision with new evidence.
    fn revise(&mut self, quality: f64) {
        self.observations += 1;
        self.total_quality += quality;

        // New evidence truth: frequency = quality, confidence grows with observations.
        let evidence_f = quality.clamp(0.0, 1.0);
        let evidence_c = (self.observations as f64 / (self.observations as f64 + 5.0)).min(0.99);
        let evidence = TruthValue::new(evidence_f, evidence_c);

        self.truth = self.truth.revision(&evidence);
    }

    /// Expected quality = truth expectation.
    fn expected_quality(&self) -> f64 {
        self.truth.expectation()
    }
}

/// The full topology: 4×4 = 16 edges between styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleTopology {
    /// Edges keyed by (from, to).
    edges: HashMap<(AgentStyle, AgentStyle), TopologyEdge>,
}

impl StyleTopology {
    pub fn new() -> Self {
        let mut edges = HashMap::new();
        for &from in AgentStyle::all() {
            for &to in AgentStyle::all() {
                edges.insert((from, to), TopologyEdge::new(from, to));
            }
        }
        Self { edges }
    }

    /// Get the edge from→to.
    pub fn edge(&self, from: AgentStyle, to: AgentStyle) -> &TopologyEdge {
        &self.edges[&(from, to)]
    }

    /// Revise an edge with observed quality.
    pub fn revise(&mut self, from: AgentStyle, to: AgentStyle, quality: f64) {
        self.edges.get_mut(&(from, to)).unwrap().revise(quality);
    }

    /// Select the best next style given the current style.
    ///
    /// With probability `exploration_rate`, picks a random style (explore).
    /// Otherwise picks the highest expected quality.
    pub fn select_next(
        &self,
        current: AgentStyle,
        step: u64,
    ) -> AgentStyle {
        // Deterministic "random" from step counter for reproducibility.
        let pseudo_random = ((step.wrapping_mul(0x9E3779B97F4A7C15)) >> 56) as f32 / 256.0;

        if pseudo_random < EXPLORATION_RATE {
            // Explore: pick the LEAST observed edge (maximize information gain).
            let mut best = AgentStyle::Plan;
            let mut min_obs = u64::MAX;
            for &to in AgentStyle::all() {
                let edge = self.edge(current, to);
                if edge.observations < min_obs {
                    min_obs = edge.observations;
                    best = to;
                }
            }
            return best;
        }

        // Exploit: pick highest expected quality.
        let mut best = AgentStyle::Plan;
        let mut best_eq = f64::NEG_INFINITY;
        for &to in AgentStyle::all() {
            let eq = self.edge(current, to).expected_quality();
            if eq > best_eq {
                best_eq = eq;
                best = to;
            }
        }
        best
    }

    /// Total observations across all edges.
    pub fn total_observations(&self) -> u64 {
        self.edges.values().map(|e| e.observations).sum()
    }

    /// Snapshot for MRI reporting.
    pub fn snapshot(&self) -> Vec<TopologyEdgeSnapshot> {
        self.edges
            .values()
            .map(|e| TopologyEdgeSnapshot {
                from: e.from.name().to_string(),
                to: e.to.name().to_string(),
                frequency: e.truth.frequency,
                confidence: e.truth.confidence,
                expectation: e.truth.expectation(),
                observations: e.observations,
                avg_quality: if e.observations > 0 {
                    e.total_quality / e.observations as f64
                } else {
                    0.0
                },
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdgeSnapshot {
    pub from: String,
    pub to: String,
    pub frequency: f64,
    pub confidence: f64,
    pub expectation: f64,
    pub observations: u64,
    pub avg_quality: f64,
}

// ============================================================================
// Orchestrator Mode
// ============================================================================

/// Current orchestration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestratorMode {
    /// NARS topology drives style selection.
    Adaptive,
    /// Classic plan→act→explore→reflex loop.
    HardcodedFallback,
}

/// Why the mode was switched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSwitchEvent {
    pub from: OrchestratorMode,
    pub to: OrchestratorMode,
    pub reason: String,
    pub efficiency_at_switch: f32,
    pub step: u64,
}

// ============================================================================
// Meta Orchestrator
// ============================================================================

/// The meta-aware orchestrator.
///
/// Monitors its own efficiency and transparently switches between
/// adaptive (NARS RL) and hardcoded (plan→act→explore→reflex) modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaOrchestrator {
    /// Current mode.
    pub mode: OrchestratorMode,
    /// NARS topology for adaptive mode.
    pub topology: StyleTopology,
    /// Last style executed (for topology edge tracking).
    pub last_style: Option<AgentStyle>,
    /// Rolling window of outcome qualities.
    pub quality_window: Vec<f32>,
    /// Total steps executed.
    pub step_count: u64,
    /// History of mode switches.
    pub mode_switches: Vec<ModeSwitchEvent>,
    /// Hardcoded sequence position (for fallback mode).
    pub fallback_position: usize,
    /// Steps spent in current mode.
    pub steps_in_current_mode: usize,
    /// Latest MUL assessment.
    pub last_mul: MulAssessment,
    /// Latest graph sensorium (None until first graph signal arrives).
    pub sensorium: Option<GraphSensorium>,
    /// Stagnation temperature: injected noise when thinking is stale.
    /// 0.0 = deterministic (normal). 1.0 = maximum randomness (shake things up).
    /// Auto-increases when GraphBias::Stagnant detected, decays otherwise.
    pub temperature: f32,
}

/// The result of one orchestrator step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Which style was selected.
    pub style: AgentStyle,
    /// Current mode when this step ran.
    pub mode: OrchestratorMode,
    /// Why this style was selected.
    pub reason: StepReason,
    /// MUL assessment at time of selection.
    pub mul: MulAssessment,
    /// Rolling efficiency at time of selection.
    pub efficiency: f32,
    /// Step number.
    pub step: u64,
}

/// Why a particular style was chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepReason {
    /// NARS topology selected this as highest expected quality (scaled by MUL free_will).
    TopologyExploit {
        expected_quality: f64,
        confidence: f64,
    },
    /// Exploration: picked least-observed edge for information gain.
    /// Exploration rate was modulated by DK position + trust texture.
    TopologyExplore {
        observations: u64,
    },
    /// Hardcoded sequence position (fallback mode).
    HardcodedSequence {
        position: usize,
    },
    /// MUL compass/DK override — forced a specific style regardless of topology.
    MulOverride {
        dk: DkPosition,
        compass: CompassDecision,
        explanation: String,
    },
}

impl MetaOrchestrator {
    /// Create a new orchestrator starting in adaptive mode.
    pub fn new() -> Self {
        Self {
            mode: OrchestratorMode::Adaptive,
            topology: StyleTopology::new(),
            last_style: None,
            quality_window: Vec::with_capacity(WINDOW_SIZE),
            step_count: 0,
            mode_switches: Vec::new(),
            fallback_position: 0,
            steps_in_current_mode: 0,
            last_mul: MulAssessment::from_efficiency(0.5, 0.5),
            sensorium: None,
            temperature: 0.0,
        }
    }

    /// Feed real-time graph signals into the orchestrator.
    ///
    /// This is the sensory input loop: graph → sensorium → MUL → style selection.
    /// Call this before `select_next()` to enable self-regulated thinking.
    pub fn update_sensorium(&mut self, signals: GraphSensorium) {
        // Auto-adjust temperature based on graph bias.
        match signals.suggested_bias() {
            GraphBias::Stagnant => {
                // Increase temperature: thinking is stuck, inject noise.
                self.temperature = (self.temperature + 0.15).min(0.9);
            }
            GraphBias::Resolve => {
                // Moderate temperature: contradictions need diverse approaches.
                self.temperature = (self.temperature + 0.05).min(0.5);
            }
            GraphBias::Exploit => {
                // Cool down: graph is consistent, don't perturb.
                self.temperature = (self.temperature - 0.1).max(0.0);
            }
            _ => {
                // Gentle decay toward 0.
                self.temperature = (self.temperature - 0.02).max(0.0);
            }
        }
        self.sensorium = Some(signals);
    }

    /// Rolling efficiency: mean of quality window.
    pub fn rolling_efficiency(&self) -> f32 {
        if self.quality_window.is_empty() {
            return 0.5; // neutral prior
        }
        self.quality_window.iter().sum::<f32>() / self.quality_window.len() as f32
    }

    /// Select the next style to execute, modulated by MUL assessment.
    ///
    /// The MUL layer runs BEFORE style selection:
    /// 1. Assess DK position from efficiency (demonstrated) vs topology confidence (felt)
    /// 2. Mount Stupid? → Force sandbox (return Reflex only, block everything else)
    /// 3. Compass says ForceExplore? → Override topology, return Explore
    /// 4. Flow state adjusts fallback threshold and exploration rate
    /// 5. Trust texture scales topology edge confidence
    /// 6. Then: adaptive (NARS topology × MUL free_will) or hardcoded fallback
    pub fn select_next(&mut self) -> StepResult {
        self.step_count += 1;
        self.steps_in_current_mode += 1;

        // ── MUL Assessment (self-regulated from graph signals when available) ──
        let avg_confidence = self.topology.edges.values()
            .filter(|e| e.observations > 0)
            .map(|e| e.truth.confidence)
            .sum::<f64>()
            / self.topology.edges.values().filter(|e| e.observations > 0).count().max(1) as f64;
        let mul = if let Some(ref signals) = self.sensorium {
            MulAssessment::from_graph_signals(signals, avg_confidence as f32)
        } else {
            MulAssessment::from_efficiency(self.rolling_efficiency(), avg_confidence as f32)
        };
        self.last_mul = mul.clone();

        // ── Compass Override ──
        if mul.compass == CompassDecision::ForceSandbox {
            return StepResult {
                style: AgentStyle::Reflex,
                mode: self.mode,
                reason: StepReason::MulOverride {
                    dk: mul.dk_position,
                    compass: mul.compass,
                    explanation: "Mount Stupid detected — sandboxing to Reflex only".into(),
                },
                mul,
                efficiency: self.rolling_efficiency(),
                step: self.step_count,
            };
        }

        if mul.compass == CompassDecision::ForceExplore {
            return StepResult {
                style: AgentStyle::Explore,
                mode: self.mode,
                reason: StepReason::MulOverride {
                    dk: mul.dk_position,
                    compass: mul.compass,
                    explanation: "Valley of Despair + low competence — compass forces exploration".into(),
                },
                mul,
                efficiency: self.rolling_efficiency(),
                step: self.step_count,
            };
        }

        // ── Mode-dependent selection with MUL modulation ──
        let (style, reason) = match self.mode {
            OrchestratorMode::Adaptive if mul.dk_position.allows_adaptive() => {
                let current = self.last_style.unwrap_or(AgentStyle::Plan);

                // Use MUL-adjusted exploration rate
                let pseudo_random =
                    ((self.step_count.wrapping_mul(0x9E3779B97F4A7C15)) >> 56) as f32 / 256.0;

                if pseudo_random < mul.effective_exploration_rate {
                    // Explore: pick least observed edge (maximize information gain)
                    let mut best = AgentStyle::Plan;
                    let mut min_obs = u64::MAX;
                    for &to in AgentStyle::all() {
                        let edge = self.topology.edge(current, to);
                        if edge.observations < min_obs {
                            min_obs = edge.observations;
                            best = to;
                        }
                    }
                    (
                        best,
                        StepReason::TopologyExplore {
                            observations: min_obs,
                        },
                    )
                } else {
                    // Exploit: pick highest expected quality, SCALED by MUL free_will.
                    // When temperature > 0, inject deterministic noise to break stagnation.
                    // Temperature acts like LLM temperature: 0 = greedy, 1 = random.
                    let mut best = AgentStyle::Plan;
                    let mut best_eq = f64::NEG_INFINITY;
                    for &to in AgentStyle::all() {
                        let base_eq = self.topology.edge(current, to).expected_quality()
                            * mul.free_will_modifier as f64;
                        // Temperature noise: deterministic from step + style index
                        let noise = if self.temperature > 0.01 {
                            let hash = (self.step_count.wrapping_mul(0x517cc1b727220a95)
                                ^ (to as u64).wrapping_mul(0x6c62272e07bb0142)) >> 48;
                            let uniform = (hash as f64) / 65536.0; // [0, 1)
                            (uniform - 0.5) * self.temperature as f64
                        } else {
                            0.0
                        };
                        let eq = base_eq + noise;
                        if eq > best_eq {
                            best_eq = eq;
                            best = to;
                        }
                    }
                    let edge = self.topology.edge(current, best);
                    (
                        best,
                        StepReason::TopologyExploit {
                            expected_quality: best_eq,
                            confidence: edge.truth.confidence * mul.trust.trust_factor() as f64,
                        },
                    )
                }
            }
            // Mount Stupid in adaptive mode → force fallback
            OrchestratorMode::Adaptive => {
                self.switch_mode(
                    OrchestratorMode::HardcodedFallback,
                    format!("DK position {:?} blocks adaptive mode", mul.dk_position),
                );
                let seq = AgentStyle::hardcoded_sequence();
                let pos = self.fallback_position % seq.len();
                self.fallback_position += 1;
                (seq[pos], StepReason::HardcodedSequence { position: pos })
            }
            OrchestratorMode::HardcodedFallback => {
                let seq = AgentStyle::hardcoded_sequence();
                let pos = self.fallback_position % seq.len();
                self.fallback_position += 1;
                (seq[pos], StepReason::HardcodedSequence { position: pos })
            }
        };

        StepResult {
            style,
            mode: self.mode,
            reason,
            mul,
            efficiency: self.rolling_efficiency(),
            step: self.step_count,
        }
    }

    /// Record the outcome of the last step and update NARS topology.
    ///
    /// `quality` is in [0.0, 1.0] where 1.0 = perfect outcome.
    /// This drives the reinforcement learning: good outcomes strengthen
    /// the topology edge that produced them.
    pub fn record_outcome(&mut self, style: AgentStyle, quality: f32) {
        // Update rolling window.
        if self.quality_window.len() >= WINDOW_SIZE {
            self.quality_window.remove(0);
        }
        self.quality_window.push(quality);

        // NARS revision on the topology edge.
        if let Some(prev) = self.last_style {
            self.topology.revise(prev, style, quality as f64);
        }
        self.last_style = Some(style);

        // Meta-awareness: check if we should switch modes.
        // Uses MUL-adjusted thresholds (flow state modifies patience).
        if self.quality_window.len() >= MIN_OBSERVATIONS {
            let eff = self.rolling_efficiency();
            let adjusted_fallback = self.last_mul.effective_fallback_threshold;
            let adjusted_restore = RESTORE_THRESHOLD * self.last_mul.flow.patience_modifier();
            match self.mode {
                OrchestratorMode::Adaptive if eff < adjusted_fallback => {
                    self.switch_mode(
                        OrchestratorMode::HardcodedFallback,
                        format!(
                            "Adaptive efficiency {:.2} < MUL-adjusted threshold {:.2} (DK={:?}, Flow={:?}) after {} steps",
                            eff, adjusted_fallback, self.last_mul.dk_position, self.last_mul.flow, self.steps_in_current_mode
                        ),
                    );
                }
                OrchestratorMode::HardcodedFallback if eff > adjusted_restore => {
                    self.switch_mode(
                        OrchestratorMode::Adaptive,
                        format!(
                            "Fallback efficiency {:.2} > MUL-adjusted restore {:.2} (DK={:?}), re-enabling adaptive",
                            eff, adjusted_restore, self.last_mul.dk_position
                        ),
                    );
                }
                _ => {}
            }
        }
    }

    fn switch_mode(&mut self, new_mode: OrchestratorMode, reason: String) {
        let event = ModeSwitchEvent {
            from: self.mode,
            to: new_mode,
            reason,
            efficiency_at_switch: self.rolling_efficiency(),
            step: self.step_count,
        };
        self.mode_switches.push(event);
        self.mode = new_mode;
        self.steps_in_current_mode = 0;
        self.fallback_position = 0;
    }

    /// Full status snapshot for the /mri endpoint.
    pub fn snapshot(&self) -> OrchestratorSnapshot {
        OrchestratorSnapshot {
            mode: self.mode,
            step_count: self.step_count,
            rolling_efficiency: self.rolling_efficiency(),
            steps_in_current_mode: self.steps_in_current_mode,
            topology: self.topology.snapshot(),
            mode_switches: self.mode_switches.clone(),
            last_style: self.last_style.map(|s| s.name().to_string()),
            mul: self.last_mul.clone(),
            fallback_threshold: self.last_mul.effective_fallback_threshold,
            restore_threshold: RESTORE_THRESHOLD * self.last_mul.flow.patience_modifier(),
        }
    }
}

impl Default for MetaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot for API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorSnapshot {
    pub mode: OrchestratorMode,
    pub step_count: u64,
    pub rolling_efficiency: f32,
    pub steps_in_current_mode: usize,
    pub topology: Vec<TopologyEdgeSnapshot>,
    pub mode_switches: Vec<ModeSwitchEvent>,
    pub last_style: Option<String>,
    pub mul: MulAssessment,
    pub fallback_threshold: f32,
    pub restore_threshold: f32,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_orchestrator_starts_adaptive() {
        let orch = MetaOrchestrator::new();
        assert_eq!(orch.mode, OrchestratorMode::Adaptive);
        assert_eq!(orch.step_count, 0);
    }

    #[test]
    fn test_select_produces_step_result() {
        let mut orch = MetaOrchestrator::new();
        let result = orch.select_next();
        assert_eq!(result.mode, OrchestratorMode::Adaptive);
        assert_eq!(result.step, 1);
    }

    #[test]
    fn test_record_outcome_revises_topology() {
        let mut orch = MetaOrchestrator::new();
        let r = orch.select_next();
        orch.record_outcome(r.style, 0.8);

        // After one observation, topology should have data.
        assert_eq!(orch.quality_window.len(), 1);
        assert!((orch.quality_window[0] - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fallback_on_low_efficiency() {
        let mut orch = MetaOrchestrator::new();

        // Feed consistently bad outcomes.
        for _ in 0..10 {
            let r = orch.select_next();
            orch.record_outcome(r.style, 0.1); // very bad
        }

        // Should have switched to fallback.
        assert_eq!(orch.mode, OrchestratorMode::HardcodedFallback);
        assert!(!orch.mode_switches.is_empty());
        assert_eq!(
            orch.mode_switches.last().unwrap().to,
            OrchestratorMode::HardcodedFallback
        );
    }

    #[test]
    fn test_restore_on_high_efficiency() {
        let mut orch = MetaOrchestrator::new();

        // Force into fallback.
        for _ in 0..10 {
            let r = orch.select_next();
            orch.record_outcome(r.style, 0.1);
        }
        assert_eq!(orch.mode, OrchestratorMode::HardcodedFallback);

        // Now feed good outcomes in fallback mode.
        for _ in 0..10 {
            let r = orch.select_next();
            orch.record_outcome(r.style, 0.9); // very good
        }

        // Should restore to adaptive.
        assert_eq!(orch.mode, OrchestratorMode::Adaptive);
        assert!(orch.mode_switches.len() >= 2);
    }

    #[test]
    fn test_hardcoded_follows_sequence() {
        let mut orch = MetaOrchestrator::new();
        // Force fallback.
        orch.mode = OrchestratorMode::HardcodedFallback;

        let r1 = orch.select_next();
        assert_eq!(r1.style, AgentStyle::Plan);
        let r2 = orch.select_next();
        assert_eq!(r2.style, AgentStyle::Act);
        let r3 = orch.select_next();
        assert_eq!(r3.style, AgentStyle::Explore);
        let r4 = orch.select_next();
        assert_eq!(r4.style, AgentStyle::Reflex);
        // Wraps around.
        let r5 = orch.select_next();
        assert_eq!(r5.style, AgentStyle::Plan);
    }

    #[test]
    fn test_topology_learns_preference() {
        let mut orch = MetaOrchestrator::new();

        // Train: Plan→Act with high quality, Plan→Explore with low quality.
        orch.last_style = Some(AgentStyle::Plan);
        for _ in 0..20 {
            orch.topology.revise(AgentStyle::Plan, AgentStyle::Act, 0.9);
            orch.topology
                .revise(AgentStyle::Plan, AgentStyle::Explore, 0.2);
        }

        let act_eq = orch
            .topology
            .edge(AgentStyle::Plan, AgentStyle::Act)
            .expected_quality();
        let explore_eq = orch
            .topology
            .edge(AgentStyle::Plan, AgentStyle::Explore)
            .expected_quality();

        // Act should be strongly preferred after Plan.
        assert!(
            act_eq > explore_eq,
            "act_eq={:.3} should be > explore_eq={:.3}",
            act_eq,
            explore_eq
        );
    }

    #[test]
    fn test_rolling_efficiency() {
        let mut orch = MetaOrchestrator::new();
        assert!((orch.rolling_efficiency() - 0.5).abs() < f32::EPSILON); // neutral prior

        orch.quality_window = vec![0.8, 0.6, 0.9, 0.7];
        assert!((orch.rolling_efficiency() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_snapshot_includes_mul() {
        let mut orch = MetaOrchestrator::new();
        let r = orch.select_next();
        orch.record_outcome(r.style, 0.7);

        let snap = orch.snapshot();
        assert_eq!(snap.mode, OrchestratorMode::Adaptive);
        assert_eq!(snap.step_count, 1);
        // 4×4 = 16 topology edges.
        assert_eq!(snap.topology.len(), 16);
        // MUL assessment should be present.
        assert!(snap.mul.free_will_modifier > 0.0);
    }

    #[test]
    fn test_mode_switch_event_recorded() {
        let mut orch = MetaOrchestrator::new();
        for _ in 0..10 {
            let r = orch.select_next();
            orch.record_outcome(r.style, 0.1);
        }

        let last_switch = orch.mode_switches.last().unwrap();
        assert_eq!(last_switch.from, OrchestratorMode::Adaptive);
        assert_eq!(last_switch.to, OrchestratorMode::HardcodedFallback);
        assert!(last_switch.reason.contains("efficiency") || last_switch.reason.contains("DK"));
    }

    #[test]
    fn test_mul_mount_stupid_forces_sandbox() {
        let mul = MulAssessment::assess(0.95, 0.1, 0.5, 0.5, 0.5);
        assert_eq!(mul.dk_position, DkPosition::MountStupid);
        assert_eq!(mul.compass, CompassDecision::ForceSandbox);
        assert!(!mul.dk_position.allows_adaptive());
    }

    #[test]
    fn test_mul_valley_increases_exploration() {
        let valley = MulAssessment::assess(0.2, 0.3, 0.7, 0.8, 0.5);
        let plateau = MulAssessment::assess(0.8, 0.85, 0.9, 0.9, 0.5);
        // Valley should have higher exploration rate than Plateau.
        assert!(valley.effective_exploration_rate > plateau.effective_exploration_rate);
    }

    #[test]
    fn test_mul_anxiety_tightens_fallback() {
        let flow = MulAssessment::assess(0.6, 0.6, 0.7, 0.8, 0.5);
        let anxious = MulAssessment::assess(0.6, 0.6, 0.7, 0.8, 0.95);
        // Anxiety should have tighter (higher) fallback threshold.
        assert!(anxious.effective_fallback_threshold > flow.effective_fallback_threshold);
    }

    #[test]
    fn test_mul_crystalline_trusts_topology() {
        let crystalline = MulAssessment::assess(0.7, 0.75, 0.95, 0.95, 0.5);
        let fuzzy = MulAssessment::assess(0.7, 0.75, 0.3, 0.3, 0.5);
        assert!(crystalline.free_will_modifier > fuzzy.free_will_modifier);
    }

    #[test]
    fn test_step_result_has_mul() {
        let mut orch = MetaOrchestrator::new();
        let result = orch.select_next();
        // MUL should always be present.
        assert!(result.mul.free_will_modifier >= 0.0);
    }

    #[test]
    fn test_window_size_cap() {
        let mut orch = MetaOrchestrator::new();
        for i in 0..50 {
            orch.quality_window.push(i as f32 / 50.0);
            if orch.quality_window.len() > WINDOW_SIZE {
                orch.quality_window.remove(0);
            }
        }
        assert_eq!(orch.quality_window.len(), WINDOW_SIZE);
    }

    // ── Graph Sensorium tests ──

    #[test]
    fn test_graph_sensorium_healthy() {
        let signals = GraphSensorium::compute(
            100, 2,                  // 100 active, 2 contradictions
            &[80, 10, 5, 3, 2],     // mostly certain
            5, 10,                   // 5 revisions in 10 steps
            3, 50,                   // 3/50 entities hot
            10, 15,                  // 10 deductions from 15 attempts
            5, 20,                   // 5/20 episodic
        );
        assert!(signals.contradiction_rate < 0.1);
        assert!(signals.truth_entropy < 0.5); // mostly certain
        assert_eq!(signals.suggested_bias(), GraphBias::Balanced);
    }

    #[test]
    fn test_graph_sensorium_contradicted() {
        let signals = GraphSensorium::compute(
            100, 40,                 // 40% contradictions!
            &[10, 10, 30, 30, 20],  // spread across bands
            1, 10,                   // low revision
            20, 50,                  // 40% hot
            2, 20,                   // low deduction
            18, 20,                  // near-full episodic
        );
        assert!(signals.contradiction_rate > 0.3);
        assert_eq!(signals.suggested_bias(), GraphBias::Resolve);
    }

    #[test]
    fn test_graph_sensorium_stagnant() {
        let signals = GraphSensorium::compute(
            100, 5,
            &[20, 20, 20, 20, 20],  // uniform = high entropy
            0, 20,                   // zero revisions = stagnant
            2, 100,                  // low plasticity
            0, 10,                   // zero deductions
            5, 20,
        );
        assert!(signals.revision_velocity < 0.05);
        assert!(signals.truth_entropy > 0.5);
        assert_eq!(signals.suggested_bias(), GraphBias::Stagnant);
    }

    #[test]
    fn test_temperature_rises_on_stagnation() {
        let mut orch = MetaOrchestrator::new();
        assert!((orch.temperature - 0.0).abs() < f32::EPSILON);

        let stagnant = GraphSensorium::compute(
            100, 5, &[20, 20, 20, 20, 20], 0, 20, 2, 100, 0, 10, 5, 20,
        );
        orch.update_sensorium(stagnant.clone());
        assert!(orch.temperature > 0.1);

        // Multiple stagnant updates should keep increasing temperature.
        orch.update_sensorium(stagnant);
        assert!(orch.temperature > 0.2);
    }

    #[test]
    fn test_temperature_cools_on_exploit() {
        let mut orch = MetaOrchestrator::new();
        orch.temperature = 0.5;

        let healthy = GraphSensorium::compute(
            100, 1, &[90, 5, 3, 1, 1], 8, 10, 1, 100, 12, 15, 5, 20,
        );
        orch.update_sensorium(healthy);
        assert!(orch.temperature < 0.5);
    }

    #[test]
    fn test_mul_from_graph_signals() {
        let signals = GraphSensorium::compute(
            100, 2, &[80, 10, 5, 3, 2], 5, 10, 3, 50, 10, 15, 5, 20,
        );
        let mul = MulAssessment::from_graph_signals(&signals, 0.7);
        // Healthy graph → should be Slope or Plateau.
        assert!(mul.dk_position != DkPosition::MountStupid);
        assert!(mul.free_will_modifier > 0.3);
    }

    #[test]
    fn test_auto_heal_contradictions() {
        let mut orch = MetaOrchestrator::new();
        let sick = GraphSensorium::compute(
            100, 30, &[10, 10, 30, 30, 20], 1, 10, 20, 50, 2, 20, 18, 20,
        );
        orch.update_sensorium(sick);
        let actions = orch.auto_heal();
        assert!(actions.iter().any(|a| a.action == HealingType::ResolveContradictions));
    }

    #[test]
    fn test_auto_heal_bootstrap_truth() {
        let mut orch = MetaOrchestrator::new();
        let unset = GraphSensorium::compute(
            100, 5, &[20, 20, 20, 20, 20], 0, 20, 2, 100, 0, 10, 5, 20,
        );
        orch.update_sensorium(unset);
        let actions = orch.auto_heal();
        assert!(actions.iter().any(|a| a.action == HealingType::BootstrapTruth));
    }

    #[test]
    fn test_auto_heal_topology_reset() {
        let mut orch = MetaOrchestrator::new();
        // Simulate 25 terrible steps.
        for _ in 0..25 {
            let r = orch.select_next();
            orch.record_outcome(r.style, 0.05);
        }
        // Now feed sick graph signals.
        let sick = GraphSensorium::compute(
            100, 30, &[20, 20, 20, 20, 20], 0, 20, 20, 50, 0, 20, 18, 20,
        );
        orch.update_sensorium(sick);
        let actions = orch.auto_heal();
        // Should trigger topology reset.
        assert!(actions.iter().any(|a| a.action == HealingType::ResetTopology));
        // Should be back in fallback mode.
        assert_eq!(orch.mode, OrchestratorMode::HardcodedFallback);
        // Temperature should be warm (0.5 from reset).
        assert!((orch.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_graph_bias_exploit() {
        let signals = GraphSensorium::compute(
            200, 2, &[180, 10, 5, 3, 2], 15, 20, 2, 100, 15, 20, 5, 50,
        );
        assert_eq!(signals.suggested_bias(), GraphBias::Exploit);
    }
}
