# cpic semiring — retrieval IS inference (frontier probe #1)

> POC over published CPIC rules — **NOT clinical decision support.**

`cargo run --release --bin semiring -- [data_dir]`

The research frontier-map's highest-leverage `[G]` probe (Dudzik & Veličković, *GNNs are
Dynamic Programmers*, [2203.15544](https://arxiv.org/abs/2203.15544): message-passing **is** DP
parameterized by a semiring). It makes the claim *executable*: the `diplotype → phenotype →
recommendation` chain `cpic reason` computes is a single **semiring transitive closure**
(Floyd-Warshall) over the CPIC edge adjacency — and **swapping only the semiring switches the
reasoning mode on the same graph**:

| semiring | `(⊕, ⊗)` | reasoning mode |
|---|---|---|
| **Boolean** | `(∨, ∧)` | reachability — "is a recommendation derivable at all?" |
| **MinPlus** | `(min, +)` | cheapest path (hops) — shortest justification |
| **Nars** | `(choose-by-expectation, deduction)` | CPIC-authoritative confidence `f=f₁f₂, c=f₁f₂c₁c₂` |
| **MaxTimes** | `(max, ·)` | most-likely path (Viterbi over edge expectations) |

The edge truths are read from the **real** CPIC data (the clopidogrel rec's `classification` +
the CYP2C19 pair's `cpiclevel`, mapped exactly as `reason.rs`).

## Output (real run)

```text
CPIC edge phenotype→rec read from data: classification=Strong, cpiclevel=A
adjacency:
  CYP2C19 *2/*2 (diplotype) --t=1.00/0.85--> CYP2C19 Poor Metabolizer
  CYP2C19 Poor Metabolizer  --t=0.95/0.95--> clopidogrel recommendation

same adjacency, the reasoning mode is the semiring:
  Boolean   reachability        reachable
  MinPlus   cheapest path       2 hops
  Nars      CPIC confidence     f=0.950 c=0.767
  MaxTimes  most-likely path    p=0.858

keystone: Nars closure = reason.rs deduction?  f=0.950 c=0.767 vs f=0.950 c=0.767 → ✓ IDENTICAL
```

## The keystone

The **Nars** semiring closure reproduces `cpic reason`'s `(f,c)` **exactly** (`f=0.95, c=0.767`).
That is the proof: *retrieval and reasoning are the same operation* — `reason.rs`'s hand-written
2-hop deduction and a generic graph closure under the NARS semiring compute the identical answer.
The reasoning mode was never in the traversal; it was in the algebra. A unit test locks the
identity; a second test confirms the four semirings give four distinct answers on one graph.

## Honest scope

- The closure here is a self-contained Floyd-Warshall so the standalone `cpic` crate stays
  dep-light. **In production this is a lance-graph GraphBLAS matrix walk** over the SPO adjacency
  — the same algebra at scale (the substrate already ships the semirings). This bin is the
  *proof of the identity*, not the production engine.
- The demo runs the one CYP2C19→clopidogrel chain end-to-end; the algebra is general (any
  `pgx_edges` adjacency), but a full-graph closure is `O(n³)` and would be run as a sparse
  GraphBLAS sweep upstream, not Floyd-Warshall here.
