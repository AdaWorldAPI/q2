#!/usr/bin/env python3
"""Fetch a keyless Iceland DEM heightfield from AWS Terrarium terrain-RGB tiles.

Terrarium tiles (`elevation-tiles-prod`, public, no key) encode elevation in the
PNG's RGB: ``elev_m = R*256 + G + B/256 - 32768``. We fetch the slippy-tile grid
covering Iceland at a chosen zoom, stitch into one elevation raster, block-mean
downsample to a tractable heightfield, and emit a compact ``.demgrid`` binary the
`iceland_dem` Rust baker reads (no HTTP dep in the geo crate).

Grid file layout (little-endian):
    "DEMG"            4 bytes magic
    version   u32     = 1
    W         u32     columns  (west->east, col 0 = west edge)
    H         u32     rows     (north->south, row 0 = north edge)
    west_lon  f64
    east_lon  f64     (lon is LINEAR across columns — WebMercator x is linear in lon)
    lat[H]    f64     latitude of each row centre (row 0 = north); non-linear in row
    elev[H*W] f32     metres, row-major, row 0 = north, col 0 = west

usage:
    python3 scripts/fetch_iceland_dem.py OUT.demgrid [--zoom 9] [--downsample 4]
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
BASE = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"
UA = {"User-Agent": "q2-iceland-dem/1.0 (keyless terrarium bake)"}


def lon_to_tilex(lon, z):
    return (lon + 180.0) / 360.0 * (1 << z)


def lat_to_tiley(lat, z):
    r = math.radians(lat)
    return (1.0 - math.log(math.tan(r) + 1.0 / math.cos(r)) / math.pi) / 2.0 * (1 << z)


def tiley_to_lat(ty, z):
    """Inverse WebMercator: fractional tile-y (at zoom z) -> latitude (deg)."""
    n = math.pi * (1.0 - 2.0 * ty / (1 << z))
    return math.degrees(math.atan(math.sinh(n)))


def fetch_tile(z, x, y, tries=4):
    url = BASE.format(z=z, x=x, y=y)
    last = None
    for _ in range(tries):
        try:
            req = urllib.request.Request(url, headers=UA)
            data = urllib.request.urlopen(req, timeout=60).read()
            im = Image.open(io.BytesIO(data)).convert("RGB")
            return x, y, np.asarray(im, dtype=np.float32)
        except Exception as e:  # noqa: BLE001
            last = e
    print(f"  tile {z}/{x}/{y} FAILED: {last!r}", file=sys.stderr)
    return x, y, None


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    out = sys.argv[1]
    zoom = 9
    down = 4
    for i, a in enumerate(sys.argv):
        if a == "--zoom":
            zoom = int(sys.argv[i + 1])
        if a == "--downsample":
            down = int(sys.argv[i + 1])

    x0 = int(math.floor(lon_to_tilex(LON_W, zoom)))
    x1 = int(math.floor(lon_to_tilex(LON_E, zoom)))
    # tile-y grows south, so LAT_N (north) -> smaller y.
    y0 = int(math.floor(lat_to_tiley(LAT_N, zoom)))
    y1 = int(math.floor(lat_to_tiley(LAT_S, zoom)))
    xs = list(range(x0, x1 + 1))
    ys = list(range(y0, y1 + 1))
    print(f"zoom {zoom}: x {x0}..{x1} ({len(xs)} tiles), y {y0}..{y1} ({len(ys)} tiles) "
          f"= {len(xs) * len(ys)} tiles", file=sys.stderr)

    Wtiles, Htiles = len(xs), len(ys)
    stitched = np.zeros((Htiles * TILE, Wtiles * TILE), dtype=np.float32)

    jobs = [(zoom, x, y) for y in ys for x in xs]
    got, missing = 0, 0
    with ThreadPoolExecutor(max_workers=16) as ex:
        for x, y, rgb in ex.map(lambda t: fetch_tile(*t), jobs):
            gx = x - x0
            gy = y - y0
            if rgb is None:
                missing += 1
                continue
            elev = rgb[:, :, 0] * 256.0 + rgb[:, :, 1] + rgb[:, :, 2] / 256.0 - 32768.0
            stitched[gy * TILE:(gy + 1) * TILE, gx * TILE:(gx + 1) * TILE] = elev
            got += 1
    print(f"fetched {got} tiles, {missing} missing", file=sys.stderr)
    if missing > len(jobs) // 10:
        print("too many missing tiles — aborting", file=sys.stderr)
        sys.exit(1)

    # Geographic extent of the stitched tile grid (tile edges, exact).
    west = xs[0] / (1 << zoom) * 360.0 - 180.0
    east = (xs[-1] + 1) / (1 << zoom) * 360.0 - 180.0
    Hpx, Wpx = stitched.shape

    # Block-mean downsample by `down` (crop to a multiple first).
    Hc = (Hpx // down) * down
    Wc = (Wpx // down) * down
    st = stitched[:Hc, :Wc].reshape(Hc // down, down, Wc // down, down).mean(axis=(1, 3))
    Hout, Wout = st.shape
    print(f"stitched {Wpx}x{Hpx} px -> downsampled {Wout}x{Hout} verts "
          f"({Wout * Hout:,} verts)", file=sys.stderr)

    # Per-row latitude: row r centre sits at tile-y = y0 + (r+0.5)*down/TILE.
    lats = np.empty(Hout, dtype=np.float64)
    for r in range(Hout):
        ty = y0 + (r + 0.5) * down / TILE
        lats[r] = tiley_to_lat(ty, zoom)

    emin, emax = float(st.min()), float(st.max())
    land = int((st > 0.5).sum())
    print(f"elev range {emin:.1f}..{emax:.1f} m; land verts {land:,} "
          f"({100 * land / st.size:.1f}%)", file=sys.stderr)

    with open(out, "wb") as f:
        f.write(b"DEMG")
        f.write(struct.pack("<I", 1))
        f.write(struct.pack("<I", Wout))
        f.write(struct.pack("<I", Hout))
        f.write(struct.pack("<d", west))
        f.write(struct.pack("<d", east))
        f.write(lats.astype("<f8").tobytes())
        f.write(st.astype("<f4").tobytes())
    print(f"wrote {out} ({Wout}x{Hout}, west {west:.4f} east {east:.4f}, "
          f"lat {lats[0]:.4f}..{lats[-1]:.4f})", file=sys.stderr)


if __name__ == "__main__":
    main()
