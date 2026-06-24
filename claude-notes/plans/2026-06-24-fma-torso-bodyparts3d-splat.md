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
- Brush: splat3d render uses gaussian scale 0.0025 (was 0.008 — the big isotropic
  brush blobbed the detail into a "Warhol" look; 0.0025 at 810x1080 restores the
  ribcage/vertebrae). Frames re-rendered.

## Follow-up PR — anisotropic + GUID-tag + map (branch claude/torso-anisotropic-map)

- [x] **SPL2 format** (supersedes SPL1): hdr 40B [`SPL2`|count|node_count|radius|
      bbox]; body 21B [pos 3f | normal 3i8 | rgb 3u8 | opacity u8 | node_row u16].
      Helix-orderable + residual-ready (the codec PR slots in here).
- [x] **Anisotropic surface-fit gaussians** ("connect the dots"): bake reads OBJ
      `vn` (BodyParts3D ships normals — free, no face traversal); render driver
      orients each gaussian flat-to-surface (`scale=[t,t,thin]`, `quat` aligns
      local-z to the normal). Tangent 0.004 connects within a structure while
      rib gaps stay visible. NOT voxels (continuous surfaces, not a discrete grid).
- [x] **Per-node SoA + O(1) switch** (the GUID/value-tenant backbone): bake emits
      `torso.nodes.json` — one row per FMA structure (178 rows, 91 own meshes):
      fma id, name, depth, HHTL tier-ranks, colour, gaussian RANGE (start+count),
      OBJ-geometry tenant (centroid + bbox + FJ handles). Each gaussian carries
      its node_row. Consumers build the switch (row -> node) once -> O(1) tenant
      reads. Position = real BodyParts3D coordinate; identity = the FMA node.
- [x] **/torso-map page**: click a gaussian -> node_row -> node SoA -> FMA label +
      partonomy breadcrumb; structure list (graph -> splat) highlights gaussians.
      Realises the osint-cad-splat thesis: graph and splat, one node at one address.
- [x] tsc clean. Browser pick-interaction not exercised here (raycast-on-Points
      logic is standard; geometry verified via the CPU frames).

## Helix-anchor codec — MEASURED (branch claude/torso-helix-codec)

`tools/spl_codec.py` encodes SPL2 -> SPL3 and round-trips it. The x265-for-
gaussians design, mapped to signals already in SPL2 + the node SoA:
  helix    = 3D Morton (Z-order) of position = identity/GUID order (locality-preserving)
  anchor   = FMA node (SoA centroid + per-node colour) = the I-frame, random-access
  motion   = gaussian offset from its node anchor (the motion vector)
  residual = helix-ordered zig-zag delta of (motion, normal)
  colour   = ANCHOR-PREDICTED -> 0 per-gaussian bytes (a 178-entry node palette)

Measured on the real torso (231,515 gaussians):
- SPL2 21.0 B/g -> SPL3 7.47 B/g  =>  **2.8x smaller** (zlib entropy stand-in)
- colour: **exact, 887 B total** for ALL colour (crisp by construction, no bleed)
- position round-trip RMSE **0.00001** (16-bit quant, effectively lossless)
- node_row RLE 35 KB / 231K gaussians (structures contiguous in helix order)
- stream split: motion 1.02 MB, normal 671 KB (the optimization target -> octahedral
  + range coder), rows 35 KB, palette 887 B

Validates the design before wiring it into the render. Next increments:
- [ ] octahedral normals + range coder (the 671 KB normal stream)
- [ ] decode SPL3 at cockpit load; anisotropic/edge-aware reconstruction
      (node_row-bounded + normal-oriented = crisp colours in the render)
- [ ] animation: deform node anchors -> motion-skinned gaussians follow
      (Motion-Blender GS; the partonomy is the rig)

## Best shading + lazylock + adaptive-FPS + SPL4 (branch claude/torso-shading)

User: "best possible shading and lazylock buffering to mitigate batching", then
"adaptive framerate prediction + SIMD batching + v4", then the key insight: "the
Motion is fixed Rotation ... so it could easily prebuffer 270 frames for 90 FPS".
Scoping answers: framerate = BOTH (render-loop throttle now + codec P-frames as
the SPL4 motion track); PR scope = all of the above incl SPL4 in one push.

### Infra fact ("GitHub uses Cargo not Dockerfile?")
q2 CI = pure Cargo+npm (`cargo fmt`/`xtask lint`/`clippy -D warnings`/`nextest`,
wasm-pack/npm). The only `docker` in CI is `docker image prune` (free runner disk).
The root `/Dockerfile` is Railway-deploy ONLY (`q2-cockpit` embeds the Vite cockpit,
clones lance-graph/ndarray/neo4j for the graph hot path). This splat feature does
not touch the Dockerfile.
- [x] **Dockerfile CPU baseline -> x86-64-v4** (user ask): `ENV
      CARGO_BUILD_RUSTFLAGS="-C target-cpu=x86-64-v4"` before the cockpit-server
      build. Flips `target_feature="avx512f"` so q2-ndarray's `simd.rs` picks the
      native `simd_avx512` backend. BF16+AMX tile GEMM ride ndarray's runtime
      autodetect polyfill (`simd_caps()` + AMX arch_prctl/model-detect) — not gated
      by the flag, lit only when the host has them, AVX2/scalar fallback always
      compiled. ⚠ v4 = AVX-512 REQUIRED at runtime (SIGILL otherwise, the PR#170
      mode one level up); AMX needs Sapphire/Emerald/Granite Rapids at runtime
      (autodetect skips it otherwise = agnostic working as intended). Documented the
      `x86-64-v3` fallback in the Dockerfile for non-AVX-512 deploy targets.

### Shading (the lit look) — DONE
- [x] Render driver (scratchpad, ndarray 1.95, OUT of q2 workspace): shade AT
      RECONSTRUCTION from the per-vertex normal already in SPL2 — hemisphere ambient
      (sky/ground) + key diffuse (n·L, L fixed in WORLD so camera orbits a still
      light = consistent turntable) + soft fill. Shading MULTIPLIES the flat palette
      colour, so the codec-free per-structure colour story is intact. 20-frame
      shaded turntable rendered (9s/frame) → JPEG (67 KB/frame) →
      cockpit/public/torso-frames/. Verified in-cockpit: volumetric depth, colours
      preserved, no Warhol blob.

### Prebuffer = the answer to BOTH (A) and (B)  [the user's insight]
The demo motion is a FIXED, periodic, deterministic camera rotation. So you neither
ADAPT the framerate nor PREDICT motion frame-by-frame — you PRECOMPUTE the closed
loop once and replay → every frame free → guaranteed 90 fps. This is exactly the
x265 GOP idea: a periodic camera path is a closed Group-of-Pictures; prebuffer the
GOP, replay forever. It is ALSO the honest SPL4 (B) motion source: the orbit is a
real known closed trajectory, so the 270 rotation steps ARE its P-frames — NO
synthetic breathing deformation needed (drop that demo).
- [ ] /torso turntable: bump FRAME_COUNT 20 → loop count over an exact 360° (frame
      N == frame 0 for a seamless loop), 90 fps playback. Re-bake at the higher count
      (background). Ship-size lever: 67 KB/frame × 270 ≈ 18 MB JPEG → offer WebM
      encode (~3 MB) as the compaction. Mandatory here because CPU EWA splat is
      9s/frame — live render impossible; prebuffer is THE technique, not an optim.
- note: the live WebGL points view is already real-time; prebuffering full
      framebuffers there is VRAM-prohibitive (270×810×1080×4 ≈ 945 MB) — so the
      live-view win is lazylock + adaptive-FPS, and image-prebuffer stays on /torso.

### Live views light up + lazylock + adaptive-FPS
- [ ] /torso-live (TorsoSplat) + /torso-map (TorsoMap): decode SPL2 `normal 3i8`
      into an aNormal attribute (both skip it today); port hemisphere+diffuse+fill
      into the FRAG. Same L → CPU frames and live WebGL agree.
- [ ] LazyLock build-once buffer: build geometry (pos+aColor+aNormal+aRow) ONCE;
      mutate only via uniforms + draw-RANGE, never rebuild.
- [ ] Adaptive-FPS: EMA of rAF delta; over budget → shrink draw-range over the
      Morton-ordered buffer (prefix = uniform spatial subsample) + drop pixelRatio;
      recover when cheap; log active fraction (no silent decimation).

### SPL4 — ship the codec (static I-frame real, motion track reserved)
- [ ] `spl_codec.py`: WRITE a real `.spl4` (helix-Morton order, per-node anchor
      I-frame, motion-from-anchor + zig-zag residual, anchor-predicted palette colour
      = 0 per-gaussian bytes, normals). Header `motion_track_count` (0 static) reserves
      the P-frame slot without a format bump (RESERVE-DON'T-RECLAIM).
- [ ] TS `decodeSpl4`: inverse — reconstruct pos/normal/rgb/row at load; all 3 views
      switch to SPL4.
- [ ] Fold deferred #55 nits: `import math` → module top; fix "round-trips it"
      docstring; TorsoMap `ray.params.Points` mutate-not-replace.
- [ ] (B) motion track = orbit-as-motion P-frames (above); ship the FORMAT slot +
      decode contract; the camera trajectory is the demonstrator (honest, not faked).

### Verify + ship
- [ ] `cd cockpit && npm run build` (tsc clean); inspect shaded turntable + live
      view; codec round-trip RMSE unchanged. Commit incrementally on
      claude/torso-shading; ASK before push (GIT PUSH POLICY).

## v4 — is_a-PRIMARY whole-body anatomical atlas (major pivot, 2026-06-24)

Operator-driven pivot, several corrections of my assumptions:
1. **Use is_a, not part-of, for classification + names.** part-of is REGIONAL
   (walk up a muscle -> chest wall -> thorax, never "muscular system") and its
   names aren't canonical. is_a is the TYPE tree: every structure resolves up to
   its canonical type (`pectoralis minor` -> ... -> `muscle organ`); is_a ships
   canonical names; is_a's mesh set is a SUPERSET of part-of (2234 vs 1258 FJ,
   +976) with finer organ segmentation (no single "aorta"/"heart" — split into
   ascending/arch/descending/abdominal, each its own mesh). Downloaded the 142 MB
   is_a obj package + the small is_a relation/name txts.
2. **container:identity / DN->GUID addressing.** tissue = walk the is_a TYPE tree
   to the first type keyword (O(1), cached) = the DistinguishedName path, which
   MATERIALISES to a numeric container:identity GUID (container = tissue class).
   Stored per node: `tissue`, `is_a` (DN path, upper-ontology stripped),
   `container`, `identity`, `guid`.
3. **Whole body is the goal — NO spatial torso filter.** Region focus (torso, an
   organ) is a future SELECT -> CAMERA-ZOOM feature on the full-body splat, driven
   O(1) by each node's centroid+bbox in the SoA, not a bake-time clip.
4. **Performance is the point.** Whole body = 602,341 gaussians / 1658 is_a
   structures / 12.6 MB (414 arteries, 382 muscles, 221 veins, 203 bones, 126
   nerves, full viscera). The deliberate load that motivates lazylock +
   adaptive-FPS (live views) and the prebuffered turntable (CPU EWA).
- bake = `bake_torso_splat.py` v4 (is_a-primary). Tissue atlas palette + depth-peel
  opacity. Driver orientation fixed (+90 about X; head was landing down).
- [ ] re-render upright whole-body turntable -> /torso; live views already decode
      the unchanged SPL2 (extra nodes.json fields are ignored) — light them +
      lazylock + adaptive-FPS to show + mitigate the 602K load.
- research: `claude-notes/research/2026-06-24-torso-anatomy-coverage-gap.md`.
