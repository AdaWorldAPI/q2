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

/// Zoom at or above which a tile is a **place you are looking at**, and must
/// therefore be answered COMPLETE — never decimated.
///
/// This exists because the previous design had no such notion. A single flat
/// `MAX_FEATURES_PER_TILE = 5_000` was applied at every zoom, and measured
/// against the Berlin bake that is not a coarse-zoom backstop at all — it is
/// the normal case. From the one tile actually measured (`14/8802/5373`,
/// `total = 15_016`), and a slippy tile quartering in area per zoom step:
///
/// | z | rows/tile (Berlin-class) | under the old 5k cap? |
/// |---|---|---|
/// | 12 | ~240,000 | no — 98% dropped |
/// | 13 | ~60,000 | no — 92% dropped |
/// | **14** | **15,016 (measured)** | **no — 67% dropped** |
/// | 15 | ~3,800 | yes |
///
/// So *one mid-sized city* was silently served two-thirds absent at the zoom
/// where a user actually reads a city. Decimating an overland survey is a
/// legitimate LOD choice; decimating a city is a wrong map. z13 is the
/// slippy-conventional city/district floor (z<=12 reads as metro-and-wider).
///
/// NOT re-measured against a denser bake than Berlin — see
/// `a_city_zoom_tile_is_served_complete`, which fails if this ever regresses.
const CITY_ZOOM_FLOOR: u32 = 13;

/// Decimation target for **overview** zooms (`z < CITY_ZOOM_FLOOR`) only,
/// where a tile covers more ground than a screen can draw and thinning is the
/// honest answer. Grounded in the one render capacity actually measured in
/// this repo: the browser run drew **177,963** markers across 49 tiles with
/// zero page errors, so ~10^5 per response is within demonstrated reach.
///
/// **CONJECTURE — the VALUE is measured, the METHOD is not.** Probe M4
/// (`bf16-hhtl-terrain.md`, and its process rule that a bucketing-strategy
/// change runs the probe first) was run for this: `osm-soa-bake`'s
/// `tier_probe`, on Berlin (city, 2.52M features) and Iceland (overland,
/// 0.65M), features per tile —
///
/// | tier | Berlin med / p95 / max | Iceland med / p95 / max |
/// |---|---|---|
/// | heel z8 | 1,564,647 (2 tiles) | 3,838 / 34,985 / 202,296 |
/// | hip z16 | 206 / 996 / 3,844 | 1 / 8 / 1,067 |
/// | twig z24 | 1 / 1 / 20 | 1 / 1 / 7 |
/// | leaf z32 | 1 / 1 / 11 | 1 / 1 / 7 |
///
/// M4's own gate was ">60% terminates at HEEL = pass, >60% at LEAF = fail".
/// Berlin puts **99.7% of tiles at one feature by TWIG** — the fail direction.
/// Consequence for decimation: there is no coarse bucket to compress *into*
/// below hip, and above it occupancy jumps 200x. So the principled overview
/// rule is one representative per **occupied hip cell** (312:1 on Berlin), not
/// a uniform row stride. The stride is a placeholder that produces a
/// defensible picture at a measured budget; it is not the cascade answer, and
/// it is labelled CONJECTURE until the cell-bucketing form is built and
/// compared. This affects overview zooms ONLY — city zooms are complete and
/// never reach here.
const OVERVIEW_ROW_BUDGET: usize = 100_000;

/// Transport backstop for city zooms — sized so it provably cannot fire for a
/// Berlin-class bake, rather than picked to look safe. A z13 tile is 8x8 = 64
/// hip (z16) tiles, and the densest measured hip tile in Berlin holds 3,844
/// features, so a z13 tile is bounded above by 64 x 3,844 = **246,016** — and
/// that bound is itself unreachable (it assumes 64 adjacent maximum-density
/// tiles; the one z13-adjacent tile actually measured, `14/8802/5373`, holds
/// 15,016). This exists only so a pathological request cannot allocate without
/// bound; if it ever *does* fire, `returned < total` reports it.
const CITY_ROW_CEILING: usize = 400_000;

/// Rows this response may serialize at zoom `z`.
///
/// The budget is zoom-conditioned because "how many features may I drop" is an
/// LOD question, and LOD is a function of what the tile *is* — not a constant.
fn row_budget(z: u32) -> usize {
    if z >= CITY_ZOOM_FLOOR {
        CITY_ROW_CEILING
    } else {
        OVERVIEW_ROW_BUDGET
    }
}

/// Take every `stride`-th row so a decimated tile stays spatially
/// representative. Split out from `query_tile` so the *selection* rule can be
/// falsified at any budget, without a fixture the size of a real overview tile.
fn stride_for(total: usize, budget: usize) -> usize {
    total.div_ceil(budget).max(1)
}

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

    // TWO separate defects were fixed here, and conflating them is how the
    // first fix hid the second.
    //
    // (1) HOW rows were dropped. `range` is Morton-ordered, so a plain prefix
    //     (`take(cap)`) returns a spatially contiguous sub-quadrant, not a
    //     sample. Measured live: tile 14/8802/5373 (5,000 of 15,016 rows)
    //     covered 99.9% of the tile's width but only 50.0% of its height,
    //     while a control tile under the cap covered 100%/100%. Walking every
    //     `stride`-th row spreads the sample across the whole curve instead.
    //
    // (2) THAT rows were dropped at all, at city zoom. Fixing (1) makes the
    //     loss uniform, which looks better and is still a wrong map: the same
    //     tile was two-thirds absent either way. A stride sample covers ~100%
    //     of a tile's EXTENT at any stride — so extent coverage cannot detect
    //     over-decimation, and a falsifier written on it certifies (2) as
    //     fine. `row_budget` is what actually fixes (2); the test that can see
    //     it is `a_city_zoom_tile_is_served_complete`, which counts rows.
    //
    // `div_ceil(...).max(1)`:
    //   * total == 0       -> div_ceil is 0, `.max(1)` guards `step_by`'s
    //                         "step must be non-zero" panic;
    //   * total <= budget  -> exactly 1, so EVERY row is returned;
    //   * total > budget   -> stride = ceil(total/budget) bounds `returned` at
    //                         the budget, since total <= stride*budget.
    let stride = stride_for(total, row_budget(z));

    let features = range
        .clone()
        .step_by(stride)
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

    /// **The completeness falsifier — the one that can see over-decimation.**
    ///
    /// A city-zoom tile must return EVERY row it contains. This is the test
    /// the previous pass did not have, and its absence is why a real defect
    /// shipped looking fixed.
    ///
    /// What went wrong, recorded so it is not repeated: the first fix replaced
    /// head-truncation with stride sampling and asserted the result covered
    /// >=95% of the tile's EXTENT. That assertion is unfalsifiable for this
    /// defect. A uniform stride covers ~100% of a bounding box at ANY stride —
    /// measured on this very fixture shape, the >=0.95 assertion passes while
    /// keeping 25% of rows, and still passes while keeping 5.9%. It certified
    /// a two-thirds-empty city as correct. Counting rows is what discriminates;
    /// measuring their bounding box is not.
    ///
    /// Two-sided by construction: the fixture is deliberately larger than the
    /// old flat 5,000 cap, so this test FAILS against the pre-`row_budget`
    /// code (which would return 5,000 of 10,000). If a future change shrinks
    /// the fixture below that, the `total > 5_000` assertion fires instead of
    /// the test going quietly vacuous.
    #[test]
    fn a_city_zoom_tile_is_served_complete() {
        const OLD_FLAT_CAP: usize = 5_000;

        let z = 14; // >= CITY_ZOOM_FLOOR
        // Placed INSIDE tile 14/8801/5374, whose bounds are lon
        // [13.381348, 13.403320] x lat [52.496160, 52.509535]. A z=14 tile is
        // only ~0.0134 deg tall at this latitude (Mercator lat tiles are not
        // uniform), so the step is sized to span ~60% of the SHORTER axis; a
        // first attempt at 0.0001 straddled the boundary and the containment
        // guard below caught it.
        let (base_lon, base_lat, step) = (13.3857_f64, 52.4988_f64, 0.00008_f64);
        let n = 100usize; // 10,000 rows — twice the old flat cap

        let mut mortons = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                mortons.push(point_to_tms_morton(
                    base_lon + i as f64 * step,
                    base_lat + j as f64 * step,
                ));
            }
        }
        let bytes = synthetic_slab(&mortons);

        let (x, y) = crate::osm_tiles::lonlat_to_tile(base_lon, base_lat, z);
        let (fx, fy) = crate::osm_tiles::lonlat_to_tile(
            base_lon + (n - 1) as f64 * step,
            base_lat + (n - 1) as f64 * step,
            z,
        );
        assert_eq!(
            (x, y),
            (fx, fy),
            "fixture must sit in ONE z={z} tile to mean anything"
        );

        let out = query_tile(&bytes, z, x, y).expect("query succeeds over a valid slab");

        assert_eq!(
            out.total,
            n * n,
            "every fixture row must land in this one tile"
        );
        assert!(
            out.total > OLD_FLAT_CAP,
            "fixture ({} rows) must exceed the old flat cap ({OLD_FLAT_CAP}) or this \
             test cannot tell the fix from the defect",
            out.total
        );
        assert_eq!(
            out.returned, out.total,
            "a city-zoom tile must be served COMPLETE — {} of {} rows is a wrong map, \
             however evenly the missing ones are spread",
            out.returned, out.total
        );
    }

    /// The budget must actually be zoom-conditioned. Can-fire and can-stay-
    /// silent on the same knob: a constant `row_budget` — the shape of the
    /// original defect — fails the first assertion.
    #[test]
    fn row_budget_is_zoom_conditioned_at_the_city_floor() {
        assert!(
            row_budget(CITY_ZOOM_FLOOR) > row_budget(CITY_ZOOM_FLOOR - 1),
            "city zoom must get a larger budget than overview zoom, else the \
             distinction is decorative"
        );
        assert_eq!(
            row_budget(CITY_ZOOM_FLOOR),
            row_budget(CITY_ZOOM_FLOOR + 5),
            "the budget must not drift ABOVE the floor — every city zoom is complete"
        );
        assert_eq!(
            row_budget(0),
            row_budget(CITY_ZOOM_FLOOR - 1),
            "every overview zoom shares one budget"
        );
    }

    /// **The spatial-bias falsifier**, now scoped to what it can actually
    /// prove: that decimation — where it IS legitimate (overview zoom) —
    /// samples the whole Morton curve rather than returning a contiguous
    /// prefix quadrant.
    ///
    /// Reproduces the mechanism measured live against the real Berlin slab:
    /// tile 14/8802/5373 (5,000 of 15,016 rows) covered 99.9% of the tile's
    /// width but only 50.0% of its height, because `tile_range` is
    /// Morton-ordered and `take(cap)` returns one recursive quadrant of the
    /// Z-order curve.
    ///
    /// Exercises `stride_for` at an injected small budget rather than
    /// `query_tile` at the real `OVERVIEW_ROW_BUDGET`, because a fixture large
    /// enough to trip 100,000 rows would be a ~50 MB unit test. The budget
    /// CONSTANT is falsified by `a_city_zoom_tile_is_served_complete` and
    /// `row_budget_is_zoom_conditioned_at_the_city_floor`; this test owns the
    /// selection RULE.
    ///
    /// Two-sided, and the first half is the point: it recomputes what the old
    /// `take(budget)` would have returned from the same range and fails if that
    /// prefix is NOT measurably bad here. Without it, a degenerate fixture
    /// would pass silently. Note the coverage assertion alone is NOT evidence
    /// of completeness — see `a_city_zoom_tile_is_served_complete`.
    #[test]
    fn a_decimated_overview_tile_samples_the_whole_curve_not_a_morton_prefix() {
        let z = 6;
        let (base_lon, base_lat, step) = (13.40_f64, 52.50_f64, 0.0005_f64);
        // 130x130 = 16,900 rows against an injected budget of 4,225 -> stride 4.
        // Deliberately coarser than the minimum stride 2: a barely-over-budget
        // fixture is the EASIEST case for coverage, hence the weakest falsifier.
        let n = 130usize;
        let budget = 4_225usize;

        let mut mortons = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                mortons.push(point_to_tms_morton(
                    base_lon + i as f64 * step,
                    base_lat + j as f64 * step,
                ));
            }
        }
        let bytes = synthetic_slab(&mortons);
        let slab = RowSlab::new(&bytes).expect("row-aligned synthetic buffer");

        let (x, y) = crate::osm_tiles::lonlat_to_tile(base_lon, base_lat, z);
        let (fx, fy) = crate::osm_tiles::lonlat_to_tile(
            base_lon + (n - 1) as f64 * step,
            base_lat + (n - 1) as f64 * step,
            z,
        );
        assert_eq!(
            (x, y),
            (fx, fy),
            "fixture must sit in ONE z={z} tile to mean anything"
        );

        let range = slab.tile_range(z, x, y);
        assert_eq!(
            range.len(),
            n * n,
            "every fixture row must land in its own tile"
        );
        assert!(
            range.len() > budget,
            "fixture must exceed the injected budget"
        );

        let span = (n - 1) as f64 * step;
        let coverage = |pts: &[(f64, f64)]| -> (f64, f64) {
            let (lo_x, hi_x) = pts
                .iter()
                .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
            let (lo_y, hi_y) = pts
                .iter()
                .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
            ((hi_x - lo_x) / span, (hi_y - lo_y) / span)
        };

        // (1) What head-truncation returned — the disable-the-fix control.
        let prefix: Vec<(f64, f64)> = range
            .clone()
            .take(budget)
            .map(|i| morton_to_lonlat(slab.morton_at(i)))
            .collect();
        let (p_lon, p_lat) = coverage(&prefix);

        // (2) What the current selection rule returns.
        let stride = stride_for(range.len(), budget);
        let got: Vec<(f64, f64)> = range
            .clone()
            .step_by(stride)
            .map(|i| morton_to_lonlat(slab.morton_at(i)))
            .collect();
        let (g_lon, g_lat) = coverage(&got);

        eprintln!(
            "PREFIX coverage lon={p_lon:.4} lat={p_lat:.4} (n={})",
            prefix.len()
        );
        eprintln!(
            "STRIDE coverage lon={g_lon:.4} lat={g_lat:.4} (n={}, stride={stride})",
            got.len()
        );

        assert!(
            got.len() <= budget,
            "the budget must still bound the decimated response"
        );
        assert!(
            p_lon < 0.7 || p_lat < 0.7,
            "fixture is not discriminating: the naive prefix already covers >=70% of \
             both axes (lon={p_lon:.4} lat={p_lat:.4}) — this test could not tell \
             stride-sampling apart from truncation"
        );
        assert!(
            g_lon >= 0.95 && g_lat >= 0.95,
            "stride-sampled tile must cover nearly the full extent on BOTH axes, got \
             lon={g_lon:.4} lat={g_lat:.4} (naive prefix: lon={p_lon:.4} lat={p_lat:.4})"
        );
    }
}
