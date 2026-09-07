# FMA bake v4 — findings, caveats, and weighted options

> Status: PROPOSAL. Nothing here is scheduled. Every claim carries the command
> or file:line that produced it; unmeasured items are labelled CONJECTURE.
> Written against q2 @ HEAD (shallow clone — see C-7 on what that limits).

## 0. Why a v4 at all

The 20260629c bake is correct as rendered geometry and is serving. What this
plan addresses is the **address layer**: three producers mint three different
identities for the same concept, one of them from iteration order; the source
part_of relation is a DAG while the cascade consumes a tree; and an axis-edge
table that upstream ships (laterality) is not consumed at all.

None of this is a rendering bug. All of it is an addressing bug, which is why
it has been invisible to a green build and a correct-looking body.

---

## 1. Findings (measured)

### F-1 — Identity is minted three ways, one of them order-dependent  [BLOCKING]

| producer | identity expression | content-stable? |
|---|---|---|
| `fma/src/bin/guid.rs:119` | `golden_id(k)`, then `+1` probe on collision (`:120-123`) | **NO** — `k` is enumeration index |
| `fma/src/bin/anchor.rs:111` | `fnv16(&path.join("/"))` | yes; **no** collision probe |
| `crates/osint-bake/src/bin/body.rs:129` | `row` (JSON node index) | **NO** — JSON order |

`golden_id` (`guid.rs:44-47`) is `((20 + 4k)·φγ mod 1)·65536`. The γ+φ
low-discrepancy walk is the right generator; feeding it `k` instead of a
content hash is what breaks immutability. Re-order the input file and every
identity in the bake moves.

Consequence: a `(classid, tiers, identity)` key minted by one producer does not
resolve in the other's output. Any cross-bake join (mesh ↔ anchor ↔ label) is
currently unsound and only appears to work because each consumer reads one bake.

### F-2 — `row: u32` is written into a `u16` identity slot, unchecked  [LATENT]

`body.rs:129` passes `row` (u32) as `mint_for`'s identity argument. Under V3 the
slot is `identity_v2: u16`. At 1,658 concepts this never fires; at >65,536 it
wraps silently. Same shape as the OBO V3/V1 false-green (60,478 rows → 658
apparent identities). No assertion guards it.

### F-3 — part_of is a DAG upstream; the cascade consumes a tree  [OPEN]

Measured on `AdaWorldAPI/BodyParts3D` (version **3.0**),
`assets/BodyParts3D_data/conventional_part_of.txt`:

- 2,358 edges over 1,523 nodes
- **536 nodes (35%) have more than one parent**
- single root `FMA20394` "human body"

Measured earlier on the **4.0** mesh-derived subset the bake actually walks:
1,368 nodes / 1,367 edges / **0** multi-parent / max depth 16.

CONJECTURE (needs measurement, see P-1): the 4.0 `partof_BP3D_4.0_obj_99`
source is also a DAG and the bake's cascade builder is collapsing it to a
spanning tree by first-parent-wins. If so, 35% of concepts are being addressed
under one arbitrary parent. This is NOT yet proven — the two numbers come from
different dataset versions and are not directly comparable.

### F-4 — Laterality ships as an edge table and is not consumed  [OPEN]

`composite_parts.txt`: 12,530 composite→primitive edges. **8,265 (66%)** have a
primitive whose name is the composite's name plus an axis word:

    right 3871  left 3849  proximal 311  distal 310  lateral 287  posterior 256
    anterior 250  superior 248  lower 239  upper 236  medial 196  inferior 192
    dorsal 34

The bake reads neither this file nor any axis concept: zero hits for
`dorsal|ventral|anterior|posterior|superior|inferior|lateral|medial` across
`bake_body_v3.py`, `bake_body_soa.py`, `body.rs`. Laterality survives only
inside label strings, where it is unaddressable.

### F-5 — `is_a` is populated and unused for addressing  [OPEN]

2,905 nodes, root "anatomical entity", max depth 20, max children 32. The V3
key rails are `part_of : is_a` (le-contract L1), i.e. the layout has a byte
lane for is_a per tier — but `body.rs:107-118` fills all six tiers from the
`is_a` ancestor **sibling-rank** chain and nothing from part_of. The rails are
carrying one relation in both axes.

### F-6 — Depth exceeds the addressable tiers  [OPEN]

part_of max depth 16; the key offers classid + 5 tier slots + identity. 279
FMA nodes (20%) sit deeper than the 7 addressable levels (118 counted below the
classid). Current behaviour: `tier_at(k)` returns 0 past the cascade length
(`body.rs:110-117`), so deep nodes share a prefix with their ancestor.

### F-7 — Upstream deliberately rejected the version we bake from  [CONTEXT]

`AdaWorldAPI/BodyParts3D` README: *"The latest version 4.0, although more
complete, was not chosen as it appears to have intersecting skin/muscle areas."*
Our bakes read 4.0 (`bake_body_v3.py:139-140`). The 20260629b/c re-bakes were
both spent on classification overlap (connective structures floating in ORGAN
and SKIN, teeth outside skeleton). Not proof of a common cause; enough to check.

### F-8 — Two group-membership mechanisms already exist  [DESIGN]

- `lance-graph/.claude/v3/soa_layout/le-contract.md:57` — **L2 facet
  `6 × (8:8)` `memberof : members`**, a sanctioned carving.
- `q2/crates/cockpit-server/src/osint_gotham.rs:12-16` — the EdgeBlock
  **16 × 8-bit adapter mask**, "a node implements the basins it points at".

Both address many-to-many. A v4 that needs group semantics must pick one, not
add a third. The materialized hub node is already ruled out
(`osint_gotham.rs:1081-1087`): *"A materialized hub cannot dock as an edge."*

### F-9 — Prior art solved F-3/F-4 with relation types, not extra nodes  [CONTEXT]

Pommert et al., *Medical Image Analysis* 5(3) 2001 (VOXEL-MAN/InnerOrgans, 650
constituents / >2000 relations over the Visible Human):

- *"Views are represented as attributes of relations."* The kidneys appear under
  abdominal viscera / urogenital system / primary retroperitoneal organs
  depending on view — F-3's multi-parent, made explicit rather than collapsed.
- A separate `branching from` relation type *"modeling the arterial blood flow"* —
  arterial convergence is not part_of and was never forced into it.
- `hidden part of`, so a constituent assembled from several segmented objects
  presents as one entity — the job `composite_parts.txt` does upstream.

---

## 2. Caveats on the evidence

- **C-1** The q2 clone is `--depth 1`. Every file carries the same mtime; git
  dates are useless for "which producer is youngest". Recency was established
  from **release asset timestamps** instead (20260628 → 29 → 29b → 29c).
- **C-2** F-3's two node counts come from **different dataset versions** (3.0
  table vs 4.0 mesh subset). They are not a before/after.
- **C-3** No producer was re-run in this session. Every number is read from
  committed source, committed data, or release metadata.
- **C-4** F-9 is a reading of a paper against our tables. No code linkage
  between VOXEL-MAN and this stack was measured, and none is claimed.
- **C-5** The `20260629c` note in `cockpit/public/body.manifest.json` documents a
  layer reclassification (39 connective structures) — that is a real fix and is
  not among the open items here.
- **C-6** `body.soa.gz` is fetched, not committed; a v4 changes a release asset,
  so the manifest + Dockerfile pull are part of the blast radius.
- **C-7** Absence of a symbol at HEAD is not absence in history (the shallow
  clone). A bounded `git fetch --depth=1000` is required before any claim of
  the form "X was never tried".

---

## 3. Options, weighted

### D-1 — Identity mint (addresses F-1, F-2)

| # | Option | Cost | Risk | Verdict |
|---|---|---|---|---|
| a | **One shared minter, content-addressed**: `identity = f(stable concept key)`, the FMA id being the obvious stable key. All three producers call it. | medium — touches 3 binaries | low | **RECOMMENDED** |
| b | Keep `golden_id` but feed it a content hash instead of `k` | low | medium — collision probe still order-dependent, so collisions resolve differently per run | fallback |
| c | Status quo + document the divergence | zero | high — the unsound join stays | reject |

FMA ids max at 270,201 = 19 bits. That does **not** fit `identity_v2: u16`, so
(a) forces a decision: hash to 16 bits and accept collisions with a
deterministic, content-ordered probe, or carry the exact id in a value tenant
(see D-4) and let the key hold a hash. The quad `4 × u24` (`identity_quad.rs`,
`LegacyOutlier::WideTriple`) exists precisely for exact identifier ordinals
where *"invertibility (not similarity) is the acceptance criterion"* — 19 bits
fits a u24 slot exactly.

### D-2 — Multi-parent / views (addresses F-3, F-5)

| # | Option | Cost | Risk | Verdict |
|---|---|---|---|---|
| a | **Fill the rails as designed**: `part_of` in one byte axis, `is_a` in the other, per L1 | medium | low — uses the carving that already exists | **RECOMMENDED** |
| b | Pommert-style views: a node addressed once per view, view selected by ClassView | high | medium — needs a view carrier that does not exist | worth a probe, not a v4 |
| c | First-parent-wins (status quo) + record which parent won | low | medium — silently arbitrary | interim only |

(a) is the cheap win and is what the rails were carved for. It does not solve
multi-parent; it stops wasting half the rail on a duplicate relation.

### D-3 — Laterality (addresses F-4)

| # | Option | Cost | Risk | Verdict |
|---|---|---|---|---|
| a | **Consume `composite_parts.txt` as an edge relation** composite→primitive; the axis stays an edge, never a key tier | medium | low — matches how upstream ships it, and Pommert's `hidden part of` | **RECOMMENDED** |
| b | Encode the axis as a byte in a rail | low | **high** — an axis is not a mereology tier; poisons prefix routing | reject |
| c | Leave in the label | zero | high — 66% of the relation unaddressable | status quo |

### D-4 — Depth overflow (addresses F-6)

| # | Option | Cost | Risk | Verdict |
|---|---|---|---|---|
| a | Accept the cap, record the truncated remainder in a value tenant | low | low | **RECOMMENDED** |
| b | Registry resolve + ref-escape past 12 native levels (the OGAR canon answer) | high | low | correct long-term |
| c | Widen the key | — | — | **forbidden** (canon: scale is the next cascade level, never field-widening) |

### D-5 — Source version (addresses F-7)

Measure before deciding. Options: stay on 4.0; move to 3.0 (loses concepts,
gains the overlap fix upstream made); or bake 4.0 and diff the skin/muscle
intersection against 3.0 as a falsifier.

---

## 4. Probes — run before any of D-1..D-5 lands

- **P-1** Parse `partof_BP3D_4.0_obj_99`; count nodes, edges, multi-parent, max
  depth. Settles F-3 and C-2. Blocks D-2.
- **P-2** Run `guid.rs` and `anchor.rs` over the same input; count concepts whose
  identity differs. Expected: ~all. Quantifies F-1.
- **P-3** Re-run `guid.rs` with the input shuffled; count identity changes.
  Expected: ~all. This is the immutability falsifier.
- **P-4** Join `composite_parts.txt` against the meshed concept set: how many
  composites and primitives are actually in the bake? Sizes D-3.
- **P-5** Diff 4.0 vs 3.0 skin/muscle geometry for intersection. Settles F-7/D-5.
- **P-6** `git fetch --depth=1000` and grep history for `tribonacci` /
  earlier vessel-radius sequences. Settles C-7.

A v4 that lands without P-1, P-2 and P-3 green is a rebake with the same
addressing defects and a new date in its filename.

## 5. Explicitly out of scope

Geometry, palettes, LOD, the 20260629c connective-layer fix, the renderer, and
the surfel/torso line. This plan is about the key, not the mesh.

---

## 6. Operator sketch — v4 direction (added 2026-09-07)

> **STATUS: TO BE RESEARCHED.** Nothing in this section is decided, scheduled,
> or recommended. It records an operator sketch and what the contract already
> says about each item, so the research starts from the carved state instead of
> re-deriving it. Verdicts belong to a later pass; the §3 weighted decisions and
> §4 probes are unchanged by anything here.

Nine directions, checked against `AdaWorldAPI/lance-graph`'s
`.claude/v3/soa_layout/` contract. Most are already carved; three are new. The
split matters because a carved item needs *wiring*, and a new one needs a
*ruling* first.

### 6.1 Already carved — wire, don't invent

| direction | where it already lives |
|---|---|
| **2 × 12 (second GUID for relationships)** | `le-contract.md:130-137` — *"**Second GUID (relationships):** when a node carries a second GUID dedicated to relationships, its rail plane is ENCOURAGED to carry six relations as `basin : relationtype` pairs"*; and *"if the basins are **12 static**, the pair upgrades to `relationtype : relationtype_orthogonal`"* |
| **6 × palette256² for Fisher-z** | `le-contract.md:171-186` — L4 reads through the **analytic Fisher-z codec** (`bgz-tensor::fisher_z::{FamilyGamma, FisherZTable}`), certified ρ≥0.999, `E-FISHERZ-CANONICAL-COSINE-REPLACEMENT-1`. *"A materialized k×k table is a CACHE of the formula, never the canon."* Boundary: replaces the distance/rank READ; the semiring COMPOSE keeps its table |
| **helix Signed360** | tenant 4 `HelixResidue`, 6 B `[112,118)` — *"48-bit helix place (2× 24-bit equal-area hemisphere, Signed360)"*. Sibling of the above: *"helix is to Fisher-2z what the cosine-replacement is to Fisher-z"* |
| **many-to-many nodes** | **three** existing mechanisms — tenant 15 `EpisodicBasin` (§6.2), L2 facet `memberof : members`, and q2's own EdgeBlock 16×8-bit adapter mask (`osint_gotham.rs:12-16`) |
| **24 × i4 Markov context** | tenant 14 `CausalWitness`, `U8 × 16` `[204,220)`, read **G24N4** — *"24 signed i4 loci… each nibble is a context pointer (signed ±8 window offset), not a strength"*. Slots 16..24 reserved-zero. **EXPERIMENTAL**, and by its own doc-comment *"not in the operator-locked §3 catalogue"*; it is a **lane shape name, never a `CascadeShape` variant** |

### 6.2 The many-to-many node already encodes the no-hub ruling

Tenant 15 `EpisodicBasin` — *"a promoted basin as **REFERENCES**: `subject` u16 ·
`member_count` u16 · `self_code` 12 B (Cam96 centroid) · `version_from`/`version_to`
u64. **Members are reached by following `(subject, [from,to))` into the triple
stream, never inlined** — the fat-concept guard §3a names. Width is NOT stored
(recomputable through the references)."*

A group that carries a member *count* and a version range but not a member
*list*. That is the same ruling q2 reached independently at
`osint_gotham.rs:1081-1087` (*"A materialized hub cannot dock as an edge"*),
arrived at from the other side. **A v4 needing group semantics picks one of the
three; it does not add a fourth.**

### 6.3 Genuinely new — TO BE RESEARCHED

- **Zipper over 2 × 12 = 24 bytes.** Arithmetic first: 24 bytes read as `(8:8)`
  rails is **12 rails**, not 24. With classid that is 13 addressable levels
  against part_of's measured max depth of **16** (F-6). It closes most of the
  279 over-deep nodes but not all, so it *pairs with* D-4b (registry resolve +
  ref-escape), it does not replace it. Open: whether the second 12 B comes from
  the EdgeBlock or from a second GUID per §6.1 — these are different rows.
- **Hexagon substrate with trie addressing.** CONFLICT, at the arithmetic
  level: HHTL is `FAN_OUT = 16`, one nibble per level, tier-of-level =
  `level >> 2` — *"a shift, never a branch"* (OGAR canon). A hex lattice has 6
  neighbours (7 with centre); neither divides a nibble. Adopting it trades away
  the shift/mask uniformity the 3×4-vs-4×3 ruling was decided on. Needs the
  standing-watch flip condition (a measured workload where it wins) before it
  is more than an idea. TO BE RESEARCHED, not proposed.
- **Volumetric fill as an EXPLICIT trie + spatial edges.** The stronger half of
  the same thought, and it does *not* carry the hex conflict.
  `fill_body_soa.py` already does 3D connected-component analysis (`:82-83`)
  and ring-sweeps a core (`:115-146`); today it emits triangles. Emitting an
  addressed trie with spatial edges instead is a real substrate change and is
  what Gagvani & Silver's volumetric skeleton (*"an advanced data structure for
  referencing all of the voxels"*) was reaching for. Prerequisite: decide
  whether the trie is the address or a second index beside it.
- **BPE for behaviour.** No prior art found anywhere in the workspace. Needs a
  statement of what the token vocabulary is over before it can be weighed.

### 6.4 Adjacent, and deliberately still out of scope

- **Better vessels** — geometry, excluded by §5. The shipped fix is empirical
  clamps (`caliber × CAP`, `PCTL = 0.30`), not a growth sequence; a v4 that
  also re-opens vessel caliber should say so explicitly and take §5 with it.
- **"Street" nodes repurposed** — the OSM `.chains` sidecar is the one carrier
  in the stack that holds an **ordered, variable-length path**
  (`osm_features.rs:829`, *"vertex chains for tagged ways"*). If vessel
  centerlines want to be first-class paths rather than swept triangles, that is
  the shape to reuse. Note its sparse-ordinal cost is already documented:
  chains need an ascending ordinal index (~17.6 MB at ~4.4 M entries) because
  row position ≠ ordinal, where dense books need none
  (`osm_chains_books_lance.rs:19-41`).
- **Masking algebra** — measured (ternlog `T3/T1 → 0.50` by K=8, flat in K,
  contingent on L2 residency: 138 → 15 GB/s past L2). Not wired to any FMA
  read path today; it is a consumer of whichever addressing v4 settles on, not
  an input to it.

### 6.5 One doc-drift found while checking the above  [DOC-ONLY]

`.claude/v3/soa_layout/tenants.md:88` describes the `Full = 3` preset as
*"all 15 tenants 0–14 (Meta … `CausalWitness`)"*. The tenant table in the same
file runs **0–15**, ending at `EpisodicBasin`.

The **code is correct** — `canonical_node.rs:942` has `EpisodicBasin = 15`, and
`:1252` compile-asserts
`ValueSchema::Full.field_mask().count() == VALUE_TENANTS.len()`, which would not
build if the preset and the table disagreed. So this is stale prose in
`tenants.md`, not a defect: 16 tenants, 0–15. Belongs upstream in lance-graph,
not in this repo; noted here only so a v4 session reading that line does not
size a preset from it.
