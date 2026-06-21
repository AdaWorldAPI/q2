# Hydration Showoffs — lance-graph Live Features

## What's Implemented (295 tests, not stubs)

### 1. HHTL Cascade Search
`cascade_search()` — 4-stage progressive neighborhood search.

```
HEEL: load 1 scent vector, compare 10K entries      → 50 survivors   (20μs)
HIP:  load 50 survivors' scents, 2nd hop             → 50 survivors   (500μs)
TWIG: 3rd hop                                        → 50 survivors   (500μs)
LEAF: load cold fingerprints, exact verification     → final results  (100μs)

TOTAL: 1.2MB loaded, 1.1ms, 200K nodes explored, 3 hops
```

**Cockpit showoff**: Status bar shows `HHTL: 200K explored · 3 hops · 1.1ms`.
Inspector shows which stage found the selected node (HEEL/HIP/TWIG/LEAF).
Graph colors nodes by discovery stage (bright = HEEL, fading = deeper hops).

### 2. Semiring Path Algebra (7 variants)
Not just "shortest path" — algebraically composable traversals:

| Semiring | Multiply | Add | Use Case |
|----------|----------|-----|----------|
| XorBundle | XOR | majority-vote | Path composition |
| BindFirst | XOR | first non-empty | BFS traversal |
| HammingMin | Hamming dist | min | Shortest path |
| SimilarityMax | similarity | max | Best match |
| Resonance | XOR bind | best-density | Query expansion |
| Boolean | AND | OR | Reachability |
| XorField | XOR | XOR | GF(2) algebra |

**Cockpit showoff**: Graph toolbar gets a semiring selector dropdown next to
FORCE/CIRCULAR/HIERARCHY. Pick "HammingMin" → edges glow by shortest path
distance. Pick "Resonance" → nodes pulse by resonance density. Pick "Boolean"
→ reachable nodes highlight, unreachable dim.

Neo4j GDS has PageRank, Louvain, betweenness. We have COMPOSABLE ALGEBRA.
You chain semirings. They run canned algorithms.

### 3. SPO Triple Store + NARS Truth Values

Every edge carries `TruthValue { frequency, confidence }`:
- `frequency` ∈ [0,1] — how often the relation holds
- `confidence` ∈ [0,1] — how certain we are
- `expectation()` = c * (f - 0.5) + 0.5 — single scalar truth

Query functions:
- `query_forward(subject)` → all edges from this node
- `query_forward_gated(subject, min_truth)` → only confident edges
- `query_reverse(object)` → all edges TO this node
- `query_relation(predicate)` → all edges of a type
- `walk_chain_forward(start, hops)` → multi-hop traversal

**Cockpit showoff**: Inspector shows NARS truth on each edge:
```
→ DEVELOPED_BY  Palantir   f:0.95 c:0.87 e:0.89
→ DEPLOYED_BY   US DoD     f:0.80 c:0.45 e:0.64
← MONITORS      Prometheus  f:1.00 c:1.00 e:1.00
```
Edge thickness = confidence. Edge opacity = frequency.
A "truth gate" slider filters edges below threshold.

### 4. Merkle Integrity Verification

Each node has a `MerkleRoot` — a 48-bit seal over its fingerprint planes.
`verify_lineage()` checks if a node's ancestry is intact.
`verify_integrity()` checks if the node itself hasn't been tampered with.

```rust
match bind_space.verify_integrity(node_addr) {
    VerifyStatus::Valid      => "✓ clean",
    VerifyStatus::Corrupted  => "✗ tampered",
    VerifyStatus::NotFound   => "? unknown",
}
```

**Cockpit showoff**: Inspector shows a green checkmark or red warning.
Graph nodes get a subtle shield icon for verified. During presentations:
"Every node has a cryptographic seal — you can prove this data hasn't been
modified since ingestion."

### 5. Fingerprint Similarity (rabitQ-compatible)

`label_fp("Palantir")` → 512-bit fingerprint
`hamming_distance(fp_a, fp_b)` → how different two entities are
`dispatch_hamming()` → SIMD-accelerated on ndarray (AVX2/AVX-512)

**Cockpit showoff**: Type a label in the query bar →
"Find similar" button → HHTL cascade finds the 10 most similar nodes
by fingerprint distance. Graph highlights them with halos sized by similarity.

### 6. GraphBLAS-style Sparse Matrix Ops

`GrBMatrix` with CSR/CSC dual storage. `GrBVector` for sparse vectors.
Element-wise add/multiply with custom semirings.
Matrix-vector multiply for one-hop neighborhood expansion.

**Cockpit showoff**: "Expand neighborhood" button on selected node.
Internally does `A × v` where A is the adjacency matrix and v is the
selection vector. Shows the algebraic operation in the status bar:
`A ⊗ v → 12 neighbors (SimilarityMax semiring, 0.3ms)`

---

## What Neo4j GDS Has That We Replace

| Neo4j GDS | lance-graph equivalent |
|-----------|----------------------|
| PageRank | SimilarityMax semiring on A^n × v |
| Louvain community | Resonance semiring clustering |
| Shortest path (Dijkstra) | HammingMin semiring chain walk |
| Node similarity | Fingerprint hamming distance |
| Link prediction | NARS truth value extrapolation |
| Graph projection | Already in-memory (blasgraph columnar) |
| Centrality | GrBMatrix diagonal from A × A^T |

We don't have canned algorithms. We have the ALGEBRA that composes them.
A presentation slide: "Neo4j runs PageRank. We run any semiring you define."

---

## Implementation in the Cockpit

### Minimal (3 features, 1 day)

1. **Truth values on edges**: Inspector shows f/c/e for each connection.
   Edge rendering uses opacity=frequency, width=confidence.
   
2. **Semiring selector**: Dropdown in graph toolbar. Switches edge
   coloring/weighting between HammingMin, SimilarityMax, Resonance.

3. **HHTL status**: Status bar shows cascade metrics after each query.

### Medium (5 features, 3 days)

4. **Merkle verification**: Shield icon on verified nodes. Click to verify.

5. **Fingerprint similarity search**: "Find similar" button in inspector.
   Highlights top-K by hamming distance.

### Full (all 6, 1 week)

6. **GraphBLAS expand**: "Expand" button does A × v with selected semiring.
   Shows algebraic notation in status bar.

---

## How To Wire

The aiwar data needs to be ingested into the SPO store with truth values
assigned. For the demo, assign truth values from the enrichment Cypher files:

- Edges from `aiwar_enrichment_grok_verified.cypher` get c:0.95
- Edges from base `aiwar_full.cypher` get c:0.80
- Edges from `aiwar_enrichment_epstein_v39_patch.cypher` get c:0.60
- Enrichment version number maps to confidence (later = less certain)

The HHTL cascade needs neighborhood vectors precomputed from the graph.
For 221 nodes this is trivial — precompute at startup, takes <100ms.

The semiring selector just changes which `HdrSemiring` variant is used
for edge weight computation. The graph data stays the same.

Merkle seals are computed at ingestion time — one `MerkleRoot::from_fingerprint()`
per node. Stored alongside the node. Verification is O(1) per node.
