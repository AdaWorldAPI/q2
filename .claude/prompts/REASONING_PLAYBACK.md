# Reasoning Playback — 3 Generations Ahead

## What Already Exists (implemented, tested)

```rust
// NARS truth revision — combines evidence from multiple sources
truth_a.revision(&truth_b) → TruthValue { frequency, confidence }

// Chain walk — multi-hop greedy traversal with cumulative distance
spo_store.walk_chain_forward(start, radius, max_hops) → Vec<TraversalHop>

// Versioned graph — Lance ACID snapshots, time-travel queries
versioned_graph.at_version(n)  // read any historical state
graph_seal_check(v1, v2) → Wisdom | Staunen  // did learning occur?

// ZeckF64 progressive encoding — 94% precision in 1 byte
byte 0: scent (7 close-bits)     → ρ=0.937, 6144× compression
bytes 0-7: full resolution        → ρ=0.982, 862× compression
```

## Feature 1: Temporal Play Button (1 day)

The 30 Cypher enrichment files ARE timestamped versions:
```
v00: aiwar_full.cypher                    (base graph, 221 nodes)
v01: aiwar_enriched.cypher                (first enrichment)
v31: aiwar_enrichment_epstein_v31_patch   (evidence accumulates)
v32: aiwar_enrichment_epstein_v32_patch   (more evidence)
...
v42: aiwar_enrichment_v42_bilderberg      (latest)
```

Each file = one `commit_encounter_round()` in the versioned graph.
Each version gets a Merkle seal. Between versions: `Wisdom` (nothing changed)
or `Staunen` (new learning occurred).

### The Play Button

```
[◀ ⏸ ▶ ⏩]  ─────●────────────── v42
              v00  v01  v31    v42
```

Press play:
- v00 loads: 221 nodes, 356 edges appear in cockpit
- v01: 45 new edges fade in (enrichment). Status: `Staunen — 45 new edges`
- v31: Epstein connections appear as dashed lines (c:0.60)
  - NARS `revision()` runs: edges that were c:0.60 in v31 get revised 
    UP to c:0.72 by v35 (more corroborating evidence)
- v42: full graph. Status bar: `Wisdom — stable since v40`

The graph GROWS in front of you. Truth values STRENGTHEN as evidence
accumulates. The seal check shows WHEN new learning occurred.

### Proc Macro

```rust
#[derive(EncounterRound)]
struct EpsteinV31 {
    #[source = "cypher/aiwar_enrichment_epstein_v31_patch.cypher"]
    #[confidence = 0.60]
    edges: Vec<Edge>,
}
```

The macro:
1. Parses the Cypher file at compile time
2. Generates an `impl EncounterRound` that loads the edges
3. Assigns truth values from the `#[confidence]` attribute
4. Registers with the versioned graph as a named version

All 30 files become 30 typed structs. `cargo build` validates them.

## Feature 2: NARS Abductive Inference (2 days)

Chain walk + truth revision = INFER edges that don't exist yet.

```
Known:  Palantir →DEVELOPED_BY→ US_DoD     (f:0.95, c:0.87)
Known:  US_DoD  →DEPLOYED_BY→  Gotham      (f:0.90, c:0.82)
Infer:  Palantir →USES→ Gotham              (f:0.86, c:0.71)  ← abducted
```

The math:
```rust
// Deduction: if A→B and B→C, then A→C
fn nars_deduction(ab: &TruthValue, bc: &TruthValue) -> TruthValue {
    let f = ab.frequency * bc.frequency;
    let c = ab.confidence * bc.confidence * ab.frequency * bc.frequency;
    TruthValue::new(f, c)
}

// Abduction: if A→B and C→B, then A→C (weaker)
fn nars_abduction(ab: &TruthValue, cb: &TruthValue) -> TruthValue {
    let f = ab.frequency;
    let c = ab.confidence * cb.confidence * cb.frequency;
    TruthValue::new(f, c)
}
```

### In the Cockpit

Toggle: `[Show inferred edges]`

Inferred edges render as DASHED lines with lower opacity.
Hover shows the inference chain:
```
Inferred: Palantir → Gotham
  Via: Palantir →(0.95)→ US_DoD →(0.90)→ Gotham
  Truth: f:0.86 c:0.71 (deduction)
  Hops: 2
```

Click "Verify" → runs `walk_chain_forward()` to show the full path.
Click "Accept" → promotes to a real edge with c:0.71 (new encounter round).
Click "Reject" → adds a negative evidence edge (f:0.0, c:0.71).

### Proc Macro

```rust
#[derive(InferenceRule)]
#[rule = "deduction"]  // or "abduction", "induction"
#[max_hops = 3]
#[min_confidence = 0.5]
struct TransitiveDeveloper;

// Generates:
impl InferenceRule for TransitiveDeveloper {
    fn apply(store: &SpoStore) -> Vec<InferredEdge> {
        // For every A→B→C chain where both edges have c > 0.5,
        // infer A→C with deduction truth
    }
}
```

Register rules. Press play. Inferred edges appear as the temporal
playback progresses. New evidence in v35 might CONFIRM a v31 inference,
strengthening it via `revision()`. Or CONTRADICT it, weakening it.

## Feature 3: Progressive Hydration Lens (1 day)

ZeckF64 progressive encoding means you can show the graph at
different "resolution levels" without reloading:

```
Level 0 (1 byte/edge):  Scent only. Binary: connected or not.
                         Fast, 94% accurate. Good for overview.
                         
Level 1 (2 bytes/edge): Scent + SPO distance quantile.
                         Edge thickness reflects similarity.
                         96% accurate.
                         
Level 7 (8 bytes/edge): Full resolution. All 7 distance quantiles.
                         98% accurate. Full detail.
```

### In the Cockpit

A "resolution" slider in the graph toolbar:

```
Resolution: [●━━━━━━━━━━━━] 1 byte  →  nodes: 221, edges: 356, ρ=0.937
Resolution: [━━━━━●━━━━━━━] 4 bytes →  nodes: 221, edges: 289, ρ=0.968
Resolution: [━━━━━━━━━━━━●] 8 bytes →  nodes: 221, edges: 201, ρ=0.982
```

At low resolution: everything is connected (scent says "close enough").
Slide right: weak connections fade, strong ones sharpen. The graph
CLARIFIES in real time. Like focusing a microscope.

This is progressive JPEG for graphs. Nobody has this.

### No Macro Needed

ZeckF64 already packs the bytes. Just mask:
```rust
fn edge_at_resolution(edge: u64, bytes: usize) -> u64 {
    let mask = match bytes {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        _ => u64::MAX,
    };
    edge & mask
}

fn edge_passes_threshold(edge: u64, bytes: usize, threshold: u8) -> bool {
    let scent = (edge & 0xFF) as u8;
    let close_bits = (scent & 0x7F).count_ones();
    close_bits >= threshold as u32
}
```

The slider just changes `bytes` and `threshold`. Graph re-renders instantly
because it's a bitmask, not a re-query.

---

## Composition: The Full Demo

Press play. This happens:

```
0s:   Base graph loads (v00). 221 nodes. Resolution: 1 byte (overview).
2s:   Enrichment v01 fades in. New edges appear.
      NARS revision runs. Some edges get thicker (more confident).
4s:   Epstein v31 arrives. Dashed inference lines appear
      (abducted connections). Status: "Staunen — 23 inferred edges"
6s:   v32-v35 stream in. Inferences either STRENGTHEN (line solidifies)
      or WEAKEN (line fades). revision() runs on each.
8s:   Slide resolution from 1→4 bytes. Weak connections vanish.
      Strong structure emerges. The graph CLARIFIES.
10s:  v40-v42. Status: "Wisdom — stable". No new learning.
      Final graph: 221 nodes, 180 strong edges, 23 inferred, 
      12 verified by Merkle seal check.
```

Total demo: 10 seconds. Shows:
1. Temporal graph evolution (versioned + seal check)
2. Live reasoning (NARS revision + abductive inference)
3. Progressive focus (ZeckF64 resolution slider)
4. Integrity verification (Merkle seals)
5. Algebraic path computation (semiring selector)

What Neo4j shows in a demo: "Here's PageRank. Here's Louvain. Here's centrality."
What we show: "Press play and watch the graph THINK."

---

## Implementation Effort

| Feature | Existing Code | New Code | Time |
|---------|--------------|----------|------|
| Play button | VersionedGraph, seal_check | Cockpit timeline component, encounter round loader | 1 day |
| NARS inference | revision(), walk_chain_forward() | Deduction/abduction functions (20 lines each), cockpit toggle | 2 days |
| Progressive lens | ZeckF64, edge_at_resolution | Cockpit slider, bitmask filter | 1 day |
| Proc macros | — | #[derive(EncounterRound)], #[derive(InferenceRule)] | 2 days |
| **Total** | **295 tests, all passing** | **~500 lines Rust + cockpit components** | **~1 week** |

The primitives exist. The composition is new. The macros make it repeatable.
