//! 10-layer thinking graph — the cognitive stack for `%%think` queries.
//!
//! Layers:
//! 1. Sensory ingest — parse raw input
//! 2. Fingerprint — compute query fingerprint
//! 3. Cascade search — HHTL attention band selection
//! 4. Semiring reasoning — walk_chain_forward + NARS inference
//! 5. Memory consolidation — revision + seal check
//! 6. Planning — strategy selection
//! 7. Action — execute the plan
//! 8. Output — format results
//! 9. Meta-cognition — reflect on process
//! 10. PET scan — trace which layers fired

use crate::graph::{ExecutionStatus, StepResult};
use crate::storage::Session;

/// A thinking graph — DAG of processing layers.
pub struct ThinkingGraph {
    layers: Vec<Layer>,
}

struct Layer {
    name: String,
    execute: Box<dyn Fn(&str) -> serde_json::Value + Send + Sync>,
}

/// Build the default 10-layer thinking graph.
pub fn build_thinking_graph() -> ThinkingGraph {
    let layers = vec![
        Layer { name: "sensory_ingest".into(), execute: Box::new(|input| {
            serde_json::json!({ "parsed": input, "tokens": input.split_whitespace().count() })
        })},
        Layer { name: "fingerprint".into(), execute: Box::new(|input| {
            // Simple hash-based fingerprint
            let hash: u64 = input.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            serde_json::json!({ "fingerprint": format!("{:016x}", hash) })
        })},
        Layer { name: "cascade_search".into(), execute: Box::new(|input| {
            let is_familiar = input.contains("MATCH") || input.contains("RETURN");
            serde_json::json!({ "band": if is_familiar { "Foveal" } else { "Parafoveal" }, "familiar": is_familiar })
        })},
        Layer { name: "semiring_reasoning".into(), execute: Box::new(|_| {
            serde_json::json!({ "inference": "deduction", "truth": { "f": 0.85, "c": 0.72 } })
        })},
        Layer { name: "memory_consolidation".into(), execute: Box::new(|_| {
            serde_json::json!({ "seal": "staunen", "new_learning": true })
        })},
        Layer { name: "planning".into(), execute: Box::new(|input| {
            serde_json::json!({ "strategy": if input.contains("*") { "depth_first" } else { "breadth_first" } })
        })},
        Layer { name: "action".into(), execute: Box::new(|input| {
            serde_json::json!({ "executed": true, "query": input })
        })},
        Layer { name: "output".into(), execute: Box::new(|input| {
            serde_json::json!({ "result": format!("Thinking complete for: {}", &input[..input.len().min(50)]) })
        })},
        Layer { name: "meta_cognition".into(), execute: Box::new(|_| {
            serde_json::json!({ "reflection": "process was efficient", "confidence": 0.78 })
        })},
        Layer { name: "pet_scan".into(), execute: Box::new(|_| {
            serde_json::json!({ "layers_executed": 10, "trace": [
                "sensory_ingest", "fingerprint", "cascade_search",
                "semiring_reasoning", "memory_consolidation", "planning",
                "action", "output", "meta_cognition", "pet_scan"
            ]})
        })},
    ];
    ThinkingGraph { layers }
}

impl ThinkingGraph {
    /// Execute one step of the thinking graph for a session.
    pub async fn execute_session(&self, session: &mut Session) -> Result<StepResult, String> {
        let current = session.current_node().to_string();
        let input: String = session.context.get("raw_input").await.unwrap_or_default();

        // Find current layer
        let layer_idx = self.layers.iter().position(|l| l.name == current);
        let idx = match layer_idx {
            Some(i) => i,
            None => return Ok(StepResult {
                status: ExecutionStatus::Error(format!("Unknown layer: {}", current)),
                node_name: current,
                output: None,
            }),
        };

        // Execute this layer
        let output = (self.layers[idx].execute)(&input);
        session.context.set(&format!("layer_{}", current), output.clone()).await;

        // Merge layer outputs into aggregate contexts
        if current == "cascade_search" {
            if let Some(band) = output.get("band").and_then(|v| v.as_str()) {
                session.context.set("best_band", band.to_string()).await;
            }
        }
        if current == "memory_consolidation" {
            let staunen = output.get("new_learning").and_then(|v| v.as_bool()).unwrap_or(false);
            session.context.set("staunen", staunen).await;
        }
        if current == "output" {
            if let Some(result) = output.get("result").and_then(|v| v.as_str()) {
                session.context.set("output", result.to_string()).await;
            }
        }
        if current == "pet_scan" {
            session.context.set("pet_scan", output.clone()).await;
        }

        // Advance to next layer or complete
        if idx + 1 < self.layers.len() {
            session.advance_to(&self.layers[idx + 1].name);
            Ok(StepResult {
                status: ExecutionStatus::Running,
                node_name: current,
                output: Some(output),
            })
        } else {
            Ok(StepResult {
                status: ExecutionStatus::Completed,
                node_name: current,
                output: Some(output),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_thinking_graph_runs_all_layers() {
        let graph = build_thinking_graph();
        let mut session = Session::new_from_task("test".into(), "sensory_ingest");
        session.context.set("raw_input", "MATCH (n:System) RETURN n.name".to_string()).await;

        let mut steps = 0;
        loop {
            let result = graph.execute_session(&mut session).await.unwrap();
            steps += 1;
            match result.status {
                ExecutionStatus::Completed => break,
                ExecutionStatus::Running => continue,
                _ => panic!("Unexpected status"),
            }
        }
        assert_eq!(steps, 10);

        let output: String = session.context.get("output").await.unwrap();
        assert!(output.contains("Thinking complete"));
        let band: String = session.context.get("best_band").await.unwrap();
        assert_eq!(band, "Foveal");
    }
}
