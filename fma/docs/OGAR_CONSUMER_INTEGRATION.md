# OGAR Consumer Integration — adopting the canonical NodeGuid / guid-shaped representation

> What a consumer does to adopt OGAR's canonical node representation, and the
> new powers it unlocks for data representation and AST. Grounded in shipped
> code: `canonical_node.rs` is the canonical home; q2's `osint-bake` + `cockpit-server`
> already consume the contract; this crate's v3 `converge` mints the canonical
> key dep-free. Speculative items are tagged **CONJECTURE**; everything else is
> a fact about code that exists today.

## 0. The one-sentence claim

A node is `4096 bit = 512 byte = key(16) | edges(16) | value(480)`, and **the
16-byte key IS the address** — `classid · HEEL · HIP · TWIG · family · identity`,
little-endian, self-describing at sight. A consumer adopts OGAR by replacing its
bespoke ids with this key. The payoff: every consumer inherits prefix routing,
key-only scans, zero-copy Lance I/O, and compression-without-losing-addressability
**for free**, because they all become readings of one shared 16-byte key.

Reference impl (the source of truth, all line numbers below are this file):
`/home/user/lance-graph/crates/lance-graph-contract/src/canonical_node.rs`.

---

## 1. The canonical surface consumers adopt

Everything a consumer needs is in `lance_graph_contract::canonical_node`. Five
types, one registry:

### 1.1 `NodeGuid` — the 16-byte key

```text
  0..4   classid   (u32)   8 hex — prefix-routable; default 0x0000_0000
  4..6   HEEL      (u16)   ┐
  6..8   HIP       (u16)   ├ 3 cascade tiers (HHTL path)
  8..10  TWIG      (u16)   ┘
 10..13  family    (u24)   ┐ trailing 6 bytes = basin-local key
 13..16  identity  (u24)   ┘ (a single masked load after the trie binds the prefix)
```

- `NodeGuid::new(classid, heel, hip, twig, family, identity)` — const fn,
  little-endian, panics (incl. const-eval) if `family`/`identity` exceed 24 bits
  (the silent-truncation footgun guard).
- `NodeGuid::local(identity)` — the bootstrap address: classid=0, family=0, only
  identity discriminates.
- Accessors: `.classid() .heel() .hip() .twig() .family() .identity()`,
  `.decode() -> GuidParts` (all six groups in one read), `.local_key() -> u64`
  (trailing 6 bytes — the only discriminator once the prefix is trie-resolved),
  `.read_mode() -> ReadMode`, `.as_bytes() -> &[u8; 16]`.
- `Display` renders the canonical `{:08x}-{:04x}-{:04x}-{:04x}-{:06x}{:06x}`
  (8-4-4-4-12 hex). The dash-groups ARE the semantic delimiters.
- Dispatch guards: `.is_default_class() .is_unbasined() .is_bootstrap_address()`.

### 1.2 `NodeRow` — the 512-byte SoA row

`#[repr(C, align(64))]`, `key: NodeGuid (0..16) | edges: EdgeBlock (16..32) | value: [u8; 480] (32..512)`.
Compile-asserted at 512 B. A `&[NodeRow]` is *already* a row-strided LE column
packet — no serialization needed.

### 1.3 `EdgeBlock` — 12 in-family + 4 out-of-family

`#[repr(C, align(16))]`, one byte per slot. Canonical, **not mandatory**: always
reserved (zeroed when unused), never shrunk. A class opting out of edges is
resolved via `classid → ClassView`, never a stride change. Read flavor is an
*interpretation* selected by `EdgeCodecFlavor` (`CoarseOnly` / `CoarseResidue` /
`Pq32x4`) — none changes `NODE_ROW_STRIDE`.

### 1.4 Value tenants via `ValueSchema` presets

The 480-byte value slab is carved by named `ValueTenant`s (stable, append-only
positions): `Meta · Qualia · MaterializedEdges · Fingerprint · HelixResidue ·
TurbovecResidue · Energy · Plasticity · EntityType`. A class picks a **preset**:

| `ValueSchema` | tenants | bytes |
|---|---|---|
| `Bootstrap` (default) | none — key + edges only | 0 |
| `Cognitive` | Meta, Qualia, Fingerprint, Energy, Plasticity, EntityType | 58 |
| `Compressed` | Fingerprint, HelixResidue, TurbovecResidue, EntityType | 56 |
| `Full` | every tenant | 112 |

Every preset carves *within* the reserved 480 B, so the choice never changes the
512-byte stride. `ValueTenant::value_offset()` / `byte_len()` let a transcode
write into the slab without hardcoding the carve.

### 1.5 `classid → ReadMode` registry

`classid_read_mode(classid) -> ReadMode { value_schema, edge_codec }` — resolved
through one `LazyLock` map (`BUILTIN_READ_MODES`). `NodeGuid::read_mode()` is the
carrier-method form. **Both a consumer transcoding a row AND OGAR minting the same
class read the identical schema** — single-sourced LE interpretation. Any classid
not in the map falls through to `ReadMode::DEFAULT` (the key's own zero-fallback
ladder). NB: `DEFAULT.value_schema` is temporarily `Full` (a 2026-06-15 POC default,
guarded by the test `read_mode_default_is_full_poc`); it flips back to `Bootstrap`
when the POC ends.

### 1.6 The lever: the zero-fallback ladder (incremental adoption)

```
classid == 0x0000_0000 → default class, no prefix routing   (dormant)
family  == 0x00_0000    → default basin, no neighborhood grouping (dormant)
⇒ while both are zero, identity (24 bit) ALONE discriminates — the bootstrap address.
```

This is what makes adoption **incremental and reversible**. A consumer can switch
to `NodeGuid::local(id)` today, keep its flat id-space (16.7M identities per
default basin), and ship. Later it mints a non-zero `classid` (to turn on prefix
routing) and/or a non-zero `family` (to turn on basins) — **with ZERO
`ENVELOPE_LAYOUT_VERSION` change**, because `classid` (4 B) and `family` (3 B)
keep fixed offsets that were reserved, never reclaimed. There is no flag day: a
zero tier means "not consulted", never "compacted away" (RESERVE, DON'T RECLAIM).

---

## 2. Per-consumer upgrade path

### 2.1 lance-graph (canonical home — already there)

`canonical_node.rs` IS the contract. `NodeRowPacket<'a>` implements `SoaEnvelope`
with a zero-copy `as_le_bytes()` (the pointer of the byte view equals the slice's
pointer — asserted in tests). Lance's columnar I/O writes those LE bytes directly
from the in-place backing store. Nothing to adopt; this is the surface everyone
else imports.

What's left here: extend `BUILTIN_READ_MODES` as real classes mint (today it
holds only the default), and flip the POC `Full` default back to `Bootstrap` (one
revert, two sites — `ReadMode::DEFAULT` + `ClassView::value_schema`).

### 2.2 q2 `osint-bake` / `cockpit-server` (already consume the contract)

- **`crates/osint-bake/src/bin/fma.rs`** mints node keys via
  `NodeGuid::new_v2(CLASSID_FMA, HEEL, HIP, TWIG, LEAF, family, identity)` — a
  **7-group** layout (it threads a distinct `LEAF` tier between TWIG and family,
  and reads it back with `.leaf()` / `.family_v2()`). It emits the cockpit's
  `OSO1` wire buffer (`node.key.as_bytes()` → 16 bytes per node) and the `/fma`
  dual-membership proof.

  **Blocker, file as an issue (do NOT silently reimplement — §5):**
  `NodeGuid::new_v2`, `.leaf()`, and `.family_v2()` **do not exist** in the
  current `canonical_node.rs` (which ships the 6-group
  `NodeGuid::new(classid, heel, hip, twig, family, identity)` with `.heel/.hip/.twig`
  but no `.leaf`/`.family_v2`/`.new_v2`). q2 pins this exact path
  (`../lance-graph/crates/lance-graph-contract`, root `Cargo.toml:214` &
  `:377`), so `osint-bake` is written against an **in-flight v2 NodeGuid API
  that has not landed in `canonical_node.rs`**. This is precisely the
  `I-LEGACY-API-FEATURE-GATED` situation: two readings of "the canonical key"
  (6-group locked vs 7-group LEAF-bearing) under one name. Resolution is upstream
  in lance-graph — either land the 7-group `new_v2` (LEAF reclaims part of the
  TWIG/family span, version-gated) or migrate `fma.rs` onto the 6-group `new`.
  Until then, `osint-bake`'s `fma` bin is the canary for that decision; treat the
  divergence as the gating item, not a thing to route around.

- **`crates/cockpit-server/src/graph_engine.rs`** already uses
  `lance_graph_contract::exploration::NarsTruth` (frequency, confidence) for edge
  truth and `lance_graph_contract::nars::InferenceType` for inference labels,
  bridging NARS deduction to `lance_graph_planner::nars::truth::TruthValue::deduction`.
  Truth/inference types are canonical here. What's left: the snapshot's
  `GraphNode { id: String, ... }` is still a stringly-typed property-graph node —
  the natural next step is to carry a `NodeGuid` per node (its id) and route the
  cockpit's render/select by `classid` prefix (the same prefix the `/fma`
  skeleton button uses), instead of by string id.

### 2.3 fma (v3 `converge` already mints the canonical key, dep-free)

`/home/user/q2/fma/src/bin/converge.rs` is a **standalone crate** (self-isolated
`[workspace]`, only dep `png`) that deliberately does NOT link the contract. It
reimplements the canonical 16-byte layout byte-for-byte:

- `node_guid_bytes(classid, heel, hip, twig, family, identity)` builds the exact
  `to_le_bytes` layout of `NodeGuid::new` (header comment asserts byte-identity to
  `lance_graph_contract::canonical_node::NodeGuid::new`, OGAR canon 2026-06-13).
- `guid_display` renders the same `8-4-4-4-12` hex as `NodeGuid`'s `Display`.
- It realizes the **two ontological axes in one key**: each 8:8 HHTL tier is
  `(place : tissue)` — high byte = PLACE (Morton spatial cell for skeletal nodes
  classid `0x0A02`, or `part_of` sibling-rank for soft tissue `0x0A01`); low byte
  = TISSUE (`is_a` taxonomy sibling-rank). The high-byte chain prefix-routes the
  body, the low-byte chain prefix-routes the type taxonomy — **both hierarchies,
  one key**. `family` (u24) is the `(part_of:is_a)` level-3 ontological basin.
  `connected_to` lands in the EdgeBlock shape: `part_of` siblings = in-family
  (≤12), `is_a` parent = out-of-family (≤4).
- Identity is the golden-stride mint (`GOLDEN_RATIO × EULER_GAMMA`), with a
  linear-probe collision guard against a `HashSet<[u8;16]>`.

This is the model for any consumer that must stay free of the lance/datafusion
closure: **own the 16-byte layout as bytes, keep it byte-identical to `NodeGuid`,
and you are on the canon without taking the dependency.** When fma later wants the
typed surface, it swaps `node_guid_bytes(...)` → `NodeGuid::new(...).as_bytes()`
with no on-disk change (the bytes are already canonical).

### 2.4 Generic consumer — the 5-step recipe

Applies to medcare-rs, woa-rs, arcgis, a `ruff`/`ty` AST, any graph/record store:

1. **Replace bespoke ids with `NodeGuid`.** Start at the bottom of the ladder:
   `NodeGuid::local(next_identity)` (classid=0, family=0). Your old flat id-space
   keeps working; nothing else changes yet. (Or, if you can't take the dep, mint
   the 16 bytes directly — see §2.3 fma.)
2. **Map your two ontological axes onto the key.** OGAR's recurring shape is two
   orthogonal hierarchies:
   - **subsumption / `is_a` / kind-of** → the **low-byte chain** of the HHTL tiers
     (and/or `classid` for the coarse class). "What it is."
   - **parthood / `part_of` / contained-in** → the **high-byte chain** of the HHTL
     tiers (and `family` for the basin). "Where it sits."
   Each tier byte is a stable **sibling-rank** under its parent (1-based;
   `converge.rs` and `osint-bake/fma.rs` both do exactly this), so the cascade IS
   the path — no edge lookup to place a node.
3. **Lateral relations → `EdgeBlock`.** The non-tree edges (your "connects-to",
   "references", "calls") go in the 12 in-family + 4 out-of-family byte slots —
   each byte a palette/centroid index or a local adjacency handle. Reserve the
   block even if you don't fill it.
4. **Payload → value tenants via a `ValueSchema` preset.** Pick `Bootstrap` (no
   payload), `Cognitive` (hot lifecycle columns), `Compressed` (codec residues),
   or `Full`. Write fields at `ValueTenant::value_offset()`. Need a tenant that
   doesn't exist? That's a Core gap — extend the Core (§5), don't bolt state onto
   an adapter.
5. **Inherit the powers for free.** Once your data wears the key, key-only routing
   (§3) and SoA scans (§3) work without any further code — they are properties of
   the 16-byte key, not of your consumer.

The recipe is monotonic with the ladder: you can stop after step 1 and still
ship; steps 2–4 light up routing/basins/payload incrementally; step 5 costs
nothing.

---

## 3. New superpowers for DATA representation

### 3.1 Prefix-routed subtree selection — O(1) candidate filter

The key prerenders nodes with **zero value decode**: classid → the class template,
HEEL/HIP/TWIG → the cascade position, family → the neighborhood, identity → the
instance. "Draw the skeleton subtree" is `classid == 0x0A02` — a 4-byte compare,
no value touched. Because the codebooks are built as a 4-level 4-ary hierarchy
(256 = 4⁴), a tier byte's nibbles are a centroid's ancestry, so `is_ancestor_of`
is centroid-tree containment and a prefix match is a reachability test that
replaces a graph walk. **CONJECTURE** (hierarchical-4⁴-vs-flat-256 fidelity is a
named-but-unrun test in the OGAR canon): treat the centroid-tree containment claim
as conjecture until that probe is green; the *byte-prefix* routing itself is
shipped and exact.

### 3.2 Key-only SoA scan — ~30× less memory, measured

`/home/user/q2/fma/src/bin/soa_scan.rs` lays out `NodeRow` columnar — a contiguous
16-byte **key column** and a contiguous 480-byte **value column** — and compares:

- **key-only** (prefix-route / render-select): stream 16 B/row, compare classid.
- **value** (decode the slab): stream 480 B/row, sum the bytes.

The key-only scan touches **16 B/row vs 480 B/row → ~30× less memory**, and stays
flat as N grows (measured at 64K / 256K / 1M synthetic rows, best-of-5). The doc's
own summary line: *"key-only touches 16 B/row vs 480 B/row (30× less); routing /
render-select needs NO value decode."* This is the same prefix routing the
`/fma`-body skeleton button does, measured at scale.

> Note on the "~90×" figure: the **memory-traffic** ratio is 480/16 = 30×. A
> larger wall-clock speedup (the work-per-byte gap: a 4-byte compare vs a
> 480-iteration sum) is what `soa_scan` prints as the `speedup` column; cite the
> 30× memory ratio as the load-bearing number and let the binary report the
> wall-clock multiple for the host it runs on.

### 3.3 Zero-copy Lance columnar I/O

`NodeRowPacket` implements `SoaEnvelope`; `as_le_bytes()` is the slice reinterpreted
as `&[u8]` (zero-copy, pointer-identical — test `single_row_packet_verifies_and_byte_view_is_zero_copy`).
Lance writes those LE bytes from the in-place backing store. Nothing serializes
between "row in memory" and "row in Lance" — the envelope is zero-copy from
creation to tombstone.

### 3.4 Compression without losing addressability

The key is **never compressed**; the value can be anything Lance wants — columnar
encodings, dictionary, PQ. So the store keeps a transparent view and a stable
address regardless of how aggressively the 480-byte value is squeezed. Compression
never costs addressability, because addressing lives entirely in the 16 bytes that
are never touched by the value codec. The `EdgeCodecFlavor` / `ValueSchema` choices
are *interpretations of reserved bytes* — `is_layout_preserving()` is `const true`
for all of them, so a compression/codec change is never an `ENVELOPE_LAYOUT_VERSION`
bump.

---

## 4. New superpowers for AST (core-first transcode)

The same key that addresses data addresses **code**. An AST node becomes a
canonical GUID, and a program becomes a `&[NodeRow]`.

### 4.1 Guid-shaped AST

Each AST node = one `NodeGuid`. `classid` = the node kind (a function, a class, a
call, an `if`); HEEL/HIP/TWIG = its position in the program's containment cascade
(module → class → method → block); family = its basin; identity = the node's stable
id. The program is a columnar `&[NodeRow]` — the same SoA the data path uses.

### 4.2 classid-keyed thin adapters (the core-first inversion)

Per the OGAR Core-First doctrine: a generated layer (codegen'd Rust, AST adapters)
is only ever as clean as the Core it targets, so the **OGAR Core is shaped first,
deliberately** and adapters are emitted *thin and classid-keyed, assuming the
Core*:

- **identity = `classid`** (the node kind routes to its template),
- **state = SoA value tenants** (the node's payload, never carried by the adapter),
- **relations = `EdgeBlock`** (lateral edges between AST nodes),
- **composition / inheritance = `classid → ClassView`** (method resolution),
- **invocation = `UnifiedStep`** (the canonical bridge).

A C++→Rust transcode (e.g. Tesseract) emits one thin adapter per leaf method, each
keyed by `classid`. **Never** build a parallel object model; **never** let an
adapter carry its own state (a Core gap → extend the Core, §5); **never** force an
intrusive/stateful method into the adapter mold (route it to a raw-pointer
hand-port — the Frankenstein-flattening guard). This holds for mechanical /
data-shaped leaf methods only and is **CONJECTURE** until the
`PROBE-OGAR-ADAPTER-UNICHARSET` byte-parity probe is green.

### 4.3 The SPO harvest IS the ClassView method-resolution manifest

A `ruff_cpp_spo`-style SPO harvest (`has_function` / `inherits_from` /
`virtually_overrides`) over the C++ source is not a separate artifact from the
codegen — it is the **ClassView method-resolution manifest**. `classid → ClassView`
answers "which method body runs for this node" by reading the harvested
`inherits_from` / `virtually_overrides` chain. The harvest and the codegen are two
halves of one system.

### 4.4 Prefix-routed program structure + key-only code scans

Everything §3 gives data, AST gets too:

- **Subsumption / parthood as prefix matches.** "All methods of class C" = a
  HEEL/HIP prefix scan; "is node A inside scope B" = a prefix containment test —
  no tree walk.
- **Key-only scans over code.** "Find every virtual override" / "every node of
  kind K" = a 16-byte key-column scan (§3.2), 30× cheaper than materializing each
  node's payload. Refactoring queries, call-graph slices, and visibility checks
  become column scans over keys.
- **Method resolution via `classid → ClassView`** is an O(1) registry read, not a
  graph traversal.

---

## 5. Migration safety

The whole design is engineered so adoption never needs a flag day.

- **Zero-fallback ladder ⇒ start dormant.** `NodeGuid::local(id)` (classid=0,
  family=0) is a drop-in for a flat id-space. Routing and basins stay *off* until
  you mint non-zero tiers. Every consumer can ship at the bottom of the ladder and
  climb later.
- **RESERVE, DON'T RECLAIM ⇒ no layout churn.** `classid` (4 B) and `family` (3 B)
  hold fixed offsets that were reserved from day one. Minting a non-zero value
  later wakes routing/basin binding with **ZERO `ENVELOPE_LAYOUT_VERSION` bump**.
  Likewise every `ValueSchema` preset and `EdgeCodecFlavor` is layout-preserving
  (`is_layout_preserving() == true` by const assertion) — a payload/codec change
  is never a stride change.
- **`I-LEGACY-API-FEATURE-GATED` ⇒ no silent dual-semantics.** Any v1→v2 accessor
  (e.g. the 6-group `NodeGuid::new` vs the in-flight 7-group `new_v2` that
  `osint-bake/fma.rs` calls — §2.2) MUST transparently route through the canonical
  mapping OR be feature-gated to a documented no-op with a migration pointer. The
  same function name must never silently produce different semantics under
  different features, and a layout reclaim must be paired with a version gate on
  any serialization path. Field-isolation matrix tests are mandatory whenever a
  layout reclaims previously-used bits.
- **Approval gate for upstream Core changes.** If a consumer needs a `ValueTenant`,
  a `ClassView` capability, or a `NodeGuid` shape that doesn't exist (e.g. the
  LEAF-bearing `new_v2`), **file an issue against lance-graph and surface it** —
  do NOT reimplement it consumer-side. A Core gap is resolved by extending the
  deliberate Core, never by hacking an adapter or forking the layout. The
  `osint-bake` `new_v2` divergence in §2.2 is the live worked example of this gate.

### Adoption checklist (per consumer)

- [ ] Import `lance_graph_contract::canonical_node` (or commit to byte-identical
      local minting, fma-style — §2.3).
- [ ] Replace primary ids with `NodeGuid` (`::local(id)` to start; classid=0,
      family=0).
- [ ] Map subsumption → low-byte/`classid`, parthood → high-byte/`family` (§2.4 step 2).
- [ ] Move lateral relations into `EdgeBlock` (12+4).
- [ ] Choose a `ValueSchema` preset; write payload at `ValueTenant::value_offset()`.
- [ ] Lay rows out columnar (`Vec<NodeRow>` or split key/value columns) → inherit
      key-only scan + zero-copy Lance I/O.
- [ ] File an issue for any missing tenant / ClassView capability / NodeGuid shape
      instead of reimplementing it (the §2.2 `new_v2` gate).
