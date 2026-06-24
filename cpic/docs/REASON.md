# cpic reason — NARS over the real CPIC graph

> POC over published CPIC rules — **NOT clinical decision support.**

`cargo run --release --bin reason -- <gene> <diplotype|phenotype> <drug>` (no args → 4 demos)

A scenario `{gene, diplotype|phenotype, drug}` is chained `diplotype → phenotype →
recommendation` by **2-hop NARS deduction** (`f = f1·f2, c = f1·f2·c1·c2` — the same algebra
as the cockpit `clinical.rs` demo and `graph_engine`), but every edge is a **published CPIC
fact** and the confidence is **authoritative**, not hand-tuned:

| edge | truth source |
|---|---|
| diplotype → phenotype (`t1`) | direct phenotype input ⇒ c≈0.99; else the simple allele-function rule (homozygous c=0.85, mixed c=0.70) |
| phenotype → recommendation (`t2`) | **f from `recommendation.classification`** (Strong 0.95 / Moderate 0.8 / Optional 0.6); **c from the pair `cpiclevel`** (A 0.95 / B 0.85 / C 0.65 / D 0.45) |

The phenotype → recommendation match is the real `recommendation.lookupkey` join (works for
both metabolizer genes and allele-status genes like HLA-B). The routable `(part_of:is_a)` GUID
prefix of each chain node is printed.

## The four demo cases (real output)

```text
CYP2C19 *2/*2 + clopidogrel   → Poor Metabolizer → g100411 Strong (cpic A)
   ⊢ f=0.950 c=0.767   CPIC: "Avoid clopidogrel if possible. Use prasugrel or ticagrelor…"

HLA-B *57:01 positive + abacavir  → (direct, 1-hop) → g100421 Strong
   ⊢ f=0.950 c=0.893   CPIC: "Abacavir is not recommended"

TPMT *3A/*3A + azathioprine   → Poor Metabolizer → g100428 Strong
   ⊢ f=0.950 c=0.767   CPIC: "Consider alternative nonthiopurine immunosuppressant therapy."
   ⚠ MULTI-GENE guideline (2 genes) — single-gene deduction is partial.

CYP2C9 *1/*1 + warfarin   → Normal Metabolizer → no simple phenotype→rec
   ⚠ COMPLEX guideline g100425 (CPIC note): "Warfarin recommendation does not follow
     simple diplotype to phenotype translation. Read the guideline text…"
```

The last two are the point as much as the first two: the POC **does not fabricate** an answer
for the guidelines CPIC itself flags as not-simple (warfarin's `notesonusage`, any multi-gene
`lookupkey`). It surfaces the flag and stops.

## Honest scope

- **diplotype → phenotype uses a transparent simple rule** (functional-status ranks
  no=0 / decreased=0.5 / normal=1 / increased=2, summed → metabolizer band), matched against
  CPIC's real phenotype vocabulary for the gene. This *approximates* the CYP2D6 activity-score
  scheme and the allele-count genes; it is **not** the authoritative `function_phenotype` table
  (deferred — see `INGEST.md`). The `t1` confidence encodes that uncertainty, and genes CPIC
  marks complex are flagged rather than auto-resolved.
- Confidence weights (classification/cpiclevel → f/c) are a defensible mapping, not a CPIC-
  published number — they are stated here so they can be tuned.
