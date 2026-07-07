# Garmin IMG typed rebuild — ground-up geo pipeline (canyon / iceland / berlin)

**Status:** format decoder VALIDATED (Python prototype, Grand Canyon renders
recognizably). Rust port + bake pipeline pending.

## Why (operator direction, 2026-07-07)

The `/geo` building heuristics were judged "extremely terrible" against the
working `/osm` (Cesium-style slippy map) and `/helix` (surfel-interpolated
anatomy). Operator's spec, verbatim requirements:

- "helix uses surfel interpolation" — the geo mesh needs that look.
- "you need building heuristics · anything color > same building · buildings
  become houses · green becomes grass or forest texture · blue becomes water ·
  walking pathways vs street texture"
- "you have neither mastered the terrain nor the kurvenlineal AFTER
  heuristics, nor the goureaud shading in the end" — **pipeline order is
  mandatory: heuristics → kurvenlineal → terrain → Gouraud.**
- "there are so many algorithms to fix height overlay · start from basics and
  use Garmin format — that has all the data you need"

Garmin IMG replaces raster color-guessing with **typed lookups**: every
polygon/polyline carries a type code (building / water / forest / street /
walking path / contour), multi-level generalization (TRE levels = LOD
pyramid), contour polylines with elevation labels (and per-tile `.DEM`
subfiles in newer builds).

## Sources (banked in `.claude/maps/`, see SOURCES.md)

- `otm-iceland.zip` + `otm-iceland-contours.zip` — garmin.opentopomap.org
- `garmin-grand-canyon/475053{10,11,16,17}.img` — gpsfiledepot Arizona Topo
  (map id 1). `47505316` = Grand Canyon Village tile.
- Other free source noted by operator: alternativaslibres.org.

## Decoder — VALIDATED (scripts/garmin_proto.py)

Python prototype, proven by rendering tile 47505316 at full detail: the
Colorado gorge, side canyons, village road grid, Bright Angel Trail, and
elevation-labelled contours all appear correctly.

Format facts (hard-earned, keep):

- **Container:** byte 0 = XOR key (0x00 on all our files); `DSKIMG` @0x10;
  blocksize = `1 << (b[0x61] + b[0x62])`; FAT = 512-B entries from 0x600
  (flag 0x01, name 8B + typ 3B, size u32 @0x0C **valid in part-0 entry**,
  part u16 @0x10, 240 u16 block pointers @0x20). Multi-part subfiles:
  concatenate blocks by `part*240 + i` order. RGN sizes read 0 in naive
  parsers — slice by block list, trust part-0 size when nonzero.
- **TRE:** bbox N/E/S/W int24 mapunits @0x15/18/1B/1E (deg = mu·360/2²⁴);
  levels @u32 0x21 (4 B each: zoom|bit7=inherited, bits, nsubdiv u16);
  subdivisions @u32 0x29 — 16-B records, **last level 14-B** (no `next`):
  rgn_off u24, objtypes (0x10 pt / 0x20 idx-pt / 0x40 line / 0x80 poly),
  center lon/lat int24, width u16 (bit15 = terminate), height u16, next u16.
- **RGN:** data section @u32 0x15, len @0x19. Subdiv block = sorted-by-rgn_off
  spans; if N object kinds present, (N−1) u16 pointers prefix the block, kinds
  in order pt/idx-pt/line/poly. Point: type, lbl u24 (bit23 = has-subtype),
  dlon/dlat i16 (<< 24−bits). Line: byte0 = type(0..5)|bit6 dir|bit7 two-byte
  len; lbl u24 (bit22 = extra-bit-per-node, bit23 = NET ref); dlon/dlat i16;
  len u8/u16 (**bitstream bytes only, info byte separate**); info byte =
  lon_base lo-nibble, lat_base hi-nibble. Poly: same but type mask 0x7F.
- **Bitstream (exact QMapShack CShiftReg semantics — the part everyone gets
  wrong):** LSB-first bits. Per axis: 1 bit "same-sign"; if set, 1 more bit =
  constant sign, values unsigned; else per-delta two's complement **with one
  extra bit added to the width**. Width from base: `n = base+2` (base ≤ 9)
  else `2·base−9`, then +1 if signed. **Continuation:** in signed mode raw ==
  `1<<(n−1)` (sign bit only) accumulates `2^(n−1)−1` and reads again; final
  value < sign → `acc + tmp`; ≥ sign → `(tmp − 2^n) − acc`. Missing this
  turns fine levels into random-walk mush (our level-4 bug). extra_bit = 1
  skip bit per pair before x.
- **LBL:** data @u32 0x15, offset multiplier `1 << b[0x1D]`, encoding @0x1E
  (6 = 6-bit packed, 9 = 8-bit latin1 0-terminated). 6-bit: 3 bytes → 4 chars
  hi-first; table `" A..Z~~~~~0..9~~~~~~"`; value > 0x2F terminates.
  Contour labels = elevation, **feet** in US topo maps (÷3.28084 → m),
  meters in OTM.
- **Type codes (this map family):** lines: 0x20/0x21/0x22 = minor/inter/major
  land contour (0x23-0x25 depth), 0x18/0x1f/0x26 streams/rivers, ≤0x07 roads,
  0x0a/0x0b/0x16 trails/walking paths. Polys: 0x3c-0x49 water, 0x50 woods,
  0x14-0x16 park/reserve. Buildings (urban maps): poly 0x13 (+0x6x variants).

## Remaining work (tasks #12-#16)

1. **Rust port** — `geo/src/garmin/` (mirror scripts/garmin_proto.py; the
   prototype IS the executable spec, diff outputs against it).
2. **Type codes → KIND** — typed lookup table per kind; sweet palette like
   `/ice` `Kind::color()`. One typed polygon = one building/house.
3. **Contours → heightfield** — grid interpolation (or TIN) from contour
   polylines + elevations; all features drape on it. Iceland tiles also
   carry `.DEM` subfiles (format: mkgmap DEM — parse later if wanted).
4. **Bake pipeline in the MANDATED order:** typed heuristics → kurvenlineal
   (`helix::CurveRuler` per-class surfel texture, AFTER heuristics) → terrain
   (houses extruded from terrain height; water/grass/paths flat) → **Gouraud
   smooth per-vertex normals LAST** (surfel-interpolation look, not flat
   facets). Encode `encode_mesh_bso2` ver-7.
5. **Scenes:** `/canyon` (Grand Canyon — new), Iceland re-bake, Berlin
   (needs a Garmin Germany/Berlin extract or mkgmap build from
   `.claude/maps/berlin-latest.osm.pbf`; java available for mkgmap).
6. **Verify visually per scene** (scratchpad shot.js harness, small-first).

## References

- QMapShack `CGarminPolygon.cpp` (GPL, consulted for format semantics only;
  our code is an original implementation of the documented format).
- John Mechalas, "The Garmin IMG File Format" (imgformat.pdf).
- mkgmap (the encoder these maps were built with).
