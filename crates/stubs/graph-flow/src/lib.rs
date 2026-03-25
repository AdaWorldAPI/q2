//! Stub for rs-graph-llm graph-flow — provides type signatures
//! used by notebook-query's orchestrator feature.

pub mod graph {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum ExecutionStatus {
        Success,
        Failed(String),
        Pending,
    }
}

pub mod storage {
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    pub struct Session {
        pub id: String,
    }

    impl Session {
        pub fn new() -> Self {
            Self { id: uuid_stub() }
        }
    }

    fn uuid_stub() -> String {
        format!("session-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis())
    }
}

pub mod thinking {
    use crate::graph::ExecutionStatus;
    use crate::storage::Session;

    pub struct ThinkingGraph {
        pub session: Session,
    }

    pub fn build_thinking_graph(session: Session) -> ThinkingGraph {
        ThinkingGraph { session }
    }

    impl ThinkingGraph {
        pub async fn run(&self, _input: &str) -> (ExecutionStatus, serde_json::Value) {
            (
                ExecutionStatus::Success,
                serde_json::json!({
                    "layers": [],
                    "stub": true,
                    "message": "graph-flow stub — real orchestrator not linked"
                }),
            )
        }
    }
}
