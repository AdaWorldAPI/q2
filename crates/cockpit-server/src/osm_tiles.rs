//! OSM tile material — **where the maps come from**, and how a slippy tile
//! address becomes an HHTL (HEEL/HIP/TWIG/LEAF) key.
//!
//! Two halves, per `docs/MERCATOR-HHTL-HELIX-MAP.md` (OGAR canon "256×256
//! centroid tile", Geo domain `0x0F`, "OSM: literal x/y"):
//!
//! 1. **The source.** OSM raster tiles come from the standard slippy-map tile
//!    servers ([`OSM_TILE_URL`]). `z/x/y` is the WebMercator (EPSG:3857)
//!    quadtree — `2^z × 2^z` tiles at zoom `z`.
//! 2. **The address.** A quadtree IS a cascade, so `z/x/y` Morton-interleaves
//!    directly into HHTL's four 16-bit tiers (`4 × 256×256` = 64 bits) — the
//!    `GEO_V3_FACET` rails 0–3. Coarse zooms land in HEEL, fine in LEAF
//!    (`tier = level >> 3`); path distance is 4 tier-table lookups, no lon/lat
//!    materialisation. Map pyramid and semantic cascade are the *same* address
//!    (D-BOTHCASC) — and, since the V3 migration, literally the same key the
//!    baked slab is sorted by.
//!
//! No external tile crate and no network on the request path: the handlers
//! return the tile **address + source URL + HHTL key**; a client fetches the
//! raster from the source. Pure math, fully unit-tested.

use axum::Json;
use axum::extract::{Path, Query};
use serde::Deserialize;

/// The canonical OSM raster-tile source (standard slippy-map template). This is
/// the answer to "where OSM gets its maps from" — the tile is fetched from here
/// by the client; the cockpit only computes the address.
pub const OSM_TILE_URL: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

/// The satellite basemap source — ESRI World Imagery, the SAME keyless slippy
/// grid the DEM/skin bakes drape from (`scripts/fetch_iceland_dem.py`), so the
/// dual map (OSM ↔ satellite) and the ver-9 terrain skins share one imagery
/// truth. NOTE the path order: ESRI is **z/y/x** (row before column), unlike
/// the OSM z/x/y — the template placeholders encode that swap.
pub const SAT_TILE_URL: &str =
    "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}";

/// Native HHTL depth: **4 tiers × 8 levels = 32 quadtree levels** (zoom
/// 0..=32) — re-exported from the substrate rather than re-declared, so this
/// module cannot drift from the key the slab is actually sorted by.
///
/// This was `24` (3 tiers) until the V3 migration. The 3-tier form is not a
/// coarser variant of the same key, it is a *different* key: `osm_soa_bake`'s
/// own note records why it was rejected — at z=24 the exact-coordinate round
/// trip errs 0.27–1.69 m, "the same order as a GNSS fix", against 1.13 mm at
/// z=32. Measured divergence at Berlin was total, not marginal (every tier
/// differed; HEEL `0x624b` vs `0xc8e1`).
pub const HHTL_DEPTH: u32 = osm_soa_bake::tms::HHTL_DEPTH4;

/// Fill a slippy-tile URL template (`{z}`/`{x}`/`{y}`) for an address. The
/// template carries the axis order, so OSM (z/x/y) and ESRI (z/y/x) both fill
/// correctly through the same call.
fn fill_template(template: &str, z: u32, x: u32, y: u32) -> String {
    template
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
}

/// The concrete OSM tile-source URL for a `z/x/y` address.
#[must_use]
pub fn tile_url(z: u32, x: u32, y: u32) -> String {
    fill_template(OSM_TILE_URL, z, x, y)
}

/// The concrete SATELLITE (ESRI World Imagery) tile URL for the same `z/x/y`
/// address — the dual-map partner of [`tile_url`].
#[must_use]
pub fn sat_tile_url(z: u32, x: u32, y: u32) -> String {
    fill_template(SAT_TILE_URL, z, x, y)
}

/// WebMercator (EPSG:3857) forward: geographic `(lon, lat)` → slippy tile
/// `(x, y)` at zoom `z`. `x` grows east, `y` grows south — the XYZ convention;
/// the TMS flip is applied separately by [`tile_to_hhtl`].
///
/// Delegates to the substrate. This module used to carry its own copy of the
/// same formula; two implementations of one projection is exactly how the
/// display address and the row key drifted apart in the first place.
#[must_use]
pub fn lonlat_to_tile(lon: f64, lat: f64, z: u32) -> (u32, u32) {
    osm_soa_bake::tms::lonlat_to_tile(lon, lat, z)
}

/// Morton-interleave two lanes: `x` bit `i` → code bit `2i`, `y` bit `i` →
/// code bit `2i+1`. Self-inverse with [`morton_deinterleave`].
///
/// Delegates to the substrate's `morton64`, which interleaves the full 32-bit
/// lanes the 4-tier key needs.
#[must_use]
pub fn morton_interleave(x: u32, y: u32) -> u64 {
    osm_soa_bake::tms::morton64(x, y)
}

/// Inverse of [`morton_interleave`].
#[must_use]
pub fn morton_deinterleave(code: u64) -> (u32, u32) {
    osm_soa_bake::tms::demorton64(code)
}

/// The four HHTL cascade tiers of a slippy tile.
///
/// Four, not three: these are `GEO_V3_FACET` rails 0–3, each a `256×256`
/// centroid tile with x and y bound literally (the OGAR canon's "OSM: literal
/// x/y"). `leaf` is the tier the V1 key never had — and reading the bytes it
/// occupies as a tier, rather than as the head of the old `family:u24`, IS the
/// V3 content-blind reinterpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Hhtl {
    /// Coarsest tier (the low-zoom neighbourhood).
    pub heel: u16,
    /// Second tier (the palette tier in the HHTL legend).
    pub hip: u16,
    /// Third tier.
    pub twig: u16,
    /// Finest tier — the leaf tile.
    pub leaf: u16,
}

/// Map a slippy tile `z/x/y` onto its HHTL key — **the same key
/// `RowSlab::tile_range` sorts rows by**, so the address the cockpit displays
/// and the address the overlay reads under are one address.
///
/// Two steps the V1 form did not have, both load-bearing:
///
/// 1. **The TMS Y-flip.** OSM-XYZ counts `y` top-down; the substrate's key is
///    Cesium-TMS, counting bottom-up. Omitting it mirrors the world about the
///    equator — which still *looks* like a key, which is why it needs a test
///    against the oracle rather than an eyeball.
/// 2. **Flip at the tile's own zoom, then left-align.** `xyz_to_tms_y` is
///    `2^z - 1 - y`, so flipping at `z` and shifting by `32 - z` yields the
///    *minimum* TMS row of the tile's z=32 range — which is precisely the
///    common Morton prefix. Shifting first and flipping at 32 would pick the
///    opposite corner and break the prefix property.
///
/// A zoom deeper than the native depth resolves to its `z=32` **ancestor**
/// (excess low bits dropped, not kept in the low Morton lane), so children of
/// one z=32 tile share a key — a correct prefix, never a collision pretending
/// to be depth. [`resolved_tile`] names that ancestor explicitly.
#[must_use]
pub fn tile_to_hhtl(z: u32, x: u32, y: u32) -> Hhtl {
    let (z, x, y) = resolved_tile(z, x, y);
    let y_tms = osm_soa_bake::tms::xyz_to_tms_y(z, y);
    let shift = HHTL_DEPTH - z;
    let code = osm_soa_bake::tms::morton64(x << shift, y_tms << shift);
    let t = osm_soa_bake::tms::tiers_of(code);
    Hhtl {
        heel: t.heel,
        hip: t.hip,
        twig: t.twig,
        leaf: t.leaf,
    }
}

/// The `z/x/y` the HHTL key actually encodes: identity for `z <= 32`, else the
/// `z=32` ancestor (`z=32`, `x >> (z-32)`, `y >> (z-32)`). Lets a handler report
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
        // Dual map: the SAME address on the satellite basemap (ESRI World
        // Imagery, z/y/x) — one HHTL key, two skins.
        "sat_tile_url": sat_tile_url(z, x, y),
        "sat_source": SAT_TILE_URL,
        "hhtl": hhtl,
        "geo_domain": "0x0F",
    }))
}

/// `GET /api/osm/tile/:z/:x/:y` — a tile address → its source URL + HHTL key.
/// For an over-native-depth zoom (`z > 32`) the HHTL key encodes the `z=32`
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
        "sat_tile_url": sat_tile_url(z, x, y),
        "sat_source": SAT_TILE_URL,
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
    fn sat_tile_url_swaps_to_esri_z_y_x_order() {
        // Same address, satellite skin — and the ESRI path is z/y/x (row before
        // column), the classic trap the template encodes: y=5677 precedes x=8749.
        assert_eq!(
            sat_tile_url(14, 8749, 5677),
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/14/5677/8749"
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
        for &(x, y) in &[
            (0u32, 0u32),
            (1, 0),
            (0, 1),
            (12345, 67890),
            (0xFFFFFF, 0xFFFFFF),
        ] {
            assert_eq!(morton_deinterleave(morton_interleave(x, y)), (x, y));
        }
    }

    #[test]
    fn hhtl_roundtrips_to_the_tile() {
        // A tile at native depth (z=32) round-trips exactly through the FOUR
        // tiers. Re-pinned from the V1 form (z=24, three tiers, 48-bit code).
        //
        // The un-flip is the part worth reading: the key is TMS, the input is
        // XYZ, so the recovered row must be flipped back before comparing.
        // Asserting `== y` directly would fail; asserting the *raw* recovered
        // value equals some constant would pass against a key that mirrors the
        // world about the equator. Neither is what we want to pin.
        let (z, x, y) = (HHTL_DEPTH, 0x00AB_12u32, 0x00CD_34u32);
        let h = tile_to_hhtl(z, x, y);
        let code = (u64::from(h.heel) << 48)
            | (u64::from(h.hip) << 32)
            | (u64::from(h.twig) << 16)
            | u64::from(h.leaf);
        let (got_x, got_y_tms) = morton_deinterleave(code);
        assert_eq!(got_x, x, "x survives the round trip unflipped");
        assert_eq!(
            osm_soa_bake::tms::xyz_to_tms_y(z, got_y_tms),
            y,
            "un-flipping the TMS row recovers the XYZ row (the flip is self-inverse)"
        );
        assert_ne!(
            got_y_tms, y,
            "and the stored row is genuinely flipped — if this were equal, \
             tile_to_hhtl would not be applying the TMS flip at all"
        );
    }

    #[test]
    fn coarse_zoom_lives_in_heel_not_twig() {
        // A low-zoom tile (z=2) left-aligns so its bits sit in HEEL; HIP/TWIG are 0.
        let h = tile_to_hhtl(2, 3, 1);
        assert_ne!(h.heel, 0, "coarse zoom must occupy HEEL");
        assert_eq!(h.hip, 0);
        assert_eq!(h.twig, 0);
        assert_eq!(h.leaf, 0, "and the new finest tier is empty too");
    }

    #[test]
    fn over_depth_zoom_folds_to_its_native_ancestor() {
        // Re-pinned from the V1 depth (24) to the V3 native depth (32). The
        // shape of the claim is unchanged; only where "over-depth" begins moved.
        //
        // z=33 x=2 y=3 → its z=32 parent is x=1 y=1; the HHTL key + resolved
        // tile must agree, and NOT use the low-bit garbage the old code did.
        assert_eq!(resolved_tile(33, 2, 3), (HHTL_DEPTH, 1, 1));
        assert_eq!(resolved_tile(34, 4, 8), (HHTL_DEPTH, 1, 2));
        assert_eq!(tile_to_hhtl(33, 2, 3), tile_to_hhtl(HHTL_DEPTH, 1, 1));
        // two children of the same z=32 parent share the key (a correct prefix).
        assert_eq!(tile_to_hhtl(33, 2, 2), tile_to_hhtl(33, 3, 3));
        // in-range zooms are untouched — including z=25, which the V1 form
        // folded and the V3 form must not, since 25 <= 32.
        assert_eq!(resolved_tile(25, 2, 3), (25, 2, 3));
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

    /// **The V3 substrate falsifier.** The address this module displays and
    /// the key `RowSlab::tile_range` sorts rows by must be the SAME key.
    /// Otherwise the cockpit reports one address while the overlay reads rows
    /// under another, and nothing catches the drift.
    ///
    /// `osm_soa_bake::tms` is the oracle, not a second opinion: it is what the
    /// baked slab is literally keyed on (the Berlin bake reports `z=32 keying`
    /// and `classid 0x0F011000` = `CLASSVIEW_V3_SUBSTRATE`).
    ///
    /// Equality against an independent implementation, deliberately — a
    /// "returns four tiers" arity assertion would pass against a wrong key.
    #[test]
    fn hhtl_agrees_with_the_v3_substrate_oracle() {
        for (name, lon, lat) in [
            ("Berlin", 13.404954, 52.520008),
            ("Reykjavik", -21.940022, 64.146575),
            (
                "Sydney (southern — exercises the TMS Y-flip)",
                151.2093,
                -33.8688,
            ),
        ] {
            let (_code, want) = osm_soa_bake::tms::point_to_tiers(lon, lat);
            let (x, y) = lonlat_to_tile(lon, lat, HHTL_DEPTH);
            let got = tile_to_hhtl(HHTL_DEPTH, x, y);
            assert_eq!(got.heel, want.heel, "{name}: HEEL must equal the oracle");
            assert_eq!(got.hip, want.hip, "{name}: HIP must equal the oracle");
            assert_eq!(got.twig, want.twig, "{name}: TWIG must equal the oracle");
        }
    }
}
