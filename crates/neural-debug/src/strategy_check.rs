//! Strategy self-check — each of the 13 planning strategies probes its own deps.
//!
//! This is the centerpiece: run all 13 at startup → instant health matrix.

use serde::{Deserialize, Serialize};

/// Status of a single dependency probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepStatus {
    /// Dependency works, returns valid data.
    Alive,
    /// Dependency returns NaN/Inf.
    Nan { input: String },
    /// Dependency returns default/empty values.
    Stub,
    /// Dependency panics with todo!/unimplemented!.
    Dead { reason: String },
    /// Dependency panics with other error.
    Error { message: String },
}

impl DepStatus {
    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Alive)
    }
}

/// Result of probing a single dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepCheck {
    pub name: String,
    pub status: DepStatus,
    pub location: String,
    pub error: Option<String>,
    pub latency_us: u64,
}

/// Verdict for a strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// All deps alive, self-test passes.
    Ready,
    /// Some deps work, some don't.
    Partial,
    /// At least one dep returns NaN.
    Nan,
    /// At least one dep is dead (todo!/panic).
    Dead,
    /// All deps return defaults.
    Stub,
}

/// Result of a synthetic self-test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticTestResult {
    pub passed: bool,
    pub latency_us: u64,
    pub output_summary: String,
}

/// Full diagnosis for one strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDiagnosis {
    pub strategy: String,
    pub strategy_index: usize,
    pub deps: Vec<DepCheck>,
    pub self_test: Option<SyntheticTestResult>,
    pub verdict: Verdict,
}

impl StrategyDiagnosis {
    pub fn first_error(&self) -> Option<String> {
        self.deps.iter().find_map(|d| d.error.clone())
    }

    pub fn alive_deps(&self) -> usize {
        self.deps.iter().filter(|d| d.status.is_operational()).count()
    }

    pub fn total_deps(&self) -> usize {
        self.deps.len()
    }
}

/// Result of a single pipeline stage check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub strategy_index: usize,
    pub strategy_name: String,
    pub verdict: Verdict,
    pub broke_at: bool,
    pub error: Option<String>,
}

/// Result of running a pipeline chain check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCheckResult {
    pub name: String,
    pub stages: Vec<StageResult>,
    pub fully_operational: bool,
    pub broke_at_stage: Option<String>,
}

/// Fix recommendation sorted by impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixRecommendation {
    pub function_id: String,
    pub location: String,
    pub state: String,
    pub blocks_strategies: Vec<String>,
    pub blocks_pipelines: Vec<String>,
    pub fix_description: String,
    pub impact_score: f32,
}

/// All 13 strategies health matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyHealthMatrix {
    pub strategies: Vec<StrategyDiagnosis>,
    pub pipelines: Vec<PipelineCheckResult>,
    pub fixes: Vec<FixRecommendation>,
    pub operational_count: usize,
    pub total_count: usize,
    pub can_do: Vec<String>,
    pub cannot_do: Vec<String>,
}

/// The 13 registered strategy names (matches lance-graph-planner).
pub const STRATEGY_NAMES: &[&str] = &[
    "cypher_parse",
    "gremlin_parse",
    "sparql_parse",
    "gql_parse",
    "arena_ir",
    "dp_join",
    "workflow_dag",
    "rule_optimizer",
    "histogram_cost",
    "sigma_scan",
    "morsel_exec",
    "truth_propagation",
    "collapse_gate",
    // Extensions:
    "stream_pipeline",
    "jit_compile",
    "extension",
];

/// Default pipeline chain definitions.
pub fn default_pipeline_checks() -> Vec<(&'static str, Vec<usize>)> {
    vec![
        ("Parse→Scan→Collapse", vec![0, 9, 12]),
        ("Parse→Scan→Truth→Collapse", vec![0, 9, 11, 12]),
        ("Parse→DPJoin→Scan→Collapse", vec![0, 5, 9, 12]),
        ("Parse→Optimize→Scan→Collapse", vec![0, 7, 9, 12]),
        ("Full pipeline (all 13)", vec![0, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dep_status_operational() {
        assert!(DepStatus::Alive.is_operational());
        assert!(!DepStatus::Dead { reason: "todo".into() }.is_operational());
        assert!(!DepStatus::Nan { input: "0.0".into() }.is_operational());
        assert!(!DepStatus::Stub.is_operational());
    }

    #[test]
    fn test_strategy_names_count() {
        assert_eq!(STRATEGY_NAMES.len(), 16); // 13 core + 3 extension
    }

    #[test]
    fn test_verdict_variants() {
        let diag = StrategyDiagnosis {
            strategy: "test".into(),
            strategy_index: 0,
            deps: vec![
                DepCheck { name: "a".into(), status: DepStatus::Alive, location: "a.rs:1".into(), error: None, latency_us: 100 },
                DepCheck { name: "b".into(), status: DepStatus::Dead { reason: "todo".into() }, location: "b.rs:1".into(), error: Some("todo".into()), latency_us: 0 },
            ],
            self_test: None,
            verdict: Verdict::Partial,
        };
        assert_eq!(diag.alive_deps(), 1);
        assert_eq!(diag.total_deps(), 2);
        assert_eq!(diag.first_error(), Some("todo".to_string()));
    }
}
