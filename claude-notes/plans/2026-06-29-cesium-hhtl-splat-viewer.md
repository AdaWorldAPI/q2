# /cesium — HHTL-tiled 3D-Gaussian-splat viewer (activation scope)

> Scoping doc, 2026-06-29. The third viewer alongside `/body` (BodyV3 mesh)
> and `/helix` (Signed360-normal mesh). `/cesium` is the **splat** path:
> 3D-Gaussian splatting, HHTL-tiled, with the **helix arc as the place-relative
> orientation (`quat_wxyz`) carrier**. No code uncommented yet — this is the
> ordered plan; cesium modules stay review-gated (Opus + CodeRabbit) per the
> crate's own rule before any `//`-scaffold goes live.

## The insight that drives it (corrected 2026-06-29)

helix `Signed360`'s rim is the **`(start, end)` endpoint pair**, each a Fisher-Z
point on the φ-spiral. Two sphere points = a great-circle **arc** = a **rotation**
(3-DOF, a 4D unit quaternion's worth). The crate frames it exactly so: `start` =
"where the arc begins" (the HHTL place anchor, `CurveRuler::from_hhtl(path,depth)`),
`end` = the residue. So the 2 Fisher-Z values do **3D/4D work** — a rotation
**relative to the HHTL tile frame**.

That is precisely a 3DGS gaussian's orientation: `GaussianBatch.quat_wxyz`,
consumed by `Spd3::from_scale_quat` → Σ = R·diag(s²)·Rᵀ. So **helix is the splat
orientation carrier** (place-relative), not sidelined by the quaternion
requirement — it satisfies it. The shading normal `/helix` decodes is just one
2-DOF projection of this fuller frame.

## What already exists (don't rebuild)

`ndarray/crates/cesium` — optional parity **oracle** (not in `default-members`,
deps commented, modules `//`-scaffold). Module map already covers the spine:
- `tileset` / `implicit_tiling` — OGC 3D Tiles 1.1 implicit tiling: subtree
  availability bitstreams + **Morton/Z-order** = the HHTL nibble-interleave
  ("Morton in centroid space"). The HHTL↔Cesium tile bridge is *already designed*.
- `khr_gs` / `spz` / `arcgis_pbf` / `esri_crs` — cold-boundary ingest (reverse-
  engineer → CAM SoA; never depend on / emit source; no JSON in hotpath).
- `to_cam_soa` — parsed splats → `splat3d::GaussianBatch` + `cam_pq` `[u8;6]`.
- `sse` (screen-space-error LOD) / `hlod` (refine ADD·REPLACE) — LOD machinery.
- `point_fallback` — explicitly "coarse-tier **HHTL** preview" (points before splats).
- `oracle` / `fixtures` — SSIM/PSNR diff vs a reference render; golden gates.

`ndarray/.../splat3d` — `GaussianBatch` (SoA: mean/scale/quat/opacity + 48 SH),
`covariance_x16` (SIMD Σ), `project`, `ply` reader. The renderer compute.

## Phases (each gated by its falsification joint)

### P0 — green skeleton + deps
Uncomment `ndarray = { workspace = true }` in cesium; wire `to_cam_soa` to the
real `GaussianBatch` + `cam_pq` types (drop the `UNVERIFIED` placeholders).
Build stays green; nothing rendered yet. **Respect the review gate** — propose
the uncomment, don't ship it unilaterally.

### P1 — one real scene → GaussianBatch (cold ingest)
Start with the simplest reference: Inria binary `.ply` via `splat3d::ply` →
`GaussianBatch` (ground-truth gaussians). `khr_gs` (glTF KHR_gaussian_splatting)
second. This gives the oracle something to diff against.

### P2 — HHTL tiling (implicit_tiling ↔ HHTL)
Quantize each gaussian's `mean_xyz` → Morton/Z-order → HEEL/HIP/TWIG place;
build the LOD tree from `implicit_tiling` subtree availability. `point_fallback`
renders the coarse HHTL tier (points) before splats stream in.
- **J2 (gate):** the HHTL Morton tile address == the `implicit_tiling` subtree-
  local index for the same `(level,x,y[,z])`. Same address, both directions.

### P3 — helix arc → `quat_wxyz` (the insight, made falsifiable)
In `to_cam_soa`, carry each gaussian's orientation as a **place-relative helix
`Signed360`**: `start = CurveRuler::from_hhtl(tile_path, depth)`, `end` = the
residue orientation; decode arc→quaternion (axis = P_start × P_end, angle = arc
length; absolute frame pinned by polar+azimuth). Store the 6-byte Signed360, not
a raw `[f32;4]` quat — the place-relative, HHTL-coupled form.
- **J1 (gate — proves "2-Z does 3D/4D"):** round-trip `quat → Signed360 → quat`
  must preserve the **covariance Σ** (`from_scale_quat`) within tol, and the
  splat render SSIM/PSNR (vs raw-quat) ≥ threshold. If Σ drifts, the arc↔quat
  bijection is wrong and this is a KILL for the place-relative-quat claim
  (fall back: store raw quat, keep helix for the normal/metric only).

### P4 — render path (this is "WebGL ndarray")
Compile `splat3d` (project + depth-sort + `covariance_x16` + SH eval) to wasm32;
`/cesium` cockpit viewer streams HHTL tiles and rasterizes gaussians via
WebGL2/WebGPU. `sse` selects tile LOD per frame. (Contrast `/helix`: a mesh path
that needs *no* ndarray in the browser. `/cesium` is where ndarray→wasm pays off
because 3DGS rasterization is heavy SoA compute.)
- **J3 (gate):** wasm `splat3d` render == native `splat3d` within precision tol
  (no drift crossing the wasm boundary).

### P5 — oracle + parity
`oracle` diffs our render vs an external reference (Inria/Cesium); `fixtures`
holds golden scenes + thresholds. Keeps "parity / better" honest by measurement,
which is the crate's whole charter.

## Cross-repo & rules
- **cesium crate** lives in `ndarray` (activate there, review-gated; relocates to
  `lance-graph/crates/cesium` once flawless). RULE 3: no serde/JSON in hotpath,
  ArcGIS **PBF** over `f=json`. Reverse-engineer only — never depend on / emit a
  source format; output is always CAM SoA.
- **helix** in lance-graph: the arc→quat decode mirrors `/helix`'s rim→normal
  decode. Ideal convergence — ship the helix codec as **one wasm** used by both
  `/helix` (normal) and `/cesium` (quat), killing TS reimplementation drift.
- **/cesium viewer** in `q2/cockpit` (like `/helix`).

## First concrete step (smallest provable slice)
Not the whole pipeline — **J1 only, native, no viewer**: a Rust probe that takes
a real `.ply` GaussianBatch, encodes each quat as a place-relative `Signed360`,
decodes back, and reports Σ-error + per-gaussian rotation error (deg). Green J1 =
"the 2 Fisher-Z carry the 3D/4D orientation" is proven on real data before any
tiling, wasm, or cockpit work. Same shape as the `helixbake` round-trip that
proved the normal path (mean 0.26°). If J1 fails, we learn it cheaply.
