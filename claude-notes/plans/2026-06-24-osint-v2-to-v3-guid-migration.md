# OSINT/Gotham GUID migration: v2 (7-group) → V3 canonical `(part_of:is_a)`

> Status: **planned, not executed** — the migration is cross-cutting and cockpit-server
> can't compile in this environment (lance-graph-ogar absent). This doc is the executable
> design; the code change is a follow-up PR validated in CI.

## Critical finding (the reason this is mandatory, not cosmetic)

`crates/cockpit-server/src/osint_gotham.rs` mints with **`NodeGuid::new_v2(...)`** and reads
**`.family_v2()` / `.identity_v2()` / `.to_hex_v2()` / `NodeGuid::CLASSID_OSINT`**. **None of
those symbols exist in the current `lance-graph-contract::canonical_node`** — it ships only the
V3 6-group API (`new`, `family()`, `identity()`, `heel/hip/twig()`; verified by reading the
source). So the OSINT bake **does not compile against the current contract**. This is the
live `I-LEGACY-API-FEATURE-GATED` case: a removed v2 layout API still called by a consumer.
The v2→V3 migration is required for cockpit-server to build at all.

(Separately: cockpit-server *also* can't build here because `lance-graph-ogar` is absent from
this lance-graph checkout — a second, orthogonal blocker. Both must clear in CI/deploy.)

## The V3 `(part_of : is_a)` mapping for OSINT

Canonical key = `classid(4) | HEEL(2) | HIP(2) | TWIG(2) | family(3) | identity(3)` (LE). Each
HHTL tier is an 8:8 tile — **high byte = `part_of` (WHERE/partonomy), low byte = `is_a`
(WHAT/taxonomy)** — exactly as FMA/CPIC. OSINT already has both hierarchies; pack them in:

| group | high byte (`part_of`, WHERE) | low byte (`is_a`, WHAT) |
|---|---|---|
| **HEEL** | enrichment-round **theme** index | OSINT_SCHEMA **class** order (System/Stakeholder/Person/…) |
| **HIP** | **anchor** nibble (org/figure tied to) | **AIRO role** bitset (Developer/Deployer/Subject…) |
| **TWIG** | 0 (reserved finer partonomy) | **primary dual-use facet** (military ∨ civic ∨ ML code) |
| **family (u24)** | basin `(theme<<4)\|anchor` — preserves the EdgeBlock mixin-mask | |
| **identity (u24)** | node index | |

**Why this is the win the operator asked for:** today the dimensions connect only by *edges*
(the facet edges rel 10-15). After this, dimension membership is **encoded in the address** —
entities sharing a class/role/facet share `is_a` low-byte prefixes, so they **prefix-route
together**. The dimensions stop being free-floating islands *structurally*, not just visually.
(The current floating is a separate, simpler issue: a **stale deployed `.soa`** — the committed
`osint_scene.soa` already has 431 facet edges + 0 isolated nodes; a redeploy connects them.)

## Per-file changes (executable)

### 1. `crates/cockpit-server/src/osint_gotham.rs` (the minting + readers)
- Add `const CLASSID_OSINT: u32 = 0x0000_0700;` (replaces the removed `NodeGuid::CLASSID_OSINT`).
- `osint_node_rows`: replace
  `NodeGuid::new_v2(CLASSID_OSINT, theme_hi, anchor_lo, 0, 0, basin.into(), i as u16)`
  with
  `NodeGuid::new(CLASSID_OSINT, (theme_hi<<8)|class_order, (anchor_lo<<8)|airo_bits, facet_primary, basin as u32, i as u32)`
  — `class_order`, `airo_bits`, `facet_primary` come from the already-computed `value` tenant
  (`value[CLASS_ORDER_TENANT]`, `value[FACET_AIRO_ROLE]`, `value[FACET_MILITARY|CIVIC|MLTYPE]`).
- SoA hub mint (`osint_soa_bytes`): same `new_v2` → `new` swap (hub identity = 0).
- Readers: `.family_v2()` → `.family()`, `.identity_v2()` → `.identity()`,
  `.to_hex_v2()` → the V3 hex (confirm whether `NodeGuid: Display` renders the 6 groups, else a
  local `hex()` from `as_bytes()`).
- Basin extraction `(key.family_v2() & 0xFF)` → `(key.family() & 0xFF)` (basin = family low byte).
- Tests (`rows_fill_the_osint_domain_with_v2_basin_tail`, etc.): `family_v2`/`identity_v2` →
  `family`/`identity`; rename away the `v2` test names.

### 2. **MANDATORY** (per `I-LEGACY-API-FEATURE-GATED`): layout-boundary field-isolation tests
The layout reclaims bytes (v2 leaf@10-12 + family-u16@12-14 + identity-u16@14-16 →
V3 family-u24@10-13 + identity-u24@13-16). The iron rule requires a field-isolation matrix
test: mint with each group set, assert every *other* group decodes unchanged. Add it.

### 3. `cockpit/src/OsintGraph.tsx` + `OsintScene3D.tsx` (the JS guid→xyz decoder)
The byte offsets MOVE v2→V3, so the client `position()` decode must change:
- basin: read **byte 10** (V3 family low byte), was the v2 family u16 @12-14.
- identity: read **bytes 13-16** (u24 LE), was v2 identity u16 @14-16.
- heel/theme: bytes 4-6 (unchanged); the new low-byte `is_a` codes (class/role/facet) become
  available to the client for colour/lens if desired.

### 4. Re-bake `crates/cockpit-server/assets/osint_scene.soa`
Run the `#[ignore] bake_osint_soa` test against the on-disk harvest (needs a compiling
cockpit-server). The wire format is unchanged (16-byte guid + class byte); only the guid's
internal layout changes, so old readers break — TS (#3) and the asset (#4) must ship together.

## Blockers / gating (honest)
- **cockpit-server can't compile in this env** (lance-graph-ogar absent) → Rust unverifiable here.
- **`.soa` re-bake needs that build + the harvest data** → can't run here.
- So this lands as a CI/deploy-validated PR; the field-isolation tests (#2) are the in-CI proof.

## Suggested execution order
1. Land the Rust minting + readers + field-isolation tests (#1, #2) — CI compiles + proves the layout.
2. Land the TS decoder (#3) in the same PR (they must agree).
3. Re-bake the asset (#4) on a machine with a working build; commit the new `.soa`.
4. Redeploy — dimensions now both edge-connected *and* address-encoded.
