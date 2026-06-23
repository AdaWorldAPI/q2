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

### Phase 2 — wire + view (next increment, touches the baked asset)
- [ ] Emit the 6-byte facet tail per node on the OSO1 wire (additive, after
      the label tail; old readers ignore it).
- [ ] Mirror the codebook in `OsintGraph.tsx`; decode the facet tail.
- [ ] Surface dual-use in node tooltips + the reasoning readout (the "direct
      line" narration).
- [ ] Add the **AIRO-role "meta" lens**: recolour actors by game-theory role
      (Subject = harm lands here, Deployer fields, Developer builds, Provider
      supplies); boomerang nodes (Subject+Deployer) flagged.
- [ ] Re-bake `assets/osint_scene.soa` from the FULL harvest
      (`/home/user/aiwar-neo4j-harvest` — must be the 920-node enriched
      source, NOT the 221-node `public/` fallback) and verify node count
      unchanged.
- [ ] `npm run build` (cockpit) green; inspect the rendered scene.

## Notes
- Source repos cloned to scratchpad: `aiwar` (Quarto site + canonical CSV),
  `aiwar-neo4j-harvest` (cypher schema + Rust harvest model + 30 enrichments).
- Join verified earlier: 65/65 aiwar systems present in the SoA by name.
- `airo:type` lives on Stakeholder/Person (the *player*), not System (the
  *instrument*) — the game-theory actor structure.
