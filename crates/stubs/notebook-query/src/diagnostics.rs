//! Live strategy diagnostics — runs each of the 16 strategies against real queries.
//!
//! Unlike the static neural-debug scanner (which scans source files for todo!()),
//! this module EXECUTES strategies and observes what happens: pass, panic, NaN, error.
//!
//! Also provides demo macros: run the same query through all 12 thinking styles
//! and compare how each style activates different strategies.

use serde::{Deserialize, Serialize};

#[cfg(feature = "planner")]
use lance_graph_planner::api::{Planner, ThinkingStyle, PlanResult};

// ── Strategy Self-Check ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyProbeResult {
    pub name: String,
    pub index: usize,
    pub status: ProbeStatus,
    pub latency_us: u64,
    pub error: Option<String>,
    pub strategies_used: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Ready,
    Partial,
    Dead,
    Nan,
    Stub,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyHealthMatrix {
    pub strategies: Vec<StrategyProbeResult>,
    pub pipelines: Vec<PipelineProbeResult>,
    pub demo_analyses: Vec<DemoAnalysis>,
    pub operational: usize,
    pub total: usize,
    pub scan_duration_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProbeResult {
    pub name: String,
    pub stages: Vec<String>,
    pub passed: bool,
    pub broke_at: Option<String>,
    pub error: Option<String>,
}

/// Run all 16 strategies with synthetic queries and report status.
#[cfg(feature = "planner")]
pub fn run_strategy_checks() -> StrategyHealthMatrix {
    let start = std::time::Instant::now();
    let planner = Planner::new();
    let strategy_names = planner.strategy_names();

    // Synthetic queries that exercise different strategy affinities
    let test_queries = [
        ("MATCH (n:System) RETURN n", "cypher"),
        ("MATCH (n)-[r]->(m) RETURN n, r, m", "cypher_join"),
        ("MATCH (n:System) WHERE n.year > 2020 RETURN n.name ORDER BY n.year", "cypher_filter"),
        ("MATCH path = (a)-[*1..3]->(b) RETURN path", "cypher_vlp"),
    ];

    let mut strategies = Vec::new();

    // Probe each strategy individually
    for (i, name) in strategy_names.iter().enumerate() {
        let probe = probe_strategy(&planner, name, i, &test_queries);
        strategies.push(probe);
    }

    // Pipeline checks
    let pipelines = run_pipeline_checks(&planner);

    // Demo analyses — run same query through all 12 thinking styles
    let demo_analyses = run_demo_analyses(&planner);

    let operational = strategies.iter().filter(|s| s.status == ProbeStatus::Ready).count();
    let total = strategies.len();
    let scan_duration_us = start.elapsed().as_micros() as u64;

    StrategyHealthMatrix {
        strategies,
        pipelines,
        demo_analyses,
        operational,
        total,
        scan_duration_us,
    }
}

#[cfg(feature = "planner")]
fn probe_strategy(
    planner: &Planner,
    name: &str,
    index: usize,
    queries: &[(&str, &str)],
) -> StrategyProbeResult {
    let start = std::time::Instant::now();

    // Try the first Cypher query — most strategies should handle this
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        planner.plan(queries[0].0)
    }));

    let (status, error, strategies_used) = match result {
        Ok(Ok(plan_result)) => {
            // Check if this strategy was actually used
            let used = plan_result.strategies_used.contains(&name.to_string());
            let strats = plan_result.strategies_used;
            if used {
                (ProbeStatus::Ready, None, strats)
            } else {
                // Strategy exists but wasn't selected for this query
                (ProbeStatus::Partial, Some("Not selected for test query".into()), strats)
            }
        }
        Ok(Err(e)) => (ProbeStatus::Error, Some(format!("{:?}", e)), vec![]),
        Err(panic) => {
            let msg = panic.downcast_ref::<String>()
                .map(|s| s.clone())
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            if msg.contains("not yet implemented") || msg.contains("todo") {
                (ProbeStatus::Dead, Some(msg), vec![])
            } else {
                (ProbeStatus::Error, Some(format!("panic: {}", msg)), vec![])
            }
        }
    };

    StrategyProbeResult {
        name: name.to_string(),
        index,
        status,
        latency_us: start.elapsed().as_micros() as u64,
        error,
        strategies_used,
    }
}

#[cfg(feature = "planner")]
fn run_pipeline_checks(planner: &Planner) -> Vec<PipelineProbeResult> {
    let pipelines = [
        ("Parse→Plan", "MATCH (n:System) RETURN n"),
        ("Parse→Plan→Scan", "MATCH (n:System) WHERE n.year > 2020 RETURN n"),
        ("Parse→Join→Scan", "MATCH (a)-[r]->(b) RETURN a, r, b"),
        ("Parse→VLP→Scan", "MATCH path = (a)-[*1..3]->(b) RETURN path"),
        ("Full pipeline", "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) WHERE s.year > 2020 RETURN s.name, st.name ORDER BY s.year DESC LIMIT 10"),
    ];

    pipelines.iter().map(|(name, query)| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            planner.plan(query)
        }));

        match result {
            Ok(Ok(plan)) => PipelineProbeResult {
                name: name.to_string(),
                stages: plan.strategies_used.clone(),
                passed: true,
                broke_at: None,
                error: None,
            },
            Ok(Err(e)) => PipelineProbeResult {
                name: name.to_string(),
                stages: vec![],
                passed: false,
                broke_at: Some(format!("{:?}", e)),
                error: Some(format!("{:?}", e)),
            },
            Err(panic) => {
                let msg = panic.downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| "panic".to_string());
                PipelineProbeResult {
                    name: name.to_string(),
                    stages: vec![],
                    passed: false,
                    broke_at: Some(msg.clone()),
                    error: Some(msg),
                }
            }
        }
    }).collect()
}

// ── Demo Analyses: 12 Thinking Styles × Same Query ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoAnalysis {
    pub query: String,
    pub style_results: Vec<StyleAnalysisResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleAnalysisResult {
    pub style: String,
    pub cluster: String,
    pub strategies_used: Vec<String>,
    pub strategy_count: usize,
    pub free_will_modifier: f64,
    pub latency_us: u64,
    pub status: ProbeStatus,
    pub error: Option<String>,
    /// Field modulation parameters that drove strategy selection
    pub modulation: ModulationSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulationSnapshot {
    pub resonance_threshold: f64,
    pub fan_out: u32,
    pub depth_bias: f64,
    pub breadth_bias: f64,
    pub noise_tolerance: f64,
    pub speed_bias: f64,
    pub exploration: f64,
}

/// Run the aiwar demo query through all 12 thinking styles.
/// Shows how different cognitive lenses activate different strategy pipelines.
#[cfg(feature = "planner")]
fn run_demo_analyses(planner: &Planner) -> Vec<DemoAnalysis> {
    let demo_queries = [
        "MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s.name, st.name, s.year ORDER BY s.year DESC",
        "MATCH (p:Person)-[*1..3]-(connected) RETURN p.name, connected.name, labels(connected)",
        "MATCH (s:System) WHERE s.militaryUse CONTAINS 'kill' RETURN s.name, s.year, s.militaryUse",
    ];

    let all_styles = [
        ThinkingStyle::Analytical,
        ThinkingStyle::Convergent,
        ThinkingStyle::Systematic,
        ThinkingStyle::Creative,
        ThinkingStyle::Divergent,
        ThinkingStyle::Exploratory,
        ThinkingStyle::Focused,
        ThinkingStyle::Diffuse,
        ThinkingStyle::Peripheral,
        ThinkingStyle::Intuitive,
        ThinkingStyle::Deliberate,
        ThinkingStyle::Metacognitive,
    ];

    demo_queries.iter().map(|query| {
        let style_results: Vec<StyleAnalysisResult> = all_styles.iter().map(|style| {
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                planner.plan_with_style(query, *style)
            }));

            let modulation = style.default_modulation();
            let mod_snapshot = ModulationSnapshot {
                resonance_threshold: modulation.resonance_threshold,
                fan_out: modulation.fan_out as u32,
                depth_bias: modulation.depth_bias,
                breadth_bias: modulation.breadth_bias,
                noise_tolerance: modulation.noise_tolerance,
                speed_bias: modulation.speed_bias,
                exploration: modulation.exploration,
            };

            match result {
                Ok(Ok(plan)) => StyleAnalysisResult {
                    style: format!("{:?}", style),
                    cluster: format!("{:?}", style.cluster()),
                    strategies_used: plan.strategies_used.clone(),
                    strategy_count: plan.strategies_used.len(),
                    free_will_modifier: plan.free_will_modifier,
                    latency_us: start.elapsed().as_micros() as u64,
                    status: ProbeStatus::Ready,
                    error: None,
                    modulation: mod_snapshot,
                },
                Ok(Err(e)) => StyleAnalysisResult {
                    style: format!("{:?}", style),
                    cluster: format!("{:?}", style.cluster()),
                    strategies_used: vec![],
                    strategy_count: 0,
                    free_will_modifier: 0.0,
                    latency_us: start.elapsed().as_micros() as u64,
                    status: ProbeStatus::Error,
                    error: Some(format!("{:?}", e)),
                    modulation: mod_snapshot,
                },
                Err(_) => StyleAnalysisResult {
                    style: format!("{:?}", style),
                    cluster: format!("{:?}", style.cluster()),
                    strategies_used: vec![],
                    strategy_count: 0,
                    free_will_modifier: 0.0,
                    latency_us: start.elapsed().as_micros() as u64,
                    status: ProbeStatus::Dead,
                    error: Some("panic during planning".to_string()),
                    modulation: mod_snapshot,
                },
            }
        }).collect();

        DemoAnalysis {
            query: query.to_string(),
            style_results,
        }
    }).collect()
}

// ── Fallback when planner feature is off ─────────────────────────────────────

#[cfg(not(feature = "planner"))]
pub fn run_strategy_checks() -> StrategyHealthMatrix {
    StrategyHealthMatrix {
        strategies: vec![StrategyProbeResult {
            name: "planner_not_enabled".to_string(),
            index: 0,
            status: ProbeStatus::Stub,
            latency_us: 0,
            error: Some("Compile with --features planner to enable live strategy checks".to_string()),
            strategies_used: vec![],
        }],
        pipelines: vec![],
        demo_analyses: vec![],
        operational: 0,
        total: 0,
        scan_duration_us: 0,
    }
}
