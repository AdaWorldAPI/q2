# Replay engine: deterministic in-Rust engine for tests (bd-45yw)

**Date:** 2026-05-03
**Beads:** bd-45yw
**Worktree:** `.worktrees/45yw-replay-engine` (branch `beads/45yw-replay-engine`, based on `main` @ `b77c5674`)
**Status:** Investigation — pending design alignment with user. **Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The engine trait surface is small and well-shaped, the parent issue (bd-o8pr) gives concrete motivation, and a clean fixture-driven approach maps onto the existing `EngineRegistry::register` extension point without touching the pipeline. Phases below are a sketch; the open questions need user alignment before we lock the fixture format and decide what scope of "engine output" the replay covers.

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

## Proposed phases (draft)

Skeleton only — actual phase contents wait on the design discussion.

- **Phase 0 — Test plan** (TDD: write failing tests first).
  - Round-trip test: capture an `ExecuteResult` → serialize to fixture → deserialize → assert equal. Establishes the fixture format.
  - Replay-engine integration test: register a `ReplayEngine` in a custom `EngineRegistry`, run `EngineExecutionStage` with a fixture-backed input, assert the pipeline observes the recorded `ExecuteResult` (markdown + `supporting_files` reaching `StageContext.resource_report` are the bd-o8pr concerns).
  - End-to-end through `q2 render` (or equivalent) using the replay engine to confirm the engine-channel resource path works without R/Python — closes the bd-o8pr Phase 2 gap.

- **Phase 1 — Fixture format + serialization.**
  - Define on-disk schema for a recorded transcript: input QMD + the full `ExecuteResult`. Likely JSON or TOML with embedded supporting-file references. Decide whether `supporting_files` are paths-only (lazily resolved against fixture dir) or content-bundled.
  - Verify `PandocIncludes` and any other non-`Serde` fields can be serialized; add `Serialize`/`Deserialize` derives where missing.

- **Phase 2 — `ReplayEngine` impl + lookup strategy.**
  - Implement `ExecutionEngine` for `ReplayEngine` keyed by `(engine_name, input_hash)` against a fixture directory.
  - Decide registry-lookup behavior: does `ReplayEngine` masquerade as `jupyter`/`knitr` (replacing them in tests) or register under its own name? Affects how docs declare it.
  - Mismatch-on-miss policy: hard fail vs. fall back to passthrough vs. invoke real engine to record. (Each enables a different workflow.)

- **Phase 3 — Recording mode (optional, decide if in scope).**
  - A `RecordingEngine` wrapper that intercepts a real engine's call, persists the result to disk, then forwards the result. Lets users update fixtures by re-running with `QUARTO_RECORD_FIXTURES=1`.
  - Without this, fixtures are hand-authored or produced by an offline `q2 fixture record` subcommand.

- **Phase 4 — Migrate at least one bd-o8pr engine-channel test off the mock.**
  - Replace the `MockRenderer` in `orchestrator_engine_channel::orchestrator_drains_engine_report_and_copies_to_output_dir` with a real-pipeline run using `ReplayEngine`. Demonstrates the tool actually closes the gap and isn't just shelfware.

- **Phase 5 — Docs.**
  - Internal note in `claude-notes/instructions/testing.md` describing how to record and use fixtures.

## Open design questions for the user

1. **Engine-name strategy.** Should `ReplayEngine` register under the name of the engine it replaces (`jupyter`, `knitr`) — so tests can keep authoring `engine: jupyter` and the registry decides at runtime — or under its own canonical name (e.g. `replay`) — so the test author opts in explicitly via `engine: replay` (or via document metadata)? The first is more transparent for CI; the second is more honest about what's actually running.

2. **Fixture granularity: per-document or per-cell?** A per-document fixture records the full `ExecuteResult` for one input QMD. A per-cell fixture records each code-cell execution (output text, mime bundles, supporting files) and the replay engine reassembles them into markdown. Per-document is dramatically simpler; per-cell is closer to what would let us reproduce flaky engine bugs at cell granularity. Which use case matters more for the first cut?

3. **Recording: live capture, offline tool, or hand-authored?**
   - Live capture: `RecordingEngine` wraps a real engine, persists alongside running. Easy refresh, but writes during normal renders.
   - Offline tool: a `q2 fixture record path/to/doc.qmd --engine jupyter` subcommand. Cleaner separation but more code.
   - Hand-authored only: fixtures are checked-in JSON. Smallest scope but tedious to update.
4. **Miss policy.** When the replay engine can't find a fixture for a given input, should it (a) hard-fail loudly, (b) fall back to passthrough (markdown engine behavior), or (c) invoke a real engine and record? Choice interacts with question 3.

5. **Scope for the first cut.** Smallest useful slice is probably: hand-authored per-document fixtures + replay-only (no recording) + hard-fail on miss + replaces `jupyter`/`knitr` in test registries only. Is that the right starting point, or is there a use case (e.g. flaky-bug repro) that pushes us toward including recording in v1?

6. **Source-info handling.** `ExecutionContext` carries `SourceInfo` and `Arc<SourceContext>`. Should the replay engine (a) reuse what's in `ctx` and ignore the recorded provenance, (b) restore recorded provenance to match the original engine run exactly, or (c) ignore source mapping in fixtures entirely (acceptable for resource-channel tests but breaks anything that asserts diagnostic source locations)?

## Risks / tradeoffs (draft)

- **`PandocIncludes` serializability is unknown.** If it isn't `Serialize`/`Deserialize` today, we either add derives (cheap) or invent a fixture-side projection (more design surface). Should be checked early in Phase 1.
- **Engine-name collision in registry.** If a `ReplayEngine` with `name() == "jupyter"` is registered alongside the real `JupyterEngine`, current `EngineRegistry::register` replaces the existing entry (`registry.rs` line ~80 — last-write-wins). That's actually convenient for tests, but the behavior should be documented — silent replacement on a name clash is the kind of thing that bites under refactor.
- **Fixture rot.** Recorded fixtures of real engine output drift as the engines themselves change. Without a recording mode (Phase 3), refreshing fixtures requires manual re-runs against R/Python, which partially defeats the purpose. Worth deciding up-front whether recording is in scope or punted.
- **`KNOWN_ENGINES` is hard-coded.** If the replay engine introduces a new name (`replay`), `detect_engine` won't match a document declaring `engine: replay` — it would return the name as-is and the pipeline would fall back to markdown with a warning. Acceptable for tests, but worth deciding deliberately.
- **Limited bd-o8pr value if scope creeps.** The originating use case is tightly scoped: exercise the engine→`supporting_files`→`resource_report` channel without R/Python. A v1 that delivers exactly that is much smaller than a general-purpose replay framework, and the latter risks becoming an unrelated infrastructure project.
