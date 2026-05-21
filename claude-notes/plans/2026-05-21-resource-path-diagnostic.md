# Source-pointing diagnostics for resource-path errors

## Overview

When a `resources:` entry in `_quarto.yml` or in a document YAML
header truly does point outside the project root (a legitimate user
error after we fix the leading-`/` bug — see plan
`2026-05-21-resource-path-leading-slash.md`), the current error is
plain text:

```
Error: resource path '/docs/download/_download.json' resolves outside
the project root '/.../quarto-web'. Project resources must live
within the project directory.
```

This tells the user *what* but not *where*. In `_quarto.yml` the
offending pattern could be one of dozens; in a document header it
could be one of many docs in the project. We want a tidyverse-style
diagnostic with an Ariadne source snippet pointing at the offending
YAML scalar, the same way other Q2 errors do.

## Why this is feasible now

The data is already there:

- `ConfigValue` carries a `SourceInfo` per scalar
  (`crates/quarto-pandoc-types/src/config_value.rs:155`).
- `quarto-error-reporting` already supports source-spanned
  diagnostics with Ariadne rendering (used across `quarto-core` —
  see `crates/quarto-core/src/error.rs`, `project/listing/sort.rs`,
  `project/dependency_graph.rs`).
- The current loss happens at two spots:
  1. `extract_resource_patterns` in
     `crates/quarto-core/src/project_resources.rs:118` flattens
     `ConfigValue` items into `Vec<String>`, discarding source.
  2. The orchestrator wraps `ResourceError` in
     `QuartoError::other(e.to_string())` at
     `crates/quarto-core/src/project/orchestrator.rs:627`, dropping
     even the structured error variant.

## Design

### 1. Preserve `SourceInfo` per pattern

Replace the flat `Vec<String>` with a small struct:

```rust
pub struct RawResourcePattern {
    pub pattern: String,
    pub source_info: SourceInfo,
}
```

Threaded through:
- `ProjectConfig::resources: Vec<RawResourcePattern>`
- `DocumentProfile::resources: Vec<RawResourcePattern>` (matching the
  same change on the doc side; see `crates/quarto-core/src/document_profile.rs`)
- `extract_resource_patterns` returns `Vec<RawResourcePattern>`,
  copying the per-item `source_info`.
- `expand_patterns` takes `&[RawResourcePattern]` and propagates the
  source info into the `ResourceError` variants.

This is intentionally a small, localized refactor — no public
surface beyond these structs needs to change.

### 2. Carry `SourceInfo` into `ResourceError`

```rust
pub enum ResourceError {
    OutOfProject {
        pattern: String,
        project_root: PathBuf,
        source_info: SourceInfo,
    },
    InvalidGlob { pattern: String, source: glob::PatternError, source_info: SourceInfo },
    GlobWalk    { pattern: String, source: glob::GlobError,    source_info: SourceInfo },
}
```

### 3. Render through `quarto-error-reporting`

In the orchestrator, instead of `QuartoError::other(e.to_string())`,
build a `DiagnosticMessage` using
`DiagnosticMessageBuilder::error(...)` with a label at
`source_info`'s span, e.g.:

```
error: resource path resolves outside the project root
   ╭─[_quarto.yml:5:7]
 5 │     - "/docs/download/_download.json"
   ·       ─────────────────┬──────────────
   ·                        ╰── this pattern resolves to '/docs/...'
   ·                            which is outside '/path/to/quarto-web'
   ╰─

i Use a project-relative path. A leading `/` is treated as
  project-root-relative — e.g. `/docs/download/_download.json` means
  `<project>/docs/download/_download.json`.
```

The `info` bullet is exactly the kind of guidance that closes the
loop with the fix from
`2026-05-21-resource-path-leading-slash.md`: once that lands, the
*only* way to hit this error is to have a real out-of-project
pattern, and the `i` line tells the user how to write the right one.

The renderer goes through `ParseError` (in
`crates/quarto-core/src/error.rs`) so we get Ariadne snippets in
text output and structured JSON for `--diagnostics-format json`.

### 4. Test infrastructure

Borrow the snapshot-test pattern already used elsewhere in
`quarto-core`. A fixture project under
`crates/quarto-core/tests/fixtures/resource-out-of-project/` with a
crafted `_quarto.yml` containing a `../escape.csv` entry. The
snapshot captures the rendered diagnostic (without hyperlinks —
`TextRenderOptions { enable_hyperlinks: false, .. }`) so the
filename doesn't leak absolute paths.

## Test plan (TDD)

1. **Unit: `ResourceError::OutOfProject` carries SourceInfo** —
   construct `expand_patterns` directly with a `../escape.csv`
   pattern + a synthesized `SourceInfo`, assert the error variant's
   `source_info` matches.
2. **Unit: project-level YAML preserves source info end-to-end** —
   parse a tiny `_quarto.yml` string via the existing config-parse
   path, run `collect_static_resources`, assert the resulting
   `ResourceError` has line/col matching the YAML scalar.
3. **Unit: document-level YAML preserves source info end-to-end** —
   same shape, but the `resources:` entry lives in a doc header.
   Asserts the diagnostic points into the `.qmd` file, not
   `_quarto.yml`.
4. **Snapshot: rendered diagnostic** — a project-fixture test that
   runs `render_project` to error and snapshots the rendered text.
   Confirms the Ariadne span, the `i` hint, and the tidyverse-bullet
   shape are all stable.
5. **Regression: leading-`/` no longer errors** — depends on
   `2026-05-21-resource-path-leading-slash.md`. A pattern
   `"/data/x.txt"` with file at `<root>/data/x.txt` succeeds.

## Work Items

- [ ] Define `RawResourcePattern` + thread through `ProjectConfig`,
  `DocumentProfile`, `extract_resource_patterns`, `expand_patterns`.
- [ ] Add `source_info: SourceInfo` to `ResourceError` variants.
- [ ] Write tests (1)–(4) above; verify they fail before the
  rendering change.
- [ ] Wire `ResourceError` → `DiagnosticMessage` in the orchestrator
  (or in a `From` impl). Use `DiagnosticMessageBuilder::error` with
  the appropriate `.with_label(source_info, "...")` and `.info(...)`.
- [ ] Re-run tests; all pass.
- [ ] Update the project-resources plan
  `claude-notes/plans/2026-05-03-project-resources.md` with a
  "diagnostics added in bd-..." note.
- [ ] `cargo xtask verify --skip-hub-build` clean.
- [ ] End-to-end: craft a tiny demo project with a real
  out-of-project resource, run `q2 render`, paste the rendered
  diagnostic into the PR description.

## Open questions

- Should the document-YAML diagnostic also include a span into
  `_quarto.yml` when `output_dir` settings affect what counts as
  "inside the project root"? Probably no; the user wrote the bad
  pattern in the doc header, not in `_quarto.yml`. Mentioning it in
  the `i` line is enough.

## Dependencies

- Blocked by: `2026-05-21-resource-path-leading-slash.md` (the bug
  fix is what makes this diagnostic land on *actual* errors instead
  of false positives).
- Related: `2026-05-21-q2-preview-diagnostics-ariadne.md` (recent
  work establishing the same Ariadne-rendering pattern in the
  preview surface).
