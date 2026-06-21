//! Compile-time function registry — every public function as a neuron.

use serde::{Deserialize, Serialize};

/// State of a neuron (function) in the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuronState {
    /// Called during execution, returns valid data.
    Alive,
    /// Compiles, has code, but never called in current execution path.
    Static,
    /// Contains todo!(), unimplemented!(), unreachable!(), panic!().
    Dead,
    /// Called, returns NaN/Inf/None where it shouldn't.
    Nan,
    /// Exists but returns hardcoded/default values.
    Stub,
    /// Called by something, but its output is never consumed.
    WiredUnused,
}

impl NeuronState {
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Alive => "🟢",
            Self::Static => "🟡",
            Self::Dead => "🔴",
            Self::Nan => "🟠",
            Self::Stub => "⚪",
            Self::WiredUnused => "🔵",
        }
    }

    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Alive)
    }
}

/// Metadata for a single function, generated at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMeta {
    /// Fully qualified function path (e.g. "lance_graph_planner::nars::deduction").
    pub id: String,
    /// Source file relative to crate root.
    pub file: String,
    /// Line number.
    pub line: u32,
    /// Module name.
    pub module: String,
    /// Function signature.
    pub signature: String,
    /// Detected at compile time: contains todo!()/unimplemented!().
    pub has_todo: bool,
    /// Detected at compile time: contains unimplemented!().
    pub has_unimplemented: bool,
    /// Return type string.
    pub return_type: String,
}

impl FunctionMeta {
    /// Heuristic: is this function a stub?
    pub fn is_stub(&self) -> bool {
        // Functions that only return Default::default() or empty Ok(())
        // This is a compile-time heuristic, refined at runtime.
        false
    }

    /// Compile-time state (before runtime data).
    pub fn compile_time_state(&self) -> NeuronState {
        if self.has_todo || self.has_unimplemented {
            NeuronState::Dead
        } else if self.is_stub() {
            NeuronState::Stub
        } else {
            NeuronState::Static // not yet called
        }
    }
}

/// A module-level summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub name: String,
    pub total: usize,
    pub alive: usize,
    pub dead: usize,
    pub nan: usize,
    pub r#static: usize,
    pub stub: usize,
    pub wired_unused: usize,
}

impl ModuleSummary {
    pub fn health_pct(&self) -> f32 {
        if self.total == 0 { return 0.0; }
        self.alive as f32 / self.total as f32 * 100.0
    }
}
