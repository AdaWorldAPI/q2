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

Preferred shape: one trace serves both diagnostic and replay roles. The blocker is trace size — current `JsonTraceObserver` output is already heavy, and traces will be checked in as CI fixtures and attached by users to bug reports. Tracked separately as **bd-5qnj** (related to bd-45yw); the unified-artifact decision in Phase 1 depends on bd-5qnj's size investigation. If size can't be bounded, fall back to a dedicated single-purpose replay artifact.

## Proposed phases (draft)

- **Phase 0 — Test plan** (TDD: write failing tests first).
  - Round-trip test: capture an `ExecuteResult` → serialize through the trace format → deserialize → assert equal. Establishes the fixture format.
  - Replay-engine integration test: register a `ReplayEngine` in a custom `EngineRegistry`, run `EngineExecutionStage` against a recorded trace, assert the pipeline observes the recorded `ExecuteResult` (markdown + `supporting_files` reaching `StageContext.resource_report` is the specific bd-o8pr concern).
  - Recording E2E test: run `q2 render` against a fixture that uses the `markdown` engine (so CI doesn't need R/Python), with replay-recording active; assert a trace artifact is produced; replay it; assert outputs match.
  - Hard-fail-on-miss test: replay against a trace whose recorded input doesn't match the document's input — assert the run fails loudly.
  - End-to-end through `q2 render` using the replay engine to confirm the engine-channel resource path works without R/Python — closes the bd-o8pr Phase 2 gap.

- **Phase 1 — Trace format extension + serialization.**
  - Decide single-trace-serves-both vs. dedicated replay artifact (subquestion above).
  - Extend `quarto-trace` types to carry the engine capture (`engine_name`, `input_qmd`, full `ExecuteResult`).
  - Verify `PandocIncludes` and any other non-`Serde` `ExecuteResult` fields can be serialized; add `Serialize`/`Deserialize` derives where missing.
  - Decide whether `supporting_files` paths are stored as paths-only (relative to a fixture-dir convention) or with bundled content. Per-document granularity makes content-bundling viable; paths-only is smaller but couples fixtures to a checked-in tree of supporting files.

- **Phase 2 — `ReplayEngine` impl.**
  - Implement `ExecutionEngine` for `ReplayEngine`. Constructor takes a deserialized capture; `execute()` validates input matches the recorded input (hard fail otherwise) and returns the recorded `ExecuteResult`.
  - Source-info: replay returns `ExecuteResult` with whatever provenance the recording captured, but the runtime `ExecutionContext.source_info` is whatever the current invocation provides — we explicitly do not try to reconstruct recorded provenance. Document this gap in v1.
  - `is_available()` returns `true` always (no external runtime).
  - Registry: a helper that takes a `Registry` and a capture, returns a registry with `ReplayEngine` substituted under the recorded engine's name, leaving everything else alone.

- **Phase 3 — Recording wiring through `quarto-trace`.**
  - Add the recording hook in `EngineExecutionStage` (or via the existing `PipelineObserver` event surface — preferable if it doesn't require contorting the observer API) so an active `JsonTraceObserver` captures the `ExecuteResult` before/after the engine runs.
  - Activation: extend `activate_trace_from_metadata` (or sibling) to also turn on engine-capture mode when requested. Plus an env var / CLI flag for activation without modifying the document.

- **Phase 4 — Replay activation in CLI / orchestrator.**
  - `q2 render --replay <trace-path>` (or `QUARTO_REPLAY=...`): parse the trace before pipeline construction; substitute `ReplayEngine` in the registry handed to `EngineExecutionStage::with_registry`.
  - Hard-fail loudly with a clear diagnostic if the trace is malformed, the engine name doesn't match a known engine, or the recorded input doesn't match the document.

- **Phase 5 — Migrate at least one bd-o8pr engine-channel test off the mock.**
  - Replace the `MockRenderer` in `orchestrator_engine_channel::orchestrator_drains_engine_report_and_copies_to_output_dir` with a real-pipeline run using `ReplayEngine` against a checked-in trace fixture. Demonstrates the tool actually closes the gap and isn't shelfware.

- **Phase 6 — Docs.**
  - Internal note in `claude-notes/instructions/testing.md` describing how to record and replay traces, plus the source-info caveat.
  - User-facing bug-report section: "How to attach a replay trace when filing an issue."

**Source-info caveat (must be documented in Phase 6):** replayed runs do not restore original-engine source provenance for engine-emitted content. Diagnostics that rely on source mapping into engine output (line numbers in error messages pointing at original `.ipynb` cells, etc.) will not match between a real engine run and its replay. Acceptable for v1; revisit if a use case appears.

## Risks / tradeoffs

- **`ExecuteResult` serializability.** `PandocIncludes` and possibly other fields may not have `Serialize`/`Deserialize` derives today. Add them in Phase 1; if any field can't be serialized cleanly, that's a Phase 1 blocker we surface early.
- **`quarto-trace` integration shape.** Folding the engine capture into `TraceEntry` vs. adding a sibling type is a real design choice. `TraceEntry` is currently per-stage; engine capture is logically per-engine-execution and there's only one of those per document. A sibling on `TraceDocument` is probably the cleaner shape, but it's worth a small spike before committing.
- **Trace artifact size.** Recording an `ExecuteResult` with bundled `supporting_files` content can be large (figures, data files). For small fixtures this is fine; if real-bug traces from users get heavy, we may want compression or a tarball-on-the-side scheme. Deciding paths-only-vs.-bundled in Phase 1 sets the ceiling.
- **Activation surface confusion.** Recording is metadata-driven (`trace: true` and friends); replay is CLI/env-driven (`--replay`). Two different surfaces is appropriate (recording lives with the document under investigation; replay is invoked by whoever is debugging) but needs a clear story in docs so users don't confuse them.
- **Source-info gap is user-visible.** Diagnostic-location tests against replayed runs will diverge from real-engine runs. Documented limitation, but worth flagging because at least one bug report from a user will eventually involve a source-position assertion that can't be replayed.
- **Limited bd-o8pr value if scope creeps.** The originating use case is exercising the engine→`supporting_files`→`resource_report` channel without R/Python. We've expanded scope deliberately (recording, trace integration) because hand-authored fixtures wouldn't help bug-report use cases — but each phase boundary is a checkpoint to ask whether we're still on the bd-o8pr-closing trajectory or building unrelated infrastructure.
