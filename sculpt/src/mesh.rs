//! Welded, indexed mesh — the sculptable object.
//!
//! [`weld`] collapses STL triangle soup onto a lattice whose cell is
//! 1e-4 × bbox-diagonal (the contract-pinned weld epsilon): coincident
//! vertices merge via an i64×3 lattice key, degenerate triangles drop,
//! then area-weighted unit normals and deduped vertex adjacency are built.
//! Sculpt ops rely on two invariants held here: `nrm` is unit everywhere,
//! `adj` lists are sorted + deduped and symmetric.

use crate::stl::TriSoup;
use std::collections::HashMap;

/// Welded, indexed mesh. `col` is per-vertex RGB (sculpt paint lives here).
/// `adj` is vertex→vertex adjacency (from shared triangle edges, deduped).
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
    pub nrm: Vec<[f32; 3]>, // per-vertex, area-weighted, unit
    pub col: Vec<[u8; 3]>,  // init 200,200,205
    pub adj: Vec<Vec<u32>>,
}

/// Weld the soup: quantize positions to a lattice of 1e-4 × bbox-diagonal,
/// coincident verts merge (i64x3 lattice key in a HashMap). Degenerate tris
/// (repeated welded index) are dropped. Then normals + adjacency are built.
pub fn weld(soup: &TriSoup) -> Mesh {
    let (mn, mx) = bbox(soup.tris.iter().flatten());
    let diag = length(sub(mx, mn));
    // Zero diagonal (empty / single-point soup) gets a tiny positive cell so
    // the quantization below stays finite; any positive value welds all-equal
    // points identically.
    let cell = if diag > 0.0 { diag * 1e-4 } else { 1e-4 };

    let mut key_to_idx: HashMap<[i64; 3], u32> = HashMap::new();
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for tri in &soup.tris {
        let mut idx = [0u32; 3];
        for (slot, v) in tri.iter().enumerate() {
            // round(), not floor(): coincident floats that straddle a cell
            // boundary by <½ cell still land in the same lattice bucket.
            let key = [
                (v[0] / cell).round() as i64,
                (v[1] / cell).round() as i64,
                (v[2] / cell).round() as i64,
            ];
            idx[slot] = *key_to_idx.entry(key).or_insert_with(|| {
                pos.push(*v); // keep the first-seen coordinates, not the cell center
                (pos.len() - 1) as u32
            });
        }
        // A repeated welded index means zero area AND a self-edge in adjacency
        // — drop before it can poison either.
        if idx[0] != idx[1] && idx[1] != idx[2] && idx[0] != idx[2] {
            tris.push(idx);
        }
    }

    let n = pos.len();
    let mut mesh = Mesh {
        pos,
        tris,
        nrm: vec![[0.0; 3]; n],
        col: vec![[200, 200, 205]; n],
        adj: Vec::new(),
    };
    mesh.recompute_normals();
    mesh.adj = build_adjacency(n, &mesh.tris);
    mesh
}

impl Mesh {
    /// Area-weighted vertex normals: the raw cross (b−a)×(c−a) has magnitude
    /// 2×area, so summing unnormalized face crosses IS the area weighting.
    /// Zero-sum vertices (isolated, or perfectly cancelling fans) fall back
    /// to +Z so the `nrm`-is-unit invariant holds everywhere.
    pub fn recompute_normals(&mut self) {
        let mut acc = vec![[0.0f32; 3]; self.pos.len()];
        for t in &self.tris {
            let a = self.pos[t[0] as usize];
            let b = self.pos[t[1] as usize];
            let c = self.pos[t[2] as usize];
            let f = cross(sub(b, a), sub(c, a));
            for &vi in t {
                let s = &mut acc[vi as usize];
                s[0] += f[0];
                s[1] += f[1];
                s[2] += f[2];
            }
        }
        self.nrm = acc
            .into_iter()
            .map(|v| {
                let l = length(v);
                if l > 0.0 {
                    [v[0] / l, v[1] / l, v[2] / l]
                } else {
                    [0.0, 0.0, 1.0]
                }
            })
            .collect();
    }

    /// Center the bbox on the origin and scale so the max half-extent → 1.0
    /// (bbox inside [-1,1]³). Uniform scale + translation: normals unchanged.
    pub fn normalize_unit(&mut self) {
        if self.pos.is_empty() {
            return;
        }
        let (mn, mx) = bbox(self.pos.iter());
        let c = [
            (mn[0] + mx[0]) * 0.5,
            (mn[1] + mx[1]) * 0.5,
            (mn[2] + mx[2]) * 0.5,
        ];
        let half = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]) * 0.5;
        let s = if half > 0.0 { 1.0 / half } else { 1.0 };
        for p in &mut self.pos {
            for a in 0..3 {
                p[a] = (p[a] - c[a]) * s;
            }
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.pos.len()
    }

    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }
}

/// Undirected adjacency from triangle edges; sorted + deduped per vertex so
/// Smooth's neighborhood mean is unweighted by edge multiplicity.
fn build_adjacency(n: usize, tris: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            adj[a as usize].push(b);
            adj[b as usize].push(a);
        }
    }
    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }
    adj
}

fn bbox<'a>(pts: impl Iterator<Item = &'a [f32; 3]>) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in pts {
        for a in 0..3 {
            mn[a] = mn[a].min(p[a]);
            mx[a] = mx[a].max(p[a]);
        }
    }
    (mn, mx)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn unique_edge_count(tris: &[[u32; 3]]) -> usize {
        let mut e = HashSet::new();
        for t in tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                e.insert((a.min(b), a.max(b)));
            }
        }
        e.len()
    }

    /// Regular tetrahedron as raw soup (outward winding), 12 soup verts → 4.
    fn tetra_soup() -> TriSoup {
        let p = [
            [1.0, 1.0, 1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
        ];
        TriSoup {
            tris: vec![
                [p[0], p[1], p[2]],
                [p[0], p[2], p[3]],
                [p[0], p[3], p[1]],
                [p[1], p[3], p[2]],
            ],
        }
    }

    #[test]
    fn weld_tetra_euler() {
        let m = weld(&tetra_soup());
        assert_eq!(m.vertex_count(), 4);
        assert_eq!(m.tri_count(), 4);
        let e = unique_edge_count(&m.tris) as i64;
        assert_eq!(m.vertex_count() as i64 - e + m.tri_count() as i64, 2);
    }

    #[test]
    fn weld_icosphere_euler() {
        let m = weld(&crate::stl::icosphere(2));
        let e = unique_edge_count(&m.tris) as i64;
        assert_eq!(
            m.vertex_count() as i64 - e + m.tri_count() as i64,
            2,
            "welded icosphere must be a closed 2-manifold (V−E+F=2)"
        );
    }

    #[test]
    fn weld_drops_degenerate() {
        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        let c = [0.0, 1.0, 0.0];
        // Second tri repeats `a` → welds to a repeated index → dropped.
        let m = weld(&TriSoup {
            tris: vec![[a, b, c], [a, a, b]],
        });
        assert_eq!(m.tri_count(), 1);
        assert_eq!(m.vertex_count(), 3);
    }

    #[test]
    fn normals_unit_length() {
        let m = weld(&crate::stl::icosphere(1));
        for n in &m.nrm {
            let l = length(*n);
            assert!((l - 1.0).abs() < 1e-4, "normal length {l}");
        }
    }

    #[test]
    fn normalize_unit_bounds() {
        // Offset + scale the tetra so normalize has real work to do.
        let mut soup = tetra_soup();
        for t in &mut soup.tris {
            for v in t.iter_mut() {
                for a in 0..3 {
                    v[a] = v[a] * 3.0 + 5.0;
                }
            }
        }
        let mut m = weld(&soup);
        m.normalize_unit();
        let mut max_ext = 0.0f32;
        for p in &m.pos {
            for a in 0..3 {
                assert!(
                    (-1.0 - 1e-5..=1.0 + 1e-5).contains(&p[a]),
                    "coord {} outside [-1,1]",
                    p[a]
                );
                max_ext = max_ext.max(p[a].abs());
            }
        }
        assert!((max_ext - 1.0).abs() < 1e-5, "max half-extent {max_ext}");
    }

    #[test]
    fn adjacency_symmetric_dedup() {
        let m = weld(&tetra_soup());
        for (i, list) in m.adj.iter().enumerate() {
            let mut seen = HashSet::new();
            for &j in list {
                assert!(seen.insert(j), "duplicate neighbor");
                assert!(
                    m.adj[j as usize].contains(&(i as u32)),
                    "adjacency not symmetric"
                );
            }
        }
        // Tetrahedron: every vertex borders the other three.
        for list in &m.adj {
            assert_eq!(list.len(), 3);
        }
    }
}
