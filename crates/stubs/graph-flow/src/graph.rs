//! Execution graph — DAG of processing nodes with status tracking.

use serde::{Deserialize, Serialize};

/// Status of a graph execution step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// All nodes completed successfully.
    Completed,
    /// Still running — more steps to execute.
    Running,
    /// An error occurred at a specific node.
    Error(String),
    /// Execution was cancelled.
    Cancelled,
}

/// Result of executing one step of the graph.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub status: ExecutionStatus,
    pub node_name: String,
    pub output: Option<serde_json::Value>,
}
