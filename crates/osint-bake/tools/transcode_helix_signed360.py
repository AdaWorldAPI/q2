#!/usr/bin/env python3
"""Transcode a BSO2 ver-5 body wire (old ndarray helix_orient pos3|nrm3 helix
column) into a ver-6 wire whose 6-byte helix column carries a render-faithful
lance-graph helix::Signed360 NORMAL — for the /helix viewer.

Why a transcode (not helixbake): the FMA body.* source columns are not available
here, but the released body.soa.gz already carries the per-vertex normals (as the
old helix_orient codes). We decode those back to Cartesian and re-encode the
Signed360 fields the renderer actually uses: polar (signed lift) + azimuth. The
rim endpoint pair (the Fisher-Z metric carrier) is NOT used by the renderer, so it
is left zero here; the full metric-coupled artifact is helixbake's job once the FMA
columns + a helix build are available. This makes /helix RENDER, faithfully.

Output is ONE SoA wire (ver 6): the Signed360 normals are a COLUMN of the same
struct-of-arrays as the F16 positions and indices — never a separate sidecar file.
The artifact is published to a GitHub release (not committed); the Dockerfile pulls
it same-origin like body.soa.gz.

Usage: transcode_helix_signed360.py in.soa.gz out.soa   (writes out.soa + out.soa.gz)
"""
import sys, gzip, math
import numpy as np

GA = math.pi * 0.7639320225002104  # helix_orient golden angle = pi*(3-sqrt5)

def codebook(half):  # helix_orient cap: [r*cos a, r*sin a, y]
    n = np.arange(256, dtype=np.float64)
    ymin = math.cos(half)
    y = 1.0 - (1.0 - ymin) * (n + 0.5) / 256.0
    r = np.sqrt(np.clip(1.0 - y * y, 0.0, None))
    a = n * GA
    return np.stack([r * np.cos(a), r * np.sin(a), y], axis=1)  # (256,3)

CAP0, CAP1, CAP2 = codebook(math.pi), codebook(0.40), codebook(0.03)

def align(a):  # a: (N,3) -> axis k (N,3), angle t (N,)
    az = np.clip(a[:, 2], -1.0, 1.0)
    v0, v1 = a[:, 1], -a[:, 0]
    s = np.hypot(v0, v1)
    small = s < 1e-9
    inv = np.where(small, 1.0, 1.0 / np.where(small, 1.0, s))
    k = np.stack([np.where(small, 1.0, v0 * inv), np.where(small, 0.0, v1 * inv), np.zeros_like(s)], axis=1)
    t = np.where(small, np.where(az > 0, 0.0, math.pi), np.arccos(az))
    return k, t

def rot(p, k, t):  # Rodrigues, vectorized
    c, s = np.cos(t)[:, None], np.sin(t)[:, None]
    kxp = np.cross(k, p)
    kd = np.sum(k * p, axis=1)[:, None]
    return p * c + kxp * s + k * (kd * (1.0 - c))

def helix_orient_decode(b0, b1, b2):  # 3-byte cap-cascade -> unit Cartesian (N,3)
    d = CAP2[b2]
    k, t = align(CAP1[b1]); d = rot(d, k, -t)
    k, t = align(CAP0[b0]); d = rot(d, k, -t)
    return d / np.linalg.norm(d, axis=1, keepdims=True).clip(1e-12)

def main():
    src, out = sys.argv[1], sys.argv[2]
    with gzip.open(src, "rb") if src.endswith(".gz") else open(src, "rb") as f:
        raw = bytearray(f.read())
    assert raw[:4] == b"BSO2", "bad magic"
    ver = int.from_bytes(raw[4:6], "little")
    nC = int.from_bytes(raw[6:10], "little")
    nV = int.from_bytes(raw[10:14], "little")
    nT = int.from_bytes(raw[14:18], "little")
    assert ver == 5, f"expected ver 5, got {ver}"
    pos_bytes = 6
    helix_off = 18 + nC * (16 + 1 + 1 + 4 + 12 + 8) + pos_bytes * nV
    helix = np.frombuffer(bytes(raw[helix_off:helix_off + 6 * nV]), dtype=np.uint8).reshape(nV, 6)
    nrm = helix[:, 3:6]  # the self-orientation half (pos half = helix[:, 0:3], dropped)

    # decode old normals -> Cartesian (bake world frame)
    d = helix_orient_decode(nrm[:, 0].astype(np.intp), nrm[:, 1].astype(np.intp), nrm[:, 2].astype(np.intp))
    nx, ny, nz = d[:, 0], d[:, 1], d[:, 2]

    # encode render-faithful Signed360 (polar partition + golden-frame azimuth).
    # decode side (BodyHelix): y from polar partition, az->phi, r=sqrt(1-y^2),
    # world (r*sin phi, y, r*cos phi). So phi = atan2(nx, nz), y = ny.
    mag = np.rint(np.abs(ny) * 127.0).astype(np.int32).clip(0, 127)
    polar = np.where(ny >= 0, 128 + mag, 127 - mag).astype(np.uint8)
    phi = np.mod(np.arctan2(nx, nz), 2 * math.pi)
    az16 = np.rint(phi / (2 * math.pi) * 65536.0).astype(np.int64) & 0xFFFF
    s360 = np.zeros((nV, 6), dtype=np.uint8)            # rim bytes [0:3] = 0 (metric, unused)
    s360[:, 3] = polar
    s360[:, 4] = (az16 & 0xFF).astype(np.uint8)
    s360[:, 5] = ((az16 >> 8) & 0xFF).astype(np.uint8)

    # ── verify: decode back exactly as BodyHelix does, vs the decoded Cartesian ──
    y = np.where(polar >= 128, (polar.astype(np.float64) - 128) / 127.0, -((127 - polar.astype(np.float64)) / 127.0))
    azf = (az16.astype(np.float64) / 65536.0) * 2 * math.pi
    r = np.sqrt(np.clip(1 - y * y, 0, None))
    recon = np.stack([r * np.sin(azf), y, r * np.cos(azf)], axis=1)
    dot = np.clip(np.sum(recon * d, axis=1), -1, 1)
    err = np.degrees(np.arccos(dot))
    print(f"verts {nV}  decode-back err: mean {err.mean():.3f}°  p99 {np.percentile(err,99):.3f}°  max {err.max():.3f}°")

    # ── reassemble ONE SoA wire: ver 5->6, splice the Signed360 helix column in place,
    # everything else (concept cols, F16 pos, row, idx, json) verbatim. The normals stay a
    # COLUMN of the same struct-of-arrays as the positions — not a separate file. ──
    raw[4:6] = (6).to_bytes(2, "little")           # ver 6 = F16 pos + Signed360 helix column
    raw[helix_off:helix_off + 6 * nV] = s360.tobytes()
    import os
    with open(out, "wb") as f:
        f.write(raw)
    with gzip.open(out + ".gz", "wb", compresslevel=6) as f:
        f.write(raw)
    print(f"wrote {out} ({len(raw)} B) + {out}.gz ({os.path.getsize(out+'.gz')} B)")

if __name__ == "__main__":
    main()
