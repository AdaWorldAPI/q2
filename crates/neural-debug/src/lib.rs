//! Neural Debugger — live function-level diagnostics for the lance-graph stack.
//!
//! Compute API side: registry, probes, counters, NaN detection, strategy self-check.
//! The q2 cockpit consumes this via MCP endpoints.
//!
//! ## Architecture
//!
//! ```text
//! Compile time:  build.rs scans .rs files → FUNCTION_REGISTRY
//! Runtime:       track() / track_numeric() → CALL_COUNTER + NAN_REGISTRY
//! Self-check:    13 strategies probe their own dependency chains
//! MCP:           /api/debug/* endpoints serve diagnosis to the cockpit
//! ```

pub mod diagnosis;
pub mod instrument;
pub mod registry;
#[cfg(feature = "mcp")]
pub mod server;
pub mod strategy_check;

pub use diagnosis::{NeuralDiagnosis, NeuronDiagnosis};
pub use instrument::{track, track_numeric, CallStats};
pub use registry::{FunctionMeta, NeuronState};
pub use strategy_check::{DepStatus, PipelineCheckResult, StrategyDiagnosis};
