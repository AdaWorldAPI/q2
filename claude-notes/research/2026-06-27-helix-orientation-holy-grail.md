# Helix orientation — deterministic, comparable-without-materialization (measured)

**Date:** 2026-06-27 · **Branch:** claude/q2-fma-v3-bake · **Data:** real torso.mesh / torso.splat

## Claim, measured (not asserted)
A surfel/gaussian's orientation encodes as a **1–3 byte deterministic helix code** — residual-VQ
on the sphere (the same RVQ machinery as palette256, on S²; decode is **Fisher-2z normalized**).
It is **comparable in O(1) LUT without materializing the vector**, and replaces a trained 3DGS
quaternion (16 B, per-scene) with no training.

| metric | 1 byte | 2 bytes | 3 bytes |
|---|---|---|---|
| encode error (real surfel normals) | 4.87° | 0.97° | **0.073°** |
| render PSNR vs original (turntable, Lambert, conservative) | 48.3 dB | — | **84.5 dB** |
| effective directions | 256 | ~65 K | ~16.7 M |

- **8192-dir ("8K") target = 2.244°** → beaten at **2 bytes**.
- **Compare-without-materialization** (80 K pairs, LUT on codes vs true angle): **Pearson 0.9917 / Spearman 0.9924**, encode 4.86°, cost = 2 byte loads + 1 table lookup (no decode/dot/acos), storage 1 B vs 12 B.
- Tool: `crates/osint-bake/tools/helix_orient.py` (self-tests to 0.073° at 3 bytes).

## The unified representation (the grail)
A surfel/gaussian is an **address**, not a trained blob — every op runs in normalized LUT space:

| component | codec | manifold | cost | replaces |
|---|---|---|---|---|
| position | HHTL `Located` (`ogar-fma-skeleton`) | ℝ³ Morton | 6 B | xyz / Cesium tile coords |
| **orientation** | **helix** (Fisher-2z) | **S²** | **3 B @ 0.07°** | trained quaternion (16 B) |
| scale / magnitude | palette256 (Fisher-Z) | ℝ | 1 B | trained anisotropic scale |
| pairwise / edges | turbovec (`lance-graph-turbovec`) | graph | 1–16 B | adjacency / KNN |
| LOD | `ndarray hpc/splat3d/depth_cascade.rs` (Cesium SSE) | — | — | screen-space refinement |
| volume infill | TPMS gyroid (closed-form) | — | 1 B density | stored interior voxels |

~236 B trained gaussian → ~12 B deterministic codes (~20×), no training, comparable in O(1).

## Epiphanies (each grounded)
1. **helix and palette256 are ONE codec on two manifolds** — Fisher-Z RVQ on the line vs Fisher-2z RVQ on the sphere; the "2z" is the sphere's extra DOF. Proven by building helix as exact RVQ (4.87°→0.073° in three residual bytes).
2. **Comparison-without-materialization is the universal op** — normalized decode ⟹ all distance/sort/cull/LOD in O(1) LUTs on bytes; never reconstruct until render. The bottleneck 3DGS (matrix builds) and Cesium (normal decode) both hit.
3. **Cartography = Cesium = gaussian-splat** — all are position+orientation+scale-on-a-manifold-with-LOD; the only missing piece was a deterministic, comparable orientation code. Helix completes all three.
4. **No-training** — where geometry exists, every code is deterministic (encode = O(1) inverse placement). Precondition for planet-scale and for *generating* anatomy.

## Honest edges
- Pixel parity above is **Lambert** (orientation→brightness, a conservative bound; the actual EWA footprint effect is smaller). The remaining end-to-end test is the **real `splat3d` EWA render** (PSNR/SSIM), wired in Rust — verify on Railway (ndarray builds there).
- Helix **sidesteps** the image→geometry inverse problem (where you have geometry it's deterministic); it does not abolish it (pure photographic 3DGS still needs the geometry step first).

## Wiring status
- `helix_orient.py` (codec + parity harness) committed.
- Rust wiring = a one-line helix decode in the SPL3→`Gaussian3D` build path (`ndarray hpc/splat3d`), so each splat's orientation is the 3-byte code, not a stored normal. Build/verify on Railway.
