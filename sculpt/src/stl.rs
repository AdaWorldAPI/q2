//! Binary + ASCII STL I/O and the built-in demo models.
//!
//! Detection order is LAW: binary-by-arithmetic FIRST. Real-world binary
//! exporters emit 80-byte headers that begin with the bytes "solid", so a
//! prefix sniff alone misroutes them into the ASCII parser. The size identity
//! `len == 84 + 50 * tri_count` is unambiguous for well-formed binary files
//! and near-impossible for ASCII to satisfy by accident (the u32 at offset 80
//! would have to equal the exact facet arithmetic of the text around it).

use std::collections::HashMap;

/// Raw triangle soup, exactly as an STL stores it (no connectivity).
#[derive(Clone, Debug)]
pub struct TriSoup {
    pub tris: Vec<[[f32; 3]; 3]>,
}

/// Binary + ASCII STL, autodetected. Binary check FIRST and by arithmetic
/// (`len >= 84 && 84 + 50*tri_count == len`) — some binary files start with
/// the bytes "solid", so `starts_with("solid")` alone is WRONG.
pub fn read_stl(bytes: &[u8]) -> Result<TriSoup, String> {
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
        // u64 arithmetic: 50 * u32::MAX must not wrap the check.
        if 84 + n as u64 * 50 == bytes.len() as u64 {
            return Ok(read_binary(bytes, n as usize));
        }
    }
    read_ascii(bytes)
}

/// Length was validated arithmetically by the caller: every slice is in bounds.
fn read_binary(bytes: &[u8], n: usize) -> TriSoup {
    let mut tris = Vec::with_capacity(n);
    let mut off = 84usize;
    for _ in 0..n {
        off += 12; // stored normal skipped: geometry is truth, the field often lies
        let mut tri = [[0f32; 3]; 3];
        for v in tri.iter_mut() {
            for c in v.iter_mut() {
                *c = f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
                off += 4;
            }
        }
        off += 2; // attribute byte count (ignored)
        tris.push(tri);
    }
    TriSoup { tris }
}

fn read_ascii(bytes: &[u8]) -> Result<TriSoup, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "not binary STL (size != 84 + 50*count) and not UTF-8 ASCII".to_string())?;
    if !text.trim_start().starts_with("solid") {
        return Err("ASCII STL must start with 'solid'".to_string());
    }
    // Token scan: only "vertex x y z" carries geometry; facet/loop keywords
    // and the (untrusted) "facet normal" line are noise.
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut tok = text.split_whitespace();
    while let Some(w) = tok.next() {
        if w != "vertex" {
            continue;
        }
        let mut p = [0f32; 3];
        for c in p.iter_mut() {
            *c = tok
                .next()
                .ok_or_else(|| "truncated vertex".to_string())?
                .parse::<f32>()
                .map_err(|e| format!("bad vertex coordinate: {e}"))?;
        }
        verts.push(p);
    }
    if !verts.len().is_multiple_of(3) {
        return Err(format!("{} vertices — not whole triangles", verts.len()));
    }
    Ok(TriSoup {
        tris: verts.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
    })
}

/// Binary STL writer (80B header, u32 count, then per triangle: 12B normal,
/// 3×12B verts, 2B attribute). Normal = unit face normal recomputed from the
/// triangle (never trusted from upstream). This is the printer round-trip:
/// what you sculpted is what you slice.
pub fn write_binary_stl(pos: &[[f32; 3]], tris: &[[u32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + tris.len() * 50);
    let mut header = [0u8; 80];
    // Any 80 bytes are legal; deliberately NOT "solid"-prefixed so naive
    // prefix-sniffing readers elsewhere stay honest too.
    header[..6].copy_from_slice(b"sculpt");
    out.extend_from_slice(&header);
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let n = normalize(cross(sub(b, a), sub(c, a)));
        for v in [n, a, b, c] {
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// Unit-radius icosphere by icosahedron subdivision. Midpoints are deduped on
/// the undirected edge, so shared soup vertices are bitwise-identical — the
/// downstream weld sees exact lattice hits and the surface stays closed.
/// `subdivisions` hard-capped at 5 (20·4⁵ = 20480 tris).
pub fn icosphere(subdivisions: u32) -> TriSoup {
    let depth = subdivisions.min(5);
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut pos: Vec<[f32; 3]> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .iter()
    .map(|&v| normalize(v))
    .collect();
    // The classic 20 faces, CCW seen from outside.
    let mut faces: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..depth {
        // Midpoint cache keyed on the undirected edge: each midpoint minted once.
        let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
        let mut next = Vec::with_capacity(faces.len() * 4);
        for &[a, b, c] in &faces {
            let ab = midpoint(&mut pos, &mut mid, a, b);
            let bc = midpoint(&mut pos, &mut mid, b, c);
            let ca = midpoint(&mut pos, &mut mid, c, a);
            // 1-into-4 split preserves the parent's winding.
            next.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = next;
    }
    TriSoup {
        tris: faces
            .iter()
            .map(|&[a, b, c]| [pos[a as usize], pos[b as usize], pos[c as usize]])
            .collect(),
    }
}

fn midpoint(pos: &mut Vec<[f32; 3]>, mid: &mut HashMap<(u32, u32), u32>, a: u32, b: u32) -> u32 {
    let key = (a.min(b), a.max(b));
    if let Some(&i) = mid.get(&key) {
        return i;
    }
    let (p, q) = (pos[a as usize], pos[b as usize]);
    // Re-projection to the unit sphere keeps radius exact at every depth.
    pos.push(normalize([p[0] + q[0], p[1] + q[1], p[2] + q[2]]));
    let i = (pos.len() - 1) as u32;
    mid.insert(key, i);
    i
}

/// XYZ-calibration-cube-ish: axis faces at ±half, every edge and corner
/// chamfered. Topology: 24 verts / 26 polygons (6 face squares + 12 edge
/// strips + 8 corner tris) → 44 triangles; V−E+F = 24−48+26 = 2, closed by
/// construction. Winding needs no bookkeeping: the solid is convex and
/// origin-centered, so outward ⇔ normal·centroid > 0 (fixed per triangle).
/// `chamfer` is clamped to `[0, 0.9·half]` so a real face always survives.
pub fn printer_cube(half: f32, chamfer: f32) -> TriSoup {
    let h = half;
    let c = chamfer.clamp(0.0, half * 0.9);
    let m = h - c;
    // Corner vertex on the `axis` face: full extent along axis, inset elsewhere.
    // Identical (axis, s) always yields bitwise-identical floats — shared soup
    // vertices weld exactly.
    let cv = |axis: usize, s: [f32; 3]| -> [f32; 3] {
        let mut p = [s[0] * m, s[1] * m, s[2] * m];
        p[axis] = s[axis] * h;
        p
    };
    let mut tris = Vec::with_capacity(44);
    // 6 face squares (inset by the chamfer on the two cross axes).
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [-1.0f32, 1.0] {
            let mk = |su: f32, sv: f32| {
                let mut s = [0.0f32; 3];
                s[axis] = sign;
                s[u] = su;
                s[v] = sv;
                cv(axis, s)
            };
            push_quad(
                &mut tris,
                [mk(-1.0, -1.0), mk(1.0, -1.0), mk(1.0, 1.0), mk(-1.0, 1.0)],
            );
        }
    }
    // 12 edge strips: one per unordered face pair (a,b) × sign combo; the
    // strip runs along the remaining axis e between the two face insets.
    for a in 0..3 {
        let b = (a + 1) % 3;
        let e = (a + 2) % 3;
        for sa in [-1.0f32, 1.0] {
            for sb in [-1.0f32, 1.0] {
                let mk = |face: usize, se: f32| {
                    let mut s = [0.0f32; 3];
                    s[a] = sa;
                    s[b] = sb;
                    s[e] = se;
                    cv(face, s)
                };
                push_quad(
                    &mut tris,
                    [mk(a, -1.0), mk(a, 1.0), mk(b, 1.0), mk(b, -1.0)],
                );
            }
        }
    }
    // 8 corner triangles close the three-way chamfer meeting point.
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                let s = [sx, sy, sz];
                push_ccw(&mut tris, cv(0, s), cv(1, s), cv(2, s));
            }
        }
    }
    TriSoup { tris }
}

/// Planar convex quad given as a proper cycle → two triangles, each
/// winding-fixed independently (consistent because the cycle is not a bowtie).
fn push_quad(tris: &mut Vec<[[f32; 3]; 3]>, q: [[f32; 3]; 4]) {
    push_ccw(tris, q[0], q[1], q[2]);
    push_ccw(tris, q[0], q[2], q[3]);
}

/// Push with outward (CCW-from-outside) winding. Valid ONLY for convex,
/// origin-centered solids: there, outward ⇔ n·centroid > 0.
fn push_ccw(tris: &mut Vec<[[f32; 3]; 3]>, a: [f32; 3], b: [f32; 3], c: [f32; 3]) {
    let n = cross(sub(b, a), sub(c, a));
    let ctr = [a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]]; // ×3: sign only
    if dot(n, ctr) < 0.0 {
        tris.push([a, c, b]);
    } else {
        tris.push([a, b, c]);
    }
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

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Unit vector; zero vector (degenerate triangle) → zero normal, per the
/// binary-STL convention.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bit-key the soup's vertices and count undirected edge incidences.
    /// Relies on shared vertices being bitwise-identical (both built-ins
    /// guarantee this by construction).
    fn edge_counts(soup: &TriSoup) -> (usize, HashMap<(u32, u32), u32>) {
        let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &soup.tris {
            let i: Vec<u32> = t
                .iter()
                .map(|p| {
                    let k = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
                    let next = ids.len() as u32;
                    *ids.entry(k).or_insert(next)
                })
                .collect();
            for k in 0..3 {
                let (a, b) = (i[k], i[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        (ids.len(), edges)
    }

    #[test]
    fn binary_round_trip() {
        // A tetrahedron, indexed.
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0f32],
        ];
        let tris = [[0u32, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]];
        let bytes = write_binary_stl(&pos, &tris);
        assert_eq!(bytes.len(), 84 + 50 * tris.len());
        let soup = read_stl(&bytes).expect("binary decode");
        assert_eq!(soup.tris.len(), tris.len());
        for (got, idx) in soup.tris.iter().zip(&tris) {
            for (g, w) in got.iter().zip(idx.iter().map(|&i| pos[i as usize])) {
                for (gc, wc) in g.iter().zip(w.iter()) {
                    assert!((gc - wc).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn ascii_two_triangles() {
        let src = "\
solid two
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
      vertex 0 1 0
    endloop
  endfacet
  facet normal 0 0 1
    outer loop
      vertex 1 0 0
      vertex 1 1 0
      vertex 0 1 0
    endloop
  endfacet
endsolid two
";
        let soup = read_stl(src.as_bytes()).expect("ascii decode");
        assert_eq!(soup.tris.len(), 2);
        assert_eq!(soup.tris[0][1], [1.0, 0.0, 0.0]);
        assert_eq!(soup.tris[1][1], [1.0, 1.0, 0.0]);
    }

    #[test]
    fn solid_prefixed_binary_is_binary() {
        let pos = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0f32]];
        let tris = [[0u32, 1, 2]];
        let mut bytes = write_binary_stl(&pos, &tris);
        // The trap this crate exists to dodge: a binary file whose header
        // starts with "solid". Arithmetic must win over the prefix sniff.
        bytes[..6].copy_from_slice(b"solid ");
        let soup = read_stl(&bytes).expect("decodes as binary despite 'solid' prefix");
        assert_eq!(soup.tris.len(), 1);
        assert_eq!(soup.tris[0][2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn icosphere_closed() {
        let soup = icosphere(2);
        assert_eq!(soup.tris.len(), 320); // 20 · 4²
        let (verts, edges) = edge_counts(&soup);
        // Closed manifold: every undirected edge borders exactly 2 triangles.
        assert!(
            edges.values().all(|&n| n == 2),
            "open or non-manifold edge found"
        );
        // Euler for a sphere: V − E + F = 2 (162 − 480 + 320).
        assert_eq!(
            verts as i64 - edges.len() as i64 + soup.tris.len() as i64,
            2
        );
    }

    #[test]
    fn printer_cube_closed() {
        let soup = printer_cube(1.0, 0.15);
        assert_eq!(soup.tris.len(), 44); // 6·2 + 12·2 + 8
        let (verts, edges) = edge_counts(&soup);
        assert_eq!(verts, 24);
        assert!(
            edges.values().all(|&n| n == 2),
            "open or non-manifold edge found"
        );
        // Triangulated Euler: 24 − 66 + 44 = 2 (48 polygon edges + 18 quad diagonals).
        assert_eq!(
            verts as i64 - edges.len() as i64 + soup.tris.len() as i64,
            2
        );
    }
}
