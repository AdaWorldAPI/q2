//! Thinking orchestrator — routes `%%think` queries through the 10-layer cognitive stack.
//!
//! Requires `--features orchestrator` to enable.
//!
//! Instead of just running a Cypher query, `%%think` routes through:
//! 1. Sensory ingest: parse the query
//! 2. Fingerprint: compute query fingerprint via lance-graph
//! 3. Cascade search: HHTL attention band (Foveal/Parafoveal)
//! 4. Semiring reasoning: walk_chain_forward + inference
//! 5. Memory consolidation: NARS revision + seal check
//! 6-9. Planning → action → output → meta-cognition
//! 10. PET scan trace: which layers fired
//!
//! The cockpit receives the PET scan trace alongside the query result.

use std::sync::Arc;

use graph_flow::graph::ExecutionStatus;
use graph_flow::storage::Session;
use graph_flow::thinking::build_thinking_graph;

/// Result of a `%%think` query — includes both the query result and the PET scan trace.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThinkResult {
    /// The query output (from Layer 9).
    pub output: String,
    /// PET scan trace: which layers fired.
    pub pet_scan: serde_json::Value,
    /// Attention band (Foveal = familiar, Parafoveal = novel).
    pub band: String,
    /// Whether new learning occurred (Staunen) or knowledge was stable (Wisdom).
    pub staunen: bool,
    /// Number of layers that executed.
    pub layers_executed: usize,
    /// Total execution time in microseconds.
    pub elapsed_us: u64,
}

/// Execute a `%%think` query through the 10-layer cognitive stack.
///
/// This routes the Cypher query through the thinking graph:
/// sensory → fingerprint → cascade → (reasoning?) → memory → plan → act → output → meta
pub async fn execute_think(source: &str) -> Result<ThinkResult, String> {
    let graph = build_thinking_graph();
    let t0 = std::time::Instant::now();

    let mut session = Session::new_from_task(
        format!("think-{}", uuid::Uuid::new_v4()),
        "sensory_ingest",
    );
    session.context.set("raw_input", source.to_string()).await;

    // Execute all layers until completion (max 50 steps for safety)
    for _ in 0..50 {
        let result = graph
            .execute_session(&mut session)
            .await
            .map_err(|e| format!("Thinking graph error: {e}"))?;
        match result.status {
            ExecutionStatus::Completed => break,
            ExecutionStatus::Error(e) => return Err(format!("Thinking error: {e}")),
            _ => {}
        }
    }

    let elapsed_us = t0.elapsed().as_micros() as u64;

    // Extract results from context
    let output: String = session
        .context
        .get("output")
        .await
        .unwrap_or_else(|| "(no output)".to_string());
    let pet_scan: serde_json::Value = session
        .context
        .get("pet_scan")
        .await
        .unwrap_or(serde_json::json!({}));
    let band: String = session
        .context
        .get("best_band")
        .await
        .unwrap_or_else(|| "Unknown".to_string());
    let staunen: bool = session.context.get("staunen").await.unwrap_or(false);
    let layers_executed = pet_scan["layers_executed"].as_u64().unwrap_or(0) as usize;

    Ok(ThinkResult {
        output,
        pet_scan,
        band,
        staunen,
        layers_executed,
        elapsed_us,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_think() {
        let result = execute_think("MATCH (n:System) RETURN n.name").await.unwrap();
        assert!(!result.output.is_empty());
        assert!(result.layers_executed >= 6);
        assert!(!result.band.is_empty());
    }

    #[tokio::test]
    async fn test_think_pet_scan_trace() {
        let result = execute_think("Hello").await.unwrap();
        let trace = result.pet_scan["trace"].as_array();
        assert!(trace.is_some());
        assert!(!trace.unwrap().is_empty());
    }
}
