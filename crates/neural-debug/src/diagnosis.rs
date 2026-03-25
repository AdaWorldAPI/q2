//! Diagnosis engine — combines compile-time registry with runtime data.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::registry::{FunctionMeta, ModuleSummary, NeuronState};
use crate::instrument::{self, CallStats};

/// Diagnosis for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronDiagnosis {
    pub id: String,
    pub file: String,
    pub line: u32,
    pub module: String,
    pub state: NeuronState,
    pub call_count: u64,
    pub avg_time_us: f64,
    pub nan_count: usize,
}

/// Full stack diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralDiagnosis {
    pub neurons: Vec<NeuronDiagnosis>,
    pub modules: Vec<ModuleSummary>,
    pub total_functions: usize,
    pub alive_pct: f32,
    pub dead_count: usize,
    pub nan_count: usize,
}

impl NeuralDiagnosis {
    /// Combine compile-time registry with runtime instrumentation data.
    pub fn diagnose(registry: &[FunctionMeta]) -> Self {
        let alive_set: HashSet<String> = instrument::alive_functions().into_iter().collect();
        let nan_set: HashSet<String> = instrument::nan_functions()
            .into_iter()
            .map(|(id, _)| id)
            .collect();

        let mut neurons = Vec::with_capacity(registry.len());
        let mut module_map: HashMap<String, Vec<NeuronState>> = HashMap::new();

        for func in registry {
            let stats = instrument::get_stats(&func.id);

            let state = if func.has_todo || func.has_unimplemented {
                NeuronState::Dead
            } else if nan_set.contains(&func.id) {
                NeuronState::Nan
            } else if alive_set.contains(&func.id) {
                NeuronState::Alive
            } else if func.is_stub() {
                NeuronState::Stub
            } else {
                NeuronState::Static
            };

            module_map.entry(func.module.clone()).or_default().push(state);

            neurons.push(NeuronDiagnosis {
                id: func.id.clone(),
                file: func.file.clone(),
                line: func.line,
                module: func.module.clone(),
                state,
                call_count: stats.call_count,
                avg_time_us: stats.avg_duration_us,
                nan_count: stats.nan_count,
            });
        }

        let modules: Vec<ModuleSummary> = module_map
            .into_iter()
            .map(|(name, states)| {
                let total = states.len();
                let alive = states.iter().filter(|s| **s == NeuronState::Alive).count();
                let dead = states.iter().filter(|s| **s == NeuronState::Dead).count();
                let nan = states.iter().filter(|s| **s == NeuronState::Nan).count();
                let stat = states.iter().filter(|s| **s == NeuronState::Static).count();
                let stub = states.iter().filter(|s| **s == NeuronState::Stub).count();
                let wired = states.iter().filter(|s| **s == NeuronState::WiredUnused).count();
                ModuleSummary { name, total, alive, dead, nan, r#static: stat, stub, wired_unused: wired }
            })
            .collect();

        let total_functions = neurons.len();
        let alive_count = neurons.iter().filter(|n| n.state == NeuronState::Alive).count();
        let dead_count = neurons.iter().filter(|n| n.state == NeuronState::Dead).count();
        let nan_count = neurons.iter().filter(|n| n.state == NeuronState::Nan).count();

        NeuralDiagnosis {
            neurons,
            modules,
            total_functions,
            alive_pct: if total_functions > 0 { alive_count as f32 / total_functions as f32 * 100.0 } else { 0.0 },
            dead_count,
            nan_count,
        }
    }
}
