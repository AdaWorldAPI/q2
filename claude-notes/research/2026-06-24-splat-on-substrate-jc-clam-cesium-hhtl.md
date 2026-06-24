# Splat render on substrate — jc / CLAM / cesium / HHTL (2026-06-24)

> **Knowledge-transfer capture, written before it dilutes.** The q2 cockpit
> anatomy gaussian-splat render must **not hand-roll** sampling, footprint,
> anti-alias, or LOD. Every piece already exists — certified, closed-form,
> Jirak/jc-bounded — in the **ndarray + lance-graph substrate**. This doc maps
> the q2 render pipeline to the existing primitives **by exact path**, so the
> next session *cites* instead of re-deriving. Operator-directed: "implement all
> of the above and document before it dilutes citing the already existing."

## CORRECTION (2026-06-24, cross-session full-read — read THIS before the rest)

A parallel session's Opus readers read the `perturbation-sim` knowledge transfer
**fully** while this doc's first draft **synthesized**. They caught real
over-claims; the honest status:

1. **The DOCUMENTED seam is cesium-wholesale, NOT hand-build-ClamTree.** The path
   the docs prescribe: `Gaussian3D → SplatBatch → cesium-3dtiles-writer → cesium
   SSE / HLOD / implicit-tiling → splat-render`
   (`lance-graph/.claude/plans/cesium-osm-substrate-v1.md` D-OSM-5/6;
   `3DGS-3D-Tiles-runtime-plan.md`). The "build a ClamTree, size each gaussian from
   its leaf radius, hand to splat3d" path in the table below is a **SYNTHESIS**
   (origin: the operator's "zwei Schenkel" framing +
   `lance-graph/crates/bgz17/src/clam_bridge.rs:233-238` δ⁺/δ⁻ + `ndarray::hpc::clam`)
   — **not** in this knowledge transfer. The docs' only ClamTree linkage is an
   explicitly **[H], NOT-wired** CHAODA *anomaly-scoring* map over contingency
   vectors — not a geometry tree whose leaf radius sizes a render gaussian. Treat
   CLAM-reach-sizing as a CANDIDATE, not doc-prescribed.

2. **HHTL is 3-tier in perturbation-sim** (TWIG = buses/leaf, HIP = basins
   Kron/Schur-reduced, HEEL = cross-border super-graph; `METHODS.md:88`). The
   4-tier HEEL/HIP/TWIG/**LEAF** is the *datalake-traversal* instantiation
   (data-block stages) — a different instance of "HHTL".

3. **electrical-Morton = Morton over the resistance-distance SPECTRAL embedding,
   never geography** (`METHODS.md:57`). That warning bites when you tile a
   *semantic* metric (e.g. FMA system-distance) but cheat with geometry. For the
   body's literal GEOMETRY, **geometric Morton is correct** — 3D Tiles does exactly
   this; the geometry *is* the metric. The [S]-rhyme would be claiming a geometric
   tile captures FMA *semantics*.

4. **The CODE seam (does `cesium` ingest raw surfels or pre-built gaussians; does
   `splat3d::Gaussian3D` take a full Σ or only scale+quat; does any existing
   driver/test already wire surfels→cesium) is being settled by the other
   session's Reader B.** DO NOT wire until that authoritative answer lands. The
   table below is the candidate map, not the confirmed seam.

**What both sessions agree survives:** the *front of the pipe* — BodyParts3D →
deterministic `part_of`/is_a system classification (colour) → per-triangle surfels
(`read_obj_tris`). The reinvention to delete is everything bolted on *after* it
(hand-rolled octree / fixed tangent / `lowdisc_indices` / `0.93`/`theta` /
CesiumJS-in-browser).

## RESOLUTION (2026-06-24 PM — what q2 actually shipped: "opaque now, gaussian later")

Three operator corrections landed in sequence and settled the q2 render:

1. **"severe chubby halo effect"** → the disks were over-sized.
2. **"rather a triangle without gaussian at all than a ghostbuster pretending to do
   gaussian splat"** → the soft-Gaussian look itself was rejected.
3. The other session's jc report confirmed the sizing fix (**k-NN metric Σ**, not
   uniform stride).

**The single durable finding: the ghost was the RENDERER, not the asset.**
`splat3d` composites soft-Gaussian alpha *tails* through the low-opacity translucent
surfels (skin 0.14 / flesh 0.45 depth-peel) → halo + fog. Swapping to an **opaque
z-buffer** on the *same bytes* — nearest surface wins, hard-edged disk, no alpha
accumulation — is crisp by construction. No re-bake was needed to kill the fog; the
fix was the compositing mode. (Pfister/Zwicker surfels, not a gaussian pretending.)

**The marble fix (second, independent): sizing does NOT belong in the bake as a
hand-rolled `sqrt(stride)` bridge.** That uniform 4× over-inflation piled disks into
marbles. Replaced by **per-structure k-NN local spacing** (≈0.55× the 3-nearest
distance) — the same metric-Σ principle the substrate render path uses, computed in
the bake so the SPL3 asset is render-ready for the three.js cockpit (which has no
Σ-fit pass of its own). `scale_max` fell 0.037→0.0086 (4.3× smaller); disks tile.

**SPL3 wire** (supersedes SPL2): body 22 B = pos 3f | normal 3i8 | rgb 3u8 | opacity
u8 | **scale u8** | node_row u16; header f32 = `scale_max` (per-surfel disk-radius
dequant ref: `scale_byte/255 * scale_max`). The opacity byte is a tissue-depth tag
the renderer uses to GATE the skin/flesh shell out (`uFloor`), not to fog.

**The split (operator-chosen):**
- **Opaque now** — q2 owns it. All three cockpit views (`/torso`, `/torso-live`,
  `/torso-map`) render opaque k-NN-sized surfels: crisp, ~40 ms/frame pure-Rust
  preview, trivial three.js (opaque depth-tested points), no gaussian shader.
- **Gaussian later** — the other session owns it. Their certified path (jc::weyl
  φ⁻¹ decimation + jc::ewa_sandwich_3d SPD cert on real surfels = 100 % valid,
  min eigenvalue 9.88e-3 px² + Cesium SSE/HLOD `depth_cascade`) is the high-fidelity
  offline render. q2 feeds it the **is_a classification it lacks** —
  `crates/osint-bake/tools/export_isa_classification.py` →
  `fma_isa_classification.json` (1658 concepts / 2234 meshes, FJ→tissue/system/GUID/DN),
  reusing the same `tissue_of` walk so the two renders share one classifier.

The convergence boundary held: **q2 = the is_a data + opaque preview; the other
session = the certified gaussian render.** Neither reimplements the other's half.

## The anti-pattern this deletes (what was hand-rolled, wrongly)

The scratchpad driver + `bake_torso_splat.py` reached the **"VR-paint iceberg"**
failure — per-vertex sampling at a **fixed tangent**, so each splat is an
isolated blob that doesn't reach its neighbours into the field (1–4 "hops"
instead of 5–12). Three hand-rolled guesses caused it, each superseded:

| Hand-rolled (DELETE) | Why it's wrong | Substrate replacement |
|---|---|---|
| per-vertex + **fixed tangent** (`scale=[t,t,thin]`) | footprint not matched to local spacing → blobs/gaps | per-triangle surfel + **Σ fit to the neighbourhood** (`splat_neighborhood`) |
| `lowdisc_indices` (hand golden-ratio φ) in the bake | reinvents Weyl equidistribution | **`jc::weyl`** |
| `0.93` scale / `1.45–1.6` theta (trial-and-error) | iterate-until-it-looks-right | **cesium SSE** + **`jc::ewa_sandwich`** (closed-form, Jirak-bounded) |

## The candidate pipeline (SYNTHESIS — see CORRECTION above; cesium-wholesale is the doc-prescribed seam)

```
per-triangle surfel ─→ CLAM reach ─→ jc::weyl sampling ─→ splat_neighborhood Σ-fit
                    ─→ ewa_coarsen (Morton-seam anti-alias) ─→ cesium SSE/HLOD (HHTL LOD)
                    ─→ jc::ewa_sandwich (certify J·Σ·Jᵀ SPD) ─→ render
```

| Stage | Primitive (what it does) | **Exact path** |
|---|---|---|
| per-triangle surfel | mean = centroid, normal = face normal — the correct *front* of the pipe | `q2: crates/osint-bake/tools/bake_torso_splat.py::read_obj_tris` |
| **metric reach** ("zwei Schenkel eines Dreiecks ergeben das dritte") | cover-tree radius via the δ⁺/δ⁻ triangle inequality (`δ⁺ = dist+radius`, `δ⁻ = max(0, dist−radius)`); `mean_leaf_radius` = local reach | `ndarray/src/hpc/clam.rs` (`Cluster::delta_plus` / `delta_minus`, `ClamTree::build_with_fn`, `mean_leaf_radius`) |
| **low-discrepancy sampling** | Weyl equidistribution (golden-ratio), the *certified* form of φ/R2 | `ndarray/crates/jc` → `jc::weyl` |
| **adaptive anisotropic footprint** | fit an SPD `Σ` to the local neighbourhood (metric-closeness-weighted covariance of neighbour offsets); `anisotropy = λ_max/λ_min` | `lance-graph/crates/perturbation-sim/src/splat.rs::splat_neighborhood` (2-D electrical ref); 3-D form in `ndarray/src/hpc/splat3d` (`Spd3::from_scale_quat`) |
| **Morton-seam anti-alias** (the Fujifilm-X / "irrational quorum" effect) | EWA pyramid coarsen down-weights spatially-distant (Morton-seam) cells, removing the seam aliasing a hard box-average introduces (test: box=34 → EWA=1) | `lance-graph/crates/perturbation-sim/src/splat.rs::ewa_coarsen` + `morton2` |
| **view-LOD** (HHTL screen-space error) | closed-form pixel error `sse_for_tile(geometric_error, distance, denom)`; `tile_meets_sse` decides refine/skip | `ndarray/crates/cesium/src/sse.rs`, `hlod.rs`, `implicit_tiling.rs` |
| **covariance certificate** | `J·Σ·Jᵀ` stays SPD across cascade hops (`prove_pillar_7`) | `ndarray/crates/jc` → `jc::ewa_sandwich`; `ndarray/src/hpc/pillar/ewa_sandwich_3d.rs` |
| **significance bound** | weak-dependence rate `n^(p/2−1)`, **NOT** classical IID Berry-Esseen | `I-NOISE-FLOOR-JIRAK` (lance-graph CLAUDE.md); Jirak 2016 (arXiv 1606.01617) |
| **HHTL cascade tiers** | HEEL/HIP/TWIG/LEAF certified traversal; tile↔data-block mapping | `lance-graph/.claude/plans/3DGS-HHTL-datalake-traversal-plan.md`; CANON node-key HEEL/HIP/TWIG u16 (lance-graph CLAUDE.md) |

## Corrections to cite (do NOT re-derive these)

1. **`jc::weyl` is Weyl *equidistribution*** (golden-ratio low-discrepancy
   sampling), **not** the eigenvalue-perturbation inequality. The genuine
   spectral-perturbation result (Weyl/Davis-Kahan on the Laplacian) lives in
   `perturbation-sim` (`perturbation.rs`, `eigen.rs`). Source:
   `perturbation-sim/README.md` § "Where it sits".
2. **Cesium is transcoded to ndarray, no WebGPU.** `ndarray/crates/cesium`
   (`khr_gs`, `sse`, `hlod`, `implicit_tiling`, `to_cam_soa`, `arcgis_pbf`,
   `osm_pbf`). It is **light** *because* the bounds (SSE, ewa_sandwich, Jirak)
   are closed-form — no trial-and-error.
3. **HHTL coords = effective-resistance spectral Morton embedding** (electrical
   distance, not geography) — `perturbation-sim/src/basin.rs` (Kron reduction +
   Cheeger sweep `μ₂/2 ≤ h ≤ √(2μ₂)`).
4. **`perturbation-sim/splat.rs` is the 2-D power-grid instance** of this
   magnitude-side algebra; the **3-D** instances are `ndarray::hpc::splat3d`
   (EWA pillars) + `crates/cesium` (tiling) + `crates/jc` (weyl, ewa_sandwich).
   Use splat.rs as the *reference shape* for the Σ-fit + ewa-coarsen; wire the
   driver to the 3-D primitives.

## Ownership boundary (read + consume, never reimplement)

- **q2 bake** (`bake_torso_splat.py`): emit per-triangle surfels (centroid,
  normal, tissue colour, opacity). The metric sizing is downstream, not here.
- **q2 driver** (scratchpad, ndarray 1.95 stable, OUT of the q2 workspace):
  consume `ndarray::hpc::{clam, splat3d}` + `jc` + `cesium`. **No** hand-rolled
  sizing constant survives.
- **Substrate** (`ndarray/crates/{jc,cesium}`, `ndarray/src/hpc/{clam,splat3d,pillar}`,
  `lance-graph/crates/perturbation-sim`): owned by other sessions. **Read and
  consume; never reimplement or modify** (architectural-compliance P0).

## MESH PIVOT + Morton/gridlake synergy (2026-06-24 PM — the triangle-surface answer + substrate check)

The opaque SURFEL render killed the fog but left "sequins" (discrete disks with gaps).
Operator: "connect to a triangle filled surface ... kurvenlineal over triangles
(Quadro/AutoCAD)" + "stay with highest quality, do NOT coarsen." The answer: render the
BodyParts3D OBJ FACES **filled** (`THREE.Mesh`, smooth Phong from the cell-averaged
per-vertex normals), not centroids-as-points. Filled triangle surfaces decisively beat
splats — solid ivory bone, red muscle, no sequins, no fog (the Open 3D Man material
aesthetic). The other session reached the same conclusion independently (601,922-tri
solid skeleton).

- **Format:** SPM1 indexed mesh (vert 21 B [pos 3f|normal 3i8|rgb 3u8|opacity u8|node_row
  u16] + tri 12 B [3× u32]); `crates/osint-bake/tools/bake_torso_mesh.py`.
- **Decimation:** vertex clustering, cell-averaged normals (= the smooth "curve-ruler"),
  reuses the is_a tissue classifier. **Highest quality = cell 3.6 mm = 1.40M tris (29 MB)**;
  web-weight coarsening (cell 5.5 mm = 831K) was REJECTED ("skull looks terrible").
- **Render:** `cockpit/src/TorsoMesh.tsx` (live `THREE.Mesh`, two-sided Phong via
  `gl_FrontFacing`, `uFloor` cuts the skin shell, S toggles). The preview driver gained a
  filled-triangle Phong z-buffer rasterizer (SPM1) beside the surfel one.

### Morton tile pyramid + simd_soa "gridlake" for the MESH ("shader batch synergies without the artifacts")

Investigated ndarray read-only. **VERDICT: PARTIAL — the indexing/LOD substrate is
genuinely gaussian-free and reusable; the *render* half of splat3d is gaussian+alpha-blend
and gives nothing for opaque triangles.**

GAUSSIAN-FREE + REUSABLE (no fog path, no `GaussianBatch`):
- `src/simd_soa.rs` `MultiLaneColumn` — fully general SoA carrier (`Arc<[u8]>`, 64-byte
  aligned, zero-copy `iter_f32x16`/`iter_f64x8`/…). **Layout-only, zero geometry awareness.**
  (The `#[derive(SoA)]` macro in `.claude/knowledge/hhtl-gridlake-pre-sprint-prompt.md` is
  PLANNED, not shipped — only `MultiLaneColumn` + the 4 iterators exist today.)
- The Morton min/max **cascade pattern** in `examples/morton_cascade_probe.rs` (example, not
  library API): Morton order makes each quadtree node a contiguous SoA range → flat min/max
  reduction → prune a whole subtree. Adaptable to a spatial per-tile-bbox pyramid.
- `crates/cesium/src/{sse,hlod}.rs` — LIVE + tested + **geometry-agnostic** OGC 3D-Tiles
  SSE (`sse_for_tile`) + HLOD `traverse_hlod` (ADD/REPLACE refine by screen-space error):
  "which tiles at this distance" works unchanged for mesh tiles. (cesium is an oracle-only
  non-default crate — lift the two modules, don't depend on it; `implicit_tiling.rs` Morton
  decoder is all-commented scaffold.)
- `src/aabb.rs` — LIVE, generic, AVX-512 `aabb_intersect_batch` / `aabb_filter_by_distance`
  — the batch frustum/box cull primitive.

MUST BUILD consumer-side (none exists; the splat3d render path is gaussian + alpha-blend):
- A triangle→Morton-leaf tiler + per-tile bbox pyramid (the probe tiles 4×4 scalar cells,
  not triangles).
- Generic `F32x16` vertex-projection + normal-transform kernels — the math is **inlined** in
  the gaussian `splat3d/project.rs::project_chunk_x16`, not exposed; `Camera` (4×4 view +
  pinhole intrinsics) exists and is general.
- An opaque Phong shade + z-buffer rasterizer — `splat3d/raster.rs` is the alpha-blend SH
  compositor (the rejected fog path).

So the synergy is real and **artifact-free at the substrate** (carrier + cascade + LOD +
cull), but a BUILD on top, not a wire-up: gaussian coupling is confined to
`splat3d/{gaussian,project,tile,raster,sh}.rs` + `cesium::to_cam_soa`, none of which the
mesh path must touch. Today's cockpit renders the 1.4M mesh on the GPU (three.js) where this
batching is moot; the gridlake/Morton path is the route to a NATIVE SIMD mesh rasterizer
(offline turntable / server render) if/when that is wanted.

## Cross-refs

- `claude-notes/research/2026-06-24-torso-anatomy-coverage-gap.md` — the is_a-primary
  whole-body atlas (the geometry this renders).
- `claude-notes/plans/2026-06-24-fma-torso-bodyparts3d-splat.md` — the cockpit
  pages + bake plan.
- lance-graph `crates/perturbation-sim/{README,METHODS,CLAM_CHAODA_FRAMING}.md` —
  the magnitude-side algebra + the jc companion framing.
