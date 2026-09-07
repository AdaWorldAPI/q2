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

### F-2 — ⊘ REFUTED by P-2/P-3 measurement, see §9.3  [WITHDRAWN]

> **This finding is wrong.** Both mint paths assert with a message naming their
> width; there is no silent wrap. The text below is kept as the original claim.
> The real (test-coverage) defect it surfaced is fixed in lance-graph#1211.

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

### F-6 — Depth exceeds the addressable tiers  [OPEN — figures replaced, see §9.4]

> ⚠ The "max depth 16" below is **withdrawn**: measured 9 (shortest) / 12
> (longest) on 3.0. Overflow is 4.99% or 50.43% depending on which depth is
> addressed — a question §9.4 shows is unruled.

**Level-counting convention, pinned here and used everywhere in this plan:**
a *level* is the classid (the root) plus one per **rail**; `identity` is the
instance slot and is NOT a level. Under that convention the V3 key gives
**6 levels** — classid + the 5 cascade rails `body.rs:121-129` fills
(HEEL/HIP/TWIG/LEAF/family), with the 6th rail spent on `identity`. This is
the same convention §6.3 uses to reach *classid + 12 rails = 13 levels*.

part_of max depth is **16**, so the key is short by 10 levels. Current
behaviour: `tier_at(k)` returns 0 past the cascade length (`body.rs:110-117`),
so deep nodes share a prefix with their ancestor.

⚠ **The overflow counts need re-deriving.** The earlier measurement — 279 FMA
nodes (20%) over the cap, 118 counted below the classid — was taken against a
7-level reading that counted `identity` as a level, which the convention above
rejects. The shape of the finding is unchanged (the tree is deeper than the key)
but the magnitude is not yet trustworthy at the pinned convention. **P-1 must
report the depth histogram, not just the max**, and D-4 is sized from that.

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

### D-2 — Rail contents and multi-parent (F-5 is closable here; F-3 is not)

| # | Option | Cost | Risk | Verdict |
|---|---|---|---|---|
| a | **Fill the rails as designed**: `part_of` in one byte axis, `is_a` in the other, per L1 | medium | low — uses the carving that already exists | **RECOMMENDED — for F-5 only** |
| b | Pommert-style views: a node addressed once per view, view selected by ClassView | high | medium — needs a view carrier that does not exist | worth a probe, not a v4 |
| c | First-parent-wins (status quo) + record which parent won | low | medium — silently arbitrary | interim only |

(a) is the cheap win and is what the rails were carved for, but it closes
**F-5 and nothing else**: one byte-axis per relation still stores exactly ONE
`part_of` parent, so a node with three parents keeps getting one. **Do not read
the RECOMMENDED verdict as covering F-3.** Multi-parent is closable only by (b),
which needs a view carrier that does not exist; until that carrier is designed,
F-3 stays open whatever this row says. §7.2 states the same thing from the
remedy side.

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
`.claude/v3/soa_layout/` contract: **five already carved (§6.1), four new
(§6.3)**. The
split matters because a carved item needs *wiring*, and a new one needs a
*ruling* first.

### 6.1 Already carved — wire, don't invent

| direction | where it already lives |
|---|---|
| **2 × 12 (second GUID for relationships)** | `le-contract.md:130-137` — *"**Second GUID (relationships):** when a node carries a second GUID dedicated to relationships, its rail plane is ENCOURAGED to carry six relations as `basin : relationtype` pairs"*; and *"if the basins are **12 static**, the pair upgrades to `relationtype : relationtype_orthogonal`"* |
| **6 × palette256² for Fisher-z** | `le-contract.md:171-186` — L4 reads through the **analytic Fisher-z codec** (`bgz-tensor::fisher_z::{FamilyGamma, FisherZTable}`), certified ρ≥0.999, `E-FISHERZ-CANONICAL-COSINE-REPLACEMENT-1`. *"A materialized k×k table is a CACHE of the formula, never the canon."* Boundary: replaces the distance/rank READ; the semiring COMPOSE keeps its table |
| **helix Signed360** | tenant 4 `HelixResidue`, 6 B `[112,118)` — *"48-bit helix place (2× 24-bit equal-area hemisphere, Signed360)"*. Sibling of the above: *"helix is to Fisher-2z what the cosine-replacement is to Fisher-z"* |
| **many-to-many nodes** | **three** existing mechanisms — tenant 15 `EpisodicBasin` (§6.2), L2 facet `memberof : members`, and q2's own EdgeBlock 16×8-bit adapter mask (`osint_gotham.rs:12-16`) |
| **24 × i4 Markov context** | tenant 14 `CausalWitness`, a 16-byte lane at `[204,220)` holding the V3 4+12 facet. The **G24N4 carving applies to the 12-byte payload, not the 16-byte lane**: `WITNESS_REGISTER_BYTES` = 12 B = **24 nibbles**, `WITNESS_LOCI = 24`, `NAMED_LOCI = 16`, and slots **`16..24` (half-open, 8 slots) reserved-empty** — "held open, never padded with a construct to reach 24" (`causal_witness.rs:71-85`). Each nibble is a context pointer (signed ±8 window offset), never a strength. **EXPERIMENTAL**: `causal_witness.rs:14-19` records that the cited "§3 L9 `G24N4`" entry **does not exist** — §3 is L1–L8 — and that a sub-byte carving is a **lane shape NAME**, never a `CascadeShape` variant |

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

---

## 7. Defect → remedy matrix (added 2026-09-07)

What is wrong today, what closes it, and what each remedy leaves standing.
Read with §2: several rows are gated on a probe that has not run, and one row
has no owning decision at all.

### 7.1 The matrix

| flaw | observable symptom today | closed by | partially addressed by | residual after the fix |
|---|---|---|---|---|
| **F-1** identity minted three ways, two order-dependent | a key from one producer does not resolve in another's output; re-ordering the input moves every identity | **D-1a** (one shared content-addressed minter) | D-1b (content hash into `golden_id`) — its `+1` collision probe still resolves in enumeration order, so collisions differ per run | FMA ids are 19 bits and `identity_v2` is 16: D-1a forces the sub-choice of hash-with-probe vs the `4 × u24` quad. Unresolved until that is picked |
| **F-2** `row: u32` → `identity: u16`, unchecked | silent wrap past 65,536 concepts; today never fires at 1,658 | **nothing on its own** | D-1a removes `row` as the identity source | **D-1a does not add a guard.** F-2 needs an explicit `TryFrom`/`debug_assert` at the mint regardless of which D-1 option wins. Do not treat it as closed by D-1 |
| **F-3** part_of is a DAG upstream; cascade consumes a tree | ~35% of concepts (3.0 table) addressed under one arbitrary parent — CONJECTURE for 4.0 | **D-2b** (views as attributes of relations) | D-2c (record which parent won) makes the arbitrariness auditable, not correct | D-2b needs a **view carrier that does not exist**. Blocked on **P-1**; until P-1 runs, the size of this flaw in 4.0 is unknown |
| **F-4** laterality edge table unconsumed | 8,265 of 12,530 composite→primitive edges carry an axis word; the axis is reachable only by string-matching a label | **D-3a** (consume `composite_parts.txt` as an edge relation) | — | Sized by **P-4** — unknown how many composites/primitives are in the meshed set. D-3b (axis into a key tier) is *rejected*, not deferred |
| **F-5** `is_a` fills both rail axes | the L1 `part_of : is_a` carving carries one relation twice; part_of contributes nothing to the address | **D-2a** (fill the rails as designed) — and D-2a closes **nothing else** | — | Closes the waste, **not F-3**. A node with three parents still gets one |
| **F-6** part_of depth 16 > the key's 6 addressable levels | deep nodes share a prefix with an ancestor via the `tier_at → 0` fallback; the old 279-node/20% figure is withdrawn pending P-1's depth histogram (see F-6) | **D-4b** (registry resolve + ref-escape) | D-4a (record the truncated remainder in a tenant) makes it lossless-on-read, not addressable; §6.3's 2×12 zipper reaches 13 levels of 16 | D-4c (widen the key) is **forbidden by canon** — scale is the next cascade level, never field-widening |
| **F-7** we bake 4.0; upstream rejected 4.0 for intersecting skin/muscle | two re-bakes (20260629b, c) spent on classification overlap | **no option yet** — D-5 is deliberately undecided | — | Gated on **P-5**. Until measured, a common cause is CONJECTURE only |
| **F-8** three group-membership mechanisms coexist | `EpisodicBasin` (tenant 15), L2 `memberof : members`, and q2's EdgeBlock adapter mask all address many-to-many | **no owning decision — this is a gap in §3** | §6.2 states the constraint ("pick one, do not add a fourth") but assigns no verdict | Needs a **D-6** before any group work lands, or the v4 adds a fourth by accident |
| **§6.5** `tenants.md:88` says 15 tenants; the table runs 0–15 | a session sizing a preset from the prose is off by one | a one-line doc fix **in lance-graph** | — | **Not fixable from this repo.** Code is correct (`canonical_node.rs:1252` compile-asserts the pairing); prose only |

### 7.2 What no remedy in this plan touches

- **Multi-parent addressing is not solved by anything currently on the table.**
  D-2a stops the rails wasting an axis; D-2b is the real answer and needs a
  carrier that does not exist. D-2a is marked RECOMMENDED for F-5 **only** —
  reading that verdict as covering F-3 is the specific misreading §3's D-2 note
  now guards against. If P-1 shows the 4.0 part_of is a DAG, this becomes the
  largest open item in the plan, larger than F-1.
- **Vessel caliber** (§6.4) is excluded by §5 and no D-item covers it.
- **The hex substrate / explicit volumetric trie** (§6.3) are research, not
  remedies; neither closes a flaw listed above.

### 7.3 Ordering constraint

F-1 is BLOCKING and independent — D-1a can land before any probe. Everything
else has a gate:

    P-1 ──gates──▶ D-2 (F-3, F-5)
    P-4 ──sizes──▶ D-3 (F-4)
    P-5 ──gates──▶ D-5 (F-7)
    P-2, P-3 ─────▶ quantify and falsify F-1 (evidence, not a gate on D-1a)

D-1a plus the F-2 guard is therefore the only work in this plan that could
begin today. Everything else waits on a measurement, and the plan's §4 rule
stands: a v4 that lands without P-1, P-2 and P-3 green is a rebake with the
same addressing defects and a new date in its filename.

---

## 8. Sources

Full reference doc — what each source is, what it establishes, and the required
BodyParts3D attribution — lives at
**`.claude/docs/anatomy-substrate-prior-art.md`**. Short form:

| source | establishes | bears on |
|---|---|---|
| Mitsuhashi et al., *Nucleic Acids Res* 2009 (PMC2686534, doi:10.1093/nar/gkn613) — **BodyParts3D** | the geometry + the `part_of` / `is_a` / `composite_parts` tables our bakes read | F-3, F-4, F-5, F-7 |
| Pommert et al., *Medical Image Analysis* 5(3), 2001 — **VOXEL-MAN / InnerOrgans** | *"views are represented as attributes of relations"*; a separate `branching from` type for arterial flow; `hidden part of`; ellipsoid + connected-component segmentation | F-3 (→ D-2b), F-4 (→ D-3a), §6.4 vessels |
| Gagvani & Silver (Rutgers) — **Animating the Visible Human** | a volumetric skeleton as *"an advanced data structure for referencing all of the voxels"* | §6.3 explicit volumetric trie |
| NLM **Visible Human Project** | the cryosection/CT source under both of the above | lineage context only |

**Lineage caution, restated because it is easy to lose:** VOXEL-MAN and Gagvani
are voxel models over the Visible Human; BodyParts3D is surface mesh from
DBCLS/Anatomography, independent. Our bake is on the mesh line. The convergence
recorded above is in the **addressing**, not the geometry — no code path linking
the lineages was measured, and none is claimed (C-4).

---

## 9. Probe results (run 2026-09-07)

Four of the six §4 probes ran. Every number below is a command output; the
commands are in the PR that carries this section. **Three findings above are
corrected by these results, one of them a refutation of a claim this plan
itself made.** The §1 entries are regraded in place, not deleted.

### 9.1 Probe status

| probe | status | why |
|---|---|---|
| **P-1** depth histogram / DAG | **PARTIAL** | ran on BodyParts3D **3.0**; the 4.0 `partof_BP3D_4.0_obj_99` the bake reads is **not on disk** |
| **P-2** identity divergence | **DONE** | real binaries, real trees |
| **P-3** immutability falsifier | **DONE**, and it **corrected the stated mechanism** |
| **P-4** laterality sizing | **PARTIAL** | 3.0 STL presence used as the "has geometry" proxy; 4.0 OBJ meshes not on disk |
| **P-5** 4.0-vs-3.0 skin/muscle | **BLOCKED** | needs both geometry sets; neither 4.0 nor a 3.0 OBJ set is present |
| **P-6** history archaeology | **DONE** | shallow clone deepened 56 → **2201** commits |

### 9.2 F-1 — CONFIRMED, but the failure mode is not the one stated

**P-2, measured on the real binaries over `fma/data/{inclusion,isa_inclusion,element_parts}.txt`:**
223 concepts appear in both outputs. **223/223 (100%) get a different
identity.** `classid`, HEEL, HIP, TWIG and F4 are **identical in all 223** —
both binaries compute the address prefix with byte-identical code, and diverge
only at the mint. So the two producers agree on *where* a concept sits and
disagree on *which* concept it is.

**P-3 refutes this plan's stated mechanism.** F-1 and the §3 D-1 table said
re-ordering the input moves every identity. Measured: reversing the input file
changes **0 of 1368**. `guid.rs` calls `sorted.sort()` before minting, so file
order cannot reach `k`.

**The real failure is enumeration-index shift.** Deleting one node near the
start of the sorted set — so every later `k` shifts by one — changes
**863 of 1367 (63%)** of the identities. That is worse than the original
claim in practice: it means **adding or removing a single concept anywhere
upstream re-mints most of the tree**, while merely receiving the same set in a
different order is harmless.

`anchor.rs` is stable as claimed: **0 of 203** changed across two runs with
different membership. Its `fnv16(path)` is a pure content hash.

Collision probes fired **0 times** in all three `guid.rs` runs — no evidence
either way about the probe path on this corpus; recorded rather than omitted.

> **Consequence for D-1.** The sub-choice is unchanged (16-bit hash+probe vs
> the `4 × u24` quad), but the *argument* changes: the defect is not
> order-sensitivity, it is that identity is a function of **set membership**.
> Any content-addressed mint fixes it; `golden_id(k)` cannot be rescued by
> stabilising input order, because input order was never the input.

### 9.3 F-2 — REFUTED. There is no silent truncation.

Both mint paths in `lance-graph-contract` assert, in release:

- `NodeGuid::new` (V1): `assert!(identity <= 0x00FF_FFFF, "identity must fit in 24 bits")` — `canonical_node.rs:209`
- `mint_for` V2/V3 arm: `assert!(identity <= 0xFFFF, "v2/v3 identity must fit in 16 bits (no silent truncation)")` — `:386-389`

`osint-bake` requests `features = ["guid-v3-tail"]`, which implies
`guid-v2-tail`, so the guarded arm is the live one. A row ≥ 65,536 **panics
with a message naming the width**; it does not wrap. F-2's "silent wrap" and
§7's "closed by nothing on its own — needs an explicit `TryFrom`/`debug_assert`"
are both **wrong** and are withdrawn.

What the audit did surface, indirectly and genuinely: the V1 guards have had
`should_panic` cover since they landed (`:2152-2162`); **the V2/V3 guards that
supersede them had none** — their panic strings grep to exactly one hit each,
the definition site. Fixed upstream in `AdaWorldAPI/lance-graph#1211` (three
tests, `cargo test -p lance-graph-contract --lib` 1318 → 1321).

### 9.4 F-6 — the depth figures are replaced, and the severity is convention-dependent

**P-1 on BodyParts3D 3.0** `conventional_part_of.txt`: 2,358 edges, 1,523
nodes, **acyclic**, single root `FMA20394 human body`.

The plan said "part_of max depth is **16**". Measured:

| metric | value |
|---|---|
| max(**min_depth**) — shortest path from root | **9** |
| max(**max_depth**) — longest path from root | **12** |

Neither is 16. The 16 is unverified and withdrawn.

**Overflow past the key's 6 addressable levels (depths 0..5) — and this is the
finding:**

| basis | nodes over | share |
|---|---|---|
| min_depth | **76** | **4.99%** |
| max_depth | **768** | **50.43%** |

A **10× spread**, from a rounding error to half the graph, decided entirely by
a question nobody has answered: **does the cascade encode a concept's shortest
ancestry or must it represent its deepest?** In a DAG with 536 multi-parent
nodes those are different addresses for the same concept. D-4 cannot be sized —
and arguably F-6 cannot be graded — until that is ruled.

### 9.5 F-3 — independently confirmed

**536 of 1,523 nodes have >1 distinct parent** (365 with exactly 2, 83 with 3,
48 with 4, 40 with ≥5). Most-parented: `FMA16202 sacrum` with **6**. Acyclic,
single-rooted. Matches the earlier count exactly, from an independent script.

### 9.6 F-4 / D-3a — the shape is right, the mechanism needs reshaping

Axis census recomputed independently over all 12,530 rows: **8,265 (66.0%)**
carry an axis word (right 3871, left 3849, proximal 311, distal 310, lateral
287, posterior 256, anterior 250, superior 248, lower 239, upper 236, medial
196, inferior 192, dorsal 34, **ventral 0**).

**The join says D-3a cannot mean what it says.** Of 579 distinct composite ids,
**0 have geometry** — the composite column holds `BP##` sentinels and
high-level aggregates (`FMA20394 human body`, `FMA7153 cardinal body part`,
`FMA72954 muscular system`). Primitives are different: **873 of 1,419** are
meshed.

| rows | count |
|---|---|
| both composite and primitive meshed | **0** |
| primitive meshed, composite not | **8,611** |
| neither meshed | 3,919 |

So "consume composite→primitive as an edge relation" cannot be an edge between
two mesh nodes. It is **an edge from a meshed concept to an abstract grouping
label** — which is still useful, and is 8,611 rows of it.

**And it is not redundant with part_of:** only **2,172 of 12,530** pairs also
appear as a part_of edge. **83% are orthogonal** — composite_parts carries
relations part_of does not have. That is the strongest evidence yet for D-3a's
core claim, even as it reshapes the mechanism.

### 9.7 P-6 — the Tribonacci→Fibonacci vessel lineage did not happen

History deepened 56 → **2201 commits** and searched in full.

**`tribonacci`: 2 hits, neither about vessels.** Both are GLSL cloud-shader
lacunarity in the terrain renderer (`b77da963`, 2026-07-08, *"tribonacci cloud
sky"*, `TRIBONACCI ≈ 1.8393` as an fBm octave multiplier). The other hit is
this plan's own P-6 line proposing the search.

**`fibonacci` / golden: all hits are orientation and placement** — helix
surfel-normal encoding, golden-angle spiral scene layout, and `GOLDEN_STRIDE` /
`golden_id` in the GUID cascade. None touch vessel radius.

**The vessel constants were empirical clamps from the first commit that
introduced them:**

| commit | date | change |
|---|---|---|
| `473ed2a5` | 2026-06-28 | introduces `CORE = 0.55`, `RMAX = 0.020`, `RMIN = 0.0008` |
| `7689878a` | 2026-06-28 | `CORE 0.55 → 0.62` |
| `daf987f8` | 2026-06-28 | median-based clamp, same formula shape |
| `2a7ac4a3` | 2026-06-29 | adds `CAP = 2.0` + `PCTL = 0.30` (per-vessel caliber cap) |

No mathematical sequence was ever tried for radii and replaced. The
"monstrous → voluptuous → clamps" narrative is **not supported by the commit
record**; §6.4's reading of the clamps as the *fix* stands, but its implied
history does not.

**Producer recency, now measurable** (the shallow clone made every file look
identically aged — C-7 discharged):

| producer | last commit | date |
|---|---|---|
| `crates/osint-bake/src/bin/body.rs` | `b3d33112` | **2026-09-06** (youngest) |
| `crates/osint-bake/tools/bake_body_v3.py` | `f9a3ad69` | 2026-06-29 |
| `crates/osint-bake/tools/fill_body_soa.py` | `2a7ac4a3` | 2026-06-29 |
| `fma/src/bin/guid.rs`, `anchor.rs` | `ac55a7a2` | 2026-06-24 (oldest) |

### 9.8 What this changes about the ordering

§7.3 said D-1a plus the F-2 guard was the only work that could start today.
**F-2 needs no work at all.** D-1a stands, with a corrected argument (§9.2),
and is now the *only* unblocked item — with its sub-choice sharpened by the
19-bit FMA id: the `4 × u24` quad holds it exactly, a 16-bit hash cannot.

D-2 and D-4 remain blocked, and P-1 did not unblock them — it ran on 3.0, and
it surfaced a **prior** question (shortest vs deepest ancestry) that must be
ruled before D-4 can be sized at all.

---

## 10. The zipper addressing proposal (measured 2026-09-07)

> **STATUS: MEASURED, TO BE RESEARCHED.** Every figure below is a command
> output over the BodyParts3D **3.0** data on disk (`FMA.csv`,
> `conventional_part_of.txt`). Nothing here is ruled, and §6 remains
> unresearched. P-5 (4.0) is still blocked, so every bound is 3.0's.

### 10.1 The proposal

Operator, 2026-09-07. Replace the shared L1 rail with a same-relation zipper
spanning two tenants:

```
old   L1 = 6 × (8:8) = part_of : is_a        → 6 is_a slots, 6 part_of slots
new   L1 = 6 × (8:8) = is_a : is_a           ┐
      L2 = 6 × (8:8) = is_a : is_a           ┘ → 24 is_a slots
```

with `part_of` read as **the parent of the last** occupied slot, **parent =
mask −1** (prefix truncation), extra `part_of` parents carried on the
`EdgeBlock`, and — where edges do not suffice — many2many group nodes.

### 10.2 Why 24: it is the measured bound, not a round number

is_a depth over all 104,698 parented nodes in `FMA.csv`:

| is_a levels | overflow | share |
|---|---|---|
| 6 (today's L1 rail) | 100,439 | **95.93 %** |
| 12 (one tenant) | 50,885 | 48.60 % |
| **24 (two tenants)** | **0** | **0.00 %** |

Max is_a depth is **exactly 24** — a coherent chain `owl#Thing` →
`Anatomical entity` → … → `Dorsal digital vein of left big toe`. At today's
6 slots the is_a rail does not merely overflow, it fails for **96 %** of
concepts. **24 has ZERO headroom**: any deepening in 4.0 breaks it.

### 10.3 `parent = mask −1` is valid for is_a, and NOT for part_of

| relation | source | nodes | multi-parent | shape |
|---|---|---|---|---|
| **is_a** | `FMA.csv` | 104,698 | **2** (0.002 %) | **tree** |
| **part_of** | `conventional_part_of.txt` | 1,522 | **536** (35 %) | **DAG** |

Prefix truncation needs a tree. is_a is one (104,696/104,698 single-parent;
the exceptions are `Aortopulmonary septum` with two real parents, plus one
malformed row). part_of is not: 365 nodes have 2 parents, 83 have 3, 48 have
4, 39 have 5, and one — the sacrum — has 6.

**§9.4's shortest-vs-deepest question (D-4) becomes MOOT** at these widths:
part_of max depth is 12 (longest path) / 9 (shortest), so a 12-slot part_of
lane overflows **0 either way**. The zipper does not answer D-4; it dissolves
it.

### 10.4 Edges carry the DAG; group nodes are not needed for it

Once the mask chain holds the primary parent, edges carry only the extras:
365 nodes need 1 slot, 83 need 2, 48 need 3, 39 need 4, **1 needs 5** — 836
extra edges, worst case **5 against the EdgeBlock's 16**.

**But a slot is a ONE-BYTE ref, and one byte is not a global address.** The
operator's two escapes — *"accumulate if they are global, or reference the
uncle"* — are decided by which relation orders the address:

| ladder the address is ordered by | extras within uncle range (climb ≤ 2) | max climb | unreachable |
|---|---|---|---|
| **is_a-ordered** | 14.0 % | 16 | **23** |
| **part_of-ordered** | **45.8 %** (88.9 % at ≤ 3) | **5** | **0** |

A one-byte relative ref (`climb` 3 bits, `sibling` 5 bits) fits the part_of
ladder: max climb 5, and only **1 of 493** internal nodes exceeds 32 children
(max 36, mean 3.1).

**Consequence for this proposal:** with is_a in the 24, the address is
is_a-ordered, so the uncle escape does NOT apply (86 % out of range, 23
unreachable) and the extras must be the **accumulating/global** kind. That is
affordable — the part_of graph is 1,523 nodes ≈ 11 bits, so ~2 bytes per ref,
worst node 5 × 2 = 10 of 16 EdgeBlock bytes. **The real decision is the
ordering relation, not the slot count**: is_a-ordered buys the clean 24-deep
taxonomy mask and pays 2-byte global edge refs; part_of-ordered buys one-byte
uncles and needs is_a represented some other way.

### 10.5 The bone baseline, and hubs that already exist

Operator: *"bones as the baseline"*, then *"bones > many2many > second hop
many2many (organs)"*. Rings outward from the 301 skeleton nodes, over
undirected part_of:

| hop | nodes | composition |
|---|---|---|
| 0 | 301 | skeleton (baseline) |
| **1** | **54** | **100 % tissue-less ("other")** |
| 2 | 143 | muscle 72, other 65, **organ 6** |
| 3 | 263 | muscle 151, other 99, **organ 13** |
| 4 | 294 | muscle 207, other 68, vessel 11, **organ 5** |

The hop-1 shell carrying **no tissue at all** is the signature of a grouping
layer — and its members are named as such: `Set of phalanges` (deg 57),
`Skeletal system of lower free limb` (44), `Foot` (33), `Anterior chest` (33),
`Abdomen` (24), `Thorax` (20), `Trunk` (19).

**The many2many hubs do not need inserting — they need recognising.** FMA
already ships regions and sets as first-class nodes; they classify as "other"
only because `layer_of` keys on tissue, which a container has none of. The
same pattern dominates is_a: `Set of arteries` (224 subtypes), `Set of
systemic veins` (169), `Set of nerves` (141).

**Correction to the hop count:** two hops reaches **6 of 24** organs; the mode
is three (13), and 5 need four. The ladder is right, one rung longer than
stated for most organs.

**Do not read the `Set of…` / `Subdivision of…` names as many2many markers —
they are fan-out, and the distinction decides whether mask −1 survives.**
Many-to-many requires more than one parent; is_a has only **2** multi-parent
nodes in all 104,698, so **no is_a naming pattern can be the many2many layer**.
Measured:

| pattern | nodes | fan-out mean/max | leaves | multi-parent |
|---|---|---|---|---|
| `Subdivision of…` | 537 | 5.0 / 40 | 3 % | **0** |
| `Tributary of…` | 146 | 3.1 / 22 | 31 % | **0** |
| `Region of…` | 489 | 5.6 / 150 | 2 % | **0** |
| `Segment of…` | 552 | 4.7 / 52 | 14 % | **0** |
| `Set of…` | 3 409 | **1.0** / 224 | **79 %** | **0** |

`Subdivision of…` and `Tributary of…` are strictly ONE-to-many (522 and 101
respectively are single-parent-with-children). They are the **depth spine** of
the is_a tree — the rungs that produce §10.2's 24-level chain (`Subdivision of
inferior systemic venous tree` → `Tributary of femoral vein` → … ). They BUILD
the tree that makes `parent = mask −1` valid; they do not violate it.

So the two layers are in two different graphs and must not be conflated:

- **depth spine** — `Subdivision of…` / `Tributary of…`, in **is_a**, a tree,
  addressed by the mask chain;
- **hub shell** — the region/set nodes of §10.5's hop-1 ring, in **part_of**,
  where the 536 genuine many2many nodes actually live, carried on edges.

Also: `Set of…` is **not** a reliable hub marker — 79 % are leaves with mean
fan-out 1.0. Only a handful (`Set of arteries` 224, `Set of systemic veins`
169, `Set of nerves` 141) are real hubs, so keying on the name would be wrong
four times in five.

Worked example (operator: *"sternum connective tissue(n) > lunge, herz
<aorta hub>"*), traced in the data:

```
sternum → anterior chest → middle mediastinum → heart        (3 hops)
sternum → anterior chest → thorax → right lung → lung        (4 hops)
sternum → anterior chest → thorax → thoracic aorta → aorta   (4 hops)
```

`anterior chest` (32 children, 1 parent) and `thorax` (19/1) are clean
fan-out hubs. `middle mediastinum` has 1 child and **2 parents** — a bridge,
i.e. one of the 536 edge-carried cases, not a fan-out hub.

**The aorta is NOT a hub in either graph**: 0 is_a subtypes, 2 part_of parts —
effectively a leaf. An aorta-rooted vessel hub would have to be MINTED; what
FMA ships is the flat `Set of arteries`. Vessel *depth* comes instead from the
`Subdivision of…` / `Tributary of…` chain, which is the 24-deep ladder of
§10.2.

### 10.6 Open items this section adds

- **O-10a — the baseline's cardinality is ambiguous.** Three skeleton counts
  are live: **203** bones (the original `bake_torso_splat.py` v4 census),
  **281** (what `/helix` renders today), **301** (part_of under `layer_of`'s
  `bone|cartilage`). If bones are the baseline, a 203/281/301 ambiguity in the
  anchor propagates into every address derived from it. Pin it before minting.
- **O-10b — 23 rows in `FMA.csv` carry an EMPTY FMAID** (e.g. `Costal surface
  of scapula`, `Oral orifice`). At mint time they either vanish or collide at
  address 0. Decide before the bake, not after.
- **O-10c — the one-byte reachability gate (falsifier).** After minting, check
  that all 836 extra part_of edges are one-byte-reachable from their child. If
  yes, edges alone suffice and group nodes stay unused headroom. If no, that is
  where group nodes earn their place — for the out-of-scope extras
  specifically, never for multi-parenthood in general.
- **O-10d — 24 has zero headroom.** The bound is exact on 3.0. P-5 (4.0) must
  run before 24 is locked.

### 10.7 Reproducing these numbers

All figures come from read-only passes over
`bodyparts3d/assets/BodyParts3D_data/{FMA.csv,conventional_part_of.txt,parts_list_e.txt}`;
`FMA.csv` ids are bare numerals and `conventional_part_of.txt` ids carry an
`FMA` prefix, so the join needs `removeprefix("FMA")` (1,496 of 1,522 part_of
children resolve into the is_a tree). Layer names follow
`crates/osint-bake/tools/body-soa-wire/src/main.rs::layer_of`. No producer was
run and no bake artifact was read or written.

---

## 11. Vessels as OSM ways ("street nodes") — measured 2026-09-07

> **STATUS: MEASURED, TO BE RESEARCHED.** Operator proposal: *"the aorta then
> could be a street node as on OSM"*. What follows is what the code and data
> say about it; nothing is ruled.

### 11.1 The current pipeline RECONSTRUCTS what a way would GIVE

`crates/osint-bake/tools/fill_body_soa.py` derives each vessel's centerline
from an **unordered point cloud**: principal axis by power iteration on the
3×3 covariance, `BINS = 14` axial bins, then a per-bin radius from the
perpendicular distances. Six constants govern it, and each one's own comment
names the failure it patches:

| const | value | the comment's justification |
|---|---|---|
| `CORE` | 0.62 | inner-core radius fraction under the wall |
| `RMAX` | 0.020 | *"ABSOLUTE diameter boundary … covers the aorta; clamps balloons"* |
| `RMIN` | 0.0008 | floor so capillaries keep a visible core |
| `CAP` | 2.0 | *"RMAX alone lets a finger artery balloon to aorta size at a bend"* |
| `PCTL` | 0.30 | *"at a strong bend two arms share one axial bin and the median perp-distance is ~half the gap (a balloon)"* |
| `CELL` | 0.015 | *"a continuous vessel keeps adjacent cells occupied … blobs farther apart split"* |

**Every one is the absence of a way.** An OSM way is an ORDERED node list:
the centerline is given, never inferred; junctions are explicit SHARED nodes,
so "two arms share one axial bin" cannot arise; and calibre is an attribute
along the way, so a finger artery cannot inherit aortic radius. `RMAX` sized
to cover the aorta together with `CAP = 2.0` (twice the vessel's own calibre)
is the mechanism behind the operator's *"way too voluptuous"* observation on
the live `/helix` render.

This corroborates §9.7 from the other direction: P-6 found the vessel
constants were empirical clamps from their first commit with no sequence ever
tried and replaced. They are not a tuning history — they are scaffolding
around a missing topology.

### 11.2 It is the same bug class `kurvenlineal.rs` already fixed

`geo/src/kurvenlineal.rs` documents the **intra-family needle bug**: seeding a
fresh `CurveRuler` per lattice cell made adjacent cells draw uncorrelated
phases, so the residue *"stepped discontinuously at every cell boundary … every
cell its own isolated spike, never connected to its neighbour"*. The fix was to
sample at lattice corners and interpolate — C1-continuous **across** boundaries
(inter-family), not a blur.

Per-bin independent radius estimation is the same failure one domain over:
**needle field → balloon field.** Both are intra-family independence where the
feature is continuous; both are fixed by inter-family continuity along the
feature.

### 11.3 What already exists to build on

- `geo/src/osm_read.rs` **already reads OSM ways** — `RawWay`, ordered
  `way.refs`, `Element::Way(w)`. It is filtered to `building=*` and closed into
  rings (`refs[..len-1]` drops the repeat), but the way machinery is there.
- `geo/src/bin/osm_helix.rs` bakes those into the **same BSO2 `/helix` wire**
  (`encode_bso2`) that the body uses — *"the geo counterpart of
  `body-soa-wire`"*.
- `cockpit/src/GeoHelix.tsx` is a verbatim `BodyHelix` fork sharing the **same
  BSO2 decoder**. Body and map are already one substrate; vessels-as-ways
  closes the loop rather than opening a new one.

### 11.4 Why this fits the §10 addressing, not fights it

§10.5 measured that the venous depth spine is `Subdivision of…` /
`Tributary of…` — 537 + 146 nodes, single-parent, ~5 children each. Those names
are *linear* language: a tributary is a segment of a run, not a taxon. The is_a
tree is currently encoding a **linear structure as a hierarchy**, which is why
the deepest chain in the whole ontology (24, §10.2) is a venous one. A way
model carries that run natively and leaves is_a to carry type.

### 11.5 Open items

- **O-11a — no ordered centerline exists in the source.** BodyParts3D ships
  meshes, not ways. The ordered node list has to come from somewhere: derived
  once at bake time and STORED (so the reconstruction happens once, not per
  render), or authored. Until it exists, this is a proposal, not a plan.
- **O-11b — the source ships NO connectivity relation at all.** This is the
  hard item, and it is harder than a missing-direction problem. Checked in both
  directions (operator, 2026-09-07): `conventional_part_of` rows carry whole AND
  part, so the aorta chain reads intact upward —
  `cardiovascular system → aorta → {thoracic aorta → {arch of aorta, ascending
  aorta}, descending aorta}` (and `thoracic aorta` has TWO parents, `aorta` and
  `thorax`, i.e. one of the §10.3 bridges). An earlier draft of this section
  said *"arch of aorta has zero parts"*; that is true DOWNWARD but misleading —
  the arch is simply a leaf of the containment tree.

  What is actually absent is a **kind** of relation, not a direction. All six
  aorta rows are mereological. **No branch, junction, upstream or downstream
  edge exists anywhere in the dataset**: nothing leaves the arch — no
  brachiocephalic, no left common carotid, no left subclavian. BodyParts3D
  ships taxonomy (`FMA.csv` is_a), containment (`conventional_part_of`) and
  composition (`composite_parts`), and **zero connectivity**. Only 8
  `bifurcation` NODES exist in all of FMA (`Bifurcation of aorta`, the carotid
  and iliac ones, trachea, pulmonary trunk, tooth root) — and a node named for a
  junction is not an edge between two ways.

  A way network is DEFINED by connectivity; nesting cannot substitute for it.

  **⊘ CORRECTED same-day (2026-09-07).** The paragraph above said connectivity
  could not be derived from the shipped relations. That is WRONG as stated, and
  the correction matters because it makes §11 cheaper, not more expensive.
  Connectivity is not declared as a RELATION, but it is **flattened into the
  labels** and is recoverable: **74 % of all FMA labels (77 677 of 104 697)**
  have the form `<Predicate> of <Object>`, and the object half resolves back to
  a real node by name at high rates:

  | flattened predicate | nodes | object resolves |
  |---|---|---|
  | `Subdivision of` | 537 | **95 %** |
  | `Periosteum of` | 1 598 | 93 % |
  | `Segment of` | 552 | 91 % |
  | **`Tributary of`** | 146 | **90 %** |
  | `Wall of` | 1 296 | 89 % |
  | `Lumen of` | 984 | 88 % |
  | `Tendon of` | 649 | 87 % |
  | `Trunk of` | 4 638 | 86 % |
  | `Vasculature of` | 1 582 | 84 % |
  | **`Branch of`** | 784 | 46 % |

  Recovered edges are real: `Branch of deep cervical artery` → `Deep cervical
  artery`; `Tributary of femoral vein` → `femoral vein`. That yields roughly
  **490 vessel connectivity edges** (359 `branch_of` + 131 `tributary_of`)
  without any new source.

  What remains true: BodyParts3D itself ships only ONE relation — its README
  calls it *"conventional inclusion"* — plus names, composite definitions and
  meshes; it is a *"dictionary-type database"* of shapes and positions, so the
  absence there is by design. And only **8** `bifurcation` NODES exist in FMA; a
  node named for a junction is still not an edge between two ways.

  So the corrected position: the junction graph is **partially derivable by
  label parsing** (a string join, no new data), not unobtainable. `Branch of` at
  46 % is the weak spot — most misses are compound objects — and that number,
  not the existence of connectivity, is what gates the way model.

- **O-11d — the predicates ask for other TYPES, and they are already counted.**
  Each flattened predicate names a type pair the substrate does not yet model:
  `Periosteum of <bone>` and `Compact bone of` / `Trabecular bone of` (1 640 /
  1 711) are tissue layers ON a bone; `Lumen of <tube>` (984) is the hollow;
  `Wall of <organ>` (1 296); `Tendon of <muscle>` (649); `Vasculature of
  <organ>` (1 582) is an organ's vessel set. These are the "other types" a v4
  bake would have to admit or deliberately collapse — and unlike the ordered
  centerline, they need no acquisition: they are derivable today from labels
  already in `FMA.csv`.
- **O-11c — falsifier.** Replace the derived centerline with a stored ordered
  way for ONE vessel (the aorta is the natural candidate: `Bifurcation of
  aorta` exists, and `thoracic aorta` → `ascending aorta` + `arch of aorta`
  gives a real segment chain) and check whether `CAP` and `PCTL` can be
  removed without the balloon returning. If the clamps are still needed with a
  given centerline, the diagnosis in §11.1 is wrong.
