# OSINT-V3 SoA Bake — aiwar-neo4j-harvest → 6×(8:8) substrate

Baked from `AdaWorldAPI/aiwar-neo4j-harvest` (611 nodes) into the V3 SoA
`6×(8:8) part_of:is_a` centroid cascade (CAM-PQ-shaped) under classid
`0x1000_0700` (System) / `0x1000_0701` (Person, McClelland).

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
