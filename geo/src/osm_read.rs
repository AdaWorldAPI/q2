//! `.osm.pbf` → projected geo features (feature `osm`; needs `osmpbf`).
//!
//! Two passes: pass 1 indexes every node's `(lon, lat)`; pass 2 resolves each
//! target way's node refs into a ground-plane ring, projected equirectangularly
//! about the region centre. Shared by both the SPM1 (`osm_bake`) and the BSO2
//! `/helix` (`osm_helix`) bins so the ingestion logic lives in exactly one place.

use std::collections::HashMap;

use osmpbf::{Element, ElementReader};

use crate::extrude::Layer;

/// Metres per degree of latitude (WGS84 mean) — the equirectangular scale.
pub const M_PER_DEG: f64 = 111_320.0;

/// One projected feature: a ground-plane ring (local metres), an extrusion
/// height, its layer, and the `(lon, lat)` centroid used for its HHTL key.
pub struct GeoFeature {
    /// Closed ring in the ground plane `[x, z]` (local metric frame).
    pub ring: Vec<[f32; 2]>,
    /// Extrusion height (metres); `0.0` = flat.
    pub height: f32,
    /// Feature layer (building / road / area).
    pub kind: Layer,
    /// Centroid longitude (for the HHTL key).
    pub lon: f64,
    /// Centroid latitude (for the HHTL key).
    pub lat: f64,
}

/// A read scene: the projected features + the projection origin `(lon0, lat0)`.
pub struct GeoScene {
    /// All features, row = vector index (BSO2 `row`, a `u32`).
    pub features: Vec<GeoFeature>,
    /// Projection origin longitude (region centroid).
    pub lon0: f64,
    /// Projection origin latitude (region centroid).
    pub lat0: f64,
}

/// One target way before projection.
struct RawWay {
    refs: Vec<i64>,
    height: f32,
}

/// Read every `building=*` way from `path` and project to a [`GeoScene`].
///
/// (Roads / water / landuse are the natural next additions — same two-pass
/// shape, different tag filter + a flat `height = 0.0`. Buildings first for the
/// `/helix` proof render.)
pub fn read_buildings(path: &str) -> Result<GeoScene, Box<dyn std::error::Error>> {
    // ── Pass 1: node id → (lon, lat). ──
    let mut nodes: HashMap<i64, (f64, f64)> = HashMap::new();
    ElementReader::from_path(path)?.for_each(|el| match el {
        Element::Node(n) => {
            nodes.insert(n.id(), (n.lon(), n.lat()));
        }
        Element::DenseNode(n) => {
            nodes.insert(n.id(), (n.lon(), n.lat()));
        }
        _ => {}
    })?;
    eprintln!("pass 1: {} nodes indexed", nodes.len());

    // ── Pass 2: building ways → refs + height. ──
    let mut ways: Vec<RawWay> = Vec::new();
    ElementReader::from_path(path)?.for_each(|el| {
        if let Element::Way(w) = el {
            let mut is_building = false;
            let mut height: Option<f32> = None;
            let mut levels: Option<f32> = None;
            for (k, v) in w.tags() {
                match k {
                    "building" if v != "no" => is_building = true,
                    "height" | "building:height" => height = parse_leading_f32(v),
                    "building:levels" => levels = parse_leading_f32(v),
                    _ => {}
                }
            }
            if is_building {
                let refs: Vec<i64> = w.refs().collect();
                if refs.len() >= 4 {
                    let h = height.or_else(|| levels.map(|l| l * 3.0)).unwrap_or(6.0);
                    ways.push(RawWay { refs, height: h });
                }
            }
        }
    })?;
    eprintln!("pass 2: {} building ways", ways.len());
    if ways.is_empty() {
        return Err("no building ways found in extract".into());
    }

    // ── Projection origin = mean building-node coordinate. ──
    let (mut sum_lon, mut sum_lat, mut cnt) = (0.0f64, 0.0f64, 0u64);
    for way in &ways {
        for id in &way.refs {
            if let Some(&(lon, lat)) = nodes.get(id) {
                sum_lon += lon;
                sum_lat += lat;
                cnt += 1;
            }
        }
    }
    if cnt == 0 {
        // A clipped/partial extract whose building ways reference nodes outside
        // the bbox — dividing by 0 would poison lon0/lat0 (NaN) into every ring.
        return Err("building ways reference no indexed nodes".into());
    }
    let (lon0, lat0) = (sum_lon / cnt as f64, sum_lat / cnt as f64);
    let cos_lat0 = lat0.to_radians().cos();
    let project = |lon: f64, lat: f64| -> [f32; 2] {
        [
            ((lon - lon0) * cos_lat0 * M_PER_DEG) as f32,
            ((lat - lat0) * M_PER_DEG) as f32,
        ]
    };

    // ── Resolve rings + centroids. ──
    let mut features = Vec::with_capacity(ways.len());
    for way in &ways {
        let mut ring = Vec::with_capacity(way.refs.len());
        let (mut clon, mut clat) = (0.0f64, 0.0f64);
        let mut ok = true;
        for id in &way.refs[..way.refs.len().saturating_sub(1)] {
            match nodes.get(id) {
                Some(&(lon, lat)) => {
                    ring.push(project(lon, lat));
                    clon += lon;
                    clat += lat;
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || ring.len() < 3 {
            continue;
        }
        let n = ring.len() as f64;
        features.push(GeoFeature {
            ring,
            height: way.height,
            kind: Layer::Building,
            lon: clon / n,
            lat: clat / n,
        });
    }

    if features.is_empty() {
        // Ways existed but none resolved to a ≥3-point ring — treat as failure
        // so a caller doesn't write a valid-but-empty artifact.
        return Err("no building ways with resolvable geometry".into());
    }
    Ok(GeoScene {
        features,
        lon0,
        lat0,
    })
}

/// Parse the leading float from an OSM measurement tag (`"12 m"` → `12`).
pub fn parse_leading_f32(s: &str) -> Option<f32> {
    let t = s.trim();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(t.len());
    t[..end].parse().ok()
}
