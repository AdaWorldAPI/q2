# Manage trace size for use as replay/regression-test fixtures (bd-5qnj)

**Date:** 2026-05-03
**Beads:** bd-5qnj
**Worktree:** `.worktrees/5qnj-trace-size` (branch `beads/5qnj-trace-size`, based on `main` @ `2b954d75`)
**Status:** Investigation — pending design alignment with user. **Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The bulk distribution is clear, the relationship
to bd-45yw is concrete, and there's a tractable design space (drop
pretty-print → gzip → dedup AST snapshots → minimal replay artifact)
that maps onto well-defined intervention points. The remaining
choices are policy-level questions for the user, not gaps in the
investigation.

The empirical numbers (full report at
`claude-notes/plans/5qnj-trace-size-investigation/measurements.md`)
short summary:

- A 6.1 KB qmd produces a **16.3 MB** pretty-printed trace; 3.16 MB
  minified; 627 KB minified+gz.
- **94% of the data bytes are `DocumentAst` payloads.**
- **42 of those snapshots collapse to 6 distinct contents** —
  no-op transforms re-serialize the same AST 36 times.
- Pretty-print alone is **81% of file bytes**.

So the file is large for cheap-to-fix reasons. The interesting
question is which of the unification roles (diagnostic vs. replay)
each intervention is for.

## Issue context

> "Spun out of bd-45yw (replay engine). Once traces double as replay
> fixtures (and as user-attached bug-report artifacts), trace size
> becomes a real constraint — they will be checked into the repo as
> regression fixtures and posted by users in issues."

- Filed 2026-05-03 by cscheid, P2, type `task`, status `open`.
- Issue is one day old — no risk of stale assumptions.
- Investigation list called out: where the bulk lives, replay vs.
  diagnostics overlap, compression on disk, lazy reads, AST
  diffing/elision, size budgets.

The originating bd-45yw plan (on `beads/45yw-replay-engine`) makes the
unified-artifact question explicit:

> "Preferred shape: one trace serves both diagnostic and replay roles.
> The blocker is trace size … Tracked separately as **bd-5qnj** … the
> unified-artifact decision in Phase 1 depends on bd-5qnj's size
> investigation. If size can't be bounded, fall back to a dedicated
> single-purpose replay artifact."

## Dependency graph

```
bd-5qnj (this) ─ related ─> bd-45yw (replay engine, in_progress)
                              └── discovered-from ─> bd-o8pr (project resources, closed)
```

- **`related: bd-45yw`** is the only edge. bd-45yw is in_progress on
  its own worktree (`beads/45yw-replay-engine`), with phases drafted
  but not started; its Phase 1 explicitly depends on this issue's
  outcome.
- **No incoming `blocks` edges.** The `dep tree` shows bd-45yw's
  downstream graph (bd-o8pr → bd-t3ny / bd-k9i1 / bd-w5os / etc.),
  but those are bd-45yw's lineage, not consumers of bd-5qnj. No
  separate work is currently waiting on this issue.

The graph tells us this is a sizing/format prereq for bd-45yw's
Phase 1 decision. It is not blocking anything else open in the queue.

## What the code looks like today

All references in the issue are still live:

- `crates/quarto-trace/src/lib.rs` — `TraceDocument`, `TraceEntry`
  unchanged from the schema described in the doc comment (lines 11–30).
  `SCHEMA_VERSION = 1`. Adding fields is additive; entry-shape
  changes (delta-encoded entries) bump the version.
- `crates/quarto-trace/src/write.rs:43` — single call to
  `serde_json::to_writer_pretty`. This is the pretty-print source.
- `crates/quarto-core/src/stage/trace.rs::JsonTraceObserver` —
  serializes via `serialize_pipeline_data` (line 366). Each
  `PipelineData` variant produces a JSON object; the `DocumentAst` /
  `AtProfile` variants embed a full AST via
  `pampa::writers::json::write` (line 437). No deduplication, no
  delta encoding, no compression. Native-only (cfg-gated against
  `wasm32`).
- `crates/quarto-core/src/stage/stages/metadata_merge.rs:294` —
  `activate_trace_from_metadata` selects observer based on `trace`
  metadata key.

The "Trace size" decision doc (D8 of
`claude-notes/plans/2026-04-14-trace-viewer-design.md`, lines 174–185)
already anticipates the progression: schema_version is in place;
gzip is the cheap first move; JSON-Patch deltas under `schema_version: 2`
are pre-planned for the diff UI in Phase 4.6. **bd-5qnj is the
forcing function for actually making those moves**, because the
unification-with-replay use case turns "noticeable on disk" into
"part of the regression suite" and "attached to user issues."

## Empirical findings (from the investigation report)

Full numbers in
`claude-notes/plans/5qnj-trace-size-investigation/measurements.md`.
Short summary:

| intervention                                | size (big fixture) | reduction |
|---------------------------------------------|-------------------:|----------:|
| current: pretty-printed JSON on disk        |          16.27 MB  |        1× |
| drop pretty-print (minified JSON)           |           3.17 MB  |      5.1× |
| pretty + gzip                               |           0.93 MB  |       17× |
| minified + gzip                             |           0.63 MB  |       26× |
| minified + dedup ASTs (6 distinct of 42) + gz | (projected) <0.1 MB |     >150× |
| minimal replay artifact (no diagnostic AST) | (projected) <30 KB |     >500× |

Reproduces same shape on the medium fixture (4.5 KB → 15.6 MB pretty);
tiny doc shows the absolute floor (124 B → 620 KB pretty / 13 KB gz)
which sets a lower bound: any document, however small, currently
emits a fixed 47-entry pipeline whose per-entry overhead dominates.

## Proposed phases (draft)

Phase numbering is independent of bd-45yw — the two issues need to
sequence relative to each other, but neither is logically a phase of
the other.

### Phase 0 — Tests / measurement harness (TDD)

- A round-trip test that runs a small fixture through the trace
  pipeline, gzips, reads back, and asserts the trace is byte-identical
  to the in-memory `TraceDocument` after re-decompression. Establishes
  that compression is schema-neutral.
- A "size budget" test: render a small known fixture with `trace: true`,
  assert the resulting trace's compressed size is below a chosen
  ceiling (TBD with user). Acts as a regression gate as we add fields.
- A dedup correctness test (only if Phase 2 happens): roundtripping
  through the dedup encoder produces an identical `TraceDocument`.

### Phase 1 — Cheap wins (schema-neutral)

- Drop `to_writer_pretty` from the on-disk path; switch to
  `to_writer`. Add `quarto trace show --pretty` for human inspection
  on demand (the SPA already formats client-side, so the change is
  invisible in the UI).
- Gzip on disk: write `latest.json.gz` instead of `latest.json`.
  Reader detects by extension. Keep `.json` as fallback for older
  traces and for users who want to `jq` directly.
- Decision needed: does Phase 1 land before bd-45yw starts (so
  recording is small from day 1) or in parallel (Phase 1 ships, then
  bd-45yw consumes the smaller artifact)?

### Phase 2 — DocumentAst dedup under `schema_version: 2`

- Replace per-entry inline AST with `{ ast_ref: <hash> }` plus a
  top-level `asts` map keyed by content hash.
- Optionally: when consecutive entries have the same hash, encode
  follow-ups as `{ ast_ref: "same" }` to avoid even hash payloads.
- Co-design with the bd-45yw `TraceEntry` extension and with the
  Phase 4.6 diff UI of the trace-viewer plan, so that one
  schema-version bump captures both shape changes.
- Migration: keep `schema_version: 1` reader path; new writer
  emits v2; CLI and viewer support both.

### Phase 3 — Minimal replay artifact (joint design with bd-45yw)

This is the core unified-artifact-vs.-split decision. Two options:

- **Unified.** One `latest.json.gz` carries both the diagnostic
  pipeline trace and the replay payload (`engine_name`, recorded
  input, `ExecuteResult`). Replay readers ignore everything outside
  the engine-capture object; diagnostic readers ignore the capture.
  After Phase 1+2, this fits the projected size budgets.
- **Split.** Diagnostic trace stays at `latest.json.gz`; replay
  artifact becomes `latest.replay.json.gz` written separately,
  containing only the engine capture. Smaller per-fixture but two
  artifacts to manage.

Recommendation in the absence of new data: **unified, after Phase 1+2
land.** The numbers project comfortably under both budgets
(checked-in fixture and user-attached). But this needs user sign-off
because it commits us to never re-introducing per-stage AST snapshots
in fixtures — see the design questions below.

### Phase 4 — Lazy load on read (deferred)

If the trace-viewer SPA's perceived load latency on large traces ever
becomes user-visible, switch the reader to streaming mode (skip
unused stage payloads at parse time, fetch on demand). Probably not
needed once Phase 1+2 land.

### Phase 5 — Docs

- Internal note in `claude-notes/instructions/` describing the trace
  format, size budgets, and how to write a regression test against a
  small fixture trace.
- User-facing bug-report section: "How to attach a `latest.json.gz`
  when filing an issue, and what we'll be looking at."

## Open design questions for the user

1. **Size budgets.** Are the provisional ceilings — ≤ 100 KB
   (compressed) for checked-in CI fixtures, ≤ 1 MB (compressed) for
   user-attached bug reports — the right targets? Tighter would push
   us toward the split-artifact option; looser would let us stop at
   Phase 1.
2. **Unified vs. split artifact.** Recommend unified after Phase 1+2.
   But this binds future schema choices: anything we add to the
   trace counts against the replay-fixture budget. Comfortable with
   that constraint, or prefer splitting now to keep replay artifacts
   trivially small?
3. **Pretty-print on disk: keep or drop?** Recommend dropping — the
   SPA formats client-side and `quarto trace show --pretty` covers the
   raw-jq use case. Any reason to keep the on-disk file
   human-readable by default?
4. **Sequencing relative to bd-45yw.** Three options:
   (a) bd-5qnj Phase 1 lands first, then bd-45yw can record into a
       smaller artifact from day 1.
   (b) bd-45yw Phase 1 lands first with the current heavyweight format;
       bd-5qnj Phase 1+2 land later as a size optimization.
   (c) Land them together (one PR, schema_version: 2 + replay capture
       in one cut). Recommend (c) only if we're confident in both
       designs; otherwise (a) — small, schema-neutral compression wins
       are easier to ship in isolation.
5. **Schema bump scope.** When we bump to `schema_version: 2`, do we
   batch in everything we know we want (AST dedup + replay capture +
   any diff-UI prerequisites for trace-viewer Phase 4.6), or treat
   each as its own bump? Bumps are cheap (readers gate on
   `schema_version`); the question is whether we want the audit
   surface of one big v2 or several smaller v2/v3/v4. Recommend one
   v2 if all three changes can land within a few weeks of each
   other; separate bumps if any of them is more than ~a month away.
6. **Snapshot of `aux:` and `transform:` entries.** The trace
   currently produces 30+ `transform:` entries plus `aux:` entries
   (e.g. `aux:crossref-index`). With AST dedup, these become almost
   free. Without it, do we want to elide no-op transforms entirely
   (don't write a TraceEntry if the AST didn't change)? Risk: the
   diff UI may want to know that a transform ran even if it was a
   no-op; eliding makes "did it run?" indistinguishable from "did
   it not match?".

## Risks / tradeoffs

- **Schema bump amortization.** v2 will require trace-viewer +
  `quarto trace show` reader updates. The cost scales with how many
  changes ride on the bump; question 5 controls that.
- **Dedup adds reader complexity.** v2 readers need to resolve
  `ast_ref` hashes against the top-level `asts` map. Manageable, but
  it means `jq` users need a stage like `jq '.asts[.pipeline[i].ast_ref]'`
  rather than `jq '.pipeline[i].data.ast'`. Acceptable IMO; worth
  flagging.
- **Gzipped traces aren't directly `jq`-able.** `zcat trace.json.gz |
  jq` is one extra step. We can soften with a `quarto trace cat` that
  decompresses to stdout. Not blocking.
- **Eliding no-op stages would change the diff UI's source data.** If
  Phase 4.6 ever ships a "what did this stage produce?" view, an
  elided no-op stage shows nothing — which may be exactly right
  ("the AST didn't change") or confusing ("did the stage run?").
  Decision deferred to question 6.
- **bd-5qnj does not in itself produce the engine-capture payload
  bd-45yw needs.** Phase 3 does that, but only after Phase 1+2 prove
  the size budget is achievable. If question 4 picks (b), we may
  ship the replay capture in bd-45yw before Phase 2 dedup is in
  place — which is fine, just worth flagging that the worst-case
  size during that window is what bd-45yw Phase 1's "is the
  artifact small enough?" check has to look at.
- **The 16 MB → 0.6 MB result depends on the document.** Documents
  with heavy supporting_files content (jupyter outputs, plot
  PNGs as base64) will not shrink as much, because the `data`
  payload share grows. We should re-measure on a representative
  jupyter trace before committing budgets.
