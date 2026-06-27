# /body — server-side HHTL LOD + helix + slicer-fill (option 2)

## Overview

`/body` must render the full FMA body as **filled polygons** (slicer-style infill,
per material — tubes/vessels/solids), addressed on the **V3 substrate**
(`classid 0x1000_0A01`, the `(part_of:is_a)` 8:8 cascade), with **LOD** driven by
the HHTL depth-cascade. The earlier `/body` was wrong on every axis: raw-OBJ hollow
shells, no fill, no LOD, no helix, V3 key mis-encoded as `(depth:is_a)`, renderer
ignoring `classid`.

**Decision (operator, 2026-06-27): option 2 — compute server-side.** The HHTL LOD
(`depth_cascade`), helix-3-byte, and slicer-fill run in `cockpit-server` (x86, full
`F32x16` SIMD), streaming LOD-selected geometry to a thin three.js viewer. Rationale:
ndarray's **wasm** SIMD backend (`simd_wasm.rs`) is an un-wired stub — `F32x16`
falls back to scalar on wasm, ~16× too slow for per-frame client-side LOD. Native is
fully polyfilled (AVX-512/AVX2/NEON), so the cascade belongs server-side. (Option 1 —
complete the wasm `F32x16` v128 backend — is the alternative, deferred.)

`splat3d` is a gaussian raster (the rejected "confetti" path); we use ONLY its
renderer-agnostic `depth_cascade` (LOD block-preselection) + `helix_orient`, and draw
**polygons**, never gaussians.

## Foundation — DONE (verified)

`scratch-fma/lodprobe` (standalone, builds against ndarray `features=["std","splat3d"]`):
body.spm1 → per-concept `BlockBounds{center,radius}` → `cascade_blocks(camera, …)`.
Verified monotonic LOD: near ⇒ 1513/1658 `ProjectExact`; far ⇒ 1446/1658 `KeepCoarse`.
API pinned: `depth_cascade::{BlockBounds, DepthCascadeBudget, HhtlAction(Reject/
KeepCoarse/Refine/ProjectExact/RenderExact), cascade_blocks}`, `project::Camera`.

## Work items

### Phase A — V3 substrate correctness (independent of LOD)
- [ ] Bake the cascade as **6×(8:8) `(part_of:is_a)`** tiles: walk BOTH
  `partof_inclusion_relation_list.txt` AND `isa_inclusion_relation_list.txt`; each
  tier byte-pair = `(part_of_rank << 8) | is_a_rank`; identity tier too. (Current bake
  packs `(depth:is_a)` and never walked part_of — wrong.)
- [ ] `body.rs`: emit the 6 tiers directly from the (part_of,is_a) pair arrays; drop
  the `mixin_for_depth` hack.
- [ ] Renderer: dispatch on `classid` — assert `0x1000_xxxx` (V3), decode the
  `(part_of:is_a)` tile per node, use it (group/colour/pick by the two axes).

### Phase B — multi-LOD geometry (the pyramid the cascade selects from)
- [ ] Per concept, bake a decimation pyramid: L0 full-res (ProjectExact), L1/L2
  vertex-cluster-decimated (KeepCoarse). Store offsets per (concept, level).
- [ ] BlockBounds table (centroid + radius) per concept, baked alongside.

### Phase C — slicer-fill + helix (the "3D printing slicer" infill)
- [ ] Per solid material (tube/vessel/organ), generate infill geometry inside the
  shell (slicer-style), placed via HHTL tile coords + `helix_orient` 3-byte → exact
  location. Tubes get tubular infill; solids get volumetric.
- [ ] Material-prototype texture per layer (tube/vessel/bone/…).

### Phase D — server endpoint + streaming viewer
- [ ] `cockpit-server`: dep ndarray `features=["std","splat3d"]`; `/api/body/lod`
  (POST camera {view,fx,fy,w,h}) → `cascade_blocks` → assemble selected (concept,LOD)
  blocks → SPM1 stream.
- [ ] `BodyV3.tsx`: thin — throttled orbit posts the camera; swap the streamed mesh.
  Drop the full 168 MB client fetch.

## Constraints
- Big baked assets (LOD pyramid, fill) → GitHub Releases (q2 `fma-body-soa-v3-*`),
  never git. `cockpit/public/body.soa*` gitignored.
- q2 workspace cargo can't build in-sandbox (proxy-blocked `runtimed` git dep);
  ndarray-only crates verify standalone; the server build runs on deploy.
- No model identifier in any committed artifact.
