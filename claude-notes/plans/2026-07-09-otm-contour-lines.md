# OTM-style contour lines + lake-vs-river fix (canyon)

Branch: `claude/geo-contour-lines` (off `main` after #97 merged).

## Overview
Push the `/garmin` terrain toward OpenTopoMap quality by adding a **contour-line
overlay** (the biggest single OTM-look element), and tighten the canyon by
**widening only the flowing river** so plateau lakes shrink back to OTM pinpoints.
Both from data we already bake — no new download.

## Key findings (probe, 2026-07-09, tile 47505316)
- Contour density: level 2 = 3.7k lines / 0.7 MB; **level 3 = 8k / 2.9 MB** (chosen);
  level 4 = 50k / 21 MB (too heavy for a browser overlay).
- Water poly types: `0x4c`(76) = flowing river-fill (1755 segments = the Colorado,
  compact); `0x41/0x46/0x47` = still lakes/reservoirs. Discriminator is the **type
  code**, not elongation (river segments are compact too).
- The contour overlay reuses the DRP1 pipeline entirely: contours are `Kind::Line`
  features of `GeoKind::Contour`, so `drape::build_drape(dec, …, &[GeoKind::Contour])`
  lifts them onto the surface and `encode_drape` emits DRP1 — the client's
  `decodeDrape` + a LineSegments already render it.

## Work items
- [ ] geo `terrain.rs`: `RIVER_FILL_TYPE = 0x4c` + `river_fill_grid()` (Water cells
      of that type) + test. Reuse `dilate_kind` to widen the river mask only.
- [ ] geo `garmin_bake.rs`: lake fix — widen only `river_fill_grid ×2` (lakes stay
      1-cell); emit `<stem>.contour.soa` via `build_drape(Contour, --contour-level=3)`
      + `encode_drape` with a darker contour-brown palette.
- [ ] cockpit-server: `garmin_contours` resolver + `/api/garmin-contours/:location`
      route + test (mirror the drape route; optional per-scene).
- [ ] `body.manifest.json`: `garmin_contours` entry for grand-canyon.
- [ ] cockpit `GeoHelix.tsx`: fetch contours (non-blocking) → `decodeDrape` → a thin
      semi-transparent brown `LineSegments`; `contours` toggle button.
- [ ] Rebake canyon → `.soa` (lake fix) + `.contour.soa`; gz into `cockpit/public`.
- [ ] Verify: geo tests, tsc, build, headless `/garmin/grand-canyon` (contours read
      as topo lines; lakes pinpoint; river bold).

## Mode split (2026-07-09 follow-up, from user feedback)

"The contours are a map layer, not the skin of the world." Two modes:

- **Beauty / surfel mode (DEFAULT)** — contours OFF by default; dry drainage
  rust-brown; blue reserved for flowing water (the Colorado + perennial 0x4c
  tributary reaches). Still-water tanks/ponds are retagged in the `--arid` bake to
  a new 10th palette slot (`LAKE_TAG=9`, `LAKE_SUBTLE=[122,130,134]`) — a
  barely-there grey fleck, deliberately below the shader's blue-dominance `wet`
  threshold so no vivid water treatment fires (1081 cells on the canyon tile).
  The ver-8 wire carries its palette count, so the extra slot is decode-safe.
- **Topo / OTM mode (the toggle, renamed `topo`)** — contour lines + a `uTopo`
  shader swap to cartographic paper: pale beige-white relief ramp, green
  vegetation-KIND tint, light carto-blue water, gentle hillshade. The vivid grade
  never shows under the line web.

## LOD honesty + stats HUD (same session)

- Verified: `postLod()` early-returns for every geo scene (GeoHelix.tsx) — the
  `/api/body/lod` cascade culls by the ANATOMY body's block-bounds, so on
  `/garmin/*` the LOD toggle was an inert placeholder. Per-scene `.blocks`
  cascade remains future work.
- UI: on geo scenes the button is now disabled (`LOD n/a`) with an explanatory
  tooltip, instead of silently doing nothing.
- Stats HUD (bottom-right, ≤2 Hz, from `renderer.info` ground truth):
  `tris 1.73M · lines 265k · calls 3 · verts 864k · LOD n/a (terrain: full mesh)`
  — and toggling topo shows `lines 745k · calls 4`, proving the layers are real.

## Notes
- Contours are a garmin_bake feature → applies to the **canyon** (and future
  garmin_bake scenes). Iceland uses the raster DEM pipeline (`iceland_dem.rs`), which
  has no vector contours; Iceland contours would need raster contour extraction —
  deferred.
