//! Real OSM feature data — the query side of `openstreetmap-website-rs`'s
//! `RowSlab` Morton-sorted SoA bake.
//!
//! **Same key space as [`crate::osm_tiles`] — since the V3 migration, and not
//! before it.** That module now computes the 4-tier, TMS-Y-flipped, `z=32` key
//! (`GEO_V3_FACET` rails 0–3) by delegating to `osm_soa_bake::tms`, which is
//! exactly what `RowSlab::tile_range` sorts rows by. The cockpit's displayed
//! address and the row key are one address.
//!
//! That was NOT true originally, and the history is why the equality test
//! exists: `osm_tiles` carried its own 3-tier `z<=24` key with no TMS flip, a
//! parallel implementation that diverged from the slab at every tier (Berlin
//! HEEL `0x624b` vs `0xc8e1` — measured, not inferred). Two implementations of
//! one projection is how they drifted; `osm_tiles::hhtl_agrees_with_the_v3_
//! substrate_oracle` is what stops it recurring.
//!
//! This module still passes the raw OSM-XYZ `z/x/y` a slippy client sends
//! straight to `tile_range`, which applies the flip internally — so the
//! request path never round-trips through a key at all.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use lance_graph_contract::canonical_node::NodeRow;
use osm_soa_bake::identity::read_identity;
use osm_soa_bake::slab::RowSlab;
use osm_soa_bake::tms::morton_to_lonlat;
use serde::Serialize;
use std::sync::OnceLock;

/// Hard cap on rows serialized per request — a dense tile at a coarse zoom
/// can cover millions of rows; `total` still reports the true count so the
/// client can see it was truncated rather than silently reading a partial
/// tile as complete.
const MAX_FEATURES_PER_TILE: usize = 5_000;

static SLAB_MMAP: OnceLock<Option<memmap2::Mmap>> = OnceLock::new();

/// The baked slab, mmap'd once and cached for the process lifetime. `None`
/// when `OSM_SLAB_PATH` is unset or the file can't be opened/mapped — the
/// handler reports that as 503, not a panic.
fn open_slab() -> Option<&'static [u8]> {
    let mmap = SLAB_MMAP.get_or_init(|| {
        let path = std::env::var("OSM_SLAB_PATH").ok()?;
        let file = std::fs::File::open(&path).ok()?;
        // SAFETY: read-only mapping of a baked, immutable artifact. Nothing
        // else in this process (or expected to run alongside it) mutates the
        // file while it's mapped — same assumption `RowSlab`'s own docs make
        // about a Lance-returned buffer.
        unsafe { memmap2::Mmap::map(&file) }.ok()
    });
    mmap.as_ref().map(|m| &m[..])
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FeatureOut {
    pub lon: f64,
    pub lat: f64,
    /// `None` when the row's identity facet couldn't be read — either the
    /// backing buffer isn't 64-byte aligned (a real mmap always is; a
    /// synthetic test buffer may not be) or the row never had an identity
    /// written. Position is still reported either way: `tile_range` and
    /// `morton_at` are byte-level and never require alignment.
    pub entity_type: Option<u16>,
    pub ordinal: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TileFeaturesOut {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub total: usize,
    pub returned: usize,
    pub features: Vec<FeatureOut>,
}

/// Pure query over already-borrowed slab bytes: the real OSM rows covering
/// slippy tile `z/x/y`. Kept separate from the axum handler so it's testable
/// without HTTP plumbing.
fn query_tile(bytes: &[u8], z: u32, x: u32, y: u32) -> Result<TileFeaturesOut, String> {
    let slab = RowSlab::new(bytes).map_err(|e| format!("slab bytes not row-aligned: {e:?}"))?;
    let range = slab.tile_range(z, x, y);
    let total = range.len();

    // `rows()` needs 64-byte alignment; a real mmap is page-aligned (always a
    // multiple of 64) so this succeeds in production. Declining rather than
    // copying to force alignment is `RowSlab`'s own zero-copy contract — when
    // it declines, we still have every byte-level operation (`morton_at`),
    // so identity is the only thing that goes missing, never position.
    let rows: Option<&[NodeRow]> = slab.rows();

    let features = range
        .clone()
        .take(MAX_FEATURES_PER_TILE)
        .map(|i| {
            let (lon, lat) = morton_to_lonlat(slab.morton_at(i));
            let (entity_type, ordinal) = rows
                .and_then(|r| read_identity(&r[i]))
                .map(|(t, o)| (Some(t), Some(o)))
                .unwrap_or((None, None));
            FeatureOut {
                lon,
                lat,
                entity_type,
                ordinal,
            }
        })
        .collect::<Vec<_>>();

    Ok(TileFeaturesOut {
        z,
        x,
        y,
        total,
        returned: features.len(),
        features,
    })
}

/// `GET /api/osm/features/:z/:x/:y` — real OSM feature rows covering a
/// slippy tile, read from the baked `RowSlab` at `OSM_SLAB_PATH`.
pub async fn osm_features_handler(Path((z, x, y)): Path<(u32, u32, u32)>) -> Response {
    let Some(bytes) = open_slab() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "OSM_SLAB_PATH is not set or the baked slab could not be opened",
            })),
        )
            .into_response();
    };
    match query_tile(bytes, z, x, y) {
        Ok(out) => (StatusCode::OK, Json(out)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::canonical_node::NODE_ROW_STRIDE;
    use osm_soa_bake::tms::point_to_tms_morton;

    /// A synthetic slab with rows at exactly the given Morton codes, laid out
    /// the way the real baker writes them: 512-byte rows, sorted, with the
    /// four cascade tiers at byte offsets 4..12 (little-endian `u16`s) — the
    /// only bytes `tile_range`/`morton_at` read. No `Keyed`/`build_row`
    /// needed: those are `pub(crate)` inside `osm-soa-bake` and this is
    /// exactly the byte-level contract `RowSlab`'s own docs describe as the
    /// alignment-free lookup path.
    fn synthetic_slab(mortons: &[u64]) -> Vec<u8> {
        let mut sorted = mortons.to_vec();
        sorted.sort_unstable();
        let mut bytes = vec![0u8; sorted.len() * NODE_ROW_STRIDE];
        for (i, &code) in sorted.iter().enumerate() {
            let row = &mut bytes[i * NODE_ROW_STRIDE..(i + 1) * NODE_ROW_STRIDE];
            row[4..6].copy_from_slice(&((code >> 48) as u16).to_le_bytes());
            row[6..8].copy_from_slice(&((code >> 32) as u16).to_le_bytes());
            row[8..10].copy_from_slice(&((code >> 16) as u16).to_le_bytes());
            row[10..12].copy_from_slice(&(code as u16).to_le_bytes());
        }
        bytes
    }

    /// Anti-vacuity test (per the plan's Phase-1 requirement): two real,
    /// widely-separated points must resolve to disjoint, NON-EMPTY row
    /// ranges under their respective tiles — not "any query returns
    /// something" and not "everything collapses to one range". The z/x/y
    /// tile addresses are computed independently (via `crate::osm_tiles`,
    /// q2's own already-tested WebMercator math) from the Morton codes
    /// (via `osm-soa-bake`'s own `point_to_tms_morton`, a different
    /// implementation) — a broken `tile_range` (e.g. one that always
    /// returns the whole slab, or an empty range, or a swapped one) fails
    /// this; a correct one passes it for a structural reason, not by luck.
    #[test]
    fn adjacent_tiles_return_disjoint_nonempty_ranges() {
        let (berlin_lon, berlin_lat) = (13.404954, 52.520008);
        let (reyk_lon, reyk_lat) = (-21.940022, 64.146575);

        let berlin_code = point_to_tms_morton(berlin_lon, berlin_lat);
        let reyk_code = point_to_tms_morton(reyk_lon, reyk_lat);
        let bytes = synthetic_slab(&[berlin_code, berlin_code + 1, reyk_code, reyk_code + 1]);
        let slab = RowSlab::new(&bytes).expect("row-aligned synthetic buffer");

        let z = 6;
        let (bx, by) = crate::osm_tiles::lonlat_to_tile(berlin_lon, berlin_lat, z);
        let (rx, ry) = crate::osm_tiles::lonlat_to_tile(reyk_lon, reyk_lat, z);
        assert_ne!(
            (bx, by),
            (rx, ry),
            "fixture must land in different z={z} tiles for this test to mean anything"
        );

        let berlin_range = slab.tile_range(z, bx, by);
        let reyk_range = slab.tile_range(z, rx, ry);

        assert!(!berlin_range.is_empty(), "Berlin tile must contain the Berlin rows, got {berlin_range:?}");
        assert!(!reyk_range.is_empty(), "Reykjavik tile must contain the Reykjavik rows, got {reyk_range:?}");
        assert!(
            berlin_range.end <= reyk_range.start || reyk_range.end <= berlin_range.start,
            "neighboring tiles must return disjoint row ranges, got {berlin_range:?} vs {reyk_range:?}"
        );
    }

    /// The two-sided twin: querying the SAME tile the fixture rows were
    /// placed in must actually recover them (not just "disjoint from
    /// something else", but "correct in the positive case").
    #[test]
    fn query_tile_recovers_the_rows_actually_in_range() {
        let (lon, lat) = (13.404954, 52.520008);
        let code = point_to_tms_morton(lon, lat);
        let bytes = synthetic_slab(&[code]);

        let z = 6;
        let (x, y) = crate::osm_tiles::lonlat_to_tile(lon, lat, z);
        let out = query_tile(&bytes, z, x, y).expect("query succeeds over a valid slab");

        assert_eq!(out.total, 1, "exactly the one fixture row must fall in its own tile");
        assert_eq!(out.returned, 1);
        let got = &out.features[0];
        assert!(
            (got.lon - lon).abs() < 0.01 && (got.lat - lat).abs() < 0.01,
            "recovered position {:?} should be close to the fixture point ({lon}, {lat})",
            (got.lon, got.lat)
        );
    }

    #[test]
    fn empty_slab_reports_total_zero_not_an_error() {
        let out = query_tile(&[], 6, 0, 0).expect("an empty slab is a valid (empty) query");
        assert_eq!(out.total, 0);
        assert_eq!(out.returned, 0);
        assert!(out.features.is_empty());
    }
}
