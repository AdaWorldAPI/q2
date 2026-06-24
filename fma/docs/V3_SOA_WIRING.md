# V3 SoA Wiring — `(part_of:is_a)` canonical-NodeGuid addressing

> What this documents: the **v3** FMA addressing that landed with `converge.rs` +
> `graph.rs` + `soa_scan.rs` (q2 PR #59, merged). It wires the OGAR canonical
> 16-byte `NodeGuid` / 512-byte `NodeRow` SoA from
> `lance_graph_contract::canonical_node` into a real, rendered, measured pipeline,
> and packs **two hierarchies into one key**: `part_of` (where, mereology) in the
> high byte of each HHTL tier, `is_a` (what, taxonomy) in the low byte.
>
> Everything below is grounded in source. Byte offsets come from
> `canonical_node.rs`; mint logic from `src/bin/converge.rs`; prefix-render from
> `src/bin/graph.rs`; measured throughput from `src/bin/soa_scan.rs`. Speculative
> extrapolations are marked **CONJECTURE**.

---

## 1. The canonical `NodeGuid` + `NodeRow` SoA

### 1.1 The 16-byte key (byte offsets are the contract)

`lance_graph_contract::canonical_node::NodeGuid` is `#[repr(C, align(16))]`
around `[u8; 16]`, little-endian throughout, with a `const _: () =
assert!(size_of::<NodeGuid>() == 16)` lock. The six canonical groups:

```text
  byte range   field      width   role
  ──────────   ─────      ─────   ────
  0..4         classid    u32     prefix-routable class id; default 0x0000_0000
  4..6         HEEL       u16     ┐
  6..8         HIP        u16     ├ 3 HHTL cascade tiers (the path)
  8..10        TWIG       u16     ┘
  10..13       family     u24     ┐ trailing 6 bytes = basin-local key
  13..16       identity   u24     ┘ (one masked load once the prefix is bound)
```

`Display` renders the canonical self-describing `8-4-4-4-12` hex form, dash-groups
== semantic delimiters (OGAR P0):

```text
classid  -HEEL-HIP -TWIG-family·identity
deadbeef-1111-2222-3333-0000ab0000cd     (NodeGuid::new(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0xAB, 0xCD))
```

The trailing 6 bytes `[10..16)` (`family ++ identity`) are contiguous and form
`local_key()` — a single masked `u64` load that is the **only** discriminator once
an HHTL radix walk has bound `classid·HEEL·HIP·TWIG`. `decode()` reads the whole
key in one shot into `GuidParts { classid, heel, hip, twig, family, identity }` —
the "read the GUID as a GUID" surface, six fields in canon print order, nothing
invented or dropped.

### 1.2 The 512-byte row

`NodeRow` is `#[repr(C, align(64))]`, locked at 512 bytes
(`assert!(size_of::<NodeRow>() == 512)`):

```text
  byte range    field    bytes   what
  ──────────    ─────    ─────   ────
  0..16         key      16      NodeGuid (the address)
  16..32        edges    16      EdgeBlock: 12 in-family + 4 out-of-family
  32..512       value    480     deferred value slab (Lance-compressible)
```

`NODE_ROW_COLUMNS` describes the three top-level slots as columns (key @ 0 / 16 B,
edges @ 16 / 16 B, value @ 32 / 480 B; Σ = 512 = `NODE_ROW_STRIDE`), and
`NodeRowPacket` is a **zero-copy** `SoaEnvelope` over `&[NodeRow]`: because
`NodeRow` is `repr(C)` with the locked layout, a `&[NodeRow]` *is already* a
row-strided LE byte packet at stride 512. `as_le_bytes()` is a `from_raw_parts`
re-view with no allocation and no translation — Lance's columnar I/O reads the
in-place backing store directly. (The `single_row_packet_verifies_and_byte_view_is_zero_copy`
test asserts `as_le_bytes().as_ptr() == rows.as_ptr()`.)

The value slab is itself class-carved but not surfaced as its own envelope column:
`ValueTenant` (Meta / Qualia / MaterializedEdges / Fingerprint / HelixResidue /
TurbovecResidue / Energy / Plasticity / EntityType) gives a stable, contiguous,
compile-asserted byte carve; `ValueSchema` presets (Bootstrap / Cognitive /
Compressed / Full) pick which tenants materialize. Every preset and every
`EdgeCodecFlavor` is **layout-preserving** — `is_layout_preserving()` is `const
true` for all of them, so adopting a schema never bumps `ENVELOPE_LAYOUT_VERSION`.
This is the canon's value-side analog of "which XSD parses this document":
`classid → ReadMode { value_schema, edge_codec }` via the `BUILTIN_READ_MODES`
`LazyLock` registry, with any unconfigured classid falling through to
`ReadMode::DEFAULT`.

### 1.3 The zero-fallback ladder

Monotonic — a zero tier means "not consulted", **never** "compacted away"
("RESERVE, DON'T RECLAIM"):

- `classid == 0x0000_0000` → default class, no prefix routing (dormant).
- `family  == 0x00_0000`   → default basin, no neighborhood grouping (dormant).
- ⇒ while both are zero, `identity` (24 bits, 16.7 M slots) **alone** discriminates
  — the bootstrap address. `is_bootstrap_address()` guards exactly this state, and
  `debug_assert_identity_unique()` enforces identity uniqueness while the family is
  still a no-op. Minting a non-zero `family` later *wakes* basin binding with zero
  layout change (the `classid(4B)` and `family(3B)` offsets are fixed).

`NodeGuid::new` `assert!`s that `family <= 0x00FF_FFFF` and `identity <=
0x00FF_FFFF` — the silent-truncation footgun (two distinct u32 inputs collapsing to
the same stored 24-bit key) is a panic, including at const-eval.

---

## 2. The `(part_of:is_a)` tile = `(place:tissue)`

The v3 insight (`converge.rs`): **each HHTL tier is an 8:8 split**, packing two
orthogonal hierarchies into one `u16`.

```text
  tier (u16):   ┌────────── high byte ──────────┬────────── low byte ──────────┐
                │  PLACE  — WHERE it sits         │  TISSUE — WHAT it is          │
                │  part_of (mereology)            │  is_a (taxonomy)              │
                └─────────────────────────────────┴───────────────────────────────┘
  HEEL = (place₀ : tissue₀)   HIP = (place₁ : tissue₁)   TWIG = (place₂ : tissue₂)
```

The packer is literally `fn tier(hi: u8, lo: u8) -> u16 { ((hi as u16) << 8) | lo
as u16 }`. The **high-byte chain** (HEEL.hi → HIP.hi → TWIG.hi) prefix-routes the
*body*; the **low-byte chain** (HEEL.lo → HIP.lo → TWIG.lo) prefix-routes the *type
taxonomy*. Both hierarchies, one key, cascading HEEL → HIP → TWIG (coarse → fine).
`family` (u24 @ `[10..13)`) carries the level-3 basin as the same split: `((po_rank3
as u32) << 8) | ia_rank3` — `(part_of:is_a)` one level deeper.

### 2.1 classid dispatches the `place` mode (OGAR `HhtlMode`)

`converge.rs` classifies each FMA node into `ConceptDomain::Anatomy` (`0x0A`):

- `classid 0x0000_0A02` — **skeleton** (bone / cartilage / vertebra / rib / femur /
  skull, via `is_skeletal()`).
- `classid 0x0000_0A01` — **soft tissue** (everything else).

The `place` bytes are then dispatched on classid:

- **Located** (skeleton `0x0A02` *with a centroid*): `place` is the **Morton
  spatial cell** of the bone's mesh centroid. `morton3()` interleaves the three
  quantized 8-bit axes into a 24-bit Z-order code; the three octets of that code
  become the high bytes of HEEL/HIP/TWIG respectively (`(m>>16)&0xFF`,
  `(m>>8)&0xFF`, `m&0xFF`). The exact anatomical anchor **is** the key — spatially
  near bones share leading high-byte groups.
- **Cascade** (soft tissue `0x0A01`, or any node lacking a centroid): `place` is the
  **`part_of` sibling-rank** at each level (`lvl(&po_ranks, 0..2)`) — ontological
  place, inheriting position from the `part_of` basin's skeleton anchor.

`tissue` (the low byte) is the **`is_a` sibling-rank** at each level
(`lvl(&ia_ranks, 0..2)`) in **both** modes. Sibling-rank comes from
`Tree::rank_of` — the IRI-sorted child index under the node's parent (stable,
`(k.min(254)) + 1`, 0 = root), so siblings get adjacent ranks and the tier prefixes
stay monotone with the hierarchy.

`identity` (@ `[13..16)`) is the **golden-stride mint**: `golden_id24(k)` from
`GOLDEN_RATIO × EULER_GAMMA` (stride-4 / offset-20, the helix `CurveRuler`
generator), with a linear-probe `(identity + 1) & 0x00FF_FFFF` on the rare
collision against the `used` set.

### 2.2 Worked examples (real `converge` output)

Located skeleton — **thoracic vertebrae T9/T10/T11**, `classid 0x0A02`, mode
Located:

```text
  FMA10014  00000a02-ce01-fe02-7b02-…   ↔ T10,T11,T12,…   shared Morton HEEL ce = same spatial octant
  FMA10059  00000a02-ce01-d602-eb02-…   ↔ T9,T10,T12,…    HIP/TWIG descend as the centroid descends (z 1164→1107)
```

Both vertebrae share `classid 0x0A02` and the **same HEEL high byte `ce`** — they
sit in the same spatial octant of the body, so their addresses agree on the coarsest
spatial group. HIP and TWIG diverge as the centroid descends. The low bytes (`01`,
`02`) are the `is_a` ranks (the bone taxonomy position).

Cascade soft tissue — **aortic segments**, `classid 0x0A01`, mode Cascade:

```text
  FMA3736  ascending aorta  00000a01-0901-0702-0e02-…   ↔ arch, descending   part_of siblings = the connected segments
```

The ascending aorta, the arch, and the descending aorta are `part_of` siblings of
the same aorta; they share the leading `part_of` high-byte groups (`09`, `07`, `0e`
…) and are connected to each other (§3). The `connected_to` column lists exactly the
sibling segments that physically continue the vessel.

The upshot, made visible in `graph.rs`: a single key prefix `00000a01-0901-0702`
selects "one `part_of`/`is_a` subtree" of triangles; `00000a02` selects the whole
skeleton. **The address is the query.**

---

## 3. `connected_to` = the EdgeBlock

`converge.rs` fills the canonical 16-byte `EdgeBlock` (12 in-family + 4
out-of-family) with the **anatomical adjacency graph**:

- **in-family** (≤ 12 slots) = **`part_of` siblings** (`Tree::siblings` — the
  co-parts under the same `part_of` parent). For the aorta these are the aortic
  segments; for the heart, the chambers — *the sub-parts that physically connect*.
  v3 emits one `in_family:part_of_sibling` edge row per sibling (capped at 12, the
  in-family slot count).
- **out-of-family** (≤ 4 slots) = the **`is_a` parent** (the type link), emitted as
  `out_family:is_a_parent`.

This is the canon's "a class opting out of edges is resolved via `classid →
ClassView`, never a stride change" used positively: anatomy populates the reserved
block with mereological adjacency, so "70K nodes connecting via relationships" is
carried in the same 512-byte row as the address — the relationships **are** the
key's neighbors. The edge byte (one per slot) is a palette/centroid index under the
default `EdgeCodecFlavor::CoarseOnly`; the manifest's human-readable `connected_to`
column carries the sibling FMA ids for inspection.

---

## 4. The SoA superpower: key-only scan, zero value decode

This is the OGAR canon's "**the key prerenders nodes with zero value decode**",
measured. `soa_scan.rs` lays the 512-byte `NodeRow` out **columnar** (struct-of-
arrays): a contiguous 16-byte key column (`Vec<[u8; 16]>`) and a contiguous 480-byte
value column (`Vec<[u8; 480]>`), separate allocations. Two scans over 1 M synthetic
rows (seeded by the FMA distribution: ~25 % skeleton classid, the rest soft tissue):

- **key-only (prefix-route / render-select):** read only the key column; decode
  `classid` from bytes `[0..4)`; count the skeleton subtree (`== 0x0000_0A02`). This
  is exactly the work the `/fma-body` skeleton button and `graph 00000a02` do.
- **value (decode the slab):** read the whole value column; sum all 480 bytes per
  row — the work a value/tenant materialization does.

Measured (real numbers from `soa_scan`, the q2 PR #59 run):

| scan                       | throughput        | bandwidth | scales with N |
| -------------------------- | ----------------- | --------- | ------------- |
| key-only (route)           | **~1.5 G rows/s** | ~24 GB/s  | **flat**      |
| value (decode 480 B slab)  | ~17 M rows/s      | ~8 GB/s   | flat          |
| **speedup**                | **89–130×**       | —         |               |

(The README summarizes this as "~90× at 1M".) The key-only scan touches **16 B/row
vs 480 B/row — 30× less memory** — and the realized speedup (89–130×) exceeds the
raw byte ratio because the 16-byte key column is cache-resident and streams at near
memory bandwidth while the 480-byte value column is RAM-bound. Crucially the ratio
is **flat as N grows** (64 K → 256 K → 1 M): routing/render-select is a property of
the key column's size, not the dataset's.

Why this falls out of the layout, not a trick:

- The discriminator a router needs — `classid` (and HEEL/HIP/TWIG for finer routing)
  — lives in the first 16 bytes, never in the value.
- Lance is free to compress the 480-byte value arbitrarily (columnar, dictionary,
  PQ); the key is never compressed and never needs the value decoded to be useful —
  **compression never costs addressability** (canon §"the GUID is the key of
  key-value").
- A prefix match on the key column is a `starts_with` over 16-byte rows; the
  zero-fallback ladder means even an all-default node is addressable by `identity`
  alone.

`graph.rs` is the same routing at render time, not at scan time: it reads the
converged manifest, and a `sel` that is all-hex-or-dash is treated as a GUID prefix —
`guid.starts_with(&sel)` selects which meshes' triangles to rasterize. `graph …
00000a02` renders ~922 K skeleton triangles; `graph all` / `tissues` / `vessel`
render the whole body / inner tissues / vascular tree. The address selects the
geometry; the prefix is the query — and it never decodes a value slab to decide.

---

## 5. Implications for GUID-shaped ASTs / rails-shaped programs

The v3 wiring demonstrates a general substrate property on anatomy; the same
machinery applies to **code**. An AST node and an anatomical part are both
"something with a parent (`part_of` / enclosing scope), a type (`is_a` / node kind),
siblings (co-parts / sibling statements), and an identity". The canonical `NodeGuid`
is anatomy-agnostic — `converge.rs` is one *consumer* of a contract that says nothing
about bones. The OGAR canon (`core-first-transcode-doctrine`) already frames code
this way: identity = `classid`, state = SoA value tenants, relations = `EdgeBlock`,
composition/inheritance = `classid → ClassView`, invocation = `UnifiedStep`.

### 5.1 Each AST node addressed by a canonical GUID

Grounded in the canon's stated structure: a transcoded program's AST node becomes a
`NodeRow` whose `classid` is the node kind (the C++/Tesseract `classid` the OGAR
`ruff_cpp_spo` harvest already assigns), whose HHTL tiers are the node's position in
the program tree, whose `family`/`identity` discriminate within a basin, and whose
`EdgeBlock` carries its structural neighbors. The OGAR canon states this directly:
"the key prerenders nodes — in any way — with zero value decode … a
renderer/router/planner can lay out, group, route, and skeleton-render nodes from
keys alone". An AST is exactly such a thing to lay out and route.

### 5.2 The `(place:tissue)` tile maps to `(scope:kind)` — **CONJECTURE**

**CONJECTURE.** The v3 8:8 tier — `(part_of:is_a)` = `(where:what)` — has a direct
code analog: `(enclosing-scope-rank : node-kind-rank)`. The high-byte chain would
prefix-route *structural containment* (module → class → function → block, the
program's `part_of`), the low-byte chain would prefix-route *syntactic kind* (the
`is_a` of `Expr`/`Stmt`/`Decl`/…). Two program hierarchies in one key, exactly as
anatomy packs mereology + taxonomy. This is a transfer of the *mechanism* v3 ships,
not something v3 itself implements for code; the FMA pipeline is the existence proof
that the tile carries two independent prefix-routable hierarchies losslessly.

### 5.3 Subsumption / parthood become prefix matches

Grounded: in v3, "is this part inside the aorta subtree?" and "is this bone in that
spatial octant?" are both `guid.starts_with(prefix)` (the `graph.rs` selector). The
same operation answers, for code, "is this node inside that function body?" (a
`part_of`/scope prefix) and "is this an expression?" (an `is_a`/kind prefix) — **if**
the AST is minted with the §5.2 tile (CONJECTURE). The canon's hierarchical-prefix
rule ("`is_ancestor_of` = centroid-tree containment", longest-prefix-wins) is the
general statement; v3 is the anatomy instance.

### 5.4 Key-only scans over code

Grounded by §4's measurement (the substrate is content-blind): a 1 M-node program
AST laid out as a `NodeRow` SoA would route the same way the FMA skeleton does —
"select every `FunctionDecl`" is a `classid`-prefix scan over the 16-byte key column
at ~1.5 G rows/s, never decoding the value slab (which would hold the node's
spans/attributes/fingerprint). "Find every node under this module" is an HHTL-prefix
scan. The flat-as-N property means whole-program structural queries stay
cache-resident regardless of program size. (CONJECTURE that real programs hit the
same constant; the *layout* guarantee — key column is 16 B/row independent of value
size — is not a conjecture.)

### 5.5 OGAR Active-Record / "rails-shaped" programs off the guid-addressed AST

**CONJECTURE (framing), grounded in the OGAR canon.** OGAR = *Open Graph of Active
Record*. A "rails-shaped" program is one whose objects are rows in this key-value
store keyed by the canonical GUID: identity is the key, state lives in the value
tenants, relations are the `EdgeBlock`, and method resolution is `classid →
ClassView` (the canon's stated mapping). The v3 anatomy graph is structurally that
object graph — `connected_to` is the relation column, `tissue`/`place` are
addressable attributes, and the renderer is a *consumer* that reads only what it
needs from the key. An Active-Record program over a guid-addressed AST would behave
the same: a "find" is a prefix scan (§5.4), a "belongs_to"/"has_many" traversal is an
`EdgeBlock` read (§3), a "type-of" dispatch is a `classid → ClassView` lookup. The
load-bearing claim is the canon's: AGI/program behavior is the runtime behavior of
the SoA under dispatch, *not* a new layer wrapped around it.

### 5.6 classid-keyed adapters (core-first transcode)

Grounded in the OGAR doctrine: the transcode of a C++ method becomes a **thin,
classid-keyed adapter that ASSUMES the Core** — it reads inputs from the value
tenants, writes outputs back to value tenants, defers composition to `ClassView`, and
carries **no state of its own**. v3 is a small validation of the Core-first stance:
`converge.rs` did not invent a parallel node model — it emits bytes *byte-identical*
to `NodeGuid::new` (the header asserts this), treating the canonical node as the Core
and the anatomy mint as an adapter over it. The doctrine's guard applies unchanged to
code: a method that needs state the value tenants can't carry, or a dispatch the
`ClassView` can't express, is a **Core gap → extend the Core deliberately**, never
hack the adapter; intrusive/stateful methods route to hand-port. (Per the canon, this
holds for mechanical/data-shaped leaf methods and remains CONJECTURE until
`PROBE-OGAR-ADAPTER-UNICHARSET` runs byte-parity green.)

---

## Source map

| Claim                                            | Source                                                            |
| ------------------------------------------------ | ----------------------------------------------------------------- |
| 16 B key layout, offsets, zero-fallback ladder   | `lance-graph/crates/lance-graph-contract/src/canonical_node.rs`    |
| 512 B `NodeRow`, columns, zero-copy envelope     | same — `NodeRow`, `NODE_ROW_COLUMNS`, `NodeRowPacket`             |
| value tenants / schemas / `classid → ReadMode`   | same — `ValueTenant`, `ValueSchema`, `classid_read_mode`         |
| `(place:tissue)` 8:8 tier, Located/Cascade, mint | `q2/fma/src/bin/converge.rs`                                       |
| `connected_to` EdgeBlock from `part_of` siblings | `q2/fma/src/bin/converge.rs` (`siblings`, edge emit)              |
| prefix-routed render (`graph 00000a02`)          | `q2/fma/src/bin/graph.rs`                                          |
| key-only vs value scan, 89–130×, flat            | `q2/fma/src/bin/soa_scan.rs`                                       |
| three coexisting v1/v2/v3 addressings            | `q2/fma/README.md`                                                 |
| GUID-is-key, key-prerenders, classid adapters    | `OGAR/CLAUDE.md`, `core-first-transcode-doctrine`                 |
