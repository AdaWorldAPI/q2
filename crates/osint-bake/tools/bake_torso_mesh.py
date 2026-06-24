#!/usr/bin/env python3
"""Bake the REAL-anatomy torso as a FILLED, SMOOTH TRIANGLE MESH (not splats) —
operator directive 2026-06-24: "connect to a triangle filled surface ... kurvenlineal
over triangles (Quadro/AutoCAD)". The BodyParts3D OBJs are already triangulated with
per-vertex normals (`vn`); we vertex-cluster-decimate each mesh (averaging the normals
per cell = the smooth "curve-ruler" over the triangles) and emit ONE indexed mesh,
coloured per is_a tissue. Rendered filled + Gouraud/Phong it is a solid CAD surface
(ivory bone, red muscle, ...), matching the Open 3D Man material-surface aesthetic.

Reuses bake_torso_splat's is_a classification (load_isa / tissue_of / isa_dn / colours)
so the mesh and the splat share ONE classifier. Geometry is the decimated triangle
surface; the splat bake stays the surfel/gaussian path.

SPM1 wire (little-endian):
  header 40 B: magic "SPM1" | vert_count u32 | tri_count u32 | node_count u32
               | bbox_min 3f | bbox_max 3f
  vertex body  vert_count x 21 B: pos 3f (12) | normal 3i8 (3) | rgb 3u8 (3) | opacity u8 (1) | node_row u16 (2)
  index body   tri_count x 12 B: 3x u32 (vertex indices, global)
Positions are normalised to [-1,1] (centre + uniform scale); orientation (x,-z,y) and
the i8-normal dequant happen in the renderer (cockpit/driver), same as SPL3.

LICENCE: BodyParts3D, (c) The Database Center for Life Science. CC-BY 4.0 / CC-BY-SA 2.1 JP.

Usage: python3 bake_torso_mesh.py <scratch_dir> <out.mesh> [cell_mm]
  cell_mm: vertex-cluster cell size in source mm (smaller = more triangles). Default 3.6
           ~= 600K triangles for the whole body.
"""
import collections
import json
import os
import struct
import sys

from bake_torso_splat import (
    ATTRIBUTION, CONTAINER_ID, ISA_ROOT, SYSTEM_OF, TISSUE_OPACITY,
    bfs, isa_dn, load_isa, tissue_color, tissue_of,
)

CELL_MM_DEFAULT = 3.6


def read_obj_mesh(path):
    """Return (verts[(x,y,z)], normals[(nx,ny,nz)] aligned to verts, faces[(a,b,c)]).
    BodyParts3D uses `f v//vn` with vn index == v index, so vn is 1:1 with v; if the
    counts disagree we fall back to a +Z normal (rare)."""
    vs, vn, faces = [], [], []
    with open(path, "rb") as f:
        for ln in f:
            if ln[:2] == b"v ":
                p = ln.split(); vs.append((float(p[1]), float(p[2]), float(p[3])))
            elif ln[:3] == b"vn ":
                p = ln.split(); vn.append((float(p[1]), float(p[2]), float(p[3])))
            elif ln[:2] == b"f ":
                p = ln.split()
                try:
                    idx = [int(t.split(b"/")[0]) - 1 for t in p[1:4]]
                except (ValueError, IndexError):
                    continue
                if len(idx) == 3 and all(0 <= k < len(vs) for k in idx):
                    faces.append((idx[0], idx[1], idx[2]))
    ns = vn if len(vn) == len(vs) else [(0.0, 0.0, 1.0)] * len(vs)
    return vs, ns, faces


def cluster_decimate(verts, normals, faces, inv_h, ox, oy, oz):
    """Vertex clustering on a global grid (cell = 1/inv_h): collapse all verts in a cell
    to one representative (mean position + mean normal), remap faces, drop degenerates.
    Averaging the normals per cell keeps the surface SMOOTH (the curve-ruler effect)."""
    cell_of = {}
    acc = []  # [sx, sy, sz, nx, ny, nz, count]
    remap = [0] * len(verts)
    for i, (x, y, z) in enumerate(verts):
        key = (int((x - ox) * inv_h), int((y - oy) * inv_h), int((z - oz) * inv_h))
        j = cell_of.get(key)
        nx, ny, nz = normals[i]
        if j is None:
            j = len(acc); cell_of[key] = j
            acc.append([x, y, z, nx, ny, nz, 1])
        else:
            a = acc[j]
            a[0] += x; a[1] += y; a[2] += z
            a[3] += nx; a[4] += ny; a[5] += nz; a[6] += 1
        remap[i] = j
    nv, nn = [], []
    for a in acc:
        c = a[6]
        nv.append((a[0] / c, a[1] / c, a[2] / c))
        nl = (a[3] * a[3] + a[4] * a[4] + a[5] * a[5]) ** 0.5 or 1.0
        nn.append((a[3] / nl, a[4] / nl, a[5] / nl))
    nf = []
    for (fa, fb, fc) in faces:
        ra, rb, rc = remap[fa], remap[fb], remap[fc]
        if ra != rb and rb != rc and ra != rc:
            nf.append((ra, rb, rc))
    return nv, nn, nf


def main(scratch, out_path, cell_mm=CELL_MM_DEFAULT):
    parent, children, name, elems, canon = load_isa(scratch)
    order, depth = bfs(ISA_ROOT, children)
    have = set(order)
    isa_obj = os.path.join(scratch, "isa_BP3D_4.0_obj_99")
    pof_obj = os.path.join(scratch, "partof", "partof_BP3D_4.0_obj_99")

    def obj_path(fj):
        p = os.path.join(isa_obj, fj + ".obj")
        return p if os.path.exists(p) else os.path.join(pof_obj, fj + ".obj")

    # deepest-first claim so the finest is_a type owns each shared mesh (mirrors the splat)
    concepts = sorted((c for c in elems if c in have), key=lambda c: -depth[c])
    owner = {}
    for c in concepts:
        for fj in elems[c]:
            owner.setdefault(fj, c)
    meshes_of = collections.defaultdict(list)
    for fj, c in owner.items():
        meshes_of[c].append(fj)
    for v in meshes_of.values():
        v.sort()
    kept = [c for c in order if c in meshes_of]

    inv_h = 1.0 / float(cell_mm)
    tcache = {}
    # global mesh accumulators
    px, py, pz, nx, ny, nz, cr, cg, cb, cop, crow = ([] for _ in range(11))
    tris = []  # (a, b, c) global vertex indices
    nodes = []
    ident_ctr = collections.Counter()
    row_of = {c: r for r, c in enumerate(kept)}

    for c in kept:
        r = row_of[c]
        nm = canon.get(c, name.get(c, c))
        tissue = tissue_of(c, parent, name, canon, tcache)
        col = tissue_color(tissue, r)
        op_u8 = max(8, min(255, int(round(TISSUE_OPACITY[tissue] * 255))))
        container = CONTAINER_ID[tissue]
        identity = ident_ctr[tissue] & 0xFFFF
        ident_ctr[tissue] += 1
        v_start = len(px)
        for fj in meshes_of[c]:
            p = obj_path(fj)
            if not os.path.exists(p):
                continue
            vs, ns, faces = read_obj_mesh(p)
            if not faces:
                continue
            ox = min(v[0] for v in vs); oy = min(v[1] for v in vs); oz = min(v[2] for v in vs)
            nv, nn, nf = cluster_decimate(vs, ns, faces, inv_h, ox, oy, oz)
            base = len(px)
            for (x, y, z), (ax, ay, az) in zip(nv, nn):
                px.append(x); py.append(y); pz.append(z)
                nx.append(ax); ny.append(ay); nz.append(az)
                cr.append(col[0]); cg.append(col[1]); cb.append(col[2])
                cop.append(op_u8); crow.append(r)
            for (a, b, cc) in nf:
                tris.append((base + a, base + b, base + cc))
        pa, seen = parent.get(c), 0
        while pa is not None and pa not in row_of and seen < 24:
            pa = parent.get(pa); seen += 1
        nodes.append({
            "row": r, "fma": c, "name": nm, "depth": depth[c], "parent": row_of.get(pa),
            "tissue": tissue, "is_a": isa_dn(c, parent, name, tissue),
            "container": container, "identity": identity, "guid": (container << 16) | identity,
            "rgb": list(col), "opacity": round(TISSUE_OPACITY[tissue], 3),
            "v_start": v_start, "v_count": len(px) - v_start, "fj": meshes_of[c],
        })

    if not px:
        sys.exit("no geometry gathered")

    cx = (min(px) + max(px)) / 2; cy = (min(py) + max(py)) / 2; cz = (min(pz) + max(pz)) / 2
    half = max(max(px) - min(px), max(py) - min(py), max(pz) - min(pz)) / 2 or 1.0
    inv = 1.0 / half
    for i in range(len(px)):
        px[i] = (px[i] - cx) * inv; py[i] = (py[i] - cy) * inv; pz[i] = (pz[i] - cz) * inv
    for nd in nodes:
        s, c = nd["v_start"], nd["v_count"]
        if c:
            xs = px[s:s + c]; ys = py[s:s + c]; zs = pz[s:s + c]
            nd["centroid"] = [sum(xs) / c, sum(ys) / c, sum(zs) / c]
            nd["bbox"] = [[min(xs), min(ys), min(zs)], [max(xs), max(ys), max(zs)]]
        else:
            nd["centroid"] = None; nd["bbox"] = None

    nvert = len(px); ntri = len(tris)
    bmin = (min(px), min(py), min(pz)); bmax = (max(px), max(py), max(pz))

    def qi8(v):
        return max(-127, min(127, int(round(v * 127))))

    buf = bytearray()
    buf += b"SPM1"
    buf += struct.pack("<III", nvert, ntri, len(nodes))
    buf += struct.pack("<3f", *bmin)
    buf += struct.pack("<3f", *bmax)
    for i in range(nvert):
        m = (nx[i] * nx[i] + ny[i] * ny[i] + nz[i] * nz[i]) ** 0.5 or 1.0
        buf += struct.pack("<3f", px[i], py[i], pz[i])
        buf += struct.pack("<3b", qi8(nx[i] / m), qi8(ny[i] / m), qi8(nz[i] / m))
        buf += struct.pack("<3B", cr[i], cg[i], cb[i])
        buf += struct.pack("<B", cop[i])
        buf += struct.pack("<H", crow[i])
    for (a, b, c) in tris:
        buf += struct.pack("<III", a, b, c)
    with open(out_path, "wb") as f:
        f.write(buf)

    tissue_hist = collections.Counter(nd["tissue"] for nd in nodes)
    pub = os.path.dirname(out_path)
    with open(os.path.join(pub, "torso.mesh.nodes.json"), "w", encoding="utf-8") as f:
        json.dump({"attribution": ATTRIBUTION, "decomposition": "is_a (BodyParts3D 4.0)",
                   "verts": nvert, "tris": ntri, "nodes": nodes}, f)
    manifest = {
        "source": "BodyParts3D 4.0 (DBCLS) is_a OBJ, vertex-cluster decimated, smooth normals",
        "license": "CC-BY 4.0 (site) / CC-BY-SA 2.1 JP (2013 files)",
        "attribution": ATTRIBUTION,
        "format": ("SPM1 (indexed triangle mesh: vert 21 B [pos 3f|normal 3i8|rgb 3u8|"
                   "opacity u8|node_row u16] + tri 12 B [3x u32]); node SoA in torso.mesh.nodes.json"),
        "build": "filled smooth triangle surface, is_a tissue colour, k-NN-free (mesh)",
        "concepts": len(nodes), "verts": nvert, "tris": ntri, "cell_mm": cell_mm,
        "bbox_min": list(bmin), "bbox_max": list(bmax), "tissues": dict(tissue_hist),
    }
    with open(os.path.join(pub, "torso.mesh.manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"baked {out_path}: {nvert:,} verts, {ntri:,} tris, {len(nodes)} structures, cell {cell_mm}mm",
          file=sys.stderr)
    print(f"  tissues: {dict(tissue_hist)}", file=sys.stderr)
    print(f"  SPM1 {len(buf):,} B + torso.mesh.nodes.json + manifest", file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2], float(a[3]) if len(a) > 3 else CELL_MM_DEFAULT)
