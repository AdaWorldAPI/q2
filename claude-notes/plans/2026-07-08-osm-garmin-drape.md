# OSM ⊕ Garmin drape — semantic features lifted onto the terrain surface

> Started 2026-07-08. Follows the merged terrain arc (#91 typed pipeline, #93
> shader, #94 surfel). Branch `claude/geo-osm-drape` → PR (the "drape beast").

## Design spine (operator-endorsed, via ChatGPT)

- **Garmin owns height + terrain material** — the ver-8 radix-grid heightfield
  (already baked, `canyon.v8grid.soa.gz` / `iceland_dem.v8grid.soa.gz`).
- **OSM / vector features own the semantic layer** — roads, trails, rivers,
  (later) labels. For the **canyon** the feature source is the Garmin IMG's own
  typed line features (Street / Path / Stream — already decoded). For **Iceland**
  the feature source is the *huge* `otm-iceland.img` database (71.9 MB: NET/NOD
  routing + typed features + per-tile DEM).
- **HHTL owns alignment** — both grid vertices and feature lines share the one
  `point_to_hhtl4` address space and the same equirectangular projection about the
  tile centre, so a feature lands on the right cell / height.
- **Renderer owns fusion** — GeoHelix draws the terrain mesh + the draped feature
  overlay, toggleable. Think **surfaces**, not "terrain".

## Architecture — least-risk, terrain wire untouched

The proven ver-8 terrain wire + decoder are **not** touched. The drape is a
**separate sidecar** (`*.drape.soa`) rendered as a `THREE.LineSegments` overlay:

1. **Bake** (`geo/src/garmin/drape.rs`): for each line feature of the wanted kinds
   at the bake's LOD level, project each `(lon,lat)` vertex to fractional grid
   `(col,row)` via `terrain::project`, **bilinear-sample the terrain `pos` grid**
   (the exact display-frame surface point the ver-8 grid encodes), and emit a
   polyline. This reuses the ver-8 bake's own `pos` array + normalization
   constants, so the drape is guaranteed co-registered with the surface.
2. **Wire** (`DRP1`): `b"DRP1" | ver u16 | nLines u32 | nKind u8 | palette(nK×3) |
   per line: kind u8, nPts u16, pts(nPts×3 f32)`. Tiny, self-describing, gzipped.
   Palette = `GeoKind::PALETTE` so colours match the terrain KIND palette.
3. **Serve**: committed to `cockpit/public/*.drape.soa.gz`; new
   `/api/garmin-drape/:location` route resolves a `garmin_drapes` manifest map
   (mirror of `garmin_scenes`). 404 → client silently skips the drape.
4. **Render** (`GeoHelix.tsx`): fetch the drape sidecar in parallel for garmin
   terrain scenes, decode DRP1, build a `LineSegments` whose vertices are the
   sampled surface point with `y *= uExagVal` (matching the terrain shader's
   `dpos.y = position.y * uExag`) plus a hair of lift, coloured per KIND. A
   **features** toggle shows/hides it.

## Alignment proof (why the lines sit on the surface)

ver-8 client reconstructs `positions = (x0 + c·dx, HALF_LUT[hf]·yscale, zrow[r])`,
no sign flips (GeoHelix `decodeGrid`). The bake's `pos[i]` **is** that display
point pre-encode. The drape samples `pos` bilinearly at the feature's fractional
`(col,row)` → identical frame. The shader lifts terrain `y` by `uExag`; the client
lifts the drape `y` by the same `uExagVal`. Co-registered by construction.

## Work items

- [x] `geo/src/garmin/drape.rs` — `DrapeLine`, `build_drape`, `encode_drape`
      (DRP1 i16 wire), `DRAPE_KINDS = [Street, Path, Stream]`; 2 tests (canyon
      level-4 lifts streets+rivers, all endpoints finite + on-surface; DRP1
      byte-exact header).
- [x] `garmin_bake.rs` — emit `<stem>.drape.soa` beside the ver-8 wire.
- [x] `cargo test` geo lib: 22 green (2 drape + regression). Terrain bake is
      **byte-identical** to the deployed `canyon.v8grid.soa.gz` (drape additive).
- [x] Bake canyon drape → 29,235 lines (Stream 11,629 · Street 15,860 · Path
      1,746) → 1.3 MB gz → `cockpit/public/canyon.v8grid.drape.soa.gz`.
- [x] cockpit-server `/api/garmin-drape/:location` + `garmin_drapes` resolver +
      test (`drape_resolves_from_its_own_map_and_is_optional`); compiled + green.
- [x] `body.manifest.json` — `garmin_drapes` map entry + note.
- [x] `GeoHelix.tsx` — DRP1 decode + LineSegments overlay (y·uExag + lift) +
      `features` toggle (shown only when a drape is present). `tsc --noEmit` clean.
- [x] Headless screenshot of `/garmin/grand-canyon`: the blue dendritic river
      network + grey roads read ON the surface; `features off` → bare terrain.
      Co-registered, toggleable (fused ↔ Garmin-only).

## Arid drainage recolor (2026-07-08 follow-up)

The dendritic `Stream` network on a desert tile is DRY drainage (washes / gullies
/ arroyos), not water — painting it river-blue made the Grand Canyon plateau read
as wet. Fix: reserve blue for the actual `Water` bodies (the Colorado + permanent
lakes) and recolour the drainage rust-brown.

- [x] `GeoKind::ARID_DRAINAGE = [120,68,44]` + `GeoKind::arid_palette()`
      (`geo/src/garmin/classify.rs`): the KIND palette with `Stream` browned,
      every other class (incl. blue `Water`) unchanged. Unit-tested.
- [x] `garmin_bake --arid` (`geo/src/bin/garmin_bake.rs`): uses `arid_palette()`
      for the ONE palette that feeds both the ver-8 terrain KIND block AND the DRP1
      drape → drainage browns consistently across both. Diff vs the non-arid bake is
      exactly the 3-byte Stream RGB entry in each wire; terrain geometry byte-identical.
- [x] `GeoHelix.tsx` — `uArid` uniform (1 = non-glacial scene): gates the glacial
      turquoise OFF (canyon Water stays plain river-blue, not teal) and re-asserts a
      clean deep river-blue for the (blue-KIND) Water cells so the Colorado survives
      the 55%-terrain-blend + warm-sunset key as the focal point. Iceland (uArid=0)
      keeps turquoise. Drainage is browned in the bake → `wet`=0 → shader leaves it.
- [x] Rebake: `garmin_bake .claude/maps/garmin-grand-canyon/47505316.img
      canyon.v8grid.soa --arid` → `gzip -9` the `.soa` + `.drape.soa` into
      `cockpit/public/canyon.v8grid{,.drape}.soa.gz`. Headless `/garmin/grand-canyon`
      verified: plateau = warm desert earth + brown incised drainage; the Colorado a
      thin blue ribbon in the canyon depth (pixel-checked: 0 → 475 blue water px).

## Follow-ups (own PRs)

- **Iceland drape** — same `build_drape` over `otm-iceland.img` line features
  (needs the otm-iceland reader path; the IMG parser already handles the format).
- **Iceland height dequantize** — `otm-iceland.img` DEM subfiles as the height
  source (real elevation, kills the ver-8 F16-quantization needle field).
- **Layer-toggle matrix** — Garmin-only / features-only / fused / x-ray ontology
  (this PR ships terrain + a single features on/off = fused vs Garmin-only).
- **Label anchors** — lift LBL place-names as billboarded anchors.
