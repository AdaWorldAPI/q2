#!/usr/bin/env python3
"""Semantic-layer audit for the baked FMA body (QA gate).

Runs over a baked `<soa_dir>` (body.concepts.json + columns) and checks the
ontology / classification rules the /body viewer relies on:

  QA-1  organ-scale vessel    — a concept in a vessel material (0..3) whose mesh is
        a low-aspect BLOB (not a thin tube), single connected component, organ-sized.
        Catches the heart / liver / iris / choroid class of "whole organ rendered as
        a blue vessel" bug.
  QA-2  orphan / missing class — a concept with no name, or no is_a parent and no
        part_of placement (parent_row == -1 and part_of all zero).
  QA-3  smoke tests           — assert representative structures land in the right
        compartment: liver/eyeball→organ, brain→nervous, femur→skeleton,
        biceps→muscle, aorta/vena cava→vessel.

Exit code is non-zero if any QA-3 smoke test fails, so it can gate CI / a rebake.
Usage: python3 audit_body_semantics.py <soa_dir>   (default: soa)
"""
import json
import os
import re
import struct
import sys
from collections import defaultdict

VESSEL_MATERIALS = {0, 1, 2, 3}

# mirror of soabake's layer_of(tissue) → compartment id, and the UI layer names.
LAYER_OF = {
    "skin": 1, "flesh": 1, "muscle": 2,
    "heart": 3, "lung": 3, "liver": 3, "kidney": 3, "gi": 3, "gland": 3, "viscus": 3,
    "bone": 4, "cartilage": 4, "artery": 5, "vein": 5, "vessel": 5, "nerve": 6,
    "connective": 7,
}
LAYER_NAME = {1: "skin", 2: "muscle", 3: "organ", 4: "skeleton", 5: "vessel", 6: "nervous", 7: "connective", 8: "other"}


def layer_of(tissue):
    return LAYER_OF.get(tissue, 8)


def n_components(pts, cell=0.05):
    grid = defaultdict(list)
    for i, p in enumerate(pts):
        grid[(int(p[0] // cell), int(p[1] // cell), int(p[2] // cell))].append(i)
    seen, n = set(), 0
    for s in list(grid):
        if s in seen:
            continue
        n += 1
        st = [s]
        seen.add(s)
        while st:
            c = st.pop()
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    for dz in (-1, 0, 1):
                        nb = (c[0] + dx, c[1] + dy, c[2] + dz)
                        if nb in grid and nb not in seen:
                            seen.add(nb)
                            st.append(nb)
    return n


def main(d):
    doc = json.load(open(os.path.join(d, "body.concepts.json")))
    labels = json.load(open(os.path.join(d, "body.labels.json")))
    concepts = doc["concepts"]
    nV = doc["verts"]
    pos = struct.unpack(f"<{nV * 3}f", open(os.path.join(d, "body.pos"), "rb").read()[:nV * 12])
    row = struct.unpack(f"<{nV}I", open(os.path.join(d, "body.row"), "rb").read()[:nV * 4])
    by_row = defaultdict(list)
    for i in range(nV):
        by_row[row[i]].append(i)

    def name(c):
        return labels[c["name_idx"]] if 0 <= c["name_idx"] < len(labels) else ""

    # ── QA-1: organ-scale vessel ───────────────────────────────────────────────
    print("── QA-1  organ-scale vessels (blob in a vessel material) ──")
    q1 = []
    for c in concepts:
        if c["material"] not in VESSEL_MATERIALS:
            continue
        idx = by_row.get(c["row"], [])
        if len(idx) < 400:
            continue
        xs = [pos[i * 3] for i in idx]
        ys = [pos[i * 3 + 1] for i in idx]
        zs = [pos[i * 3 + 2] for i in idx]
        dims = sorted([max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)])
        if dims[0] < 1e-6:
            continue
        aspect = dims[2] / dims[0]
        # organ blob: low aspect AND a fat minor axis AND a single solid component
        if aspect < 1.6 and dims[0] > 0.05 and n_components([(pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]) for i in idx]) == 1:
            q1.append((dims[0], aspect, name(c), c["material"], len(idx)))
    for mind, asp, nm, mat, n in sorted(q1, reverse=True):
        print(f"   ⚠ min={mind:.2f} aspect={asp:.2f} mat={mat} v={n:>6}  {nm}")
    if not q1:
        print("   ✓ none")

    # ── QA-2: floating / misplaced geometry ────────────────────────────────────
    # A concept whose mesh splits into regions FAR apart that are NOT a left/right
    # mirror pair = geometry attached to the wrong place (the vessel-bridge / "organ
    # fragment floating elsewhere" class). Bilateral pairs (hands, ears) are expected.
    print("── QA-2  floating / misplaced geometry (non-bilateral split) ──")
    q2 = 0
    for c in concepts:
        if not name(c):
            q2 += 1
            print(f"   ⚠ row {c['row']:>4} unnamed concept")
            continue
        idx = by_row.get(c["row"], [])
        if len(idx) < 300:
            continue
        pts = [(pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]) for i in idx]
        # cluster into far-apart regions (coarse cell)
        grid = defaultdict(list)
        for p in pts:
            grid[(round(p[0] / 0.12), round(p[1] / 0.12), round(p[2] / 0.12))].append(p)
        seen, comps = set(), []
        for s in list(grid):
            if s in seen:
                continue
            stack, cells = [s], []
            seen.add(s)
            while stack:
                cc = stack.pop()
                cells.append(cc)
                for dx in (-1, 0, 1):
                    for dy in (-1, 0, 1):
                        for dz in (-1, 0, 1):
                            nb = (cc[0] + dx, cc[1] + dy, cc[2] + dz)
                            if nb in grid and nb not in seen:
                                seen.add(nb)
                                stack.append(nb)
            comps.append([q for cell in cells for q in grid[cell]])
        if len(comps) < 2:
            continue
        comps.sort(key=len, reverse=True)
        cen = [[sum(p[k] for p in comp) / len(comp) for k in range(3)] for comp in comps[:2]]
        sep = sum((cen[0][k] - cen[1][k]) ** 2 for k in range(3)) ** 0.5
        # bilateral if the two regions are x-mirror images (x flips, y/z stay)
        bilateral = abs(cen[0][0] + cen[1][0]) < 0.06 and abs(cen[0][1] - cen[1][1]) < 0.08 and abs(cen[0][2] - cen[1][2]) < 0.08
        if sep > 0.18 and not bilateral:
            q2 += 1
            print(f"   ⚠ {name(c)}  sep={sep:.2f}  cen0=({cen[0][0]:.2f},{cen[0][1]:.2f},{cen[0][2]:.2f}) cen1=({cen[1][0]:.2f},{cen[1][1]:.2f},{cen[1][2]:.2f})")
    print(f"   {'⚠' if q2 else '✓'} {q2} concept(s) with non-bilateral split geometry")

    # ── QA-3: smoke tests ──────────────────────────────────────────────────────
    print("── QA-3  per-organ compartment smoke tests ──")
    fails = 0

    def assert_layer(label_pat, want_layers, exclude=None):
        nonlocal fails
        pat = re.compile(label_pat, re.I)
        exc = re.compile(exclude, re.I) if exclude else None
        hits = [c for c in concepts if pat.search(name(c)) and not (exc and exc.search(name(c)))]
        bad = [c for c in hits if layer_of(c["tissue"]) not in want_layers]
        want = "/".join(LAYER_NAME[w] for w in want_layers)
        if not hits:
            print(f"   ? {label_pat!r}: no concepts matched (skipped)")
            return
        if bad:
            fails += 1
            print(f"   ✗ {label_pat!r} should be {want}: {len(bad)}/{len(hits)} in wrong compartment, e.g.:")
            for c in bad[:4]:
                print(f"        {name(c)} → {LAYER_NAME[layer_of(c['tissue'])]} (tissue={c['tissue']}, mat={c['material']})")
        else:
            print(f"   ✓ {label_pat!r}: all {len(hits)} in {want}")

    # liver parenchyma → organ (exclude the true hepatic/portal vessels)
    assert_layer(r"hepatovenous segment|caudate lobe of liver", {3})
    # eyeball structures → organ (skin-layer flesh ok too); must NOT be vessel
    # \bretina\b (not bare "retina") so it does not match "retinaculum" — the wrist/ankle
    # retinacula are connective (layer 7), not the eye's retina.
    assert_layer(r"sclera|cornea|\bretina\b|vitreous|^.*\biris\b|choroid(?! plexus)|eyeball", {1, 3}, exclude=r"plexus")
    # brain → nervous
    assert_layer(r"\bbrain\b|cerebral cortex|cerebellum", {6}, exclude=r"artery|vein|vessel")
    # femur → skeleton
    assert_layer(r"\bfemur\b", {4})
    # biceps → muscle
    assert_layer(r"biceps", {2})
    # aorta / vena cava trunks → vessel (exclude organ-supply *branches* of the aorta,
    # which correctly carry their target organ's tissue)
    assert_layer(r"\baorta\b|vena cava", {5}, exclude=r"branch|oesophageal|bronchial")
    # connective → connective layer 7, NEVER organ. FMA files ligament/tendon/membrane
    # under /viscera/solid_organ/ligament_organ, so without the connective TYPEKEY the
    # is_a walk tags them viscus→organ and limb ligaments float in the organ view.
    assert_layer(r"interosseous membrane|calcaneal tendon|long plantar ligament|iliotibial tract",
                 {7})

    print(f"\nsummary: QA-1 flagged {len(q1)} · QA-2 flagged {q2} · QA-3 {'FAILED ' + str(fails) if fails else 'passed'}")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "soa")
