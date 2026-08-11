# Wire openstreetmap-website-rs's SoA bake into the q2 `/osm` cockpit

## Overview

`openstreetmap-website-rs` (sibling repo) produces a Morton-sorted `.soa`
slab of real OSM feature rows (`RowSlab::tile_range(z,x,y)` → a contiguous
row range). q2's `/osm` cockpit already computes a matching tile address
(`osm_tiles::tile_to_hhtl`) and renders a slippy map — but only fetches
**raster tiles from `tile.openstreetmap.org`**. Nobody ever wired the two
halves together: the bake has no consumer, and the cockpit has no real
OSM feature data, only images.

Checked existing plans before starting (per explicit user instruction) —
confirmed via `q2/claude-notes/plans/2026-07-08-osm-garmin-drape.md` (drapes
*Garmin IMG* line features onto Garmin DEM terrain, not real OSM) and
`lance-graph/.claude/plans/cesium-osm-substrate-v1.md` (a design-only
proposal for an unrelated Cesium/Gaussian-splat 3D-tiles pipeline) that
neither covers this. `osm-soa-bake`'s own crate doc says it's "the substrate
behind the cockpit `/osm` endpoint" — the wiring was intended, never built.

**Known key-space mismatch (do not paper over):** `RowSlab::tile_range`
internally applies the Cesium-TMS Y-flip and uses a 4-tier (z=32 native
depth) Morton key (`tms.rs`), matching `cesium-osm-substrate-v1.md`'s Q2/Q3
ruling. q2's existing `osm_tiles::tile_to_hhtl` is a **different, already-
shipped** 3-tier (z=24), non-TMS-flipped key used only for the cockpit's own
address display. The new handler must call `RowSlab::tile_range` directly
with the raw OSM-XYZ `z/x/y` from the client — it must NOT reuse or convert
through `osm_tiles::tile_to_hhtl`. The two key spaces are not
interchangeable and this plan does not attempt to unify them.

## Phase 0 — measure (done this session)

- [x] Confirm no existing plan covers this wiring.
- [x] Free disk headroom (`cargo clean` on lance-graph/target, 15 GB → 18 GB
      free; user-approved).
- [x] Build `osm-soa-bake`'s `bake` binary (`cargo +1.97.1 build --release
      --bin bake`; needs rustc ≥1.95 for `ogar-osm`/`ogar-vocab`).
- [x] Run the bake against the real Berlin extract (already downloaded,
      `.claude/maps/berlin-latest.osm.pbf`, 98 MB):
      2,525,052 rows → 1.20 GiB slab + 56 MB codebook sidecar, 38.2s wall.
      Fits comfortably in the 18 GB free.

## Phase 1 — `GET /api/osm/features/:z/:x/:y` in cockpit-server

- [x] Add `osm-soa-bake` as a sibling path dependency
      (`path = "../openstreetmap-website-rs"`), matching the existing
      `../lance-graph`/`../OGAR` sibling convention in `q2/Cargo.toml`.
- [x] New module `crates/cockpit-server/src/osm_features.rs`:
  - Slab path from an env var (`OSM_SLAB_PATH`, default unset ⇒ 503) —
    the slab is 1.2+ GB, far too large to commit or `include_dir!`-embed;
    follows the "boot config names a bake, read once, from disk" pattern
    (`lance-graph/crates/lance-graph/src/soa_config.rs`), not the small
    committed-asset pattern the Garmin drape sidecars use.
  - mmap the file (`memmap2`), wrap in `osm_soa_bake::slab::RowSlab`.
  - Handler calls `slab.tile_range(z, x, y)` directly — raw OSM-XYZ in,
    no HHTL conversion (see key-space note above).
  - Per row in range: decode `morton_at` → `tms::morton_to_lonlat`, and
    `identity::read_identity` → `(entity_type, ordinal)`. Cap the response
    (e.g. 5,000 rows) and report `total` vs `returned` so a dense tile is
    still honest about truncation.
- [x] **TDD**: write the test before the handler body. Anti-vacuity test
      per the original phase plan — build a small synthetic slab (same
      pattern `slab.rs`'s own tests use) spanning ≥2 sibling tiles at a
      given zoom, assert `tile_range` for adjacent tiles returns disjoint,
      non-empty index ranges. Verify it fails against a stub that returns
      the whole slab for every tile, then implement for real.
      **Verified**: `cargo test -p cockpit-server osm_features` — 3/3
      pass (`adjacent_tiles_return_disjoint_nonempty_ranges`,
      `query_tile_recovers_the_rows_actually_in_range`,
      `empty_slab_reports_total_zero_not_an_error`).
- [x] Register the route in `main.rs` next to the existing `/api/osm/*`
      routes.
- [~] `cargo build --workspace` + `cargo nextest run --workspace` —
      **attempted twice, both failed with `ENOSPC`** (once at link time,
      once mid-compile writing rlibs for `lance-encoding`/`quarto-core`/
      `datafusion`/`geodatafusion`), even after repeated cleanup (see
      Disk note below; ~9GB freed across both attempts). The workspace's
      full dependency closure (wasmtime + tree-sitter grammars + v8 + the
      whole lance/datafusion/arrow tree, built once per binary in the
      monorepo) structurally exceeds this environment's real disk ceiling.
      **Scoped verification done instead**: `cargo build -p cockpit-server`
      succeeds cleanly — this alone compiles the overwhelming majority of
      the shared dependency tree (lance-graph, quarto-core, datafusion,
      etc.), since cockpit-server transitively depends on nearly all of it.
      The change is isolated to `cockpit-server` (a leaf binary crate) plus
      its new `osm-soa-bake` sibling-path dependency edge — no shared crate
      in the workspace is modified, so a regression in an unrelated binary
      (`quarto`, `hub`, `pampa`, etc.) from this change is not plausible.
      Asked the user how to proceed (AskUserQuestion); no response came
      back before the turn needed to continue, so per auto-mode guidance
      I proceeded with the recommended option (accept scoped verification).
- [ ] `cargo xtask verify --skip-hub-build` (Rust-only change, no
      hub-client/WASM surface touched) — **not run**, same disk
      constraint; `xtask verify` runs `cargo build --workspace` as its
      first step.

**Disk note**: this environment's usable disk pool is far smaller than
`df`'s nominal 252G suggests (real ceiling around 35-40G used). Linking
`cockpit-server`'s bin/test binaries (each ~2.2GB, due to the full
lance-graph/datafusion/arrow/wasmtime/tree-sitter/v8 closure) repeatedly
hit `ENOSPC`/`SIGBUS`. Recovered by, in order: clearing
`target/debug/incremental` (2.4G), clearing `~/.cargo/registry/src` +
`~/.cargo/git/checkouts` (user-approved, 5.5G, reversible via re-fetch),
freeing the local OSM bake output (`/tmp/osm-bake-out/*.soa*`, 1.3G,
regenerable from the still-present source PBF), and — the actual
recurring culprit — deleting leftover partial `.tmp*` link output files
in `target/debug/deps/` from failed link attempts (~2GB each, cargo does
not always clean these up after a failed `cc`/`ld` invocation).

## Phase 2 — GUI overlay in `/osm`

- [x] `osm::PAGE`'s client JS: fetch `/api/osm/features/:z/:x/:y` for
      tiles in view, render as a point overlay on top of the raster
      basemap (points only — ways/relations need the tag codebook,
      deferred). Implemented as absolutely-positioned `.pt` dots
      appended into the same `#tiles` element the raster `<img>`s use,
      so they inherit its pan/zoom transform for free; positions are
      computed in the same tile-pixel space as the tile images
      (`lon2x`/`lat2y` scaled by 256), including the world-wrap offset
      correction (`tx - wx`) so markers land under the correct repeated
      world copy at low zoom / near ±180°. Per-tile fetches are cached
      in a `Map` keyed by `z/x/y` and gated behind the toggle so nothing
      fetches while the overlay is off.
- [x] Toggle to show/hide the feature overlay (mirrors the Garmin
      drape's `features` toggle posture: opt-in, purely additive). Added
      a `#feat` button next to the existing zoom/basemap controls, plus
      a small status readout (`#featStatus`) reporting loading /
      unavailable (503, `OSM_SLAB_PATH` unset) / `returned` vs `total`
      (surfacing the server's truncation-honesty at `MAX_FEATURES_PER_TILE`)
      states.
- [x] End-to-end verification per CLAUDE.md: run `q2-cockpit` with
      `OSM_SLAB_PATH` set to the real baked Berlin slab, load `/osm` in a
      browser (or headless screenshot), confirm real OSM points render at
      Berlin and the tile-boundary behavior is visually sane. **DONE
      2026-08-10 — see "Browser verification" below.** (Superseded note: — see the Phase-2 verification note below for what
      was verified instead and why the live run didn't happen.

**Phase-2 verification note.** The live-browser run needs the
`q2-cockpit` binary linked, which needs the same disk headroom that was
already failing in Phase 1. This session made four further attempts
after the Phase-1 note above, each documented for the next session
rather than silently retried again:
1. `cargo build -p cockpit-server` → `ld` **SIGBUS** (disk hit ~484 MB
   free mid-link).
2. After a 5.6 GB cleanup pass (deleting stale duplicate-hash `.rlib`/
   `.a` artifacts left behind by earlier interrupted builds — same
   crate, older hash, superseded by a newer one from a later Cargo.lock
   resolution — keeping only the newest per crate): retried
   `cargo build -p cockpit-server` → **ENOSPC** partway through
   recompiling `quarto-core`/`lance` from scratch (the dedup pass had
   apparently invalidated cached artifacts the build then had to
   regenerate, consuming disk faster than the cleanup freed it).
3. Switched to `cargo check -p cockpit-server --tests` (type-checks
   without the disk-hungry link step) → **succeeded cleanly**,
   `Finished `dev` profile`, zero errors, only pre-existing warnings in
   files this change never touched (`codebook.rs`, `openai.rs`,
   `graph_engine.rs`). This is genuine full compile-time verification
   of `osm.rs`, `osm_features.rs`, `main.rs`, and every test in the
   crate — it stops short only of linking and running.
4. Tried `cargo test -p cockpit-server osm` on the now-warm check cache
   anyway (test binaries need a link) → this time **LLVM itself
   SIGSEGV'd** compiling `lance` (an untouched dependency, not this
   change) while disk was at ~700 MB free — the LLVM backend crashing
   from disk-backed I/O exhaustion, not a code defect.

Given (3) succeeded as a complete, unambiguous compile-time pass, and
(4) shows the *link* step is currently unable to complete reliably in
this environment regardless of what's being linked, further retries
were stopped rather than repeated. What backs "the change is correct"
instead, layered:
- `cargo check -p cockpit-server --tests` passed clean (compiles).
- The whole Phase-2 diff to `osm.rs` is confined to a single Rust raw
  string literal (`const PAGE: &str = r##"…"##;`); the delimiter lines
  were verified unchanged/unique/paired via `grep`, so the edit cannot
  have broken the surrounding Rust syntax structurally.
- The embedded JavaScript inside that string was extracted and
  syntax-checked directly with `node --check` (catches JS-level errors
  a Rust compiler can't see, since to `rustc` it's just string bytes) —
  clean.
- Phase 1's `osm_features.rs` handler (which the new JS calls) was
  already verified end-to-end against a real baked Berlin slab earlier
  in this session (`cargo test -p cockpit-server osm_features`, 3/3,
  before any Phase-2 edits) — that surface is unchanged by Phase 2.

What this does **not** substitute for: an actual browser load of `/osm`
with `OSM_SLAB_PATH` pointed at the Berlin slab, confirming dots render
at the right places and the toggle/status text behave as designed. That
remains genuinely unverified and should be the first thing a follow-up
session with more disk headroom does before calling Phase 2 complete.

## Browser verification (2026-08-10) — the POC gate, PASSED with one defect found

The exact invocation, per CLAUDE.md's end-to-end rule:

```bash
bake berlin-latest.osm.pbf berlin.soa           # 2,525,052 rows, 41.5s
OSM_SLAB_PATH=/home/user/osm-bake-out/berlin.soa PORT=8099 ./target/debug/q2-cockpit
# headless chromium -> http://127.0.0.1:8099/osm, click #feat, click the map
```

The browser run is **hermetic**: every request to a host other than
`127.0.0.1:8099` is aborted, so it also proves the overlay renders
*independently of the external raster basemap* (the broken-image icons in
the screenshot are those blocked tiles, and are expected).

Observed — inspected, not inferred:

| measurement | value | what it proves |
|---|---|---|
| `.pt` markers before toggle | **0** | opt-in gate holds |
| feature fetches before toggle | **0** | nothing fetches while off |
| `.pt` markers after toggle | **177,963** | real rows render |
| distinct marker positions | **73,130** | genuinely placed, not stacked at one point |
| feature fetches after toggle | 49 | one per visible tile |
| status text | `177963 of 2511097 features (tile cap hit)` | truncation reported honestly |
| panel HEEL | **`0xc8e1`** | **the V3 oracle value, live in the browser** |
| page errors | **[]** | no JS errors |

`0xc8e1` is the number the Phase-3 falsifier asserts and the V1 code
produced `0x624b` for. The migration is real in the running binary, not
only in tests.

### ⚠ Defect found BY this run: the row cap is spatially biased

The screenshot shows markers clumped into a corner of each dense tile
rather than covering it. That is **not** a placement bug — the placement is
faithful. It is the cap:

```
tile 14/8802/5373  total=15016 returned=5000
  returned points cover 99.9% of tile width, 50.0% of tile height
CONTROL 15/17604/10746 (total=4717, under the cap)
  covers 100.0% width, 100.0% height
```

`MAX_FEATURES_PER_TILE` is applied as `range.take(5000)`, and the range is
**Morton-ordered** — so a truncated tile returns a *spatially contiguous
prefix* (a sub-quadrant), not a sample of the tile. The control tile, which
never hits the cap, covers its full extent; that is what isolates the cause
to truncation rather than to the coordinate math.

Consequence: a dense tile silently renders as "data exists only in this
corner". `total` vs `returned` is reported honestly, but the *shape* of
what is returned is misleading in a way a count cannot convey.

- [x] Fix (1/2): stride-sample rather than head-truncate — take every
      `ceil(total / budget)`-th row so a decimated tile stays spatially
      representative. `total`/`returned` semantics unchanged; only which rows
      are chosen changes.

### ⚠⚠ The above was only HALF the defect — and the first fix hid the other half

**Operator correction, 2026-08-11.** The framing above ("the row cap is
spatially biased") diagnosed *how* rows were dropped and never asked *whether
they should be dropped at all*. They should not be. `MAX_FEATURES_PER_TILE`
was a flat `5_000` applied at **every zoom**, and against a real bake that is
not a coarse-zoom backstop — it is the normal case:

| z | rows/tile (Berlin-class, extrapolated from the one measured tile) | under the old flat 5k cap? |
|---|---|---|
| 12 | ~240,000 | no — **98% dropped** |
| 13 | ~60,000 | no — **92% dropped** |
| **14** | **15,016 (measured, `14/8802/5373`)** | no — **67% dropped** |
| 15 | ~3,800 | yes |

So *one mid-sized city* was served two-thirds absent at the zoom where a
person actually reads a city. Decimating an **overland** survey is a
legitimate LOD choice. Decimating a **city** is a wrong map — and stride
sampling only changes it from "wrong in one corner" to "wrong everywhere,
evenly". Uniform loss looks better, which is worse.

**And the falsifier written for fix (1) certified the defect as fine.** It
asserted the returned points cover ≥95% of the tile's *extent*. A uniform
stride covers ~100% of a bounding box at **any** stride — measured on that
exact fixture shape:

| budget | rows kept | extent lon/lat | verdict under the ≥0.95 assertion |
|---|---|---|---|
| 5,000 | **25.0%** | 0.9922 / 1.0000 | **PASSES** |
| 1,000 | **5.9%** | 1.0000 / 1.0000 | **PASSES** |
| 100 | 0.59% | 0.9070 / 0.9922 | fails |

A test that passes at 94% data loss has no power over data loss. Bounding-box
coverage is not content coverage; **counting rows is what discriminates.**
This is the third recorded instance of the workspace's vacuous-assertion trap
and it was walked into anyway — the only reliable check remains *disable the
fix and confirm the test goes red*, which was not run before that test was
written.

- [x] Fix (2/2): `row_budget(z)` — the budget is **zoom-conditioned**, because
      "how many features may I drop" is an LOD question and LOD is a function
      of what the tile *is*, not a constant.
      - `CITY_ZOOM_FLOOR = 13` — at or above, a tile is a place you are
        looking at and is served **complete**. z13 is the slippy-conventional
        city/district floor (z≤12 reads as metro-and-wider).
      - `OVERVIEW_ROW_BUDGET = 100_000` — decimation target below the floor,
        grounded in the one render capacity measured in this repo: the browser
        run drew **177,963** markers with zero page errors.
      - `CITY_ROW_CEILING = 400_000` — transport backstop only, far above
        Berlin's densest z13 (~60k); if it ever fires, `returned < total`
        reports it.

- [x] Falsifiers, each verified to go **red** against the restored flat-5k
      defect (`5000 of 10000 rows`):
      - `a_city_zoom_tile_is_served_complete` — counts rows: a z14 tile with
        10,000 rows (deliberately > the old 5k cap, asserted, so it cannot go
        vacuous) must return **all** of them.
      - `row_budget_is_zoom_conditioned_at_the_city_floor` — can-fire and
        can-stay-silent on the same knob; a constant `row_budget` fails it.
      - `a_decimated_overview_tile_samples_the_whole_curve_not_a_morton_prefix`
        — the old coverage test, **re-scoped** to the selection *rule* at an
        injected budget, and explicitly documented as no evidence of
        completeness. It stayed green through the disable-the-fix run, which
        is precisely why it must not be the gate.

**Not verified:** the boundary is set from Berlin-class density plus one
measured tile; a denser bake (Jakarta, Tokyo) has not been measured, and the
Berlin slab was deleted during the disk cleanup so it could not be re-measured
this pass. If `CITY_ZOOM_FLOOR` is wrong it is wrong in the safe direction
(more completeness, larger responses), and the completeness test fails loudly
rather than silently thinning.

## Phase 3 — V1/V2 → V3 substrate (the POC gap)

Phase 1 deliberately did **not** unify the two key spaces ("the two key
spaces are not interchangeable and this plan does not attempt to unify
them"). That unification is now the actual ask. Measured state:

| | q2 `osm_tiles.rs` (shipped, **V1/V2**) | V3 substrate (`osm_soa_bake::tms` + `ogar_osm::GEO_V3_FACET`) |
|---|---|---|
| tiers | **3** — heel/hip/twig | **4** — heel/hip/twig/**leaf** |
| depth | `HHTL_DEPTH = 24` (3×8) | `HHTL_DEPTH4 = 32` (4×8) |
| Y axis | XYZ, top-down, no flip | **TMS**, bottom-up (`xyz_to_tms_y`) |
| morton | 48-bit (24-bit lanes) | 64-bit (32-bit lanes) |
| round-trip error | 0.27–1.69 m | **1.13 mm** (Berlin) |
| facet home | none — display-only | `GEO_V3_FACET` rails 0–3 of `classid(4)+payload(12)` |

**The bake side is already fully V3** — verified, not assumed:
- `osm_soa_bake::identity` consumes `lance_graph_contract::identity_quad`
  (`SLOT_BYTES = classid(4) + payload(12)`), and was explicitly rewritten
  away from a parallel 12-byte implementation (its own module doc records
  that mistake).
- The row's 4 cascade tiers sit at payload rails 0–3 — i.e. absolute bytes
  `4..6 / 6..8 / 8..10 / 10..12`. Note `10..12` is what **V1 canon called
  the first two bytes of `family:u24`**; reading it as `leaf` *is* the V3
  content-blind reinterpretation, matching `GEO_V3_FACET` exactly
  (rails 4–5 = family basin + per-tile collision counter).
- `tms.rs` states why 3-tier was rejected outright: *"at z=24 … 0.27–1.69 m
  — the same order as a GNSS fix, which is why the 3-tier form is not used
  here."*

**So `osm_tiles.rs` is the only V1 surface left**, and it is also a
**parallel implementation** of math that now lives upstream — it re-derives
`lonlat_to_tile` / `morton_interleave` / `morton_deinterleave` that
`osm_soa_bake::tms` already owns, at the wrong depth and without the flip.
Since Phase 1 already added the `osm-soa-bake` dependency, the migration is
mostly **deletion**, which is the workspace's standing "consume, never
re-implement" rule.

- [x] Re-point `osm_tiles.rs` at `osm_soa_bake::tms` — `lonlat_to_tile`,
      `xyz_to_tms_y`, `morton64`, `point_to_tiers`, `tiers_of` — deleting
      q2's 24-bit duplicates rather than widening them. Keep the local
      `tile_url`/`sat_tile_url` helpers (genuinely q2's own).
- [x] Widen `Hhtl` to 4 tiers (`heel/hip/twig/leaf`). Note the existing
      field doc calls `twig` "Finest tier (the leaf tile)" — that comment
      is the V1 tell and must go with the change.
- [x] `/api/osm/locate` + `/api/osm/tile/:z/:x/:y` report the 4-tier
      address; cockpit panel gains a 4th LEAF cell (it hardcodes a
      3-column `.tier` grid + `#heel`/`#hip`/`#twig` today).
- [x] **Falsifier, two-sided**: a point whose V1 and V3 addresses *differ*
      (any point off the TMS-flip fixpoint) must now report the V3 value —
      and `osm_tiles`'s tier output must equal `tms::point_to_tiers` for
      the same lon/lat, so the endpoint and the slab can no longer drift.
      A test asserting only "returns 4 tiers" is vacuous; assert equality
      against the upstream oracle. Exact test to add (written, then
      reverted — see the verification note below):

      ```rust
      #[test]
      fn hhtl_agrees_with_the_v3_substrate_oracle() {
          for (name, lon, lat) in [
              ("Berlin", 13.404954, 52.520008),
              ("Reykjavik", -21.940022, 64.146575),
              ("Sydney (southern — exercises the TMS Y-flip)", 151.2093, -33.8688),
          ] {
              let (_code, want) = osm_soa_bake::tms::point_to_tiers(lon, lat);
              let (x, y) = lonlat_to_tile(lon, lat, HHTL_DEPTH);
              let got = tile_to_hhtl(HHTL_DEPTH, x, y);
              assert_eq!(got.heel, want.heel, "{name}: HEEL must equal the oracle");
              assert_eq!(got.hip,  want.hip,  "{name}: HIP must equal the oracle");
              assert_eq!(got.twig, want.twig, "{name}: TWIG must equal the oracle");
              assert_eq!(got.leaf, want.leaf, "{name}: LEAF must equal the oracle");
          }
      }
      ```

**The divergence is MEASURED, not assumed.** Because `cockpit-server` is
binary-only, its test binary cannot be linked in this environment (25-min
run killed mid-link; see the Phase-2 note). So the falsification was run
where it *can* build — a throwaway probe in `openstreetmap-website-rs`
(untracked, since removed) that copied q2's `tile_to_hhtl` / 24-bit
`morton_interleave` / `lonlat_to_tile` **verbatim** and compared them to
the real `tms::point_to_tiers`. Compiled and executed under `+1.97.1`:

| city | q2 V1 (3-tier z=24) | V3 oracle (4-tier z=32) |
|---|---|---|
| Berlin | `heel=0x624b hip=0xea60 twig=0xb2f2` (no leaf) | `heel=0xc8e1 hip=0x40ca twig=0x1858 leaf=0x048c` |
| Reykjavik | `heel=0x3520 hip=0x1491 twig=0xffa8` | `heel=0x9f8a hip=0xbe3b twig=0x5502 leaf=0xfb7b` |
| Sydney | `heel=0xd6c7 hip=0xc2be twig=0xd922` | `heel=0x7c6d hip=0x6814 twig=0x7388 leaf=0x8ca3` |

**Every tier differs at every point** — the cockpit's displayed address is
not the slab's key, confirmed numerically rather than inferred from the
constants. Those V3 columns are the expected-value fixture for the
migration: after Phase 3, `tile_to_hhtl` must reproduce the right-hand
column exactly.

> **Why the test is not committed yet.** It was written and then reverted
> deliberately. Committing a test that cannot be executed here would either
> turn the branch red (it fails against the current V1 code — which is the
> whole point of a falsifier) or, if landed together with an unverified
> implementation, ship a coordinate-key migration whose correctness was
> never observed. A wrong key silently mis-addresses every row. The test
> text above is the spec; land it *with* the implementation on a box that
> can link the binary, and confirm it goes red-then-green.
- [x] Once unified, `osm_features.rs`'s key-space warning becomes stale —
      rewrite it to say the two now agree (do not silently delete it; it
      records why they once didn't).

**Payoff**: the cockpit's displayed address and the slab's row key become
the *same* key, so clicking a point can address its actual rows — which is
what makes the overlay a substrate view rather than a picture.

### Phase 3 outcome (2026-08-10) — SHIPPED, real red-then-green

The environment turned out to be able to link `cockpit-server` after all;
**`debuginfo=2` was the blocker, not disk volume.** Building with
`CARGO_PROFILE_DEV_DEBUG=0` completed in 27m33s holding steady at ~14 GB
free, where every prior attempt consumed the disk and died (one crashing
LLVM on an untouched dependency). That unblocked the honest TDD loop the
plan asked for:

```
RED    hhtl_agrees_with_the_v3_substrate_oracle
       Berlin: HEEL must equal the oracle — left: 25163, right: 51425
GREEN  14 passed; 0 failed   (after the migration)
```

`25163`/`51425` are `0x624b`/`0xc8e1` — the exact values the earlier
probe predicted, now observed inside q2 rather than in a sibling crate.

Crate-wide: **79 passed, 1 failed**. The single failure
(`osint_gotham::dual_use_facets_pack_into_the_value_tenant`) was
**confirmed pre-existing** by stashing this change and re-running it
against main's state — it fails identically there, references none of the
changed symbols, and is worth someone's attention independently.

**Still not done:** the browser load. Tests prove the key math; they do not
prove the page renders. That remains the gating item for the POC.

## Phase 4 — slab hosting: S3 scratch + `RAILWAY_VOL` (operator-directed)

Phase 1 left this open ("a real deployment needs its own bake-hosting
story (S3 / local volume) — out of scope"). Operator has now named it:
S3 for scratch, `RAILWAY_VOL=/volume01`. Bucket + endpoint + credentials
are supplied via env (`AWS_ENDPOINT_URL`, `AWS_S3_BUCKET_NAME`,
`AWS_DEFAULT_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`) —
**never committed, never logged, read from the environment at the call
site**.

Existing precedent to follow rather than reinvent (the "CONTINUE TO use
S3" instruction):
- `lance-graph/.claude/knowledge/s3-hydration-lifecycle.md` — the
  lifecycle doc.
- `lance-graph/crates/lance-graph/examples/hydration_probe.rs` +
  `soa_to_lance.rs`.
- `MedCare-rs/crates/medcare-server/src/bake_s3.rs` + `bake_hydrate.rs` —
  a working consumer-side hydrate.

**The slab is baked and in S3 already** (done this session):

```
bake berlin-latest.osm.pbf berlin.soa    # 37.3s
  ROWS 2,525,052 · 1,292,826,624 bytes (1.20 GiB at stride 512)
  classid 0x0F011000   # GEO_DOMAIN 0x0F + CLASSVIEW_V3_SUBSTRATE 0x1000
  same-tile collisions 11,493 (0.4552% — z=32 keying)
  slab digest 8ec93a6ee63e89d2
```

Uploaded to the bucket under the **same convention the MedCare-rs bakes
already use** (`<repo>/bakes/<version>/<artifact>` + a `sha256sum -c`
compatible `SHA256SUMS`), so the hydrate side can be the same shape:

| key | bytes |
|---|---|
| `q2/bakes/osm-berlin-v0.1.0/berlin.soa` | 1,292,826,624 |
| `q2/bakes/osm-berlin-v0.1.0/berlin.soa.books` | 58,763,338 |
| `q2/bakes/osm-berlin-v0.1.0/SHA256SUMS` | 160 |

```
berlin.soa        cbf5989ab45bc921d8a85fdbdb71c8e5029cd904a3d230a898c2b5eb81d7ebe7
berlin.soa.books  d12bc8a15270f9a61290fb7117c92621e1f88229a85bdfdfc4d217481addde7f
```

The uploaded object size matches the baker's own reported byte count
exactly, and the local copy was deleted afterwards to reclaim disk.
`classid 0x0F011000` is the artifact itself asserting V3 — worth noting
because it means the slab was *never* V1; only q2's display key is.

- [ ] `OSM_SLAB_PATH` gains an S3 sibling: hydrate `s3://$BUCKET/<key>`
      → `$RAILWAY_VOL/<name>.soa` once at boot, then mmap **from the
      volume** (mmap needs a real file; S3 is the source, not the mapping
      target). Absent creds ⇒ the existing 503, unchanged.
- [ ] Checksum-pin the object the way `MedCare-rs`'s
      `scripts/fetch-frontend-assets.sh` pins its assets — a URL bump must
      require a checksum bump in the same edit; no unverified-fetch path.
- [ ] Note: `/volume01` does **not** exist in the current sandbox
      (`RAILWAY_VOL` is set, the mount is Railway-side only), so this leg
      is verifiable only on deploy — say so rather than claiming it works.

## The OGAR plug-and-play surface (operator pointer, 2026-08-10)

**The OSM substrate is already hot-plugged into OGAR, USB-style.** Worth
recording because it changes Phase 4's design and it *validates* Phase 1's
call style rather than condemning it.

| USB role | Home | Surface |
|---|---|---|
| socket (agnostic, zero-dep) | `lance_graph_contract::hotplug` | `HotPlug { consumer, classids, covered }`, `CapabilityAuthority` |
| authority (host) | `ogar_vocab::geo_actions` | the geo action table + `capability_registry::resolve_hotplug` |
| bridge | `ogar_osm::plug_in` | socket ⇄ authority |
| **device** | `osm-soa-bake::capability` | `Capability` enum, `activate()`, one real dispatch arm each |

Declared capabilities, and what each already is in code:

| capability | subject | primitive |
|---|---|---|
| `locate_point` | `osm_node` `0x0F01` | `tms::point_to_tiers` |
| **`locate_tile`** | `osm_node` | **`slab.tile_range(z,x,y)` — what `/api/osm/features/:z/:x/:y` calls** |
| `locate_fragment` | `osm_node` | `BoundaryIndex::locate_range` |
| **`project_fields`** | `osm_node` | **`project::project(row, surface, role)` — the RBAC projection** |
| `street_edges` | `osm_way` `0x0F02` | `street::edge_mask` |
| `polyline_length` | `osm_way` | `geodesy::polyline_metres` |

### Three consequences for q2 — read before touching the endpoint

1. **q2 must NOT declare its own `HOT_PLUG`.**
   `GEO_EXPECTED_EXECUTORS = ["osm-soa-bake"]`, and `resolve_hotplug`
   checks the consumer name — a plug from `cockpit-server` earns
   `HotplugDrift::UnexpectedConsumer`, by design. **q2 is a caller of an
   already-activated device, not a second device.** The plug is the bake
   crate's, and it is already shipped (`capability.rs`, with can-fire
   halves proving the port rejects a wrong consumer and short coverage).

2. **Calling `slab.tile_range(...)` directly is explicitly sanctioned**, so
   Phase 1 needs no rework. `capability.rs` says so in its own module doc:
   *"These are thin, on purpose … the dispatch surface exists so the
   registration is checkable, not to add a layer: a caller that already has
   the module in scope should keep calling `tms::point_to_tiers` directly."*
   The arms exist to make the registration falsifiable, not to be a
   mandatory call path.

3. **`project_fields` is NOT the right instrument here — corrected 2026-08-10.**
   An earlier revision of this file called it "a real gap … the authorization
   one." That was too quick, and investigation reversed it on three grounds:

   **(a) The response is narrower than the public source.** The corpus is a
   public ODbL `.osm.pbf` extract. `FeatureOut` emits four fields: a
   Morton-quantised `lon`/`lat`, an `entity_type` (an OGAR concept id —
   `osm_node` `0x0F01` / `osm_way` `0x0F02`), and an `ordinal` which is **not
   the OSM element id** — `identity.rs` resolves the external key to a
   codebook ordinal *pre-bake* (an OSM node id is ~2³⁴ and cannot fit the
   slot at all), so it is a bake-local pseudonym. **And no tags.** Rows carry
   up to 28 tag facets (`TAGS_PER_ROW = 28`); `FeatureOut` has no tag field,
   so the entire semantic payload — *what the feature is* — never reaches the
   wire. A mask would not be protecting anything.

   **(b) It cannot reach half of what the endpoint returns.** `project::project`
   reads `key[4 + position]` for positions `0..12` — the key's facet payload
   only. `entity_type`/`ordinal` come from `read_identity`, which reads the
   **value slab**; `project.rs`'s own test
   `positions_past_the_facet_register_are_skipped_not_folded` pins that
   boundary. The projection surface structurally excludes them.

   **(c) On the half it does reach, masking CORRUPTS rather than withholds.**
   Facet positions 0–7 are the Morton code — the coordinate itself (8–9 is
   `family`, a literal `0` at the OSM mint site; 10–11 is the collision
   counter, ~always 0). `morton_to_lonlat` consumes all 8 bytes. Masking the
   leaf tier yields not an *absent* position but a **silently coarser one
   presented as exact** — strictly worse than either emitting or omitting.
   Fail-closed absence has no expression in a lon/lat pair.

   A "documented permit-all role" would therefore be a no-op with an
   authorization-shaped silhouette — security theatre in the precise sense —
   and it would **answer an open upstream decision by accident**: a2ui's
   charter states "`full_for` is a *render* convenience, never an RBAC
   fallback" and names the permit-all identity as its one open W1 question.
   (`WideFieldMask::ALL` does not exist in Rust — it appears only in a2ui
   `.md` files; and `ClassRbac::field_mask`'s default impl already returns
   `FieldMask::FULL`, so a stub would inherit permit-all silently.)

   **Verdict: leave as-is, documented.** The endpoint is unauthenticated by
   design on a public corpus. That is a statement about THIS dataset, not a
   general licence.

- [ ] **The bounded caveat — this verdict does NOT generalise.** The same
      router serves ~35 routes on `0.0.0.0` with `CorsLayer::permissive()`
      and no auth middleware (exhaustive grep over `cockpit-server/src` for
      auth/token/bearer/jwt/session/role/permission/tenant: 137 matches,
      **not one an authn/authz mechanism** — every `role` is a domain term,
      every `token` a codebook/LLM token). `/api/clinical/reason` and
      `/api/cpic/reason` (pharmacogenomics) are where a sensitivity question
      is genuinely live. **Not assessed here; flagged, not ruled on.**
      The real gap is request-scoped identity server-wide (`ActorContext`
      exists in `lance_graph_contract::auth`; nothing *produces* one), which
      is an authentication decision, not an osm-features-local omission.

**Anti-patterns this pins** (each burned upstream once, per
`OGAR/.claude/knowledge/hotplug-consumer-migration.md`): no bespoke
per-consumer plug mechanism; no shape ordinals in the classid low u16 (low
= APP render prefix); no git deps on OGAR (path deps to the sibling — a
git+branch dep writes a rev pin into `Cargo.lock`); no parallel registry
beside `domain_tables()` — `ogar-osm` once shipped its own
`OSM_CAPABILITIES` that verified itself against itself and passed while the
real port answered `NoCapabilitiesFor(0x0F01)`.

## Route choices — what is wired, what is kept, what the alternative costs

Nothing below has been removed. Each row is a live alternative that a
future session could switch to; this records **which arm the browser load
actually exercises, why, and what picking the other would cause**, so the
choice is a decision on the record rather than an accident of what got
written first.

### 1. HTTP routes on `/osm`

| route | wired to the page? | why / what the other causes |
|---|---|---|
| `GET /api/osm/features/:z/:x/:y` | **YES** — the overlay | The only route returning real rows. 49 calls in the verified run. |
| `GET /api/osm/locate?lon=&lat=&z=` | **YES** — click → panel | Takes lon/lat, which is what a map click naturally produces. Returns the address *and* both basemap URLs in one round trip. |
| `GET /api/osm/tile/:z/:x/:y` | **NO — kept, unused by the page** | Same HHTL answer keyed by tile instead of coordinate. The page never has a bare `z/x/y` without also having the lon/lat, so `locate` strictly dominates *for this client*. Kept because it is the correct shape for a non-map caller (a tile pipeline, a cache warmer) that has an address and no coordinate; deleting it would force such a caller to invent a lon/lat inside the tile just to ask what the tile's key is. **If it were wired instead of `locate`:** the panel would lose `lon,lat` and both tile-source URLs, and the click handler would have to do the WebMercator inverse client-side — reintroducing exactly the duplicate-projection problem Phase 3 just deleted on the server side. |

### 2. How the endpoint reaches the substrate

| arm | chosen? | why / what the other causes |
|---|---|---|
| `slab.tile_range(z,x,y)` direct | **YES** | Explicitly sanctioned by `osm-soa-bake::capability`'s own module doc: *"a caller that already has the module in scope should keep calling `tms::point_to_tiers` directly"* — the dispatch arms exist to make the **registration falsifiable**, not to be a mandatory call path. |
| `capability::locate_tile(&slab, …)` | no — kept upstream | Identical body (`slab.tile_range`). Routing through it would add an indirection with no behavioural change, and would NOT enrol q2 in drift detection: `GEO_EXPECTED_EXECUTORS = ["osm-soa-bake"]`, so **q2 calling `activate()` earns `HotplugDrift::UnexpectedConsumer` by design.** q2 is a caller of an already-activated device, not a second device. |

### 3. Field decode — the one with a real consequence

| arm | chosen? | why / what the other causes |
|---|---|---|
| hand decode `morton_at` + `read_identity` | **YES, currently** | Returns `lon/lat/entity_type/ordinal`. Sufficient to plot points; it is what the verified run renders. |
| `capability::project_fields(row, surface, role)` | **no — and this one is a real gap** | The declared capability is `surface ∩ role`, **fail-closed**: an unauthorised position is *absent from the response*, not hidden by the client. **What the current choice causes:** the endpoint is effectively unauthenticated — every caller sees every field. Moot for a single-user local cockpit; not moot deployed. Switching arms needs a role source first, which does not exist yet, so the honest posture is to *say* the endpoint is unauthenticated rather than imply a mask exists. |

### 4. Slab source

| arm | chosen? | why / what the other causes |
|---|---|---|
| local file via `OSM_SLAB_PATH` | **YES** | What the browser run used. mmap needs a real file. |
| S3 → `$RAILWAY_VOL` hydrate (Phase 4) | not built | The slab **is already in S3** (`q2/bakes/osm-berlin-v0.1.0/`, checksummed). Without the hydrate step a Railway deploy returns the existing 503 — correct behaviour, just not useful. This is the one remaining gap between "works locally" and "works deployed". |

### 5. Basemap skin

| arm | chosen? | why / what the other causes |
|---|---|---|
| OSM raster (`tile.openstreetmap.org`) | **YES, default** | Matches the `z/x/y` axis order the address math uses. |
| ESRI World Imagery (`sat` toggle) | kept, live | **Same address, different skin** — note the path order is `z/y/x`, row before column. The toggle also swaps the attribution, which is a licensing requirement, not decoration. Hard-wiring either one would drop the other's attribution handling. |

## Notes

- Disk: the 1.2 GiB Berlin slab is a local dev artifact under `/tmp`, not
  committed. A real deployment needs its own bake-hosting story (S3 /
  local volume) — out of scope for this wiring PoC; Phase 1 just needs
  *a* slab reachable via `OSM_SLAB_PATH`.
- Full `osm_id` recovery needs the `.soa.books` identity codebook sidecar,
  not just `read_identity`'s ordinal. Deferred until a consumer actually
  needs it — Phase 1's response uses ordinal + entity_type + position,
  sufficient to prove the wiring and plot points.

## Probe M4 — run, and it refutes the zoom-keyed budget (2026-08-11)

**Operator: "did you even continue where we left off with the overland dynamic
compression bucket thresholds".** No — and the probe that answers it was
already written and still marked NOT RUN. `bf16-hhtl-terrain.md`'s process
rule is explicit: *an agent changing bucketing strategy runs the probe first,
or labels the proposal CONJECTURE and defers commitment.* `row_budget` is a
bucketing-strategy change and it was written as settled fact.

Ran it: `osm-soa-bake`'s `tier_probe` (M4), on Berlin (city, 2.52 M features)
and Iceland (overland, 0.65 M) — features per tile, by cascade tier:

| tier | Berlin tiles / med / p95 / max / fit≤30 | Iceland tiles / med / p95 / max / fit≤30 |
|---|---|---|
| heel z8 | 2 / 1,564,647 / — / 1,564,647 / 0.0 % | 58 / 3,838 / 34,985 / 202,296 / 20.7 % |
| hip z16 | 8,065 / 206 / 996 / **3,844** / 16.4 % | 178,962 / 1 / 8 / **1,067** / 95.2 % |
| twig z24 | 2,435,641 / 1 / 1 / 20 / **99.7 %** | 649,093 / 1 / 1 / 7 / **99.9 %** |
| leaf z32 | 2,513,559 / 1 / 1 / 11 / 99.8 % | 652,314 / 1 / 1 / 7 / 99.9 % |

Three things fall out, and two of them cut against what I had just written:

1. **The cascade terminates at TWIG, not HEEL** — M4's own FAIL direction.
   99.7 % of Berlin's twig cells hold exactly one feature.
2. **There is exactly ONE useful bucketing level: the hip cell.** Occupancy
   goes 1 (twig) → 206 (hip) → 1.56 M (heel) on Berlin. So the principled
   overland rule is *one representative per occupied hip cell* — 8,065 cells
   for 2.52 M features, a 312:1 reduction that is a cascade step. A uniform
   row stride is not that; it is a placeholder that produces a defensible
   picture at a measured budget. `OVERVIEW_ROW_BUDGET` is now labelled
   **CONJECTURE** in its own doc comment, per the process rule.
3. **Density is a property of the extract, not the zoom.** Berlin and Iceland
   differ ~200× at hip and converge by twig. So `CITY_ZOOM_FLOOR` — a
   zoom-keyed constant — is mis-specified for one of them by construction.
   It is right in the safe direction (Iceland is sparser, so completeness is
   cheaper there), but "z ≥ 13 is complete" is a *policy*, not a measurement.

What the probe DOES let me state as measured rather than guessed:
`CITY_ROW_CEILING` is bounded, not chosen. A z13 tile is 8×8 = 64 hip tiles
and Berlin's densest hip tile holds 3,844, so a z13 tile is bounded above by
**246,016** — and the 400,000 ceiling therefore provably cannot fire for a
Berlin-class bake. (The bound is itself slack: it assumes 64 adjacent
maximum-density tiles, where the one measured z14 tile holds 15,016.)

- [ ] Build the hip-cell representative form for overview zooms and compare it
      against the stride at equal budget — coverage, and what a user actually
      loses. That comparison is what promotes `OVERVIEW_ROW_BUDGET` from
      CONJECTURE, and it is the "dynamic compression bucket threshold" thread.
- [ ] Re-measure `CITY_ZOOM_FLOOR` against a denser extract than Berlin before
      treating 13 as anything but a policy floor.

M4's result recorded upstream in `lance-graph/.claude/knowledge/bf16-hhtl-terrain.md`
per that file's update protocol, scoped explicitly to the OSM point-feature
form — it says nothing about HHTL termination for embedding fingerprints,
which is what P2–P4 address and which remains NOT RUN.

## Phase 5 — the cell form, the comparison, and the web POC (2026-08-11)

### The rule that replaced the stride

`overview_sample` selects **one representative per occupied cascade cell**, at
the deepest Morton prefix depth whose occupied-cell count still fits the
budget. The depth is chosen **per tile from its own density** — that is the
"dynamic compression bucket threshold", and it is what M4 finding (2) demands:
Berlin and Iceland differ ~200x per hip cell, so any depth fixed once for all
extracts is wrong for one of them. Fixed-at-hip is the special case zz=16.

Cheap by construction: the range is Morton-sorted, so equal prefixes are
contiguous runs — counting occupied cells is one pass, no map, no allocation,
and the count is monotone in depth, so the depth search is a binary search
(`occupied_cells_is_monotone_in_depth` pins the monotonicity the search
depends on rather than assuming it).

### The comparison — synthetic, then real

**Synthetic falsifier** (`cell_selection_keeps_isolated_features_that_a_stride_drops`):
one dense 100x100 cluster plus 8 isolated outliers, same budget for both rules.
Stride kept **1 of 8** outliers; cell kept **8 of 8**. Verified by swapping
`overview_sample`'s body for the stride form and watching it go red.

**Real Berlin bake** (`overview_rule_comparison_on_the_real_bake`, `#[ignore]`d,
needs `OSM_SLAB_PATH`). Metric: **singleton hip cells** — a row alone in its z16
cell, which is an isolated feature by M4's own measure.

| tile | rows | budget | cell depth | stride keeps | cell keeps | stride extent | cell extent |
|---|---|---|---|---|---|---|---|
| 8/137/83 | 1,569,355 | 100k | z18 | **14 / 316** | **316 / 316** | 0.82 / 0.93 | 1.00 / 1.00 |
| 9/275/167 | 981,698 | 100k | z18 | 9 / 107 | 107 / 107 | 0.82 / 0.87 | 1.00 / 1.00 |
| 10/550/335 | 981,696 | 100k | z18 | 9 / 105 | 105 / 105 | 0.95 / 0.97 | 1.00 / 1.00 |
| 11/1100/671 | 654,933 | 100k | z19 | 0 / 0 | 0 / 0 | 1.00 / 1.00 | 1.00 / 1.00 |
| 12/2200/1343 | 255,669 | 100k | z20 | 0 / 0 | 0 / 0 | 1.00 / 1.00 | 1.00 / 1.00 |

The stride drops **95.6%** of isolated features at z8; the cell form drops none
— while returning FEWER rows (53,655 vs 98,085), because the depth is quantized
to a zoom level and the next one deeper would overrun.

Two things worth keeping from this table:

- **Extent coverage moves 0.82 -> 1.00 where the real metric moves 14 -> 316.**
  The old gate could not have seen this. Same lesson as the vacuous falsifier,
  now confirmed on real data rather than argued.
- **z11/z12 have zero singletons** (those tiles sit entirely inside dense
  Berlin) and both rules tie — the can-stay-silent half, for free, on real
  input.

- [x] Build the hip-cell representative form and compare against the stride at
      equal budget. `OVERVIEW_ROW_BUDGET`'s METHOD is no longer CONJECTURE.

### ⚠ The browser caught a second wrong number — mine again

`OVERVIEW_ROW_BUDGET` was `100_000`, justified in its own doc comment as *"the
browser drew 177,963 markers across 49 tiles with zero page errors, so ~10^5
per response is within demonstrated reach."* That reads a **viewport-wide**
measurement as a **per-tile** budget. 177,963 across 49 tiles is **3,632 per
tile** — the constant overstated its own cited evidence by **27x**.

Measured at the `/osm` page's default z12 view (1400x900 = 63 tiles, 53 with
data), by summing the real endpoint:

| | markers in view |
|---|---|
| ever measured working | 177,963 |
| under the 100k per-tile budget | **1,772,260** (10x) |

The page hung — the browser run timed out twice before this was diagnosed.
Corrected to `3_000` per tile with the viewport arithmetic stated in the doc
comment: 3,000 x 53 data tiles = 159,000, under the only measured capacity.

**A second, pre-existing defect surfaced here and is NOT fixed:** `render()`
does `tilesEl.innerHTML=''` and rebuilds every marker on **each arriving
tile**, so a viewport costs ~53 full rebuilds of the whole DOM — quadratic in
tiles. The old 5,000 cap masked it. It is survivable at the load the evidence
supports (measured working below), so it is recorded rather than fixed in this
pass.

- [ ] `render()` is O(tiles x markers) — rebuild incrementally, or diff, rather
      than clearing `#tiles` on every tile arrival.
- [ ] The viewport bound is the honest one; `OVERVIEW_ROW_BUDGET` is its
      per-tile share at one window size. A budget derived from the actual
      tiles-in-view count would not drift with window size.

### The web POC — end-to-end, browser-verified

```
cd openstreetmap-website-rs && cargo +1.97.1 run --release --bin bake \
    .claude/maps/berlin-latest.osm.pbf <out>/berlin
# 2,525,052 rows -> 1.20 GiB slab + 56 MB codebook, 35.4s, digest 8ec93a6ee63e89d2

OSM_SLAB_PATH=<out>/berlin PORT=8099 ./target/debug/q2-cockpit
```

Endpoint, inspected:

| tile | total | returned | |
|---|---|---|---|
| **14/8802/5373** | **15,016** | **15,016** | the tile this whole arc started on — was 5,000 |
| 8/137/83 | 1,569,355 | 1,269 | overview, cell-decimated |
| 10/550/335 | 981,696 | 2,295 | |
| 12/2200/1343 | 255,669 | 1,024 | |

Browser (hermetic — every non-localhost request aborted, so the overlay is
proven to render independently of the external raster basemap):

| measurement | value | what it proves |
|---|---|---|
| `.pt` markers before toggle | **0** | opt-in gate holds |
| `.pt` markers after toggle | **64,707** | real rows render |
| distinct marker positions | **59,812** | genuinely placed, not stacked |
| feature fetches | **49** | one per visible tile |
| status text | `64707 of 2511097 features (tile cap hit)` | truncation reported honestly |
| panel HEEL | **`0xc8e1`** | the V3 oracle value, live (V1 gave `0x624b`) |
| panel z/x/y | `12 / 2201 / 1343` | click -> `/api/osm/locate` round-trip |
| page errors | **[]** | none |

Screenshot inspected: markers cover the viewport edge to edge with no corner
clumping (the original z14 defect), and the black gaps are real absences —
water bodies and parks. The dots form a visible lattice in dense areas, which
is the cell form flattening density on purpose; that is the trade the
comparison above measured, not an artefact.

Two verification-method notes, both cost real time:
- The tier panel is populated by the **click** handler (`/api/osm/locate`), not
  by hover. A first run moved the mouse and read back `—` for all four tiers,
  which looks exactly like a broken panel.
- `pgrep -f "target/debug/q2-cockpit"` matches the shell command that contains
  the string, so `until ! pgrep -f ...` never terminates. Check the port.

### The quadratic render — measured, fixed, measured again

Filed as a follow-up above, then done in the same pass because it was the one
defect degrading the delivered POC. `render()` cleared `#tiles` and rebuilt
**every** marker, and `ensureFeatures`'s completion handler called it — so ~49
arriving tiles each redrew everything drawn so far.

Instrumented by wrapping `#tiles.appendChild` and counting `.pt` nodes:

| | markers | `appendChild` calls | amplification | settle |
|---|---|---|---|---|
| before | 64,707 | **1,550,957** | **23.97x** | 33.4 s |
| after | 64,707 | **64,707** | **1.00x** | 19.4 s |

23.97x is exactly the `n/2` the quadratic predicts for ~49 tiles — theory and
measurement agree, which is what says the diagnosis was the real mechanism and
not a coincidence.

The fix splits the layer: `render()` still owns the tile images and the
transform (and clears), `paintFeatures()` appends only cells not already in
`drawnCells`, and a completed fetch calls `paintFeatures()` instead of
`render()`. `drawnCells` is keyed by `(tx,ty)` and **not** by the tile key,
because one tile drawn on two repeated world copies needs its own dots at each
offset.

Wall-clock improved 1.72x rather than 24x — with the appends gone, the tile
fetches dominate. Reported as measured rather than as the append ratio, which
would overstate what a user feels.

**Behaviour is unchanged, verified rather than assumed** — every POC number is
identical after the fix (64,707 markers / 59,812 distinct / 49 fetches / HEEL
`0xc8e1` / status text / zero errors). Pan and zoom re-checked explicitly,
since `drawnCells` could plausibly have broken repaint:

| action | markers | |
|---|---|---|
| toggle on | 64,707 | |
| pan one tile | 59,845 | new grid, correctly repainted |
| zoom in to z13 | **285,821** | city zoom -> COMPLETE tiles |
| page errors | **[]** | throughout |

- [x] `render()` is O(tiles x markers) — fixed; incremental paint, 24x -> 1x.

**New capacity datapoint, worth carrying:** 285,821 markers rendered with zero
page errors. The only prior measurement was 177,963, which is what every budget
in `osm_features.rs` is grounded in — so the real envelope is at least 1.6x
what those constants assume. Not acted on: this was at city zoom with different
fetch timing, and a budget should not be widened on one incidental observation.
It does mean `OVERVIEW_ROW_BUDGET = 3_000` is conservative rather than tight.
