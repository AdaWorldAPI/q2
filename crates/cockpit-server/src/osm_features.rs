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

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lance_graph_contract::canonical_node::NodeRow;
use osm_soa_bake::identity::read_identity;
use osm_soa_bake::slab::RowSlab;
use osm_soa_bake::tms::morton_to_lonlat;
use serde::Serialize;
use std::ops::Range;
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
/// Probe M4 (`bf16-hhtl-terrain.md`, and its process rule that a
/// bucketing-strategy change runs the probe first) was run for this:
/// `osm-soa-bake`'s `tier_probe`, on Berlin (city, 2.52M features) and Iceland
/// (overland, 0.65M), features per tile —
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
/// Consequence: there is no coarse bucket to compress *into* below hip, and
/// above it occupancy jumps 200x, so the cascade has exactly one useful
/// bucketing level and a uniform row stride is not it.
///
/// **The METHOD is no longer conjecture — it was built and measured.** The
/// stride is gone; `overview_sample` selects one representative per occupied
/// cell at a per-tile depth. Measured against the real Berlin bake by
/// `overview_rule_comparison_on_the_real_bake`, counting **singleton hip
/// cells** (a row alone in its z16 cell — an isolated feature by M4's own
/// measure):
///
/// | tile | rows | stride keeps | cell keeps | stride extent | cell extent |
/// |---|---|---|---|---|---|
/// | 8/137/83 | 1,569,355 | **14 / 316** | **316 / 316** | 0.82 / 0.93 | 1.00 / 1.00 |
/// | 9/275/167 | 981,698 | 9 / 107 | 107 / 107 | 0.82 / 0.87 | 1.00 / 1.00 |
/// | 10/550/335 | 981,696 | 9 / 105 | 105 / 105 | 0.95 / 0.97 | 1.00 / 1.00 |
///
/// The stride drops **95.6%** of isolated features at z8 and the cell form
/// drops none — while returning FEWER rows (53,655 vs 98,085), because the
/// depth is quantized to a zoom level and the next one deeper would overrun.
/// Note the extent column moves 0.82 -> 1.00 where the real metric moves
/// 14 -> 316: extent coverage could not see this, which is why it was the
/// wrong gate. z11/z12 have zero singletons (those tiles are entirely inside
/// dense Berlin) and both rules tie there — the can-stay-silent half, free.
///
/// **The VALUE was wrong, and the browser is what caught it.** It was
/// `100_000`, justified as "the browser drew 177,963 markers with zero page
/// errors, so ~10^5 per response is within reach". That reads a **viewport-
/// wide** measurement as a **per-tile** budget. 177,963 was the total across
/// **49 tiles** — 3,632 per tile — so the constant overstated its own evidence
/// by 27x. Measured at the `/osm` page's default z12 view (1400x900 = 63
/// tiles, 53 with data):
///
/// | | markers in view |
/// |---|---|
/// | ever measured working | 177,963 |
/// | under a 100k per-tile budget | **1,772,260** (10x) |
///
/// The page hung. `render()` rebuilds every marker on each arriving tile, so
/// that is ~53 rebuilds of a 1.77M-node DOM. (The quadratic is a real
/// pre-existing defect — recorded in the plan — but it was survivable at the
/// load the evidence actually supports, and is not what made this wrong.)
///
/// So the budget is stated per tile with the viewport arithmetic shown:
/// 3,000 x 53 data tiles = 159,000 in view, under the 177,963 that is the only
/// browser capacity ever measured here. A larger window holds more tiles and
/// scales that up — the honest bound is per VIEWPORT and this constant is the
/// per-tile share of it.
///
/// A smaller budget costs far less here than it would have under a stride:
/// cell selection spends its budget on *distinct places* rather than in
/// proportion to density, which is the whole point of the comparison above.
/// Overview zooms ONLY — city zooms are complete and never decimate.
const OVERVIEW_ROW_BUDGET: usize = 3_000;

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

/// The `bits`-wide Morton prefix of `code` — the cascade cell it falls in.
///
/// One slippy zoom level is **two** Morton bits (one x, one y), so zoom `zz`
/// is `bits = 2 * zz`. `bits == 0` is the whole world (one cell); `bits == 64`
/// is the full key, so the shift never reaches 64 and never overflows.
fn cell_prefix(code: u64, bits: u32) -> u64 {
    if bits == 0 { 0 } else { code >> (64 - bits) }
}

/// Occupied cells at cascade depth `bits` within a Morton-sorted index range.
///
/// The range is sorted, so rows sharing a prefix are **contiguous** — counting
/// distinct prefixes is one pass with no map and no allocation. Monotone
/// non-decreasing in `bits` (a finer prefix can only split a run, never merge
/// two), which is what makes `choose_cell_zoom`'s binary search valid; pinned
/// by `occupied_cells_is_monotone_in_depth`.
fn occupied_cells(range: Range<usize>, morton: impl Fn(usize) -> u64, bits: u32) -> usize {
    let mut cells = 0usize;
    let mut prev: Option<u64> = None;
    for i in range {
        let p = cell_prefix(morton(i), bits);
        if prev != Some(p) {
            cells += 1;
            prev = Some(p);
        }
    }
    cells
}

/// The deepest slippy zoom whose occupied-cell count still fits `budget`.
///
/// This is the "dynamic compression bucket threshold": the cascade depth is
/// chosen **per tile from its own density**, not fixed at a tier. M4 measured
/// why that matters — Berlin and Iceland differ ~200x in features per hip cell
/// and converge only by twig, so any depth picked once for all extracts is
/// wrong for one of them.
fn choose_cell_zoom(
    z: u32,
    range: Range<usize>,
    morton: impl Fn(usize) -> u64 + Copy,
    budget: usize,
) -> u32 {
    // Below the tile's own zoom every row is one cell — nothing to choose.
    let (mut lo, mut hi) = (z, HHTL_ZOOM_MAX);
    if occupied_cells(range.clone(), morton, hi * 2) <= budget {
        return hi;
    }
    // Invariant: `lo` fits, `hi` does not. Converge on the deepest that fits.
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if occupied_cells(range.clone(), morton, mid * 2) <= budget {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The bake's native cascade depth — 4 tiers x 16 Morton bits = z32.
const HHTL_ZOOM_MAX: u32 = 32;

/// **The overview selection rule**: one representative per occupied cell, at
/// the deepest cascade depth that fits the budget.
///
/// Contrast with a uniform row stride, which samples the Morton curve at even
/// *index* spacing. Index spacing is not spatial spacing: a stride keeps
/// features in proportion to local density, so it preserves how crowded a
/// place looks and **drops isolated features entirely** — a lone village
/// surrounded by empty country is one row among a city's hundred thousand and
/// survives only if its index happens to land on the stride. Cell selection
/// inverts that trade: every occupied cell contributes exactly one row, so
/// isolated features always survive and dense ones are flattened toward a
/// uniform spatial sample. For a MAP, "somewhere has data" is the load-bearing
/// claim and "how much" is not, which is why this is the cascade answer.
/// Measured comparison: `examples/osm_overview_sample.rs`.
fn overview_sample(
    z: u32,
    range: Range<usize>,
    morton: impl Fn(usize) -> u64 + Copy,
    budget: usize,
) -> Vec<usize> {
    if range.len() <= budget {
        return range.collect();
    }
    let bits = choose_cell_zoom(z, range.clone(), morton, budget) * 2;
    let mut out = Vec::new();
    let mut prev: Option<u64> = None;
    for i in range {
        let p = cell_prefix(morton(i), bits);
        if prev != Some(p) {
            out.push(i);
            prev = Some(p);
        }
    }
    out
}

type SlabMmap = (memmap2::Mmap, usize, usize);
static SLAB_MMAP: OnceLock<Option<SlabMmap>> = OnceLock::new();

/// The baked slab, mmap'd once and cached for the process lifetime. `None`
/// when no row-byte source resolves — the handler reports that as 503, not
/// a panic.
///
/// # The serving contract: hydrate once → sink into Lance once → mmap the
/// # `.lance` bytes lazily from disk (operator-directed)
///
/// Two sources, in priority order:
///
/// 1. `OSM_SLAB_ROW_FILE` + `OSM_SLAB_ROW_OFFSET` + `OSM_SLAB_ROW_LEN` —
///    the Lance dataset's own sole data file, set at boot by
///    [`crate::osm_lance::locate_row_column`] only after FOUR verifications
///    (sole fragment, aligned start, head anchor, tail anchor — the same
///    assertions lance-graph's `soa_verbatim.rs` test makes; see that
///    module's doc for the outage the unverified version caused). The mmap
///    slice is bounded to EXACTLY `offset..offset + len`; the pages load
///    lazily through the page cache — nothing slab-sized is read into heap.
/// 2. `OSM_SLAB_PATH` — the raw `.soa` file, unconditionally set by
///    `osm_slab_hydrate::ensure_slab_local`. `OSM_SLAB_PATH` is NEVER
///    repointed at the Lance file: it also anchors the `.books`/`.chains`
///    sidecar paths ([`open_books`], [`open_chains`]), which live beside
///    the raw bake, not inside the Lance dataset's `data/` directory.
///
/// Strictness over clamping: the Lance-file branch uses exact bounds
/// (`get(off..off + len)`), and ANY failure — unset/garbled vars, a file
/// that shrank, a slice that isn't whole rows — falls back to the raw
/// `.soa` path rather than serving a truncated slice. The pre-fix version
/// clamped (`end.min(m.len())`) and served a footer-polluted slice whose
/// length wasn't a multiple of `NODE_ROW_STRIDE`; `RowSlab::new` then
/// rejected every request.
fn open_slab() -> Option<&'static [u8]> {
    fn map_range(path: &str, off: usize, len: usize) -> Option<(memmap2::Mmap, usize, usize)> {
        if len == 0 || !len.is_multiple_of(lance_graph_contract::canonical_node::NODE_ROW_STRIDE) {
            return None;
        }
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: read-only mapping of a baked, immutable artifact. Nothing
        // else in this process (or expected to run alongside it) mutates the
        // file while it's mapped — same assumption `RowSlab`'s own docs make
        // about a Lance-returned buffer.
        let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        // Exact bounds — a shrunken or wrong file declines, never clamps.
        off.checked_add(len).filter(|&end| end <= mmap.len())?;
        Some((mmap, off, len))
    }

    let cell = SLAB_MMAP.get_or_init(|| {
        let lance = || -> Option<(memmap2::Mmap, usize, usize)> {
            let path = std::env::var("OSM_SLAB_ROW_FILE").ok()?;
            let off: usize = std::env::var("OSM_SLAB_ROW_OFFSET").ok()?.parse().ok()?;
            let len: usize = std::env::var("OSM_SLAB_ROW_LEN").ok()?.parse().ok()?;
            map_range(&path, off, len)
        };
        let raw = || -> Option<(memmap2::Mmap, usize, usize)> {
            let path = std::env::var("OSM_SLAB_PATH").ok()?;
            let file = std::fs::File::open(&path).ok()?;
            // SAFETY: as above.
            let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
            let len = mmap.len();
            Some((mmap, 0, len))
        };
        lance().or_else(raw)
    });
    cell.as_ref()
        .and_then(|(m, off, len)| m.get(*off..*off + *len))
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
    /// This feature's row index in the slab — the handle `/api/osm/feature/:idx`
    /// takes to answer "what IS this dot?".
    ///
    /// Carried per feature rather than serving tags inline because a tile can
    /// return 15,016 features and each carries up to `TAGS_PER_ROW` tags: the
    /// inline form would multiply an already-large response by the tag fan-out,
    /// for data a viewer wants about ONE dot at a time. An index is 4 bytes and
    /// makes the detail a click, not a download.
    pub idx: usize,
}

/// One feature's resolved identity and tags — the answer to clicking a dot.
#[derive(Debug, Serialize, PartialEq)]
pub struct FeatureDetailOut {
    pub idx: usize,
    pub lon: f64,
    pub lat: f64,
    pub entity_type: Option<u16>,
    pub ordinal: Option<u32>,
    /// The element's OSM identity as the bake keyed it: `"{kind:04x}:{osm_id}"`
    /// resolved through `Books::identities`. `None` when the codebook sidecar
    /// is absent or the ordinal is not in it.
    pub osm_key: Option<String>,
    /// `k=v` tags, resolved from ordinals through `Books::{tag_keys,tag_values}`.
    /// Empty (not absent) when the row carries none — an untagged node is a
    /// real answer, not a failure.
    pub tags: std::collections::BTreeMap<String, String>,
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
    // (3) WHICH rows, once some must go. A stride keeps features in proportion
    //     to local density, so it drops ISOLATED ones — measured on a fixture
    //     of one dense cluster plus 8 outliers, a stride kept 1 of 8 while
    //     cell selection kept 8 of 8. On a map that is the worst thing to
    //     lose: a lone village in empty country is what a viewer is looking
    //     for, and its absence reads as "nothing is there".
    //
    // `overview_sample` is the whole rule and needs no zoom branch of its own:
    // when `total <= row_budget(z)` it returns the range untouched, which is
    // what makes a city tile complete (`row_budget` is the ceiling there), and
    // it only reaches the cascade when a tile genuinely overruns its budget.
    let selected = overview_sample(z, range.clone(), |i| slab.morton_at(i), row_budget(z));

    let features = selected
        .into_iter()
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
                idx: i,
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

/// The codebook sidecar, read once. `None` when the slab path is unset or the
/// `.books` file is missing/unreadable — tags then resolve to nothing and the
/// detail endpoint still answers with position and ordinals, which is strictly
/// more than the tile response carries.
///
/// Path is derived, not configured: `with_extension("books")` turns both the
/// hydrated `berlin.soa` and a locally-baked extensionless `berlin` into
/// `berlin.books`, so the two naming conventions in play both resolve without
/// a second env var to keep in sync with the first.
static BOOKS: OnceLock<Option<osm_soa_bake::codebook::Books>> = OnceLock::new();

/// Resolve `<slab-stem>.<ext>` from `OSM_SLAB_PATH`.
///
/// Split out so the "no path" arm *says so*. Both sidecar openers used to
/// spell this `std::env::var("OSM_SLAB_PATH").ok()?` — a silent `None` — and
/// that silence is what made a missing codebook so expensive to diagnose:
/// every other failure in this file names itself, but the two that fire on a
/// misconfigured deploy did not.
fn sidecar_path(ext: &str) -> Option<std::path::PathBuf> {
    match std::env::var("OSM_SLAB_PATH") {
        Ok(slab) => Some(std::path::PathBuf::from(slab).with_extension(ext)),
        Err(_) => {
            // Not an error, and deliberately not a warning: local dev and any
            // deploy without an OSM bake runs this way by design, and the
            // endpoints already answer 503. It is logged so "unset" and
            // "present but unreadable" are never confused for each other.
            tracing::debug!(
                ext,
                "osm sidecar: OSM_SLAB_PATH unset — no OSM bake configured"
            );
            None
        }
    }
}

fn open_books() -> Option<&'static osm_soa_bake::codebook::Books> {
    BOOKS
        .get_or_init(|| {
            let path = sidecar_path("books")?;
            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                // THE grey-map cause, and it used to be invisible. Without the
                // codebook `class_for_tags` never runs, every shape carries
                // `ShapeClass::Other` (`fill:'none'`, a thin grey stroke), and
                // feature clicks resolve no tags — a correct-looking render of
                // correct geometry with all the meaning stripped out. Nothing
                // 500s and nothing 503s, so this line is the only thing that
                // can tell an operator why the map went grey.
                Err(e) => {
                    tracing::error!(
                        path = %path.display(), error = %e,
                        "osm books: codebook ABSENT next to the slab — every feature falls back \
                         to ShapeClass::Other, so the map draws grey/untagged. Ship \
                         <slab-stem>.books from the SAME bake run as the slab and .chains"
                    );
                    return None;
                }
            };
            let mut r = std::io::BufReader::new(file);
            match osm_soa_bake::codebook::read_books(&mut r) {
                Ok((header, books)) => {
                    // The header's `slab` is a `hash_slab` of the rows the
                    // codebook was built from — the SAME cross-bake pin
                    // `.chains` enforces, and it was being discarded here.
                    // Accepting a foreign codebook is worse than rejecting it:
                    // every ordinal resolves to some OTHER bake's string, so
                    // the map answers confidently and wrongly.
                    let want = slab_digest()?;
                    if header.slab != want {
                        tracing::error!(
                            got = format_args!("{:016x}", header.slab),
                            want = format_args!("{want:016x}"),
                            path = %path.display(),
                            "osm books: codebook pinned to a DIFFERENT slab — re-bake this \
                             region so slab and sidecars come from one run; tags unavailable"
                        );
                        return None;
                    }
                    tracing::info!(path = %path.display(), rows = header.rows, "osm books: codebook loaded");
                    Some(books)
                }
                // A magic mismatch is not "corrupt" — it is an OUTDATED (or
                // foreign) bake, and saying so is the difference between a
                // one-line fix and diagnosing a grey map by eye. Everything
                // downstream falls back to `ShapeClass::Other` without tags,
                // so the symptom is a map that draws but says nothing.
                Err(osm_soa_bake::codebook::BookError::Magic(found)) => {
                    tracing::error!(
                        path = %path.display(),
                        found = %String::from_utf8_lossy(&found).escape_debug().to_string(),
                        expected = %String::from_utf8_lossy(&osm_soa_bake::codebook::MAGIC).escape_debug().to_string(),
                        "osm books: OUTDATED bake format — this build does not speak that \
                         codebook version; re-bake the region with the current baker"
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = ?e, "osm books: unreadable; tags unavailable");
                    None
                }
            }
        })
        .as_ref()
}

/// The one line an operator actually reads, as a pure function so each arm can
/// be tested without mutating process env or faking a filesystem.
///
/// Each arm names the NEXT ACTION, not just the state: "books: false" alone
/// does not distinguish "ship the missing file" from "re-bake the region",
/// and those are different jobs. The `present but not loaded` arm is the one
/// worth keeping distinct — it is the case where the operator has already
/// done the obvious thing and it did not work.
fn styling_verdict(configured: bool, books_loaded: bool, books_present: bool) -> &'static str {
    if !configured {
        "unconfigured: no OSM bake on this deploy (the tile endpoints answer 503 by design)"
    } else if books_loaded {
        "ok: features classify by tag, so per-class fills and textures apply"
    } else if books_present {
        "DEGRADED: the codebook file is present but was REFUSED — a foreign bake (digest \
         mismatch) or an outdated format. Check the `osm books:` error line; re-bake the \
         region so slab and sidecars come from one run"
    } else {
        "DEGRADED: the codebook file is ABSENT — every shape falls back to ShapeClass::Other, \
         so the map draws grey/stroke-only and feature clicks resolve no tags. Ship \
         <slab-stem>.books from the SAME bake run as the slab"
    }
}

/// Header-only validity check for the `.books` sidecar: does it parse, and
/// does its recorded slab digest match the CURRENTLY-served slab? Reads ~40
/// bytes via [`osm_soa_bake::codebook::read_books_header`] — never the full
/// codebook `open_books()` would parse.
fn books_header_valid_for_slab(path: Option<std::path::PathBuf>) -> bool {
    let Some(path) = path else { return false };
    let Ok(mut file) = std::fs::File::open(&path) else {
        return false;
    };
    let Ok(header) = osm_soa_bake::codebook::read_books_header(&mut file) else {
        return false;
    };
    Some(header.slab) == slab_digest()
}

/// The chains-side sibling of [`books_header_valid_for_slab`], via
/// [`osm_soa_bake::chains::read_chains_header`] (~24 bytes).
fn chains_header_valid_for_slab(path: Option<std::path::PathBuf>) -> bool {
    let Some(path) = path else { return false };
    let Ok(mut file) = std::fs::File::open(&path) else {
        return false;
    };
    let Ok((slab_digest_read, _count, _blob_len)) =
        osm_soa_bake::chains::read_chains_header(&mut file)
    else {
        return false;
    };
    Some(slab_digest_read) == slab_digest()
}

/// `GET /api/osm/health` — why the map looks the way it does.
///
/// Exists because the failure that matters most on this endpoint family is
/// INVISIBLE from the outside. With the codebook absent every shape classes as
/// `ShapeClass::Other`, so the server returns a full, correct tile of correct
/// geometry and the browser draws it grey and untagged: no error, no 503, no
/// wrong status code, nothing to catch. Diagnosing that from the outside meant
/// guessing between four causes that look identical on screen — unset env,
/// absent file, foreign bake, outdated format. This reports which one it is.
///
/// Deliberately reports the FILE state and the LOADED state separately: they
/// disagree in exactly the interesting case (the file is present but was
/// refused for a digest or magic mismatch), and collapsing them into one
/// boolean would hide the difference between "ship the file" and "re-bake the
/// region". Cheap enough to be safe: two `stat`s plus two ~40/24-byte header
/// reads — never the full eager singleton load (see the two helpers above).
pub async fn osm_health_handler() -> Json<serde_json::Value> {
    let slab_env = std::env::var("OSM_SLAB_PATH").ok();
    let sidecar = |ext: &str| {
        slab_env
            .as_ref()
            .map(|s| std::path::PathBuf::from(s).with_extension(ext))
    };
    let stat = |p: Option<std::path::PathBuf>| match p {
        Some(p) => match std::fs::metadata(&p) {
            Ok(m) => serde_json::json!({
                "path": p.display().to_string(), "present": true, "bytes": m.len(),
            }),
            Err(e) => serde_json::json!({
                "path": p.display().to_string(), "present": false, "error": e.to_string(),
            }),
        },
        None => serde_json::json!({ "present": false, "error": "OSM_SLAB_PATH unset" }),
    };

    let books_file = stat(sidecar("books"));
    let chains_file = stat(sidecar("chains"));
    // "loaded" here means "the header validates for the CURRENT slab" — a
    // cheap ~40/24-byte read, deliberately NOT `open_books()`/`open_chains()`.
    // This is a status/diagnostic endpoint; it must not be the thing that
    // triggers the multi-GB permanent singleton load the serving path exists
    // to avoid (see osm_chains_books_lance.rs's module doc and
    // claude-notes/plans/2026-08-16-chains-books-lancedb-blob.md). The JSON
    // key name is kept as "loaded" for compatibility with any existing
    // monitoring against this endpoint, even though what it now reports is
    // "header-valid", not "resident in memory".
    let books_loaded = books_header_valid_for_slab(sidecar("books"));
    let chains_loaded = chains_header_valid_for_slab(sidecar("chains"));
    let rows = open_slab()
        .and_then(|b| RowSlab::new(b).ok())
        .map(|s| s.len());

    let styling = styling_verdict(
        slab_env.is_some(),
        books_loaded,
        books_file["present"] == serde_json::json!(true),
    );

    Json(serde_json::json!({
        "slab": {
            "path": slab_env,
            "rows": rows,
            "digest": slab_digest().map(|d| format!("{d:016x}")),
        },
        "books":  { "file": books_file,  "loaded": books_loaded },
        "chains": { "file": chains_file, "loaded": chains_loaded },
        "styling": styling,
    }))
}

/// Resolve one slab row into identity + tags.
///
/// Tags bind to their member by ORDINAL, not by slot adjacency (the property
/// `cluster` documents as letting a continuation row be read alone), so a row's
/// own tags are the tag facets whose `member` equals its identity ordinal.
/// Filtering on that rather than taking every tag facet is what stops a
/// continuation row's tags being attributed to the wrong element.
fn query_feature(
    bytes: &[u8],
    idx: usize,
    books: Option<&BooksHandle<'_>>,
) -> Result<FeatureDetailOut, String> {
    let slab = RowSlab::new(bytes).map_err(|e| format!("slab bytes not row-aligned: {e:?}"))?;
    if idx >= slab.len() {
        return Err(format!(
            "row {idx} is past the end of the slab ({})",
            slab.len()
        ));
    }
    let (lon, lat) = morton_to_lonlat(slab.morton_at(idx));

    // Identity and tags both need the typed row; position never does.
    let Some(rows) = slab.rows() else {
        return Ok(FeatureDetailOut {
            idx,
            lon,
            lat,
            entity_type: None,
            ordinal: None,
            osm_key: None,
            tags: Default::default(),
        });
    };
    let row = &rows[idx];
    let (entity_type, ordinal) = match read_identity(row) {
        Some((t, o)) => (Some(t), Some(o)),
        None => (None, None),
    };

    let osm_key = books
        .zip(ordinal)
        .and_then(|(b, o)| b.identity(o))
        .map(str::to_string);

    let mut tags = std::collections::BTreeMap::new();
    if let (Some(b), Some(mine)) = (books, ordinal) {
        for (_slot, facet) in osm_soa_bake::cluster::facets(row) {
            if let osm_soa_bake::cluster::Facet::Tag { member, key, value } = facet {
                if member != mine {
                    continue; // another member's tag; see the doc comment.
                }
                if let (Some(k), Some(v)) = (b.tag_key(key), b.tag_value(value)) {
                    tags.insert(k.to_string(), v.to_string());
                }
            }
        }
    }

    Ok(FeatureDetailOut {
        idx,
        lon,
        lat,
        entity_type,
        ordinal,
        osm_key,
        tags,
    })
}

/// Every ordinal [`query_feature`] will need for row `idx`: its own identity
/// ordinal (for `osm_key`) plus every tag key/value ordinal on that row's own
/// facets (member-filtered, same rule the function itself documents). Reads
/// the row once — `query_feature` re-reads it, the same accepted duplication
/// [`tile_sources`]/`query_tile_shapes` already carry (see that pair's doc).
fn feature_ordinals(bytes: &[u8], idx: usize) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let Ok(slab) = RowSlab::new(bytes) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    if idx >= slab.len() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let Some(rows) = slab.rows() else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let row = &rows[idx];
    let Some((_, ordinal)) = read_identity(row) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let mut key_ordinals = Vec::new();
    let mut value_ordinals = Vec::new();
    for (_slot, facet) in osm_soa_bake::cluster::facets(row) {
        if let osm_soa_bake::cluster::Facet::Tag { member, key, value } = facet
            && member == ordinal
        {
            key_ordinals.push(key);
            value_ordinals.push(value);
        }
    }
    (vec![ordinal], key_ordinals, value_ordinals)
}

/// `GET /api/osm/feature/:idx` — what IS this dot?
pub async fn osm_feature_handler(Path(idx): Path<usize>) -> Response {
    let Some(bytes) = open_slab() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "OSM_SLAB_PATH is not set or the baked slab could not be opened",
            })),
        )
            .into_response();
    };
    let (identity_ordinals, key_ordinals, value_ordinals) = feature_ordinals(bytes, idx);
    let books = single_gather_books(&identity_ordinals, &key_ordinals, &value_ordinals).await;
    match query_feature(bytes, idx, books.as_ref()) {
        Ok(out) => Json(out).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// The `.chains` sidecar — vertex chains for tagged ways, opened once.
///
/// **The digest pin is enforced, not decorative:** a chains file whose
/// `slab_digest` does not match the mapped slab is refused entirely, because
/// serving ring geometry from one bake against identities of another is
/// silent cross-bake corruption — the exact drift the pin exists to make loud.
static CHAINS: OnceLock<Option<osm_soa_bake::chains::Chains>> = OnceLock::new();

fn open_chains() -> Option<&'static osm_soa_bake::chains::Chains> {
    CHAINS
        .get_or_init(|| {
            let slab = open_slab()?;
            let path = sidecar_path("chains")?;
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                // Same silent-`None` defect as the books arm above: absent
                // chains means no ring geometry at all (the dots still work),
                // which reads on screen as an empty neighbourhood rather than
                // as a missing file.
                Err(e) => {
                    tracing::error!(
                        path = %path.display(), error = %e,
                        "osm chains: geometry sidecar ABSENT next to the slab — no shapes will \
                         draw. Ship <slab-stem>.chains from the SAME bake run as the slab"
                    );
                    return None;
                }
            };
            let _ = slab; // digest reads the same mmap via slab_digest()
            match osm_soa_bake::chains::Chains::from_bytes(bytes) {
                Ok(ch) => {
                    // One hash per process: the same digest doubles as the
                    // binary tile wire's ETag (see `slab_digest`).
                    let want = slab_digest()?;
                    if ch.slab_digest != want {
                        tracing::error!(
                            got = format_args!("{:016x}", ch.slab_digest),
                            want = format_args!("{want:016x}"),
                            "osm chains: sidecar pinned to a DIFFERENT slab; refusing"
                        );
                        return None;
                    }
                    tracing::info!(path = %path.display(), ways = ch.len(), "osm chains: loaded");
                    Some(ch)
                }
                Err(osm_soa_bake::chains::ChainError::BadMagic) => {
                    tracing::error!(
                        path = %path.display(),
                        expected = %String::from_utf8_lossy(&osm_soa_bake::chains::MAGIC).escape_debug().to_string(),
                        "osm chains: OUTDATED bake format — this build does not speak that \
                         chains layout; re-bake the region with the current baker"
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = ?e, "osm chains: unreadable; geometry unavailable");
                    None
                }
            }
        })
        .as_ref()
}

/// One feature's rehydrated shape — the decode half of the chains codec.
#[derive(Debug, Serialize, PartialEq)]
pub struct FeatureGeometryOut {
    pub idx: usize,
    pub ordinal: Option<u32>,
    /// First vertex == last vertex and at least 4 vertices — a ring. The
    /// renderer fills a ring carrying an areal tag and strokes everything else;
    /// that CLASSIFICATION is the client's, the SHAPE is the codec's.
    pub closed: bool,
    /// `[lon, lat]` pairs in way order, decoded from z=32 cells — the same
    /// grid the anchors live on, so a way's first vertex and a node at the
    /// same position agree exactly.
    pub points: Vec<[f64; 2]>,
}

fn query_geometry(
    bytes: &[u8],
    idx: usize,
    chains: Option<&ChainsHandle<'_>>,
) -> Result<Option<FeatureGeometryOut>, String> {
    let slab = RowSlab::new(bytes).map_err(|e| format!("slab bytes not row-aligned: {e:?}"))?;
    if idx >= slab.len() {
        return Err(format!(
            "row {idx} is past the end of the slab ({})",
            slab.len()
        ));
    }
    let Some(rows) = slab.rows() else {
        return Ok(None);
    };
    let Some((_, ordinal)) = read_identity(&rows[idx]) else {
        return Ok(None);
    };
    let Some(chains) = chains else {
        return Ok(None);
    };
    let chain = chains
        .get(ordinal)
        .map_err(|e| format!("chain record for ordinal {ordinal} is malformed: {e:?}"))?;
    let Some(chain) = chain else {
        return Ok(None); // a node or relation: no chain is a real answer
    };
    let closed = chain.len() >= 4 && chain.first() == chain.last();
    let points = chain
        .iter()
        .map(|c| {
            let (lon, lat) = osm_soa_bake::tms::tile_to_lonlat(c.x, c.y_xyz);
            [lon, lat]
        })
        .collect();
    Ok(Some(FeatureGeometryOut {
        idx,
        ordinal: Some(ordinal),
        closed,
        points,
    }))
}

/// `GET /api/osm/geometry/:idx` — the dot's SHAPE. 404 (not 200-with-empty)
/// when the feature has no chain, so "no geometry stored" and "empty geometry"
/// can never be confused.
pub async fn osm_geometry_handler(Path(idx): Path<usize>) -> Response {
    let Some(bytes) = open_slab() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "OSM_SLAB_PATH is not set or the baked slab could not be opened",
            })),
        )
            .into_response();
    };
    let (identity_ordinals, _, _) = feature_ordinals(bytes, idx);
    let chains = single_gather_chains(&identity_ordinals).await;
    match query_geometry(bytes, idx, chains.as_ref()) {
        Ok(Some(out)) => Json(out).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "no chain stored for this feature" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The vector basemap: the bake DRAWS the map, instead of decorating a rented
// raster. `/api/osm/geometry/:idx` answers "what shape is THIS one?"; the tile
// form below answers "what shapes are HERE?", which is the whole basemap.
// ─────────────────────────────────────────────────────────────────────────

/// What a shape IS, derived from the tags the bake already stored.
///
/// The CATEGORY is data: the same rule has to answer for one clicked way and
/// for ten thousand basemap ways, so it lives here and is served, rather than
/// being re-derived in the client from tags it would first have to download.
/// Shipping every shape's tags would multiply the payload by the tag fan-out
/// for data a viewer wants about one shape at a time — the argument
/// [`FeatureOut::idx`] already makes for detail.
///
/// The STYLE is NOT here. This names what a thing is; colour, width and
/// z-order stay the client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeClass {
    Water,
    Building,
    Wood,
    /// Whatever carries a green-ish tag but is not one of the specific kinds
    /// below. Kept as the FALLBACK so widening the specific set never demotes
    /// something to `Other`.
    Green,
    Rail,
    Road,
    Other,
    // Appended, never inserted: 0-6 are a wire contract with the client's
    // CLASS_ORDER array (see `wire_code`).
    /// Grass, meadow, farmland, heath — the textured kinds. Split out because
    /// they are the majority of a rural or suburban viewport and reading them
    /// as one flat wash with everything else is what made a whole z16 view
    /// uniformly green.
    Meadow,
    /// Managed green: park, garden, pitch, cemetery.
    Park,
    /// `landuse=residential` and its industrial/commercial siblings. NOT
    /// green — this is the tag that covers most of a suburb, and colouring it
    /// as vegetation is why Zehlendorf rendered as a meadow.
    Built,
}

impl ShapeClass {
    /// The class's byte on the binary wire. EXPLICIT, not `as u8`: the enum's
    /// declaration order is a refactoring surface, the wire is a contract —
    /// the client's `CLASS_ORDER` table indexes by these exact values, and
    /// `wire_codes_are_pinned` fails if either side moves without the other.
    fn wire_code(self) -> u8 {
        match self {
            ShapeClass::Water => 0,
            ShapeClass::Building => 1,
            ShapeClass::Wood => 2,
            ShapeClass::Green => 3,
            ShapeClass::Rail => 4,
            ShapeClass::Road => 5,
            ShapeClass::Other => 6,
            ShapeClass::Meadow => 7,
            ShapeClass::Park => 8,
            ShapeClass::Built => 9,
        }
    }
}

/// Classify a tag set by precedence.
///
/// Tags arrive in arbitrary order, so this cannot be "first match wins" over
/// the iterator: it captures the handful of keys that matter in one pass, then
/// applies precedence. Specific beats generic — `natural=water` is Water, not
/// Green, even though `natural` alone means Green.
fn class_for_tags<'a, I>(tags: I) -> ShapeClass
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let (mut natural, mut landuse, mut leisure) = (None, None, None);
    let (mut building, mut waterway, mut highway, mut railway) = (false, false, false, false);
    for (k, v) in tags {
        match k {
            "natural" => natural = Some(v),
            "landuse" => landuse = Some(v),
            "leisure" => leisure = Some(v),
            "building" => building = true,
            "waterway" => waterway = true,
            "highway" => highway = true,
            "railway" => railway = true,
            _ => {}
        }
    }
    if natural == Some("water") || waterway {
        return ShapeClass::Water;
    }
    if building {
        return ShapeClass::Building;
    }
    if natural == Some("wood") || landuse == Some("forest") {
        return ShapeClass::Wood;
    }
    // The three specific kinds, ahead of the generic green fallback. Order
    // among THEM does not matter (the tag sets are disjoint); order relative
    // to the fallback is everything.
    if matches!(
        landuse,
        Some(
            "grass"
                | "meadow"
                | "farmland"
                | "farmyard"
                | "orchard"
                | "vineyard"
                | "allotments"
                | "village_green"
                | "greenfield"
                | "flowerbed"
        )
    ) || matches!(
        natural,
        Some("grassland" | "heath" | "scrub" | "moor" | "fell")
    ) {
        return ShapeClass::Meadow;
    }
    if matches!(
        leisure,
        Some(
            "park"
                | "garden"
                | "pitch"
                | "playground"
                | "recreation_ground"
                | "golf_course"
                | "dog_park"
                | "common"
        )
    ) || matches!(
        landuse,
        Some("cemetery" | "grave_yard" | "recreation_ground")
    ) {
        return ShapeClass::Park;
    }
    if matches!(
        landuse,
        Some(
            "residential"
                | "industrial"
                | "commercial"
                | "retail"
                | "construction"
                | "garages"
                | "railway"
                | "quarry"
                | "brownfield"
                | "military"
        )
    ) {
        return ShapeClass::Built;
    }
    if landuse.is_some() || leisure.is_some() || natural.is_some() {
        return ShapeClass::Green;
    }
    if railway {
        return ShapeClass::Rail;
    }
    if highway {
        return ShapeClass::Road;
    }
    ShapeClass::Other
}

/// This row's own tags as borrowed `(k, v)` pairs.
///
/// Tags bind to their member by ORDINAL, not by slot adjacency — the same
/// filter [`query_feature`] documents, and for the same reason: without it a
/// continuation row's tags are attributed to the wrong element.
fn row_tags<'a>(
    row: &NodeRow,
    books: &'a BooksHandle<'_>,
    ordinal: u32,
) -> Vec<(&'a str, &'a str)> {
    let mut out = Vec::new();
    for (_slot, facet) in osm_soa_bake::cluster::facets(row) {
        if let osm_soa_bake::cluster::Facet::Tag { member, key, value } = facet {
            if member != ordinal {
                continue;
            }
            if let (Some(k), Some(v)) = (books.tag_key(key), books.tag_value(value)) {
                out.push((k, v));
            }
        }
    }
    out
}

/// How far to shift a z32 cell to land on the screen pixel grid at zoom `z`.
///
/// A slippy tile is 256 px = 2^8, so the world is 2^(z+8) px wide at zoom `z`
/// and the chain's own z32 coordinate needs `32 - (z + 8)` bits removed. One
/// extra bit of headroom keeps a half-pixel of detail, because a phone's
/// `devicePixelRatio` is 2-3 and simplifying exactly to the CSS pixel grid is
/// visible as faceting on those screens.
///
/// Saturates at 0: past z≈23 the chain is already finer than the screen and
/// nothing should be dropped.
const SUBPIXEL_BITS: u32 = 1;
fn pixel_shift(z: u32) -> u32 {
    32u32.saturating_sub(z + 8 + SUBPIXEL_BITS)
}

/// Drop vertices that land on the same (sub)pixel at this zoom, keeping the
/// survivors as RAW z32 cells.
///
/// This is an integer compare on the codec's own grid — no floating-point
/// distance test and no tolerance constant to tune, because the display's
/// resolution IS the tolerance. Sub-pixel detail is not "less accurate" to
/// draw; it is invisible, and at city scale it is most of the payload.
///
/// The first and last vertices always survive, so a ring that survives at all
/// stays closed. A ring that thins below 4 vertices is no longer a ring at
/// this zoom — the caller drops it rather than emit a degenerate polygon.
///
/// Raw cells (not lon/lat) on purpose: the JSON wire projects them to
/// degrees, the binary wire projects them to tile-relative pixels, and both
/// projections must consume the SAME survivor set or the two forms drift.
fn simplify_cells_raw(
    chain: &[osm_soa_bake::tms::TileXy],
    z: u32,
) -> Vec<osm_soa_bake::tms::TileXy> {
    let shift = pixel_shift(z);
    let mut out = Vec::new();
    let mut last: Option<(u32, u32)> = None;
    for (i, c) in chain.iter().enumerate() {
        let cell = (c.x >> shift, c.y_xyz >> shift);
        let is_last = i + 1 == chain.len();
        // `is_last` forces the closing vertex through even when it repeats the
        // previous kept pixel: for a ring that repeat IS the closure.
        if last != Some(cell) || is_last {
            out.push(*c);
            last = Some(cell);
        }
    }
    out
}

/// Geometry is far heavier per row than a dot, so it gets its own ceiling
/// rather than reusing [`row_budget`]. Same zoom split and same reasoning as
/// there: decimating an overview is a legitimate LOD choice, decimating a city
/// is a wrong map.
/// Geometry-wire budgets, re-grounded 2026-08-12 for the GL renderer.
///
/// The old values (1_500 / 12_000) inherited the DOM-marker cost model that
/// `OVERVIEW_ROW_BUDGET`'s history documents (177,963 retained DOM nodes =
/// the measured ceiling; 1.77M = a hung page). The geometry wire no longer
/// renders that way: shapes land in retained per-tile VBOs and draw as two
/// GL calls per tile (`drawBaseGL`), and THIS deployment measured the same
/// GPU path at 38.78M tris / 19.40M verts in 2 draw calls, LOD off, on
/// `/garmin/havel`. Measured against that:
///
/// - **City zooms are complete, never decimated** — the budget becomes the
///   same transport backstop the dot wire uses (`CITY_ROW_CEILING`): the
///   densest possible Berlin z13 tile is bounded by 246,016 rows (64 hip
///   tiles x 3,844 max measured), so the ceiling provably cannot fire for a
///   Berlin-class bake; if it ever does, `sampled < total` reports it.
/// - **Overview zooms keep the Morton-cell LOD** (`overview_sample` — one
///   representative per occupied cell, the rule the table above proved
///   drops no isolated features), with 8x the room: 12,000 x ~63 viewport
///   tiles ≈ 750k shapes ≈ single-digit-millions of line verts, an order of
///   magnitude under the 19.4M verts measured live. The dot wire's budget is
///   untouched — dots are points, geometry is the map.
const GEOMETRY_OVERVIEW_BUDGET: usize = 12_000;
const GEOMETRY_CITY_BUDGET: usize = CITY_ROW_CEILING;
/// Below this on-screen extent a shape has no resolvable outline, and drawing
/// one is worse than drawing nothing.
///
/// At z12 a Berlin building is a fraction of a pixel. Its OUTLINE still gets
/// stroked — and the stroke colours are dark by design (`other` is `#39424f`)
/// — so in the dense core thousands of them land on the same pixels and
/// saturate it to black, while the FILL, which is the colour that actually
/// carries the meaning, never survives at all. The outskirts stay legible
/// only because they are sparse. That is the whole mechanism behind "the
/// centre defaults to black": edges merging into nothingness.
///
/// Such a shape degrades to ONE point carrying its class, which the client
/// paints in the class's fill colour. The aggregate then reads as a colour
/// field — the landcover the zoom is actually asking about — instead of edge
/// mush.
///
/// This is a RESOLUTION rule, not a zoom rule: it is a no-op the moment a
/// shape is bigger than a pixel, so it needs no zoom branch and disables
/// itself as you zoom in. At city zooms essentially nothing qualifies, which
/// is why the under-budget city view is unaffected.
const DOT_PX: f64 = 1.5;

/// `Some(centroid)` when the shape cannot resolve an outline at `z`.
fn dot_if_subpixel(
    cells: &[osm_soa_bake::tms::TileXy],
    z: u32,
) -> Option<osm_soa_bake::tms::TileXy> {
    if cells.is_empty() {
        return None;
    }
    let (mut lox, mut hix) = (u32::MAX, 0u32);
    let (mut loy, mut hiy) = (u32::MAX, 0u32);
    for c in cells {
        lox = lox.min(c.x);
        hix = hix.max(c.x);
        loy = loy.min(c.y_xyz);
        hiy = hiy.max(c.y_xyz);
    }
    // The same `2^(z-24)` cell→world-pixel factor `encode_tile_bin` applies,
    // so the test is in the units the shape will actually be drawn in.
    let scale = f64::exp2(z as f64 - 24.0);
    if (hix - lox) as f64 * scale >= DOT_PX || (hiy - loy) as f64 * scale >= DOT_PX {
        return None;
    }
    // u64 midpoint: `lox + hix` overflows u32 for cells in the upper half of
    // the z32 grid, which is most of the eastern hemisphere.
    Some(osm_soa_bake::tms::TileXy {
        x: ((lox as u64 + hix as u64) / 2) as u32,
        y_xyz: ((loy as u64 + hiy as u64) / 2) as u32,
    })
}

fn geometry_row_budget(z: u32) -> usize {
    if z < CITY_ZOOM_FLOOR {
        GEOMETRY_OVERVIEW_BUDGET
    } else {
        GEOMETRY_CITY_BUDGET
    }
}

#[derive(Debug, Serialize)]
pub struct TileShapeOut {
    pub idx: usize,
    pub class: ShapeClass,
    pub closed: bool,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Serialize)]
pub struct TileGeometryOut {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    /// Rows the tile covers.
    pub total: usize,
    /// Rows the budget selected — `total` when the tile is under budget.
    pub sampled: usize,
    /// Shapes actually returned. Lower than `sampled` by the rows that carry
    /// no chain (every node in the tile) plus any that simplified away.
    pub returned: usize,
    /// Chain records that failed to decode. Reported rather than swallowed: a
    /// corrupt sidecar must not masquerade as an empty neighbourhood, and one
    /// bad record must not 500 a whole tile.
    pub malformed: usize,
    pub shapes: Vec<TileShapeOut>,
}

/// One shape as the query produced it: simplified survivor CELLS, not yet
/// projected to any wire form.
struct RawShape {
    idx: usize,
    class: ShapeClass,
    closed: bool,
    cells: Vec<osm_soa_bake::tms::TileXy>,
}

/// The tile query's result before wire projection. JSON (`TileGeometryOut`)
/// and the binary form (`encode_tile_bin`) are both views of THIS, so the two
/// wires can never disagree about sampling, classification, or survivor sets.
struct TileShapesRaw {
    z: u32,
    x: u32,
    y: u32,
    total: usize,
    sampled: usize,
    malformed: usize,
    shapes: Vec<RawShape>,
}

/// Which rows a tile request will actually touch — split out of
/// [`query_tile_shapes`] so the async gather phase (which needs to know the
/// ordinals BEFORE it can ask Lance for them) and the sync resolve phase
/// share exactly one sampling implementation. Two implementations of "which
/// rows get sampled" is precisely the kind of drift this module's own doc
/// comment (top of file) warns about for `osm_tiles`' old V1/V3 key split.
fn sample_tile_rows(slab: &RowSlab<'_>, z: u32, x: u32, y: u32) -> (usize, Vec<usize>) {
    let range = slab.tile_range(z, x, y);
    let total = range.len();
    let selected = overview_sample(
        z,
        range.clone(),
        |i| slab.morton_at(i),
        geometry_row_budget(z),
    );
    (total, selected)
}

/// Either source of chain geometry a request can be served from: the
/// request-scoped Lance gather (the common, RAM-safe case — see
/// `osm_chains_books_lance`'s module doc) or the eager resident singleton
/// (fallback when the Lance dataset isn't available). Deliberately an enum,
/// not a trait: both arms already expose the same `get(ordinal)` shape as
/// inherent methods, and dispatching on a fixed two-case enum reads clearer
/// at every call site than a generic bound or a trait object would here.
enum ChainsHandle<'a> {
    Lance(crate::osm_chains_books_lance::RequestChains),
    Eager(&'a osm_soa_bake::chains::Chains),
}

impl ChainsHandle<'_> {
    fn get(
        &self,
        ordinal: u32,
    ) -> Result<Option<Vec<osm_soa_bake::tms::TileXy>>, osm_soa_bake::chains::ChainError> {
        match self {
            Self::Lance(r) => r.get(ordinal),
            Self::Eager(c) => c.get(ordinal),
        }
    }
}

/// The books-side sibling of [`ChainsHandle`]. `identity`/`tag_key`/
/// `tag_value` name the three codebooks by role rather than exposing them as
/// fields (unlike the resident `Books`), since the Lance-gathered
/// [`osm_chains_books_lance::RequestBooks`] and the eager `Books` don't share
/// a field type to borrow through uniformly.
enum BooksHandle<'a> {
    Lance(crate::osm_chains_books_lance::RequestBooks),
    Eager(&'a osm_soa_bake::codebook::Books),
}

impl BooksHandle<'_> {
    fn identity(&self, ordinal: u32) -> Option<&str> {
        match self {
            Self::Lance(r) => r.identities.key(ordinal),
            Self::Eager(b) => b.identities.key(ordinal),
        }
    }
    fn tag_key(&self, ordinal: u32) -> Option<&str> {
        match self {
            Self::Lance(r) => r.tag_keys.key(ordinal),
            Self::Eager(b) => b.tag_keys.key(ordinal),
        }
    }
    fn tag_value(&self, ordinal: u32) -> Option<&str> {
        match self {
            Self::Lance(r) => r.tag_values.key(ordinal),
            Self::Eager(b) => b.tag_values.key(ordinal),
        }
    }
}

fn query_tile_shapes(
    bytes: &[u8],
    z: u32,
    x: u32,
    y: u32,
    chains: Option<&ChainsHandle<'_>>,
    books: Option<&BooksHandle<'_>>,
) -> Result<TileShapesRaw, String> {
    let slab = RowSlab::new(bytes).map_err(|e| format!("slab bytes not row-aligned: {e:?}"))?;
    let (total, selected) = sample_tile_rows(&slab, z, x, y);
    let empty = |sampled| TileShapesRaw {
        z,
        x,
        y,
        total,
        sampled,
        malformed: 0,
        shapes: Vec::new(),
    };

    // Identity needs the typed row and geometry needs the sidecar; without
    // either there are no shapes to draw, which is a real answer and not an
    // error (the dots still work).
    let (Some(rows), Some(chains)) = (slab.rows(), chains) else {
        return Ok(empty(0));
    };

    let sampled = selected.len();

    let mut shapes = Vec::new();
    let mut malformed = 0usize;
    for i in selected {
        let Some((_, ordinal)) = read_identity(&rows[i]) else {
            continue;
        };
        let chain = match chains.get(ordinal) {
            Ok(Some(c)) => c,
            Ok(None) => continue, // a node or relation: no chain is a real answer
            Err(_) => {
                malformed += 1;
                continue;
            }
        };
        let ring = chain.len() >= 4 && chain.first() == chain.last();
        let cells = simplify_cells_raw(&chain, z);
        // Sub-pixel at this zoom: a ring that can no longer be a ring, or a
        // line with nothing left to join. Dropping beats emitting a degenerate
        // polygon the renderer would fill as a sliver.
        if (ring && cells.len() < 4) || cells.len() < 2 {
            continue;
        }
        let class = books
            .map(|b| class_for_tags(row_tags(&rows[i], b, ordinal)))
            .unwrap_or(ShapeClass::Other);
        // A shape too small to resolve an outline carries its class as ONE
        // point instead. `closed` goes false with it: a one-point ring is not
        // a ring, and saying so keeps the client's fill path off it.
        let (cells, closed) = match dot_if_subpixel(&cells, z) {
            Some(c) => (vec![c], false),
            None => (cells, ring),
        };
        shapes.push(RawShape {
            idx: i,
            class,
            closed,
            cells,
        });
    }

    Ok(TileShapesRaw {
        z,
        x,
        y,
        total,
        sampled,
        malformed,
        shapes,
    })
}

/// The JSON projection of [`query_tile_shapes`].
fn query_tile_geometry(
    bytes: &[u8],
    z: u32,
    x: u32,
    y: u32,
    chains: Option<&ChainsHandle<'_>>,
    books: Option<&BooksHandle<'_>>,
) -> Result<TileGeometryOut, String> {
    let raw = query_tile_shapes(bytes, z, x, y, chains, books)?;
    let shapes: Vec<TileShapeOut> = raw
        .shapes
        .iter()
        .map(|s| TileShapeOut {
            idx: s.idx,
            class: s.class,
            closed: s.closed,
            points: s
                .cells
                .iter()
                .map(|c| {
                    let (lon, lat) = osm_soa_bake::tms::tile_to_lonlat(c.x, c.y_xyz);
                    [lon, lat]
                })
                .collect(),
        })
        .collect();
    Ok(TileGeometryOut {
        z: raw.z,
        x: raw.x,
        y: raw.y,
        total: raw.total,
        sampled: raw.sampled,
        returned: shapes.len(),
        malformed: raw.malformed,
        shapes,
    })
}

/// The binary tile wire — `OSM1`, all little-endian, per the house rule that
/// `to_le_bytes` IS the wire format (T3/ADR-022: no serde on the hot path).
///
/// ```text
/// magic   u32   0x314D534F  ("OSM1" as LE bytes)
/// total   u32   rows the tile covers
/// sampled u32   rows the budget selected
/// count   u32   shapes that follow
/// malformed u32
/// per shape:
///   idx     u32   slab row index (the /api/osm/feature/:idx handle)
///   class   u8    ShapeClass::wire_code
///   closed  u8    1 = ring
///   npoints u16
///   npoints × (f32 dx, f32 dy)   tile-relative world-pixels at z
/// ```
///
/// Coordinates are projected STRAIGHT from the chain's z32 cells:
/// `world_px = cell · 2^(z+8) / 2^32 = cell · 2^(z−24)`, minus the tile
/// origin — one multiply, no lon/lat round-trip and none of its trig. They
/// are tile-RELATIVE so the f32 mantissa is spent on sub-pixel precision
/// instead of on world position (absolute world-pixels at z19 need 27 bits,
/// which f32 does not have; relative values stay small at every zoom).
///
/// ~8 bytes/point against ~45 for the JSON form — measured 5–8× smaller
/// before compression on realistic shapes.
const TILE_BIN_MAGIC: u32 = 0x314D_534F;

fn encode_tile_bin(raw: &TileShapesRaw) -> Vec<u8> {
    let npts: usize = raw.shapes.iter().map(|s| s.cells.len()).sum();
    let mut out = Vec::with_capacity(20 + raw.shapes.len() * 8 + npts * 8);
    out.extend_from_slice(&TILE_BIN_MAGIC.to_le_bytes());
    out.extend_from_slice(&(raw.total as u32).to_le_bytes());
    out.extend_from_slice(&(raw.sampled as u32).to_le_bytes());
    out.extend_from_slice(&(raw.shapes.len() as u32).to_le_bytes());
    out.extend_from_slice(&(raw.malformed as u32).to_le_bytes());

    // 2^(z-24) as a plain multiply. z ≤ 24 gives a fractional factor, z > 24
    // a multiple — exp2 handles both signs of the exponent.
    let scale = f64::exp2(raw.z as f64 - 24.0);
    let (ox, oy) = ((raw.x * 256) as f64, (raw.y * 256) as f64);

    for s in &raw.shapes {
        // u16 point count: an OSM way is capped at 2,000 nodes upstream, so
        // this cannot fire on real data — but a malformed chain must truncate
        // loudly in the count rather than alias into the next shape's header.
        let n = s.cells.len().min(u16::MAX as usize);
        out.extend_from_slice(&(s.idx as u32).to_le_bytes());
        out.push(s.class.wire_code());
        out.push(u8::from(s.closed));
        out.extend_from_slice(&(n as u16).to_le_bytes());
        for c in &s.cells[..n] {
            out.extend_from_slice(&((c.x as f64 * scale - ox) as f32).to_le_bytes());
            out.extend_from_slice(&((c.y_xyz as f64 * scale - oy) as f32).to_le_bytes());
        }
    }
    out
}

/// The slab's content digest, computed once. Doubles as the binary tile
/// wire's cache validator: the digest is ALREADY the cross-bake pin (the
/// `.chains` sidecar is refused against the wrong slab by this same value),
/// so an ETag built from it busts client caches on a new bake by
/// construction, and never otherwise.
static SLAB_DIGEST: OnceLock<Option<u64>> = OnceLock::new();
fn slab_digest() -> Option<u64> {
    *SLAB_DIGEST.get_or_init(|| open_slab().map(osm_soa_bake::codebook::hash_slab))
}

/// Gather-then-serve orchestration for the tile-shapes hot path.
///
/// Samples the tile's rows (via [`sample_tile_rows`] — the SAME sampling
/// [`query_tile_shapes`] itself uses, so what gets gathered and what gets
/// served can never disagree), collects exactly the chain/tag/identity
/// ordinals those rows need, and gathers them from Lance in one batched read
/// per table. Falls back to the eager resident singletons
/// (`open_chains()`/`open_books()`) only when the Lance dataset isn't
/// available — the boot-time conversion failed, was skipped, or hasn't run.
///
/// This is what actually stops the permanent `Vec`/`String` residency in the
/// success case; see `osm_chains_books_lance`'s module doc and
/// `claude-notes/plans/2026-08-16-chains-books-lancedb-blob.md`.
async fn tile_sources<'a>(
    slab: &RowSlab<'a>,
    selected: &[usize],
) -> (Option<ChainsHandle<'a>>, Option<BooksHandle<'a>>) {
    let Some(rows) = slab.rows() else {
        return (None, None);
    };

    let mut ordinals = Vec::new();
    let mut key_ordinals = Vec::new();
    let mut value_ordinals = Vec::new();
    for &i in selected {
        let Some((_, ordinal)) = read_identity(&rows[i]) else {
            continue;
        };
        ordinals.push(ordinal);
        for (_slot, facet) in osm_soa_bake::cluster::facets(&rows[i]) {
            if let osm_soa_bake::cluster::Facet::Tag { member, key, value } = facet
                && member == ordinal
            {
                key_ordinals.push(key);
                value_ordinals.push(value);
            }
        }
    }

    let chains = single_gather_chains(&ordinals).await;
    // identities aren't read by the tile-shapes hot path — see BooksHandle's doc
    let books = single_gather_books(&[], &key_ordinals, &value_ordinals).await;
    (chains, books)
}

/// The chains half of the gather, shared by [`tile_sources`] (many ordinals)
/// and the single-feature endpoints (one ordinal). `'static` because the
/// eager fallback (`open_chains()`) only ever returns `'static` references —
/// callers with a shorter-lived [`ChainsHandle`] target accept this trivially
/// (`'static` outlives everything).
async fn single_gather_chains(ordinals: &[u32]) -> Option<ChainsHandle<'static>> {
    match sidecar_path("chains") {
        Some(chains_path) => {
            let dataset_path = chains_path.with_extension("chains.lance");
            match crate::osm_chains_books_lance::gather_chains(&dataset_path, ordinals).await {
                Some(r) => Some(ChainsHandle::Lance(r)),
                None => open_chains().map(ChainsHandle::Eager),
            }
        }
        None => open_chains().map(ChainsHandle::Eager),
    }
}

/// The books half of the gather — see [`single_gather_chains`]'s doc for why
/// `'static`.
async fn single_gather_books(
    identity_ordinals: &[u32],
    key_ordinals: &[u32],
    value_ordinals: &[u32],
) -> Option<BooksHandle<'static>> {
    match sidecar_path("books") {
        Some(books_path) => {
            let identities_path = books_path.with_extension("identities.lance");
            let tag_keys_path = books_path.with_extension("tag_keys.lance");
            let tag_values_path = books_path.with_extension("tag_values.lance");
            match crate::osm_chains_books_lance::gather_books(
                &identities_path,
                &tag_keys_path,
                &tag_values_path,
                identity_ordinals,
                key_ordinals,
                value_ordinals,
            )
            .await
            {
                Some(r) => Some(BooksHandle::Lance(r)),
                None => open_books().map(BooksHandle::Eager),
            }
        }
        None => open_books().map(BooksHandle::Eager),
    }
}

/// `GET /api/osm/geometry/tile-bin/:z/:x/:y` — the same tile query as the
/// JSON form, on the binary wire, with an honest cache story:
/// `Cache-Control: no-cache` + a digest ETag means a zoom revisit costs one
/// conditional request and a 304, never a re-download — and a NEW bake
/// changes the ETag, so nothing stale ever survives a redeploy.
pub async fn osm_tile_geometry_bin_handler(
    Path((z, x, y)): Path<(u32, u32, u32)>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(bytes) = open_slab() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "OSM_SLAB_PATH is not set or the baked slab could not be opened",
            })),
        )
            .into_response();
    };
    // "osm1-" carries the wire-format generation: bumping the format busts
    // caches even when the bake (and so the digest) is unchanged.
    let etag = format!("\"osm1-{:016x}\"", slab_digest().unwrap_or(0));
    if headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (axum::http::header::ETAG, etag),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, no-cache".to_string(),
                ),
            ],
        )
            .into_response();
    }
    let Ok(slab) = RowSlab::new(bytes) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "slab bytes not row-aligned" })),
        )
            .into_response();
    };
    let (_, selected) = sample_tile_rows(&slab, z, x, y);
    let (chains, books) = tile_sources(&slab, &selected).await;
    match query_tile_shapes(bytes, z, x, y, chains.as_ref(), books.as_ref()) {
        Ok(raw) => (
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    "application/octet-stream".to_string(),
                ),
                (axum::http::header::ETAG, etag),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, no-cache".to_string(),
                ),
            ],
            encode_tile_bin(&raw),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// `GET /api/osm/geometry/tile/:z/:x/:y` — every shape anchored in this tile.
/// The basemap itself, served from the bake instead of a third-party CDN.
pub async fn osm_tile_geometry_handler(Path((z, x, y)): Path<(u32, u32, u32)>) -> Response {
    let Some(bytes) = open_slab() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "OSM_SLAB_PATH is not set or the baked slab could not be opened",
            })),
        )
            .into_response();
    };
    let Ok(slab) = RowSlab::new(bytes) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "slab bytes not row-aligned" })),
        )
            .into_response();
    };
    let (_, selected) = sample_tile_rows(&slab, z, x, y);
    let (chains, books) = tile_sources(&slab, &selected).await;
    match query_tile_geometry(bytes, z, x, y, chains.as_ref(), books.as_ref()) {
        Ok(out) => Json(out).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::canonical_node::NODE_ROW_STRIDE;
    use osm_soa_bake::tms::point_to_tms_morton;

    /// All four arms are reachable and distinct — the point of the endpoint is
    /// telling them APART, so a verdict that collapsed any two would be worse
    /// than none. In particular `present-but-not-loaded` must not read as
    /// `absent`: the operator has already shipped the file in that case and
    /// needs to be sent to re-baking, not back to copying.
    #[test]
    fn styling_verdict_separates_all_four_states() {
        let unconfigured = styling_verdict(false, false, false);
        let ok = styling_verdict(true, true, true);
        let refused = styling_verdict(true, false, true);
        let absent = styling_verdict(true, false, false);

        for (a, b) in [
            (unconfigured, ok),
            (unconfigured, refused),
            (unconfigured, absent),
            (ok, refused),
            (ok, absent),
            (refused, absent),
        ] {
            assert_ne!(a, b, "two states must never produce the same verdict");
        }

        // The two that actually fire on a broken deploy must be findable by an
        // operator grepping for trouble, and must name their distinct fix.
        assert!(refused.contains("DEGRADED") && absent.contains("DEGRADED"));
        assert!(
            refused.contains("re-bake"),
            "a refused codebook needs a re-bake"
        );
        assert!(
            absent.contains("Ship"),
            "an absent codebook needs the file shipped"
        );
        // A working deploy must NOT shout — otherwise the signal is worthless.
        assert!(!ok.contains("DEGRADED") && !unconfigured.contains("DEGRADED"));
    }

    /// The sidecar contract main.rs promises: `OSM_SLAB_PATH` stays anchored to
    /// the `.soa`, so both sidecars resolve from the SAME stem.
    ///
    /// The second half is the falsifier that matters. `with_extension` replaces
    /// only the final component, so if `OSM_SLAB_PATH` were ever repointed at
    /// the Lance dataset (`berlin.soa.lance`) the sidecars would silently
    /// resolve to `berlin.soa.books` — a file that does not exist — and the map
    /// would go grey with the slab itself still serving perfectly. That is
    /// precisely the invisible failure this endpoint exists to catch, so the
    /// path arithmetic behind it is worth pinning rather than assuming.
    #[test]
    fn sidecars_resolve_from_the_slab_stem_not_the_lance_dir() {
        use std::path::PathBuf;
        let slab = PathBuf::from("/vol/osm/berlin.soa");
        assert_eq!(
            slab.with_extension("books"),
            PathBuf::from("/vol/osm/berlin.books")
        );
        assert_eq!(
            slab.with_extension("chains"),
            PathBuf::from("/vol/osm/berlin.chains")
        );

        let lance = PathBuf::from("/vol/osm/berlin.soa.lance");
        assert_eq!(
            lance.with_extension("books"),
            PathBuf::from("/vol/osm/berlin.soa.books"),
            "repointing at the Lance dir would resolve a sidecar that never exists"
        );
        assert_ne!(
            lance.with_extension("books"),
            slab.with_extension("books"),
            "the two anchors must not be confusable"
        );
    }

    /// End-to-end proof that `books_header_valid_for_slab`/
    /// `chains_header_valid_for_slab` — the `/api/osm/health` replacements
    /// for `open_books().is_some()`/`open_chains().is_some()` — actually
    /// agree with a real slab digest, not just that the header parses.
    ///
    /// This is the first test in this file to set `OSM_SLAB_PATH` and
    /// exercise the real `open_slab()`/`slab_digest()` `OnceLock`s — safe
    /// ONLY because nextest runs every test in its own process (per this
    /// repo's `.claude/rules/integration-tests.md`), so the env var and the
    /// process-global locks it seeds never leak into another test.
    #[test]
    fn books_and_chains_header_validity_agrees_with_the_real_slab_digest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        // Content is irrelevant to open_slab()'s plain-OSM_SLAB_PATH path (no
        // row-stride requirement there — only the OSM_SLAB_ROW_* mmap-range
        // path checks alignment) — any non-empty bytes exercise a real digest.
        let slab_bytes = b"not a real row-aligned slab, and that's fine here".to_vec();
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");
        // SAFETY: nextest runs this test in its own process (see the doc
        // comment above), so no other thread in this process reads or
        // mutates the environment concurrently.
        unsafe { std::env::set_var("OSM_SLAB_PATH", &slab_path) };

        let real_digest = osm_soa_bake::codebook::hash_slab(&slab_bytes);
        assert_eq!(
            slab_digest(),
            Some(real_digest),
            "the fixture must actually seed the real OnceLock before asserting against it"
        );

        // Missing files: false, cheaply, no digest comparison needed.
        assert!(!books_header_valid_for_slab(Some(
            dir.path().join("nope.books")
        )));
        assert!(!chains_header_valid_for_slab(Some(
            dir.path().join("nope.chains")
        )));
        assert!(!books_header_valid_for_slab(None));
        assert!(!chains_header_valid_for_slab(None));

        // A books/chains sidecar whose header claims the SAME digest as the
        // real slab: true.
        let books_path = dir.path().join("region.books");
        {
            use lance_graph_contract::identity_quad::IdentityCodebook;
            use osm_soa_bake::codebook::Books;
            let books = Books {
                identities: IdentityCodebook::try_new(vec!["node/1".into()]).unwrap(),
                tag_keys: IdentityCodebook::try_new(vec!["highway".into()]).unwrap(),
                tag_values: IdentityCodebook::try_new(vec!["primary".into()]).unwrap(),
                labels: IdentityCodebook::try_new(Vec::<String>::new()).unwrap(),
            };
            let header = osm_soa_bake::codebook::Header {
                rows: 1,
                slots_written: 1,
                slab: real_digest,
                rounding: osm_soa_bake::tms::AnchorRounding::CURRENT,
            };
            let mut buf = Vec::new();
            osm_soa_bake::codebook::write_books(&mut buf, &header, &books).expect("write books");
            std::fs::write(&books_path, &buf).expect("write");
        }
        assert!(books_header_valid_for_slab(Some(books_path.clone())));

        let chains_path = dir.path().join("region.chains");
        {
            let mut buf = Vec::new();
            let mut chains: Vec<(u32, Vec<osm_soa_bake::tms::TileXy>)> = Vec::new();
            osm_soa_bake::chains::write_chains(&mut buf, real_digest, &mut chains)
                .expect("write chains");
            std::fs::write(&chains_path, &buf).expect("write");
        }
        assert!(chains_header_valid_for_slab(Some(chains_path.clone())));

        // Now the digest mismatch case: a sidecar valid for a DIFFERENT slab
        // must report false, not true — the exact "wrong marriage" this
        // endpoint exists to catch (see its own doc comment).
        let wrong_books_path = dir.path().join("wrong.books");
        {
            use lance_graph_contract::identity_quad::IdentityCodebook;
            use osm_soa_bake::codebook::{Books, Header};
            let books = Books {
                identities: IdentityCodebook::try_new(Vec::<String>::new()).unwrap(),
                tag_keys: IdentityCodebook::try_new(Vec::<String>::new()).unwrap(),
                tag_values: IdentityCodebook::try_new(Vec::<String>::new()).unwrap(),
                labels: IdentityCodebook::try_new(Vec::<String>::new()).unwrap(),
            };
            let header = Header {
                rows: 0,
                slots_written: 0,
                slab: real_digest.wrapping_add(1), // deliberately wrong
                rounding: osm_soa_bake::tms::AnchorRounding::CURRENT,
            };
            let mut buf = Vec::new();
            osm_soa_bake::codebook::write_books(&mut buf, &header, &books).expect("write");
            std::fs::write(&wrong_books_path, &buf).expect("write");
        }
        assert!(!books_header_valid_for_slab(Some(wrong_books_path)));

        let wrong_chains_path = dir.path().join("wrong.chains");
        {
            let mut buf = Vec::new();
            let mut chains: Vec<(u32, Vec<osm_soa_bake::tms::TileXy>)> = Vec::new();
            osm_soa_bake::chains::write_chains(&mut buf, real_digest.wrapping_add(1), &mut chains)
                .expect("write");
            std::fs::write(&wrong_chains_path, &buf).expect("write");
        }
        assert!(!chains_header_valid_for_slab(Some(wrong_chains_path)));

        // SAFETY: same as the set_var call above.
        unsafe { std::env::remove_var("OSM_SLAB_PATH") };
    }

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

    /// A chain along a straight west→east line, `n` vertices spaced `step`
    /// z32 cells apart. Straight on purpose: simplification must be judged on
    /// resolution alone, and a wiggly fixture would let a shape-aware
    /// implementation pass for the wrong reason.
    fn straight_chain(n: usize, step: u32) -> Vec<osm_soa_bake::tms::TileXy> {
        let base = 1u32 << 31; // mid-world, away from any wrap edge
        (0..n)
            .map(|i| osm_soa_bake::tms::TileXy {
                x: base + i as u32 * step,
                y_xyz: base,
            })
            .collect()
    }

    /// Test-only convenience: the lon/lat projection of [`simplify_cells_raw`],
    /// so the assertions below can compare degrees directly instead of
    /// re-deriving `tile_to_lonlat` at every call site. Production code
    /// projects `simplify_cells_raw`'s output inline (`query_tile_geometry`
    /// for JSON, `encode_tile_bin` for the binary wire) — this wrapper has no
    /// production caller of its own.
    fn simplify_cells(chain: &[osm_soa_bake::tms::TileXy], z: u32) -> Vec<[f64; 2]> {
        simplify_cells_raw(chain, z)
            .into_iter()
            .map(|c| {
                let (lon, lat) = osm_soa_bake::tms::tile_to_lonlat(c.x, c.y_xyz);
                [lon, lat]
            })
            .collect()
    }

    #[test]
    fn class_for_tags_applies_precedence_not_first_match() {
        // Each rule fires on its own…
        assert_eq!(class_for_tags([("natural", "water")]), ShapeClass::Water);
        assert_eq!(class_for_tags([("waterway", "river")]), ShapeClass::Water);
        assert_eq!(class_for_tags([("building", "yes")]), ShapeClass::Building);
        assert_eq!(class_for_tags([("natural", "wood")]), ShapeClass::Wood);
        assert_eq!(class_for_tags([("landuse", "forest")]), ShapeClass::Wood);
        // Re-pinned: this asserted `Green` when every green-ish tag collapsed
        // into one bucket. A park is now its own class — the assertion moved
        // because the BEHAVIOUR deliberately changed, and the split is what
        // `specific_land_kinds_beat_the_generic_green_fallback` covers.
        assert_eq!(class_for_tags([("leisure", "park")]), ShapeClass::Park);
        assert_eq!(class_for_tags([("railway", "rail")]), ShapeClass::Rail);
        assert_eq!(class_for_tags([("highway", "primary")]), ShapeClass::Road);

        // …and SPECIFIC beats GENERIC regardless of iteration order, which is
        // the whole reason this is a two-pass classifier and not first-match.
        // `natural=water` carries the generic `natural` key that alone means
        // Green, so a first-match implementation returns Green for one of
        // these two orders and Water for the other.
        assert_eq!(
            class_for_tags([("natural", "water"), ("landuse", "basin")]),
            ShapeClass::Water
        );
        assert_eq!(
            class_for_tags([("landuse", "basin"), ("natural", "water")]),
            ShapeClass::Water
        );

        // Anti-vacuity: the classifier is not a constant. An untagged way and
        // a way whose tags match no rule both fall through to Other.
        assert_eq!(class_for_tags([]), ShapeClass::Other);
        assert_eq!(
            class_for_tags([("name", "Ackerstraße"), ("addr:city", "Berlin")]),
            ShapeClass::Other
        );
    }

    /// **Two-sided on zoom** — the property a constant threshold cannot have.
    ///
    /// The SAME chain must collapse at overview zoom and survive whole at
    /// street zoom. An implementation that simplifies by a fixed tolerance
    /// (or not at all) fails one side or the other.
    #[test]
    fn simplify_collapses_at_overview_zoom_and_keeps_detail_at_street_zoom() {
        // 400 vertices, one z32 cell apart: far finer than any screen pixel
        // until the deepest zooms.
        let chain = straight_chain(400, 1);

        let coarse = simplify_cells(&chain, 4);
        let paired = simplify_cells(&chain, 22);
        let fine = simplify_cells(&chain, 23);

        assert!(
            coarse.len() < 10,
            "at z4 a 400-vertex chain one cell wide is a single pixel; kept {}",
            coarse.len()
        );
        // z23 is where `pixel_shift` reaches 0 — the screen grid is finer than
        // the chain's own z32 cells, so there is nothing left to merge.
        assert_eq!(
            fine.len(),
            400,
            "at z23 the shift is 0 and no vertex may be dropped"
        );
        // One zoom coarser the shift is exactly 1 bit, so cells pair up: 200
        // pairs plus the forced closing vertex. Pinning the intermediate is
        // what ties this test to `pixel_shift`'s actual arithmetic rather than
        // to a vague "fewer points at lower zoom".
        assert_eq!(
            paired.len(),
            201,
            "at z22 one shift bit merges adjacent cells pairwise"
        );
        assert!(
            fine.len() > coarse.len() * 10,
            "zoom must drive the decision: z23 kept {}, z4 kept {}",
            fine.len(),
            coarse.len()
        );
    }

    /// Extent is never lost: whatever else goes, the endpoints stay put. A
    /// simplifier that drops trailing vertices shortens ways visibly, and on a
    /// ring it would silently open the shape.
    #[test]
    fn simplify_keeps_first_and_last_vertex_exactly() {
        let chain = straight_chain(500, 3);
        let (first, last) = (chain[0], chain[499]);
        let out = simplify_cells(&chain, 6);

        let expect_first = osm_soa_bake::tms::tile_to_lonlat(first.x, first.y_xyz);
        let expect_last = osm_soa_bake::tms::tile_to_lonlat(last.x, last.y_xyz);
        assert_eq!(out.first().copied(), Some([expect_first.0, expect_first.1]));
        assert_eq!(out.last().copied(), Some([expect_last.0, expect_last.1]));
        assert!(out.len() < 500, "the fixture must actually be simplified");
    }

    /// A ring that survives stays a ring. This is what the forced final vertex
    /// in `simplify_cells` buys: without it the closing repeat is dropped as a
    /// same-pixel duplicate and the polygon opens.
    #[test]
    fn simplify_keeps_a_ring_closed() {
        let base = 1u32 << 31;
        let step = 1u32 << 14; // comfortably above a pixel at the zoom below
        let corners = [(0, 0), (1, 0), (1, 1), (0, 1), (0, 0)];
        let chain: Vec<_> = corners
            .iter()
            .map(|(dx, dy)| osm_soa_bake::tms::TileXy {
                x: base + dx * step,
                y_xyz: base + dy * step,
            })
            .collect();

        let out = simplify_cells(&chain, 14);
        assert!(out.len() >= 4, "ring must survive at this zoom: {out:?}");
        assert_eq!(
            out.first(),
            out.last(),
            "a simplified ring must still close on itself"
        );
    }

    /// The wire bytes are a CONTRACT with the client's `CLASS_ORDER` array —
    /// its index IS the byte. An enum reorder that silently shifted these
    /// would recolour the whole map (water drawn as buildings) with no error
    /// anywhere, which is why the codes are pinned by value and not `as u8`.
    #[test]
    fn wire_codes_are_pinned() {
        assert_eq!(ShapeClass::Water.wire_code(), 0);
        assert_eq!(ShapeClass::Building.wire_code(), 1);
        assert_eq!(ShapeClass::Wood.wire_code(), 2);
        assert_eq!(ShapeClass::Green.wire_code(), 3);
        assert_eq!(ShapeClass::Rail.wire_code(), 4);
        assert_eq!(ShapeClass::Road.wire_code(), 5);
        assert_eq!(ShapeClass::Other.wire_code(), 6);
        // Appended after the first shipped wire. A client built against the
        // 7-class table renders these as unknown, never as the WRONG class —
        // which is only true while 0-6 keep their meaning.
        assert_eq!(ShapeClass::Meadow.wire_code(), 7);
        assert_eq!(ShapeClass::Park.wire_code(), 8);
        assert_eq!(ShapeClass::Built.wire_code(), 9);
    }

    /// The flat-green-wash defect, pinned. A whole suburban z16 viewport read
    /// as meadow because `landuse=residential` fell into the generic `Green`
    /// fallback along with actual vegetation. Two-sided: the specific kinds
    /// resolve to themselves AND residential is provably not green.
    #[test]
    fn specific_land_kinds_beat_the_generic_green_fallback() {
        assert_eq!(class_for_tags([("landuse", "meadow")]), ShapeClass::Meadow);
        assert_eq!(
            class_for_tags([("landuse", "farmland")]),
            ShapeClass::Meadow
        );
        assert_eq!(class_for_tags([("natural", "heath")]), ShapeClass::Meadow);
        assert_eq!(class_for_tags([("leisure", "park")]), ShapeClass::Park);
        assert_eq!(class_for_tags([("landuse", "cemetery")]), ShapeClass::Park);

        let residential = class_for_tags([("landuse", "residential")]);
        assert_eq!(residential, ShapeClass::Built);
        assert_ne!(
            residential,
            ShapeClass::Green,
            "a suburb is not vegetation — this is the whole defect"
        );

        // The specific kinds must not have eaten the ones that already worked.
        assert_eq!(class_for_tags([("natural", "wood")]), ShapeClass::Wood);
        assert_eq!(class_for_tags([("natural", "water")]), ShapeClass::Water);
        assert_eq!(class_for_tags([("building", "yes")]), ShapeClass::Building);
        // …and the fallback still catches a green-ish tag none of them name,
        // so widening the specific set can never demote one to `Other`.
        assert_eq!(
            class_for_tags([("landuse", "some_new_tag")]),
            ShapeClass::Green
        );
        assert_eq!(class_for_tags([("leisure", "marina")]), ShapeClass::Green);
    }

    /// Decode `OSM1` bytes back into shapes — an independent reader for the
    /// round-trip test below, written from the format DOC, not from the
    /// encoder's code, so a shared misunderstanding can't self-certify.
    fn decode_tile_bin(
        buf: &[u8],
    ) -> Option<(u32, u32, u32, u32, Vec<(u32, u8, bool, Vec<(f32, f32)>)>)> {
        let rd_u32 = |at: usize| u32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
        if buf.len() < 20 || rd_u32(0) != 0x314D_534F {
            return None;
        }
        let (total, sampled, count, malformed) = (rd_u32(4), rd_u32(8), rd_u32(12), rd_u32(16));
        let mut shapes = Vec::new();
        let mut at = 20;
        for _ in 0..count {
            let idx = rd_u32(at);
            let class = buf[at + 4];
            let closed = buf[at + 5] == 1;
            let n = u16::from_le_bytes(buf[at + 6..at + 8].try_into().unwrap()) as usize;
            at += 8;
            let mut pts = Vec::with_capacity(n);
            for i in 0..n {
                let x = f32::from_le_bytes(buf[at + i * 8..at + i * 8 + 4].try_into().unwrap());
                let y = f32::from_le_bytes(buf[at + i * 8 + 4..at + i * 8 + 8].try_into().unwrap());
                pts.push((x, y));
            }
            at += n * 8;
            shapes.push((idx, class, closed, pts));
        }
        // The buffer must be EXACTLY consumed — trailing bytes mean the
        // encoder and this reader disagree about the format.
        assert_eq!(at, buf.len(), "wire not exactly consumed");
        Some((total, sampled, count, malformed, shapes))
    }

    /// Round-trip through the binary wire, with the coordinate values checked
    /// against an INDEPENDENT projection: the JSON path's lon/lat run through
    /// the client's own slippy formulas. The binary encoder never touches
    /// lon/lat (it scales z32 cells directly), so agreement here is two
    /// implementations meeting, not one implementation echoed.
    #[test]
    fn tile_bin_round_trips_and_matches_the_lonlat_projection() {
        let z = 14u32;
        // A tile that really contains the cells below (Berlin-ish).
        let cells = [
            osm_soa_bake::tms::TileXy {
                x: 0x8988_0000,
                y_xyz: 0x5470_0000,
            },
            osm_soa_bake::tms::TileXy {
                x: 0x8988_4000,
                y_xyz: 0x5470_4000,
            },
            osm_soa_bake::tms::TileXy {
                x: 0x8988_8000,
                y_xyz: 0x5470_0000,
            },
            osm_soa_bake::tms::TileXy {
                x: 0x8988_0000,
                y_xyz: 0x5470_0000,
            },
        ];
        let (tx, ty) = (
            cells[0].x >> (32 - z as usize as u32),
            cells[0].y_xyz >> (32 - z),
        );
        let raw = TileShapesRaw {
            z,
            x: tx,
            y: ty,
            total: 7,
            sampled: 5,
            malformed: 1,
            shapes: vec![RawShape {
                idx: 42,
                class: ShapeClass::Water,
                closed: true,
                cells: cells.to_vec(),
            }],
        };
        let bin = encode_tile_bin(&raw);
        let (total, sampled, count, malformed, shapes) = decode_tile_bin(&bin).expect("magic");
        assert_eq!((total, sampled, count, malformed), (7, 5, 1, 1));
        let (idx, class, closed, pts) = &shapes[0];
        assert_eq!((*idx, *class, *closed), (42, 0, true));
        assert_eq!(pts.len(), 4);

        // Independent check: JSON-path lon/lat -> world px via the SAME
        // formulas the page's lon2x/lat2y use, minus the tile origin.
        for (i, c) in cells.iter().enumerate() {
            let (lon, lat) = osm_soa_bake::tms::tile_to_lonlat(c.x, c.y_xyz);
            let n = f64::exp2(z as f64);
            let wx = (lon + 180.0) / 360.0 * n * 256.0 - (tx * 256) as f64;
            let r = lat.to_radians();
            let wy =
                (1.0 - (r.tan() + 1.0 / r.cos()).ln() / std::f64::consts::PI) / 2.0 * n * 256.0
                    - (ty * 256) as f64;
            assert!(
                (pts[i].0 as f64 - wx).abs() < 0.01 && (pts[i].1 as f64 - wy).abs() < 0.01,
                "point {i}: bin ({}, {}) vs lonlat projection ({wx:.4}, {wy:.4})",
                pts[i].0,
                pts[i].1
            );
        }
    }

    /// An empty tile still carries its honest header — `total` non-zero with
    /// zero shapes is the "all nodes, no ways" answer, and the client's
    /// status line depends on those counts being real.
    #[test]
    fn tile_bin_empty_tile_is_a_valid_header_only_wire() {
        let raw = TileShapesRaw {
            z: 12,
            x: 2200,
            y: 1341,
            total: 33,
            sampled: 33,
            malformed: 0,
            shapes: Vec::new(),
        };
        let bin = encode_tile_bin(&raw);
        assert_eq!(bin.len(), 20, "header-only wire");
        let (total, sampled, count, _, shapes) = decode_tile_bin(&bin).unwrap();
        assert_eq!((total, sampled, count), (33, 33, 0));
        assert!(shapes.is_empty());
    }

    #[test]
    fn geometry_row_budget_is_zoom_conditioned_at_the_city_floor() {
        assert_eq!(
            geometry_row_budget(CITY_ZOOM_FLOOR - 1),
            GEOMETRY_OVERVIEW_BUDGET
        );
        assert_eq!(geometry_row_budget(CITY_ZOOM_FLOOR), GEOMETRY_CITY_BUDGET);
        assert!(
            GEOMETRY_CITY_BUDGET > GEOMETRY_OVERVIEW_BUDGET,
            "a city tile must be allowed more shapes than an overview tile"
        );
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

        assert!(
            !berlin_range.is_empty(),
            "Berlin tile must contain the Berlin rows, got {berlin_range:?}"
        );
        assert!(
            !reyk_range.is_empty(),
            "Reykjavik tile must contain the Reykjavik rows, got {reyk_range:?}"
        );
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

        assert_eq!(
            out.total, 1,
            "exactly the one fixture row must fall in its own tile"
        );
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
    /// \>=95% of the tile's EXTENT. That assertion is unfalsifiable for this
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

    /// **The rule that discriminates cell selection from a row stride.**
    ///
    /// A stride samples the Morton curve at even *index* spacing, and index
    /// spacing is not spatial spacing. Features are kept in proportion to
    /// local density, so an **isolated** feature — one row among a dense
    /// cluster's tens of thousands — survives only if its index happens to
    /// land on the stride. On a map that is the worst thing to lose: a lone
    /// village in empty country is exactly the feature a viewer is looking
    /// for, and its absence is indistinguishable from "nothing is there".
    ///
    /// Cell selection cannot drop it: an isolated feature occupies a cell of
    /// its own at any depth fine enough to separate it, and every occupied
    /// cell contributes exactly one row.
    ///
    /// Two-sided, and BOTH halves are computed from the same fixture and the
    /// same budget so neither can be true by construction: the stride arm must
    /// be measurably worse (it loses outliers) AND the cell arm must keep all
    /// of them. Verified red by swapping `overview_sample`'s body for the
    /// stride form.
    #[test]
    fn cell_selection_keeps_isolated_features_that_a_stride_drops() {
        let z = 6;
        let budget = 500usize;

        // A dense cluster: 100x100 in a ~0.01 deg box.
        let (cx, cy, step) = (13.40_f64, 52.50_f64, 0.0001_f64);
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for i in 0..100 {
            for j in 0..100 {
                pts.push((cx + i as f64 * step, cy + j as f64 * step));
            }
        }
        // Eight isolated features, spread across the rest of the same z6 tile
        // and far from the cluster and from each other. Placed as FRACTIONS of
        // the measured tile bounds (z6 tile 34/20 is lon [11.2500, 16.8750] x
        // lat [52.4828, 55.7766]) rather than as offsets from the cluster — a
        // first attempt used fixed degree offsets and walked straight out of
        // the 5.625-deg-wide tile, which the containment guard below caught.
        let (t_lon0, t_lon1, t_lat0, t_lat1) = (11.2500, 16.8750, 52.4828, 55.7766);
        let outliers: Vec<(f64, f64)> = (0..8)
            .map(|k| {
                (
                    t_lon0 + (t_lon1 - t_lon0) * (0.12 + 0.10 * k as f64),
                    t_lat0 + (t_lat1 - t_lat0) * (0.10 + 0.09 * k as f64),
                )
            })
            .collect();
        pts.extend_from_slice(&outliers);

        let mut mortons: Vec<u64> = pts
            .iter()
            .map(|&(a, b)| point_to_tms_morton(a, b))
            .collect();
        mortons.sort_unstable();
        let bytes = synthetic_slab(&mortons);
        let slab = RowSlab::new(&bytes).expect("row-aligned synthetic buffer");

        let (x, y) = crate::osm_tiles::lonlat_to_tile(cx, cy, z);
        // Every fixture point must be in ONE z6 tile, outliers included, or
        // the comparison is measuring tile membership rather than selection.
        for &(a, b) in pts.iter() {
            assert_eq!(
                crate::osm_tiles::lonlat_to_tile(a, b, z),
                (x, y),
                "fixture point ({a}, {b}) escaped the z={z} tile under test"
            );
        }

        let range = slab.tile_range(z, x, y);
        assert_eq!(range.len(), pts.len(), "every fixture row must land here");
        assert!(
            range.len() > budget,
            "fixture must exceed the budget or nothing is decimated"
        );

        let morton = |i: usize| slab.morton_at(i);
        let is_outlier = |i: usize| {
            let (lon, lat) = morton_to_lonlat(slab.morton_at(i));
            outliers
                .iter()
                .any(|&(a, b)| (lon - a).abs() < 1e-4 && (lat - b).abs() < 1e-4)
        };

        // (1) The stride arm — what the previous rule returned.
        let stride = stride_for(range.len(), budget);
        let stride_kept = range
            .clone()
            .step_by(stride)
            .filter(|&i| is_outlier(i))
            .count();

        // (2) The cell arm — the rule under test.
        let cell = overview_sample(z, range.clone(), morton, budget);
        let cell_kept = cell.iter().copied().filter(|&i| is_outlier(i)).count();

        eprintln!(
            "outliers kept: stride={stride_kept}/8 (stride={stride})  cell={cell_kept}/8 (n={})",
            cell.len()
        );

        assert!(
            cell.len() <= budget,
            "cell selection must still respect the budget, got {}",
            cell.len()
        );
        assert_eq!(
            cell_kept,
            outliers.len(),
            "cell selection must keep EVERY isolated feature — each occupies its \
             own cell at any depth that separates it"
        );
        assert!(
            stride_kept < outliers.len(),
            "fixture is not discriminating: the stride kept {stride_kept}/8 outliers \
             too, so this test cannot tell the two rules apart"
        );
    }

    /// `choose_cell_zoom`'s binary search is only valid because occupied-cell
    /// count never decreases with depth. Pinned rather than assumed — a finer
    /// prefix can split a run but never merge two.
    #[test]
    fn occupied_cells_is_monotone_in_depth() {
        let (cx, cy, step) = (13.40_f64, 52.50_f64, 0.002_f64);
        let mut mortons: Vec<u64> = (0..40)
            .flat_map(|i| {
                (0..40)
                    .map(move |j| point_to_tms_morton(cx + i as f64 * step, cy + j as f64 * step))
            })
            .collect();
        mortons.sort_unstable();
        let bytes = synthetic_slab(&mortons);
        let slab = RowSlab::new(&bytes).expect("row-aligned synthetic buffer");
        let range = 0..mortons.len();
        let morton = |i: usize| slab.morton_at(i);

        let mut prev = 0usize;
        let mut grew = false;
        for zz in 0..=HHTL_ZOOM_MAX {
            let n = occupied_cells(range.clone(), morton, zz * 2);
            assert!(
                n >= prev,
                "occupied cells fell from {prev} to {n} going to z={zz} — binary \
                 search in choose_cell_zoom would be invalid"
            );
            if n > prev {
                grew = true;
            }
            prev = n;
        }
        // Anti-vacuity: monotonicity is trivially true of a constant sequence.
        assert!(
            grew,
            "fixture never split a cell, so monotonicity proves nothing"
        );
        assert_eq!(
            prev,
            mortons.len(),
            "at full depth every distinct point must be its own cell"
        );
    }

    /// The chosen depth must be **maximal** — one level deeper must not fit.
    /// Without this, returning the shallowest depth (or a constant) would pass
    /// every other assertion while throwing away all the detail the budget
    /// could have afforded.
    #[test]
    fn choose_cell_zoom_returns_the_deepest_depth_that_fits() {
        let (cx, cy, step) = (13.40_f64, 52.50_f64, 0.0005_f64);
        let mut mortons: Vec<u64> = (0..60)
            .flat_map(|i| {
                (0..60)
                    .map(move |j| point_to_tms_morton(cx + i as f64 * step, cy + j as f64 * step))
            })
            .collect();
        mortons.sort_unstable();
        let bytes = synthetic_slab(&mortons);
        let slab = RowSlab::new(&bytes).expect("row-aligned synthetic buffer");
        let range = 0..mortons.len();
        let morton = |i: usize| slab.morton_at(i);

        let z = 6;
        let budget = 300usize;
        let zz = choose_cell_zoom(z, range.clone(), morton, budget);

        let here = occupied_cells(range.clone(), morton, zz * 2);
        assert!(
            here <= budget,
            "chosen depth z={zz} yields {here} cells, over budget"
        );

        assert!(
            zz < HHTL_ZOOM_MAX,
            "fixture must not saturate at full depth or maximality is untestable"
        );
        let deeper = occupied_cells(range.clone(), morton, (zz + 1) * 2);
        assert!(
            deeper > budget,
            "z={} would still fit ({deeper} cells <= {budget}) — the chosen depth \
             z={zz} is not maximal and detail is being thrown away",
            zz + 1
        );
    }

    /// **The cell-vs-stride comparison, on the real Berlin bake.**
    ///
    /// Ignored by default because it needs the 1.2 GiB slab; run with
    /// `OSM_SLAB_PATH=... cargo test --bin q2-cockpit --release \
    ///  overview_rule_comparison -- --ignored --nocapture`.
    ///
    /// Measures, per real overview tile, what each rule actually costs. The
    /// headline metric is **singleton-cell retention**: a row alone in its hip
    /// (z16) cell is a genuinely isolated feature by M4's own measure (Iceland's
    /// median hip occupancy is 1, Berlin's is 206), and losing it is losing the
    /// only evidence that a place exists. Extent coverage is reported too, and
    /// is deliberately shown NOT to discriminate — that was the vacuous metric.
    #[test]
    #[ignore = "needs the 1.2 GiB Berlin slab via OSM_SLAB_PATH"]
    fn overview_rule_comparison_on_the_real_bake() {
        let Some(bytes) = open_slab() else {
            panic!("set OSM_SLAB_PATH to the baked Berlin slab");
        };
        let slab = RowSlab::new(bytes).expect("real mmap is row-aligned");
        let morton = |i: usize| slab.morton_at(i);

        println!(
            "\n{:<12} {:>9} {:>7} {:>6} {:>19} {:>19}",
            "tile", "total", "budget", "cellz", "STRIDE kept/single", "CELL   kept/single"
        );

        // Berlin sits in these tiles at each overview zoom.
        let tiles: Vec<(u32, u32, u32)> = (8..=12)
            .map(|z| {
                let (x, y) = crate::osm_tiles::lonlat_to_tile(13.404954, 52.520008, z);
                (z, x, y)
            })
            .collect();

        for (z, x, y) in tiles {
            let range = slab.tile_range(z, x, y);
            let total = range.len();
            if total == 0 {
                continue;
            }
            let budget = row_budget(z);

            // Singleton hip cells: the isolated features, by M4's measure.
            let hip_bits = 16 * 2;
            let mut singleton: std::collections::HashSet<u64> = Default::default();
            {
                let mut prev: Option<(u64, usize)> = None;
                for i in range.clone() {
                    let p = cell_prefix(morton(i), hip_bits);
                    match prev {
                        Some((q, n)) if q == p => prev = Some((q, n + 1)),
                        Some((q, 1)) => {
                            singleton.insert(q);
                            prev = Some((p, 1));
                        }
                        _ => prev = Some((p, 1)),
                    }
                }
                if let Some((q, 1)) = prev {
                    singleton.insert(q);
                }
            }

            let kept_singletons = |sel: &[usize]| -> usize {
                let hit: std::collections::HashSet<u64> = sel
                    .iter()
                    .map(|&i| cell_prefix(morton(i), hip_bits))
                    .filter(|p| singleton.contains(p))
                    .collect();
                hit.len()
            };
            let extent = |sel: &[usize]| -> (f64, f64) {
                let pts: Vec<(f64, f64)> =
                    sel.iter().map(|&i| morton_to_lonlat(morton(i))).collect();
                let all: Vec<(f64, f64)> =
                    range.clone().map(|i| morton_to_lonlat(morton(i))).collect();
                let span = |v: &[(f64, f64)], f: fn(&(f64, f64)) -> f64| {
                    let (lo, hi) = v
                        .iter()
                        .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(f(p)), a.1.max(f(p))));
                    hi - lo
                };
                (
                    span(&pts, |p| p.0) / span(&all, |p| p.0),
                    span(&pts, |p| p.1) / span(&all, |p| p.1),
                )
            };

            let stride = stride_for(total, budget);
            let s_sel: Vec<usize> = range.clone().step_by(stride).collect();
            let c_sel = overview_sample(z, range.clone(), morton, budget);
            let cellz = choose_cell_zoom(z, range.clone(), morton, budget);

            let (sx, sy) = extent(&s_sel);
            let (cx_, cy_) = extent(&c_sel);

            println!(
                "{:<12} {:>9} {:>7} {:>6} {:>8}/{:<10} {:>8}/{:<10}",
                format!("{z}/{x}/{y}"),
                total,
                budget,
                cellz,
                s_sel.len(),
                format!("{}/{}", kept_singletons(&s_sel), singleton.len()),
                c_sel.len(),
                format!("{}/{}", kept_singletons(&c_sel), singleton.len()),
            );
            println!(
                "{:<12} {:>9} {:>7} {:>6} {:>19} {:>19}",
                "",
                "",
                "",
                "extent",
                format!("{sx:.4}/{sy:.4}"),
                format!("{cx_:.4}/{cy_:.4}"),
            );
        }
        println!();
    }
}
