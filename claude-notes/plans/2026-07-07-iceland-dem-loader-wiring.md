# /ice Iceland DEM loader wiring — 2-part split zip → same-origin serve

**Status:** wiring landed (commit `d0a0e347`), **not yet deploy-verified**
(no Docker/Railway in the authoring session). This note is the handover for
the session that verifies the deploy.

## Overview

The `/ice` cockpit scene renders a full-resolution **textured** Iceland DEM.
The bake is `iceland_dem.helix.soa.gz` — **151 MB**, BSO2 **ver 7** (F16 pos +
Signed360 normal + per-vertex RGB texture), **16,515,072 verts / 32,997,378
tris / 3,584 HHTL tiles**. Colour = KIND classification
(ocean/green/rock/scree/ice/lava, from ESRI World Imagery luminance+saturation
+ elevation) × `helix::CurveRuler` golden-spiral residue brightness × imagery
luminance.

Two hard constraints shaped how it ships:

1. **> GitHub's 100 MB blob limit** → cannot be a normal git-tracked file.
2. **This session type cannot upload release assets** (verified: `urllib` and
   `pygithub` both return 403 "Creating, editing, or deleting releases is not
   permitted for this session type") → cannot live in the release like the
   body/berlin wires.

So it is version-controlled as a **2-part split zip** under `.claude/maps/`,
each part < 100 MB:

```
.claude/maps/iceland_dem_16m.z01   76 MB   (part 1)
.claude/maps/iceland_dem_16m.zip   69 MB   (last part = central directory)
```

## The loader chain (how /ice loads the bake)

```
.claude/maps/iceland_dem_16m.{z01,zip}   (committed, 2-part split zip)
        │  Dockerfile builder stage: reassemble + extract
        ▼
cockpit/dist/iceland_dem.helix.soa.gz    (151 MB, single gzip)
        │  include_dir! embeds cockpit/dist/ into the q2-cockpit binary
        ▼
server serves it SAME-ORIGIN at  /iceland_dem.helix.soa.gz
        │  BodyHelix.tsx fetchSoa(): reads body.manifest.json → iceland_latest
        │  → fetch(`/${iceland_latest}`) same-origin FIRST (BodyHelix.tsx:568)
        │  → inflate (DecompressionStream 'gzip' on raw .gz bytes)
        │  → decode BSO2 ver 7 (per-vertex aColor from the rgb block)
        ▼
/ice renders the 16.5M-vert textured terrain
```

The release URL (`${REL}/iceland_dem.helix.soa.gz`) is only a **fallback** that
BodyHelix never reaches when the same-origin copy is present — and it would fail
anyway (the github releases redirect sends no CORS header; that is the whole
reason every big wire is staged same-origin).

## What landed (commit `d0a0e347`)

**`Dockerfile`** (builder stage, `debian:bookworm`):
- Added `zip unzip` to the `apt-get install` line.
- Replaced the old `cp .../iceland.helix.soa.gz` step (which staged the sparse
  ~1M-vert scatter under the wrong bit of the manifest) with:
  ```dockerfile
  RUN cd /build/q2/.claude/maps \
   && zip -s 0 iceland_dem_16m.zip --out /tmp/iceland_dem_full.zip \
   && unzip -o /tmp/iceland_dem_full.zip -d /build/q2/cockpit/dist/ \
   && rm -f /tmp/iceland_dem_full.zip \
   && ls -lh /build/q2/cockpit/dist/iceland_dem.helix.soa.gz
  ```
  `zip -s 0 <last-part> --out <full>` is the canonical reassembly of a split
  archive — `cat`-ing the parts does NOT work for `zip -s` splits. The `.z01`
  part must sit in the same dir as the `.zip` (it does — both under
  `.claude/maps/`, carried into the build by `COPY . /build/q2`).

**`cockpit/public/body.manifest.json`**:
- `iceland_latest` was already `iceland_dem.helix.soa.gz` (unchanged).
- Corrected the stale `iceland_note` (it still described the old Terrarium
  ~1M-vert bake) to the current 16.5M textured one.

## Verified in the authoring session (native, no Docker)

- Reassembly `zip -s 0 iceland_dem_16m.zip --out full.zip && unzip full.zip`
  produces `iceland_dem.helix.soa.gz` — a **valid gzip** (`gzip -t` OK),
  **710,102,109 B uncompressed**, header `42 53 4f 32 07 00` = **BSO2 ver 7**,
  **sha256 `cb3ef34ed6f0e153315b9881ec9dbb15b7ef24364097176effecbd6fb46378d4`**.
- Header decodes to nc=3584, nv=16,515,072, nt=32,997,378.
- BodyHelix `fetchSoa()` fetches `/${iceland_latest}` same-origin first
  (`BodyHelix.tsx:568`), matching the exact filename the Dockerfile stages.
- No stale references to the old name remain anywhere the deploy reads.

## NOT verified (the handover work)

The **Docker build + Railway deploy** was not run (no Docker daemon / Railway
access in the authoring session). To close the loop:

1. **Build the image** (or push and let Railway build):
   ```bash
   docker build -t q2-ice-test .
   ```
   Watch the reassembly step's `ls -lh` line print
   `... 151M ... /build/q2/cockpit/dist/iceland_dem.helix.soa.gz`.
   If `zip`/`unzip` are missing, the apt line didn't take — re-check line ~36.

2. **Confirm the binary embeds it.** `include_dir!` embeds `cockpit/dist/` at
   compile time, so the 151 MB file makes the binary ~151 MB larger (on top of
   berlin 92 MB + the body wires). This is the intended, established pattern —
   the user explicitly wants the full 16.5M-vert bake ("20M is not a budget,
   it's what works flawlessly"). If the build OOMs or the embed is rejected,
   that is the thing to report back, not silently downsample.

3. **Runtime smoke test:**
   ```bash
   docker run -p 8080:8080 q2-ice-test
   curl -sI http://localhost:8080/iceland_dem.helix.soa.gz   # expect 200, ~151 MB
   ```
   Then open `/ice` in a browser (WebGL) and confirm the textured terrain
   renders — green/rock/ice/ocean, smooth, not needles, not muddy.

## Gotchas / notes

- **Do NOT re-split or re-shrink.** The 2-part zip is exactly what the user
  asked for ("2 part zip"). If a rebake changes the bytes, regenerate the split
  the same way and keep two parts:
  ```bash
  # from a fresh iceland_dem.helix.soa.gz:
  zip -s 76m .claude/maps/iceland_dem_16m.zip iceland_dem.helix.soa.gz
  # → iceland_dem_16m.z01 (76 MB) + iceland_dem_16m.zip (remainder)
  ```
  Keep both parts < 100 MB.
- The old sparse `.claude/maps/iceland.helix.soa.gz` (15 MB) is still tracked
  but now **unreferenced** by the deploy — left on disk deliberately (the
  manifest note says so). Safe to delete in a later cleanup if desired.
- A live dev bake at `cockpit/public/iceland_dem.helix.soa.gz` is gitignored
  (`.gitignore:66`) so a 151 MB working copy never gets committed by accident.
- If a release upload ever becomes possible in a future session, the artifact
  could move to the `fma-body-soa-v3-v1` release like berlin (92 MB) and the
  Dockerfile could `curl` it instead of reassembling the split zip — but that
  is optional; the same-origin serve is what matters, and the split-zip path
  already delivers it with zero external dependency.

## Provenance / re-bake recipe

- Fetch DEM + imagery: `scripts/fetch_iceland_dem.py` → `.demgrid` (DEMG v2:
  elevation + ESRI RGB on one grid).
- Bake: `geo/src/bin/iceland_dem.rs` (feature `helix`) →
  `bso2::encode_mesh_bso2(pos, nrm, rows, tris, concepts, labels, colors)`
  (non-empty `colors` ⇒ ver 7).
