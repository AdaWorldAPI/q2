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

- [ ] Add `osm-soa-bake` as a sibling path dependency
      (`path = "../openstreetmap-website-rs"`), matching the existing
      `../lance-graph`/`../OGAR` sibling convention in `q2/Cargo.toml`.
- [ ] New module `crates/cockpit-server/src/osm_features.rs`:
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
- [ ] **TDD**: write the test before the handler body. Anti-vacuity test
      per the original phase plan — build a small synthetic slab (same
      pattern `slab.rs`'s own tests use) spanning ≥2 sibling tiles at a
      given zoom, assert `tile_range` for adjacent tiles returns disjoint,
      non-empty index ranges. Verify it fails against a stub that returns
      the whole slab for every tile, then implement for real.
- [ ] Register the route in `main.rs` next to the existing `/api/osm/*`
      routes.
- [ ] `cargo build --workspace` + `cargo nextest run --workspace`.
- [ ] `cargo xtask verify --skip-hub-build` (Rust-only change, no
      hub-client/WASM surface touched).

## Phase 2 — GUI overlay in `/osm`

- [ ] `osm::PAGE`'s client JS: fetch `/api/osm/features/:z/:x/:y` for
      tiles in view, render as an SVG/canvas overlay of points on top of
      the raster basemap (start with points only — ways/relations need
      the tag codebook, deferred).
- [ ] Toggle to show/hide the feature overlay (mirrors the Garmin drape's
      `features` toggle).
- [ ] End-to-end verification per CLAUDE.md: run `q2-cockpit` with
      `OSM_SLAB_PATH` set to the real baked Berlin slab, load `/osm` in a
      browser (or headless screenshot), confirm real OSM points render at
      Berlin and the tile-boundary behavior is visually sane.

## Notes

- Disk: the 1.2 GiB Berlin slab is a local dev artifact under `/tmp`, not
  committed. A real deployment needs its own bake-hosting story (S3 /
  local volume) — out of scope for this wiring PoC; Phase 1 just needs
  *a* slab reachable via `OSM_SLAB_PATH`.
- Full `osm_id` recovery needs the `.soa.books` identity codebook sidecar,
  not just `read_identity`'s ordinal. Deferred until a consumer actually
  needs it — Phase 1's response uses ordinal + entity_type + position,
  sufficient to prove the wiring and plot points.
