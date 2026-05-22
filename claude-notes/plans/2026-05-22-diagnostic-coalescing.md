# Cross-page diagnostic coalescing

**Status:** drafting — pending user review
**Parent:** [theme-diagnostic epic](2026-05-22-theme-diagnostic-epic.md)
**Beads:** bd-9hlja
**Depends on:** [structured theme diagnostic](2026-05-22-theme-diagnostic-structured.md) (bd-pgczr)
(for a concrete and high-value test case; the coalescing API itself
doesn't strictly require it).

## Goal

When the same diagnostic — same `Q-X-Y` code, same source location —
is produced by N pages, the CLI should print it **once**, with a list
of affected pages. For `quarto render external-sources/quarto-web`
today this would turn ~280 identical lines into one ariadne block
plus a single "Affected: a, b, c (and 277 others)" line.

## Where it plugs in

`ProjectRenderSummary`
(`crates/quarto-core/src/project/orchestrator.rs:358-373`) already
collects per-page output. Two channels carry diagnostics:

- `summary.pass2_failures: Vec<FileFailure>` — a page that failed to
  render. Each `FileFailure` has `input: PathBuf`, `diagnostics:
  Vec<DiagnosticMessage>`, `source_context: Option<SourceContext>`.
- `summary.outputs[i].render_output.diagnostics` — non-fatal
  per-page diagnostics from a successful render.

The CLI walks both at `crates/quarto/src/commands/render.rs:704-735`
and emits each unchanged. The coalescer sits **right before that
loop**: build a grouped view of all incoming `(input_path,
diagnostic)` pairs, then print groups.

The summary structure does not change. The CLI's render-summary
printer changes.

## Design

### Coalescing key

```
LocationKey  (derived from the diagnostic's SourceInfo)
```

Per the epic, the source location *is* the primary key. No code or
title in the key — two diagnostics pointing at the same span are
presumed the same error in v1. The simpler key buys clarity; the
risk of an accidental collision (two unrelated checks landing on the
same span) is low and easy to fix later if it materializes.

For the `Original { file_id, start_offset, end_offset }` shape —
which is what the theme diagnostic produces — `LocationKey` is the
obvious tuple. For `Substring`, walk the parent chain to the root
`Original` and use its key (with the substring offsets composed).
For `Concat` and `FilterProvenance`: opt out — these diagnostics are
"not coalescable" and pass through to the legacy per-page emission
unchanged. That's a deliberate first-cut: those shapes are rare and
the conservative behavior (don't coalesce when in doubt) is safe.

Diagnostics with **no** `location` also pass through as singletons
(no location → no key → cannot coalesce). The theme diagnostic
always has a location once bd-pgczr lands.

`SourceInfo` derives `PartialEq` but not `Hash`
(`crates/quarto-source-map/src/source_info.rs:21`). We will:

- **Not** add `Hash` to `SourceInfo` itself (the `Concat`-with-`Vec`
  case would force a non-trivial impl and we don't need it).
- Define `LocationKey` locally in the coalescer (or in
  `quarto-error-reporting`) and implement `From<&SourceInfo>` →
  `Option<LocationKey>` returning `None` for the non-coalescable
  shapes.

### Output shape

For a group of N affected paths:

```
Error: [Q-14-1] Invalid theme configuration
   ╭─[ _quarto.yml:685:5 ]
685 │     theme:
   │     ─┬───
   │      ╰── theme must be a string or array of strings
   ╰─
Affected files: 404.qmd, about.qmd, docs/advanced/index.qmd (and 277 others)
```

- The ariadne body is printed verbatim from any one of the group's
  diagnostics (they're identical by construction).
- The "Affected files:" line is appended after the ariadne report,
  with a default cap of **3 names + count of the rest** (constant,
  tunable in code; the user can override the cap later if they want).
- Paths are relativized to the project root for readability.

For groups of size 1, print as today (no "Affected:" line — it would
just repeat the file already shown in the ariadne header).

### API

A standalone module — proposal:
`crates/quarto-error-reporting/src/coalesce.rs`:

```rust
pub struct CoalescedDiagnostic {
    pub representative: DiagnosticMessage,
    pub source_context: Option<SourceContext>,
    pub affected_files: Vec<PathBuf>,   // in the order encountered
}

pub fn coalesce_by_source(
    input: impl IntoIterator<Item = (PathBuf, DiagnosticMessage, Option<SourceContext>)>,
) -> Vec<CoalescedDiagnostic>;
```

`CoalescedDiagnostic` also gains a `to_text(...)` analogous to
`DiagnosticMessage::to_text`, which renders the ariadne body and
appends the "Affected files:" line.

Single-element groups produce a `CoalescedDiagnostic` whose
`affected_files.len() == 1`; the formatter omits the "Affected" line
in that case. That keeps the caller simple — one path for everything.

### Where the CLI calls it

`crates/quarto/src/commands/render.rs:704-735`
(`print_render_diagnostics`). Replace the two loops over
`pass2_failures` and `outputs[i].render_output.diagnostics` with:

```rust
let entries = summary.pass2_failures.iter().flat_map(|f| {
    f.diagnostics.iter().map(move |d| (f.input.clone(), d.clone(), f.source_context.clone()))
}).chain(summary.outputs.iter().flat_map(|r| {
    r.render_output.diagnostics.iter()
        .map(move |d| (r.input_path.clone(), d.clone(), Some(r.render_output.source_context.clone())))
}));
for group in coalesce_by_source(entries) {
    eprintln!("{}", group.to_text());
}
```

The legacy "error: <path>: <plain string>" fallback is still needed
for failures whose `diagnostics` is empty (any non-structured
remaining error path, including sass errors we haven't migrated yet
and any future stragglers). Keep that loop, but only for failures
where `diagnostics.is_empty()`.

## Test plan (TDD)

1. **Unit tests in `quarto-error-reporting::coalesce`**:
   - Two diagnostics with identical (code, file_id, offsets, title)
     coalesce into one group of size 2 with both files listed in
     input order.
   - Different codes do not coalesce.
   - Different offsets do not coalesce.
   - A `SourceInfo::Concat` diagnostic passes through as a singleton
     (non-coalescable).
   - Cap behavior: 5 files with cap=3 → "a, b, c (and 2 others)".
   - Singleton groups omit the "Affected" line.

2. **End-to-end test** through the CLI surface: a 3-page fixture
   project with a single `_quarto.yml` theme error → assert the
   captured stderr contains exactly one ariadne block and one
   "Affected files:" line listing all three pages.

3. **Manual verification**: run
   `cargo run --bin q2 -- render external-sources/quarto-web 2>&1 | rg -c "Invalid theme"`
   and expect `1` (down from ~280). Record output snippet in this plan.

## Work items

- [ ] Write red coalescer unit tests.
- [ ] Implement `LocationKey` + `coalesce_by_source` +
      `CoalescedDiagnostic::to_text` in `quarto-error-reporting`.
- [ ] Decide and document the affected-files display cap
      (proposed: 3 + count). Tiny test.
- [ ] Wire the coalescer into `print_render_diagnostics` in
      `crates/quarto/src/commands/render.rs`. Preserve the legacy
      "no diagnostics" fallback.
- [ ] Verify the `outputs[i].input_path` field exists / is the right
      name (cite-and-correct during impl; the orchestrator returns
      `RenderToFileResult` so check that type's fields).
- [ ] Snapshot test of the coalesced text output.
- [ ] Run `cargo xtask verify --skip-hub-build`.
- [ ] Record manual run output against quarto-web in this plan.

## Risks and edge cases

- **Order stability.** The user wants the listed file order to be
  deterministic. Use the encounter order from
  `summary.pass2_failures`/`outputs` (which is itself in
  `project.files` order). Don't sort alphabetically — sort order
  changes when files are added.
- **Different errors at the same location.** In v1 the key is
  location-only, so two distinct diagnostics at the same `SourceInfo`
  *would* merge into one group with a mixed-content representative.
  We accept this v1 risk explicitly (per the epic's decision); the
  representative is whichever diagnostic was encountered first. If
  this turns out to bite in practice we widen the key to
  `(location, code)`. Add a unit test that documents the v1 behavior
  so a future widening surfaces as an explicit test update.
- **Diagnostics without a code.** Code is `Option<String>`; the
  coalescing key includes the `Option`, so two code-less diagnostics
  with the same location and title still coalesce.
- **Source-context divergence.** If two pages bring different
  `SourceContext`s into a group, pick the representative's
  (`representative.source_context`). The ariadne render only needs
  the underlying file content, which is the same for the shared
  `FileId`. Add a debug-assert that the source-text bytes match for
  any shared file id, in case we get this wrong.
- **`Concat`/`FilterProvenance`.** The first cut keeps these as
  pass-throughs. Recheck after the theme work to see whether the
  rest of the diagnostic surface ever exercises them at scale.
