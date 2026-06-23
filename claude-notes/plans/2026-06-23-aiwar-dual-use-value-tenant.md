# aiwar dual-use → value tenant on the OSINT:Aiwar class

## Overview

The aiwar "AI War Cloud Database" (Sarah Ciston, STARTS Prize 2025) is a
**dual-use taxonomy**: every AI decision system carries a military use and a
civic use, and the actors carry an AIRO role (AISubject / AIDeveloper /
AIDeployer / AIProvider). The project's thesis is the *techno-imperial
boomerang* — tech built for war, turned on citizens — and its method is
"drawing a direct line" between the two.

The dual-use axes (`militaryUse`, `civicUse`, `airo:type`, `MLType`,
`purpose:vair`, `capacity:airo`) were **unwired** in the cockpit. The source
(`AdaWorldAPI/aiwar` + `AdaWorldAPI/aiwar-neo4j-harvest`) models them as
**schema-as-data**: each axis is a `SchemaAxis` node, each value a
`SchemaValue` node linked `VALID_FOR`. `aiwar-neo4j-harvest/src/model.rs`
calls this a *faceted graph* — "nodes belong to multiple overlapping
taxonomies simultaneously."

### The architecture decision (operator, 2026-06-23)

Not separate `SchemaAxis`/`SchemaValue` nodes, not cold name-join metadata,
not a per-axis family-adapter class. **A value tenant in the OSINT:Aiwar
class node's value region**, now that the node is the 4096-bit / 512-byte
CANON node (`key(16) | edges(16) | value(480)`).

This *extends an existing precedent*: `osint_gotham.rs` already uses
`value[CLASS_ORDER_TENANT=0]` as a tenant (the class-label order — an
instance "inherits its label by the order it carries in its value tenant").
The OGAR `ClassView` is display-only over classid `0x0700`. The dual-use
facets become **more tenant bytes** in the same `[0u8; 480]` slab, read by
the same ClassView. This makes dual-use **hot** (a SIMD scan over the value
column filters/groups by facet without touching any cold blob).

The boomerang is literally in the data: `2× "AIDeployer, AISubject"` — one
node that both *fields* the tech and has it *turned on itself* — so
`airo:type` is a **bitset** (compound), not a single code.

## Tenant layout (value-slab bytes)

| byte | tenant | encoding |
|---|---|---|
| 0 | `CLASS_ORDER_TENANT` (existing) | label order into `OSINT_SCHEMA` |
| 1 | `FACET_MILITARY` | `militaryUse` primary token → u8 code (1+idx) |
| 2 | `FACET_CIVIC` | `civicUse` primary token → u8 code |
| 3 | `FACET_AIRO_ROLE` | `airo:type` → u8 **bitset** (Subject/Deployer/Developer/Provider/Operator/Supplier) |
| 4 | `FACET_MLTYPE` | `MLTask`/`MLTasks` primary → u8 code |
| 5 | `FACET_PURPOSE` | `purpose`/`purpose:vair` → u8 code |
| 6 | `FACET_CAPACITY` | `capacity`/`capacity:airo` → u8 code |

Codebooks are the schema-as-data value sets (closed enums), stabilised as
sorted `&[&str]`; `0` = absent/unknown (graceful). Systems fill 1/2/4/5/6;
stakeholders & people fill 3; schema nodes fill none.

## Work items

### Phase 1 — model (this increment, asset-neutral)
- [ ] Add the facet codebook + `facet_code` / `airo_role_bits` /
      `write_facet_tenant` to `osint_gotham.rs`.
- [ ] Populate `value[1..=6]` in `osint_node_rows`.
- [ ] Update the existing tenant-invariant test (byte 0 → bytes 0..=6).
- [ ] New test: a System packs mil/civic/ML/purpose/capacity; a boomerang
      stakeholder packs AIDeployer|AISubject bits.
- [ ] `cargo nextest run -p cockpit-server` green.

### Phase 2 — dimensions IN the schema (facet edges)  ← CORRECTED 2026-06-23

**Why the original Phase 2 was wrong.** The plan was to decode the tenant into
tooltips + an AIRO lens. Operator feedback (with screenshots): "the dimensions
are still not in the schema; the family-adapter-as-model ↔ ClassView doesn't
work." Diagnosis confirmed in the harvest cypher: the 12 `SchemaAxis` + their
`SchemaValue` leaves exist, but there is **zero** edge from any entity to a
`SchemaValue` — the schema is a disconnected legend. The `0x0700` ClassView is
empty + display-only, so it can never put a dimension "in the schema" (a
ClassView renders one entity's fields on click; it does not emit shared graph
edges). The tenant bytes are a hot-scan twin, not graph structure. "In the
schema" = traversable edges.

**The fix (done):** emit `entity → SchemaValue` facet edges — the harvest's own
faceted graph (model.rs pattern #1) that its cypher never emitted.
- [x] `entity_facet_edges()` in `osint_gotham.rs`: per-axis edges (rel 10..15)
      from each node's facet props to the matching `SchemaValue` (keyed by value
      string); compound values split. Emitted in `osint_soa_bytes`.
- [x] `OsintGraph.tsx`: REL_NAME/REL_COLOR for 10..15; a **dimension-layer
      toggle** (`◇ dimensions`) that hides cls 5/6 nodes + VALID_FOR + facet
      edges via vis `hidden` (no relayout) — the "family concepts" off/on.
- [x] tests: `facet_edges_wire_entities_to_schema_values` (5 edges, compound
      split, no spurious sources); logic verified standalone; `npm run build`
      (cockpit) green.
- [ ] **RE-BAKE REQUIRED** — the facet edges are inert until `osint_scene.soa`
      is regenerated. The bake (`cargo test -p cockpit-server --bin q2-cockpit
      -- --ignored bake_osint_soa`) needs cockpit-server to compile (lance 7 +
      datafusion — disk-infeasible in the dev sandbox) AND the FULL enriched
      harvest at `/home/user/aiwar-neo4j-harvest` (NOT the 221-node `public/`
      fallback, which has no `SchemaValue` nodes to link to). Do this on
      Railway / a full-disk machine, then verify node count unchanged and the
      facet edges present.

### Phase 3 — optional follow-ups
- [ ] Canonicalize the facet edges in the harvest cypher (source-side), so all
      consumers get them, not just the cockpit bake.
- [ ] "Same tool" / boomerang traversal: from a `SchemaValue` walk to every
      entity sharing it; flag the Deployer∩Subject nodes.

## Notes
- Source repos cloned to scratchpad: `aiwar` (Quarto site + canonical CSV),
  `aiwar-neo4j-harvest` (cypher schema + Rust harvest model + 30 enrichments).
- Join verified earlier: 65/65 aiwar systems present in the SoA by name.
- `airo:type` lives on Stakeholder/Person (the *player*), not System (the
  *instrument*) — the game-theory actor structure.
