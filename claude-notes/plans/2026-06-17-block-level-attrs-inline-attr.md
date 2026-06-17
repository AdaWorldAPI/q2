# Block-level attributes via collected trailing `Inline::Attr`

**Strand:** bd-itqcfxc3 (discovered-from bd-38ioql41)
**Date:** 2026-06-17
**Status:** design sketch — separate implementation session, not started

## Motivation

Pandoc's AST attaches `Attr` (id, classes, key-values) to some blocks
(`Header`, `CodeBlock`, `Div`, `Table`, `Figure`) but **not** to others
(`Paragraph`, `Plain`, list items). This blocks several things Quarto wants:

- `<p class="caption">` for a hoisted figure caption (the
  reveal auto-stretch figure work, bd-38ioql41 Case 1).
- `<li class="…">` / per-list-item styling (came up independently elsewhere).
- Block-level attributes generally, produced by pure-AST filters.

We already have the building block: `Inline::Attr(InlineAttr)` — a **standalone
attribute node** carrying a full `Attr` + source info. It was introduced "to
represent commonmark attributes … in places where they are not directly attached
to a block, like in headings and tables" (`inline.rs:35-41`). Today:

- The **parser** emits a trailing `Inline::Attr` for `## Heading {#id .c}` etc.
- **Postprocess folds** it into the block when the block *has* an `Attr` field —
  e.g. `Header` at `postprocess.rs:924-969` pops the trailing `Inline::Attr`
  into `header.attr`.
- For blocks **without** an `Attr` field there is nowhere to fold, and the
  **HTML inline writer drops** `Inline::Attr` (`html.rs:1058`: "These should not
  appear in final output").

## Proposal

Let a filter/transform inject a standalone `Inline::Attr` (typically at the end
of a block's inline content), and teach the **writers** to *collect* those
nodes, strip them from the rendered inline content, and apply the merged `Attr`
to the emitted block element. This gives Attr-less Pandoc blocks the attributes
the AST can't natively carry, without changing the Pandoc block types.

Example AST → HTML:

```text
Paragraph[ Str("This is a caption."), Attr({"", ["caption"], {}}) ]
  ⇒  <p class="caption">This is a caption.</p>
```

## Design questions to resolve (the session's real work)

### 1. Where does collection happen — writers vs a normalization transform?

- **(A) Writer-side collection (user's proposal).** Each block writer scans its
  inlines for `Inline::Attr`, folds them onto the element, and skips them when
  writing content. AST stays plain `Para + trailing Attr` (round-trips, simple).
  **Cost:** *every* renderer must implement the same rule — HTML writer,
  `native`/`json` writers (already special-case `Inline::Attr`), **and the
  hub-client React leaves** — or `q2 render` and `q2 preview` diverge. This is
  the same preview-parity trap the reveal work hit; it is the dominant risk.
- **(B) Normalization transform.** A late AST pass folds a trailing
  `Inline::Attr` into a carrier the writers already attribute — but `Para` has
  no `Attr` field, so the carrier would be a `Div`/`Span`/custom node, changing
  the DOM (`<div class="caption"><p>…</p></div>`), which is *not* what we want.
- **Leaning (A)** for fidelity, but the parity cost must be paid deliberately:
  enumerate every writer/renderer and cover them in one change, with a shared
  helper so the rule lives in one place.

### 2. Collection semantics

- **Which nodes:** all `Inline::Attr` in the block, or only a trailing run?
  (Heading precedent is trailing-only.) Leaning: collect *all*, but document.
- **Merge rule:** classes accumulate (in order), ids — last non-empty wins (or
  error on conflict?), key-values — last wins.
- **Whitespace:** strip the `Space`/`SoftBreak` immediately preceding a
  collected `Inline::Attr` so no trailing space leaks into the element text.
- **Empty-attr no-op:** an empty `Inline::Attr` collects to nothing (today
  postprocess already treats empty attrs specially — see `is_empty_attr`).

### 3. Which blocks, in what order

- `Paragraph` first (unblocks the figure caption).
- `Plain` and **list items** next (the `<li>` styling case).
- Consider `BlockQuote` and others later; keep the helper block-agnostic.

### 4. Source mapping & round-trip

- `InlineAttr` already carries `attr_source` + `source_info`. For filter-injected
  attrs the source is `Generated{by:…}`. The HTML change is output-only; the
  `qmd` writer already round-trips `Inline::Attr` (`qmd.rs:2393`). Verify the
  collected-and-applied path keeps source info coherent for the source map.

## Affected code (survey)

- `crates/quarto-pandoc-types/src/inline.rs` — `Inline::Attr` / `InlineAttr`.
- `crates/pampa/src/writers/html.rs:1058` — currently drops `Inline::Attr`;
  block writers for `Paragraph`/`Plain` at `:1282`/`:1286`.
- `crates/pampa/src/writers/{native,json,plaintext,ansi,incremental}.rs` —
  each already has an `Inline::Attr` arm; align behavior.
- `crates/pampa/src/pandoc/treesitter_utils/postprocess.rs:924-969` — the
  existing Header fold precedent (mirror its semantics).
- **hub-client** React leaves that render `Paragraph`/list items — must honor
  the same collection rule for preview parity (locate during the session).

## Test plan (TDD)

- Writer unit tests: `Paragraph` with a trailing `Inline::Attr{.caption}` →
  `<p class="caption">…</p>`; multiple attrs merge; empty attr is a no-op;
  preceding space stripped; list item gets `<li class>`.
- Parity test: the same AST through the q2-preview path yields the same element
  attributes (guards the multi-renderer risk).
- Round-trip: `Inline::Attr` still survives qmd round-trip unaffected.

## Relationship to other work

- **bd-38ioql41** (reveal figure auto-stretch) — Case 1's caption can then be a
  proper `<p class="caption">` (plan option (c)). That strand can ship first
  with a plain `<p>` and adopt this capability when ready; not a hard blocker,
  hence `discovered-from`/related rather than `blocks`.
- The `<li>` styling need (raised separately) is a second consumer; design the
  helper to serve both.
