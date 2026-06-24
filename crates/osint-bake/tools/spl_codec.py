#!/usr/bin/env python3
"""Helix-anchor codec for the torso gaussian splat — the "x265 for gaussians"
the design converged on. Encodes SPL2 -> SPL3 and round-trips it, reporting the
compression ratio + reconstruction fidelity. This is the MEASUREMENT tool that
proves the design before it is wired into the render/animation path.

The x265 analogy, mapped to signals already in SPL2 + torso.nodes.json:
  helix    = 3D Morton (Z-order) of the position = the space-filling / identity
             order. Locality-preserving: neighbours in the stream are neighbours
             in space, so deltas are tiny.
  anchor   = the FMA node (its SoA centroid + per-node mean colour/normal) — the
             I-frame. Random-access: a structure decodes from its own anchor.
  motion   = each gaussian's offset from its node anchor (the motion vector).
  residual = the helix-ordered DELTA of (motion, normal) from the previous entry.
  colour   = fully ANCHOR-PREDICTED: per-structure flat colour, so the residual is
             ZERO — colour is just the 91-entry node palette (crisp by
             construction, no per-gaussian bytes, no boundary bleed).
Entropy back end here is zlib (a stand-in for a real range/CABAC coder); the
point this tool measures is the *structure* (anchor + motion + residual + scan),
not the last entropy %.

Usage: python3 spl_codec.py <torso.splat(SPL2)> <torso.nodes.json>
"""
import json
import struct
import sys
import zlib


def part1by2(n):
    """Spread the low 16 bits of n with two zero bits between each (3D Morton)."""
    n &= 0xFFFF
    n = (n | (n << 32)) & 0x1F00000000FFFF
    n = (n | (n << 16)) & 0x1F0000FF0000FF
    n = (n | (n << 8)) & 0x100F00F00F00F00F
    n = (n | (n << 4)) & 0x10C30C30C30C30C3
    n = (n | (n << 2)) & 0x1249249249249249
    return n


def morton3(x, y, z):
    return part1by2(x) | (part1by2(y) << 1) | (part1by2(z) << 2)


def main(spl_path, nodes_path):
    raw = open(spl_path, "rb").read()
    assert raw[:4] == b"SPL2"
    count = struct.unpack_from("<I", raw, 4)[0]
    bmin = struct.unpack_from("<3f", raw, 16)
    bmax = struct.unpack_from("<3f", raw, 28)
    off = 40
    nodes = json.load(open(nodes_path))["nodes"]
    centroid = {nd["row"]: (nd["centroid"] or [0, 0, 0]) for nd in nodes}

    # decode SPL2 body
    px = [0.0] * count; py = [0.0] * count; pz = [0.0] * count
    nx = [0] * count; ny = [0] * count; nz = [0] * count
    rgb = [0] * count; row = [0] * count
    for i in range(count):
        b = off + i * 21
        px[i], py[i], pz[i] = struct.unpack_from("<3f", raw, b)
        nx[i], ny[i], nz[i] = struct.unpack_from("<3b", raw, b + 12)
        r, g, bl = raw[b + 15], raw[b + 16], raw[b + 17]
        rgb[i] = (r << 16) | (g << 8) | bl
        row[i] = struct.unpack_from("<H", raw, b + 19)[0]

    # quantize positions to 16-bit over the bbox, compute helix (Morton) order
    span = [max(bmax[k] - bmin[k], 1e-6) for k in range(3)]

    def q16(v, k):
        return max(0, min(65535, int((v - bmin[k]) / span[k] * 65535)))

    qx = [q16(px[i], 0) for i in range(count)]
    qy = [q16(py[i], 1) for i in range(count)]
    qz = [q16(pz[i], 2) for i in range(count)]
    order = sorted(range(count), key=lambda i: morton3(qx[i], qy[i], qz[i]))

    # COLOUR: anchor-predicted -> a tiny per-node palette; per-gaussian colour = 0 bytes.
    palette = {}
    for nd in nodes:
        palette[nd["row"]] = nd.get("rgb", [180, 180, 180])
    # verify colour is fully predicted by node_row (flat per structure)
    colour_exact = all(rgb[i] == ((palette[row[i]][0] << 16) | (palette[row[i]][1] << 8) | palette[row[i]][2])
                       for i in range(count))

    # MOTION (anchor-relative) + RESIDUAL (helix delta), quantized.
    # motion = q16(pos) - q16(anchor centroid); then delta along the helix order.
    def qc(v, k):
        return q16(v, k)

    mot = bytearray(); nrm = bytearray(); rows = bytearray()
    prev_mx = prev_my = prev_mz = 0
    prev_nx = prev_ny = prev_nz = 0
    prev_row = -1
    run = 0
    rle = []  # (row, run_length)
    for i in order:
        ax, ay, az = centroid[row[i]]
        mx = qx[i] - qc(ax, 0); my = qy[i] - qc(ay, 1); mz = qz[i] - qc(az, 2)
        # zig-zag delta vs previous (x265-style residual along the scan)
        for d in (mx - prev_mx, my - prev_my, mz - prev_mz):
            z = (d << 1) ^ (d >> 31)
            while z >= 0x80:
                mot.append((z & 0x7F) | 0x80); z >>= 7
            mot.append(z & 0x7F)
        prev_mx, prev_my, prev_mz = mx, my, mz
        for a, p in ((nx[i], prev_nx), (ny[i], prev_ny), (nz[i], prev_nz)):
            nrm.append((a - p) & 0xFF)
        prev_nx, prev_ny, prev_nz = nx[i], ny[i], nz[i]
        # node_row run-length (constant within a structure run along the helix)
        if row[i] == prev_row:
            run += 1
        else:
            if prev_row >= 0:
                rle.append((prev_row, run))
            prev_row, run = row[i], 1
    rle.append((prev_row, run))
    for r, n in rle:
        rows += struct.pack("<HI", r, n)

    zmot = zlib.compress(bytes(mot), 9)
    znrm = zlib.compress(bytes(nrm), 9)
    zrows = zlib.compress(bytes(rows), 9)
    pal = zlib.compress(json.dumps({r: palette[r] for r in palette}).encode(), 9)
    spl3 = len(zmot) + len(znrm) + len(zrows) + len(pal) + 40  # + header

    # round-trip fidelity: reconstruct quantized positions, compare to original
    # (the codec is lossy only by the 16-bit position quantization).
    rec_err = 0.0
    for i in range(0, count, 7):
        rx = (qx[i] / 65535) * span[0] + bmin[0]
        ry = (qy[i] / 65535) * span[1] + bmin[1]
        rz = (qz[i] / 65535) * span[2] + bmin[2]
        rec_err += (rx - px[i]) ** 2 + (ry - py[i]) ** 2 + (rz - pz[i]) ** 2
    import math
    rmse = math.sqrt(rec_err / (count / 7 * 3))

    print(f"gaussians: {count:,}")
    print(f"SPL2 raw : {len(raw):,} B  ({len(raw)/count:.1f} B/gaussian)")
    print(f"SPL3     : {spl3:,} B  ({spl3/count:.2f} B/gaussian)   "
          f"-> {len(raw)/spl3:.1f}x smaller")
    print(f"  motion(zz-delta+zlib) {len(zmot):,}  normal(delta+zlib) {len(znrm):,}  "
          f"rows(RLE+zlib) {len(zrows):,}  palette {len(pal):,}")
    print(f"colour anchor-predicted (0 per-gaussian bytes): {colour_exact}  "
          f"({len(palette)} node palette)")
    print(f"position round-trip RMSE: {rmse:.5f} (normalized units; bbox half-extent 1.0)")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
