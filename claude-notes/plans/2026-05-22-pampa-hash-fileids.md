# Migrate pampa to hash-based FileIds

**Status:** drafting — pending review
**Beads:** bd-ky14a
**Discovered-from:** bd-1pwy8 (UnknownTheme structured diagnostic)
**Workflow:** **PR-reviewed, NOT direct-to-main** — coordinate with
the parallel source-location workstream before merging.

## Motivation

While wiring `SassError::UnknownTheme` through the structured
diagnostic system, we discovered an asymmetry in how two parsers
assign `FileId`s:

| Producer | FileId scheme | Globally meaningful? |
| --- | --- | --- |
| `quarto_yaml::parse_file` | `hash(filename)` (via `quarto_yaml::file_id_for_filename`) | yes — same path → same FileId in any `SourceContext` |
| pampa's `ASTContext` | sequential (`FileId(0)` for the primary file in each context) | no — `FileId(0)` means different files in different contexts |

When a `SourceInfo` produced by pampa is consumed by code that
needs to render an ariadne snippet from a *fresh* `SourceContext`
(e.g. `sass_error_to_parse_error` or any future bridge into
hub-client / q2-preview / a diagnostics endpoint), the consumer
can't tell which file `FileId(0)` refers to without out-of-band
knowledge of where the SourceInfo came from.

## Symptoms already in-tree

1. **`sass_error_to_parse_error` signature**
   (`crates/quarto-core/src/theme_diagnostic.rs:51`):

   ```rust
   pub fn sass_error_to_parse_error(
       err: &SassError,
       candidate_sources: &[(FileId, &Path)],   // ← (FileId, &Path) pairs
   ) -> ParseError
   ```

   Each caller declares the FileId binding for each candidate path
   because the converter can't infer the scheme from the diagnostic
   alone. The CompileThemeCssStage call site has to know that
   `_quarto.yml` lives under `quarto_yaml::file_id_for_filename(...)`
   and the document lives under `FileId(0)`.

2. **`include_expansion.rs:162-211`** maintains two parallel
   `SourceContext`s (`doc.ast_context.source_context` and
   `doc.source_context`) in lockstep, with a `debug_assert_eq!` to
   catch desync, and calls `quarto_ast_reconcile::remap_file_ids` on
   every included sub-AST to rewrite `FileId(0)` to a freshly
   assigned sequential ID. Under hash-based FileIds the included
   document would land at `FileId(hash("/path/to/sub.qmd"))`
   natively, requiring no remap.

3. The `current_file_id()` method
   (`crates/pampa/src/pandoc/ast_context.rs:96-98`) has a comment
   reading "Future flexibility - if we need to track current file
   differently" — i.e. the original authors knew the design was
   not the long-term shape.

## Proposed design

Have pampa adopt the same `hash(filename)` scheme `quarto_yaml`
already uses. Concrete shape:

### 1. `ASTContext` constructors compute the hash

```rust
// crates/pampa/src/pandoc/ast_context.rs

impl ASTContext {
    pub fn with_filename(filename: impl Into<String>) -> Self {
        let filename_str = filename.into();
        let file_id = quarto_yaml::file_id_for_filename(&filename_str);

        let mut source_context = SourceContext::new();
        source_context.add_file_with_id(file_id, filename_str.clone(), None);

        ASTContext {
            filenames: vec![filename_str],
            example_list_counter: Cell::new(1),
            source_context,
            parent_source_info: None,
            // NEW: cache the primary FileId so `current_file_id`
            // doesn't have to reach into source_context every call.
            primary_file_id: file_id,
        }
    }

    pub fn current_file_id(&self) -> FileId {
        self.primary_file_id   // was: FileId(0)
    }
}
```

`new()` and `anonymous()` (which use `"<unknown>"` /
`"<anonymous>"` placeholder names) keep working — they'd produce
`FileId(hash("<unknown>"))` etc. Multiple anonymous contexts share
the same FileId, but they should never be merged, so that's fine.

### 2. Replace the 50 literal `FileId(0)` references in pampa

All instances of `FileId(0)` and `quarto_source_map::FileId(0)` in
the pampa source tree should become `context.current_file_id()`
(or `context.primary_file_id()` where appropriate). A few places
in `location.rs` use `FileId(0)` as a hard fallback when no
context is available; those should either receive a context or
explicitly justify the unknown-file case.

Concrete list of files to revisit:

- `crates/pampa/src/pandoc/treesitter.rs` (12 sites, all already
  `context.current_file_id()` — no change)
- `crates/pampa/src/pandoc/location.rs` (4 hardcoded literals:
  `:85, :185, :253, :312`)
- `crates/pampa/src/pandoc/{meta, html, writers/...}` — grep again
  during implementation; the `FileId(0)` count from this plan is
  approximate.

### 3. Simplify `include_expansion.rs`

With hash-based FileIds, including `sub.qmd`:

- Parses with `ASTContext::with_filename("sub.qmd")`, giving its
  primary file `FileId(hash("sub.qmd"))`.
- Adding `sub.qmd` to the parent's `source_context` via
  `add_file_with_id(FileId(hash("sub.qmd")), ...)` produces the
  same FileId.
- No remap step. The two parallel SourceContexts can be merged
  (or kept separate but trivially synchronized via the shared
  FileId).

The `remap_file_ids` call in `include_expansion.rs:206` becomes
unnecessary. The `debug_assert_eq!` becomes a tautology and can be
deleted. Net: ~30 lines simpler.

### 4. Public-API impact

- `ASTContext` is an exported type. Adding a field is a
  backwards-incompatible struct change for anyone constructing it
  directly; the public constructors (`new`, `with_filename`,
  `anonymous`) hide the field. Document.
- `current_file_id()` keeps its signature. Existing callers see
  the same `FileId` type — they just no longer get `FileId(0)`.
  If anyone was matching on the literal `FileId(0)` (rather than
  comparing via `==`), they'd break. **TODO during impl**: grep
  for that pattern.
- `SassError::with_location` and `sass_error_to_parse_error` from
  bd-1pwy8: the helper signature could be simplified after this
  lands (`(&[Path])` once both parsers agree on the scheme).
  Out of scope for this issue.

## Test strategy

1. **No new functional tests required** — existing tests cover
   the parser surface, and the migration is a pure
   identifier-scheme change. Behavior is unchanged.
2. **Lock in the new contract**: add one test in `pampa` that
   asserts `ASTContext::with_filename("foo.qmd").current_file_id()
   == quarto_yaml::file_id_for_filename("foo.qmd")`. This is the
   one invariant the next bridge consumer will rely on.
3. **Drop the `debug_assert_eq!` desync check** in
   `include_expansion.rs` as part of the simplification, but
   first add a test that includes a sub-document and asserts a
   diagnostic in the sub-doc resolves to a SourceContext entry
   with the right path (this currently passes via the lockstep
   assertion; under hash FileIds it should pass without it).
4. Run full `cargo nextest run --workspace` + `cargo xtask
   verify`. The "no functional change" claim is testable: if
   tests fail, it means we changed semantics somewhere.

## Coordination

**The user has parallel source-location work in flight.** Before
merging, this PR should be reviewed against that workstream to
ensure no interaction. Specifically:

- Check whether the parallel work also touches `ASTContext` or
  `SourceContext` shape — if it adds fields, conflict resolution
  is needed but mechanical.
- Check whether the parallel work introduces new `FileId(0)`
  literals — those should be updated as part of the merge.
- Check whether anything in the parallel work depends on
  `FileId(0)` as a specific value (e.g. an invariant or stable
  serialization key).

## Work items (when implementation lands)

- [ ] Create implementation branch off main (NOT direct-to-main).
- [ ] Probe `FileId(0)` references in pampa with a fresh grep
      against current HEAD — the count above may have drifted.
- [ ] Add `primary_file_id: FileId` field to `ASTContext`;
      update three constructors.
- [ ] Change `current_file_id()` to return `self.primary_file_id`.
- [ ] Replace any literal `FileId(0)` in pampa source that should
      reference the context's primary file.
- [ ] Simplify `include_expansion.rs:162-211` — drop the remap +
      parallel-SourceContext invariant.
- [ ] Optional follow-up: simplify
      `sass_error_to_parse_error` signature to `(&[Path])` now
      that all candidates share the same FileId scheme. (Could be
      a follow-up issue.)
- [ ] Add the one invariant test
      (`with_filename(x).current_file_id() ==
      file_id_for_filename(x)`).
- [ ] Run `cargo xtask verify` — full pass, not Rust-only —
      because this touches the WASM build's pampa.
- [ ] Open PR. Include reproducer-quality test: a multi-file
      project with include-expansion + a cross-document diagnostic
      should render correctly under both schemes.

## Risks

- **Parallel source-location work** may have its own opinions
  about FileId schemes. Don't merge in a vacuum.
- **Serialized FileIds** in q2-preview JSON or attribution data
  may suddenly become 16-20-digit numbers (hash output) instead
  of small integers. Check whether anything downstream
  pretty-prints or asserts on the bit width.
- **Test cases that hardcode `FileId(0)`** in `assert_eq!` will
  break. Grep for them and decide per-site whether to update or
  delete.
- **Hash collisions**: extremely unlikely for distinct file paths
  (`DefaultHasher` is 64-bit; ~`2^32` distinct paths before a
  collision becomes ~50% likely). Not worth defending against.
- **Anonymous ASTContexts collide on FileId**: multiple
  `ASTContext::anonymous()` instances share
  `FileId(hash("<anonymous>"))`. If any code path puts both into
  the same `SourceContext`, `add_file_with_id` will panic. Audit
  during implementation.