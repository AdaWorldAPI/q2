# Replay engine: deterministic in-Rust engine for tests (bd-45yw)

**Date:** 2026-05-03
**Beads:** bd-45yw
**Worktree:** `.worktrees/45yw-replay-engine` (branch `beads/45yw-replay-engine`, based on `main` @ `b77c5674`)
**Status:** Investigation — pending design alignment with user. **Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design (post-alignment).** The engine trait surface is small and well-shaped, the parent issue (bd-o8pr) gives concrete motivation, and a clean fixture-driven approach maps onto the existing `EngineRegistry::register` extension point without touching the pipeline. Design questions resolved with user 2026-05-03 — see "Resolved design decisions" below. Remaining work is implementation-side scoping: chiefly, deciding the integration point with the existing `quarto-trace` framework.

## Resolved design decisions (2026-05-03)

1. **Engine name.** New name (`replay`), not an override of `jupyter`/`knitr`. The replay engine is positioned as an explicit debugging/QA tool, not a transparent stand-in. Documents (or callers) opt in via env var, metadata flag, or CLI parameter — never silently. Implication: `KNOWN_ENGINES` in `detection.rs:30` will need to either include `replay` or the activation path will bypass `detect_engine` entirely (preferred — see "Activation surface" below).
2. **Granularity.** Per-document. This tool is for capturing the context of an execution to make CI regression tests and user bug reports faster — not a substitute for test reduction.
3. **Recording strategy.** Recording is in v1 — merged with the existing `quarto-trace` framework (`crates/quarto-trace/`, activated via `trace: true` metadata in `metadata_merge.rs:294`). Rationale: stay as close as possible to the environment that triggered the bug; also reuses an existing observer/serialization story instead of inventing a parallel one. Implication: `TraceEntry` (or a sibling artifact) needs to carry the engine `ExecuteResult` payload, and the trace becomes the fixture format.
4. **Miss policy.** Hard, loud fail. No quiet fallback. Replay misses on a debugging tool send investigators on wild-goose chases; we make them impossible.
5. **v1 scope.** Recording-capable v1 is fine, even if it means more phases. Hand-authored fixtures alone don't help users reporting actual bugs; recording does.
6. **Source-info handling.** Ignore in fixtures for v1; document explicitly that this breaks diagnostic-location tests against replayed runs. A future phase can add it back if a use case appears.

## Issue context

> "From the bd-o8pr Phase 2 work session: writing E2E tests for engine-emitted resources (and other engine-channel features) requires either real R/Python/jupyter installs or a custom test injection point. Both are heavy. Idea: build a 'replay engine' that can reproduce the behavior of any existing engine but runs entirely in Rust. Records a real engine's transcript (markdown output, supporting_files, includes, …) into a fixture; replays deterministically without the engine runtime."

- Filed 2026-05-03 by cscheid, P2, type `feature`, status `open`.
- Use cases listed: CI tests without R/Python, reproducing flaky engine bugs, fixture-driven testing of engine-channel features (resources, filters, ExecuteResult fields), testing Jupyter custom kernels.
- Issue is one day old — no risk of stale assumptions.

## Dependency graph

```
bd-45yw (this) ─ discovered-from ─> bd-o8pr (closed: project resources)
                                       ├── related: bd-t3ny (publish, completed)
                                       └── related: bd-k9i1 (non-renderable site resources, open P3)
```

- **discovered-from bd-o8pr** (project resources, closed 2026-05-03): the parent. Phase 2 wired the engine channel for `ExecuteResult.supporting_files`, but the closing notes (lines 524–537 of `claude-notes/plans/2026-05-03-project-resources.md`) explicitly call out the test gap this issue addresses:
  > "Engine-channel E2E (jupyter / knitr) needs either real engine installs or a test injection point. A 'replay engine' — records a real engine's transcript once, replays in pure Rust — would cover this cleanly. Particularly important for Jupyter where custom kernels are common."
- **No incoming `blocks` edges.** Nothing in the open queue currently waits on this — so no urgency pressure from elsewhere. The motivation is the standing engine-channel test gap, not a downstream feature.
- **No `related` edges of its own.** Filed as a standalone tooling issue.

The graph tells us: this is a tooling enabler born from a specific test gap. It is not yet a hard prerequisite for any open work, which means we can shape it for general utility without trying to satisfy a particular consumer first.

## What the code looks like today

All file paths from the issue exist and are still the right entry points:

- `crates/quarto-core/src/engine/registry.rs` — `EngineRegistry::register(Arc<dyn ExecutionEngine>)` is public and already used as the test seam (`EngineExecutionStage::with_registry`). A replay engine drops in here cleanly.
- `crates/quarto-core/src/engine/traits.rs` — `ExecutionEngine` is a small trait: `name()`, `execute(input, ctx) -> ExecuteResult`, `can_freeze()`, `intermediate_files()`, `is_available()`. Nothing exotic to mock.
- `crates/quarto-core/src/engine/context.rs` — `ExecuteResult` has the fields the issue references: `markdown`, `supporting_files: Vec<PathBuf>`, `filters: Vec<String>`, `includes: PandocIncludes`, `needs_postprocess: bool`. All `Clone + Debug + Default`. Serializability would need to be checked for `PandocIncludes`.
- `crates/quarto-core/src/engine/{markdown,knitr,jupyter}/*` — concrete engines to model after. `MarkdownEngine` is the cleanest reference for trait implementation shape.
- `crates/quarto-core/src/engine/detection.rs` — `KNOWN_ENGINES = ["markdown", "knitr", "jupyter"]` is a hard-coded list. A replay engine that reuses an existing name (`jupyter`/`knitr`) would compete with the real one in registry registration order; an engine that introduces a new name (`replay`) wouldn't be matched by `detect_engine` for documents declaring `engine: jupyter`. This is one of the design questions below.
- `crates/quarto-core/src/stage/stages/engine_execution.rs` — `EngineExecutionStage::with_registry(registry)` already exists as the test injection point (line 85). No pipeline-level change is required to plug a replay engine in.

There is **no existing replay/record/fixture-engine infrastructure** in the tree (`grep -rn "Replay\|replay" crates/ --include="*.rs"` returns nothing relevant). The only test-time engine pattern in the tree is the trivial `TestEngine` struct in `traits.rs:134`, which is just a passthrough used to verify the trait compiles.

## Activation surface (design note)

Decision: do not route replay activation through `detect_engine` / document `engine:` metadata. The replay engine is an *out-of-band* debugging mode — the document under investigation should not have to be modified to be replayed. Instead, replay is activated by a CLI flag and/or env var (e.g. `q2 render doc.qmd --replay path/to/trace.json` / `QUARTO_REPLAY=path/to/trace.json`) that overrides whatever engine the document declares. The override happens at the registry level: when replay mode is active, `EngineRegistry::register` substitutes the `ReplayEngine` for whichever name the document declared. This keeps the document untouched and gives one canonical path for activation regardless of which real engine recorded the trace.

Recording is the symmetric story: activated by `trace: true` (existing) plus a new flag indicating the engine `ExecuteResult` should be captured into the trace, or by a dedicated `replay-record: true` metadata key. The exact surface is a Phase 1 detail.

## Trace integration (design note)

The plan now extends the existing `quarto-trace` framework rather than inventing a parallel fixture format. Touch points:

- `crates/quarto-trace/src/lib.rs` — `TraceEntry` (currently per-stage, ~108) gains an optional engine-execution payload, OR a sibling type (e.g. `EngineCapture`) is added on `TraceDocument` carrying `(engine_name, input_qmd, ExecuteResult)`. Whichever design keeps `TraceEntry` clean.
- `crates/quarto-core/src/stage/trace.rs` — `JsonTraceObserver` already serializes pipeline state; extend its `on_stage_end` (or equivalent) for `EngineExecutionStage` to capture the `ExecuteResult` if recording is enabled.
- `crates/quarto-core/src/stage/stages/metadata_merge.rs:294` — `activate_trace_from_metadata` is the existing entry point; either extend it to also activate replay-recording, or factor out a sibling `activate_replay_from_metadata`.
- The replay-input side is read at orchestrator/CLI level (before pipeline construction) — when `--replay <path>` is set, parse the trace and substitute `ReplayEngine` in the registry handed to `EngineExecutionStage::with_registry`.

**Decided 2026-05-03:** unified artifact. One `TraceDocument` serves both diagnostic and replay roles, in memory and at the type level. Size is bd-5qnj's concern, addressed *only* at the serialization boundary (`quarto-trace::write` / `read`) — in-memory shape and replay code see the full structure unchanged. This keeps the replay implementation independent of size work and preserves a clean boundary between "trace storage optimization" and "trace usage in the program."

**Recording activation:** `trace: true` is the single knob. Whenever tracing is on, engine output is captured. We accept the size cost (engine output is useful diagnostic data anyway) and accept the rare-but-real case that a regression needing a huge engine capture may not be storable as a checked-in fixture; that case falls back to a different test-fixture mechanism. Replay activation remains out-of-band (CLI/env), since replay is a debugging mode the document under investigation shouldn't have to know about.

## Proposed phases

### Phase 0 — Test plan (TDD: failing tests first)

- [x] Round-trip test in `quarto-trace`: build an in-memory `TraceDocument` carrying an engine capture (`engine_name`, `input_qmd`, full `ExecuteResult`), `write_trace` to a tempfile, `read_trace`, assert deep equality.
- [x] Replay-engine unit test in `quarto-core`: construct a `ReplayEngine` from an in-memory capture; call `execute(input, ctx)` with matching input; assert returned `ExecuteResult` equals the recorded one.
- [x] Replay-engine miss test: same as above but with non-matching input; assert `execute` returns a hard `ExecutionError` (no fallback).
- [x] Replay-engine integration test: build an `EngineRegistry` with `ReplayEngine` substituted under a synthetic engine name (`mock-replay-engine`), run the full `EngineExecutionStage` against a `StageContext`, assert `ctx.resource_report` receives the recorded `supporting_files` tagged `ResourceOrigin::Engine`. (Closes the specific bd-o8pr gap.)
- [ ] Recording E2E test through `q2 render`: render a fixture with `trace: true`, assert the produced trace contains an engine capture with the document's input QMD and a non-empty `ExecuteResult`. Use the `markdown` engine so CI doesn't need R/Python.
- [ ] Replay E2E test through `q2 render`: take a trace produced by the recording test, run `q2 render --replay <trace>` against the same input, assert the rendered output matches and assert the run did not invoke the real engine (verifiable by registry inspection or by replaying a trace whose engine name is one we deliberately omitted from the registry).
- [ ] Hard-fail-on-miss E2E: replay a trace against a *different* input QMD; assert the CLI exits non-zero with a clear diagnostic.

### Phase 1 — Trace format extension

- [x] Audit `ExecuteResult` for `Serialize`/`Deserialize` derives. **Result:** `ExecuteResult` and `PandocIncludes` were missing derives; both are pure POD (`String`, `Vec<PathBuf>`, `Vec<String>`, `bool`). Added `serde::{Serialize, Deserialize}` derives — no field needed special handling. Workspace builds clean.
- [x] Decided `supporting_files` representation: stored as JSON paths inside the captured `ExecuteResult` (i.e., paths-only). The orchestrator drains them on replay just like a real engine run; the resolver in `project_resources::resolve_reported_resources` handles the rest. Bundling content into the trace is deferred to bd-5qnj's size investigation.
- [x] Added `EngineCapture { engine_name, input_qmd, result }` to `quarto-trace::lib`. Attached as `Option<EngineCapture>` on `TraceDocument` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — pre-existing traces deserialize cleanly. `result` is held as `serde_json::Value` so `quarto-trace` stays leaf-level.
- [x] Round-trip tests pass (`test_engine_capture_roundtrip_through_disk`, `test_engine_capture_absent_by_default`).

### Phase 2 — `ReplayEngine` impl

- [x] New module `crates/quarto-core/src/engine/replay.rs`. `ReplayEngine` holds an `Arc<EngineCapture>`; constructors `new(EngineCapture)` and `from_arc(Arc<EngineCapture>)`.
- [x] `impl ExecutionEngine`: `name()` returns recorded engine name; `execute()` validates input via byte-equality and returns the deserialized `ExecuteResult`; `can_freeze()` → false; `is_available()` → true. Mismatch returns `ExecutionError::ExecutionFailed` with a "replay miss" diagnostic. Malformed `result` JSON likewise returns `ExecutionFailed` with a clear message.
- [x] Source-info handling: documented in module rustdoc as a v1 limitation. `execute()` ignores both recorded provenance and `ctx.source_info`.
- [x] Registry helper `EngineRegistry::with_replay(capture)` — starts from a default registry and registers the replay engine; last-write-wins replacement makes the substitution transparent.
- [x] Phase 0 unit + miss + integration tests pass (9 unit tests + 2 stage-integration tests, including an end-to-end miss assertion through the real `EngineExecutionStage`).

### Phase 3 — Recording hook

- [x] Hook location: reused the existing open-ended `PipelineObserver::on_auxiliary_data` event. No new trait method — the kind/data API is exactly the seam designed for typed-data emission. `ENGINE_CAPTURE_KIND = "EngineCapture"` exported from `stage::stages::engine_execution` so producer and consumer share the constant.
- [x] `EngineExecutionStage` emits the capture event immediately after `engine.execute()` returns, *before* the stage drains `result.includes` into `ctx.includes` and `mem::take`s `supporting_files`. This guarantees the capture reflects the engine's full pre-drain output. Serialization failure of `ExecuteResult` (should not happen — pure POD) is logged via `trace_event!` and does not break the render.
- [x] `JsonTraceObserver::on_auxiliary_data` recognizes `ENGINE_CAPTURE_KIND`, deserializes the payload into `quarto_trace::EngineCapture`, and stores it on `state.doc.engine_capture` (typed slot). Other kinds remain on the open-ended pipeline-aux channel. Malformed payload falls through to the generic aux entry with a stderr warning so investigators see what arrived.
- [x] Activation: no new metadata key. `trace: true` (handled by existing `activate_trace_from_metadata` in `metadata_merge.rs:294`) is sufficient — installing a `JsonTraceObserver` automatically captures the engine output via the aux event.
- [x] Phase 0 recording E2E test passes (`test_engine_execution_records_trace_round_trip_to_disk`): drives `EngineExecutionStage` through a real `JsonTraceObserver`, writes to disk, reads back via `quarto_trace::read::read_trace`, asserts `engine_capture` populated and round-trips back to a usable `ExecuteResult`.

### Phase 4 — Replay activation in CLI / orchestrator

- [ ] CLI flag `--replay <path>` on `q2 render`. Optional env var `QUARTO_REPLAY=<path>` as a parallel surface (useful in scripted CI).
- [ ] When set: read the trace via `quarto-trace::read::read_trace`, extract `engine_capture`, build the registry via `EngineRegistry::with_replay(capture)`, hand it to `EngineExecutionStage::with_registry` in the orchestrator path. Pipeline construction otherwise unchanged.
- [ ] Diagnostics: trace path missing → fail loudly with a useful message. Trace lacks an engine capture → fail loudly. Document's input doesn't match recorded input → fail loudly (this is the miss-policy assertion from Phase 2 firing through the CLI).
- [ ] Phase 0 replay E2E + hard-fail-on-miss tests pass through the real CLI.

### Phase 5 — Migrate one bd-o8pr engine-channel test off the mock

- [ ] Identify the test (`orchestrator_engine_channel::orchestrator_drains_engine_report_and_copies_to_output_dir`). Record a trace fixture for it (using whatever engine is convenient — `markdown` if a no-op fits the test, otherwise a real run on a development machine, checked in).
- [ ] Replace `MockRenderer` with a real-pipeline run via `ReplayEngine` against the checked-in trace.
- [ ] Confirm the migrated test still exercises the engine→`supporting_files`→`resource_report`→output-dir-copy path.

### Phase 6 — Docs

- [ ] Internal note at `claude-notes/instructions/testing.md` (or new file) covering: how to record a trace, how to replay, how to author a regression fixture from a real bug report, the source-info caveat, the size-vs.-bd-5qnj note.
- [ ] User-facing section in the bug-reporting docs: "Attach a replay trace when filing an engine-related issue."

**Source-info caveat (Phase 6 must document):** replayed runs do not restore original-engine source provenance for engine-emitted content. Diagnostics that rely on source mapping into engine output (line numbers in error messages pointing at original `.ipynb` cells, etc.) will not match between a real engine run and its replay. Acceptable for v1; revisit if a use case appears.

## Risks / tradeoffs

- **`ExecuteResult` serializability.** `PandocIncludes` and possibly other fields may not have `Serialize`/`Deserialize` derives today. Add them in Phase 1; if any field can't be serialized cleanly, that's a Phase 1 blocker we surface early.
- **`quarto-trace` integration shape.** Folding the engine capture into `TraceEntry` vs. adding a sibling type is a real design choice. `TraceEntry` is currently per-stage; engine capture is logically per-engine-execution and there's only one of those per document. A sibling on `TraceDocument` is probably the cleaner shape, but it's worth a small spike before committing.
- **Trace artifact size.** Recording an `ExecuteResult` with bundled `supporting_files` content can be large (figures, data files). For small fixtures this is fine; if real-bug traces from users get heavy, we may want compression or a tarball-on-the-side scheme. Deciding paths-only-vs.-bundled in Phase 1 sets the ceiling.
- **Activation surface confusion.** Recording is metadata-driven (`trace: true` and friends); replay is CLI/env-driven (`--replay`). Two different surfaces is appropriate (recording lives with the document under investigation; replay is invoked by whoever is debugging) but needs a clear story in docs so users don't confuse them.
- **Source-info gap is user-visible.** Diagnostic-location tests against replayed runs will diverge from real-engine runs. Documented limitation, but worth flagging because at least one bug report from a user will eventually involve a source-position assertion that can't be replayed.
- **Limited bd-o8pr value if scope creeps.** The originating use case is exercising the engine→`supporting_files`→`resource_report` channel without R/Python. We've expanded scope deliberately (recording, trace integration) because hand-authored fixtures wouldn't help bug-report use cases — but each phase boundary is a checkpoint to ask whether we're still on the bd-o8pr-closing trajectory or building unrelated infrastructure.
