//! Runtime instrumentation — call counters, timing, NaN detection.
//!
//! Feature-gated behind `instrument`. Off by default in production.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Global call counter (thread-safe).
static COUNTERS: Mutex<Option<Counters>> = Mutex::new(None);

struct Counters {
    calls: HashMap<String, u64>,
    timing: HashMap<String, Vec<Duration>>,
    nan_producers: HashMap<String, Vec<NanEvent>>,
    last_called: HashMap<String, Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NanEvent {
    pub timestamp_ms: u64,
    pub input_snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallStats {
    pub call_count: u64,
    pub avg_duration_us: f64,
    pub max_duration_us: f64,
    pub nan_count: usize,
    pub last_called_ago_ms: Option<u64>,
}

fn counters() -> parking_lot::MutexGuard<'static, Option<Counters>> {
    let mut guard = COUNTERS.lock();
    if guard.is_none() {
        *guard = Some(Counters {
            calls: HashMap::new(),
            timing: HashMap::new(),
            nan_producers: HashMap::new(),
            last_called: HashMap::new(),
        });
    }
    guard
}

/// Track a function call. Returns the function's return value.
#[inline]
pub fn track<T>(_function_id: &str, f: impl FnOnce() -> T) -> T {
    #[cfg(not(feature = "instrument"))]
    { return f(); }

    #[cfg(feature = "instrument")]
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();

        let mut c = counters();
        let c = c.as_mut().unwrap();
        *c.calls.entry(function_id.to_string()).or_insert(0) += 1;
        c.timing.entry(function_id.to_string()).or_default().push(elapsed);
        c.last_called.insert(function_id.to_string(), Instant::now());

        result
    }
}

/// Track a function returning f32, also checks for NaN.
#[inline]
pub fn track_numeric(_function_id: &str, value: f32) -> f32 {
    #[cfg(not(feature = "instrument"))]
    { return value; }

    #[cfg(feature = "instrument")]
    {
        let mut c = counters();
        let c = c.as_mut().unwrap();
        *c.calls.entry(function_id.to_string()).or_insert(0) += 1;

        if value.is_nan() || value.is_infinite() {
            c.nan_producers.entry(function_id.to_string()).or_default().push(NanEvent {
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                input_snapshot: format!("returned {value}"),
            });
        }

        value
    }
}

/// Track f64 variant.
#[inline]
pub fn track_numeric_f64(_function_id: &str, value: f64) -> f64 {
    #[cfg(not(feature = "instrument"))]
    { return value; }

    #[cfg(feature = "instrument")]
    {
        let mut c = counters();
        let c = c.as_mut().unwrap();
        *c.calls.entry(function_id.to_string()).or_insert(0) += 1;

        if value.is_nan() || value.is_infinite() {
            c.nan_producers.entry(function_id.to_string()).or_default().push(NanEvent {
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                input_snapshot: format!("returned {value}"),
            });
        }

        value
    }
}

/// Get stats for a function.
pub fn get_stats(function_id: &str) -> CallStats {
    let c = counters();
    let c = c.as_ref().unwrap();
    let call_count = c.calls.get(function_id).copied().unwrap_or(0);
    let timings = c.timing.get(function_id);
    let nan_count = c.nan_producers.get(function_id).map(|v| v.len()).unwrap_or(0);

    let (avg_us, max_us) = if let Some(ts) = timings {
        if ts.is_empty() {
            (0.0, 0.0)
        } else {
            let total: Duration = ts.iter().sum();
            let avg = total.as_micros() as f64 / ts.len() as f64;
            let max = ts.iter().map(|d| d.as_micros()).max().unwrap_or(0) as f64;
            (avg, max)
        }
    } else {
        (0.0, 0.0)
    };

    let last_called_ago_ms = c.last_called.get(function_id).map(|t| t.elapsed().as_millis() as u64);

    CallStats {
        call_count,
        avg_duration_us: avg_us,
        max_duration_us: max_us,
        nan_count,
        last_called_ago_ms,
    }
}

/// Get all function IDs that have been called.
pub fn alive_functions() -> Vec<String> {
    let c = counters();
    let c = c.as_ref().unwrap();
    c.calls.keys().cloned().collect()
}

/// Get all NaN-producing function IDs.
pub fn nan_functions() -> Vec<(String, Vec<NanEvent>)> {
    let c = counters();
    let c = c.as_ref().unwrap();
    c.nan_producers.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Reset all counters.
pub fn reset() {
    let mut c = COUNTERS.lock();
    *c = None;
}
