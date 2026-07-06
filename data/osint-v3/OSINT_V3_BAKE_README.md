# OSINT-V3 SoA Bake — aiwar-neo4j-harvest → 6×(8:8) substrate

Baked from `AdaWorldAPI/aiwar-neo4j-harvest` (611 nodes) into the V3 SoA
`6×(8:8) part_of:is_a` centroid cascade (CAM-PQ-shaped) under the canonical
V3 classid `0x0701_1000` (`CLASSID_OSINT_V3` — OSINT domain `0x07`, appid
`:01` q2, V3 marker `0x1000` LOW).

**ONE OSINT class, deliberately.** Stakeholder/institution nodes and person
nodes share this single classid so the *instrument-of-power* edge — a
stakeholder supplying a person the means to execute power (McClelland nPow) —
and all stakeholder↔person interaction reasoning live *inside* the class,
never blocked by a class boundary. GUID1 is the situational/institutional
facet (AIRO need/offer/impact); GUID2 is the personal facet (McClelland
need/motive/Rubicon). Facets scale by **adding a GUID**, never by splitting
the class.

## GUID1 is 12× is_a — kind is read by position, never labelled

GUID1's 12 bytes are **12 orthogonal is_a dimensions**, one per position; the
schema (the ClassView field card) knows which position is which field. A node
stores only the **value** at each position — never a label. So kind (person /
institution / system) is not a separate property to add: it is already read
off the 12-is_a vector by position. There is no "garbage" byte — a value at any
position is that dimension's is_a value for the node. (An earlier pass wrongly
declared 423 nodes "misclassified" by assuming a node's kind restricts which
positions apply; it does not — every node carries a value in every dimension.)

> **classid flip (2026-07-04).** Rebaked to the post-2026-07-02 canon-high
> order. Legacy stored forms were `0x1000_0700` (System) / `0x1000_0701`
> (Person); both normalize to the single `0x0701_1000` (position swap + appid
> `:00→:01`), collapsing the wrong stakeholder/person **class split** into the
> one OSINT class. **Only the classid u32 changed** — every rail byte
> (`6×(8:8)` tiers) is byte-identical to the original bake. Reference artifact;
> the runtime OSINT path (`osint_gotham.rs` → `osint_scene.soa`) does not yet
> read these rails — wiring it (facet by GUID slot, kind by reading the 12×
> is_a per position) is the follow-up this flip clears the way for.

> **hi/lo tier-order correction (2026-07-06).** Rebaked so the stored tier
> bytes match the **declared field order**. The original bake wrote each
> `6×(8:8)` tier as a little-endian `u16`, landing the two bytes of every
> `hi:lo` pair in memory as `(lo, hi)`. But the field cards in
> `crates/cockpit-server/src/osint_classview.rs` (`OSINT_SYSTEM_FIELDS` 12,
> `OSINT_PERSON_FIELDS` 5) and the contract's position law
> (`lance-graph-contract` `ClassView::facet_rows`: **field position i = facet
> byte i**, `hi:lo` reading) read position i as facet byte i. The two disagreed
> by a byte swap within each tier pair. Decoding the pre-correction bake against
> the codebook's declared order landed only **16/611 GUID1** and **52/133
> GUID2** values in-vocabulary; swapping each tier pair's two bytes raises that
> to **609/611 GUID1** and **133/133 GUID2**. (GUID2 "before" = 52/133 counts
> the 5 McClelland fields 1-based ignoring the TWIG `_` padding byte; the
> stricter count that also requires the padding byte to be empty is 0/133 →
> 133/133, because pre-correction the *motive* value sat in the padding slot,
> not in its own position.) Per operator ruling, the **DATA was fixed (this
> rebake), not the field declarations**. Only the tier bytes changed: the
> `row_id`, the `classid` u32 (`0x0701_1000`, LE `00 10 01 07`), the header, and
> every non-GUID node field are byte-identical to the prior bake; `unswap(new) ==
> old` for all 611 rows. **This supersedes the 2026-07-04 note's claim that
> "every rail byte is byte-identical to the original bake"** — that held across
> the classid flip but not across this hi/lo correction.
>
> The remaining **2/611 GUID1** out-of-vocab after correction are genuine
> codebook↔harvest gaps, not swap artifacts: `DreamSecurity` MLType=30 (vocab
> max 29) and `Shoebox` civicUse=17 (vocab max 16) — each one past its vocab
> tail; reconcile the vocab against the harvest to close them.
>
> **McClelland indexing (GUID2 persons): 1-indexed against `mcclelland_vocab`.**
> After correction every person field's values fall in `1..=len` with the
> observed max equal to the vocab length (stage 2..5 / len 5, need 1..3 / len 3,
> receptor/rubicon/motive 1..5 / len 5), min nonzero ≥ 1, and no meaningful
> zeros — so value `v` resolves to `vocab[v-1]` (matching `airo_vocab`'s explicit
> 1-based integers). Under 0-indexing 26/133 would overflow their arrays. The
> TWIG `_` padding byte (GUID2 tier position 5) is `0` for all 133 persons after
> correction; pre-correction it wrongly carried the motive value.
>
> **Provenance.** There is no in-repo baker for this artifact (it is baked from
> the external `AdaWorldAPI/aiwar-neo4j-harvest`, which is not present and cannot
> regenerate offline). The correction was applied by the deliberate one-shot
> transform `scripts/osint_v3_rebake_hilo.py` (checked in): classid bytes 0..4
> untouched, tier bytes 4..16 swapped pairwise per GUID, `.soa` and
> `osint_v3_nodes.json` rewritten consistently and cross-verified byte-for-byte.
> The script guards against double-application (a pairwise swap is its own
> inverse) by refusing to run unless the data is in the pre-swap broken state.

## Assets
- `osint_v3.soa` — binary SoA. Header `OSINTV3\0` + `ver(u16=3) count(u32) stride(u16=36)`,
  then `count` records of `row_id(u32) | GUID1(16B) | GUID2(16B)` little-endian.
- `osint_v3_codebook.json` — the aiwc.ods controlled vocab → byte map, tier layout, classids.
- `osint_v3_nodes.json` — per-node index (row, id, name, guid1/guid2 hex, is_person).

## GUID1 (identity+location, 6×(8:8), CAM-PQ-shaped)
`[classid u32][HEEL|HIP|TWIG|LEAF|family|identity]` — each tier `hi:lo` byte-pair:
HEEL currentStatus:type · HIP militaryUse:civicUse · TWIG MLTask:MLType ·
LEAF purpose:capacity · family output:impact · identity stakeholder:airo_type.

## GUID2 (relationships, McClelland, persons)
`[classid u32][stage:need|receptor:rubicon|motive:_|...]`.

## Findings (this bake)
- Codebook max cardinality = 30 → every dim fits u8 (u16 is 8× overkill).
- 45 dual-use HIP basins (militaryUse:civicUse prefix) emerge with zero hub nodes —
  the AIRO "island" axes collapse into prefix-adjacency.
- 78 collision groups = same-type basins (finance/politician/…), u32 row = local identity.
- 133 persons carry a McClelland GUID2.

Provisional: enrichment for unwired dims was inferred by a 15-agent sweep against
the aiwc.ods controlled vocabulary; base dims (type/currentStatus/stakeholder) are
from the harvest. Reconcile against ground-truth cypher enrichments before locking.
