# Grand Canyon — dense DEM height + ver-9 photoreal satellite skin

Branch: `claude/geo-canyon-dem` (off `main` after #98 merged).

## Overview
Replace the sparse Garmin-contour canyon (844×1024 = 864k verts, blocky) with a
**dense real DEM heightfield** carrying a **raw satellite-imagery skin** — the
"Diaprojektor sunk into the HHTL grid" model (per-vertex colour, not a glued
texture). The dead plateau half of the tile is cropped off. Budget-LOD decimates
the render on mobile; the full grid is the desktop birdview.

## What shipped
- **`scripts/fetch_iceland_dem.py`** — `--bbox W,S,E,N` param (was Iceland-only).
  Fetched the canyon DEM+imagery at z14/ds3 → `canyon.demgrid` (DEMG v2, 4096×4949,
  Terrarium elevation + ESRI World Imagery). Both keyless.
- **`bso2.rs` `encode_grid_bso2`** — optional `colors: &[[u8;3]]` (0 or W·H). Empty →
  ver-8 (byte-identical, palette-only). Present → **ver-9** = an `rgb[W·H]` block
  after `kind`, the raw per-vertex satellite drape. The grid stays radix-compact and
  the client's stride-LOD sub-samples `rgb` identically to height/kind.
- **`garmin_bake.rs`**
  - `--dem <file.demgrid>` — source height (bilinear at each cell's lon/lat) from the
    dense DEM instead of sparse contours; a v2 demgrid also fills the ver-9 skin.
    Grid dims auto-match the DEM's native resolution.
  - `--crop W,S,E,N` — bake only a sub-window (cut the dead plateau). HHTL-safe: keys
    are `point_to_hhtl4(lon,lat)` on absolute coords, so a narrower window is a pure
    bbox change, no remapping. Drape/contour skipped in crop mode (tile-bbox features).
  - A `Dem` reader/sampler (`read_dem` + `colf`/`rowf`/`elev_at`/`rgb_at`), DEMG v1/v2.
- **`GeoHelix.tsx`**
  - `decodeGrid` reads ver 8|9; ver-9 sinks `rgb[W·H]` into the mesh's vertex colours
    (stride-selected); `decode()` + `gridBudgetStride` accept ver 9.
  - Shader `uSkin` branch: on a ver-9 scene the satellite photo IS the colour — only a
    soft relief hillshade, no hypsometric/water/topo recolour.
  - **Topo switch = mode switch** on a skin scene: photoreal ↔ cartographic paper
    (flips `uSkin`), works even without contour data (a cropped scene has none).

## The Grand Canyon result
- Crop `-112.83,35.80,-111.77,36.3504` (north half; SW/SE relief 26/29 = dead flat,
  NW/NE 113/162 = the canyon). 4116×2637 = **10.85M verts**, gz **~36 MB**.
- Budget-LOD (4.2M): full 10.85M birdview → stride-2 ≈ 2.7M on mobile.
- Verified headless (803k proxy): the satellite skin renders (forest / red walls /
  Colorado), sky, `verts · LOD · full grid` HUD. Full-size render is the Railway view.

## Havel canoe map (2026-07-10, same pipeline)
- User's next-10-days canoe region: `--bbox 12.246494,53.011917,13.657894,53.539721`
  (upper-left / lower-right corners from satellites.pro), z14/ds3.
- **No Garmin tile** → baked by `iceland_dem --grid --skin` (new flag): sinks the raw
  ESRI pixels into the ver-9 grid instead of the classified KIND palette. Vertex i IS
  demgrid cell i (1:1, no resampling).
- 5546×3498 = **19.4M verts**, elev 12–174 m (flat lowland — the value is the skin),
  gz **69.3 MB**, committed in `public/` (Release-hosted on-demand is the follow-up;
  MCP tooling can't create Releases this session). Budget-LOD: 19.4M → stride-2 =
  **4.85M** (the "20M is OK if LOD is 5 Mio" operating point).
- Scene = `/garmin/havel` via one manifest entry; the dropdown auto-populates from
  `garmin_scenes` — zero cockpit code.

## North-up fix + white balance + /havel route (2026-07-10, operator alignment check)
- **N–S mirror found by the operator** (Havel vs Google: Kölpinsee rendered SOUTH of
  the Müritz). Root cause: bake stores z = (lat−lat0)·M (north = +z) while the default
  aerial camera sits on the +z side (screen-up = −z) → every ver-8/9 grid scene rendered
  N–S mirrored. Ground truth (demgrid) verified correct — display-side bug only.
- **Fix in `decodeGrid` (no rebake, all grid scenes at once):** negate `zrow` at read
  (positions + normals derive), emit winding flipped ((a,d2,b)/(b,d2,e)) so faces stay
  +y, negate concept-centroid z. Wire + HHTL keys untouched.
- **White balance (operator: "rather dull"):** ESRI over Mecklenburg is hazy — measured
  p98 white point only 171/166/145. Bake-time per-channel p2→p98 stretch (→10..250) +
  sat ×1.18 on the demgrid rgb, then rebake — corrected colour sunk once (Diaprojektor),
  not shader-corrected per frame.
- **`/havel` first-class route** (like /ice): main.tsx Route + pathScene alias →
  `garmin:havel`. First attempt missed the top-level router (path fell through to the
  AIWAR cockpit) — caught by the headless shot.

## Follow-ups
- **66 MB from a q2 Release, on-demand** (operator preference) — host the full-tile /
  higher-zoom asset in a GitHub Release; scene fetches it on demand instead of git.
- **"Make it 20 again"** — re-fetch the crop at z15 for ~20M of *real* canyon detail
  (LOD → ~5M), instead of z14 native (10.85M).
- **Contour lines on the crop** — generate from the DEM (marching squares) since the
  Garmin tile-frame contours are skipped in crop mode.
- **`/osm` dual basemap** (task) + **Havel canoe map** (`--bbox 12.2465,53.0119,13.6579,53.5397`) —
  both reuse the DEM+skin pipeline; Havel is flat lowland → skin-dominant.
- **Sentinel-2 skin** — swap the ESRI source for `GrandCanyon_S2_20260620.tif`
  (source-agnostic; the skin just comes from the demgrid rgb).

## Terrain cinematographer — imagery is the ceiling (2026-07-10, operator critique)

Operator diagnosed three renderer (not data) artifacts, all pointing at the imagery:
1. **White patches = CLOUDS** in the ESRI capture (soft fuzzy edges, don't follow the
   canyon, change between captures). Confirmed: near-white = 0.85% of the demgrid,
   fuzzy blobs over normal terrain. Not geology.
2. **Washed mid-canyon** — some mosaic tiles are low-contrast; the DEM has the relief
   but the imagery lost the texture contrast there.
3. **10M looks blockier than 2.7M** — our colour is PER-VERTEX at grid resolution
   (= z14 imagery res ≈ 6.7 m/px), so there's no texture stretch; more mesh just
   samples the *same z14 pixels* more finely and exposes their native blockiness.
   The imagery is the resolution ceiling; more vertices don't add image detail.

Response (all shader/decode — no rebake):
- **DEM ambient occlusion** (`aAo`, decode-side) — the operator's "terrain-aware
  shading pass that restores the contrast the mosaic lost". A height box-blur gives
  a sky-openness proxy: a vertex below its neighbourhood mean is a gorge (occluded →
  darker), a ridge is open (brighter). This puts the dark cracks back in the WASHED
  tiles *from the geometry*, which has them.
- **Relief-model mode** (`uRelief`, `relief` toggle on skin scenes) — the operator's
  experiment made permanent: drop the imagery, render a flat sandstone albedo under
  the museum light + AO. "The Grand Canyon 3D-printed in sandstone, lit perfectly."
  Doubles as the diagnostic (if this is razor-sharp, the bottleneck is imagery, not
  geometry) and as the timeless imagery-independent aesthetic the operator wants.
- **Cloud de-emphasis** — the near-white (cloud) response is dimmed (0.86) + de-chroma'd
  + its specular sheen removed, so cloud blobs read as pale haze, not glowing snow.
- **Material response** — vegetation gets a wider light wrap (cheap subsurface); water
  is the only specular; snow/cloud is matte; sandstone matte. Not colour alone.

Open follow-ups the critique named: higher-res imagery (z15/Sentinel-2 → half the
px size), mosaic seam blending, per-tile contrast normalization.

## Museum lighting — the NatGeo art direction (2026-07-10, operator brief)

> "Stop thinking like a GIS, start thinking like a landscape photographer. …
> Don't optimize against Google Maps. Optimize against National Geographic."

The operator's six directives + river protagonist, implemented in the ver-9 skin
branch of the vertex shader (GeoHelix VERT, `uSkin` path) + one bake fix:

1. **Museum lighting** — warm morning KEY (wrap diffuse `(n·L+0.35)/1.35`), WEAK
   cool sky fill, and a **warm umber shadow floor** (`vec3(0.46,0.33,0.26)` —
   reddish-black, never grey): gallery light on a relief model, not orbit light.
2. **Colour separation** — hue-preserving chroma ×1.34 so the strata split into
   cream limestone / ochre / orange sandstone / deep red.
3. **Warm shadows** — the umber floor above; the canyon glows in its own bounce.
4. **Warm atmospheric perspective** — distance lifts toward pale warm amber
   (`vec3(0.93,0.86,0.74)`), never grey fog; Arizona's dry air.
5. **Terrain first, imagery second** — partial DE-LIGHT: the photo's baked-in
   orbital luma is pulled 50% toward a mid reference so OUR key re-shades the
   surfel form; the satellite palette remains as tint.
6. **Relief-model mindset** — emerges from 1–5.

**The river is the protagonist** — the ONLY material with dynamic lighting:
- `aWet` vertex attribute from the Garmin KIND grid (blue-dominant palette slot;
  LAKE_SUBTLE stays below threshold by design). Gouraud interpolation feathers
  the banks for free.
- Deep blue-green channel colour + faint riparian-green fringe at the banks.
- Two view-dependent Blinn lobes (pow 80 broad sun-path + pow 420 sparkle) that
  SLIDE along the bends as the camera orbits — sandstone stays matte; a few
  moving glints read as living water without animation.
- **Bake fix:** under `--crop` the KIND grid was rasterized against the FULL tile
  bbox (misregistered — invisible while the skin covered it). `garmin_bake` now
  builds a deg→mu `kbbox` from the crop window for `kind_grid`/`river_fill_grid`,
  so the Colorado's Water cells land on the real river (6,050 cells in the crop).

Also this round: **hue-preserving skin brighten** (operator: color "on the dark
side"; satellite haze + atmospheric degradation + the eye idealizing diffuse
morning light) — global luma p2→p98 stretch to [12,242] (NOT per-channel — that
would grey the red rock), gamma 0.94 shadow-open, sat ×1.12, warm bias
(R×1.02/B×0.97): luma mean 108→139, warmth kept R155>G140>B89.

**Follow-up (river continuity):** Garmin's 0x4c river-FILL polys rasterize the
thin inner-gorge reaches sub-cell (dashed segments); the CC filter keeps the wide
reaches (4,084 cells at full res). For continuous material coverage, rasterize the
river as a width-stamped POLYLINE from the Garmin line features instead.

## /osm dual basemap (2026-07-10, follow-up shipped)
- The `/osm` slippy cockpit gets a **basemap toggle**: OSM map ↔ ESRI World Imagery
  satellite — the SAME keyless imagery the ver-9 terrain skins drape from, so the flat
  map and the 3D scenes share one imagery truth. Same tile addresses, same HHTL keys —
  two skins.
- Server (`osm_tiles.rs`): `SAT_TILE_URL` + `sat_tile_url()` (ESRI is **z/y/x** — row
  before column; the shared `fill_template` encodes the swap), both locate/tile-meta
  responses now report `sat_tile_url`/`sat_source` (additive JSON). Test asserts the
  axis order.
- Page (`osm.rs`): `BASEMAPS` table (src template + attribution + next), `sat`/`osm`
  toggle button in the map controls, attribution swaps with the source (OSM
  contributors ↔ Esri/Maxar/Earthstar).
- Verified headless on the extracted page: toggle OSM→sat→OSM, srcs swap with the
  z/y/x order CORRECT, attribution follows. cockpit-server itself cannot compile in
  this container (rusty_v8 static-lib download blocked by egress policy) — Rust side
  mirrors the existing tested pattern; CI runs the tests.
