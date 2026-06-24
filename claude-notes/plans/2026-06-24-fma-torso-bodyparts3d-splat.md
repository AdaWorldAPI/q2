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

- [x] `tools/bake_torso_splat.py` — BodyParts3D → SPL1 gaussian asset + manifest
      (231K gaussians, 577 meshes, 102 structures; muted pastel per-structure hues)
- [x] `cockpit/public/torso.splat` (3.7 MB) + `torso.manifest.json` (attribution/legend)
- [x] SPL1 TS decoder + `/torso-live` three.js orbit page + route
- [x] `/torso` splat3d CPU render: scratchpad `torso-render` driver reads SPL1 →
      `Gaussian3D` → ndarray::hpc::splat3d turntable (no Inria .ply needed) →
      20 JPEG frames in `cockpit/public/torso-frames/` → `/torso` viewer page + route
- [x] attribution surfaced in-UI; tsc clean
- [ ] PR

Notes:
- The CPU render runs under ndarray's own 1.95 toolchain (scratchpad project,
  path-dep on ../ndarray) — q2's workspace stays free of the ndarray dep.
  ~6.6 s/frame on the scalar path (no AVX target-cpu in the scratchpad project);
  correctness verified by viewing the rendered frames.
- Colours: golden-angle hue per structure at S=0.34 V=0.78 (muted, per request).
