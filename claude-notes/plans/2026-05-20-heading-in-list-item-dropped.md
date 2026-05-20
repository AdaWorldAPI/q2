# Heading inside list item is dropped

Tracking issue: **bd-zpl4u**

## Overview

Pampa silently drops a heading that appears inside a bullet (or
ordered) list item. The minimal example

```qmd
* # Section 1
```

produces

```json
{ "t": "BulletList", "c": [ [] ] }
```

— a list with one empty item — instead of Pandoc's

```json
{ "t": "BulletList", "c": [ [ { "t": "Header", "c": [1, ["section-1", [], []], [...] ] } ] ] }
```

— a list with one item whose only block is a `Header`.

## Reproduction

```bash
echo '* # Section 1' | cargo run --bin pampa -- --to json | jq '.blocks'
# → [{"c":[[]],"s":0,"t":"BulletList"}]

echo '* # Section 1' | pandoc -f markdown -t json | jq '.blocks'
# → BulletList containing a Header
```

Verified 2026-05-20 on `main` at 6af6809a.

## Root cause

The tree-sitter parse tree is correct: the parser produces

```
list_item
  list_marker_star
  section
    atx_heading
      …
```

The bug is in the **tree-sitter → Pandoc AST converter**, not in the
grammar. In `crates/pampa/src/pandoc/treesitter.rs`, the
`process_list_item` function (lines 262–317) iterates its children
through a `filter_map` that has *no arm* for
`PandocNativeIntermediate::IntermediateSection`. The catch-all
`_ => None` (line 306) silently discards it, so the `Header` block
inside the section is never added to the list item's body.

By contrast, `process_section` in
`crates/pampa/src/pandoc/treesitter_utils/section.rs` (lines 27–29)
*does* handle `IntermediateSection` by extending its block list with
the section's contents. The fix to `process_list_item` should mirror
that pattern.

## Why this matters

Pandoc allows arbitrary block-level content inside list items —
including headings, block quotes, code blocks, and nested lists.
Silently dropping a heading is a correctness bug that produces
data-loss for documents using this construct (e.g. an outline-style
qmd file where each list item is a section heading).

## Test strategy (TDD)

Write the failing test **first**, then implement.

### Phase 1 — Failing tests

- [ ] Add a parser-level test in `crates/pampa/tests/` that parses
      `* # Section 1\n` and asserts the result is a `BulletList`
      with one item whose sole block is a `Header { level: 1, … }`.
      The natural place is the JSON snapshot fixture corpus — add
      a `.qmd` fixture and its `.snap` (let `cargo insta review`
      generate the snapshot on the first failing run).
- [ ] Add a variant test with an ordered list:
      `1. # Section 1\n`.
- [ ] Add a variant test with a sub-heading and following content
      on a *separate* item:
      ```qmd
      * # Section 1
      * regular item
      ```
      to confirm the regression doesn't cascade and that the
      regular item still parses correctly.
- [ ] Run the new tests and confirm they fail in the expected way
      (empty list item bodies / missing `Header`).

### Phase 2 — Implementation

- [ ] In `crates/pampa/src/pandoc/treesitter.rs`, inside
      `process_list_item`'s `filter_map`, replace the catch-all
      `_ => None` with an explicit handling of
      `PandocNativeIntermediate::IntermediateSection(blocks)`.
      Because `filter_map` produces a single `Option<Block>` per
      child, a section containing multiple blocks cannot be
      flat-mapped through it; refactor to a `Vec<Block>` builder
      loop (à la `process_section`) so we can `blocks.extend(…)`.
      Keep the existing arms (`IntermediateBlock`,
      `IntermediateMetadataString`, list-marker handling) working.
- [ ] Decide what to do with truly-unexpected variants. Today the
      wildcard drops them silently. After the refactor, prefer to
      either (a) panic with a descriptive message (matching
      `process_section`'s behavior at line 41) so future regressions
      are loud, or (b) emit a `Q-…` diagnostic. Choose (a) unless
      that breaks an existing test — silent data-loss is worse than
      a panic during development.
- [ ] Run the new tests and confirm they pass.

### Phase 3 — Regression sweep

- [ ] `cargo nextest run -p pampa` — confirm no other snapshots
      regressed. If snapshots changed, review each diff carefully:
      a list item that *should* have been empty becoming non-empty
      is a separate bug; one that *was* missing content gaining it
      is the intended fix.
- [ ] `cargo nextest run --workspace` — monorepo-wide regression
      check (per CLAUDE.md, fixes in `pampa` can affect downstream
      crates like `qmd-syntax-helper`).
- [ ] `cargo xtask verify --skip-hub-build` — Rust-only verify.
      This passes converter changes; not the WASM leg.

### Phase 4 — End-to-end verification

- [ ] Run `cargo run --bin pampa -- -t json` on the minimal
      fixture and on the multi-item variant. Confirm the output
      matches Pandoc's structure (a `Header` block inside the
      list item).
- [ ] If the broader `q2 render` path is affected (it should be,
      since the converter is shared), render a `.qmd` fixture with
      `* # Heading` and grep the HTML for an `<h1>` inside the
      `<li>`.

## Out of scope

- The separate observation that, in the input
  ```
  * ## Section 2
    some text
  ```
  the indented "some text" gets *absorbed into* the
  `atx_heading` node by the grammar (its span runs to row 1, col
  2). That's a tree-sitter grammar issue distinct from this
  converter bug, and should be filed separately if it isn't
  already.
- bd-vet6 (multi-line block quote inside list item fails to
  parse) is a related-but-distinct tree-sitter scanner bug. Not
  this issue.

## Files

- `crates/pampa/src/pandoc/treesitter.rs` — site of the fix
  (`process_list_item`, lines 262–317).
- `crates/pampa/src/pandoc/treesitter_utils/section.rs` — the
  pattern to mirror (`process_section`, lines 15–43).
- `crates/pampa/src/pandoc/treesitter_utils/pandocnativeintermediate.rs`
  — the `PandocNativeIntermediate::IntermediateSection` variant
  definition.
