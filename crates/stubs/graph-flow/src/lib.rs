//! Graph-flow execution engine — LangGraph-style DAG orchestration.
//!
//! Migrated from rs-graph-llm. Provides the 10-layer cognitive stack
//! used by notebook-query's `%%think` command.
//!
//! Core concepts:
//! - **ThinkingGraph**: a DAG of processing nodes (layers)
//! - **Session**: execution context with typed key-value store
//! - **Channel**: communication between layers (with reducers)
//! - **ExecutionStatus**: tracks whether the graph completed or errored

pub mod graph;
pub mod storage;
pub mod thinking;
