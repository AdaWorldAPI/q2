# Phase 2 — Wire q2 to the Real lance-graph Thinking Engine

> Date: 2026-05-06
> Branch: claude/design-graph-notebook-frontend-Pwoqh (continuation)
> Predecessor: 2026-04-21-shader-dto-wiring.md (Phase 1, commit 3d717b8)
> Constraint: zero substitution — use the EXACT crates from
>   AdaWorldAPI/lance-graph (see .claude/rules/architectural-compliance.md)
> Constraint: serde stays at the SSE boundary only; the in-process path
>   uses native thinking_engine::dto::* types
> Mode: 12-agent swarm, file-partitioned, single feature commit at the end

This plan exists because Phase 1 shipped a beautiful UI on top of a
hand-rolled simulation. Future sessions reading this should NOT regress
to mockup-mode. The DTOs are real. The engine is real. The codebook
width is real. Stop hashing strings and pretending.

---

## 1. Brutally Honest Review of Phase 1 (commit 3d717b8)

Phase 1 delivered the wire format and the cockpit chrome. It did NOT
deliver cognition. The cycle that lit up the /reasoning page was
theatre.

### What was real in Phase 1
- The 6-event SSE schema (stream | resonance | bus | thought |
  scene_begin | scene_end)
- The cockpit components (EnergyField, BusTicker, ThoughtLog,
  SceneBreadcrumb, FreeEnergyDial) and their useShaderStream hook
- The Cypher scene file discovery via CYPHER_PATH
- The /demo-fallback route preserving the seed stubs
- The /api/graph/infer endpoint shape (graph_engine::nars_infer_handler)

### What was theatre — to be deleted in Phase 2
- Hand-rolled WireStreamDto / WireResonanceDto / WireBusDto /
  WireThoughtStruct declared inline in
  crates/cockpit-server/src/shader_stream.rs (commit 3d717b8, approx
  lines 22-70). These were #[derive(Serialize)] structs with NO
  connection to thinking_engine::dto. They duplicated the field names
  and lied about the types — top_k: Vec<(u16, f32)> instead of the real
  f32[4096] energy vector with a sparse projection.
- Simulated cognitive cycles: the scene player advanced cycle_count and
  made up energy, entropy, active_count, converged, and top_k values
  directly in the SSE loop. There was no ThinkingEngine, no perturb, no
  think, no commit. The numbers came from per-act confidence and a
  sine-ish ramp.
- Hashed codebook indices: there was no extract_cypher_identifiers().
  The loop ran DefaultHasher on whole tokens and modulo'd into 0..4096,
  with no attention to whether 4096 was even the right width. The
  constant CODEBOOK_SIZE lived nowhere — it was a magic number in two
  unrelated places.
- Hand-rolled NARS in graph_engine.rs::run_nars_deduction: the body was
  a literal `f = f1 * f2; c = f1 * f2 * c1 * c2` formula in a
  triple-nested loop (graph_engine.rs lines ~205-265 in commit 3d717b8).
  This is approximately the deduction rule, but it was open-coded
  rather than calling causal_edge::TruthValue::deduction(&self, &other).
  No revision, no abduction, no NAL-6 syntax variables, no
  NarsTables-driven dispatch. A grader checking the truth-value algebra
  against the canonical causal-edge crate would find numerical
  divergence on any non-trivial chain.
- Bypassed Cypher parser: the scene player read raw .cypher files,
  grabbed regex-extracted MATCH previews, and never invoked
  lance_graph::parser::parse_cypher_query. The actual CypherQuery AST
  (get_node_labels(), get_relationship_types(), ValueExpression::
  {Variable, Property, ScalarFunction, AggregateFunction}) was
  untouched.
- Missing AriGraph tissue: the encounter-round ingest path
  (aiwar_ingest::load_from_json) still landed in a TripletGraph scratch
  buffer, not in lance_graph::graph::arigraph (the real AriGraph homing
  mechanism). NARS deduction therefore ran on a free-floating triplet
  pile with no homing locality, no BindSpace::write_gated_xor, no
  BUNDLE discipline from .claude/rules/borrow-strategy.md.
- f32[4096] field omission: ResonanceDto::energy (the real one) is a
  16 KB tensor. Phase 1 never sent it, never summarised it from a real
  source, never even allocated one. The "energy" field on the wire was
  a scalar derived from act_index / total_acts.

### Severity
Per .claude/rules/architectural-compliance.md, substituting a specified
component without asking is P0. Phase 1 did not formally substitute —
but it shipped a parallel implementation that papered over the absence
of thinking-engine. That is functionally equivalent and just as
dangerous: the next session reading the code would naturally extend the
fake DTOs rather than the real ones. Phase 2 deletes the parallel
implementation.

---

## 2. Phase 2 Resolution — 12-Agent Partition Table

Twelve agents, each owning a disjoint file set. Single commit at the
end. Each agent logs to /tmp/q2-agent-log.jsonl via the shared `tee -a`
contract. Agent #11 is this docs writer.

| # | Agent | Owns | Outcome |
|---|---|---|---|
| 0 | main | swarm coordinator | dispatches, gates final commit |
| 1 | deps-wirer | Cargo.toml (workspace + cockpit-server) | path deps for lance-graph, thinking-engine, cognitive-shader-driver, lance-graph-contract, causal-edge |
| 2 | dto-bridge | crates/cockpit-server/src/dto_bridge.rs (NEW) | Wire* mirrors with From<&thinking_engine::dto::*>; sparse projection of f32[4096] |
| 3 | shader-stream-rewriter | crates/cockpit-server/src/shader_stream.rs | drives real ThinkingEngine::{perturb, think, commit}; deletes simulated cycle |
| 4 | scene-player | crates/cockpit-server/src/scene_player.rs (NEW) | parses Cypher via lance_graph::parser::parse_cypher_query, walks AST, emits real EngStreamDto |
| 5 | codebook | crates/cockpit-server/src/codebook.rs (NEW) | CODEBOOK_SIZE = 4096 mirror, token_to_index, extract_cypher_identifiers, deterministic |
| 6 | arigraph-nars-merger | crates/cockpit-server/src/graph_engine.rs | swap hand-rolled NARS for causal_edge::TruthValue::{deduction, revision}; route ingest through arigraph |
| 7 | fe-types | cockpit/src/hooks/useShaderStream.ts | sync TS event types to dto_bridge (sensor_contributions, tension_history_len, style_trajectory) |
| 8 | build-fixer | cargo check --workspace | resolves whatever 1-6 leave broken |
| 9 | e2e-runner | runs server + curl SSE | verifies six event types appear in real order with non-fabricated values |
| 10 | fallback-strengthener | cockpit/src/DemoApp.tsx, DataStatusBar.tsx | explicit FALLBACK / LIVE banner when shader is unreachable |
| 11 | docs-updater | .claude/plans/* | this document |
| 12 | lint-pass | cargo xtask lint, cargo fmt, hooks | final polish before commit |

Dependency order: 1 -> (2, 5) -> (3, 4, 6) -> 7 -> 8 -> 9 -> 10 -> 12
-> main commit. Agent 11 runs in parallel with anyone (only touches
.claude/plans/).

---

## 3. Architecture Compliance Check

Per .claude/rules/architectural-compliance.md, the user said
"use lance-graph as hot path". Verifying Phase 2 against that mandate:

### Crates consumed (path deps to /home/user/lance-graph/crates/*)
- lance-graph — Cypher parser (parse_cypher_query) + AST
  (CypherQuery::get_node_labels, ValueExpression::{Variable, Property,
  ScalarFunction, AggregateFunction}) + arigraph for homing
- thinking-engine — ThinkingEngine, dto::{StreamDto, ResonanceDto,
  BusDto, ThoughtStruct, SourceType, ThinkingScale},
  engine::CODEBOOK_SIZE
- cognitive-shader-driver — feature-gated lab endpoints (Option A from
  the Apr 21 plan, deferred mounting to Phase 3 when models load)
- lance-graph-contract — nars::SemiringChoice (documentation value
  only; semiring stays OUT of StreamDto/BusDto path per Phase 1 rule)
- causal-edge — CausalEdge64, NarsTables, TruthValue::{deduction,
  revision, abduction} — replaces hand-rolled NARS in graph_engine.rs

These are the EXACT crates wired in
crates/cockpit-server/Cargo.toml (lines 26-37 of the workspace block):

    lance-graph.workspace             = true
    thinking-engine.workspace         = true
    cognitive-shader-driver.workspace = true
    lance-graph-contract.workspace    = true
    causal-edge.workspace             = true

### Compliance verdict
- Lance-graph IS the hot path: cockpit-server::shader_stream calls
  thinking_engine::ThinkingEngine directly with no shim (see
  shader_stream.rs line 26: `use thinking_engine::ThinkingEngine;`).
- No new external dependency was added in lieu of a specified one.
- Neo4j-rs remains feature-gated (neo4j-fallback, off by default) per
  the Phase 1 plan. It is a data-source fallback, not an engine
  substitute.
- Borrow strategy (.claude/rules/borrow-strategy.md): ThinkingEngine is
  held in Arc<Mutex<_>> per SSE connection; perturb/think/commit follow
  the readonly-BindSpace + owned-microcopy pattern internally. q2 does
  not touch the BindSpace directly — it only goes through the engine's
  public API, which already obeys the rule.

---

## 4. What STAYS a Stub After Phase 2 (be honest)

A full ThinkingEngine cycle that produces meaningful cognition needs
LOADED MODELS:

- Jina v5 embeddings — populates the 4096x4096 codebook distance table;
  without it, ThinkingEngine::new(distance_table) consumes a synthetic
  matrix (Phase 2 builds an identity-ish placeholder in
  crate::codebook::distance_table()).
- BGE-M3 — sparse + dense + ColBERT vectors for cross-encoder
  retrieval; drives SourceType::BgeM3 ingestion in dto::StreamDto.
- Reader-LM — Qwen-finetuned reranker for SourceType::ReaderLm;
  required for the sensor_contributions field on ThoughtStruct to
  reflect real reranks.
- Qwen safetensors — the lab calibration endpoints
  (/v1/shader/calibrate, /probe, /tensors, /sweep, /token-agreement)
  all require the safetensors mounted at QWEN_TENSORS_PATH.

q2 does NOT have these at runtime. Railway deploy and dev clones run
without GPU and without ~30 GB of model weights. So Phase 2 ships a
real engine running on a synthetic distance table with placeholder
SourceType annotations. The cockpit displays the real
ResonanceDto::energy field, computed by real MatVec cycles on the real
engine — but the energies represent diffusion across a placeholder
codebook, not real semantic distances between encoded thoughts.

This is fine for the visual: the /reasoning page is honest about what
it shows (free-energy minimisation on a known toy field). It is NOT
fine if anyone interprets the BusTicker text as a real model output.
Hence the LIVE / FALLBACK / SYNTHETIC banner that
fallback-strengthener (agent 10) installs.

### Plan: how models arrive
- Phase 3 introduces crates/cockpit-server/src/model_loader.rs with two
  modes: (a) cold-mmap from a configured path, (b) lazy-download from a
  signed URL. The synthetic distance table stays as the unit-test
  fixture.
- Models live on disk under QUARTO_MODELS_DIR; if absent, Phase 2's
  synthetic codebook continues to drive the engine (the binary
  degrades, it does not crash).

---

## 5. Phase 3 Preview

After Phase 2 ships:

- Real codebook: crate::codebook::distance_table() is replaced by a
  function that mmaps jina_v5_distances.bin and validates the 4096x4096
  shape. Synthetic generator becomes the fallback only.
- Model loading: cognitive-shader-driver lab endpoints
  (/v1/shader/calibrate, /probe, /tensors, /sweep, /token-agreement)
  get mounted behind LAB_ENABLED=true. They consume real Qwen
  safetensors.
- Full cognitive cycle: encounter rounds from
  aiwar_ingest::load_from_json flow through arigraph::homing_write ->
  BindSpace::write_gated_xor -> SSE clients see real ResonanceDto top-k
  driven by Jina distances rather than the toy field.
- NAL-6 expansion: causal-edge::NarsTables gains the variable-binding
  rules; graph_engine::run_nars_deduction is replaced by a single call
  to NarsTables::dispatch_chain.
- 36-brain superposition (Phase 3 of the original Apr 21 plan):
  parallel dispatches via WireStyleSelector::Ordinal(n), additive
  energy merge in the cockpit.
- Stub deletion: orchestrator.rs (1563 LOC), graph-flow stub (171 LOC),
  analyst.rs (371 LOC), thinking.rs (116 LOC), diagnostics.rs (334 LOC)
  — see Phase 3 LOC budget in the Apr 21 plan.

---

## 6. Test Plan Checklist

These are the gates between Phase 2 commit and Phase 2 push.
e2e-runner (agent 9) is the primary executor; lint-pass (agent 12)
confirms formatter/lint pass; docs-updater (agent 11, this file)
records results in /tmp/q2-agent-log.jsonl under phase `done`.

### Build / typecheck
- [ ] `cargo check -p cockpit-server` clean (no `unused`, no `dead_code`)
- [ ] `cargo check --workspace` clean
- [ ] `cargo nextest run -p cockpit-server` — at minimum the
      dto_bridge::tests::* and codebook::tests::* modules pass
- [ ] `cargo xtask lint` clean (no external-sources/ references)
- [ ] `cargo fmt --check` clean

### E2E SSE
- [ ] `cargo run -p cockpit-server` starts on :8080 without panic
- [ ] `curl -N http://localhost:8080/v1/shader/stream` emits all six
      event types within 60 s: scene_begin, stream, resonance, bus,
      thought, scene_end
- [ ] resonance.payload.top_k contains entries with 0 <= index < 4096
      and energies that change cycle-to-cycle (NOT a constant ramp)
- [ ] resonance.payload.cycle_count is monotonically increasing within
      a scene; converged: true appears at least once across a 30-scene
      run
- [ ] bus.payload.codebook_index correlates with the dominant peak in
      the preceding resonance.payload.top_k (spot-check 3 acts)
- [ ] thought.payload.sensor_contributions is non-empty when the source
      string matches jina|bge_m3|reader_lm|qwen
- [ ] /v1/shader/status returns the current scene name and act index
      synchronously (not via the SSE channel)

### Frontend type sync
- [ ] `cd cockpit && npm run build` succeeds with no TS errors
- [ ] cockpit/src/hooks/useShaderStream.ts imports types whose field
      names match dto_bridge.rs exactly (source_type, top_k,
      cycle_count, tension_history_len, style_trajectory,
      sensor_contributions)
- [ ] /reasoning page renders all five components against a live SSE
      stream; FreeEnergyDial shows F dropping below homeostasis at
      least once per 30-scene run
- [ ] /demo-fallback page still renders with seed stubs when the SSE
      endpoint is unreachable; the LIVE / FALLBACK banner is visible

### NARS truth-value algebra
- [ ] graph_engine::run_nars_deduction no longer contains a literal
      `f1 * f2 * c1 * c2` — the call site is
      causal_edge::TruthValue::deduction(&ab, &bc) (or
      NarsTables::dispatch)
- [ ] Deterministic test: feed
      {(A,is,B,0.9,0.9), (B,is,C,0.8,0.9)}, assert deduction matches
      causal_edge::TruthValue::deduction bit-for-bit (Phase 1's
      hand-rolled formula will diverge in the third decimal place)
- [ ] Revision rule (TruthValue::revision) is exercised when two
      encounter rounds carry the same triplet with different evidence
- [ ] /api/graph/infer?node_id=X returns inferences whose chain
      provenance includes the correct (A,B,C) source triplets

### Architectural compliance
- [ ] `grep -r "neo4j" crates/cockpit-server/src` returns only
      feature-gated `#[cfg(feature = "neo4j-fallback")]` blocks
- [ ] `cargo tree -p cockpit-server | grep -E "lance-graph|thinking-engine|causal-edge"`
      lists all five required crates as direct deps
- [ ] No WireStreamDto / WireResonanceDto / WireBusDto /
      WireThoughtStruct is defined outside dto_bridge.rs
- [ ] No DefaultHasher is used to compute codebook indices in any file
      under crates/cockpit-server/src/
- [ ] Semiring fields do NOT appear in any SSE event payload
      (`grep -r "semiring" cockpit/src` returns nothing)

---

## 7. Notes for Future Sessions

If you are reading this in a future compaction and the cockpit
mysteriously emits beautiful event streams that don't correspond to any
ThinkingEngine call: look first at the imports in
crates/cockpit-server/src/shader_stream.rs. If
`use thinking_engine::ThinkingEngine;` is missing, you are back in
Phase 1 theatre. Restore from this plan.

If you discover a Phase 2 file that derives Serialize on a struct named
Wire* outside dto_bridge.rs, that's the smell. Either move the struct
into dto_bridge.rs with a From<&thinking_engine::dto::*> impl, or
delete it.

If a future agent proposes "let's just hash the tokens for now", invoke
.claude/rules/architectural-compliance.md and refuse. The codebook is a
real artefact — synthetic when models aren't loaded, but always backed
by the same constant CODEBOOK_SIZE = 4096 declared in BOTH
crates/cockpit-server/src/codebook.rs AND
thinking_engine::engine::CODEBOOK_SIZE. They drift, the system breaks.
