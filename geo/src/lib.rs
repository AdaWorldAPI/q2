//! `geo-hhtl` — OSM geography baked into the HHTL substrate, emitted as SPM1.
//!
//! The geo twin of the `fma` anatomy crate. `fma` bakes BodyParts3D meshes into
//! the SPM1 wire keyed by anatomy GUIDs; `geo` bakes OSM features into the SAME
//! wire keyed by their HHTL tile GUIDs. The cockpit's `FmaBody.tsx` decoder
//! renders either — geography and anatomy are two domain bindings of one
//! address cascade (OGAR canon "Tier interpretation — 256×256 CENTROID TILE").
//!
//! Three dep-free stages, all pure:
//! - [`hhtl`] — `(lon, lat, z)` → the HHTL tier key (spatial half of the GUID);
//! - [`extrude`] — footprint + `building:height` → a 3D prism;
//! - [`spm1`] — the indexed-triangle mesh wire the viewer already decodes.
//!
//! The one thing NOT here is `.osm.pbf` ingestion (it needs an external reader);
//! that lands in a `bin` once the reader path is chosen. Everything from a
//! `Vec<Footprint>` onward is complete and tested.

pub mod extrude;
pub mod hhtl;
pub mod spm1;

/// Garmin IMG map decoder (dep-free, pure `std`): typed features + contours
/// from the banked `.img` sources — the ground-up replacement for raster
/// colour-guessing in the geo bake pipeline.
pub mod garmin;

/// Cesium-substrate seed (dep-free): the ratified TMS-quadkey addressing for the
/// `cesium-osm-substrate-v1` path — the "version 2, for later" the `/helix` bake
/// is a throwaway proof of.
pub mod cesium;

/// `.osm.pbf` ingestion (feature `osm`; pulls `osmpbf`).
#[cfg(feature = "osm")]
pub mod osm_read;

/// BSO2 `/helix` wire encoder (feature `helix`; pulls `ndarray` + `lance-graph-contract`).
#[cfg(feature = "helix")]
pub mod bso2;

/// The Kurvenlineal — the real `helix::CurveRuler` golden-spiral residue as a
/// CONTINUOUS INTER-FAMILY field (fixes the per-cell "needle" discontinuity).
#[cfg(feature = "helix")]
pub mod kurvenlineal;

pub use extrude::{extrude_into, Footprint, Layer};
pub use hhtl::{point_to_hhtl, tile_to_hhtl, Hhtl, GEO_DOMAIN, HHTL_DEPTH};
pub use spm1::{Mesh, Vertex};

/// A baked OSM feature: its HHTL key (for addressing / routing / dedup) paired
/// with the footprint that produced its geometry. The key is what a renderer or
/// planner reads with zero value decode; the mesh is the value residue.
#[derive(Debug, Clone)]
pub struct BakedFeature {
    /// The feature's HHTL tier key (from its footprint centroid at `zoom`).
    pub key: Hhtl,
    /// The source footprint (geometry + layer + row).
    pub footprint: Footprint,
}

/// Bake a batch of `(footprint, lon, lat)` features at a fixed `zoom` into one
/// SPM1 [`Mesh`] plus the per-feature HHTL keys. The footprint ring is already
/// in the local metric frame; `(lon, lat)` is the feature's anchor used only to
/// compute its HHTL key (the mesh carries the geometry, the key carries the
/// address — place/residue split, per the OGAR helix encoding).
#[must_use]
pub fn bake(features: &[(Footprint, f64, f64)], zoom: u32) -> (Mesh, Vec<BakedFeature>) {
    let mut mesh = Mesh::default();
    let mut baked = Vec::with_capacity(features.len());
    for (footprint, lon, lat) in features {
        extrude::extrude_into(&mut mesh, footprint);
        baked.push(BakedFeature {
            key: hhtl::point_to_hhtl(*lon, *lat, zoom),
            footprint: footprint.clone(),
        });
    }
    mesh.node_count = baked.len() as u32;
    (mesh, baked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_bremen_building_bakes_to_keyed_spm1() {
        // A single 10×10 m building anchored at Bremen centre, baked at z=16.
        let f = Footprint {
            ring: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            height: 15.0,
            layer: Layer::Building,
            node_row: 0,
        };
        let (mesh, baked) = bake(&[(f, 8.807, 53.075)], 16);

        // The mesh is a valid, non-empty SPM1 prism with one source node.
        assert_eq!(mesh.node_count, 1);
        assert_eq!(mesh.verts.len(), 21);
        assert_eq!(mesh.tris.len(), 12);
        let mut buf = Vec::new();
        mesh.write_spm1(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"SPM1");
        assert_eq!(buf.len(), mesh.spm1_len());

        // The feature carries the SAME HHTL key the tile cascade would compute
        // for its anchor — geography addressed exactly like a semantic node.
        assert_eq!(baked.len(), 1);
        assert_eq!(baked[0].key, hhtl::point_to_hhtl(8.807, 53.075, 16));
    }
}
