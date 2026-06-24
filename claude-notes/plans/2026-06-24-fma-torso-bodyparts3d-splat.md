# FMA torso gaussian splat — real BodyParts3D geometry, two cockpit pages

> 2026-06-24 · status: in progress
> The `/fma` heart slice (PR #51) renders the FMA partonomy with *synthesized*
> 8:8-HHTL layout because FMA itself has **zero geometry**. This adds a **torso**
> rendered from **real anatomical meshes** — BodyParts3D, which keys 3D meshes
> directly on FMA concept IDs — across two new cockpit pages.

## The convergence

- **FMA partonomy** (`part_of`) = mereotopological containment in BodyParts3D
  (NAR 2009, Table 1: `A part_of B` ⟺ `A° ⊂ B°`, coveredBy). So the HHTL
  `[container:identity]` cascade *is* the spatial nesting.
- **BodyParts3D** (DBCLS, CC-BY 4.0) realizes FMA concepts as OBJ meshes in one
  shared whole-body frame. `concept id` column **is** the FMA id.
- Z-Anatomy / the Unity app are curated atlases on the same data; we use the raw
  FMA-keyed OBJ (no Blender needed).

## Data (measured)

- Root `FMA7181 trunk` → **178 concepts, 577 OBJ meshes, ~694K verts**, all present.
  Regions: thoracic segment (548 meshes), body wall (81), abdominal (24), perineum (4).
  The heart (PR #51) nests inside (`trunk → … → content of middle mediastinum → heart`).
- Source (external, NOT committed): `dbarchive.biosciencedbc.jp/.../partof_BP3D_4.0_obj_99.zip`
  + `partof_inclusion_relation_list.txt` / `partof_element_parts.txt` / `partof_parts_list_e.txt`.
- License: **CC-BY 4.0** — attribution: "BodyParts3D, © The Database Center for
  Life Science licensed under CC Attribution 4.0 International".

## Pipeline

```
BodyParts3D (FMA partof tree + FJ OBJ meshes)
  → tools/bake_torso_splat.py  (BFS FMA7181 → concepts → FJ meshes → vertices,
                                recenter/normalize, region-colour, downsample)
  → cockpit/public/torso.splat   (SPL1 binary: real positions + rgb + opacity)
  → /torso-live  three.js orbit  (reads SPL1; the "live orbit")
  → torso.ply (Inria) → ndarray splat3d_flex → frames → /torso  (the "splat", CPU)
```

`splat3d` (ndarray, CPU-SIMD, no GPU) renders bake-side via its own 1.95 toolchain
(q2 stays clean of the ndarray dep). Both pages read ONE asset → identical geometry.

## Checklist

- [ ] `tools/bake_torso_splat.py` — BodyParts3D → SPL1 gaussian asset + manifest
- [ ] `cockpit/public/torso.splat` (+ `torso.manifest.json` attribution/legend)
- [ ] SPL1 TS decoder + `/torso-live` three.js orbit page + route
- [ ] torso.ply + splat3d_flex render → `/torso` frames page + route
- [ ] attribution surfaced in-UI; verify tsc; commit; PR
