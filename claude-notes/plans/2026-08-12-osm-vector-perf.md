# OSM vector basemap performance — representation before SIMD

## Overview

The vector basemap works on the live deploy (measured: 69,125 shapes from
132,382 rows at one Berlin view) and is **terribly slow**. Operator asked
"any ideas in regards to ndarray?" — the honest answer, per this workspace's
own measured precedent (tesseract-rs: `strip_borders` and the prescale
finding — *representation is the first-order lever, SIMD second-order*), is
that the compute is not what's hot. Two representations are wrong:

1. **69k SVG DOM elements.** Every pan frame forces the browser to
   re-rasterize the entire retained `<svg>` layer. Zoom rebuilds it from
   scratch. This is the client-side cost.
2. **serde-JSON on the hot path.** A z13 city view moves ~40–60 MB of
   `[lon,lat]` f64 JSON, re-fetched on every zoom change with no caching
   headers. The house doctrine (T3 / ADR-022, "no serialization in the hot
   path; `to_le_bytes` IS the wire format") already names this an
   anti-pattern.

## The fix

**Server** — a binary LE SoA tile endpoint beside the JSON one:

- `GET /api/osm/geometry/tile-bin/:z/:x/:y` → `application/octet-stream`,
  format `OSM1` (see `encode_tile_bin` doc): header counts + per shape
  `idx u32, class u8, closed u8, npoints u16` + `npoints × (f32 dx, f32 dy)`
  **tile-relative world-pixels at z**, projected directly from the chain's
  own z32 cells (`px = cell × 2^(z-24)`) — no lon/lat round-trip at all.
  ~8 B/point against ~45 B/point JSON.
- `ETag` derived from the slab digest (+ format tag) with
  `Cache-Control: public, no-cache` → zoom churn becomes 304s, not
  re-downloads. The digest is already the cross-bake pin; reusing it means
  a new bake busts the cache by construction.
- JSON endpoint unchanged (tests, curl-ability, the click path).
- Internal refactor: `query_tile_shapes` returns RAW simplified cells;
  the JSON shape (`TileGeometryOut`) and the binary encoder both project
  from it, so the two wire forms cannot drift in sampling/classification.

**Client** — canvas raster of vector data, replacing the retained SVG:

- Per tile, per class: shapes are merged into **one `Path2D`** (fill set +
  stroke set). A full viewport redraw is then ~15 tiles × ~10 paths ≈ 150
  native draw calls, not 69,125 DOM nodes.
- One viewport-sized canvas under `#tiles` (dots/selection stay above),
  redrawn rAF-throttled on pan; `devicePixelRatio`-scaled.
- Zoom drops cached paths of other zooms (they are zoom-specific and were
  previously retained forever — an unbounded-memory bug on top of the perf).
- Draw order fixed by class (areas: wood→green→water→building; lines:
  other→rail→road) so roads sit on top.

## The frame (operator, 2026-08-12): askama-style projection over the slab

The tile endpoint is `render_field_view` for the geo domain — a PROJECTION
over data already resident in the slab, never a transformation into a
document. Same binary, same bytes:

- **The tile range IS the mask.** `slab.tile_range(z,x,y)` is a
  Morton-prefix row range over the same mmap — `surface ∩ mask`, nothing
  copied.
- **Zoom IS the projection depth.** `pixel_shift(z) = 32−(z+8)−1` is the
  ClassView picking a reading GRANULARITY of the same z32 register — a
  shift, never a branch (the same rule as tier-of-level `>>2`). Zooming in
  narrows the mask (deeper Morton prefix) and deepens the reading (smaller
  shift); nothing is re-encoded or stored twice.
- **The OSM1 wire is the fold's output**, LE bytes end to end; the client's
  Path2D/canvas is the render skin, exactly where a2ui puts it.

## ndarray — where it actually fits (recorded, not built now)

- **Bake-time pre-tiling** (`osm-soa-bake`): a `.tiles` sidecar with
  ready-to-serve per-(z,x,y) buffers — serving becomes a range memcpy;
  SIMD batch-projects all chains once at bake. The right home for ndarray
  if serving ever profiles hot after this change.
- **Zero-copy chains lens**: `Chains::get` heap-allocates a `Vec<TileXy>`
  per way per request; a borrowed iterator over the LE record would remove
  that. A lens question (zero-copy law), not a SIMD one.
- Per-request projection SIMD: only if profiling demands it later; with
  304-cacheable binary tiles it will not.

## Work items

- [x] Plan file
- [x] Server: `query_tile_shapes` raw refactor (JSON output byte-identical)
- [x] Server: `encode_tile_bin` + wire-code pin + tests (round-trip decode,
      JSON/bin equivalence, header layout, empty tile)
- [x] Server: `tile-bin` route + ETag/no-cache + If-None-Match → 304
- [x] Client: binary parse → per-class `Path2D` per tile
- [x] Client: canvas draw loop, rAF-throttled; zoom cache eviction
- [x] Client: delete retained-SVG basemap machinery (selection SVG stays)
- [x] Verify in headless Chromium: canvas pixels drawn, 0 external
      requests, pan still works, phone layout intact
- [x] Wire-size measurement JSON vs binary recorded in PR

## Verification status

Same honesty note as the parent plan: no slab in this container, so
verification is synthetic-fixture through the real client code; the live
deploy is the real test. The wire-size ratio is measured on synthetic
shapes with realistic point counts.
