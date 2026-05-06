//! Wire-serializable bridge layer for the canonical R1 cognitive-shader DTO
//! family from `lance_graph_contract::cognitive_shader`.
//!
//! The canonical R1 surface is `ShaderDispatch` → `ShaderResonance` → `ShaderBus`
//! → `ShaderCrystal` (Φ Ψ B Γ). These zero-dep types do not derive `Serialize`
//! by design — they live on the cognitive hot path and serde would be dead
//! weight. q2's cockpit needs JSON for SSE, so this module provides `Wire*`
//! mirrors that DO derive `Serialize`, plus `From<&...>` impls.
//!
//! Critical bandwidth concerns:
//!   * `ShaderBus::cycle_fingerprint` is `[u64; 256]` (2 KB) — XOR-folded to a
//!     single u64 over the wire.
//!   * `ShaderResonance::top_k` is `[ShaderHit; 8]` — truncated to the
//!     non-empty prefix (`hit_count` entries).
//!   * `AlphaComposite::color_acc` is `[f32; 32]` (128 B) — collapsed to an
//!     active-dim count.
//!   * Per-row `emitted_edges` payload is dropped; the count alone rides over
//!     the wire.

use lance_graph_contract::cognitive_shader::{
    AlphaComposite, EmitMode, MetaSummary, ShaderBus, ShaderCrystal, ShaderDispatch, ShaderHit,
    ShaderResonance, StyleSelector,
};
use lance_graph_contract::collapse_gate::{GateDecision, MergeMode};
use serde::Serialize;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers — enum → wire-stable strings
// ═══════════════════════════════════════════════════════════════════════════

/// Stable string projection of `MergeMode` for wire/JSON.
///
/// These strings are the contract — never rename without bumping the SSE
/// schema version.
pub(crate) fn merge_mode_str(m: MergeMode) -> &'static str {
    match m {
        MergeMode::Xor => "Xor",
        MergeMode::Bundle => "Bundle",
        MergeMode::Superposition => "Superposition",
        MergeMode::AlphaFrontToBack => "AlphaFrontToBack",
    }
}

/// Stable string projection of `EmitMode` for wire/JSON.
pub(crate) fn emit_mode_str(e: EmitMode) -> &'static str {
    match e {
        EmitMode::Cycle => "Cycle",
        EmitMode::Bundle => "Bundle",
        EmitMode::Persist => "Persist",
    }
}

/// Stable string projection of `StyleSelector` for wire/JSON.
///
/// `Ordinal` and `Named` collapse to their string forms; `Auto` is a constant.
pub(crate) fn style_selector_str(s: StyleSelector) -> &'static str {
    match s {
        StyleSelector::Auto => "auto",
        // Both Ordinal and Named are effectively explicit selections; we
        // surface them as the same wire token because the driver resolves
        // them identically (the cockpit cares only auto-vs-explicit).
        StyleSelector::Ordinal(_) => "explicit",
        StyleSelector::Named(_) => "explicit",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ψ — WireShaderHit
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireShaderHit {
    pub row: u32,
    pub distance: u16,
    pub predicates: u8,
    pub resonance: f32,
    pub cycle_index: u32,
}

impl From<&ShaderHit> for WireShaderHit {
    fn from(h: &ShaderHit) -> Self {
        Self {
            row: h.row,
            distance: h.distance,
            predicates: h.predicates,
            resonance: h.resonance,
            cycle_index: h.cycle_index,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ψ — WireShaderResonance (truncated top_k, no fixed-array waste)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireShaderResonance {
    /// Truncated to the non-empty prefix (`hit_count`); only hits with
    /// `row != 0` OR `resonance > 0` survive — keeps default-state frames
    /// from leaking 8 zero-rows over the wire.
    pub top_k: Vec<WireShaderHit>,
    pub hit_count: u16,
    pub cycles_used: u16,
    pub entropy: f32,
    pub std_dev: f32,
    pub style_ord: u8,
}

impl From<&ShaderResonance> for WireShaderResonance {
    fn from(r: &ShaderResonance) -> Self {
        let take = (r.hit_count as usize).min(r.top_k.len());
        let top_k: Vec<WireShaderHit> = r.top_k[..take]
            .iter()
            .filter(|h| h.row != 0 || h.resonance > 0.0)
            .map(WireShaderHit::from)
            .collect();
        Self {
            top_k,
            hit_count: r.hit_count,
            cycles_used: r.cycles_used,
            entropy: r.entropy,
            std_dev: r.std_dev,
            style_ord: r.style_ord,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Layer-3 — WireGateDecision (gate ordinal + merge name)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireGateDecision {
    pub gate: u8,
    pub merge: &'static str,
}

impl From<GateDecision> for WireGateDecision {
    fn from(g: GateDecision) -> Self {
        Self {
            gate: g.gate,
            merge: merge_mode_str(g.merge),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// B — WireShaderBus (XOR-folded fingerprint, no [u64; 256] over the wire)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireShaderBus {
    /// XOR-fold of the 2 KB `cycle_fingerprint: [u64; 256]`. The cockpit
    /// uses this as a stable identity hash for the cycle; the full
    /// fingerprint never leaves the engine.
    pub cycle_fingerprint_hash: u64,
    pub emitted_edge_count: u8,
    pub gate: WireGateDecision,
    pub resonance: WireShaderResonance,
}

impl From<&ShaderBus> for WireShaderBus {
    fn from(b: &ShaderBus) -> Self {
        let cycle_fingerprint_hash = b.cycle_fingerprint.iter().fold(0u64, |acc, w| acc ^ w);
        Self {
            cycle_fingerprint_hash,
            emitted_edge_count: b.emitted_edge_count,
            gate: WireGateDecision::from(b.gate),
            resonance: WireShaderResonance::from(&b.resonance),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Pillar-7 — WireMetaSummary
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireMetaSummary {
    pub confidence: f32,
    pub meta_confidence: f32,
    pub brier: f32,
    pub should_admit_ignorance: bool,
}

impl From<&MetaSummary> for WireMetaSummary {
    fn from(m: &MetaSummary) -> Self {
        Self {
            confidence: m.confidence,
            meta_confidence: m.meta_confidence,
            brier: m.brier,
            should_admit_ignorance: m.should_admit_ignorance,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Pillar-7 — WireAlphaComposite (color_acc dimensionality only)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireAlphaComposite {
    pub alpha_acc: f32,
    pub hits_consumed: u16,
    pub saturated: bool,
    /// Number of non-zero dims in the dropped `color_acc: [f32; 32]`.
    /// The full vector (128 B) does not ride over the wire — the cockpit
    /// surfaces only the activation count as a "depth of merge" gauge.
    pub color_acc_active_dims: u8,
}

impl From<&AlphaComposite> for WireAlphaComposite {
    fn from(a: &AlphaComposite) -> Self {
        let color_acc_active_dims = a.color_acc.iter().filter(|v| **v != 0.0).count() as u8;
        Self {
            alpha_acc: a.alpha_acc,
            hits_consumed: a.hits_consumed,
            saturated: a.saturated,
            color_acc_active_dims,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Γ — WireShaderCrystal
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireShaderCrystal {
    pub bus: WireShaderBus,
    pub persisted_row: Option<u32>,
    pub meta: WireMetaSummary,
    pub alpha_composite: Option<WireAlphaComposite>,
}

impl From<&ShaderCrystal> for WireShaderCrystal {
    fn from(c: &ShaderCrystal) -> Self {
        Self {
            bus: WireShaderBus::from(&c.bus),
            persisted_row: c.persisted_row,
            meta: WireMetaSummary::from(&c.meta),
            alpha_composite: c.alpha_composite.as_ref().map(WireAlphaComposite::from),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Φ — WireShaderDispatch (drops meta_prefilter / rows / rung)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireShaderDispatch {
    pub layer_mask: u8,
    pub radius: u16,
    pub style: &'static str,
    pub max_cycles: u16,
    pub entropy_floor: f32,
    pub emit: &'static str,
    pub merge_override: Option<&'static str>,
}

impl From<&ShaderDispatch> for WireShaderDispatch {
    fn from(d: &ShaderDispatch) -> Self {
        Self {
            layer_mask: d.layer_mask,
            radius: d.radius,
            style: style_selector_str(d.style),
            max_cycles: d.max_cycles,
            entropy_floor: d.entropy_floor,
            emit: emit_mode_str(d.emit),
            merge_override: d.merge_override.map(merge_mode_str),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shader_crystal_size_under_2kb() {
        // The full ShaderBus carries a 2 KB cycle_fingerprint plus 64 B of
        // emitted_edges. If any of that leaks into JSON, this assertion fires.
        let crystal = ShaderCrystal {
            bus: ShaderBus::empty(),
            persisted_row: None,
            meta: MetaSummary::default(),
            alpha_composite: None,
        };
        let wire: WireShaderCrystal = (&crystal).into();
        let j = serde_json::to_string(&wire).expect("serialize wire crystal");
        assert!(
            j.len() < 2048,
            "wire crystal JSON {} bytes — fingerprint or color_acc leaked",
            j.len()
        );
        // Defensive: the serialized form must NOT contain a 256-element array.
        // A leaked [u64; 256] would show as ~256 commas in a single field.
        let comma_count = j.matches(',').count();
        assert!(
            comma_count < 64,
            "wire crystal has too many commas ({comma_count}) — array leak suspected"
        );
    }

    #[test]
    fn xor_fold_deterministic() {
        let mut bus = ShaderBus::empty();
        for (i, w) in bus.cycle_fingerprint.iter_mut().enumerate() {
            *w = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
        let a: WireShaderBus = (&bus).into();
        let b: WireShaderBus = (&bus).into();
        assert_eq!(a.cycle_fingerprint_hash, b.cycle_fingerprint_hash);
        // Sanity: the fold of all-zeros is zero, so a non-trivial fingerprint
        // must produce a non-zero hash.
        assert_ne!(a.cycle_fingerprint_hash, 0);
    }

    #[test]
    fn top_k_truncates_to_hit_count() {
        let mut res = ShaderResonance::default();
        // Populate 5 hits but only declare 3 valid.
        for i in 0..5 {
            res.top_k[i] = ShaderHit {
                row: 100 + i as u32,
                distance: 7,
                predicates: 0xFF,
                _pad: 0,
                resonance: 0.5,
                cycle_index: 1,
            };
        }
        res.hit_count = 3;
        let wire: WireShaderResonance = (&res).into();
        assert_eq!(
            wire.top_k.len(),
            3,
            "top_k must truncate to hit_count, got {}",
            wire.top_k.len()
        );
        assert_eq!(wire.top_k[0].row, 100);
        assert_eq!(wire.top_k[2].row, 102);
    }

    #[test]
    fn gate_decision_string_stable() {
        // All four MergeMode variants must produce stable, distinct names.
        assert_eq!(merge_mode_str(MergeMode::Xor), "Xor");
        assert_eq!(merge_mode_str(MergeMode::Bundle), "Bundle");
        assert_eq!(merge_mode_str(MergeMode::Superposition), "Superposition");
        assert_eq!(
            merge_mode_str(MergeMode::AlphaFrontToBack),
            "AlphaFrontToBack"
        );

        // GateDecision::FLOW_BUNDLE round-trips.
        let w: WireGateDecision = GateDecision::FLOW_BUNDLE.into();
        assert_eq!(w.gate, 0);
        assert_eq!(w.merge, "Bundle");

        let blocked: WireGateDecision = GateDecision::BLOCK.into();
        assert_eq!(blocked.gate, 1);
        assert_eq!(blocked.merge, "Xor");
    }

    #[test]
    fn default_round_trip() {
        // Every Wire* default form must serialize without panicking and
        // produce no NaN tokens.
        let dispatch = ShaderDispatch::default();
        let bus = ShaderBus::empty();
        let crystal = ShaderCrystal {
            bus: bus.clone(),
            persisted_row: None,
            meta: MetaSummary::default(),
            alpha_composite: Some(AlphaComposite::default()),
        };

        let j_dispatch =
            serde_json::to_string(&WireShaderDispatch::from(&dispatch)).expect("dispatch json");
        let j_bus = serde_json::to_string(&WireShaderBus::from(&bus)).expect("bus json");
        let j_crystal =
            serde_json::to_string(&WireShaderCrystal::from(&crystal)).expect("crystal json");

        for j in [&j_dispatch, &j_bus, &j_crystal] {
            assert!(!j.contains("NaN"), "wire frame contains NaN: {j}");
            assert!(!j.contains("null,null"), "double-null leak: {j}");
        }

        // Sanity: dispatch defaults surface `style: "auto"` and `emit: "Cycle"`.
        assert!(j_dispatch.contains("\"style\":\"auto\""));
        assert!(j_dispatch.contains("\"emit\":\"Cycle\""));
        // Default ShaderBus has gate=HOLD (gate ordinal 2, merge Xor).
        assert!(j_bus.contains("\"gate\":2"));
        assert!(j_bus.contains("\"merge\":\"Xor\""));
    }

    #[test]
    fn emit_and_style_helpers_cover_variants() {
        assert_eq!(emit_mode_str(EmitMode::Cycle), "Cycle");
        assert_eq!(emit_mode_str(EmitMode::Bundle), "Bundle");
        assert_eq!(emit_mode_str(EmitMode::Persist), "Persist");

        assert_eq!(style_selector_str(StyleSelector::Auto), "auto");
        assert_eq!(style_selector_str(StyleSelector::Ordinal(7)), "explicit");
        assert_eq!(style_selector_str(StyleSelector::Named("focus")), "explicit");
    }

    #[test]
    fn alpha_composite_drops_color_acc_array() {
        let mut ac = AlphaComposite::default();
        ac.color_acc[0] = 0.4;
        ac.color_acc[5] = 0.1;
        ac.color_acc[17] = 0.25;
        ac.alpha_acc = 0.75;
        ac.hits_consumed = 4;
        ac.saturated = false;

        let wire: WireAlphaComposite = (&ac).into();
        assert_eq!(wire.color_acc_active_dims, 3);

        let j = serde_json::to_string(&wire).unwrap();
        // The full color_acc array MUST NOT appear in the serialized form.
        assert!(!j.contains("color_acc\":["), "color_acc array leaked: {j}");
        assert!(j.contains("\"color_acc_active_dims\":3"));
        // Footprint sanity: small struct must serialize tiny.
        assert!(j.len() < 256, "alpha composite frame too large: {} B", j.len());
    }
}
