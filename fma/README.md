# fma — FMA-addressed 3D human anatomy

Real connected anatomical geometry (triangle meshes), addressed by the
Foundational Model of Anatomy (FMA), rendered solid on the CPU and served under
`/FMA`. Sibling of the cubus `/torso` route — same idea, real geometry.

> Three things, one system: **geometry** (triangles) ⟷ **address** (canonical
> GUID per part) ⟷ **route** (`/FMA`, `/FMA/turntable`, `/FMA/live`).

![skeleton](docs/mesh_bones.png) ![tissues](docs/mesh_tissues.png)

602K-triangle skeleton · 6.2M-triangle tissue body · `docs/frame_0067.png` is one
turntable frame.

## Pipeline

```
BodyParts3D meshes ──tissue (is_a tree)──► triangle rasterizer (z-buffer + Gouraud)
       │                                          └─► solid renders / 360 turntable
       └──part_of distinguished name──► canonical GUID  classid::HEEL::HIP::TWIG::F4::F5:IDENTITY
                                         (prefix-routable cascade + golden-stride identity mint)
```

- **Geometry** is the real mesh triangles (not gaussian splats): per-triangle
  z-buffered fill with smooth per-vertex (Gouraud) normals, two-sided shading,
  FMA-tissue color (bone ivory, muscle red, vessel red, nerve yellow, …).
- **Address** is the OGAR-canon node key: each part's **distinguished name** is
  its `part_of` ancestry (`human body / cardiovascular system / … / aorta`);
  the GUID cascade tiers are FNV-1a of the cumulative ancestor prefix (so
  siblings share leading groups — prefix-routable), and the **IDENTITY** tier is
  the **golden-stride mint** (`GOLDEN_RATIO × EULER_GAMMA`, stride-4/offset-20 —
  the helix CurveRuler, same generator as bgz17 / bgz-hhtl-d / helix).

## Binaries

| bin | what |
|---|---|
| `mesh` | static solid render (`bones` / `tissues` / `all`) → `mesh/mesh_<mode>.png` |
| `turntable` | parallel 360° prerender, N frames → `fma_frames/frame_NNNN.png` |
| `serve` | dep-free std HTTP server, all routes under `/FMA` (binds `0.0.0.0:$PORT`) |
| `guid` | mint the canonical GUID per FMA node → `guid/guid_manifest.tsv`, `guid/fj_guid.tsv` |
| `anchor` | compression study: cascade vs raw-cartesian vs Cartesian-Skeleton hybrid |

## Routes (`serve`)

All under `/FMA` — no case-only `/fma` vs `/FMA` overlap.

| route | serves |
|---|---|
| `GET /FMA` | viewer: renders + tissue legend + live part lookup |
| `GET /FMA/skeleton.png`, `GET /FMA/body.png` | solid skeleton / tissue body (PNG) |
| `GET /FMA/guid/<FMAID>` | `{container, guid, distinguished_name}` (JSON) |
| `GET /FMA/manifest` | full GUID manifest (TSV) |
| `GET /FMA/turntable` | 360° turntable, 90 fps autoplay (LazyLock-prebuffered frames) |
| `GET /FMA/live` | interactive drag-to-rotate over the same frames |
| `GET /FMA/frame/<i>` | one turntable frame (PNG, from the RAM prebuffer) |

## Run

```sh
./fetch_data.sh                                  # BodyParts3D meshes + combined map
cargo run --release --bin guid                   # GUID manifest
cargo run --release --bin mesh -- data/isa_parts/isa_BP3D_4.0_obj_99 \
    data/combined_element_parts.txt data/inclusion.txt data/isa_inclusion.txt mesh tissues
cargo run --release --bin turntable              # 270 frames (3s @ 90fps)
PORT=8088 cargo run --release --bin serve        # open http://localhost:8088/FMA
```

Build with AVX-512 (x86-64-v4): `RUSTFLAGS="-C target-cpu=native"` (or `x86-64-v4`).

## Data & attribution

`data/*.txt` are the small BodyParts3D ID/relation maps (committed). The meshes
are fetched by `fetch_data.sh` (not committed).

- Geometry: **BodyParts3D**, © The Database Center for Life Science, licensed
  under **CC Attribution-Share Alike 2.1 Japan**.
- Ontology: **Foundational Model of Anatomy (FMA)**.
- Code: Apache-2.0.
