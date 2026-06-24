# cpic scan — 1M-patient cohort: key-only vs value-decode

> POC over published CPIC rules — **NOT clinical decision support.**

`cargo run --release --bin scan -- [n_patients=1000000] [reps=5]`

The scalability headline for the V3 substrate. Each synthetic patient is the canon's
**`NodeRow` = key(16) + value(496) = 512 bytes**:

- **key** = a phenotype `(part_of:is_a)` GUID — `classid = PHENOTYPE`, gene-family basin,
  per-patient identity. 16 bytes.
- **value** = the patient's `gene_result.consultationtext` (the paragraph-long clinical
  blurb) packed into the 496-byte value slab — the bulky, genuinely-skippable column.

Stored **SoA** (separate key column + value column), the cohort query *"how many of N patients
are CYP2C19 Poor Metabolizer?"* runs two ways:

| scan | what it touches | reads |
|---|---|---|
| **key-only** (prefix-route) | 13 key bytes/row (classid + HEEL/HIP/TWIG + family) | the 16 MB key column — cache-resident |
| **value** (decode slab) | all 496 value bytes/row (models decompress) + match | the 496 MB value column — RAM-bound |

## Measured (1M patients, best-of-5)

```text
key column 16 MB · value column 496 MB
  key-only (prefix-route)   10063 hits    817 M rows/s · 13.1 GB/s   [touched 16 B/row]
  value    (decode slab)    10063 hits      3 M rows/s ·  1.6 GB/s   [touched 496 B/row]

  speedup  251× wall-clock   (31× memory floor: 496/16 = the value column never read)
```

Both scans return the **identical 10,063-patient cohort** (asserted) — same answer, ~250×
apart. This is the canon's *"the key prerenders nodes with zero value decode"* at cohort scale,
with a value (clinical consultation text) that is genuinely worth skipping: to answer the
cohort question you route on the `(part_of:is_a)` key and **never decompress a single consult
blob**.

## Honest reading of the numbers

- **31× is the memory floor** — the pure column-size ratio (496/16). The key column never
  reads the value column, so that ratio is unavoidable structure.
- **251× is the realized speedup** because the value path models a real **decode**: it touches
  every one of the 496 bytes per row (a rotate-xor reduction standing in for Lance
  decompression) before matching. That's compute + memory, which is what decompressing a
  columnar value actually costs — so the gap is wider than the bandwidth floor.
- Numbers are from this sandbox (`x86-64`); absolute throughput varies by host, the **ratio**
  is the portable result.
- The cohort is **synthetic** (1M patients drawn over the 101 real CPIC phenotypes); the real
  diplotype space (~110k, see `INGEST.md`) is the natural source of real keys at this scale.
