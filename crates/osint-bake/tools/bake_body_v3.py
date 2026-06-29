#!/usr/bin/env python3
"""Bake the FULL-RESOLUTION anatomy body geometry + the V3-cascade concept table
that the Rust `body` bin fuses into `cockpit/public/body.soa`.

This is the un-decimated twin of `bake_torso_mesh.py`. Where that tool
vertex-cluster-decimates at cell_mm 3.6 (collapsing the body to ~600 K tris / the
"2000 bubbles" the operator rejected), this one keeps **ALL points** from the 2234
BodyParts3D is_a OBJ meshes (~4.2 M verts / 6.7 M tris) — the full polygon surface,
no confetti. Optional `cell_mm > 0` welds only EXACT-coincident points (a light
dedup, still "all points"); `cell_mm = 0` keeps every vertex verbatim.

It reuses the SAME is_a classifier (load_isa / tissue_of / isa_dn / colours) as the
splat + decimated-mesh bakes, so tissue, names and colours are identical. The ONE
addition over `bake_torso_mesh.py`: per concept it emits the **is_a ancestor sibling
-rank chain** (`cascade`: up to 5 tier identity bytes, root->leaf) so the Rust side
can mint the `CLASSID_FMA_V3` NodeGuid on the (part_of:is_a) cascade exactly the way
`crates/osint-bake/src/bin/fma.rs` mints the heart slice — the partonomy IS the key.

Outputs (into <out_dir>):
  body.spm1        SPM1 geometry (same wire as torso.mesh; full-res)
  body.nodes.json  concept table: per node {row, fma, name, tissue, rgb, opacity,
                   depth, parent_row, v_start, v_count, cascade:[tier ids], identity}

The Rust `body` bin reads both and writes body.soa (BSO1 = V3 node table + SPM1
block). Geometry/data: BodyParts3D, (c) The Database Center for Life Science,
CC-BY 4.0 / CC-BY-SA 2.1 JP. Attribution shown in-view (licence requirement).

Usage: python3 bake_body_v3.py <scratch_dir> <out_dir> [cell_mm=0]
"""
import collections
import json
import os
import struct
import sys

from bake_torso_splat import (
    ATTRIBUTION, CONTAINER_ID, ISA_ROOT, TISSUE_OPACITY,
    bfs, isa_dn, load_isa, tissue_color, tissue_of,
)

# Compartment LAYER id (the per-vertex byte-19 gating key the /body viewer's buttons
# toggle) — mirrors fma's cockpit_bake layer_of / FmaBody.tsx LAYERS, but maps the
# FINER is_a tissues onto the same 8 layers (1 skin·2 muscle·3 organ·4 skeleton·
# 5 vessel·6 nerve·7 connective·8 other). This is what makes /body compartmentalized
# like /fma-body instead of a single depth-peel floor like /torso-live.
LAYER_OF = {
    "skin": 1, "flesh": 1,
    "muscle": 2,
    "heart": 3, "lung": 3, "liver": 3, "kidney": 3, "gi": 3, "gland": 3, "viscus": 3,
    "bone": 4, "cartilage": 4,
    "artery": 5, "vein": 5, "vessel": 5,
    "nerve": 6,
    "connective": 7,  # ligaments / tendons / membranes / fascia / aponeuroses / retinacula
}  # default → 8 "other"


def read_obj_mesh(path):
    """(verts, normals aligned to verts, faces). BodyParts3D uses `f v//vn` with
    vn index == v index, so vn is 1:1 with v; fall back to +Z if counts disagree."""
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


def weld_exact(verts, normals, faces, inv_h):
    """Light weld: collapse only points that fall in the SAME 1/inv_h cell (inv_h
    large => only exact-coincident points merge). inv_h <= 0 => keep every vertex."""
    if inv_h <= 0:
        return verts, normals, faces
    cell_of, acc, remap = {}, [], [0] * len(verts)
    ox = min(v[0] for v in verts); oy = min(v[1] for v in verts); oz = min(v[2] for v in verts)
    for i, (x, y, z) in enumerate(verts):
        key = (round((x - ox) * inv_h), round((y - oy) * inv_h), round((z - oz) * inv_h))
        j = cell_of.get(key)
        nx, ny, nz = normals[i]
        if j is None:
            j = len(acc); cell_of[key] = j
            acc.append([x, y, z, nx, ny, nz, 1])
        else:
            a = acc[j]
            a[0] += x; a[1] += y; a[2] += z; a[3] += nx; a[4] += ny; a[5] += nz; a[6] += 1
        remap[i] = j
    nv, nn = [], []
    for a in acc:
        c = a[6]
        nv.append((a[0] / c, a[1] / c, a[2] / c))
        nl = (a[3] * a[3] + a[4] * a[4] + a[5] * a[5]) ** 0.5 or 1.0
        nn.append((a[3] / nl, a[4] / nl, a[5] / nl))
    nf = [(remap[a], remap[b], remap[c]) for (a, b, c) in faces
          if remap[a] != remap[b] and remap[b] != remap[c] and remap[a] != remap[c]]
    return nv, nn, nf


def cascade_of(c, parent, children, depth, rank_cache):
    """The is_a ancestor sibling-rank chain root->self, up to 5 tier identity bytes.
    Tier k = 1-based rank of the ancestor at depth k among its parent's children
    (deterministic: children sorted by FMA id). Mirrors fma.rs's HHTL [mixin:id]
    identities; the mixin (kind by depth) is assigned Rust-side."""
    chain = []
    cur = c
    while cur is not None:
        chain.append(cur)
        cur = parent.get(cur)
    chain.reverse()  # root .. self

    def rank(node):
        p = parent.get(node)
        if p is None:
            return 0
        r = rank_cache.get(node)
        if r is None:
            sibs = sorted(children[p])
            for k, s in enumerate(sibs):
                rank_cache[s] = (k + 1) & 0xFF
            r = rank_cache.get(node, 0)
        return r

    return [rank(n) for n in chain[:5]]


def main(scratch, out_dir, cell_mm=0.0):
    parent, children, name, elems, canon = load_isa(scratch)
    order, depth = bfs(ISA_ROOT, children)
    have = set(order)
    isa_obj = os.path.join(scratch, "isa_BP3D_4.0_obj_99")
    pof_obj = os.path.join(scratch, "partof", "partof_BP3D_4.0_obj_99")

    def obj_path(fj):
        p = os.path.join(isa_obj, fj + ".obj")
        return p if os.path.exists(p) else os.path.join(pof_obj, fj + ".obj")

    # deepest-first claim so the finest is_a type owns each shared mesh (= the splat/mesh)
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

    inv_h = 1.0 / float(cell_mm) if cell_mm and cell_mm > 0 else 0.0
    tcache, rank_cache = {}, {}
    px, py, pz, nx, ny, nz, cr, cg, cb, cop, crow = ([] for _ in range(11))
    tris = []
    nodes = []
    row_of = {c: r for r, c in enumerate(kept)}

    for c in kept:
        r = row_of[c]
        nm = canon.get(c, name.get(c, c))
        tissue = tissue_of(c, parent, name, canon, tcache)
        col = tissue_color(tissue, r)
        layer_id = LAYER_OF.get(tissue, 8)  # byte-19 = compartment layer (not opacity)
        v_start = len(px)
        for fj in meshes_of[c]:
            p = obj_path(fj)
            if not os.path.exists(p):
                continue
            vs, ns, faces = read_obj_mesh(p)
            if not faces:
                continue
            nv, nn, nf = weld_exact(vs, ns, faces, inv_h)
            base = len(px)
            for (x, y, z), (ax, ay, az) in zip(nv, nn):
                px.append(x); py.append(y); pz.append(z)
                nx.append(ax); ny.append(ay); nz.append(az)
                cr.append(col[0]); cg.append(col[1]); cb.append(col[2])
                cop.append(layer_id); crow.append(r)
            for (a, b, cc) in nf:
                tris.append((base + a, base + b, base + cc))
        pa, seen = parent.get(c), 0
        while pa is not None and pa not in row_of and seen < 24:
            pa = parent.get(pa); seen += 1
        nodes.append({
            "row": r, "fma": c, "name": nm, "tissue": tissue, "depth": depth[c],
            "parent_row": row_of.get(pa, -1), "container": CONTAINER_ID[tissue],
            "rgb": list(col), "layer": layer_id, "opacity": round(TISSUE_OPACITY[tissue], 3),
            "is_a": isa_dn(c, parent, name, tissue),
            "cascade": cascade_of(c, parent, children, depth, rank_cache),
            "v_start": v_start, "v_count": len(px) - v_start, "fj": meshes_of[c],
        })

    if not px:
        sys.exit("no geometry gathered")

    cx = (min(px) + max(px)) / 2; cy = (min(py) + max(py)) / 2; cz = (min(pz) + max(pz)) / 2
    half = max(max(px) - min(px), max(py) - min(py), max(pz) - min(pz)) / 2 or 1.0
    inv = 1.0 / half
    for i in range(len(px)):
        px[i] = (px[i] - cx) * inv; py[i] = (py[i] - cy) * inv; pz[i] = (pz[i] - cz) * inv

    nvert = len(px); ntri = len(tris)
    bmin = (min(px), min(py), min(pz)); bmax = (max(px), max(py), max(pz))

    def qi8(v):
        return max(-127, min(127, int(round(v * 127))))

    os.makedirs(out_dir, exist_ok=True)
    spm1 = os.path.join(out_dir, "body.spm1")
    with open(spm1, "wb") as f:
        f.write(b"SPM1")
        f.write(struct.pack("<III", nvert, ntri, len(nodes)))
        f.write(struct.pack("<3f", *bmin)); f.write(struct.pack("<3f", *bmax))
        buf = bytearray()
        for i in range(nvert):
            m = (nx[i] * nx[i] + ny[i] * ny[i] + nz[i] * nz[i]) ** 0.5 or 1.0
            buf += struct.pack("<3f", px[i], py[i], pz[i])
            buf += struct.pack("<3b", qi8(nx[i] / m), qi8(ny[i] / m), qi8(nz[i] / m))
            buf += struct.pack("<3B", cr[i], cg[i], cb[i])
            buf += struct.pack("<B", cop[i])
            buf += struct.pack("<H", crow[i] & 0xFFFF)
            if len(buf) >= (1 << 20):
                f.write(buf); buf = bytearray()
        f.write(buf)
        buf = bytearray()
        for (a, b, c) in tris:
            buf += struct.pack("<III", a, b, c)
            if len(buf) >= (1 << 20):
                f.write(buf); buf = bytearray()
        f.write(buf)

    with open(os.path.join(out_dir, "body.nodes.json"), "w", encoding="utf-8") as f:
        json.dump({"attribution": ATTRIBUTION, "decomposition": "is_a (BodyParts3D 4.0)",
                   "verts": nvert, "tris": ntri, "cell_mm": cell_mm, "nodes": nodes}, f)

    tissue_hist = collections.Counter(nd["tissue"] for nd in nodes)
    print(f"baked {spm1}: {nvert:,} verts, {ntri:,} tris, {len(nodes)} concepts, "
          f"cell_mm={cell_mm} (0 = ALL points)", file=sys.stderr)
    print(f"  tissues: {dict(tissue_hist)}", file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2], float(a[3]) if len(a) > 3 else 0.0)
