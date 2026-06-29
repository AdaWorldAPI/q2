#!/usr/bin/env python3
"""Bake the REAL-anatomy torso gaussian splat from BodyParts3D — v4: IS_A-PRIMARY
(canonical type + canonical name + the superset meshes), tissue-typed colour,
depth-peel opacity, DistinguishedName -> container:identity GUID.

WHY is_a-primary (corrected 2026-06-24 — operator: "we NEED the is_a data; do NOT
assume the canonical name is in part-of"): part-of is a REGIONAL/containment
decomposition — walking UP from a muscle hits chest-wall -> thorax, never a
"muscular system", so it cannot classify tissue, and it carries non-canonical
regional names. is_a is the TYPE decomposition: every structure resolves UP to its
canonical type (`pectoralis minor` is_a -> ... -> `muscle organ`; `descending
aorta` is_a -> ... -> `artery`), is_a ships canonical names, AND its mesh set is a
SUPERSET of part-of's (2234 vs 1258 FJ, +976) with FINER organ segmentation (no
single "aorta"/"heart" concept — split into ascending/arch/descending/abdominal,
each its own mesh). So classification, names, and geometry all come from is_a.

The is_a TYPE PATH is the DistinguishedName; it MATERIALISES to a numeric
container:identity GUID (8:8) — container = the tissue class (the deterministic,
"cesium-style" address), identity = the structure within it. Tissue resolution is
O(1) per structure (walk the is_a tree once, cache).

part-of is used ONLY to define the torso ENVELOPE bbox (the body wall delimits the
thoraco-abdominal volume); any is_a structure whose centroid lies in that envelope
is kept -> a TORSO (organs + vessels + nerves), not the whole body with head+limbs.

Colour stays FLAT PER STRUCTURE (codec anchor-prediction stays free); tissue colour
makes the palette MORE compressible (all arteries share one red). Depth-peel
opacity (muscle/wall translucent, deep organs/vessels/bone solid) lets inner
structures show through — the EWA renderer already alpha-composites.

Source: BodyParts3D 4.0 (DBCLS), FMA-keyed OBJ in one shared whole-body frame.
Inputs (scratchpad):
  isa_inclusion_relation_list.txt   is_a TYPE tree (classification + canonical names)
  isa_element_parts.txt             is_a concept -> FJ mesh (+ name)
  isa_parts_list_e.txt              is_a concept -> canonical en name
  isa_BP3D_4.0_obj_99/FJ####.obj    the is_a meshes (superset; v + vn)
  partof_inclusion_relation_list.txt + partof_element_parts.txt + partof obj
                                    -> trunk (FMA7181) envelope bbox ONLY

LICENCE / ATTRIBUTION (required): BodyParts3D, (c) The Database Center for Life
Science. CC-BY 4.0 (site) / CC-BY-SA 2.1 JP (2013 mesh files). Permissive /
commercial-safe (unlike the VOXEL-MAN / Open-3D-Man reference atlases, CC BY-NC-ND).

Outputs (cockpit/public/): torso.splat (SPL2), torso.nodes.json (+ is_a DN, tissue,
container, identity, guid), torso.manifest.json.

Usage: python3 bake_torso_splat.py <scratch_dir> <out.splat> [budget]
"""
import collections
import json
import os
import struct
import sys

ISA_ROOT = "FMA62955"           # is_a root "anatomical entity"
BUDGET_DEFAULT = 400_000
ATTRIBUTION = ("BodyParts3D, (c) The Database Center for Life Science. "
               "Current site licence: CC-BY 4.0; 2013 mesh files embed "
               "CC-BY-SA 2.1 Japan.")

# ── Tissue typing by walking the is_a TYPE tree (first keyword match up the chain
#    wins). Exact, O(1) (cached). Order = most specific first. ───────────────────
TYPEKEYS = [
    ("cartilage", ["cartilage"]),
    ("bone", ["bone organ", "bone", "skeletal element"]),
    ("muscle", ["muscle organ", "muscle"]),
    ("heart", ["cardiac", "heart", "myocardi"]),
    ("artery", ["arterial", "artery"]),
    ("vein", ["venous", "vein"]),
    ("nerve", ["nerve", "neuron", "neural", "ganglion", "spinal cord", "neuraxis"]),
    ("lung", ["respiratory", "lung", "bronch", "alveol", "trachea", "larynx"]),
    ("liver", ["hepatic", "liver"]),
    ("kidney", ["renal", "kidney", "urinary", "ureter"]),
    ("gi", ["alimentary", "gastrointestinal", "intestine", "stomach", "gastric",
            "esophag", "digestive", "bowel"]),
    ("gland", ["gland", "pancrea", "splen", "thymus", "endocrine"]),
    ("vessel", ["vascular", "vessel"]),
    ("viscus", ["viscus", "parenchymatous organ"]),
    ("skin", ["skin", "integument", "dermis"]),
]
# Fallback on the structure's OWN canonical name (for is_a concepts whose type
# chain doesn't reach a keyword).
NAMEKEYS = [
    ("cartilage", ["cartilage"]),
    ("muscle", ["muscle", "pectoralis", "oblique", "transversus", "diaphragm",
                "sphincter", "psoas", "piriformis", "obturator"]),
    ("bone", ["vertebra", "rib", "sternum", "clavicle", "scapula", "ilium",
              "ischium", "pubis", "sacrum", "manubrium", "xiphoid",
              # teeth are not is_a bone in FMA (dentin/enamel, not skeletal element), so
              # they fell through to "flesh" → layer 1 (skin), hidden in the skeleton view
              # (toothless skull). Bucket them with the skeleton; gingiva stays flesh.
              "tooth", "molar", "incisor", "canine", "premolar", "cuspid"]),
    ("artery", ["aorta", "artery", "arterial", "trunk"]),
    ("vein", ["vein", "vena", "venous"]),
    ("nerve", ["nerve", "plexus", "ganglion"]),
    ("liver", ["liver", "hepatic"]),
    ("kidney", ["kidney", "renal"]),
    ("gi", ["stomach", "intestine", "colon", "esophagus", "duoden"]),
]
SYSTEM_OF = {
    "artery": "cardiovascular", "vein": "cardiovascular", "heart": "cardiovascular",
    "vessel": "cardiovascular", "lung": "respiratory", "gi": "alimentary",
    "liver": "alimentary", "gland": "alimentary", "viscus": "viscera",
    "kidney": "urinary", "nerve": "nervous", "bone": "musculoskeletal",
    "cartilage": "musculoskeletal", "muscle": "musculoskeletal",
    "skin": "integument", "flesh": "integument",
}
TISSUE_RGB = {
    "bone": (230, 221, 196), "cartilage": (199, 209, 224), "muscle": (189, 96, 89),
    "artery": (201, 58, 52), "vein": (66, 95, 176), "nerve": (226, 205, 88),
    "heart": (168, 72, 71), "lung": (211, 152, 156), "liver": (139, 82, 76),
    "kidney": (150, 86, 80), "gi": (201, 167, 131), "gland": (206, 179, 150),
    "viscus": (188, 132, 120), "vessel": (180, 90, 110), "skin": (214, 178, 162),
    "flesh": (199, 160, 150),
}
TISSUE_OPACITY = {
    "skin": 0.14, "flesh": 0.45, "muscle": 0.55, "cartilage": 0.70,
    "bone": 0.92, "heart": 0.90, "lung": 0.82, "liver": 0.92, "kidney": 0.92,
    "gi": 0.90, "gland": 0.90, "viscus": 0.90, "artery": 0.96, "vein": 0.96,
    "vessel": 0.94, "nerve": 0.97,
}
TISSUE_CONTAINERS = ["bone", "cartilage", "muscle", "artery", "vein", "vessel",
                     "heart", "lung", "liver", "kidney", "gi", "gland", "viscus",
                     "nerve", "skin", "flesh"]
CONTAINER_ID = {t: i for i, t in enumerate(TISSUE_CONTAINERS)}


def load_isa(d):
    parent, children, name = {}, collections.defaultdict(list), {}
    with open(os.path.join(d, "isa_inclusion_relation_list.txt"), encoding="utf-8") as f:
        next(f)
        for ln in f:
            p, pn, c, cn = ln.rstrip("\n").split("\t")
            parent[c] = p
            children[p].append(c)
            name[p], name[c] = pn, cn
    elems = collections.defaultdict(list)
    with open(os.path.join(d, "isa_element_parts.txt"), encoding="utf-8") as f:
        next(f)
        for ln in f:
            c, n, fj = ln.rstrip("\n").split("\t")
            elems[c].append(fj)
            name.setdefault(c, n)
    canon = {}
    with open(os.path.join(d, "isa_parts_list_e.txt"), encoding="utf-8") as f:
        next(f)
        for ln in f:
            col = ln.rstrip("\n").split("\t")
            canon[col[0]] = col[-1]
    for v in children.values():
        v.sort()
    return parent, children, name, elems, canon


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


def read_obj_tris(path):
    """One oriented SURFEL per triangle (the other session's technique): mean =
    centroid, normal = face normal, r = mean centroid->vertex distance (so the disk
    COVERS the triangle). Returns [(cx,cy,cz, nx,ny,nz, r)]. This is what reaches
    into the field instead of leaving isolated per-vertex blobs."""
    vs = []
    out = []
    with open(path, "rb") as f:
        for ln in f:
            if ln[:2] == b"v ":
                p = ln.split(); vs.append((float(p[1]), float(p[2]), float(p[3])))
            elif ln[:2] == b"f ":
                p = ln.split()
                try:
                    ia = int(p[1].split(b"/")[0]) - 1
                    ib = int(p[2].split(b"/")[0]) - 1
                    ic = int(p[3].split(b"/")[0]) - 1
                except (ValueError, IndexError):
                    continue
                if not (0 <= ia < len(vs) and 0 <= ib < len(vs) and 0 <= ic < len(vs)):
                    continue
                a, b, c = vs[ia], vs[ib], vs[ic]
                cx = (a[0] + b[0] + c[0]) / 3
                cy = (a[1] + b[1] + c[1]) / 3
                cz = (a[2] + b[2] + c[2]) / 3
                e1 = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
                e2 = (c[0] - a[0], c[1] - a[1], c[2] - a[2])
                nx = e1[1] * e2[2] - e1[2] * e2[1]
                ny = e1[2] * e2[0] - e1[0] * e2[2]
                nz = e1[0] * e2[1] - e1[1] * e2[0]
                nl = (nx * nx + ny * ny + nz * nz) ** 0.5 or 1.0
                r = (((cx - a[0]) ** 2 + (cy - a[1]) ** 2 + (cz - a[2]) ** 2) ** 0.5
                     + ((cx - b[0]) ** 2 + (cy - b[1]) ** 2 + (cz - b[2]) ** 2) ** 0.5
                     + ((cx - c[0]) ** 2 + (cy - c[1]) ** 2 + (cz - c[2]) ** 2) ** 0.5) / 3
                out.append((cx, cy, cz, nx / nl, ny / nl, nz / nl, r))
    return out


def lowdisc_indices(n, stride):
    """Golden-ratio (R1) low-discrepancy subset of [0, n): ~n/stride indices, evenly
    spread AND aperiodic — the cheapest 'Kurvenlineal / X-Trans' decorrelation. A
    regular range(0,n,stride) picks every Nth vertex and BEATS against the mesh
    tessellation (structured aliasing) and clumps on irregular meshes; the golden
    step 1/phi never re-aligns (most-irrational continued fraction [1;1,1,...]), so
    coverage is even with no periodic beat -> a smaller brush closes the surface
    without tearing holes."""
    if stride <= 1 or n <= 1:
        return range(n)
    m = max(1, round(n / stride))
    g = 0.6180339887498949  # 1/phi
    return sorted({min(n - 1, int(((i + 1) * g % 1.0) * n)) for i in range(m)})


def knn_radii(xs, ys, zs, k=3, factor=0.55):
    """Per-surfel disk radius = `factor` * mean distance to the k nearest neighbours
    within the SAME structure (the LOCAL sample spacing). This is the metric Sigma-fit
    that REPLACES the hand-rolled sqrt(stride) bridge: dense regions get small disks,
    sparse regions larger, with NO uniform over-inflation — so opaque disks TILE the
    surface watertight (factor 0.55 -> ~10% overlap, crisp) instead of piling into
    oversized marbles. Grid-hashed, ~O(n). Same principle the substrate render path
    uses (k-NN metric Sigma); baked here so the SPL3 asset is render-ready for the
    three.js cockpit consumer, which has no Sigma-fit pass of its own."""
    n = len(xs)
    if n <= 1:
        return [0.006] * n
    x0, y0, z0 = min(xs), min(ys), min(zs)
    span = max(max(xs) - x0, max(ys) - y0, max(zs) - z0) or 1.0
    cell = max(span / (n ** 0.5), 1e-4)  # ~one surface-spacing per cell
    inv = 1.0 / cell
    grid = collections.defaultdict(list)

    def cellkey(i):
        return (int((xs[i] - x0) * inv), int((ys[i] - y0) * inv), int((zs[i] - z0) * inv))

    for i in range(n):
        grid[cellkey(i)].append(i)
    out = [cell * factor] * n
    for i in range(n):
        bx, by, bz = cellkey(i)
        ds, ring = [], 1
        while len(ds) < k + 1 and ring <= 4:
            ds = []
            for dx in range(-ring, ring + 1):
                for dy in range(-ring, ring + 1):
                    for dz in range(-ring, ring + 1):
                        for j in grid.get((bx + dx, by + dy, bz + dz), ()):
                            if j != i:
                                ds.append((xs[i] - xs[j]) ** 2 + (ys[i] - ys[j]) ** 2
                                          + (zs[i] - zs[j]) ** 2)
            ring += 1
        ds.sort()
        kk = min(k, len(ds))
        if kk:
            out[i] = factor * (sum(d ** 0.5 for d in ds[:kk]) / kk)
    return out


def tissue_of(fma, parent, name, canon, cache):
    """Walk the is_a TYPE chain to the first tissue keyword; fall back to the
    structure's own canonical name; cache per fma (O(1) amortised)."""
    if fma in cache:
        return cache[fma]
    cur, seen = fma, 0
    while cur is not None and seen < 24:
        nm = name.get(cur, "").lower()
        for t, keys in TYPEKEYS:
            if any(k in nm for k in keys):
                cache[fma] = t
                return t
        cur = parent.get(cur); seen += 1
    own = canon.get(fma, name.get(fma, "")).lower()
    for t, keys in NAMEKEYS:
        if any(k in own for k in keys):
            cache[fma] = t
            return t
    cache[fma] = "flesh"
    return "flesh"


# Generic FMA upper-ontology nodes carry no type signal — strip them so the DN
# reads as the meaningful type chain (/cardiovascular/artery/segment_of_artery/...).
DN_SKIP = {
    "anatomical entity", "physical anatomical entity", "material anatomical entity",
    "immaterial anatomical entity", "anatomical structure", "anatomical set",
    "organ", "organ part", "organ component", "organ subdivision", "body part",
    "cardinal organ part", "subdivision of cardinal organ part", "anatomical cluster",
    "nonparenchymatous organ", "parenchymatous organ", "cavitated organ",
    "organ with cavitated organ parts", "organ with organ cavity", "set",
}


def isa_dn(fma, parent, name, tissue):
    """The is_a DistinguishedName path: /system/<meaningful type ancestors>/<self>."""
    chain, cur, seen = [], fma, 0
    while cur is not None and seen < 24:
        nm = name.get(cur, cur)
        if nm.lower() not in DN_SKIP:
            chain.append(nm)
        cur = parent.get(cur); seen += 1
    body = "/".join(reversed(chain)).replace(" ", "_")
    return f"/{SYSTEM_OF.get(tissue, 'body')}/{body}"


def tissue_color(tissue, idx):
    r, g, b = TISSUE_RGB[tissue]
    j = ((idx * 0.6180339887498949) % 1.0 - 0.5) * 0.10
    f = 1.0 + j
    return (max(0, min(255, int(r * f))), max(0, min(255, int(g * f))),
            max(0, min(255, int(b * f))))


def main(scratch, out_path, budget=BUDGET_DEFAULT):
    parent, children, name, elems, canon = load_isa(scratch)
    order, depth = bfs(ISA_ROOT, children)
    have_depth = {c for c in order}

    isa_obj = os.path.join(scratch, "isa_BP3D_4.0_obj_99")
    pof_obj = os.path.join(scratch, "partof", "partof_BP3D_4.0_obj_99")

    def obj_path(fj):
        p = os.path.join(isa_obj, fj + ".obj")
        return p if os.path.exists(p) else os.path.join(pof_obj, fj + ".obj")

    # concepts that carry meshes, ordered by is_a depth (deepest-first claiming so
    # the FINEST type owns each mesh).
    concepts = [c for c in elems if c in have_depth]
    concepts_by_depth = sorted(concepts, key=lambda c: -depth[c])
    owner = {}
    for c in concepts_by_depth:
        for fj in elems[c]:
            owner.setdefault(fj, c)
    meshes_of = collections.defaultdict(list)
    for fj, c in owner.items():
        meshes_of[c].append(fj)
    for v in meshes_of.values():
        v.sort()

    # WHOLE BODY (operator 2026-06-24: "render the whole body, that's the goal;
    # make a select -> camera-zoom later"). No spatial clip — every is_a structure
    # with a mesh is baked. Region focus (torso, an organ) is a CAMERA move on the
    # full-body splat, driven O(1) by each node's centroid+bbox in the SoA, not a
    # bake-time filter.
    vcache = {}
    incl = collections.defaultdict(list)
    total_v = 0
    owners_ordered = [c for c in order if c in meshes_of]
    for c in owners_ordered:
        for fj in meshes_of[c]:
            p = obj_path(fj)
            if not os.path.exists(p):
                continue
            tris = read_obj_tris(p)  # one oriented surfel per triangle (the agreed front)
            vcache[fj] = tris
            incl[c].append(fj); total_v += len(tris)
    stride = max(1, round(total_v / budget))

    # pass 2: gather gaussians grouped by node; tissue colour + depth-peel opacity;
    # is_a DN -> materialised container:identity GUID.
    tcache = {}
    gx, gy, gz, gnx, gny, gnz, gr, gg, gb, gop, grow = ([] for _ in range(11))
    nodes = []
    ident_ctr = collections.Counter()
    row_of = {}
    kept = [c for c in order if incl.get(c)]
    for r, c in enumerate(kept):
        row_of[c] = r
    for c in kept:
        r = row_of[c]
        nm = canon.get(c, name.get(c, c))
        tissue = tissue_of(c, parent, name, canon, tcache)
        col = tissue_color(tissue, r)
        op_u8 = max(8, min(255, int(round(TISSUE_OPACITY[tissue] * 255))))
        container = CONTAINER_ID[tissue]
        identity = ident_ctr[tissue] & 0xFFFF; ident_ctr[tissue] += 1
        g_start = len(gx)
        for fj in incl[c]:
            tris = vcache[fj]
            for k in lowdisc_indices(len(tris), stride):
                (x, y, z, nx, ny, nz, _rad) = tris[k]
                gx.append(x); gy.append(y); gz.append(z)
                gnx.append(nx); gny.append(ny); gnz.append(nz)
                gr.append(col[0]); gg.append(col[1]); gb.append(col[2])
                gop.append(op_u8); grow.append(r)
        # nearest kept is_a ancestor -> parent row (the DN partonomy)
        pa, seen = parent.get(c), 0
        while pa is not None and pa not in row_of and seen < 24:
            pa = parent.get(pa); seen += 1
        nodes.append({
            "row": r, "fma": c, "name": nm, "depth": depth[c],
            "parent": row_of.get(pa),
            "tissue": tissue, "is_a": isa_dn(c, parent, name, tissue),
            "container": container, "identity": identity,
            "guid": (container << 16) | identity,
            "rgb": list(col), "opacity": round(TISSUE_OPACITY[tissue], 3),
            "g_start": g_start, "g_count": len(gx) - g_start,
            "fj": incl[c],
        })

    if not gx:
        sys.exit("no vertices gathered (check the torso envelope / obj dirs)")

    cx = (min(gx) + max(gx)) / 2; cy = (min(gy) + max(gy)) / 2; cz = (min(gz) + max(gz)) / 2
    half = max(max(gx) - min(gx), max(gy) - min(gy), max(gz) - min(gz)) / 2 or 1.0
    inv = 1.0 / half
    for i in range(len(gx)):
        gx[i] = (gx[i] - cx) * inv; gy[i] = (gy[i] - cy) * inv; gz[i] = (gz[i] - cz) * inv
    # per-structure k-NN disk sizing (metric Sigma-fit; replaces the sqrt(stride)
    # bridge). Computed on the NORMALISED coords so the radii are already in asset
    # space. Each structure sized among its own surfels — no cross-tissue contamination.
    gsc = [0.0] * len(gx)
    for nd in nodes:
        s, cnt = nd["g_start"], nd["g_count"]
        gsc[s:s + cnt] = knn_radii(gx[s:s + cnt], gy[s:s + cnt], gz[s:s + cnt])
    for nd in nodes:
        s, c = nd["g_start"], nd["g_count"]
        xs = gx[s:s + c]; ys = gy[s:s + c]; zs = gz[s:s + c]
        nd["centroid"] = [sum(xs) / c, sum(ys) / c, sum(zs) / c]
        nd["bbox"] = [[min(xs), min(ys), min(zs)], [max(xs), max(ys), max(zs)]]

    n = len(gx)
    bmin = (min(gx), min(gy), min(gz)); bmax = (max(gx), max(gy), max(gz))
    # SPL3: the header f32 is scale_max — the dequant reference for the per-gaussian
    # scale byte. Use the 99th percentile of the disk radii (NOT the max) so a few
    # giant triangles don't crush the precision of the bulk; the top 1% saturate at
    # byte 255 (slightly under-sized, harmless). Robust quantization, not a tuned
    # constant. Per-gaussian disk radius = scale_byte / 255 * scale_max.
    ssort = sorted(gsc)
    scale_max = ssort[min(len(ssort) - 1, int(round(0.99 * len(ssort))))] if ssort else 1.0
    scale_max = scale_max or 1.0

    def qi8(v):
        return max(-127, min(127, int(round(v * 127))))

    buf = bytearray()
    buf += b"SPL3"
    buf += struct.pack("<II", n, len(nodes))
    buf += struct.pack("<f", scale_max)
    buf += struct.pack("<3f", *bmin)
    buf += struct.pack("<3f", *bmax)
    for i in range(n):
        nx, ny, nz = gnx[i], gny[i], gnz[i]
        m = (nx * nx + ny * ny + nz * nz) ** 0.5 or 1.0
        sc_u8 = max(1, min(255, int(round(gsc[i] / scale_max * 255))))
        buf += struct.pack("<3f", gx[i], gy[i], gz[i])
        buf += struct.pack("<3b", qi8(nx / m), qi8(ny / m), qi8(nz / m))
        buf += struct.pack("<3B", gr[i], gg[i], gb[i])
        buf += struct.pack("<B", gop[i])
        buf += struct.pack("<B", sc_u8)
        buf += struct.pack("<H", grow[i])
    with open(out_path, "wb") as f:
        f.write(buf)

    tissue_hist = collections.Counter(nd["tissue"] for nd in nodes)
    pub = os.path.dirname(out_path)
    with open(os.path.join(pub, "torso.nodes.json"), "w", encoding="utf-8") as f:
        json.dump({"attribution": ATTRIBUTION, "decomposition": "is_a (BodyParts3D 4.0)",
                   "scale_max": scale_max, "count": n, "nodes": nodes}, f)
    manifest = {
        "source": "BodyParts3D 4.0 (DBCLS) is_a OBJ, decimated 99%, with vn normals",
        "license": "CC-BY 4.0 (site) / CC-BY-SA 2.1 JP (2013 files)",
        "attribution": ATTRIBUTION,
        "format": ("SPL3 (per-triangle surfels: pos 3f | normal 3i8 | rgb 3u8 | "
                   "opacity u8 | scale u8 | node_row u16 = 22 B; header f32 = "
                   "scale_max dequant ref); node SoA in torso.nodes.json"),
        "build": "v4 is_a-primary (canonical type+name+meshes), tissue colour + depth-peel, DN->GUID",
        "concepts": len(nodes), "meshes": len(owner), "gaussians": n, "stride": stride,
        "scale_max": scale_max, "bbox_min": list(bmin), "bbox_max": list(bmax),
        "tissues": dict(tissue_hist),
    }
    with open(os.path.join(pub, "torso.manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"baked {out_path}: {n:,} gaussians, {len(nodes)} is_a structures, stride {stride}",
          file=sys.stderr)
    print(f"  tissues: {dict(tissue_hist)}", file=sys.stderr)
    print(f"  scale_max {scale_max:.5f} (99th pct disk radius); SPL3 {len(buf):,} B "
          f"+ torso.nodes.json + manifest", file=sys.stderr)


if __name__ == "__main__":
    a = sys.argv
    main(a[1], a[2], int(a[3]) if len(a) > 3 else BUDGET_DEFAULT)
