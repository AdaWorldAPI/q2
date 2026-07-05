//! OSM tile material — **where the maps come from**, and how a slippy tile
//! address becomes an HHTL (HEEL/HIP/TWIG) key.
//!
//! Two halves, per `docs/MERCATOR-HHTL-HELIX-MAP.md` (OGAR canon "256×256
//! centroid tile", Geo domain `0x0F`, "OSM: literal x/y"):
//!
//! 1. **The source.** OSM raster tiles come from the standard slippy-map tile
//!    servers ([`OSM_TILE_URL`]). `z/x/y` is the WebMercator (EPSG:3857)
//!    quadtree — `2^z × 2^z` tiles at zoom `z`.
//! 2. **The address.** A quadtree IS a cascade, so `z/x/y` Morton-interleaves
//!    directly into HHTL's three 16-bit tiers (`3 × 256×256` = 48 bits). Coarse
//!    zooms land in HEEL, fine in TWIG (`tier = level >> 3`); path distance is
//!    then 3 tier-table lookups — no lon/lat materialisation. This is the map
//!    pyramid and the semantic cascade being the *same* address (D-BOTHCASC).
//!
//! No external tile crate and no network on the request path: the handlers
//! return the tile **address + source URL + HHTL key**; a client fetches the
//! raster from the source. Pure math, fully unit-tested.

use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;
use std::f64::consts::PI;

/// The canonical OSM raster-tile source (standard slippy-map template). This is
/// the answer to "where OSM gets its maps from" — the tile is fetched from here
/// by the client; the cockpit only computes the address.
pub const OSM_TILE_URL: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

/// Native HHTL depth: 3 tiers × 8 levels = 24 quadtree levels (zoom 0..=24).
pub const HHTL_DEPTH: u32 = 24;

/// The concrete tile-source URL for a `z/x/y` address.
#[must_use]
pub fn tile_url(z: u32, x: u32, y: u32) -> String {
    OSM_TILE_URL
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
}

/// WebMercator (EPSG:3857) forward: geographic `(lon, lat)` → slippy tile
/// `(x, y)` at zoom `z`. `x` grows east, `y` grows south (TMS-flipped is the
/// caller's concern). Clamped to the valid `0..2^z` range.
#[must_use]
pub fn lonlat_to_tile(lon: f64, lat: f64, z: u32) -> (u32, u32) {
    let n = f64::from(2u32.saturating_pow(z));
    let x = ((lon + 180.0) / 360.0 * n).floor();
    let lat_rad = lat.to_radians();
    // asinh(tan φ) = ln(tan φ + sec φ) — the Mercator y, no PROJ needed.
    let merc_y = (lat_rad.tan() + 1.0 / lat_rad.cos()).ln();
    let y = ((1.0 - merc_y / PI) / 2.0 * n).floor();
    let max = (n - 1.0).max(0.0);
    (x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32)
}

/// Morton-interleave two 24-bit lanes into a 48-bit code: `x` bit `i` → code bit
/// `2i`, `y` bit `i` → code bit `2i+1`. Self-inverse with [`morton_deinterleave`].
#[must_use]
pub fn morton_interleave(x24: u32, y24: u32) -> u64 {
    let mut code = 0u64;
    for i in 0..HHTL_DEPTH {
        code |= (u64::from((x24 >> i) & 1)) << (2 * i);
        code |= (u64::from((y24 >> i) & 1)) << (2 * i + 1);
    }
    code
}

/// Inverse of [`morton_interleave`]: 48-bit code → `(x24, y24)`.
#[must_use]
pub fn morton_deinterleave(code: u64) -> (u32, u32) {
    let mut x = 0u32;
    let mut y = 0u32;
    for i in 0..HHTL_DEPTH {
        x |= (((code >> (2 * i)) & 1) as u32) << i;
        y |= (((code >> (2 * i + 1)) & 1) as u32) << i;
    }
    (x, y)
}

/// The three HHTL cascade tiers of a slippy tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Hhtl {
    /// Coarsest tier (the low-zoom neighbourhood).
    pub heel: u16,
    /// Middle tier (the palette tier in the HHTL legend).
    pub hip: u16,
    /// Finest tier (the leaf tile).
    pub twig: u16,
}

/// Map a slippy tile `z/x/y` onto its HHTL key. The tile is left-aligned to the
/// native 24-level depth so the **coarsest** quadtree bit lands in HEEL's high
/// bit (`tier = level >> 3`); the fine bits fall into TWIG.
///
/// A zoom **deeper** than the native depth resolves to its `z=24` **ancestor**
/// tile — the excess low bits of `x`/`y` are dropped (`x >> (z-24)`), NOT kept
/// in the low Morton lane. So two children of the same z=24 tile share an HHTL
/// key (a correct prefix), and the key is always a valid ancestor of the
/// requested tile. Beyond 24 native levels, depth is the hierarchy's job
/// (registry resolve / ref-escape, per the OGAR canon). [`resolved_tile`]
/// returns the same-granularity `z/x/y` the key actually encodes.
#[must_use]
pub fn tile_to_hhtl(z: u32, x: u32, y: u32) -> Hhtl {
    let (_, x, y) = resolved_tile(z, x, y);
    let z = z.min(HHTL_DEPTH);
    let shift = HHTL_DEPTH - z;
    let code = morton_interleave(x << shift, y << shift);
    Hhtl {
        heel: (code >> 32) as u16,
        hip: (code >> 16) as u16,
        twig: code as u16,
    }
}

/// The `z/x/y` the HHTL key actually encodes: identity for `z <= 24`, else the
/// `z=24` ancestor (`z=24`, `x >> (z-24)`, `y >> (z-24)`). Lets a handler report
/// a tile address that matches its own HHTL key instead of echoing an
/// over-depth `x/y` the key can't represent.
#[must_use]
pub fn resolved_tile(z: u32, x: u32, y: u32) -> (u32, u32, u32) {
    if z > HHTL_DEPTH {
        let excess = z - HHTL_DEPTH;
        (HHTL_DEPTH, x >> excess, y >> excess)
    } else {
        (z, x, y)
    }
}

// ── HTTP handlers ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LocateQuery {
    lon: f64,
    lat: f64,
    #[serde(default = "default_zoom")]
    z: u32,
}

fn default_zoom() -> u32 {
    14
}

/// `GET /api/osm/locate?lon=&lat=&z=` — geographic point → tile address + source
/// URL + HHTL key. The whole "where's the map, and what's its key" answer.
pub async fn osm_locate_handler(Query(q): Query<LocateQuery>) -> Json<serde_json::Value> {
    let z = q.z.min(HHTL_DEPTH);
    let (x, y) = lonlat_to_tile(q.lon, q.lat, z);
    let hhtl = tile_to_hhtl(z, x, y);
    Json(serde_json::json!({
        "lon": q.lon, "lat": q.lat, "z": z, "x": x, "y": y,
        "tile_url": tile_url(z, x, y),
        "source": OSM_TILE_URL,
        "hhtl": hhtl,
        "geo_domain": "0x0F",
    }))
}

/// `GET /api/osm/tile/:z/:x/:y` — a tile address → its source URL + HHTL key.
/// For an over-native-depth zoom (`z > 24`) the HHTL key encodes the `z=24`
/// ancestor; `resolved` names that ancestor explicitly so the key and the
/// address never silently describe different tiles.
pub async fn osm_tile_meta_handler(
    Path((z, x, y)): Path<(u32, u32, u32)>,
) -> Json<serde_json::Value> {
    let hhtl = tile_to_hhtl(z, x, y);
    let (rz, rx, ry) = resolved_tile(z, x, y);
    Json(serde_json::json!({
        "z": z, "x": x, "y": y,
        "tile_url": tile_url(z, x, y),
        "source": OSM_TILE_URL,
        "hhtl": hhtl,
        "resolved": { "z": rz, "x": rx, "y": ry },
        "geo_domain": "0x0F",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_url_fills_the_template() {
        assert_eq!(
            tile_url(14, 8749, 5677),
            "https://tile.openstreetmap.org/14/8749/5677.png"
        );
    }

    #[test]
    fn null_island_is_the_center_tile() {
        // lon=0, lat=0 at z=1 → the 2×2 grid's (1,1) corner (center of the world).
        assert_eq!(lonlat_to_tile(0.0, 0.0, 1), (1, 1));
        // z=0 → single tile (0,0).
        assert_eq!(lonlat_to_tile(0.0, 0.0, 0), (0, 0));
    }

    #[test]
    fn berlin_lands_in_the_expected_tile() {
        // Berlin (13.404954, 52.520008) at z=14 is the well-known (8802, 5373).
        assert_eq!(lonlat_to_tile(13.404954, 52.520008, 14), (8802, 5373));
    }

    #[test]
    fn tile_x_grows_east_y_grows_south() {
        let (x0, y0) = lonlat_to_tile(0.0, 0.0, 10);
        let (xe, _) = lonlat_to_tile(10.0, 0.0, 10);
        let (_, ys) = lonlat_to_tile(0.0, -10.0, 10);
        assert!(xe > x0, "east increases x");
        assert!(ys > y0, "south increases y");
    }

    #[test]
    fn morton_roundtrips() {
        for &(x, y) in &[(0u32, 0u32), (1, 0), (0, 1), (12345, 67890), (0xFFFFFF, 0xFFFFFF)] {
            assert_eq!(morton_deinterleave(morton_interleave(x, y)), (x, y));
        }
    }

    #[test]
    fn hhtl_roundtrips_to_the_tile() {
        // A tile at native depth (z=24) round-trips exactly through HHTL.
        let (z, x, y) = (24u32, 0x00AB_12u32, 0x00CD_34u32);
        let h = tile_to_hhtl(z, x, y);
        let code = (u64::from(h.heel) << 32) | (u64::from(h.hip) << 16) | u64::from(h.twig);
        assert_eq!(morton_deinterleave(code), (x, y));
    }

    #[test]
    fn coarse_zoom_lives_in_heel_not_twig() {
        // A low-zoom tile (z=2) left-aligns so its bits sit in HEEL; HIP/TWIG are 0.
        let h = tile_to_hhtl(2, 3, 1);
        assert_ne!(h.heel, 0, "coarse zoom must occupy HEEL");
        assert_eq!(h.hip, 0);
        assert_eq!(h.twig, 0);
    }

    #[test]
    fn over_depth_zoom_folds_to_its_native_ancestor() {
        // z=25 x=2 y=3 → its z=24 parent is x=1 y=1; the HHTL key + resolved
        // tile must agree, and NOT use the low-24-bit garbage the old code did.
        assert_eq!(resolved_tile(25, 2, 3), (24, 1, 1));
        assert_eq!(resolved_tile(26, 4, 8), (24, 1, 2));
        assert_eq!(tile_to_hhtl(25, 2, 3), tile_to_hhtl(24, 1, 1));
        // two children of the same z=24 parent share the key (a correct prefix).
        assert_eq!(tile_to_hhtl(25, 2, 2), tile_to_hhtl(25, 3, 3));
        // in-range zooms are untouched.
        assert_eq!(resolved_tile(14, 8802, 5373), (14, 8802, 5373));
    }

    #[test]
    fn neighbouring_tiles_share_a_heel_prefix() {
        // Two adjacent fine tiles differ only in the finest tier — the cascade
        // locality the HHTL address is for.
        let a = tile_to_hhtl(20, 100_000, 100_000);
        let b = tile_to_hhtl(20, 100_001, 100_000);
        assert_eq!(a.heel, b.heel, "adjacent tiles share the coarse HEEL tier");
    }
}
