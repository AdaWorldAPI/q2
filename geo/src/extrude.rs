//! Footprint polygon + height → a building prism (walls + flat roof).
//!
//! This is where the substrate's Z axis pays off: an OSM building `way` carries
//! a `building:height` (or `building:levels` × ~3 m); extruding the footprint by
//! it turns a 2D map feature into 3D massing — a city block rendered from GUIDs,
//! the same way `fma` renders an organ mesh. Flat-shaded (per-face normals, no
//! vertex sharing between faces) so each wall/roof toggles cleanly.

use crate::spm1::{Mesh, Vertex};

/// Coarse OSM feature layer → the SPM1 `layer` toggle byte + a base colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// A `building=*` footprint, extruded to its height.
    Building = 3,
    /// A `highway=*` / `waterway=*` centre-line strip (flat).
    Road = 4,
    /// A `natural=water` / `landuse=*` fill (flat).
    Area = 5,
}

impl Layer {
    /// The base RGB for this layer (the `is_a` colour byte analogue).
    #[must_use]
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Layer::Building => [198, 184, 162], // warm tan
            Layer::Road => [90, 90, 96],        // asphalt grey
            Layer::Area => [122, 158, 196],     // water blue
        }
    }
}

/// A single OSM feature to bake: a footprint ring (x/z ground-plane metres in
/// the local frame), an extrusion height, its layer, and its source row index.
#[derive(Debug, Clone)]
pub struct Footprint {
    /// Closed ring in the ground plane `[x, z]` (last point need not repeat the
    /// first; the ring is treated as implicitly closed).
    pub ring: Vec<[f32; 2]>,
    /// Extrusion height in metres. `0.0` emits only the flat cap (roads/areas).
    pub height: f32,
    /// Feature layer (colour + toggle byte).
    pub layer: Layer,
    /// Source node row (OSM feature index) stamped into every emitted vertex.
    pub node_row: u16,
}

fn ring_centroid(ring: &[[f32; 2]]) -> [f32; 2] {
    let n = ring.len().max(1) as f32;
    let (mut sx, mut sz) = (0.0f32, 0.0f32);
    for p in ring {
        sx += p[0];
        sz += p[1];
    }
    [sx / n, sz / n]
}

/// Append one footprint's prism (walls + roof cap) into `mesh`. Non-sharing
/// vertices keep normals flat and per-face; degenerate rings (<3 points) are
/// skipped so a bad OSM way can't poison the mesh.
pub fn extrude_into(mesh: &mut Mesh, f: &Footprint) {
    let n = f.ring.len();
    if n < 3 {
        return;
    }
    let rgb = f.layer.rgb();
    let layer = f.layer as u8;
    let row = f.node_row;
    let top = f.height.max(0.0);

    let push = |mesh: &mut Mesh, pos: [f32; 3], normal: [i8; 3]| -> u32 {
        let idx = mesh.verts.len() as u32;
        mesh.verts.push(Vertex {
            pos,
            normal,
            rgb,
            layer,
            node_row: row,
        });
        idx
    };

    // ── Walls: one quad per edge (two triangles), only when extruded. ──
    if top > 0.0 {
        for i in 0..n {
            let a = f.ring[i];
            let b = f.ring[(i + 1) % n];
            // Outward-ish normal from the edge direction (flat-shaded, i8-quant).
            let (ex, ez) = (b[0] - a[0], b[1] - a[1]);
            let len = (ex * ex + ez * ez).sqrt().max(1e-6);
            let nx = (ez / len * 127.0) as i8;
            let nz = (-ex / len * 127.0) as i8;
            let nrm = [nx, 0, nz];
            let v0 = push(mesh, [a[0], 0.0, a[1]], nrm);
            let v1 = push(mesh, [b[0], 0.0, b[1]], nrm);
            let v2 = push(mesh, [b[0], top, b[1]], nrm);
            let v3 = push(mesh, [a[0], top, a[1]], nrm);
            mesh.tris.push([v0, v1, v2]);
            mesh.tris.push([v0, v2, v3]);
        }
    }

    // ── Cap: fan from the centroid at the top height (roof, or ground fill). ──
    let c = ring_centroid(&f.ring);
    let up = [0i8, 127, 0];
    let cap = push(mesh, [c[0], top, c[1]], up);
    let mut rim = Vec::with_capacity(n);
    for p in &f.ring {
        rim.push(push(mesh, [p[0], top, p[1]], up));
    }
    for i in 0..n {
        mesh.tris.push([cap, rim[i], rim[(i + 1) % n]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_building_extrudes_to_a_prism() {
        let f = Footprint {
            ring: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            height: 12.0,
            layer: Layer::Building,
            node_row: 42,
        };
        let mut mesh = Mesh::default();
        extrude_into(&mut mesh, &f);
        // Walls: 4 edges × 4 verts = 16, × 2 tris = 8. Cap: centroid + 4 rim = 5
        // verts, 4 fan tris. Total 21 verts, 12 tris.
        assert_eq!(mesh.verts.len(), 21);
        assert_eq!(mesh.tris.len(), 12);
        // Every vertex carries the feature's row + layer.
        assert!(mesh.verts.iter().all(|v| v.node_row == 42 && v.layer == 3));
        // The roof cap reaches the extrusion height.
        assert!(mesh.verts.iter().any(|v| (v.pos[1] - 12.0).abs() < 1e-6));
    }

    #[test]
    fn a_flat_area_emits_only_the_cap() {
        let f = Footprint {
            ring: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
            height: 0.0,
            layer: Layer::Area,
            node_row: 1,
        };
        let mut mesh = Mesh::default();
        extrude_into(&mut mesh, &f);
        // No walls (height 0): centroid + 4 rim = 5 verts, 4 tris.
        assert_eq!(mesh.verts.len(), 5);
        assert_eq!(mesh.tris.len(), 4);
        assert!(mesh.verts.iter().all(|v| v.pos[1] == 0.0));
    }
}
