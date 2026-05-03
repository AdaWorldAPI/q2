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

- [ ] Round-trip test in `quarto-trace`: build an in-memory `TraceDocument` carrying an engine capture (`engine_name`, `input_qmd`, full `ExecuteResult`), `write_trace` to a tempfile, `read_trace`, assert deep equality.
- [ ] Replay-engine unit test in `quarto-core`: construct a `ReplayEngine` from an in-memory capture; call `execute(input, ctx)` with matching input; assert returned `ExecuteResult` equals the recorded one.
- [ ] Replay-engine miss test: same as above but with non-matching input; assert `execute` returns a hard `ExecutionError` (no fallback).
- [ ] Replay-engine integration test: build an `EngineRegistry` with `ReplayEngine` substituted under `"markdown"`, run the full `EngineExecutionStage` against a `StageContext`, assert `ctx.resource_report` receives the recorded `supporting_files` tagged `ResourceOrigin::Engine`. (Closes the specific bd-o8pr gap.)
- [ ] Recording E2E test through `q2 render`: render a fixture with `trace: true`, assert the produced trace contains an engine capture with the document's input QMD and a non-empty `ExecuteResult`. Use the `markdown` engine so CI doesn't need R/Python.
- [ ] Replay E2E test through `q2 render`: take a trace produced by the recording test, run `q2 render --replay <trace>` against the same input, assert the rendered output matches and assert the run did not invoke the real engine (verifiable by registry inspection or by replaying a trace whose engine name is one we deliberately omitted from the registry).
- [ ] Hard-fail-on-miss E2E: replay a trace against a *different* input QMD; assert the CLI exits non-zero with a clear diagnostic.

### Phase 1 — Trace format extension

- [ ] Audit `ExecuteResult` for `Serialize`/`Deserialize` derives. Specifically check `PandocIncludes` and any nested types it owns. Add derives where missing. If any field is structurally non-serializable, surface immediately — that's a Phase 1 blocker.
- [ ] Decide `supporting_files` representation in the trace: paths-only vs. content-bundled. Per-document granularity makes bundling viable; paths-only requires the fixture dir to live alongside the trace. Default lean: bundled (self-contained traces are easier to attach to bug reports), with a size note pointing at bd-5qnj.
- [ ] Add `EngineCapture { engine_name: String, input_qmd: String, result: ExecuteResult }` (exact field names TBD) to `quarto-trace::lib`. Attach as `Option<EngineCapture>` on `TraceDocument` (per-document, single capture). Confirm with workspace build that adding the field doesn't break existing observers/readers.
- [ ] Round-trip tests from Phase 0 pass.

### Phase 2 — `ReplayEngine` impl

- [ ] New module `crates/quarto-core/src/engine/replay.rs`. Struct `ReplayEngine { capture: EngineCapture }` (or borrows the capture via `Arc`).
- [ ] `impl ExecutionEngine`: `name()` returns the recorded engine's name (so it slots into the registry under the same name); `execute()` validates input matches recorded input verbatim (string equality is fine for v1) and returns the recorded `ExecuteResult` cloned; on mismatch returns a hard `ExecutionError`. `is_available()` returns `true`. `can_freeze()` returns `false`.
- [ ] Source-info handling: `execute` ignores recorded provenance and ignores `ctx.source_info`. Document the limitation in module-level rustdoc; we will surface it again in Phase 6.
- [ ] Registry helper: `EngineRegistry::with_replay(capture: EngineCapture) -> Self` (or similar) — start from a default registry and `register` the replay engine, which (per `EngineRegistry::register`'s last-write-wins semantics) replaces the real engine of the same name.
- [ ] Phase 0 unit + miss + integration tests pass.

### Phase 3 — Recording hook

- [ ] Decide hook location: extend `PipelineObserver` with an `on_engine_executed(&EngineCapture)` event vs. plumb the capture through `StageContext` and have `JsonTraceObserver` pull it at end-of-pipeline. The observer-event approach is closer to the existing tracing seam and avoids `StageContext` growth — start with that.
- [ ] `EngineExecutionStage` emits the capture event after a successful engine run (regardless of which engine ran).
- [ ] `JsonTraceObserver` records the capture into `TraceDocument.engine_capture`. No-op observers ignore the event.
- [ ] Activation: no new metadata key. `trace: true` is sufficient — recording is automatic when tracing is on. Confirm `activate_trace_from_metadata` in `metadata_merge.rs:294` already covers this; if not, extend.
- [ ] Phase 0 recording E2E test passes.

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
