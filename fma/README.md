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
| `guid` | mint the part_of GUID per FMA node → `guid/guid_manifest.tsv`, `guid/fj_guid.tsv` |
| `converge` | **v3**: cascading-HHTL `(place:tissue)` **canonical NodeGuid** + `connected_to` edges → `guid/{guid_converged,nodes,edges}.tsv` |
| `graph` | **v3 render**: the connectivity graph — nodes placed/colored by the key, wired by `connected_to` → `graph/graph_<mode>.png` |
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

## Three coexisting FMA addressings (lose neither version)

The Ada workspace has two independent FMA bodies of work; this crate adds a third
that converges them **without replacing either** — disjoint files, disjoint routes:

| version | what | axis | where |
|---|---|---|---|
| **v1** (other session) | FMA **heart** graph, canonical `NodeGuid`, served at **`/fma`** | `is_a` (taxonomy) | `crates/osint-bake/.../fma.rs`, `cockpit/.../FmaGraph.tsx` |
| **v2** (this crate, `guid`) | **full-body** part_of FNV cascade + 3D mesh at **`/FMA`** | `part_of` (mereology) | `fma/src/bin/guid.rs` |
| **v3** (this crate, `converge`+`graph`) | cascading-HHTL **canonical `NodeGuid`** + `connected_to` + key-driven render | **`(place:tissue)`** | `fma/src/bin/{converge,graph}.rs` |

**v3 is the convergence** — and it now converges the *render*, not just the address.
Each 8:8 HHTL tier packs both axes — `high = place` (*where*), `low = tissue`
(*what* = is_a) — cascading HEEL→HIP→TWIG so the high-byte chain prefix-routes the
body and the low-byte chain prefix-routes the type taxonomy: **both hierarchies in
one key.** The 16-byte layout is byte-identical to
`lance_graph_contract::canonical_node::NodeGuid` (OGAR canon, 2026-06-13:
`classid·HEEL·HIP·TWIG·family·identity`); `classid` uses the same `0x0A`
`ConceptDomain::Anatomy` space as v1's bake (`0x0A01` soft, `0x0A02` skeleton). v3
is dep-free, so this crate stays standalone.

Three things make `place` and the render converge (`classid`-dispatched, OGAR `HhtlMode`):

1. **Located skeleton** — for `0x0A02` bones, the `place` bytes are the **Morton
   spatial cell** of the bone centroid (the exact anchor *is* the key — my `anchor.rs`
   hybrid). For `0x0A01` soft tissue, `place` is the `part_of` rank (Cascade), inheriting
   position from its `part_of` basin's skeleton anchor.
2. **`connected_to`** — the canonical EdgeBlock (12 in-family + 4 out): `part_of`
   siblings are the in-family adjacency (the aortic segments / heart chambers that
   physically connect), the `is_a` parent is the out-of-family type link.
3. **The renderer reads the key** — `graph` places each node by `place`, colors it by
   `tissue` (is_a), and wires it by `connected_to`. No mesh needed: the address *is* the
   render.

```text
Located skeleton (thoracic vertebrae T9/T10/T11, classid 0x0A02, mode Located):
  FMA10014  00000a02-ce01-fe02-7b02-…   ↔ T10,T11,T12,…   shared Morton HEEL ce = same spatial octant
  FMA10059  00000a02-ce01-d602-eb02-…   ↔ T9,T10,T12,…    HIP/TWIG descend as the centroid descends (z 1164→1107)
Cascade soft tissue (aortic segments, 0x0A01, mode Cascade):
  FMA3736 ascending  00000a01-0901-0702-0e02-…  ↔ arch, descending   part_of siblings = the connected segments
```

![FMA connectivity graph](docs/graph_all.png)

*`graph all` — 1368 FMA nodes placed by `place` (Located Morton for the ivory bone
clusters in skull/hands/feet, centroids for the red vascular tree), colored by `tissue`
(is_a), wired by `connected_to`. The address is the render.*

## Run

```sh
./fetch_data.sh                                  # BodyParts3D meshes + combined map
cargo run --release --bin guid                   # v2 part_of GUID manifest
cargo run --release --bin converge               # v3 (part_of:is_a) canonical NodeGuid → guid/guid_converged.tsv
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
