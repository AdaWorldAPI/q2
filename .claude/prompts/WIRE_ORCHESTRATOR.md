# rs-graph-llm — The Thinking Orchestrator (optional, `--features orchestrator`)

## FIRST: Read .claude/rules/architectural-compliance.md
## SECOND: Read .claude/rules/borrow-strategy.md

## What rs-graph-llm IS

It's LangGraph/LangStudio reimplemented in Rust. The `graph-flow` crate
provides typed task graphs with conditional routing, parallel fanout,
subgraph composition, streaming execution, and persistent sessions.

On top of that sits `thinking.rs` — a 10-layer cognitive processing
pipeline that orchestrates lance-graph features into a reasoning loop.

```
graph-flow (framework)
  ├── Task, Graph, GraphBuilder, FlowRunner    — the execution engine
  ├── ReAct agent pattern                       — reason → decide → act loop
  ├── FanOut                                    — parallel child tasks
  ├── Subgraph                                  — hierarchical composition
  ├── Conditional routing                       — if/else in the graph
  ├── StreamingRunner                           — SSE-compatible event stream
  ├── MCP tool integration (McpToolTask)        — call external tools
  ├── Agent Card YAML → Graph compiler          — declarative agents
  ├── Lance session storage + time travel       — rewind any session
  └── Tiered storage: Lance → Postgres → S3     — hot/warm/cold sessions

thinking.rs (the 10-layer cognitive stack)
  Layer 1:  Sensory Ingest      → MCP input → raw data
  Layer 2:  Fingerprint         → label_fp() → encounter
  Layer 3:  Cascade Search      → HHTL → attention band (Foveal/Parafoveal)
  Layer 5:  Semiring Reasoning  → walk_chain_forward() + semiring algebra
  Layer 6:  Memory Consolidate  → NARS revision + seal check (Wisdom/Staunen)
  Layer 7:  Planning            → goal decomposition
  Layer 8:  Action Selection    → RL credit assignment
  Layer 9:  Output Generation   → LLM or template
  Layer 10: Meta-Cognition      → PET scan trace (which layers fired)

  Conditional routing:
    Cascade → Foveal?  → skip reasoning, go to memory consolidation
    Memory  → Staunen? → replan. Wisdom? → act directly.
```

## What It Does For the Hydration Showoffs

Without rs-graph-llm: the play button advances versions manually.
Human clicks through enrichment files one by one.

With rs-graph-llm: the thinking graph IS the play button.
Each enrichment file is a `sensory_ingest`. The graph DECIDES:
- Is this Foveal (familiar)? → skip reasoning, consolidate.
- Is this Parafoveal (novel)? → run semiring inference, then consolidate.
- Did Staunen occur (new learning)? → replan, reassess, iterate.
- Wisdom (stable)? → move to next version.

The cockpit doesn't just show data evolving. It shows REASONING evolving.
The PET scan trace (Layer 10) shows which layers fired for each version,
visible as a heatmap in the status bar or a dedicated panel.

## How It Integrates With lance-graph

rs-graph-llm CALLS lance-graph. It doesn't replace it.

```
thinking.rs Layer 2 (Fingerprint)      → calls lance_graph::fingerprint::label_fp()
thinking.rs Layer 3 (Cascade Search)   → calls lance_graph::graph::blasgraph::cascade_search()
thinking.rs Layer 5 (Semiring Reason)  → calls lance_graph::graph::spo::walk_chain_forward()
thinking.rs Layer 6 (Memory Consolidate) → calls lance_graph::graph::versioned::commit_encounter_round()
                                         → calls lance_graph::graph::spo::truth::TruthValue::revision()
                                         → calls lance_graph::graph::versioned::graph_seal_check()
```

Currently `thinking.rs` has placeholder implementations for these calls.
The wiring task replaces placeholders with real lance-graph API calls.

## How It Integrates With q2

```toml
# q2/Cargo.toml (already there)
[dependencies.rs-graph-llm]
git = "https://github.com/AdaWorldAPI/rs-graph-llm"
optional = true

[features]
orchestrator = ["rs-graph-llm"]
```

When enabled, notebook-query gains a `%%think` magic command:

```
%%think
MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s, st
```

Instead of just running the Cypher, this routes through the 10-layer
thinking graph:
1. Sensory ingest: parse the Cypher query
2. Fingerprint: compute query fingerprint
3. Cascade search: is this a familiar query pattern? (Foveal → cached result)
4. Semiring reasoning: run the query + infer additional edges
5. Memory consolidation: update the knowledge graph with encounter
6. Output: return enriched results (original + inferred)

The cockpit shows the PET scan trace alongside the result:
```
L1 ████ L2 ████ L3 ███ L5 ████████ L6 ██ L7 · L8 ████ L9 ██████ L10 ███
Parafoveal · Staunen · 3 inferred edges · 14ms total
```

## Borrow Strategy Compliance

The thinking graph uses the readonly BindSpace pattern:

```rust
// Layer 5: Semiring Reasoning
async fn run(&self, context: Context) -> Result<TaskResult> {
    let spo_store: &SpoStore = context.get_ref("spo_store").await;  // &self, readonly
    
    // Compute on owned microcopies
    let hits = spo_store.query_forward(&query_fp);  // returns owned Vec<SpoHit>
    let mut inferred: Vec<InferredEdge> = Vec::new();
    
    for hit in &hits {
        let mut truth = hit.record.truth;  // Copy, owned
        let chain = spo_store.walk_chain_forward(&hit.record.subject, 100, 3);
        for hop in &chain {
            truth = truth.revision(&hop.truth);  // owned computation
        }
        if truth.confidence > 0.5 {
            inferred.push(InferredEdge { /* ... */ });
        }
    }
    
    // Write back through gate
    context.set("inferred_edges", inferred).await;  // no &mut on spo_store
    Ok(TaskResult::new(Some(format!("{} inferred", inferred.len())), NextAction::Continue))
}
```

SIMD stays on slices. Reasoning stays on microcopies. Write-back through context, not through &mut on the store.

## The Play Button (temporal reasoning orchestration)

```rust
/// Run the full temporal playback as a graph-flow execution.
async fn run_temporal_playback(
    enrichment_files: &[PathBuf],
    spo_store: &SpoStore,
    versioned_graph: &VersionedGraph,
) -> Vec<StreamChunk> {
    let graph = build_thinking_graph();
    let storage = Arc::new(InMemorySessionStorage::new());
    let streaming_runner = StreamingRunner::new(graph.clone(), storage.clone());
    
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    
    for (version, file) in enrichment_files.iter().enumerate() {
        let cypher = std::fs::read_to_string(file).unwrap();
        
        let mut session = Session::new_from_task(
            format!("v{}", version),
            "sensory_ingest",
        );
        session.context.set("raw_input", cypher).await;
        session.context.set("version", version).await;
        session.context.set("spo_store_ref", spo_store).await;
        
        // Execute thinking graph — streams chunks to cockpit via SSE
        streaming_runner.run_streaming(&mut session, tx.clone()).await.unwrap();
        
        // After each version: check seal
        let seal_status = versioned_graph.graph_seal_check(version, version + 1);
        // Stream seal status to cockpit
    }
}
```

Each enrichment file triggers a full thinking loop.
The cockpit receives StreamChunks via SSE.
The PET scan trace accumulates across versions.
The temporal slider reflects which version the thinking graph is processing.

## Implementation Order

### Phase 1: Wire the calls (2 days)
- Replace `thinking.rs` placeholder implementations with real lance-graph calls
- Add lance-graph as a dependency to graph-flow (behind feature flag)
- Verify the thinking graph runs end-to-end with real fingerprints + cascade + SPO

### Phase 2: Wire to q2 (1 day)
- Add `%%think` magic command to notebook-query
- StreamingRunner → SSE events on `/mcp/sse`
- PET scan trace in cockpit status bar

### Phase 3: Temporal playback (2 days)
- Load enrichment files as encounter rounds
- StreamingRunner orchestrates the play button
- Cockpit timeline component receives StreamChunks
- Version slider + play/pause controls

### Phase 4: Agent cards for custom reasoning (later)
- YAML-defined agent workflows for domain-specific inference
- "Load agent card" in cockpit
- Custom thinking graphs per notebook

## What NOT to Do

- Do NOT substitute graph-flow with another orchestration framework
- Do NOT move lance-graph calls into graph-flow — graph-flow CALLS lance-graph
- Do NOT remove the streaming interface — the cockpit needs SSE chunks
- Do NOT use &mut on SpoStore during Layer 5 — use owned microcopies
- Do NOT copy data for SIMD — use slices into the backing store
