# One 128-bit node: OSINT graph, CAD, and Gaussian splats on one substrate

> 2026-06-22 · status: design converged, OSINT instantiation in progress
> Context: hydrating the aiwar/OSINT graph as a Palantir-Gotham/neo4j proof of
> concept surfaced a substrate that generalizes well beyond OSINT. This note
> captures the convergence so it is durable, not chat-ephemeral.

## The thesis — one object

Every "thing" in all the systems below is the same **128-bit canonical node**
(`lance_graph_contract::canonical_node`), three fixed blocks:

```
node = GUID(128 bit)              ← the ADDRESS
         classid · HEEL · HIP · TWIG · leaf · family · identity
     ⊕ EdgeBlock(128 bit)         ← RELATIONS  (16 × 8-bit family-node mixins, flat)
     ⊕ value tenant(16 × 8-bit)   ← PARAMS/LABELS (ordered slots, classid-typed)
```

- **classid** routes the kind (OSINT entity / CAD primitive / Gaussian / …).
- **GUID upper 64 bit** = `classid·HEEL·HIP` (class + coarse HHTL cascade).
- **GUID lower 64 bit** = `TWIG·leaf·family·identity` (the basin-local key /
  fast local address once the HHTL prefix is trie-bound).
- **EdgeBlock** = up to 16 mixin/relay adapters (the 12+4 split is waived → flat).
- **value tenant** = 16 ordered 8-bit slots; what they mean is `classid`-typed:
  OSINT label-orders, CAD quantized params, splat attributes.

## The flow — compile → operate → naturalize

1. **Compile** (once, at ingest): a front-end lowers source into the address
   space. Regex/parsing lives *here only*. cypher text → nodes; NL → CAD prims;
   video/point-cloud → Gaussians.
2. **Operate** (every query/edit): pure bit ops over the address space — classid
   prefix routes, `EdgeBlock` walks adjacency, value-tenant **mask/slot reads**
   test labels/params. **No strings, no re-parse.**
3. **Naturalize** (only at the edge): bits → display. OSINT → JSON; CAD → Blender
   geometry; splat → render. The human label (`"Stakeholder"`) is a codebook
   lookup done *once*, last — or pushed to the client with a tiny codebook.

The anti-pattern this kills: `GUID → label-string → regex-parse → serialize`
on every read. The "parse result" is already bits (classid + tenant); strings
are a late naturalization, never the operating surface.

## HHTL + helix + family-mixins = a general spatial index (the splat reuse)

Hydrating the tree as **HHTL** (cascade tiers) + **helix** (φ-spiral / space-
filling order on `identity`) + **family mixins** (EdgeBlock) is not OSINT
plumbing — it is a 128-bit spatial acceleration structure. For Gaussian
splatting it gives, for free:

| splat need | primitive |
|---|---|
| LOD / frustum cull | HHTL prefix routing (mask upper GUID bits) |
| depth sort + cache locality | `identity` = helix (Morton/Vogel) order |
| densify / prune / kNN | EdgeBlock neighbor adapters (no separate kd-tree) |

Open knob: `helix_order(&Graph) -> Vec<u16>` — the rank function onto the
spiral. Default: degree-ranked within `(HEEL,HIP)`, basins golden-angle
sequenced. A splat consumer swaps it for "sort by 3D position."

## Convergence across the literature (why this isn't ad-hoc)

| reference | primitive=classid | params=value tenant | relations=EdgeBlock | hierarchy=HHTL | naturalize |
|---|---|---|---|---|---|
| Text2CAD (2409.17106) | line/arc/circle/extrude | **8-bit quantized → 256 labels** | construction seq | sketch→face→loop→curve | mesh |
| CADAM (Adam-CAD) | OpenSCAD module | 2–22 sliders, **edit w/o re-gen** | module calls | SCAD tree | STL/SCAD/DXF |
| ProcFunc (2604.26943) | Blender prim fn | typed args | **compute graph** | primitive→module→scene | Blender render |
| Motion-Blender GS (2503.09040) | 3D Gaussian | pose/scale/opacity/color | **motion graph links** | kinematic tree | splat render |
| Blender Geometry Nodes | node op | input sockets | **node graph edges** | node tree | geometry |

Five non-coincidences:

1. **Text2CAD quantizes every param to 8-bit/256** = the value tenant byte-for-byte.
2. **Motion-Blender propagates link motion → Gaussians via skinning weights** =
   the **family-mixin (relay) → member** propagation. Kinematic tree = HHTL;
   deformable graph = EdgeBlock.
3. **ProcFunc primitive→module→scene + compute-graph** = HHTL tiers + EdgeBlock.
4. **CADAM edit-without-re-generation** = operate-on-bits, re-naturalize (the
   no-string-roundtrip principle, shipping).
5. **Blender Geometry Nodes is literally typed-nodes + edges → geometry** — the
   substrate running in production.

## Instantiation #1 — OSINT / Gotham (current, measured)

aiwar-neo4j-harvest: base 221 + 30 cypher enrichment rounds.

- **Was dead**: harvest JSON had 1660 bare `NaN` (illegal JSON) → enrichable
  copy unloadable → silent fallback to base 221. Fixed: NaN/Infinity→null in
  `load_from_str`.
- **Parser was lossy**: regex emitted unbound cypher variables `a`/`b`/`v` as
  phantom endpoints (~60% of edge endpoints). Fixed: per-`;`-statement variable
  resolution (bind by id→value→name; WITH / multi-MATCH / inline endpoints;
  skip unresolved; drop pandas `nan`/empty).
- **Result**: 734 real nodes, 2065 resolved edges, isolated 195→29, top hubs
  real (United States/Palantir/Israel/Epstein). All 7 labels covered →
  **0 items missing label↔identity**.
- **Link chart**: 2032/2065 typed entity→entity edges now in the OSINT view
  (was 0) over 186 basin compartments + the CANON substrate.
- Tests: 19 aiwar-ingest + 7 cockpit `osint_gotham`. `eval_650` (ignored) is
  the reproducible richness harness.

## Next (the bit-encode refactor — not yet landed)

1. value tenant → 16×8-bit **ordered** slots (slot 0 = class order today;
   1..16 reserved as the CAD/splat param carrier).
2. resolution keyed by the **full 128-bit GUID** (`to_hex_v2`), not `osint:{id}`.
3. **helix** identity ordering + 4-tier HHTL assignment.
4. naturalize labels **only** in the `/api/graph/osint` handler.
5. production hydrate → the 734-node enriched set (today main.rs prefers the
   clean-but-un-enrichable `cockpit/public` copy).
