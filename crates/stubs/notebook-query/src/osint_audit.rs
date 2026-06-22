//! Live OSINT pipeline auditing — real-time health monitoring for AriGraph.
//!
//! Uses neural-debug's `RuntimeRegistry` pattern for atomic call counting,
//! combined with AriGraph graph health metrics (triplet count, truth distribution,
//! contradiction detection, episodic memory saturation).
//!
//! Exposed via q2 cockpit-server at `/api/debug/osint`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

// ── Global OSINT Pipeline Registry ──────────────────────────────────────────

/// Global singleton registry for OSINT pipeline call tracking.
/// Same pattern as neural-debug's RuntimeRegistry but specialized for AriGraph ops.
static OSINT_REGISTRY: OnceLock<OsintRegistry> = OnceLock::new();

/// Get or initialize the global OSINT registry.
pub fn osint_registry() -> &'static OsintRegistry {
    OSINT_REGISTRY.get_or_init(OsintRegistry::new)
}

/// Atomic counter for a single OSINT operation.
pub struct OsintCounter {
    pub calls: AtomicU64,
    pub successes: AtomicU64,
    pub failures: AtomicU64,
    pub total_ns: AtomicU64,
    pub triplets_produced: AtomicU64,
}

impl OsintCounter {
    pub const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            total_ns: AtomicU64::new(0),
            triplets_produced: AtomicU64::new(0),
        }
    }

    pub fn record_success(&self, elapsed_ns: u64, triplets: u64) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.successes.fetch_add(1, Ordering::Relaxed);
        self.total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
        self.triplets_produced.fetch_add(triplets, Ordering::Relaxed);
    }

    pub fn record_failure(&self, elapsed_ns: u64) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
        self.total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> OsintCounterSnapshot {
        let calls = self.calls.load(Ordering::Relaxed);
        let total_ns = self.total_ns.load(Ordering::Relaxed);
        OsintCounterSnapshot {
            calls,
            successes: self.successes.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            avg_latency_us: if calls > 0 {
                total_ns / calls / 1000
            } else {
                0
            },
            triplets_produced: self.triplets_produced.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsintCounterSnapshot {
    pub calls: u64,
    pub successes: u64,
    pub failures: u64,
    pub avg_latency_us: u64,
    pub triplets_produced: u64,
}

/// Registry tracking all OSINT pipeline stages.
pub struct OsintRegistry {
    pub extraction: OsintCounter,
    pub refinement: OsintCounter,
    pub planning: OsintCounter,
    pub classification: OsintCounter,
    pub deduction: OsintCounter,
    pub contradiction: OsintCounter,
    pub revision: OsintCounter,
    pub episodic_store: OsintCounter,
    pub episodic_retrieve: OsintCounter,
    pub graph_bfs: OsintCounter,
    pub spatial_path: OsintCounter,
    pub xai_api_call: OsintCounter,
}

impl OsintRegistry {
    pub fn new() -> Self {
        Self {
            extraction: OsintCounter::new(),
            refinement: OsintCounter::new(),
            planning: OsintCounter::new(),
            classification: OsintCounter::new(),
            deduction: OsintCounter::new(),
            contradiction: OsintCounter::new(),
            revision: OsintCounter::new(),
            episodic_store: OsintCounter::new(),
            episodic_retrieve: OsintCounter::new(),
            graph_bfs: OsintCounter::new(),
            spatial_path: OsintCounter::new(),
            xai_api_call: OsintCounter::new(),
        }
    }

    /// Full snapshot of all pipeline stages.
    pub fn snapshot(&self) -> OsintPipelineHealth {
        OsintPipelineHealth {
            stages: vec![
                ("extraction".into(), self.extraction.snapshot()),
                ("refinement".into(), self.refinement.snapshot()),
                ("planning".into(), self.planning.snapshot()),
                ("classification".into(), self.classification.snapshot()),
                ("deduction".into(), self.deduction.snapshot()),
                ("contradiction".into(), self.contradiction.snapshot()),
                ("revision".into(), self.revision.snapshot()),
                ("episodic_store".into(), self.episodic_store.snapshot()),
                ("episodic_retrieve".into(), self.episodic_retrieve.snapshot()),
                ("graph_bfs".into(), self.graph_bfs.snapshot()),
                ("spatial_path".into(), self.spatial_path.snapshot()),
                ("xai_api_call".into(), self.xai_api_call.snapshot()),
            ],
        }
    }
}

// ── Graph Health Report ─────────────────────────────────────────────────────

/// Comprehensive health report for the OSINT knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsintGraphHealth {
    /// Total triplets in the graph (including soft-deleted).
    pub total_triplets: usize,
    /// Active (non-deleted) triplets.
    pub active_triplets: usize,
    /// Soft-deleted triplets (truth = unknown).
    pub deleted_triplets: usize,
    /// Unique entities (subjects + objects).
    pub unique_entities: usize,
    /// Spatial edges count.
    pub spatial_edges: usize,
    /// Contradictions detected (same S+O, different relation, both confident).
    pub contradictions: usize,
    /// Truth value distribution: how many triplets in each confidence band.
    pub truth_distribution: TruthDistribution,
    /// Episodic memory stats.
    pub episodic: EpisodicHealth,
    /// NARS inference stats.
    pub nars: NarsHealth,
}

/// Distribution of truth confidence across triplets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthDistribution {
    /// Confidence >= 0.9 (certain).
    pub certain: usize,
    /// Confidence 0.7-0.9 (strong).
    pub strong: usize,
    /// Confidence 0.4-0.7 (moderate).
    pub moderate: usize,
    /// Confidence 0.1-0.4 (weak).
    pub weak: usize,
    /// Confidence < 0.1 (unknown/deleted).
    pub unknown: usize,
}

/// Episodic memory health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicHealth {
    pub episodes_stored: usize,
    pub capacity: usize,
    pub saturation_pct: f32,
}

/// NARS inference engine health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarsHealth {
    /// Total deductions inferred this session.
    pub deductions_inferred: u64,
    /// Contradictions auto-detected this session.
    pub contradictions_detected: u64,
    /// Revisions applied this session.
    pub revisions_applied: u64,
}

// ── Pipeline Health (combines registry + graph health) ──────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsintPipelineHealth {
    pub stages: Vec<(String, OsintCounterSnapshot)>,
}

/// Full OSINT audit result — pipeline health + graph health + recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsintAuditResult {
    /// Pipeline stage call counts and latencies.
    pub pipeline: OsintPipelineHealth,
    /// Graph structure and truth distribution.
    pub graph: OsintGraphHealth,
    /// xAI API status.
    pub xai_status: XaiStatus,
    /// Prioritized recommendations.
    pub recommendations: Vec<String>,
    /// Audit timestamp (Unix millis).
    pub timestamp_ms: u64,
}

/// xAI API connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiStatus {
    /// Whether ADA_XAI env var is set.
    pub api_key_present: bool,
    /// Last API call latency (0 if never called).
    pub last_latency_us: u64,
    /// Total API calls this session.
    pub total_calls: u64,
    /// Total failures this session.
    pub total_failures: u64,
}

/// Run a full OSINT pipeline audit.
///
/// This is the function that the cockpit-server calls at `/api/debug/osint`.
pub fn run_osint_audit(
    graph_triplet_count: usize,
    graph_active_count: usize,
    graph_entity_count: usize,
    graph_spatial_edges: usize,
    graph_contradictions: usize,
    episodic_count: usize,
    episodic_capacity: usize,
) -> OsintAuditResult {
    let registry = osint_registry();
    let pipeline = registry.snapshot();

    let xai_snap = registry.xai_api_call.snapshot();
    let nars_deductions = registry.deduction.snapshot();
    let nars_contradictions = registry.contradiction.snapshot();
    let nars_revisions = registry.revision.snapshot();

    let deleted = graph_triplet_count.saturating_sub(graph_active_count);

    // Build truth distribution (placeholder — real impl would scan graph)
    let truth_distribution = TruthDistribution {
        certain: graph_active_count, // approximate
        strong: 0,
        moderate: 0,
        weak: 0,
        unknown: deleted,
    };

    let episodic_saturation = if episodic_capacity > 0 {
        (episodic_count as f32 / episodic_capacity as f32) * 100.0
    } else {
        0.0
    };

    let graph = OsintGraphHealth {
        total_triplets: graph_triplet_count,
        active_triplets: graph_active_count,
        deleted_triplets: deleted,
        unique_entities: graph_entity_count,
        spatial_edges: graph_spatial_edges,
        contradictions: graph_contradictions,
        truth_distribution,
        episodic: EpisodicHealth {
            episodes_stored: episodic_count,
            capacity: episodic_capacity,
            saturation_pct: episodic_saturation,
        },
        nars: NarsHealth {
            deductions_inferred: nars_deductions.triplets_produced,
            contradictions_detected: nars_contradictions.calls,
            revisions_applied: nars_revisions.calls,
        },
    };

    let xai_status = XaiStatus {
        api_key_present: std::env::var("ADA_XAI").is_ok(),
        last_latency_us: xai_snap.avg_latency_us,
        total_calls: xai_snap.calls,
        total_failures: xai_snap.failures,
    };

    // Generate recommendations
    let mut recommendations = Vec::new();
    if !xai_status.api_key_present {
        recommendations.push("Set ADA_XAI environment variable for xAI/Grok extraction".into());
    }
    if graph_contradictions > 0 {
        recommendations.push(format!(
            "{} contradictions detected — run NARS contradiction resolution",
            graph_contradictions
        ));
    }
    if episodic_saturation > 90.0 {
        recommendations.push("Episodic memory >90% full — consider increasing capacity or pruning".into());
    }
    if deleted as f32 / (graph_triplet_count.max(1) as f32) > 0.3 {
        recommendations.push("30%+ triplets soft-deleted — consider compaction".into());
    }
    if xai_snap.failures > xai_snap.successes && xai_snap.calls > 0 {
        recommendations.push("xAI API failure rate >50% — check API key and network".into());
    }
    if nars_deductions.calls == 0 && graph_active_count > 10 {
        recommendations.push("No NARS deductions run yet — call infer_deductions() to expand knowledge".into());
    }

    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    OsintAuditResult {
        pipeline,
        graph,
        xai_status,
        recommendations,
        timestamp_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_success() {
        let c = OsintCounter::new();
        c.record_success(1_000_000, 5);
        c.record_success(2_000_000, 3);
        let snap = c.snapshot();
        assert_eq!(snap.calls, 2);
        assert_eq!(snap.successes, 2);
        assert_eq!(snap.failures, 0);
        assert_eq!(snap.triplets_produced, 8);
    }

    #[test]
    fn test_counter_failure() {
        let c = OsintCounter::new();
        c.record_failure(500_000);
        let snap = c.snapshot();
        assert_eq!(snap.calls, 1);
        assert_eq!(snap.failures, 1);
        assert_eq!(snap.successes, 0);
    }

    #[test]
    fn test_registry_snapshot() {
        let r = OsintRegistry::new();
        r.extraction.record_success(100_000, 5);
        r.xai_api_call.record_success(50_000_000, 0);
        let health = r.snapshot();
        assert_eq!(health.stages.len(), 12);
        let extraction = health.stages.iter().find(|(n, _)| n == "extraction").unwrap();
        assert_eq!(extraction.1.calls, 1);
    }

    #[test]
    fn test_run_osint_audit() {
        let result = run_osint_audit(100, 80, 50, 10, 2, 15, 20);
        assert_eq!(result.graph.total_triplets, 100);
        assert_eq!(result.graph.active_triplets, 80);
        assert_eq!(result.graph.deleted_triplets, 20);
        assert_eq!(result.graph.contradictions, 2);
        assert!(!result.recommendations.is_empty());
        assert!(result.recommendations.iter().any(|r| r.contains("contradiction")));
    }

    #[test]
    fn test_xai_status_no_key() {
        // ADA_XAI is not set in test env (usually)
        let result = run_osint_audit(10, 10, 5, 0, 0, 0, 10);
        // We can't assert api_key_present because it depends on env
        assert_eq!(result.graph.total_triplets, 10);
    }

    #[test]
    fn test_episodic_saturation_warning() {
        let result = run_osint_audit(10, 10, 5, 0, 0, 19, 20);
        assert!(result.graph.episodic.saturation_pct > 90.0);
        assert!(result.recommendations.iter().any(|r| r.contains("90%")));
    }

    #[test]
    fn test_global_registry() {
        let r1 = osint_registry();
        let r2 = osint_registry();
        // Same singleton
        r1.extraction.record_success(100, 1);
        assert_eq!(r2.extraction.snapshot().calls, r1.extraction.snapshot().calls);
    }
}
