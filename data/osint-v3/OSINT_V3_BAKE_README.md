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
