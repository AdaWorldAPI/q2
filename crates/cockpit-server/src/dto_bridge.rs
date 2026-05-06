//! Wire-serializable bridge layer for `thinking-engine::dto` types.
//!
//! `thinking-engine` does not derive `Serialize` on its DTOs by design — it's a
//! hot-path internal crate where serde is dead weight. q2 needs JSON for SSE,
//! so this module wraps the canonical DTOs in `Wire*` mirrors that DO derive
//! `Serialize`, plus `From<&...>` impls for cheap conversion at the SSE edge.
//!
//! The full `f32[4096]` `ResonanceDto::energy` field is NOT serialized — it
//! would dwarf every SSE frame. We project to `top_k + entropy + active_count`
//! (sparse summary) which is what the cockpit UI actually consumes.

use serde::Serialize;
use thinking_engine::dto::{
    BusDto, ResonanceDto, SourceType, StreamDto, ThinkingScale, ThoughtStruct,
};

// ═══════════════════════════════════════════════════════════════════════════
// Helpers — enum → wire-stable strings
// ═══════════════════════════════════════════════════════════════════════════

/// Stable string projection of `SourceType` for wire/JSON.
///
/// These strings are the contract — never rename without bumping the SSE
/// schema version. The cockpit UI keys off these exact values.
pub fn source_type_str(s: SourceType) -> &'static str {
    match s {
        SourceType::Jina => "jina",
        SourceType::BgeM3 => "bge_m3",
        SourceType::ReaderLm => "reader_lm",
        SourceType::Qwen => "qwen",
        SourceType::DeepNsm => "deep_nsm",
        SourceType::Wikidata => "wikidata",
        SourceType::AriGraph => "ari_graph",
        SourceType::ImageGen => "image_gen",
        SourceType::User => "user",
    }
}

/// Stable string projection of `ThinkingScale` for wire/JSON.
pub fn scale_str(s: ThinkingScale) -> &'static str {
    match s {
        ThinkingScale::Exploiting => "exploiting",
        ThinkingScale::Focused => "focused",
        ThinkingScale::Exploring => "exploring",
        ThinkingScale::Abstract => "abstract",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Φ — WireStreamDto
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireStreamDto {
    pub source: &'static str,
    pub codebook_indices: Vec<u16>,
    pub timestamp: u64,
}

impl From<&StreamDto> for WireStreamDto {
    fn from(s: &StreamDto) -> Self {
        Self {
            source: source_type_str(s.source),
            codebook_indices: s.codebook_indices.clone(),
            timestamp: s.timestamp,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ψ — WireResonanceDto (sparse projection — no full f32[4096])
// ═══════════════════════════════════════════════════════════════════════════

/// One `(codebook_index, energy)` pair from `top_k`. Tuples don't derive
/// `Serialize` as named JSON objects, so we expand to a struct here.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WireTopKEntry {
    pub index: u16,
    pub energy: f32,
}

/// SPARSE wire projection of `ResonanceDto`.
///
/// The canonical `ResonanceDto` carries `Vec<f32>` of length 4096 (~16 KB).
/// Sending that on every SSE frame would saturate the channel for no UI
/// benefit. We project to:
///   - `top_k` (8 dominant peaks) — what the cockpit ribbon renders
///   - `entropy` — single-scalar resonance dispersion
///   - `active_count` — how many entries crossed the activity threshold
#[derive(Clone, Debug, Serialize)]
pub struct WireResonanceDto {
    pub cycle_count: u16,
    pub converged: bool,
    pub top_k: [WireTopKEntry; 8],
    pub entropy: f32,
    pub active_count: usize,
}

impl WireResonanceDto {
    /// Threshold below which an energy entry is considered inactive.
    /// Matches the conventional 1e-6 floor used elsewhere in q2.
    pub const ACTIVE_THRESHOLD: f32 = 1e-6;
}

impl From<&ResonanceDto> for WireResonanceDto {
    fn from(r: &ResonanceDto) -> Self {
        let mut top_k = [WireTopKEntry { index: 0, energy: 0.0 }; 8];
        for (i, &(idx, e)) in r.top_k.iter().enumerate() {
            top_k[i] = WireTopKEntry { index: idx, energy: e };
        }
        Self {
            cycle_count: r.cycle_count,
            converged: r.converged,
            top_k,
            entropy: r.entropy(),
            active_count: r.active_count(Self::ACTIVE_THRESHOLD),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// B — WireBusDto
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize)]
pub struct WireBusDto {
    pub codebook_index: u16,
    pub energy: f32,
    pub top_k: [WireTopKEntry; 8],
    pub cycle_count: u16,
    pub converged: bool,
}

impl From<&BusDto> for WireBusDto {
    fn from(b: &BusDto) -> Self {
        let mut top_k = [WireTopKEntry { index: 0, energy: 0.0 }; 8];
        for (i, &(idx, e)) in b.top_k.iter().enumerate() {
            top_k[i] = WireTopKEntry { index: idx, energy: e };
        }
        Self {
            codebook_index: b.codebook_index,
            energy: b.energy,
            top_k,
            cycle_count: b.cycle_count,
            converged: b.converged,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Γ — WireThoughtStruct
// ═══════════════════════════════════════════════════════════════════════════

/// `(SourceType, indices)` pair as wire-friendly named struct.
#[derive(Clone, Debug, Serialize)]
pub struct WireSensorContribution {
    pub source: &'static str,
    pub codebook_indices: Vec<u16>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WireThoughtStruct {
    pub bus: WireBusDto,
    pub text: Option<String>,
    /// Last entry of `style_trajectory`, retained for ThoughtLog frontend
    /// back-compat. Defaults to `"idle"` if the trajectory is empty.
    pub style: &'static str,
    pub sensor_contributions: Vec<WireSensorContribution>,
    /// Length of the engine's `tension_history: Vec<Vec<f32>>`. The full
    /// per-cycle energy snapshots (~4096 f32 each) are NEVER serialized over
    /// SSE — the cockpit only needs the depth as a "thinking effort" gauge.
    pub tension_history_len: u32,
    pub style_trajectory: Vec<&'static str>,
}

impl From<&ThoughtStruct> for WireThoughtStruct {
    fn from(t: &ThoughtStruct) -> Self {
        let trajectory: Vec<&'static str> =
            t.style_trajectory.iter().copied().map(scale_str).collect();
        let style = trajectory.last().copied().unwrap_or("idle");
        Self {
            bus: WireBusDto::from(&t.bus),
            text: t.text.clone(),
            style,
            sensor_contributions: t
                .sensor_contributions
                .iter()
                .map(|(src, idx)| WireSensorContribution {
                    source: source_type_str(*src),
                    codebook_indices: idx.clone(),
                })
                .collect(),
            tension_history_len: t.tension_history.len() as u32,
            style_trajectory: trajectory,
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
    fn source_type_strings_stable() {
        assert_eq!(source_type_str(SourceType::Jina), "jina");
        assert_eq!(source_type_str(SourceType::BgeM3), "bge_m3");
        assert_eq!(source_type_str(SourceType::ReaderLm), "reader_lm");
        assert_eq!(source_type_str(SourceType::DeepNsm), "deep_nsm");
        assert_eq!(source_type_str(SourceType::AriGraph), "ari_graph");
        assert_eq!(source_type_str(SourceType::User), "user");
    }

    #[test]
    fn scale_strings_stable() {
        assert_eq!(scale_str(ThinkingScale::Exploiting), "exploiting");
        assert_eq!(scale_str(ThinkingScale::Focused), "focused");
        assert_eq!(scale_str(ThinkingScale::Exploring), "exploring");
        assert_eq!(scale_str(ThinkingScale::Abstract), "abstract");
    }

    #[test]
    fn stream_round_trips_to_json() {
        let s = StreamDto {
            source: SourceType::Jina,
            codebook_indices: vec![1, 2, 3],
            timestamp: 999,
        };
        let w: WireStreamDto = (&s).into();
        let j = serde_json::to_string(&w).unwrap();
        assert!(j.contains("\"source\":\"jina\""));
        assert!(j.contains("\"timestamp\":999"));
    }

    #[test]
    fn resonance_drops_full_energy_field() {
        // 4096-entry energy must NOT appear in JSON — only the sparse summary.
        let mut energy = vec![0.0f32; 4096];
        energy[42] = 0.7;
        energy[100] = 0.2;
        let r = ResonanceDto::from_energy_f32(&energy, 3);
        let w: WireResonanceDto = (&r).into();
        let j = serde_json::to_string(&w).unwrap();
        // Sanity: top peak must round-trip.
        assert_eq!(w.top_k[0].index, 42);
        assert!((w.top_k[0].energy - 0.7).abs() < 1e-6);
        // No raw "energy" array key in serialized output.
        assert!(!j.contains("\"energy\":[")); // array form forbidden
        // active_count + entropy MUST be present.
        assert!(j.contains("\"active_count\""));
        assert!(j.contains("\"entropy\""));
        // Frame size sanity: should be tiny, not ~16 KB.
        assert!(j.len() < 1024, "wire frame too large: {} bytes", j.len());
    }

    #[test]
    fn bus_round_trips() {
        let b = BusDto {
            codebook_index: 7,
            energy: 0.42,
            top_k: [(7, 0.42); 8],
            cycle_count: 4,
            converged: true,
        };
        let w: WireBusDto = (&b).into();
        let j = serde_json::to_string(&w).unwrap();
        assert!(j.contains("\"codebook_index\":7"));
        assert!(j.contains("\"converged\":true"));
    }

    #[test]
    fn thought_struct_round_trips() {
        let bus = BusDto {
            codebook_index: 11,
            energy: 0.3,
            top_k: [(11, 0.3); 8],
            cycle_count: 2,
            converged: true,
        };
        let mut t = ThoughtStruct::from_engine(
            bus,
            vec![(SourceType::Jina, vec![11, 22])],
        )
        .with_text("hello".into());
        t.style_trajectory.push(ThinkingScale::Focused);
        t.style_trajectory.push(ThinkingScale::Exploring);

        let w: WireThoughtStruct = (&t).into();
        let j = serde_json::to_string(&w).unwrap();
        assert!(j.contains("\"text\":\"hello\""));
        assert!(j.contains("\"source\":\"jina\""));
        assert!(j.contains("\"focused\""));
        assert!(j.contains("\"exploring\""));
    }
}
