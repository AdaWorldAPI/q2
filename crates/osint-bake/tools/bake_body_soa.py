#!/usr/bin/env python3
"""Body SoA-column emitter (option-2 rebake, LOCKED design 2026-06-27).

Supersedes bake_body_v3.py's AoS BSO1. Emits the geometry as **struct-of-arrays
columns** (raw little-endian, 64-byte padded — MultiLaneColumn-shaped) plus the
per-concept cascade table and the label/material **codebooks** (indices, never
raw text). A standalone Rust stage (`body_soa` bin) then mints the address GUID
(part_of:is_a), encodes the 2 helices, and assembles the SoA wire.

Full resolution: every OBJ vertex (≈4.2 M verts / 6.68 M tris) — NO decimation.

Per-VERTEX columns (SoA row = vertex; node_row links to its concept):
  body.pos    f32×3 · N   normalized [-1,1] XYZ (location; Z=slice, X·Y=in-slice grid)
  body.nrm    f32×3 · N   smooth normal (helix-normal source for the Rust stage)
  body.row    u32   · N   concept index (the SoA-linked identity → concept table)
  body.idx    u32×3 · T   triangle vertex indices

Per-CONCEPT table (body.concepts.json), 1 row per is_a-meshed structure:
  {row, fma, part_of[6], is_a[6], material, label, centroid, v_start, v_count}
  part_of[k]/is_a[k] = sibling rank at HHTL level k (Rust packs the 8:8 address tiers)
  material = index into the Doppler/material codebook · label = index into the label codebook

Codebooks (content stores, resolved via ClassView — wire carries only the index):
  body.labels.json     [text, …]                 unique concept names
  body.materials.json  [{id,name,doppler,rgb}, …] 6 flow/solid prototypes

Data: BodyParts3D 4.0 (DBCLS), CC-BY 4.0 / CC-BY-SA 2.1 JP.
Usage: python3 bake_body_soa.py <scratch_dir> <out_dir>
"""
import collections
import json
import os
import struct
import sys

from bake_torso_splat import (
    ATTRIBUTION, ISA_ROOT, bfs, isa_dn, load_isa, tissue_color, tissue_of,
)

# ── Doppler / material prototypes (the "texture for tubes") — radiologykey
#    abdominal-vessel signatures + solid fallbacks. Wire stores the index. ──
MATERIALS = [
    {"id": 0, "name": "low_resistance_artery", "doppler": "continuous_forward_diastolic", "rgb": [201, 58, 52]},
    {"id": 1, "name": "high_resistance_artery", "doppler": "triphasic", "rgb": [176, 42, 38]},
    {"id": 2, "name": "portal_venous", "doppler": "continuous_undulating_hepatopetal", "rgb": [70, 110, 180]},
    {"id": 3, "name": "systemic_venous", "doppler": "phasic_respiratory_cardiac", "rgb": [66, 95, 176]},
    {"id": 4, "name": "solid_tissue", "doppler": "none", "rgb": [200, 150, 140]},
    {"id": 5, "name": "neural", "doppler": "none", "rgb": [226, 205, 88]},
]
TISSUE_MATERIAL = {
    "artery": 0, "vein": 3, "vessel": 3, "nerve": 5,
    # heart is a chambered ORGAN, not a tube — mapping it to artery(0) made the slicer-
    # fill sweep a PCA-centerline lumen rod through it. Fall through to solid_tissue
    # (like every other solid: bone/cartilage/muscle/organs/skin/flesh).
}


def read_obj_mesh(path):
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


def load_partof(scratch_or_up):
    """part_of inclusion tree (regional containment) → parent/children maps."""
    cand = [
        os.path.join(scratch_or_up, "partof_inclusion_relation_list.txt"),
        os.path.join(scratch_or_up, "partof", "partof_inclusion_relation_list.txt"),
    ]
    path = next((p for p in cand if os.path.exists(p)), None)
    parent, children = {}, collections.defaultdict(list)
    if path:
        with open(path, encoding="utf-8") as f:
            next(f)
            for ln in f:
                p, _pn, c, _cn = ln.rstrip("\n").split("\t")
                parent[c] = p
                children[p].append(c)
        for v in children.values():
            v.sort()
    return parent, children


def sibrank6(node, parent, children):
    """ancestor sibling-rank chain root→self, 6 levels (1-based; 0 = root/absent)."""
    chain, cur = [], node
    while cur is not None:
        chain.append(cur); cur = parent.get(cur)
    chain.reverse()
    out = []
    for n in chain[:6]:
        p = parent.get(n)
        if p is None:
            out.append(0)
        else:
            sibs = children[p]
            out.append(((sibs.index(n) + 1) & 0xFF) if n in sibs else 0)
    while len(out) < 6:
        out.append(0)
    return out


def main(scratch, out_dir):
    up = "/root/.claude/uploads/2e96121c-3007-5a1a-9af1-10b1dfd06f58"
    parent_isa, children_isa, name, elems, canon = load_isa(scratch)
    parent_pof, children_pof = load_partof(scratch)
    if not parent_pof:
        parent_pof, children_pof = load_partof(up)
    order, depth = bfs(ISA_ROOT, children_isa)
    have = set(order)
    isa_obj = os.path.join(scratch, "isa_BP3D_4.0_obj_99")
    pof_obj = os.path.join(scratch, "partof", "partof_BP3D_4.0_obj_99")

    def obj_path(fj):
        p = os.path.join(isa_obj, fj + ".obj")
        return p if os.path.exists(p) else os.path.join(pof_obj, fj + ".obj")

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
    row_of = {c: r for r, c in enumerate(kept)}

    labels, label_idx = [], {}
    def label_index(s):
        i = label_idx.get(s)
        if i is None:
            i = len(labels); label_idx[s] = i; labels.append(s)
        return i

    tcache = {}
    px, py, pz, nx, ny, nz, crow = ([] for _ in range(7))
    tris = []
    concept_rows = []

    for c in kept:
        r = row_of[c]
        nm = canon.get(c, name.get(c, c))
        tissue = tissue_of(c, parent_isa, name, canon, tcache)
        # Liver parenchyma is modelled as Couinaud "hepatovenous segment N" — named for
        # its venous drainage, so tissue_of tags it 'vein'. That coloured the whole liver
        # blue and slicer-filled it as a tube (it read as a blue vessel blob above the
        # colon). It is solid liver tissue; only the true hepatic/portal *veins* (which
        # all carry 'vein' in their name) stay vessels. Reclassify to liver → solid
        # organ colour + the organ compartment.
        if "hepatovenous segment" in nm.lower():
            tissue = "liver"
        material = TISSUE_MATERIAL.get(tissue, 4)
        v_start = len(px)
        for fj in meshes_of[c]:
            p = obj_path(fj)
            if not os.path.exists(p):
                continue
            vs, ns, faces = read_obj_mesh(p)
            if not faces:
                continue
            base = len(px)
            for (x, y, z), (ax, ay, az) in zip(vs, ns):
                px.append(x); py.append(y); pz.append(z)
                nx.append(ax); ny.append(ay); nz.append(az)
                crow.append(r)
            for (a, b, cc) in faces:
                tris.append((base + a, base + b, base + cc))
        concept_rows.append({
            "row": r, "fma": c, "name_idx": label_index(nm), "material": material,
            "tissue": tissue, "depth": depth[c],
            "part_of": sibrank6(c, parent_pof, children_pof),
            "is_a": sibrank6(c, parent_isa, children_isa),
            "is_a_dn": isa_dn(c, parent_isa, name, tissue),
            "v_start": v_start, "v_count": len(px) - v_start,
            "parent_row": row_of.get(parent_isa.get(c), -1),
        })

    if not px:
        sys.exit("no geometry")

    # normalize XYZ to [-1,1] centred (location columns, GPU/slicer native)
    cx = (min(px) + max(px)) / 2; cy = (min(py) + max(py)) / 2; cz = (min(pz) + max(pz)) / 2
    half = max(max(px) - min(px), max(py) - min(py), max(pz) - min(pz)) / 2 or 1.0
    inv = 1.0 / half
    nvert, ntri = len(px), len(tris)

    os.makedirs(out_dir, exist_ok=True)
    def pad64(f):
        n = f.tell() % 64
        if n: f.write(b"\x00" * (64 - n))

    # per-concept centroid (in normalized space) for BlockBounds / address identity
    for nd in concept_rows:
        s, cc = nd["v_start"], nd["v_count"]
        if cc:
            xs = [(px[i] - cx) * inv for i in range(s, s + cc)]
            ys = [(py[i] - cy) * inv for i in range(s, s + cc)]
            zs = [(pz[i] - cz) * inv for i in range(s, s + cc)]
            nd["centroid"] = [sum(xs) / cc, sum(ys) / cc, sum(zs) / cc]
        else:
            nd["centroid"] = [0.0, 0.0, 0.0]

    with open(os.path.join(out_dir, "body.pos"), "wb") as f:
        for i in range(nvert):
            f.write(struct.pack("<3f", (px[i] - cx) * inv, (py[i] - cy) * inv, (pz[i] - cz) * inv))
        pad64(f)
    with open(os.path.join(out_dir, "body.nrm"), "wb") as f:
        for i in range(nvert):
            m = (nx[i] * nx[i] + ny[i] * ny[i] + nz[i] * nz[i]) ** 0.5 or 1.0
            f.write(struct.pack("<3f", nx[i] / m, ny[i] / m, nz[i] / m))
        pad64(f)
    with open(os.path.join(out_dir, "body.row"), "wb") as f:
        for i in range(nvert):
            f.write(struct.pack("<I", crow[i]))
        pad64(f)
    with open(os.path.join(out_dir, "body.idx"), "wb") as f:
        for (a, b, c) in tris:
            f.write(struct.pack("<3I", a, b, c))
        pad64(f)

    with open(os.path.join(out_dir, "body.concepts.json"), "w", encoding="utf-8") as f:
        json.dump({"attribution": ATTRIBUTION, "verts": nvert, "tris": ntri,
                   "concepts": concept_rows}, f)
    with open(os.path.join(out_dir, "body.labels.json"), "w", encoding="utf-8") as f:
        json.dump(labels, f, ensure_ascii=False)
    with open(os.path.join(out_dir, "body.materials.json"), "w", encoding="utf-8") as f:
        json.dump(MATERIALS, f)

    mat_hist = collections.Counter(MATERIALS[nd["material"]]["name"] for nd in concept_rows)
    print(f"SoA bake: {nvert:,} verts · {ntri:,} tris · {len(kept)} concepts "
          f"· {len(labels)} labels · {len(MATERIALS)} materials", file=sys.stderr)
    print(f"  part_of tree: {len(parent_pof)} edges  ({sum(1 for c in kept if c in parent_pof)} concepts placed)",
          file=sys.stderr)
    print(f"  materials: {dict(mat_hist)}", file=sys.stderr)
    print(f"  columns: body.pos/nrm/row/idx (raw LE, 64-pad) + concepts/labels/materials json",
          file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2])
