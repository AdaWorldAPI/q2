//! MCP server endpoints for the neural debugger.
//!
//! Consumed by the q2 cockpit dashboard.
//! Feature-gated behind `mcp`.

use serde_json::json;

use crate::diagnosis::NeuralDiagnosis;
use crate::registry::FunctionMeta;
use crate::strategy_check::StrategyHealthMatrix;

/// Build JSON response for /api/debug/registry
pub fn registry_json(registry: &[FunctionMeta]) -> serde_json::Value {
    json!({
        "functions": registry,
        "total": registry.len(),
        "dead": registry.iter().filter(|f| f.has_todo || f.has_unimplemented).count(),
    })
}

/// Build JSON response for /api/debug/diagnosis
pub fn diagnosis_json(registry: &[FunctionMeta]) -> serde_json::Value {
    let diagnosis = NeuralDiagnosis::diagnose(registry);
    serde_json::to_value(&diagnosis).unwrap_or(json!({"error": "serialization failed"}))
}

/// Build JSON response for /api/debug/nan
pub fn nan_json() -> serde_json::Value {
    let nans = crate::instrument::nan_functions();
    json!({
        "nan_producers": nans.iter().map(|(id, events)| {
            json!({
                "function_id": id,
                "events": events,
                "count": events.len(),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Build JSON response for /api/debug/strategies
pub fn strategies_json(matrix: &StrategyHealthMatrix) -> serde_json::Value {
    serde_json::to_value(matrix).unwrap_or(json!({"error": "serialization failed"}))
}

/// Build JSON response for /api/debug/impact (prioritized fix list)
pub fn impact_json(matrix: &StrategyHealthMatrix) -> serde_json::Value {
    json!({
        "fixes": matrix.fixes,
        "operational": matrix.operational_count,
        "total": matrix.total_count,
        "can_do": matrix.can_do,
        "cannot_do": matrix.cannot_do,
    })
}

/// Build JSON response for /api/debug/coverage (per-module health)
pub fn coverage_json(registry: &[FunctionMeta]) -> serde_json::Value {
    let diagnosis = NeuralDiagnosis::diagnose(registry);
    json!({
        "modules": diagnosis.modules,
        "alive_pct": diagnosis.alive_pct,
        "total_functions": diagnosis.total_functions,
    })
}
