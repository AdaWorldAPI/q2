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
        }
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

        // ── MUL Assessment ──
        let avg_confidence = self.topology.edges.values()
            .filter(|e| e.observations > 0)
            .map(|e| e.truth.confidence)
            .sum::<f64>()
            / self.topology.edges.values().filter(|e| e.observations > 0).count().max(1) as f64;
        let mul = MulAssessment::from_efficiency(self.rolling_efficiency(), avg_confidence as f32);
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
                    // Exploit: pick highest expected quality, SCALED by MUL free_will
                    let mut best = AgentStyle::Plan;
                    let mut best_eq = f64::NEG_INFINITY;
                    for &to in AgentStyle::all() {
                        let eq = self.topology.edge(current, to).expected_quality()
                            * mul.free_will_modifier as f64;
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
}
