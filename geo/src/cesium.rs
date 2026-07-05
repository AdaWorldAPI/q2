//! Cesium-substrate seed — the "version 2, for later" path (dep-free).
//!
//! The `/helix` bake ([`crate::bso2`]) is the **throwaway triangle proof**:
//! OSM-XYZ HHTL keys, extruded prisms, the BSO2 mesh wire. The **ratified**
//! path is different, and this module seeds it without building the whole thing:
//!
//! `lance-graph/.claude/plans/cesium-osm-substrate-v1.md` makes OSM a 6th source
//! class in the 3DGS/ArcGIS/Cesium pipeline (D-OSM-1..7). Its rulings that this
//! seed honors:
//! - **Q2 — Cesium TMS quadkey**, NOT the OSM-XYZ slippy key. See
//!   [`crate::hhtl::point_to_tms_morton`] (the boundary Y-flip is
//!   [`crate::hhtl::xyz_to_tms_y`], per Q3).
//! - **Substrate = Gaussian splats** (`ndarray::hpc::splat3d::GaussianBatch` +
//!   `cam_pq`), shared with the anatomy/ultrasound arc — "two scenes, one
//!   substrate." Prisms are NOT the target; a footprint becomes a set of
//!   3D Gaussians fit to its massing.
//! - **Home = `ndarray::crates/cesium/src/osm_pbf.rs`** (D-OSM-1 stub shipped;
//!   D-OSM-2 wires `osmpbf` there). This seed's reader ([`crate::osm_read`]) is
//!   the ingestion half D-OSM-2 needs; the transform-to-Gaussian half is D-OSM-3.
//!
//! So this module carries the ONE piece that is both small and ratified — the
//! TMS-keyed feature shape — so the future cesium session starts from the right
//! key instead of re-deriving it from the throwaway path's rejected OSM-XYZ key.

use crate::extrude::Layer;
use crate::hhtl;

/// A feature addressed the **Cesium** way: the TMS quadkey Morton index (the
/// `implicit_tiling` subtree coordinate) + the raw footprint + height + kind.
/// The D-OSM-3 transform lowers this to `GaussianBatch` + `cam_pq` indices;
/// until then it is the honest hand-off shape between ingest and substrate.
#[derive(Debug, Clone)]
pub struct CesiumFeature {
    /// Cesium TMS quadkey Morton index (NiblePath prefix key).
    pub tms_morton: u64,
    /// Footprint ring in local metric ground plane `[x, z]`.
    pub ring: Vec<[f32; 2]>,
    /// Extrusion height (metres); `0.0` = flat.
    pub height: f32,
    /// Feature kind.
    pub kind: Layer,
}

/// The TMS zoom the seed keys at (matches the HHTL 4-tier depth so the two keys
/// are directly comparable in tests / audits).
pub const CESIUM_TMS_ZOOM: u32 = hhtl::HHTL_DEPTH4;

/// Key one geographic feature the Cesium way. `(lon, lat)` is the anchor; the
/// footprint/height carry the geometry the D-OSM-3 Gaussian fit will consume.
#[must_use]
pub fn cesium_feature(
    lon: f64,
    lat: f64,
    ring: Vec<[f32; 2]>,
    height: f32,
    kind: Layer,
) -> CesiumFeature {
    CesiumFeature {
        tms_morton: hhtl::point_to_tms_morton(lon, lat, CESIUM_TMS_ZOOM),
        ring,
        height,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cesium_feature_carries_the_tms_key() {
        let f = cesium_feature(
            8.807,
            53.075,
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            6.0,
            Layer::Building,
        );
        // The key is the TMS Morton, not the OSM-XYZ HHTL twig — the ratified
        // difference this seed exists to preserve.
        assert_eq!(
            f.tms_morton,
            hhtl::point_to_tms_morton(8.807, 53.075, CESIUM_TMS_ZOOM)
        );
        assert_eq!(f.ring.len(), 3);
    }
}
