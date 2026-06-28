#!/usr/bin/env python3
"""Slicer-fill pass — solid lumen cores for vessels (option-2 rebake, geometry stage).

Reads bake_body_soa.py's SoA columns and, for each VESSEL concept (Doppler material
0..3: low/high-res artery · portal · systemic venous), extracts a centerline and
generates a solid inner-core tube swept along it — the "3D-printing slicer" lumen
fill. The core renders SOLID (opaque material) inside the translucent #17 wall, so a
vessel reads as a filled flowing tube, not a hollow shell. Appends the fill geometry
to the columns (tagged with the source concept's row → inherits its material/layer),
updates body.concepts.json verts/tris, and the Rust stage re-emits BSO2.

Centerline (no medial-axis engine; tractable + faithful for tubular structures):
  PCA principal axis → project verts → bin into segments → per-bin cross-section
  centroid + median radius → polyline centerline → ring-sweep a core at CORE·radius.

CRITICAL — connected components first: a single FMA vessel concept's mesh is often
SEVERAL disconnected anatomical blobs (e.g. left+right hand/foot vessels under one
concept; a thigh vein row that also tags toe vertices). Fitting ONE PCA axis over all
of them and ring-sweeping bridges the blobs with a solid tube THROUGH EMPTY SPACE —
the "out-of-body" tubes. So we split each concept into spatially-connected components
(grid flood-fill) and fill each component on its own centerline; disconnected blobs
never get bridged.

Usage: python3 fill_body_soa.py <soa_dir>   (rewrites the columns in place)
"""
import json
import math
import os
import struct
import sys

K = 8          # ring resolution (octagon core)
BINS = 14      # centerline segments along the axis
CORE = 0.62    # inner-core radius fraction (under the wall)
RMAX = 0.020   # ABSOLUTE diameter boundary: max cross-section radius in normalized
               # [-1,1] body units (~34 mm dia — covers the aorta; clamps balloons).
RMIN = 0.0008  # floor so capillaries still get a visible core
CELL = 0.015   # connected-component grid cell (~13 mm). A continuous vessel keeps
               # adjacent cells occupied (26-neighbour reach ~26 mm bridges sampling
               # gaps); blobs farther apart (hands ~300 mm, thigh→toe >100 mm) split.
VESSEL_MATERIALS = {0, 1, 2, 3}


def princ_axis(pts, mean):
    """principal axis via power iteration on the 3x3 covariance."""
    cxx = cxy = cxz = cyy = cyz = czz = 0.0
    for (x, y, z) in pts:
        dx, dy, dz = x - mean[0], y - mean[1], z - mean[2]
        cxx += dx * dx; cxy += dx * dy; cxz += dx * dz
        cyy += dy * dy; cyz += dy * dz; czz += dz * dz
    n = len(pts) or 1
    c = [[cxx / n, cxy / n, cxz / n], [cxy / n, cyy / n, cyz / n], [cxz / n, cyz / n, czz / n]]
    v = [1.0, 0.3, 0.1]
    for _ in range(24):
        nv = [c[0][0]*v[0]+c[0][1]*v[1]+c[0][2]*v[2],
              c[1][0]*v[0]+c[1][1]*v[1]+c[1][2]*v[2],
              c[2][0]*v[0]+c[2][1]*v[1]+c[2][2]*v[2]]
        m = math.sqrt(sum(a*a for a in nv)) or 1.0
        v = [a / m for a in nv]
    return v


def ortho_frame(axis):
    """two unit vectors perpendicular to axis (for the ring)."""
    a = axis
    ref = [1.0, 0.0, 0.0] if abs(a[0]) < 0.9 else [0.0, 1.0, 0.0]
    u = [a[1]*ref[2]-a[2]*ref[1], a[2]*ref[0]-a[0]*ref[2], a[0]*ref[1]-a[1]*ref[0]]
    m = math.sqrt(sum(c*c for c in u)) or 1.0
    u = [c/m for c in u]
    w = [a[1]*u[2]-a[2]*u[1], a[2]*u[0]-a[0]*u[2], a[0]*u[1]-a[1]*u[0]]
    return u, w


def components(pts, cell):
    """connected components of pts via a coarse grid (26-neighbour flood fill).
    Points are in one component iff a chain of occupied neighbouring cells links them,
    so disconnected blobs sharing one concept are never bridged by the fill."""
    grid = {}
    for i, p in enumerate(pts):
        key = (int(math.floor(p[0]/cell)), int(math.floor(p[1]/cell)), int(math.floor(p[2]/cell)))
        grid.setdefault(key, []).append(i)
    seen = set()
    comps = []
    for start in list(grid.keys()):
        if start in seen:
            continue
        stack = [start]; seen.add(start); cells = []
        while stack:
            c = stack.pop(); cells.append(c)
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    for dz in (-1, 0, 1):
                        nb = (c[0]+dx, c[1]+dy, c[2]+dz)
                        if nb in grid and nb not in seen:
                            seen.add(nb); stack.append(nb)
        comps.append([pts[i] for c in cells for i in grid[c]])
    return comps


def fill_one(pts, crow, fpx, fnx, frow, ftri, base):
    """Centerline-fill one connected blob; append a solid core to the shared columns.
    Returns 1 if a core was generated, else 0."""
    mean = [sum(p[k] for p in pts)/len(pts) for k in range(3)]
    axis = princ_axis(pts, mean)
    u, w = ortho_frame(axis)
    ts = [(p[0]-mean[0])*axis[0] + (p[1]-mean[1])*axis[1] + (p[2]-mean[2])*axis[2] for p in pts]
    tmin, tmax = min(ts), max(ts)
    span = (tmax - tmin) or 1e-4
    # bin along the axis; each ring = its bin's own centroid (follows the curve) and
    # MEDIAN perpendicular distance from that centroid, clamped to [RMIN, RMAX].
    binned = [[] for _ in range(BINS)]
    for p, t in zip(pts, ts):
        b = min(BINS-1, int((t - tmin)/span*BINS))
        binned[b].append(p)
    rings = []
    for bp in binned:
        if len(bp) < 1:
            continue
        n = len(bp)
        cen = [sum(q[k] for q in bp)/n for k in range(3)]
        dists = []
        for q in bp:
            dx, dy, dz = q[0]-cen[0], q[1]-cen[1], q[2]-cen[2]
            axial = dx*axis[0] + dy*axis[1] + dz*axis[2]
            perp2 = (dx*dx + dy*dy + dz*dz) - axial*axial
            dists.append(math.sqrt(perp2) if perp2 > 0.0 else 0.0)
        dists.sort()
        rad = min(RMAX, max(RMIN, dists[len(dists)//2] * CORE))
        rings.append((cen, rad))
    if len(rings) < 2:
        return 0
    ring_start = []
    for (cen, rad) in rings:
        ring_start.append(base + len(fpx)//3)
        for k in range(K):
            ang = 2*math.pi*k/K
            nx = math.cos(ang)*u[0] + math.sin(ang)*w[0]
            ny = math.cos(ang)*u[1] + math.sin(ang)*w[1]
            nz = math.cos(ang)*u[2] + math.sin(ang)*w[2]
            fpx += [cen[0]+rad*nx, cen[1]+rad*ny, cen[2]+rad*nz]
            fnx += [nx, ny, nz]          # radial normal (helix-normal IS radial for tubes)
            frow.append(crow)
    for s in range(len(rings)-1):
        a0, a1 = ring_start[s], ring_start[s+1]
        for k in range(K):
            kn = (k+1) % K
            ftri += [a0+k, a0+kn, a1+k, a1+k, a0+kn, a1+kn]
    return 1


def main(d):
    doc = json.load(open(os.path.join(d, "body.concepts.json")))
    concepts = doc["concepts"]; nV = doc["verts"]; nT = doc["tris"]
    pos = list(struct.unpack(f"<{nV*3}f", open(os.path.join(d, "body.pos"), "rb").read()[:nV*12]))
    row = list(struct.unpack(f"<{nV}I", open(os.path.join(d, "body.row"), "rb").read()[:nV*4]))

    # group vertex indices by concept row (one O(nV) pass)
    by_row = {}
    for i in range(nV):
        by_row.setdefault(row[i], []).append(i)

    fpx, fnx, frow, ftri = [], [], [], []   # fill: pos(3·), normal(3·), row, tris (combined index space)
    base = nV
    vessels = 0
    for c in concepts:
        if c["material"] not in VESSEL_MATERIALS:
            continue
        idxs = by_row.get(c["row"], [])
        if len(idxs) < K * 2:
            continue
        cpts = [(pos[i*3], pos[i*3+1], pos[i*3+2]) for i in idxs]
        # split into connected components so disconnected blobs are never bridged
        for comp in components(cpts, CELL):
            if len(comp) < K * 2:
                continue
            vessels += fill_one(comp, c["row"], fpx, fnx, frow, ftri, base)

    if not fpx:
        print("no vessel fill generated", file=sys.stderr); return
    nfv = len(fpx)//3; nft = len(ftri)//3
    # truncate each column to its EXACT data size (drop bake's 64-byte tail pad),
    # then append the fill — so the Rust read stays aligned (no padding mid-stream).
    def trunc_append(name, exact, data):
        with open(os.path.join(d, name), "r+b") as f:
            f.truncate(exact); f.seek(exact); f.write(data)
    trunc_append("body.pos", nV*12, struct.pack(f"<{len(fpx)}f", *fpx))
    trunc_append("body.nrm", nV*12, struct.pack(f"<{len(fnx)}f", *fnx))
    trunc_append("body.row", nV*4,  struct.pack(f"<{len(frow)}I", *frow))
    trunc_append("body.idx", nT*12, struct.pack(f"<{len(ftri)}I", *ftri))
    doc["verts"] = nV + nfv; doc["tris"] = nT + nft
    json.dump(doc, open(os.path.join(d, "body.concepts.json"), "w", encoding="utf-8"))
    print(f"slicer-fill: {vessels} vessel components → +{nfv:,} core verts / +{nft:,} core tris "
          f"(total {doc['verts']:,} verts / {doc['tris']:,} tris)", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "soa")
