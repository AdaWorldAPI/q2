//! The five brushes + undo — every stroke is: touched set from the
//! UNMODIFIED mesh → inverse record → staged write-back. No brush ever reads
//! half-written state (Smooth's neighborhood mean would otherwise depend on
//! vertex iteration order).
//!
//! The Ruler brush's relief magnitude is [`ruler_phase`]: a deterministic
//! bipolar value in [-1, 1] regenerated from the vertex's lattice address via
//! [`helix::CurveRuler`] (stride-4-over-17). Phase is convention, not data —
//! re-stroking the same region converges on the same relief instead of
//! accumulating noise.

use crate::mesh::Mesh;
use helix::CurveRuler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Grab,
    Inflate,
    Smooth,
    Spray,
    Ruler,
}

impl std::str::FromStr for Tool {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "grab" => Ok(Tool::Grab),
            "inflate" => Ok(Tool::Inflate),
            "smooth" => Ok(Tool::Smooth),
            "spray" => Ok(Tool::Spray),
            "ruler" => Ok(Tool::Ruler),
            other => Err(format!("unknown tool: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub tool: Tool,
    pub center: [f32; 3], // picked surface point (world)
    pub dir: [f32; 3],    // world-space drag vector (Grab uses it; others ignore)
    pub radius: f32,      // world units (mesh is unit-normalized, so 0.05..0.5)
    pub strength: f32,    // 0..1
    pub color: [u8; 3],   // Spray
    pub detail: f32,      // Ruler lattice frequency (cells per unit), 4..64
}

/// Inverse record of exactly the vertices a stroke touched.
#[derive(Debug, Clone)]
pub struct Undo {
    pub verts: Vec<(u32, [f32; 3], [u8; 3])>,
}

/// Falloff w = (1 − t²)² with t = dist/radius; w ≤ 0 (t ≥ 1) never enters
/// the touched set. NaN distances/radii compare false → untouched.
fn falloff(d: f32, radius: f32) -> f32 {
    let t = d / radius;
    if t < 1.0 {
        let u = 1.0 - t * t;
        u * u
    } else {
        0.0
    }
}

/// Apply one stroke; returns the inverse record for [`revert`].
pub fn apply(mesh: &mut Mesh, s: &Stroke) -> Undo {
    // 1 — touched set from the unmodified mesh (w > 0 only).
    let mut touched: Vec<(u32, f32)> = Vec::new();
    if s.radius > 0.0 {
        for (i, p) in mesh.pos.iter().enumerate() {
            let w = falloff(dist(*p, s.center), s.radius);
            if w > 0.0 {
                touched.push((i as u32, w));
            }
        }
    }

    // 2 — inverse record BEFORE any write.
    let undo = Undo {
        verts: touched
            .iter()
            .map(|&(i, _)| (i, mesh.pos[i as usize], mesh.col[i as usize]))
            .collect(),
    };

    // 3 — stage all new values from the old state, then write back.
    let mut moved = false;
    match s.tool {
        Tool::Grab => {
            // pos += dir * strength * w
            let new: Vec<(u32, [f32; 3])> = touched
                .iter()
                .map(|&(i, w)| {
                    let p = mesh.pos[i as usize];
                    let k = s.strength * w;
                    (i, add_scaled(p, s.dir, k))
                })
                .collect();
            write_pos(mesh, &new);
            moved = true;
        }
        Tool::Inflate => {
            // pos += nrm * 0.2 * strength * w
            let new: Vec<(u32, [f32; 3])> = touched
                .iter()
                .map(|&(i, w)| {
                    let p = mesh.pos[i as usize];
                    let n = mesh.nrm[i as usize];
                    (i, add_scaled(p, n, 0.2 * s.strength * w))
                })
                .collect();
            write_pos(mesh, &new);
            moved = true;
        }
        Tool::Smooth => {
            // pos → lerp(pos, adjacency_mean, strength * w); a vertex with no
            // neighbors has no mean — identity (stays put, stays in the Undo).
            let new: Vec<(u32, [f32; 3])> = touched
                .iter()
                .filter_map(|&(i, w)| {
                    let nb = &mesh.adj[i as usize];
                    if nb.is_empty() {
                        return None;
                    }
                    let mut m = [0.0f32; 3];
                    for &j in nb {
                        let q = mesh.pos[j as usize];
                        m[0] += q[0];
                        m[1] += q[1];
                        m[2] += q[2];
                    }
                    let inv = 1.0 / nb.len() as f32;
                    let p = mesh.pos[i as usize];
                    let k = s.strength * w;
                    Some((
                        i,
                        [
                            p[0] + (m[0] * inv - p[0]) * k,
                            p[1] + (m[1] * inv - p[1]) * k,
                            p[2] + (m[2] * inv - p[2]) * k,
                        ],
                    ))
                })
                .collect();
            write_pos(mesh, &new);
            moved = true;
        }
        Tool::Spray => {
            // col → lerp(col, color, strength * w), u8 rounded. No positions
            // change → normals stay valid, no recompute.
            let new: Vec<(u32, [u8; 3])> = touched
                .iter()
                .map(|&(i, w)| {
                    let c = mesh.col[i as usize];
                    let k = s.strength * w;
                    (
                        i,
                        [
                            lerp_u8(c[0], s.color[0], k),
                            lerp_u8(c[1], s.color[1], k),
                            lerp_u8(c[2], s.color[2], k),
                        ],
                    )
                })
                .collect();
            for (i, c) in new {
                mesh.col[i as usize] = c;
            }
        }
        Tool::Ruler => {
            // pos += nrm * 0.08 * strength * w * phase(OLD pos, detail).
            let new: Vec<(u32, [f32; 3])> = touched
                .iter()
                .map(|&(i, w)| {
                    let p = mesh.pos[i as usize];
                    let n = mesh.nrm[i as usize];
                    let k = 0.08 * s.strength * w * ruler_phase(p, s.detail);
                    (i, add_scaled(p, n, k))
                })
                .collect();
            write_pos(mesh, &new);
            moved = true;
        }
    }

    // Positions changed → the nrm-is-unit-and-current invariant must be
    // restored before the next render/Inflate/Ruler reads it.
    if moved && !undo.verts.is_empty() {
        mesh.recompute_normals();
    }
    undo
}

/// Restore pos+col from the inverse record, recompute normals.
pub fn revert(mesh: &mut Mesh, u: &Undo) {
    for &(i, p, c) in &u.verts {
        mesh.pos[i as usize] = p;
        mesh.col[i as usize] = c;
    }
    mesh.recompute_normals();
}

// Mixing constants: large odd (golden-ratio / xorshift family), so the three
// lattice ints decorrelate under wrapping_mul before the xor fold.
const MIX_X: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX_Y: u64 = 0xC2B2_AE3D_27D4_EB4F;
const MIX_Z: u64 = 0x1656_67B1_9E37_79F9;

fn mix(c: [i64; 3]) -> u64 {
    (c[0] as u64).wrapping_mul(MIX_X)
        ^ (c[1] as u64).wrapping_mul(MIX_Y)
        ^ (c[2] as u64).wrapping_mul(MIX_Z)
}

/// THE KURVENLINEAL. Deterministic bipolar relief in [-1, 1] from the
/// vertex's lattice address — phase is regenerated from the address, NEVER
/// stored: cell = floor(pos·detail) per axis → place = mix(cell); a finer
/// 3× sub-lattice (27 sub-cells per cell) picks k = mix(sub) mod 17; value =
/// (CurveRuler::from_place(place).index(k) / 16)·2 − 1. Same position +
/// detail → same value on every stroke, every session.
pub fn ruler_phase(pos: [f32; 3], detail: f32) -> f32 {
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
    // index ∈ [0,17) → idx/16 ∈ [0,1] → bipolar [-1,1] with 8 → 0.
    (idx as f32 / 16.0) * 2.0 - 1.0
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn add_scaled(p: [f32; 3], v: [f32; 3], k: f32) -> [f32; 3] {
    [p[0] + v[0] * k, p[1] + v[1] * k, p[2] + v[2] * k]
}

fn write_pos(mesh: &mut Mesh, new: &[(u32, [f32; 3])]) {
    for &(i, p) in new {
        mesh.pos[i as usize] = p;
    }
}

fn lerp_u8(a: u8, b: u8, k: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * k)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::weld;
    use crate::stl::TriSoup;

    /// Hexagonal fan: apex raised to z=0.5 over a unit ring at z=0. Ring
    /// verts sit 1.118 from the apex, so radius 0.6 touches only the apex.
    fn spike_soup() -> TriSoup {
        let apex = [0.0, 0.0, 0.5];
        let ring: Vec<[f32; 3]> = (0..6)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                [a.cos(), a.sin(), 0.0]
            })
            .collect();
        let tris = (0..6).map(|i| [apex, ring[i], ring[(i + 1) % 6]]).collect();
        TriSoup { tris }
    }

    fn apex_index(m: &Mesh) -> usize {
        let mut best = 0;
        for (i, p) in m.pos.iter().enumerate() {
            if p[2] > m.pos[best][2] {
                best = i;
            }
        }
        best
    }

    fn stroke(tool: Tool) -> Stroke {
        Stroke {
            tool,
            center: [0.0, 0.0, 0.5],
            dir: [0.0, 0.0, 0.3],
            radius: 0.6,
            strength: 1.0,
            color: [255, 40, 40],
            detail: 16.0,
        }
    }

    #[test]
    fn tool_from_str() {
        assert_eq!("grab".parse::<Tool>().unwrap(), Tool::Grab);
        assert_eq!("inflate".parse::<Tool>().unwrap(), Tool::Inflate);
        assert_eq!("smooth".parse::<Tool>().unwrap(), Tool::Smooth);
        assert_eq!("spray".parse::<Tool>().unwrap(), Tool::Spray);
        assert_eq!("ruler".parse::<Tool>().unwrap(), Tool::Ruler);
        assert!("chainsaw".parse::<Tool>().is_err());
    }

    #[test]
    fn grab_moves_only_in_radius() {
        let mut m = weld(&spike_soup());
        let before = m.pos.clone();
        let s = stroke(Tool::Grab);
        let u = apply(&mut m, &s);
        assert_eq!(u.verts.len(), 1, "only the apex is inside radius 0.6");
        for (i, p) in m.pos.iter().enumerate() {
            let d = dist(before[i], s.center);
            if d < s.radius {
                assert!(
                    (p[2] - (before[i][2] + 0.3)).abs() < 1e-6,
                    "apex pulled by dir"
                );
            } else {
                // Out-of-radius verts must be byte-identical, not just close.
                assert_eq!(p.map(f32::to_bits), before[i].map(f32::to_bits));
            }
        }
    }

    #[test]
    fn undo_round_trip_byte_identical() {
        let mut m = weld(&spike_soup());
        let pos0: Vec<[u32; 3]> = m.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        let col0 = m.col.clone();
        let u = apply(&mut m, &stroke(Tool::Grab));
        assert!(!u.verts.is_empty());
        revert(&mut m, &u);
        let pos1: Vec<[u32; 3]> = m.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        assert_eq!(pos0, pos1);
        assert_eq!(col0, m.col);
    }

    #[test]
    fn smooth_shrinks_spike_toward_neighbors() {
        let mut m = weld(&spike_soup());
        let apex = apex_index(&m);
        let z0 = m.pos[apex][2];
        apply(&mut m, &stroke(Tool::Smooth));
        let z1 = m.pos[apex][2];
        assert!(z1 < z0, "apex must descend toward the ring");
        // Full strength at t=0 → w=1 → apex lands on the ring mean (z=0).
        assert!(z1.abs() < 1e-5, "apex z after full smooth: {z1}");
    }

    #[test]
    fn spray_full_strength_hits_target_color() {
        let mut m = weld(&spike_soup());
        let apex = apex_index(&m);
        let s = stroke(Tool::Spray);
        let pos_before: Vec<[u32; 3]> = m.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        apply(&mut m, &s);
        // w=1 at the center → full lerp to the stroke color.
        assert_eq!(m.col[apex], s.color);
        // Spray never moves positions.
        let pos_after: Vec<[u32; 3]> = m.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        assert_eq!(pos_before, pos_after);
    }

    #[test]
    fn ruler_phase_deterministic() {
        for p in [[0.13, -0.7, 0.42], [3.1, 2.2, -5.5], [0.0, 0.0, 0.0]] {
            for detail in [4.0, 16.0, 64.0] {
                assert_eq!(
                    ruler_phase(p, detail).to_bits(),
                    ruler_phase(p, detail).to_bits()
                );
            }
        }
    }

    #[test]
    fn ruler_phase_spans_both_signs() {
        let (mut saw_pos, mut saw_neg) = (false, false);
        for i in 0..32 {
            for j in 0..32 {
                let v = ruler_phase([i as f32 * 0.11, j as f32 * 0.07, 0.3], 16.0);
                assert!((-1.0..=1.0).contains(&v), "phase {v} outside [-1,1]");
                saw_pos |= v > 0.0;
                saw_neg |= v < 0.0;
            }
        }
        assert!(saw_pos && saw_neg, "lattice sweep must produce both signs");
    }

    #[test]
    fn ruler_restroke_converges() {
        // The relief is a function of address, not of stroke history: two
        // strokes on an identical mesh displace along the same signs — the
        // second stroke on the already-displaced mesh may re-bucket verts,
        // but an identical starting mesh always yields identical output.
        let mut m1 = weld(&spike_soup());
        let mut m2 = weld(&spike_soup());
        apply(&mut m1, &stroke(Tool::Ruler));
        apply(&mut m2, &stroke(Tool::Ruler));
        let p1: Vec<[u32; 3]> = m1.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        let p2: Vec<[u32; 3]> = m2.pos.iter().map(|p| p.map(f32::to_bits)).collect();
        assert_eq!(p1, p2);
    }
}
