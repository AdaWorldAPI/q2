#!/usr/bin/env python3
"""Fetch a keyless Iceland DEM heightfield + true-colour imagery drape.

Two keyless slippy-tile sources on the SAME grid:
  * elevation — AWS Terrarium terrain-RGB (`elevation-tiles-prod`, no key):
    ``elev_m = R*256 + G + B/256 - 32768``.
  * imagery   — ESRI World Imagery (`server.arcgisonline.com`, no key, z/y/x
    order): real satellite true-colour (green vegetation, white glaciers, black
    lava, brown rock) — the TEXTURE the height palette can only approximate.

We fetch both grids at a chosen zoom, stitch, block-mean downsample, and emit a
compact ``.demgrid`` binary the `iceland_dem` Rust baker reads (no HTTP dep in
the geo crate). The imagery is draped per-vertex into the mesh's colour channel.

Grid file layout (little-endian):
    "DEMG"            4 bytes magic
    version   u32     = 2  (v1 = elev only; v2 adds the rgb drape below)
    W         u32     columns  (west->east, col 0 = west edge)
    H         u32     rows     (north->south, row 0 = north edge)
    west_lon  f64
    east_lon  f64     (lon is LINEAR across columns — WebMercator x is linear in lon)
    lat[H]    f64     latitude of each row centre (row 0 = north); non-linear in row
    elev[H*W] f32     metres, row-major, row 0 = north, col 0 = west
    rgb[H*W*3] u8     v2 ONLY: true-colour drape, same row-major order as elev

usage:
    python3 scripts/fetch_iceland_dem.py OUT.demgrid [--zoom 9] [--downsample 4] [--no-imagery]
"""
import io
import math
import struct
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor

import numpy as np
from PIL import Image

# Iceland bounding box (slightly padded so the whole island + a sea margin is in).
LON_W, LON_E = -25.0, -13.0
LAT_S, LAT_N = 63.0, 67.0

TILE = 256
DEM_BASE = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"
# ESRI World Imagery is z/y/x (row before col), unlike the Terrarium z/x/y.
IMG_BASE = "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"
UA = {"User-Agent": "q2-iceland-dem/2.0 (keyless terrarium + esri imagery bake)"}


def lon_to_tilex(lon, z):
    return (lon + 180.0) / 360.0 * (1 << z)


def lat_to_tiley(lat, z):
    r = math.radians(lat)
    return (1.0 - math.log(math.tan(r) + 1.0 / math.cos(r)) / math.pi) / 2.0 * (1 << z)


def tiley_to_lat(ty, z):
    """Inverse WebMercator: fractional tile-y (at zoom z) -> latitude (deg)."""
    n = math.pi * (1.0 - 2.0 * ty / (1 << z))
    return math.degrees(math.atan(math.sinh(n)))


def fetch_rgb_tile(url, tries=4):
    """Fetch one tile as an (H, W, 3) uint8 array, or None after `tries` failures."""
    last = None
    for _ in range(tries):
        try:
            req = urllib.request.Request(url, headers=UA)
            data = urllib.request.urlopen(req, timeout=60).read()
            im = Image.open(io.BytesIO(data)).convert("RGB")
            return np.asarray(im, dtype=np.uint8)
        except Exception as e:  # noqa: BLE001
            last = e
    print(f"  tile {url} FAILED: {last!r}", file=sys.stderr)
    return None


def stitch_grid(base_url, zoom, xs, ys, x0, y0, decode, dtype, channels, label):
    """Fetch `xs×ys` tiles from `base_url`, decode each, stitch into one raster.
    `decode(rgb_uint8) -> array` maps a tile's raw RGB to the stored value.
    Aborts (sys.exit) on ANY missing tile after retries — a hole would bake a
    synthetic flat patch that looks like real data but isn't."""
    Wtiles, Htiles = len(xs), len(ys)
    shape = (Htiles * TILE, Wtiles * TILE) + ((channels,) if channels > 1 else ())
    stitched = np.zeros(shape, dtype=dtype)
    jobs = [(x, y) for y in ys for x in xs]

    def one(t):
        x, y = t
        return x, y, fetch_rgb_tile(base_url.format(z=zoom, x=x, y=y))

    got, missing = 0, 0
    with ThreadPoolExecutor(max_workers=16) as ex:
        for x, y, rgb in ex.map(one, jobs):
            gx, gy = x - x0, y - y0
            if rgb is None:
                missing += 1
                continue
            stitched[gy * TILE:(gy + 1) * TILE, gx * TILE:(gx + 1) * TILE] = decode(rgb)
            got += 1
    print(f"{label}: fetched {got} tiles, {missing} missing", file=sys.stderr)
    if missing > 0:
        print(f"{label}: {missing} tile(s) missing after retries — aborting "
              f"(a hole would bake a synthetic patch)", file=sys.stderr)
        sys.exit(1)
    return stitched


def block_mean(a, down):
    """Block-mean downsample the first two axes by `down` (crop to a multiple first)."""
    if down == 1:
        return a
    Hpx, Wpx = a.shape[0], a.shape[1]
    Hc, Wc = (Hpx // down) * down, (Wpx // down) * down
    if a.ndim == 2:
        return a[:Hc, :Wc].reshape(Hc // down, down, Wc // down, down).mean(axis=(1, 3))
    # (H, W, C) — mean each channel over the block.
    c = a.shape[2]
    return a[:Hc, :Wc].reshape(Hc // down, down, Wc // down, down, c).mean(axis=(1, 3))


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    out = sys.argv[1]
    zoom = 9
    down = 4
    want_imagery = True
    for i, a in enumerate(sys.argv):
        if a == "--zoom":
            zoom = int(sys.argv[i + 1])
        if a == "--downsample":
            down = int(sys.argv[i + 1])
        if a == "--no-imagery":
            want_imagery = False

    x0 = int(math.floor(lon_to_tilex(LON_W, zoom)))
    x1 = int(math.floor(lon_to_tilex(LON_E, zoom)))
    # tile-y grows south, so LAT_N (north) -> smaller y.
    y0 = int(math.floor(lat_to_tiley(LAT_N, zoom)))
    y1 = int(math.floor(lat_to_tiley(LAT_S, zoom)))
    xs = list(range(x0, x1 + 1))
    ys = list(range(y0, y1 + 1))
    print(f"zoom {zoom}: x {x0}..{x1} ({len(xs)} tiles), y {y0}..{y1} ({len(ys)} tiles) "
          f"= {len(xs) * len(ys)} tiles/layer", file=sys.stderr)

    # ── Elevation (Terrarium terrain-RGB → metres). ──
    def dem_decode(rgb):
        rgb = rgb.astype(np.float32)
        return rgb[:, :, 0] * 256.0 + rgb[:, :, 1] + rgb[:, :, 2] / 256.0 - 32768.0

    stitched = stitch_grid(DEM_BASE, zoom, xs, ys, x0, y0, dem_decode, np.float32, 1, "dem")

    # ── Imagery (ESRI World Imagery → true-colour RGB). ──
    img_stitched = None
    if want_imagery:
        img_stitched = stitch_grid(
            IMG_BASE, zoom, xs, ys, x0, y0, lambda rgb: rgb, np.uint8, 3, "imagery")

    # Geographic extent of the stitched tile grid (tile edges, exact).
    west = xs[0] / (1 << zoom) * 360.0 - 180.0
    east = (xs[-1] + 1) / (1 << zoom) * 360.0 - 180.0
    Hpx, Wpx = stitched.shape

    st = block_mean(stitched, down)
    Hout, Wout = st.shape
    print(f"stitched {Wpx}x{Hpx} px -> {Wout}x{Hout} verts "
          f"({Wout * Hout:,} verts)", file=sys.stderr)

    img = None
    if img_stitched is not None:
        img = np.clip(block_mean(img_stitched.astype(np.float32), down), 0, 255).astype(np.uint8)
        # Crop imagery to the exact elev grid (block_mean crops to a multiple; match shapes).
        img = img[:Hout, :Wout, :]

    # Per-row latitude: row r centre sits at tile-y = y0 + (r+0.5)*down/TILE.
    lats = np.empty(Hout, dtype=np.float64)
    for r in range(Hout):
        ty = y0 + (r + 0.5) * down / TILE
        lats[r] = tiley_to_lat(ty, zoom)

    emin, emax = float(st.min()), float(st.max())
    land = int((st > 0.5).sum())
    print(f"elev range {emin:.1f}..{emax:.1f} m; land verts {land:,} "
          f"({100 * land / st.size:.1f}%){'; imagery drape' if img is not None else ''}",
          file=sys.stderr)

    version = 2 if img is not None else 1
    with open(out, "wb") as f:
        f.write(b"DEMG")
        f.write(struct.pack("<I", version))
        f.write(struct.pack("<I", Wout))
        f.write(struct.pack("<I", Hout))
        f.write(struct.pack("<d", west))
        f.write(struct.pack("<d", east))
        f.write(lats.astype("<f8").tobytes())
        f.write(st.astype("<f4").tobytes())
        if img is not None:
            f.write(np.ascontiguousarray(img, dtype=np.uint8).tobytes())
    print(f"wrote {out} v{version} ({Wout}x{Hout}, west {west:.4f} east {east:.4f}, "
          f"lat {lats[0]:.4f}..{lats[-1]:.4f})", file=sys.stderr)


if __name__ == "__main__":
    main()
