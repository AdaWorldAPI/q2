# Rewire q2 onto lance-graph (real engine, drop neo4j-rs)

## Overview
The q2 cockpit (AIWAR graph notebook, 221 nodes / ~326 edges) renders **mock
JSON** because the workspace cannot build: `crates/aiwar-ingest` depends on
`neo4j-rs` at `../../../neo4j-rs`, which is absent — so the whole `crates/*`
workspace manifest fails to load.

Operator decision (2026-06-18): **`neo4j-rs` is not needed.** It was a
Neo4j-flavored *GUI* experiment; the graph **API/engine is lance-graph**.
Presentation (the neo4j-style cockpit) is the vis-network layer. So
`aiwar-ingest` drops `neo4j-rs` and becomes the engine-agnostic ingest layer
feeding the lance-graph-backed consumers (`graph_engine`, `notebook-query`).

Data source: `AdaWorldAPI/aiwar-neo4j-harvest` `/data` (aiwar_graph.json,
schema.json, csv) + `/cypher` (30 versioned enrichment rounds). Vendored to
`/home/user/aiwar-neo4j-harvest/` (sibling path main.rs already probes).

## Phase A — Unblock the build (P0)
- [ ] `aiwar-ingest/Cargo.toml`: remove `neo4j-rs` (+ unused `tokio`); keep serde/serde_json/regex/thiserror/tracing
- [ ] `aiwar-ingest/src/lib.rs`: drop neo4j `Graph<MemoryBackend>` builder + `query_result_to_vis_json`; keep typed JSON model; add engine-agnostic `AiWarGraph { nodes, edges }` + `load_from_file`/`load_from_json` + `to_vis_json()` + `apply_round()` (enrichment)
- [ ] keep `encounter_round.rs` (parses /cypher rounds) untouched
- [ ] tests: load real aiwar_graph.json → 221 nodes; vis-json shape; enrichment merge
- [ ] `graph_engine.rs::hydrate_from_aiwar_json`: delegate to `aiwar_ingest::load_from_file` (dedupe the parallel inline parser); optionally apply /cypher rounds
- [ ] `cockpit-server/main.rs`: default `AIWAR_DATA_PATH` to vendored data when env unset
- [ ] `cargo check -p aiwar-ingest` green (light, fast); then full consumer build green

## Phase B — Real Gremlin execution (the demo query)
- [ ] `notebook-query`: Gremlin→Cypher transpile for the traversal subset
      (`g.V()`, `hasLabel`, `has`, `out`/`in`/`outE`/`inV`/`both`, `values`, `path`, `limit`)
- [ ] route transpiled Cypher through the existing REAL `execute_cypher` lance-graph path
- [ ] replace the "return full graph + planner meta" stub in `execute_graph_query`
- [ ] tests: `g.V().hasLabel('System').outE().inV().path()` → real filtered subgraph

## Phase C — Verify
- [ ] `cargo build` workspace green; `cargo nextest run` (lance-graph consumers) green
- [ ] smoke: Cypher + Gremlin queries return real rows/subgraph from aiwar data

## Future (noted, not now — operator's last message)
- SurrealDB `lite-unified` (lance-graph PR #540, default-OFF) + AR-shaped
  AST-DLL "Neo4j-shaped AST adapter" for Gremlin/Elixir. Separate workstream;
  leave a transpile seam in Phase B that an AST adapter can later replace.

## Sync state
- lance-graph local branch fast-forwarded to origin/main `ef7e97e` (incl PR #539, #540).
- branch (both repos): `claude/pensive-mendel-ou2rj6`.
