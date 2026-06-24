# cpic ingest — CPIC pharmacogenomics on the V3 `(part_of : is_a)` GUID

> POC over published CPIC rules — **NOT clinical decision support** (that needs the
> full guideline text + pharmacist review). This slice mints addresses and wires the
> graph; it does not give dosing advice.

`cargo run --release --bin ingest -- [data_dir] [out_dir] [max_diplotype_rows]`

Positional args; when run from `cpic/` the defaults are `data out 4000`. Example:
`cargo run --release --bin ingest -- data out 4000`.

Reads the CPIC JSON tables and mints the **canonical 16-byte `NodeGuid`** for every
entity, emitting `out/pgx_nodes.tsv` + `out/pgx_edges.tsv`. The GUID layout is
byte-identical to `lance_graph_contract::canonical_node::NodeGuid`
(`classid·HEEL·HIP·TWIG·family·identity`, all little-endian), the same canon the `fma/`
crate's `converge` bin targets — only the domain changes.

## The `(part_of : is_a)` tile

Each of the three HHTL tiers (HEEL/HIP/TWIG) is an **8:8 tile**:

```text
        HEEL              HIP               TWIG
   ┌────────┬───┐   ┌────────┬───┐   ┌────────┬───┐
   │ part_of│isa│   │ part_of│isa│   │ part_of│isa│
   └────────┴───┘   └────────┴───┘   └────────┴───┘
     HIGH    LOW       HIGH   LOW       HIGH   LOW
   = WHERE  = WHAT
   partonomy taxonomy
```

- **high byte = `part_of`** (WHERE — the gene-family basin / partonomy): cumulative
  FNV-1a prefix of the entity's part-of distinguished name. Siblings sharing a prefix
  share the high byte at that tier → **prefix-routable on the partonomy**.
- **low byte = `is_a`** (WHAT — the functional/metabolizer/ATC taxonomy): cumulative
  FNV-1a prefix of the is-a distinguished name → **prefix-routable on the taxonomy**.

Both hierarchies are *already in the CPIC columns* — nothing is invented:
`allele.clinicalfunctionalstatus` is the functional `is_a` ladder, the gene→family
partonomy is the `part_of` axis, and ATC drug codes are a second ready-made `is_a`
cascade.

## classid — pharmacogenomics domain `0x0C`

`classid` prefix-routes the entity kind (the OGAR `ClassView` dispatch). We use domain
`0x0C` (anatomy was `0x0A`):

| classid | entity | part_of (high) | is_a (low) |
|---|---|---|---|
| `0x000C0001` | gene | `pharmacogenome / CYP / CYP2 / CYP2C / CYP2C19` | `entity / gene` |
| `0x000C0002` | allele (`*2`) | gene partonomy ++ allele | `allele / no_function` |
| `0x000C0003` | diplotype (`*2/*2`) | gene ++ `diplotypes` ++ dip | `diplotype / homozygous` |
| `0x000C0004` | phenotype | gene ++ `phenotypes` ++ result | `phenotype / poor_metabolizer` |
| `0x000C0005` | drug | `pharmacogenome / drugs / name` | `drug / N / N06 / N06A / N06AA` |
| `0x000C0006` | recommendation | `recommendations / g{id} / drug` | `recommendation / strong` |

- **family (u24)** = the basin: the gene symbol (gene/allele/diplotype/phenotype), the
  ATC root letter (drug), or `rec:g{id}` (recommendation). Groups all of a gene's
  entities into one masked-load basin.
- **identity (u24)** = a per-basin golden-ratio multiplicative mint
  (`counter × 0x9E3779B9 & 0xFFFFFF`) — a bijection over the basin counter, so **0
  collisions by construction** within a basin (and the full 16-byte key is asserted
  unique globally).

## Both axes route — the cascade demo (real output)

```text
allele         guid                                  HEEL  HIP   TWIG  family
CYP2C19 *2     000c0002-c358-ec6f-f76f-d69bfe558274  c358  ec6f  f76f  d69bfe   (No function)
CYP2C9  *2     000c0002-c358-ecd8-f7d8-fbd6cbd5c622  c358  ecd8  f7d8  fbd6cb   (Decreased function)
CYP2C19 *1     000c0002-c358-ec07-f707-d69bfe3c1176  c358  ec07  f707  d69bfe   (Normal function)
CYP2D6  *4     000c0002-c358-ec6f-f76f-1066f96f7ace  c358  ec6f  f76f  1066f9   (No function)
```

- **part_of HIGH bytes** `c3 / ec / f7` = `pharmacogenome → CYP → CYP2`, shared by all
  four — sibling genes route together at HHTL granularity.
- **is_a LOW bytes** `58` = allele, then the function class: `no_function = 6f`,
  `decreased = d8`, `normal = 07`.
- **`CYP2C19*2` and `CYP2D6*4`** (both no-function, both CYP2 subfamily) share the
  **entire** path `c358-ec6f-f76f` — only the **family basin** (the gene) differs. That
  is the cascade working: HHTL routes to the *neighborhood*, family+identity is the
  basin-local key. (Functional classes verified against `allele.clinicalfunctionalstatus`.)

## Edges

| rel | from → to | source |
|---|---|---|
| `part_of` | allele / diplotype / phenotype → gene | partonomy (in-family) |
| `pair` | gene → drug | `pair` table (the connected_to / out-of-family edge) |
| `recommends` | phenotype (or allele-status) → recommendation | `recommendation.lookupkey` join |
| `targets` | recommendation → drug | `recommendation.drugid` |

The `lookupkey` join works for **both** lookup methods: phenotype genes (key matches
`gene_result.result`, e.g. `Poor Metabolizer`) and allele-status genes (key matches
`*57:01 positive`). Worked clinical chain present in the output:

```text
HLA-B *57:01 positive ──recommends──▶ g100421 [HLA-B:*57:01 positive] → Strong ──targets──▶ abacavir
```

i.e. the real CPIC abacavir-hypersensitivity decision, entirely as `(part_of:is_a)`
nodes + edges. This is exactly the chain the `reason` slice will traverse with NARS.

### Deferred: `maps_to` (diplotype → phenotype)

A diplotype's `functionphenotypeid` references a separate **`function_phenotype`** table
(194 distinct ids, **disjoint id space from `gene_result.id`** — verified 0 overlap)
that is **not in this CPIC dump**. So `diplotype --maps_to--> phenotype` is deferred — it
is a one-line join the moment that table is provided. Diplotype nodes still mint with
correct GUIDs; only the edge is missing.

## Scale (the scalability preview for the `scan` slice)

With the diplotype table present locally:

```text
minted 114,410 GUIDs, 0 collisions
  genes 132 · alleles 1349 · phenotypes 101 · drugs 323 · recommendations 2159
  diplotypes minted 110,346  (the combinatorial layer: N(N+1)/2 per gene)
edges 10000  (part_of 5450 · pair 573 · recommends 1818 · targets 2159)
  part_of = allele/phenotype→gene (1450) + recorded diplotype→gene (4000)
```

The **110k diplotypes** are the natural large-N target for the `scan` slice: a cohort
key-only prefix-scan (`classid=phenotype, family=CYP2C19, identity=PM`) where the bulky
`gene_result.consultationtext` is the **value column that stays compressed and never
decodes** — the canon's "the key prerenders with zero value decode," with a genuinely
skippable value.

## Data

Committed: the 8 small CPIC tables (`gene`, `allele`, `allele_definition`,
`gene_result`, `pair`, `guideline`, `recommendation`, `drug`). The big
`gene_result_diplotype.json` (21.8 MB, the diplotype enumeration) and
`allele_frequency.json` (27 MB, the population-frequency value column) are **fetched
separately** (gitignored) — the ingest runs fine without the diplotype file (it skips
that layer with a notice). CPIC data: © CPIC / clinpgx, freely available.
