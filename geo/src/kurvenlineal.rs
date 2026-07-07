//! The Kurvenlineal — the real `helix::CurveRuler` golden-spiral residue
//! (stride-4-over-17), as a **continuous inter-family** field.
//!
//! ## Why this exists — the intra-family needle bug
//!
//! The prior terrain bake seeded a *fresh* `CurveRuler` per lattice cell
//! (`from_place(mix(floor(pos·detail)))`, see the old `ruler_phase` in
//! `bin/iceland_dem.rs`). Adjacent cells drew *uncorrelated* phases, so the
//! residue **stepped discontinuously at every cell boundary**. That was
//! harmless while it only tinted colour — but pushed into *geometry* it is the
//! "needle field": every cell its own isolated spike, never connected to its
//! neighbour. That is the **intra-family** failure.
//!
//! This module samples the exact stride-4-over-17 residue at the integer
//! lattice **corners** and smoothstep-interpolates between them (value noise),
//! so the field is C1-continuous *across* cell boundaries — **inter-family**.
//! Neighbouring cells share corner values, so the relief flows smoothly to the
//! next spike instead of jumping. It is **not** a Gaussian blur: the field
//! passes through the exact CurveRuler value at every lattice corner; only the
//! cell interior is blended. Same `pos` + `detail` → same value on every bake
//! (phase is convention, not data — OGAR D-QUANTGATE).

use helix::CurveRuler;

// Odd 64-bit mixing constants (fractional golden / √2 / √3), one per axis, so a
// lattice corner decorrelates into a place anchor.
const MIX_X: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX_Y: u64 = 0xC2B2_AE3D_27D4_EB4F;
const MIX_Z: u64 = 0x1656_67B1_9E37_79F9;

/// Decorrelate an integer lattice corner into a `u64` place anchor.
fn mix(c: [i64; 3]) -> u64 {
    (c[0] as u64)
        .wrapping_mul(MIX_X)
        .wrapping_add((c[1] as u64).wrapping_mul(MIX_Y))
        .wrapping_add((c[2] as u64).wrapping_mul(MIX_Z))
}

/// The exact stride-4-over-17 CurveRuler residue at one integer lattice corner,
/// mapped to bipolar `[-1, 1]` (index 8 → 0). Deterministic per corner.
fn corner_residue(corner: [i64; 3]) -> f32 {
    let place = mix(corner);
    // A stable per-corner arc index, taken from the high bits of the same hash
    // so it is decorrelated from `start = place % 17`.
    let k = ((place >> 40) % 17) as u32;
    let idx = CurveRuler::from_place(place).index(k);
    (f32::from(idx) / 16.0) * 2.0 - 1.0
}

/// Smoothstep (cubic Hermite): `smooth(0) = 0`, `smooth(1) = 1`, zero derivative
/// at both ends — so the interpolated field has no facet kinks at the lattice.
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Continuous inter-family kurvenlineal relief in `[-1, 1]` at `pos`, sampled at
/// spatial frequency `detail` (cells per unit). Trilinear value-noise over the 8
/// surrounding lattice corners, smoothstep-weighted per axis.
///
/// Feed the return value into a vertex's height (bipolar relief) or into a
/// per-vertex colour brightness — it is C1-continuous across cell boundaries
/// either way, so it connects rather than spikes.
#[must_use]
pub fn relief(pos: [f32; 3], detail: f32) -> f32 {
    let p = [pos[0] * detail, pos[1] * detail, pos[2] * detail];
    let base = [p[0].floor(), p[1].floor(), p[2].floor()];
    // `floor` rounds toward -inf, so the fraction is in [0, 1) even for negatives.
    let w = [
        smooth(p[0] - base[0]),
        smooth(p[1] - base[1]),
        smooth(p[2] - base[2]),
    ];
    let b = [base[0] as i64, base[1] as i64, base[2] as i64];

    let mut acc = 0.0f32;
    for (dz, wz) in [(0i64, 1.0 - w[2]), (1, w[2])] {
        for (dy, wy) in [(0i64, 1.0 - w[1]), (1, w[1])] {
            for (dx, wx) in [(0i64, 1.0 - w[0]), (1, w[0])] {
                let v = corner_residue([b[0] + dx, b[1] + dy, b[2] + dz]);
                acc += v * wx * wy * wz;
            }
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The OLD per-cell construction (verbatim shape of `iceland_dem::ruler_phase`)
    /// — reproduced here only to PROVE it is discontinuous where `relief` is not.
    fn old_per_cell(pos: [f32; 3], detail: f32) -> f32 {
        let cell = [
            (pos[0] * detail).floor() as i64,
            (pos[1] * detail).floor() as i64,
            (pos[2] * detail).floor() as i64,
        ];
        let sub = [
            (pos[0] * detail * 3.0).floor() as i64,
            (pos[1] * detail * 3.0).floor() as i64,
            (pos[2] * detail * 3.0).floor() as i64,
        ];
        let k = (mix(sub) % 17) as u32;
        let idx = CurveRuler::from_place(mix(cell)).index(k);
        (f32::from(idx) / 16.0) * 2.0 - 1.0
    }

    fn max_adjacent_delta(f: impl Fn([f32; 3], f32) -> f32) -> f32 {
        // Sweep a line crossing ~10 cell boundaries at fine resolution.
        let detail = 1.0;
        let mut prev = f([0.0, 0.37, 0.11], detail);
        let mut worst = 0.0f32;
        let mut x = 0.0f32;
        while x <= 10.0 {
            let cur = f([x, 0.37, 0.11], detail);
            worst = worst.max((cur - prev).abs());
            prev = cur;
            x += 0.002;
        }
        worst
    }

    #[test]
    fn relief_is_deterministic_and_bounded() {
        for i in 0..1000 {
            let p = [i as f32 * 0.013, i as f32 * -0.021, i as f32 * 0.007];
            let v = relief(p, 2.5);
            assert_eq!(v, relief(p, 2.5), "deterministic");
            assert!((-1.0..=1.0).contains(&v), "in range: {v}");
        }
    }

    #[test]
    fn relief_passes_through_the_lattice_values() {
        // At an exact integer lattice point the fractions are 0, so `relief`
        // reduces to that corner's exact CurveRuler residue (value-noise, not blur).
        for c in [[0i64, 0, 0], [3, -2, 5], [-7, 11, -4]] {
            let pos = [c[0] as f32, c[1] as f32, c[2] as f32];
            assert!((relief(pos, 1.0) - corner_residue(c)).abs() < 1e-6);
        }
    }

    #[test]
    fn inter_family_is_continuous_where_intra_family_jumps() {
        // THE fix, proven: the old per-cell residue jumps hard at cell seams
        // (the needle field); the new inter-family field flows smoothly.
        let old_jump = max_adjacent_delta(old_per_cell);
        let new_jump = max_adjacent_delta(relief);
        assert!(
            old_jump > 0.3,
            "old per-cell should step discontinuously, got {old_jump}"
        );
        assert!(
            new_jump < 0.05,
            "new inter-family field should be continuous, got {new_jump}"
        );
    }
}
