#!/usr/bin/env python3
"""Bake the REAL-anatomy torso gaussian splat from BodyParts3D — v2: anisotropic
(surface normals) + per-node SoA with O(1) tenant tags.

v1 emitted flat points (pos+rgb). v2 adds, in one pass over the meshes:
  - the per-vertex SURFACE NORMAL (BodyParts3D OBJ ships `vn`) -> orientation, so
    consumers can render oriented surface-tangent gaussians ("connect the dots")
    instead of isotropic blobs;
  - a per-gaussian NODE-ROW tag + a NODE SoA (one row per FMA structure) carrying
    the value-tenants of that node's identity: fma id, name, partonomy depth +
    HHTL tier-ranks, colour, the gaussian RANGE (start+count), and the OBJ-geometry
    summary (centroid + bbox + FJ mesh handles). A consumer builds the "switch"
    (identity -> row hashtable) once and reads any tenant in O(1). Geometry, graph,
    and splat become three tenants of one identity.

Source: BodyParts3D 4.0 (DBCLS), FMA-keyed OBJ meshes in one shared whole-body
frame. concept id == FMA id. Pairwise inputs (the user-supplied text files):
  partof_inclusion_relation_list.txt  FMA parent<->child  (the partonomy / HHTL cascade)
  partof_element_parts.txt            FMA concept<->FJ mesh (geometry binding)
  partof_parts_list_e.txt             FMA<->repr-id<->name  (labels)
  partof_BP3D_4.0_obj_99/FJ####.obj   meshes (v + vn, shared frame)

LICENCE / ATTRIBUTION (required): BodyParts3D, (c) The Database Center for Life
Science. The 2013 OBJ files embed CC-BY-SA 2.1 JP; the current DBCLS site relicenses
to CC-BY 4.0 -- both are carried below to be safe.

Outputs (cockpit/public/):
  torso.splat         SPL2 binary  (hdr 40B: "SPL2"|count u32|node_count u32|radius f32|
                                    bbox_min 3f|bbox_max 3f; body count*21B:
                                    pos 3f|normal 3i8|rgb 3u8|opacity u8|node_row u16)
  torso.nodes.json    the node SoA (one row per FMA structure)
  torso.manifest.json summary + attribution

Usage: python3 bake_torso_splat.py <bp_dir> <obj_dir> <out.splat> [root_fma] [budget]
"""
import collections
import colorsys
import json
import os
import struct
import sys

ROOT_DEFAULT = "FMA7181"        # trunk (synonym: Torso)
BUDGET_DEFAULT = 250_000
ATTRIBUTION = ("BodyParts3D, (c) The Database Center for Life Science. "
               "Current site licence: CC-BY 4.0; 2013 mesh files embed "
               "CC-BY-SA 2.1 Japan.")


def load_tree(bp_dir):
    parent, children, name = {}, collections.defaultdict(list), {}
    with open(os.path.join(bp_dir, "partof_inclusion_relation_list.txt"), encoding="utf-8") as f:
        next(f)
        for line in f:
            p, pn, c, cn = line.rstrip("\n").split("\t")
            parent[c] = p
            children[p].append(c)
            name[p], name[c] = pn, cn
    elems = collections.defaultdict(list)
    with open(os.path.join(bp_dir, "partof_element_parts.txt"), encoding="utf-8") as f:
        next(f)
        for line in f:
            cid, _nm, fj = line.rstrip("\n").split("\t")
            elems[cid].append(fj)
    for v in children.values():
        v.sort()  # deterministic sibling order -> stable tier ranks
    return parent, children, name, elems


def bfs(root, children):
    depth, order, q = {root: 0}, [root], [root]
    while q:
        n = q.pop(0)
        for c in children.get(n, []):
            if c not in depth:
                depth[c] = depth[n] + 1
                order.append(c)
                q.append(c)
    return order, depth


def tier_ranks(node, parent, children):
    """The sibling-rank chain root->node (the HHTL tier address / GUID content)."""
    chain = []
    cur = node
    while cur in parent:
        sibs = children[parent[cur]]
        chain.append(sibs.index(cur) + 1)  # 1-based rank under parent
        cur = parent[cur]
    chain.reverse()
    return chain


def read_obj_v_vn(path):
    """Return parallel (positions, normals). BodyParts3D OBJ ships `vn`; faces are
    `v//vn` with v_idx == vn_idx, so vertex i pairs with normal i."""
    vs, ns = [], []
    with open(path, "rb") as f:
        for ln in f:
            if ln[:2] == b"v ":
                p = ln.split()
                vs.append((float(p[1]), float(p[2]), float(p[3])))
            elif ln[:3] == b"vn ":
                p = ln.split()
                ns.append((float(p[1]), float(p[2]), float(p[3])))
    if len(ns) != len(vs):
        ns = [(0.0, 0.0, 1.0)] * len(vs)  # fallback: no usable normals
    return vs, ns


def concept_color(idx):
    h = (idx * 0.6180339887498949) % 1.0
    r, g, b = colorsys.hsv_to_rgb(h, 0.34, 0.78)  # muted pastel per structure
    return (int(r * 255), int(g * 255), int(b * 255))


def main(bp_dir, obj_dir, out_path, root=ROOT_DEFAULT, budget=BUDGET_DEFAULT):
    parent, children, name, elems = load_tree(bp_dir)
    if root not in children and root not in name:
        sys.exit(f"root {root} not in BodyParts3D part-of tree")
    order, depth = bfs(root, children)
    row_of = {fma: i for i, fma in enumerate(order)}

    # claim each mesh to its DEEPEST owning concept (compound concepts list all
    # descendant elements; deepest-first so leaves own their own meshes).
    owner = {}
    for fma in sorted(order, key=lambda c: -depth[c]):
        for fj in elems.get(fma, []):
            owner.setdefault(fj, fma)
    meshes_of = collections.defaultdict(list)
    for fj, fma in owner.items():
        meshes_of[fma].append(fj)
    for v in meshes_of.values():
        v.sort()

    # pass 1: total vertex count -> global stride for the budget
    total_v = 0
    vcache = {}
    for fma in order:
        for fj in meshes_of.get(fma, []):
            vs, ns = read_obj_v_vn(os.path.join(obj_dir, fj + ".obj"))
            vcache[fj] = (vs, ns)
            total_v += len(vs)
    stride = max(1, round(total_v / budget))

    # pass 2: gather gaussians GROUPED by node (contiguous ranges) with the
    # per-vertex normal; build node SoA rows.
    gx, gy, gz, gnx, gny, gnz, gr, gg, gb, grow = ([] for _ in range(10))
    nodes = []
    for fma in order:
        r = row_of[fma]
        col = concept_color(r)
        g_start = len(gx)
        for fj in meshes_of.get(fma, []):
            vs, ns = vcache[fj]
            for k in range(0, len(vs), stride):
                (x, y, z) = vs[k]
                (nx, ny, nz) = ns[k]
                gx.append(x); gy.append(y); gz.append(z)
                gnx.append(nx); gny.append(ny); gnz.append(nz)
                gr.append(col[0]); gg.append(col[1]); gb.append(col[2])
                grow.append(r)
        g_count = len(gx) - g_start
        nodes.append({
            "row": r, "fma": fma, "name": name.get(fma, fma), "depth": depth[fma],
            "parent": row_of.get(parent.get(fma)) if fma in parent else None,
            "tiers": tier_ranks(fma, parent, children),
            "rgb": list(col), "g_start": g_start, "g_count": g_count,
            "fj": meshes_of.get(fma, []),
        })

    if not gx:
        sys.exit("no vertices gathered")

    # recenter to centroid, uniform-normalize so max half-extent = 1 (normals,
    # being directions under a recenter + uniform scale, stay valid).
    cx = (min(gx) + max(gx)) / 2; cy = (min(gy) + max(gy)) / 2; cz = (min(gz) + max(gz)) / 2
    half = max(max(gx) - min(gx), max(gy) - min(gy), max(gz) - min(gz)) / 2 or 1.0
    inv = 1.0 / half
    for i in range(len(gx)):
        gx[i] = (gx[i] - cx) * inv; gy[i] = (gy[i] - cy) * inv; gz[i] = (gz[i] - cz) * inv

    # per-node centroid + bbox in the normalized frame (the OBJ-geometry tenant).
    for nd in nodes:
        s, c = nd["g_start"], nd["g_count"]
        if c == 0:
            nd["centroid"] = None; nd["bbox"] = None
            continue
        xs = gx[s:s + c]; ys = gy[s:s + c]; zs = gz[s:s + c]
        nd["centroid"] = [sum(xs) / c, sum(ys) / c, sum(zs) / c]
        nd["bbox"] = [[min(xs), min(ys), min(zs)], [max(xs), max(ys), max(zs)]]

    n = len(gx)
    bmin = (min(gx), min(gy), min(gz)); bmax = (max(gx), max(gy), max(gz))
    radius = 0.0035

    def qi8(v):  # normalize a normal component to a signed byte
        return max(-127, min(127, int(round(v * 127))))

    buf = bytearray()
    buf += b"SPL2"
    buf += struct.pack("<II", n, len(nodes))
    buf += struct.pack("<f", radius)
    buf += struct.pack("<3f", *bmin)
    buf += struct.pack("<3f", *bmax)
    for i in range(n):
        # renormalize the (possibly fallback) normal
        nx, ny, nz = gnx[i], gny[i], gnz[i]
        m = (nx * nx + ny * ny + nz * nz) ** 0.5 or 1.0
        buf += struct.pack("<3f", gx[i], gy[i], gz[i])
        buf += struct.pack("<3b", qi8(nx / m), qi8(ny / m), qi8(nz / m))
        buf += struct.pack("<3B", gr[i], gg[i], gb[i])
        buf += struct.pack("<B", 220)
        buf += struct.pack("<H", grow[i])
    with open(out_path, "wb") as f:
        f.write(buf)

    pub = os.path.dirname(out_path)
    with open(os.path.join(pub, "torso.nodes.json"), "w", encoding="utf-8") as f:
        json.dump({"attribution": ATTRIBUTION, "root": root, "radius": radius,
                   "count": n, "nodes": nodes}, f)
    manifest = {
        "source": "BodyParts3D 4.0 (DBCLS) part-of OBJ, decimated 99%, with vn normals",
        "license": "CC-BY 4.0 (site) / CC-BY-SA 2.1 JP (2013 files)",
        "attribution": ATTRIBUTION,
        "format": "SPL2 (anisotropic + node-row tags); node SoA in torso.nodes.json",
        "root_fma": root, "root_name": name.get(root, root),
        "concepts": len(nodes), "meshes": len(owner), "gaussians": n,
        "radius": radius, "bbox_min": list(bmin), "bbox_max": list(bmax),
        "owners": sum(1 for nd in nodes if nd["g_count"] > 0),
    }
    with open(os.path.join(pub, "torso.manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    owners = sum(1 for nd in nodes if nd["g_count"] > 0)
    print(f"baked {out_path}: {n:,} gaussians, {len(nodes)} node rows "
          f"({owners} own meshes), {len(owner)} meshes, stride {stride}", file=sys.stderr)
    print(f"  SPL2 {len(buf):,} B + torso.nodes.json + manifest", file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2], a[3],
         a[4] if len(a) > 4 else ROOT_DEFAULT,
         int(a[5]) if len(a) > 5 else BUDGET_DEFAULT)
