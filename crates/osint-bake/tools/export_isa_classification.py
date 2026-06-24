#!/usr/bin/env python3
"""Export the FULL is_a tissue classification — every BodyParts3D is_a concept that
carries a mesh, mapped to {tissue, system, canonical name, is_a DistinguishedName,
container:identity GUID, rgb, FJ meshes}.

CONVERGENCE ARTIFACT (2026-06-24, "opaque now, gaussian later"): the certified-gaussian
render session has the is_a MESHES (FJ####.obj) but not the is_a MAPPING (it has
part_of's inclusion tree, not is_a's). Join this table by FJ id to classify + colour +
GUID-address every mesh in that pipeline. It reuses bake_torso_splat's is_a tree walk
(`load_isa` + `tissue_of` + `isa_dn`) so the two renders share ONE classifier — no drift.

Deepest-first claiming mirrors the bake: the FINEST is_a type owns each shared mesh.

Source: BodyParts3D 4.0 (DBCLS). CC-BY 4.0 / CC-BY-SA 2.1 JP.

Usage: python3 export_isa_classification.py <scratch_dir> <out.json>
"""
import collections
import json
import sys

from bake_torso_splat import (
    ATTRIBUTION, CONTAINER_ID, ISA_ROOT, SYSTEM_OF,
    bfs, isa_dn, load_isa, tissue_color, tissue_of,
)


def main(scratch, out_path):
    parent, children, name, elems, canon = load_isa(scratch)
    order, depth = bfs(ISA_ROOT, children)
    have = set(order)

    # deepest-first claim so the finest is_a type owns each shared mesh (mirrors the bake)
    concepts = sorted((c for c in elems if c in have), key=lambda c: -depth[c])
    owner = {}
    for c in concepts:
        for fj in elems[c]:
            owner.setdefault(fj, c)
    meshes_of = collections.defaultdict(list)
    for fj, c in owner.items():
        meshes_of[c].append(fj)

    tcache = {}
    ident = collections.Counter()
    rows = []
    for c in sorted(meshes_of, key=lambda c: (depth[c], c)):
        tissue = tissue_of(c, parent, name, canon, tcache)
        container = CONTAINER_ID[tissue]
        identity = ident[tissue] & 0xFFFF
        ident[tissue] += 1
        rows.append({
            "fma": c,
            "name": canon.get(c, name.get(c, c)),
            "tissue": tissue,
            "system": SYSTEM_OF.get(tissue, "body"),
            "is_a": isa_dn(c, parent, name, tissue),
            "container": container,
            "identity": identity,
            "guid": (container << 16) | identity,
            "rgb": list(tissue_color(tissue, len(rows))),
            "fj": sorted(meshes_of[c]),
        })

    nmesh = sum(len(r["fj"]) for r in rows)
    json.dump({
        "attribution": ATTRIBUTION,
        "source": "BodyParts3D 4.0 (DBCLS) is_a TYPE tree",
        "note": "join by FJ id; container:identity is the 8:8 GUID, is_a is the DN path",
        "concepts": len(rows), "meshes": nmesh,
        "classification": rows,
    }, open(out_path, "w", encoding="utf-8"))

    hist = collections.Counter(r["tissue"] for r in rows)
    print(f"{out_path}: {len(rows)} is_a concepts, {nmesh} meshes", file=sys.stderr)
    print(f"  tissues: {dict(hist)}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
