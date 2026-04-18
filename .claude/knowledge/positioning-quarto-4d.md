# q2 Positioning — Quarto 4D + Neo4j Browser, One Rust Binary

> Market-facing framing. Informs naming, doc voice, landing-page copy, demo
> scripts. Internal architectural work keeps its own language; this doc is
> what the world sees.

---

## The One-Line

> **q2 is Quarto 4D with a Neo4j Browser inside it — a single Rust binary
> that parses Cypher, GQL, Gremlin, and SPARQL into the same IR, executes
> them on a SIMD graph engine, and renders the result as a live 3D
> notebook with a timeline axis.**

Three-second elevator pitch. Everything else is features.

---

## What the World Sees

| Feature | Positioning |
|---|---|
| Cypher / GQL / Gremlin / SPARQL support | "Query your graph in whatever dialect your team already uses." |
| CAM-PQ + bgz17 + SIMD kernels | "10-100× faster than Neo4j Browser on semantic graph traversal." |
| Single binary, no JVM, no separate DB daemon | "Drop it on a laptop. Works offline." |
| Live 3D scene + timeline axis | "Quarto 4D — the 4th dimension is execution history." |
| Notebook cells with reactive dependencies | "Cells react to each other. Quarto for graphs." |
| Export to PDF / HTML / R Markdown / Jupyter | "Standard Quarto output. Your existing workflow still works." |
| MCP + OpenAI-compatible API | "LLM-ready. Use Claude, ChatGPT, or any MCP client as a query frontend." |
| OSINT crate (`lance-graph-osint`) with crawler + extractor | "Ingest intelligence feeds directly into the notebook." |
| Neural debugger (`/debug`) | "See which planner strategies are alive, which are degraded." |
| Brain MRI (`/mri`) | "Advanced graph analytics: live cognitive telemetry during query execution." |

The features go from boring (Cypher compat) to novel (3D timeline) to
sophisticated (live cognitive telemetry). The last three are **the
headline differentiation** — but they're framed as "advanced graph
analytics", not "AGI primitives". Nobody asks awkward questions.

---

## What Ships Alongside (Framed as Advanced Features)

These aren't mentioned in marketing copy but are visible to anyone who
reads the API docs or source. All frame cleanly as "features you'd expect
from a Palantir-class graph system":

| Capability | Doc framing |
|---|---|
| 4-pillar contract (NARS + thinking styles + qualia + proprioception) | "Agent-assisted query planning. The engine tracks its own search state and adapts strategy." |
| AriGraph episodic memory | "Query history as a first-class object. Recall similar past queries by content, not just timestamp." |
| WorldMapDto + renderer trait | "Every analytic has a pluggable presentation layer. Relabel anything for your domain." |
| Cognitive shader driver + BindSpace | "16K-bit vector substrate for semantic similarity. Faster than cosine on embeddings." |
| Blackboard + multi-agent routing | "Query federation. Split a question across multiple sub-analysts." |
| MUL / Flow state / Dunning-Kruger | "Confidence calibration on every result. You know when the engine is unsure." |
| Cycle fingerprint provenance | "Every result carries a deterministic signature. Reproducible queries, even across restarts." |
| 11D state-classifier anchors | "Query-regime classification. Exploit vs explore vs reflect modes visible in telemetry." |

Every one of these reads as "competent graph-analytics feature" to a
reviewer. None of them read as "embodied AGI substrate". The engineering
is the same; the framing is the product.

---

## The "Incidentally" Strategy

Sell **Neo4j at a fraction of the time** first. Once a team pilots q2
for its speed, they discover:

1. Their queries now have "confidence scores" (NARS truth values).
2. The engine can explain *why* it took a particular query path
   (proprioception state report, anchor classification).
3. Past queries are retrievable by content, not just text match
   (AriGraph episodic memory with cycle-fingerprint similarity).
4. They can run a query in OSINT mode and get pipeline telemetry
   (the OSINT crate kicks in).
5. The live 3D MRI view turns out to be useful for debugging their
   queries (not just a demo gimmick).

Each of these is a legitimate product feature. Collectively they are
the 4-pillar agent contract exercising itself. The customer doesn't
need to know or care about the distinction.

---

## Why "Quarto 4D"

Quarto is already a well-known scientific-notebook ecosystem (R, Python,
Julia, Observable). q2 inherits the Quarto name, QMD format, Pandoc AST,
and R kernel protocol — everything in the upstream `quarto-dev/q2` tree
stays intact and works.

The "4D" is the addition:

- **Dimension 1-2:** traditional document (text + code cells).
- **Dimension 3:** 3D scene view — graphs as spatial objects, not flat
  SVG renderings. Cells are positioned in a spatial layout; results
  appear as volumes.
- **Dimension 4:** timeline. Every cell execution is a point in time.
  The notebook has a scrubber. Re-execute from any prior state. Fork
  the timeline at a cell boundary and explore alternative branches.

The timeline is where `cycle_fingerprint` shines. Each execution
fingerprints its world-model state; the timeline axis indexes them. A
user can ask "show me the query history where the engine was in
'Observer' mode" and the timeline filters accordingly.

---

## What This Means for Implementation Priorities

Given the positioning, priority ordering for the next vertical should be:

1. **Fast Cypher.** Single most important thing. If q2 isn't obviously
   faster than Neo4j Browser on a standard benchmark, the pitch dies.
   Everything else is supporting cast.
2. **3D + timeline scene.** The "Quarto 4D" visible differentiator.
   Screenshots and demos live here.
3. **Cypher-in-MCP.** LLMs querying the graph via MCP is the zeitgeist
   hook — customers already using Claude/ChatGPT see immediate value.
4. **OSINT crate demo.** Real-world data pipeline that shows the
   "analytics features" ride-along is not hypothetical.
5. **AGI primitives as debug/MRI views.** Sold as "engine telemetry",
   not as "proprioceptive self-awareness". But visible to anyone who
   opens the `/mri` tab.

The chess-NARS vertical (see `chess-nars-vertical-slice.md`) serves this
positioning: chess is a legitimate graph-analytics demo domain, and the
live MRI view during play becomes the killer screenshot for the product
page.

---

## Naming Discipline

- **Public:** "graph notebook", "query engine", "analytics telemetry",
  "provenance fingerprint", "agent-assisted planner", "3D graph view",
  "timeline scrubber", "OSINT pipeline".
- **Internal (keep out of docs, READMEs, landing pages):** "qualia",
  "proprioception", "felt", "embodiment", "bond", "companion agent",
  "persona", "episodic self-awareness".

Code identifiers can be whatever makes sense internally; the contract
crate already uses neutral names (`StateClassifier`, `WorldMapRenderer`,
`ProprioceptionAxes`). The positioning is about what the *website* and
the *docs.rs front pages* say, not what the Rust types are called.

When in doubt: would a Gartner analyst writing a "modern graph
databases" report quote this sentence favourably? If yes, ship it. If
no, rewrite.

---

## Competitor Framing

| They say | We say |
|---|---|
| Neo4j: "native graph database" | "Cypher-compatible. Same query, 10× faster, one binary." |
| Neo4j Bloom: "graph visualization" | "3D + timeline. Visualization is a view, not a product." |
| Palantir Gotham: "intel platform, $M/yr" | "Same UX, open source, runs on your laptop." |
| TigerGraph: "enterprise graph analytics" | "Multi-dialect query (Cypher/GQL/Gremlin/SPARQL). No lock-in." |
| Ontotext GraphDB: "RDF / SPARQL" | "SPARQL *and* Cypher *and* GQL. One engine, no bridge." |
| DuckDB: "in-process OLAP" | "The graph-native DuckDB. Same ethos, graph algebra." |

Differentiators that actually matter:
- Speed (CAM-PQ is genuinely faster than adjacency-matrix BFS on
  semantic similarity queries).
- Single-binary distribution (no JVM, no daemon, no separate server).
- 4D (timeline + 3D) as a first-class viewport.
- Telemetry that looks like Palantir but runs locally.
