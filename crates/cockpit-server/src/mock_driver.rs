//! Phase 2B synthetic [`CognitiveShaderDriver`] implementation.
//!
//! This is the cockpit-server-side mock that lets us treat
//! `lance_graph_contract::cognitive_shader::CognitiveShaderDriver` as the
//! canonical seam — cockpit-server consumes the trait, not the
//! thinking-engine concrete type. The real driver (BindSpace SoA + bgz17
//! distance + JIT styles) lands in Phase 3 when models load.
//!
//! Every number this driver emits is **synthetic** — derived from the
//! `last_perturbation` indices, not from a real bgz17 distance sweep over
//! BindSpace. Specifically:
//!
//! - `ShaderHit::row` = perturbation index modulo `row_count`
//! - `ShaderHit::distance` = `i * 64` (monotone in rank)
//! - `ShaderHit::resonance` = `1.0 - i * 0.1` (clamped to [0, 1])
//! - `ShaderHit::cycle_index` = monotone counter shared across dispatches
//! - `ShaderResonance::entropy` = `1.0 + 0.05 * indices.len()`
//! - `ShaderBus::cycle_fingerprint` = XOR-fold of perturbation indices
//! - `MetaSummary` is a hand-tuned constant
//!
//! These values are **only** for round-trip wiring in cockpit-server
//! (scene player → driver → DTO bridge → SSE). Phase 3 replaces this with
//! `BgzShaderDriver` over a real BindSpace.
//!
//! # Borrow strategy
//!
//! Per `borrow-strategy.md` (microcopies pattern), `dispatch_with_sink`
//! takes `&self`. The `Mutex<Vec<u16>>` over `last_perturbation` is held
//! only long enough to clone the indices into an owned local Vec, then
//! dropped before any further work — keeping the read window tight and
//! avoiding any &mut on driver state during synthesis.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use lance_graph_contract::cognitive_shader::{
    CognitiveShaderDriver, MetaSummary, ShaderBus, ShaderCrystal, ShaderDispatch, ShaderHit,
    ShaderResonance, ShaderSink,
};
use lance_graph_contract::collapse_gate::GateDecision;

/// Maximum number of perturbation indices we retain per [`perturb`] call.
/// Bounds work in `dispatch_with_sink` and matches the cap used by
/// `extract_cypher_identifiers` upstream.
const MAX_PERTURBATION_INDICES: usize = 32;

/// Number of synthetic top-K hits emitted per dispatch.
const TOP_K: usize = 8;

/// Synthetic [`CognitiveShaderDriver`] used by cockpit-server through Phase 2B.
///
/// Holds three pieces of state:
///
/// * `row_count` — the synthetic BindSpace size; used to clamp every
///   `ShaderHit::row` so the DTO bridge never produces an out-of-range row.
/// * `last_perturbation` — the most recent codebook indices fed in via
///   [`MockShaderDriver::perturb`], cloned out under a short-lived lock
///   inside `dispatch_with_sink`.
/// * `cycle_counter` — a monotone counter stamped on every emitted
///   [`ShaderHit::cycle_index`].
pub struct MockShaderDriver {
    /// Total rows in the synthetic BindSpace. Used to clamp `ShaderHit::row`.
    pub row_count: u32,
    /// Latest perturbation indices from a Cypher act. Updated by
    /// [`MockShaderDriver::perturb`]. Wrapped in a `Mutex` so `dispatch`
    /// (which is `&self`) can clone-and-read without holding the lock
    /// across synthesis.
    pub last_perturbation: Mutex<Vec<u16>>,
    /// Cycle counter used for `ShaderHit::cycle_index`. Atomic so
    /// concurrent dispatches still produce monotone values.
    pub cycle_counter: AtomicU32,
}

impl MockShaderDriver {
    /// Build a driver pretending to back `row_count` BindSpace rows.
    ///
    /// The driver starts with no perturbation (empty top_k) and a cycle
    /// counter at 0.
    pub fn new(row_count: u32) -> Self {
        Self {
            row_count,
            last_perturbation: Mutex::new(Vec::new()),
            cycle_counter: AtomicU32::new(0),
        }
    }

    /// Replace the latest perturbation indices.
    ///
    /// The slice is cloned and truncated to [`MAX_PERTURBATION_INDICES`]
    /// to bound the synthesis work in [`dispatch_with_sink`]. Any prior
    /// indices are dropped.
    pub fn perturb(&self, codebook_indices: &[u16]) {
        let take = codebook_indices.len().min(MAX_PERTURBATION_INDICES);
        let mut snap = Vec::with_capacity(take);
        snap.extend_from_slice(&codebook_indices[..take]);
        // Borrow strategy: short-lived &mut on the Mutex's interior, no
        // other state borrowed across the lock.
        let mut guard = self
            .last_perturbation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = snap;
    }

    /// Read the current perturbation indices into an owned Vec.
    ///
    /// Holds the mutex only long enough to clone — caller works on the
    /// owned microcopy thereafter.
    fn snapshot_perturbation(&self) -> Vec<u16> {
        let guard = self
            .last_perturbation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    }
}

impl CognitiveShaderDriver for MockShaderDriver {
    fn row_count(&self) -> u32 {
        self.row_count
    }

    /// Synthetic byte footprint estimate.
    ///
    /// `row_count * 256` documents that the real Phase-3 driver tracks
    /// per-row BindSpace columns (256 u64 words for the content plane is
    /// the dominant column). The actual cost is zero — this driver
    /// allocates only the perturbation Vec.
    fn byte_footprint(&self) -> usize {
        (self.row_count as usize).saturating_mul(256)
    }

    fn dispatch(&self, req: &ShaderDispatch) -> ShaderCrystal {
        let mut sink = lance_graph_contract::cognitive_shader::NullSink;
        self.dispatch_with_sink(req, &mut sink)
    }

    fn dispatch_with_sink<S: ShaderSink>(
        &self,
        _req: &ShaderDispatch,
        sink: &mut S,
    ) -> ShaderCrystal {
        // Step 1: take an owned microcopy of the perturbation. Lock is
        // dropped before any synthesis runs.
        let indices = self.snapshot_perturbation();

        // Step 2: build the synthetic top_k from the first TOP_K indices.
        let row_clamp = self.row_count.max(1);
        let mut top_k = [ShaderHit::default(); TOP_K];
        let used = indices.len().min(TOP_K);
        for (i, idx) in indices.iter().take(TOP_K).enumerate() {
            let resonance = (1.0 - (i as f32) * 0.1).clamp(0.0, 1.0);
            top_k[i] = ShaderHit {
                row: (*idx as u32) % row_clamp,
                distance: (i as u16).saturating_mul(64),
                predicates: 0xFF,
                _pad: 0,
                resonance,
                cycle_index: self.cycle_counter.fetch_add(1, Ordering::Relaxed),
            };
        }

        // Step 3: assemble the resonance summary.
        let resonance = ShaderResonance {
            top_k,
            hit_count: used as u16,
            cycles_used: 5,
            entropy: 1.0 + (indices.len() as f32) * 0.05,
            std_dev: 0.1,
            style_ord: 0,
        };

        // Step 4: sink callback — short-circuit if the consumer rejects.
        if !sink.on_resonance(&resonance) {
            // Default crystal carries the resonance we computed, but an
            // empty bus (no fingerprint, no edges, HOLD gate) — nothing
            // committed downstream.
            let crystal = ShaderCrystal {
                bus: ShaderBus::empty(),
                persisted_row: None,
                meta: default_meta_summary(),
                alpha_composite: None,
            };
            sink.on_crystal(&crystal);
            return crystal;
        }

        // Step 5: XOR-fold the perturbation indices into a synthetic
        // 256-u64 cycle_fingerprint. Mostly zeros — only the slots
        // touched by the perturbation carry signal.
        let mut cycle_fingerprint = [0u64; 256];
        for (i, idx) in indices.iter().enumerate() {
            cycle_fingerprint[i % 256] ^= *idx as u64;
        }

        // Step 6: build the bus. FLOW_BUNDLE is the canonical multi-writer
        // gate for synthetic cycles — see borrow-strategy.md "Multiple
        // writers → use BUNDLE".
        let bus = ShaderBus {
            cycle_fingerprint,
            emitted_edges: [0u64; 8],
            emitted_edge_count: 0,
            gate: GateDecision::FLOW_BUNDLE,
            resonance,
        };

        // Step 7: bus callback — short-circuit returns the bus we built
        // but with no crystallisation downstream.
        if !sink.on_bus(&bus) {
            let crystal = ShaderCrystal {
                bus,
                persisted_row: None,
                meta: default_meta_summary(),
                alpha_composite: None,
            };
            sink.on_crystal(&crystal);
            return crystal;
        }

        // Step 8: crystallise.
        let crystal = ShaderCrystal {
            bus,
            persisted_row: None,
            meta: default_meta_summary(),
            alpha_composite: None,
        };
        sink.on_crystal(&crystal);
        crystal
    }
}

/// Hand-tuned `MetaSummary` for synthetic crystals.
///
/// Phase-3 will replace this with the real meta-cognitive assessment
/// (Brier score, NARS-revised confidence, ignorance admission). Until
/// then, mid-confidence + low Brier reads as "cycle ran cleanly but no
/// strong claim".
fn default_meta_summary() -> MetaSummary {
    MetaSummary {
        confidence: 0.7,
        meta_confidence: 0.5,
        brier: 0.2,
        should_admit_ignorance: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::cognitive_shader::NullSink;

    /// Sink that always rejects at `on_resonance`. Used to exercise the
    /// short-circuit path in `dispatch_with_sink`.
    struct RejectingResonanceSink {
        saw_crystal: bool,
    }

    impl ShaderSink for RejectingResonanceSink {
        fn on_resonance(&mut self, _r: &ShaderResonance) -> bool {
            false
        }
        fn on_bus(&mut self, _b: &ShaderBus) -> bool {
            // Should never be called when on_resonance returned false.
            panic!("on_bus must not be called after on_resonance returned false");
        }
        fn on_crystal(&mut self, _c: &ShaderCrystal) {
            self.saw_crystal = true;
        }
    }

    #[test]
    fn default_dispatch_returns_crystal() {
        let driver = MockShaderDriver::new(100);
        let crystal = driver.dispatch(&ShaderDispatch::default());

        // Bus is populated (default fingerprint is zero — no perturbation
        // — but the gate is set and the resonance carries a valid frame).
        assert_eq!(crystal.bus.cycle_fingerprint.len(), 256);
        assert_eq!(crystal.bus.emitted_edge_count, 0);
        // No perturbation → hit_count = 0 but resonance fields populated.
        assert_eq!(crystal.bus.resonance.hit_count, 0);
        assert_eq!(crystal.bus.resonance.cycles_used, 5);
        assert!(crystal.bus.resonance.entropy >= 1.0);
        // Meta is the synthetic constant.
        assert!((crystal.meta.confidence - 0.7).abs() < f32::EPSILON);
        assert!((crystal.meta.brier - 0.2).abs() < f32::EPSILON);
        assert!(!crystal.meta.should_admit_ignorance);
        assert!(crystal.persisted_row.is_none());
        assert!(crystal.alpha_composite.is_none());

        // CognitiveShaderDriver surface methods.
        assert_eq!(driver.row_count(), 100);
        assert_eq!(driver.byte_footprint(), 100 * 256);
    }

    #[test]
    fn perturb_updates_top_k() {
        let driver = MockShaderDriver::new(1000);
        let indices: [u16; 3] = [42, 17, 99];
        driver.perturb(&indices);

        let crystal = driver.dispatch(&ShaderDispatch::default());
        let top_k = &crystal.bus.resonance.top_k;

        // Indices ≤ row_count → row equals idx % row_count = idx.
        assert_eq!(top_k[0].row, 42);
        assert_eq!(top_k[1].row, 17);
        assert_eq!(top_k[2].row, 99);
        // Slots beyond the supplied indices are zero.
        for slot in top_k.iter().skip(3) {
            assert_eq!(slot.row, 0);
            assert_eq!(slot.distance, 0);
            assert_eq!(slot.resonance, 0.0);
        }
        // hit_count tracks supplied indices, capped at TOP_K.
        assert_eq!(crystal.bus.resonance.hit_count, 3);
        // resonance is monotone non-increasing across the populated prefix.
        assert!(top_k[0].resonance >= top_k[1].resonance);
        assert!(top_k[1].resonance >= top_k[2].resonance);
        // distance is monotone non-decreasing.
        assert!(top_k[0].distance <= top_k[1].distance);
        assert!(top_k[1].distance <= top_k[2].distance);
    }

    #[test]
    fn cycle_index_monotonic() {
        let driver = MockShaderDriver::new(64);
        driver.perturb(&[1, 2, 3, 4]);

        let first = driver.dispatch(&ShaderDispatch::default());
        let second = driver.dispatch(&ShaderDispatch::default());

        // Each dispatch consumes 4 cycle_index slots (one per emitted hit).
        // The lowest cycle_index of the second dispatch must strictly
        // exceed the highest cycle_index of the first.
        let first_max = first
            .bus
            .resonance
            .top_k
            .iter()
            .take(first.bus.resonance.hit_count as usize)
            .map(|h| h.cycle_index)
            .max()
            .expect("first dispatch produced hits");
        let second_min = second
            .bus
            .resonance
            .top_k
            .iter()
            .take(second.bus.resonance.hit_count as usize)
            .map(|h| h.cycle_index)
            .min()
            .expect("second dispatch produced hits");

        assert!(
            second_min > first_max,
            "second dispatch cycle_index {} must exceed first dispatch cycle_index {}",
            second_min,
            first_max
        );
    }

    #[test]
    fn sink_short_circuit_on_resonance() {
        let driver = MockShaderDriver::new(50);
        driver.perturb(&[7, 11, 13]);

        let mut sink = RejectingResonanceSink { saw_crystal: false };
        let crystal = driver.dispatch_with_sink(&ShaderDispatch::default(), &mut sink);

        // Sink got the crystal callback even though it rejected.
        assert!(sink.saw_crystal, "on_crystal must fire on short-circuit");
        // Short-circuit crystal carries an empty bus (no fingerprint
        // bits, HOLD gate from ShaderBus::empty()) — the cycle did not
        // commit.
        assert_eq!(crystal.bus.cycle_fingerprint, [0u64; 256]);
        assert!(crystal.bus.gate.is_hold());
        assert!(crystal.persisted_row.is_none());
        // But the meta summary is still the synthetic default.
        assert!((crystal.meta.confidence - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn row_count_clamps_hits() {
        let row_count: u32 = 10;
        let driver = MockShaderDriver::new(row_count);
        // Perturb with indices well above row_count.
        driver.perturb(&[12, 25, 4095, 137]);

        let mut sink = NullSink;
        let crystal = driver.dispatch_with_sink(&ShaderDispatch::default(), &mut sink);

        let hit_count = crystal.bus.resonance.hit_count as usize;
        assert!(hit_count > 0, "perturbation should produce hits");
        for hit in crystal.bus.resonance.top_k.iter().take(hit_count) {
            assert!(
                hit.row < row_count,
                "hit.row {} must be < row_count {}",
                hit.row,
                row_count
            );
        }
        // Spot-check the modular reduction: 12 % 10 = 2.
        assert_eq!(crystal.bus.resonance.top_k[0].row, 12 % row_count);
        assert_eq!(crystal.bus.resonance.top_k[1].row, 25 % row_count);
        assert_eq!(crystal.bus.resonance.top_k[2].row, 4095 % row_count);
        assert_eq!(crystal.bus.resonance.top_k[3].row, 137 % row_count);
    }
}
