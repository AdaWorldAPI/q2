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

## LOCKED design (operator review, 2026-06-27) — supersedes the BSO1 AoS bake

The wire is **SoA columns** (`MultiLaneColumn`, 64-byte aligned, `Arc<[u8]>`),
joined by ONE SoA row identity. **Store identities/indices, never raw values;
ClassView dereferences.** Three separate 16-byte GUID columns (cheap: 16 B ×
100k = 1.6 MB; × 396k surfels = 6.4 MB — separation beats bit-packing):

| column | 16-B GUID / value | content | resolves via |
|---|---|---|---|
| **address** | `classid 0x1000` + `(part_of:is_a)` **8:8** cascade + identity tier | the node key (unique; routable prefix) | ClassView / registry |
| **location** | `XYZ` standard 3D | position — GPU/slicer native; Z = slice, X·Y = in-slice 256² grid (256=4⁴ hierarchical) | direct |
| **helix** | **2 helices** + reserved | helix-pos (dir from origin 0,0,0, 3 B) · helix-normal (self orientation, 3 B) · depth derivable by trig from XYZ | direct decode |
| material | **codebook index** | Doppler flow class (low-res artery / high-res artery / portal / hepatic-vein / caval) | ClassView → material prototype |
| label | **codebook index** | never raw text | ClassView → text + synonyms |
| edges | **SoA-row refs** | part_of parent / branches / supplies / synonym alias | ClassView |

Rules that fell out of review:
- **Collusion = a location collision = same geometry ⇒ a ClassView resolution,
  not a bug.** (`celiac trunk ≡ celiac artery` = same lumen → ClassView aliases;
  the 3 celiac branches have distinct XYZ → distinct, linked as children.)
  Bilateral pairs are unique by x-sign for free.
- **Relationships reference the SoA row (linked identity), never embed a
  neighbour's location/helix.** Edge says *who*; row says *where/what/oriented*.
- **64k⁶ = (256³)⁴** — same space; XYZ-bytes factoring is the slicer-native one.
- **Tubes:** normal is radial from the centerline ⇒ helix-normal derivable by trig
  from XYZ + the part_of branch-tree centerline; slicer fills cylindrically
  (depth-along-axis · helix-angle · radius). Explicit helix bytes only for
  non-radial surfaces (sheets/capsules).
- **Material fill** densifies the *surface* (slicer-style), per Doppler class.
- **Render:** Gouraud shading; **bgz17/Base17 (#17) palette drives the alpha /
  transparency** channel (17 levels). Keep **6+ M polygons** (NO decimation to
  1.6 M yet — LOD pyramid is later).
- compute server-side (ndarray native SIMD); deno_core/V8 is the *document* JS
  engine, never in the 3D path.

## Shipped increments (2026-06-28)

### Vessel "inflatable tube" fix — `fill_body_soa.py`
The slicer-fill cores ballooned where vessels curve: the radius was the
perpendicular distance from the **global** PCA axis, so a point on a bend sits
far off the straight axis → radius inflates. Fixed: bin the points along the
axis, then derive each ring from its **own bin** — centroid = bin's local centre
(follows the curve), radius = **median** perpendicular distance from *that* bin
centroid (robust to outliers), clamped to an **absolute** `[RMIN, RMAX]`
diameter boundary (`RMAX=0.020` ≈ 34 mm dia, covers the aorta, kills balloons).
662 vessels → +71,872 core verts / +133,152 tris.

### Half-precision positions — the "A" brick
> **SUPERSEDED to F16 (BSO2 ver 5).** BF16 (ver 4) was tried first and **rejected**:
> its 7-bit mantissa gave a ~3 mm step near the head (y≈0.85) → a visible staircase
> (Treppeneffekt) on the eye/brain. Shipped format is **F16 / IEEE half (ver 5)** —
> same 6 B/vertex, 10-bit mantissa, ~0.2 mm (measured 0.21 mm max over the wire), no
> staircase. Bake uses ndarray's `F16::from_f32`; renderer widens via a 64K half→f32
> LUT. `BodyV3.tsx` reads ver 3 (f32) / 4 (BF16) / 5 (F16). gz ≈ 63 MB (vs f32's 80).
> The BF16 description below is retained for history.

Per-vertex `pos` column was **BF16** (3× u16 LE = 6 B/vertex), half of ver 3's
12 B f32. Conversion via ndarray's sanctioned RNE batch path
(`f32_to_bf16_batch_rne`) on the native bake host (AVX-512/AMX). The renderer
widens back to f32 client-side (`bits << 16` — BF16 is the top 16 bits of f32, so
the widening is exact). Round-trip ≈ 1.4 mm — which turned out to be visible on
small smooth structures, hence the F16 upgrade above. Asset in release
`fma-body-soa-v3-v1` (Dockerfile pulls it same-origin).

### "B": server-side HHTL-O(1) LOD endpoint — WIRED (de-risked)
cockpit-server can't build in-sandbox (quarto-core→runtimelib is a proxy-blocked
git dep), so B is a blind deploy — de-risked three ways:
1. **Verified core, standalone:** `scratch-fma/bodylod` builds + runs here against
   ndarray-only. `build_blocks(wire)` → per-concept `BlockBounds`, `concept_actions`
   → `cascade_blocks` (HHTL HEEL→HIP→TWIG→LEAF, O(concepts≈1658), the O(1)
   reference). Monotonic LOD on the real BF16 wire: near 1521 ProjectExact / 137
   KeepCoarse → far 211 / 1447. The cockpit-server handler reuses this exact logic.
2. **Tiny embedded asset, not the geometry:** `soabake` bakes `body.blocks`
   (1658×16 B = 26 KB: centroid + radius per concept, in the renderer's DISPLAY
   space so the client posts its three.js camera directly). cockpit-server
   `include_bytes!`s it — no 57 MB startup gunzip, no feature gate.
3. **Opt-in client, default OFF:** `BodyV3.tsx` keeps the full render; a "server
   LOD" toggle (default off) posts the throttled camera to `/api/body/lod`, writes
   the per-concept `HhtlAction` bytes into a 1658-px R8 `DataTexture`, and the
   frag shader discards Reject concepts (gated by `uLodOn`). If the endpoint 404s
   (old deploy) or errors, it silently falls back to the full render. So a wrong
   cull (camera-space math is unverifiable here) only ever shows when the user
   flips the toggle — never by default.

Files: `crates/cockpit-server/src/body_lod.rs` (+ route in `main.rs`, `splat3d`
feature in `Cargo.toml`, `assets/body.blocks`); `cockpit/src/BodyV3.tsx`.
**Deferred (Phase B pyramid):** with single-LOD geometry the cascade only
distinguishes show/cull, so the win is frustum-culling whole concepts when zoomed
in; switching KeepCoarse → a decimated mesh needs the L1/L2 decimation pyramid.

## Constraints
- Big baked assets (LOD pyramid, fill) → GitHub Releases (q2 `fma-body-soa-v3-*`),
  never git. `cockpit/public/body.soa*` gitignored.
- q2 workspace cargo can't build in-sandbox (proxy-blocked `runtimed` git dep);
  ndarray-only crates verify standalone; the server build runs on deploy.
- No model identifier in any committed artifact.
