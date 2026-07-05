//! The SPM1 indexed-triangle-mesh wire — byte-identical to `fma/src/bin/
//! cockpit_bake.rs` (and `bake_torso_mesh.py`), so the cockpit's existing
//! `FmaBody.tsx` decoder renders geo meshes with no viewer change.
//!
//! ```text
//! header 40 B: "SPM1" | vert_count u32 | tri_count u32 | node_count u32
//!              | bbox_min 3f | bbox_max 3f
//! vertex 21 B: pos 3f | normal 3i8 | rgb 3u8 | opacity(=LAYER id) u8 | node_row u16
//! index  12 B: 3× u32
//! ```
//! All little-endian. Positions are emitted in the baker's local frame; the
//! renderer applies the `(x, -z, y)` orientation + i8-normal dequant, same as
//! the anatomy body. `opacity` carries a LAYER id (a clean toggle byte), NOT a
//! continuous alpha — for geo it names the feature layer (building / road / …).

use std::io::{self, Write};

/// A single mesh vertex, laid out to the 21-byte SPM1 vertex record.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    /// Position in the baker's local frame (renderer reorients to `(x, -z, y)`).
    pub pos: [f32; 3],
    /// Quantized face normal (`i8`, renderer dequantizes to unit).
    pub normal: [i8; 3],
    /// Feature colour (`is_a` byte analogue — building tan, water blue, …).
    pub rgb: [u8; 3],
    /// LAYER id — the toggle byte the viewer buttons switch on.
    pub layer: u8,
    /// Back-reference to the source node row (the OSM feature's index).
    pub node_row: u16,
}

/// An indexed triangle mesh ready to serialize to SPM1.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    /// Vertex records.
    pub verts: Vec<Vertex>,
    /// Triangle index triples into `verts`.
    pub tris: Vec<[u32; 3]>,
    /// `node_count` header field: the number of distinct source nodes (OSM
    /// features) whose geometry is baked in.
    pub node_count: u32,
}

impl Mesh {
    /// Axis-aligned bounding box `(min, max)` over all vertex positions. Returns
    /// `([0;3], [0;3])` for an empty mesh (the header carries a degenerate box).
    #[must_use]
    pub fn bbox(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &self.verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
        }
        if self.verts.is_empty() {
            ([0.0; 3], [0.0; 3])
        } else {
            (lo, hi)
        }
    }

    /// Serialize to the SPM1 wire (little-endian). The exact byte length is
    /// `40 + 21 * verts.len() + 12 * tris.len()`.
    pub fn write_spm1<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let (lo, hi) = self.bbox();
        w.write_all(b"SPM1")?;
        w.write_all(&(self.verts.len() as u32).to_le_bytes())?;
        w.write_all(&(self.tris.len() as u32).to_le_bytes())?;
        w.write_all(&self.node_count.to_le_bytes())?;
        for c in lo {
            w.write_all(&c.to_le_bytes())?;
        }
        for c in hi {
            w.write_all(&c.to_le_bytes())?;
        }
        for v in &self.verts {
            for c in v.pos {
                w.write_all(&c.to_le_bytes())?;
            }
            w.write_all(&[v.normal[0] as u8, v.normal[1] as u8, v.normal[2] as u8])?;
            w.write_all(&v.rgb)?;
            w.write_all(&[v.layer])?;
            w.write_all(&v.node_row.to_le_bytes())?;
        }
        for t in &self.tris {
            for idx in t {
                w.write_all(&idx.to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// The exact serialized SPM1 length in bytes, without allocating.
    #[must_use]
    pub fn spm1_len(&self) -> usize {
        40 + 21 * self.verts.len() + 12 * self.tris.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_length_are_wire_exact() {
        let mesh = Mesh {
            verts: vec![Vertex {
                pos: [1.0, 2.0, 3.0],
                normal: [0, 127, 0],
                rgb: [200, 180, 150],
                layer: 3,
                node_row: 7,
            }],
            tris: vec![],
            node_count: 1,
        };
        let mut buf = Vec::new();
        mesh.write_spm1(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"SPM1");
        assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 1); // vert_count
        assert_eq!(u32::from_le_bytes(buf[8..12].try_into().unwrap()), 0); // tri_count
        assert_eq!(u32::from_le_bytes(buf[12..16].try_into().unwrap()), 1); // node_count
        assert_eq!(buf.len(), mesh.spm1_len());
        assert_eq!(buf.len(), 40 + 21);
    }
}
