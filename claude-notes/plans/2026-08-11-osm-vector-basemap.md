# OSM vector basemap — draw the map from the bake, not from a CDN

## Overview

The cockpit's `/osm` view currently renders **someone else's raster** and
overlays our data on it: `OSM_TILE_URL` points at `tile.openstreetmap.org` and
`SAT_TILE_URL` at Esri's World Imagery. The 1.42 GB Berlin bake supplies only
the feature dots and — since #116 — the shape of the ONE feature you click.

Two problems follow, and the first is the architectural one:

1. **The bake isn't drawing the map.** "The map pyramid and the slab's row key
   are one and the same address" is true as arithmetic while every pixel is
   rented. This is gap #1 from #116 ("the target is an area-fill base layer").
2. **Policy exposure.** A publicly deployed host on `tile.openstreetmap.org`
   runs against the OSMF Tile Usage Policy; Esri's imagery has its own terms.

The decode half already exists: `.chains` holds every way's z=32 vertex chain,
and `query_geometry` reads one by row index. The step here is to serve them
**per tile**, budgeted and simplified, and draw them client-side in the SVG
layer #116 already added.

Bundled with it (operator's call, one PR): the `/osm` page is unusable on a
phone — `grid-template-columns:1fr 320px` leaves the map ~75 px wide on a
~395 px viewport, and drag is bound to `mousedown`/`mousemove` with no
`touch-action`, so the map cannot be panned by finger at all.

## Design decisions

- **Category is data; style is the client's.** A `ShapeClass` (water /
  building / wood / green / rail / road / other) is derived from the tags the
  bake already stored and returned per shape. Shipping every shape's full tag
  set would multiply the payload by the tag fan-out for data a viewer wants
  about one shape at a time (the same argument `FeatureOut::idx` already
  makes). The client maps class → colour/width/z-order. `FeatureGeometryOut`
  gains the same field so the click path and the basemap path share ONE
  classification, in Rust — the JS `classFor(tags)` rule is deleted rather
  than duplicated.
- **Simplify in the chain's own coordinate system.** A tile is 256 px, so a
  world pixel at zoom `z` is the z32 cell shifted right by `32 - (z + 8)`.
  Dropping vertices that land on the same (sub)pixel is an integer compare on
  the codec's own grid — no floating-point distance test, no tolerance
  constant to tune. Sub-pixel detail is not "less accurate" to draw; it is
  invisible, and at city scale it is most of the payload.
- **Budget by row, report honestly.** Geometry is far heavier per row than a
  dot, so it gets its own budget, reusing `overview_sample` (whose cascade-cell
  selection keeps ISOLATED features that a plain stride drops). The response
  reports `total` / `sampled` / `returned` / `malformed` separately so a thin
  basemap is legible as LOD rather than looking like missing data.
- **Known LOD characteristic (not yet measured):** the budget is spent on
  ROWS, and rows that are nodes carry no chain, so at overview zoom some of
  the budget buys nothing. Whether that reads as too sparse can only be judged
  against the real bake. If it does, the next rung is to over-sample and then
  re-spread the chain-bearing survivors through `overview_sample` a second
  time — recorded here so it is not re-derived.
- **External rasters become opt-in, not default.** `vector` is the default
  basemap; the OSM and Esri skins stay reachable through the existing toggle
  (they remain useful for eyeballing our render against a reference), but a
  default page load no longer touches a third-party tile server.

## Phase 1 — tests first

- [x] `class_for_tags` — each rule fires; precedence is as documented; an
      untagged way and a way whose tags match nothing both fall to `Other`
      (anti-vacuity: the classifier is not a constant).
- [x] `simplify_cells` — two-sided on zoom: a dense chain collapses at low
      zoom and is kept whole at high zoom (a constant-threshold bug fails
      this); first/last vertices always survive; a closed ring stays closed.
- [x] `geometry_row_budget` — two-sided at `CITY_ZOOM_FLOOR`.

## Phase 2 — server

- [x] `ShapeClass`, `class_for_tags`, `row_tags`
- [x] `simplify_cells` + `pixel_shift`
- [x] `geometry_row_budget`
- [x] `query_tile_geometry` + `osm_tile_geometry_handler`
- [x] `class` on `FeatureGeometryOut` (one classification, shared)
- [x] route `/api/osm/geometry/tile/:z/:x/:y`

## Phase 3 — client

- [x] `vector` basemap (default), drawn into an SVG layer beneath the dots
- [x] incremental paint (mirrors `drawnCells`; the n² redraw is a solved
      problem here and must not be re-introduced)
- [x] `styleForClass` replaces `classFor`; click path uses the server's class
- [x] attribution reflects the actual source (ODbL data credit stays)

## Phase 4 — mobile (bundled)

- [x] stacked layout under a width breakpoint; map keeps a real height
- [x] pointer events + `touch-action:none` so a finger pans the map

## Verification status

Unit tests run against synthetic fixtures (this crate's established pattern —
`synthetic_slab`). **Real-bake verification did not happen locally**: the
container lost the slab on restart and `AWS_S3_BUCKET_NAME` is not set here,
so `ensure_slab_local` cannot hydrate. The render path over real Berlin data
is therefore UNVERIFIED until the deploy. Said plainly rather than implied.
