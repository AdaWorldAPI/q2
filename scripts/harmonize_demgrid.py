#!/usr/bin/env python3
"""Flat-field harmonization for a `.demgrid` satellite skin (DEMG v2).

A `.demgrid` produced by `fetch_iceland_dem.py` carries a raw ESRI World
Imagery drape (the `rgb` block, DEMG v2). Over a large region that drape is a
*quilt*: a mosaic of unrelated satellite acquisitions differing in season, sun
angle and white balance, with visible tile seams. Used directly as a terrain
texture the quilt reads as arbitrary bright/dark/desaturated patches rather
than as terrain.

This pass removes the *low-frequency* tile-to-tile variation (the quilt)
while preserving *high-frequency* detail (fields, forest, roads, the actual
terrain texture). It is deliberately gentle — the philosophy is that the
satellite imagery is *source material*, not the final appearance, but we still
want it to read as a real place, not a flat paint chip.

Algorithm (per RGB channel, operating on the full skin as one image):

  1. Large-radius Gaussian blur `low` estimates the local mean — i.e. the
     quilt's slowly-varying exposure/white-balance field.
  2. Subtract `(low - global_mean) * strength` to flatten that field toward a
     single global mean. `strength=0.80` removes most of the seam contrast
     without erasing genuine large-scale terrain (a real dark forest block vs
     a real bright agricultural block survives at 0.80; it does not at 1.0).
  3. Water is *protected*: dark pixels (low luma) get a reduced strength so
     lakes/rivers keep their true dark tone instead of being lifted toward the
     land mean.
  4. Chroma is compressed toward a target magnitude so an over-saturated tile
     and a washed-out tile land on a common vegetation chroma, killing the
     "some tiles are grey, some are vivid green" tell.

This is the transform that produced the shipped `havel.v8grid.soa.gz`. The
`--strength 0.80 --radius 80 --chroma 28` defaults are the values approved for
Havel; other regions may want a different radius (scale it with the skin's
pixel dimensions — 80 suits ~2000px-wide skins).

Only the `rgb` block is rewritten; header, `lats` and `elev` are copied byte
-for-byte, so geometry and elevation are untouched.

Usage:
    python scripts/harmonize_demgrid.py in.demgrid out.demgrid \
        [--strength 0.80] [--radius 80] [--chroma 28] [--water-knee 60] \
        [--water-soft 40]
"""

import argparse
import struct
import sys

import numpy as np
from PIL import Image, ImageFilter

LUMA = np.float32([0.299, 0.587, 0.114])


def read_demgrid(path):
    b = np.fromfile(path, dtype=np.uint8)
    if b[:4].tobytes() != b"DEMG":
        sys.exit(f"{path}: not a DEMG file")
    ver, w, h = struct.unpack_from("<III", b, 4)
    if ver != 2:
        sys.exit(f"{path}: expected DEMG v2 (has raw skin); got v{ver}")
    west, east = struct.unpack_from("<dd", b, 16)
    n = w * h
    o = 32
    lats = b[o : o + h * 8].copy()
    o += h * 8
    elev = b[o : o + n * 4].copy()
    o += n * 4
    rgb = b[o : o + n * 3].astype(np.float32).reshape(h, w, 3)
    header = b[:32].copy()
    return {"header": header, "w": w, "h": h, "lats": lats, "elev": elev, "rgb": rgb}


def write_demgrid(path, dem, rgb_u8):
    with open(path, "wb") as f:
        f.write(dem["header"].tobytes())
        f.write(dem["lats"].tobytes())
        f.write(dem["elev"].tobytes())
        f.write(rgb_u8.reshape(-1).tobytes())


def harmonize(rgb, strength, radius, chroma_target, water_knee, water_soft):
    h, w, _ = rgb.shape
    gmean = rgb.reshape(-1, 3).mean(0)
    lum = rgb @ LUMA

    # (1) low-frequency field = the quilt's exposure/white-balance variation
    low = np.asarray(
        Image.fromarray(rgb.astype("u1"), "RGB").filter(
            ImageFilter.GaussianBlur(radius)
        ),
        dtype=np.float32,
    )

    # (3) water protection: dark pixels flatten less, keeping lakes dark
    water = np.clip((water_knee - lum) / max(water_soft, 1e-3), 0.0, 1.0)
    eff = (strength * (1.0 - 0.85 * water))[:, :, None]

    # (2) flatten the low-frequency field toward the global mean
    flat = rgb - (low - gmean) * eff

    # (4) compress chroma toward a common vegetation magnitude
    lum2 = (flat @ LUMA)[:, :, None]
    chroma = flat - lum2
    cmag = np.linalg.norm(chroma, axis=2, keepdims=True) + 1e-3
    scale = np.clip(chroma_target / cmag, 0.6, 1.7)
    harm = lum2 + chroma * scale

    return np.clip(harm, 0, 255).astype(np.uint8)


def near_white_frac(rgb_u8):
    """Fraction of pixels that are near-white (a proxy for washed/quilt tiles)."""
    m = rgb_u8.reshape(-1, 3).min(axis=1)
    return float((m > 200).mean())


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("input")
    ap.add_argument("output")
    ap.add_argument("--strength", type=float, default=0.80)
    ap.add_argument("--radius", type=float, default=80.0)
    ap.add_argument("--chroma", type=float, default=28.0, help="target chroma magnitude")
    ap.add_argument("--water-knee", type=float, default=60.0, help="luma below which water protection ramps in")
    ap.add_argument("--water-soft", type=float, default=40.0, help="luma softness of the water knee")
    args = ap.parse_args()

    dem = read_demgrid(args.input)
    raw_u8 = dem["rgb"].astype(np.uint8)
    harm = harmonize(dem["rgb"], args.strength, args.radius, args.chroma, args.water_knee, args.water_soft)
    write_demgrid(args.output, dem, harm)

    print(
        f"harmonized {dem['w']}x{dem['h']} skin  "
        f"near-white {near_white_frac(raw_u8) * 100:.2f}% -> {near_white_frac(harm) * 100:.2f}%  "
        f"(strength={args.strength} radius={args.radius} chroma={args.chroma})"
    )
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
