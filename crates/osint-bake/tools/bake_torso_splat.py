#!/usr/bin/env python3
"""Bake a REAL-anatomy gaussian-splat asset for the cockpit /torso pages from
BodyParts3D (the FMA-keyed 3D mesh database).

FMA itself has zero geometry (it is a symbolic ontology). BodyParts3D (DBCLS)
realises FMA concepts as OBJ meshes in one shared whole-body coordinate frame and
keys each concept on its FMA id — so a torso splat needs no synthesized
positions: we read the real vertices. The FMA `part_of` tree (which IS the
mereotopological containment of those meshes, per BodyParts3D NAR 2009 Table 1)
selects the torso subtree and colours it.

Inputs (BodyParts3D 4.0, external — NOT committed; download once):
  partof_inclusion_relation_list.txt   FMA part-of tree  (parent_id name child_id name)
  partof_element_parts.txt             FMA concept -> FJ element OBJ files
  partof_parts_list_e.txt              FMA concept -> English name
  partof_BP3D_4.0_obj_99/FJ####.obj    the meshes (decimated 99%, shared frame)
  https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/

Output: cockpit/public/torso.splat (SPL1 binary) + cockpit/public/torso.manifest.json

LICENSE / ATTRIBUTION (required, CC-BY 4.0):
  "BodyParts3D, (c) The Database Center for Life Science
   licensed under CC Attribution 4.0 International"

SPL1 binary layout (little-endian), the cockpit decoder mirrors this:
  header 36 B:  magic "SPL1"(4) | count u32 | radius f32 | bbox_min 3xf32 | bbox_max 3xf32
  body  count x 16 B:  pos 3xf32 (12) | r,g,b u8 (3) | opacity u8 (1)
positions are in the NORMALIZED frame (centroid at origin, max half-extent = 1.0).

Usage: python3 bake_torso_splat.py <bodyparts3d_dir> <obj_dir> <out.splat> [root_fma] [budget]
"""
import collections
import colorsys
import json
import os
import struct
import sys

ROOT_DEFAULT = "FMA7181"        # trunk (synonym: Torso)
BUDGET_DEFAULT = 250_000        # ~4 MB asset; downsample vertices to this many gaussians
ATTRIBUTION = ("BodyParts3D, (c) The Database Center for Life Science "
               "licensed under CC Attribution 4.0 International")


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


def region_of(cid, root, parent):
    """The depth-1 ancestor under `root` (thoracic/abdominal segment, body wall,
    perineum) — the gross torso region the concept belongs to."""
    cur, prev = cid, cid
    while cur in parent and cur != root:
        prev, cur = cur, parent[cur]
    return prev if cur == root else cid


def read_obj_vertices(path):
    out = []
    with open(path, "rb") as f:
        for ln in f:
            if ln[:2] == b"v ":
                p = ln.split()
                out.append((float(p[1]), float(p[2]), float(p[3])))
    return out


def concept_color(idx):
    """Deterministic distinct hue per concept (golden-angle walk) so each
    anatomical structure reads as its own colour."""
    h = (idx * 0.6180339887498949) % 1.0
    r, g, b = colorsys.hsv_to_rgb(h, 0.62, 0.96)
    return (int(r * 255), int(g * 255), int(b * 255))


def main(bp_dir, obj_dir, out_path, root=ROOT_DEFAULT, budget=BUDGET_DEFAULT):
    parent, children, name, elems = load_tree(bp_dir)
    if root not in children and root not in name:
        sys.exit(f"root {root} not in BodyParts3D part-of tree")
    order, depth = bfs(root, children)

    # gather (x,y,z, r,g,b) per vertex; colour each mesh by its DEEPEST owning
    # concept. BodyParts3D `element_parts` lists every descendant element under a
    # compound concept, so we claim meshes deepest-first — each structure (leaf)
    # owns its own meshes and gets its own hue; the root only mops up unowned ones.
    cidx = {cid: i for i, cid in enumerate(order)}   # stable BFS index -> colour
    pts = []                      # list of (x,y,z,r,g,b)
    seen_mesh = set()
    regions = {}                  # region cid -> name
    used_concepts = 0
    for cid in sorted(order, key=lambda c: -depth[c]):
        fjs = elems.get(cid, [])
        if not fjs:
            continue
        col = concept_color(cidx[cid])
        reg = region_of(cid, root, parent)
        got = False
        for fj in fjs:
            if fj in seen_mesh:
                continue
            seen_mesh.add(fj)
            p = os.path.join(obj_dir, fj + ".obj")
            if not os.path.exists(p):
                continue
            for (x, y, z) in read_obj_vertices(p):
                pts.append((x, y, z, col[0], col[1], col[2]))
                got = True
        if got:
            used_concepts += 1
            regions.setdefault(reg, name.get(reg, reg))

    if not pts:
        sys.exit("no vertices gathered — check obj_dir / root")

    # deterministic downsample to the budget (uniform global stride).
    stride = max(1, round(len(pts) / budget))
    sampled = pts[::stride]

    # recenter to centroid, normalize so max half-extent = 1.0.
    xs = [p[0] for p in sampled]; ys = [p[1] for p in sampled]; zs = [p[2] for p in sampled]
    cx, cy, cz = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2, (min(zs) + max(zs)) / 2
    half = max(max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)) / 2 or 1.0
    inv = 1.0 / half

    gx = [(p[0] - cx) * inv for p in sampled]
    gy = [(p[1] - cy) * inv for p in sampled]
    gz = [(p[2] - cz) * inv for p in sampled]
    bmin = (min(gx), min(gy), min(gz))
    bmax = (max(gx), max(gy), max(gz))
    radius = 0.0045
    opacity = 220

    n = len(sampled)
    buf = bytearray()
    buf += b"SPL1"
    buf += struct.pack("<I", n)
    buf += struct.pack("<f", radius)
    buf += struct.pack("<3f", *bmin)
    buf += struct.pack("<3f", *bmax)
    for i in range(n):
        buf += struct.pack("<3f", gx[i], gy[i], gz[i])
        buf += struct.pack("<3B", sampled[i][3], sampled[i][4], sampled[i][5])
        buf += struct.pack("<B", opacity)

    with open(out_path, "wb") as f:
        f.write(buf)

    manifest = {
        "source": "BodyParts3D 4.0 (DBCLS) part-of OBJ, decimated 99%",
        "license": "CC-BY 4.0",
        "attribution": ATTRIBUTION,
        "root_fma": root,
        "root_name": name.get(root, root),
        "concepts": used_concepts,
        "meshes": len(seen_mesh),
        "vertices_total": len(pts),
        "gaussians": n,
        "radius": radius,
        "bbox_min": list(bmin),
        "bbox_max": list(bmax),
        "regions": [{"fma": k, "name": v} for k, v in regions.items()],
        "generated_by": "crates/osint-bake/tools/bake_torso_splat.py",
        "format": "SPL1: hdr[magic4|count u32|radius f32|bbox_min 3f|bbox_max 3f]; "
                  "body count*[pos 3f|rgb 3u8|opacity u8]; little-endian",
    }
    mpath = os.path.join(os.path.dirname(out_path), "torso.manifest.json")
    with open(mpath, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"baked {out_path}: {n:,} gaussians from {len(seen_mesh)} meshes / "
          f"{used_concepts} concepts ({len(pts):,} verts, stride {stride})", file=sys.stderr)
    print(f"  bbox [{bmin[0]:.2f},{bmin[1]:.2f},{bmin[2]:.2f}].."
          f"[{bmax[0]:.2f},{bmax[1]:.2f},{bmax[2]:.2f}]  size {len(buf):,} B", file=sys.stderr)
    print(f"  regions: {', '.join(name.get(k, k) for k in regions)}", file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2], a[3],
         a[4] if len(a) > 4 else ROOT_DEFAULT,
         int(a[5]) if len(a) > 5 else BUDGET_DEFAULT)
