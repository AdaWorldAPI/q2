# Block-level attributes via collected trailing `Inline::Attr`

**Strand:** bd-itqcfxc3 (discovered-from bd-38ioql41)
**Date:** 2026-06-17
**Status:** in progress — JSON-writer keystone landed (see Progress below)

## Progress

- **2026-06-17 — JSON transport keystone (done).** The conceptual blocker was
  that the `q2 preview` path is `json.rs` → React, and `json.rs` *errored*
  (`Q-3-32`) on a stray `Inline::Attr`, so a block attr could never reach the
  preview. **Resolved via the "safe extra key" channel** (the same trick the
  `s`/`l` source-info keys use): a `Para` with a trailing `Inline::Attr` now
  serializes to a Pandoc-valid `Para` node carrying an extra `attr` object key
  (`{"attr":[id,classes,kvs], "c":[…stripped inlines…], …}`). **Verified
  empirically** that Pandoc 3.9 ignores the extra key (`pandoc -f json` on the
  writer's literal bytes yields `Para [Str …]` → `<p>…</p>`) — so the wire form
  degrades gracefully for external Pandoc while our React renderer can read it.
  - New shared helper `crates/pampa/src/writers/block_attr.rs`
    `split_trailing_block_attr(&[Inline]) -> (&[Inline], Attr)` — collects the
    trailing `Inline::Attr` run (+ interleaved/preceding whitespace), merges
    (id last-wins, classes accumulate+dedup, kv last-wins), returns the retained
    prefix. Block-agnostic; html.rs and the list-item case will reuse it.
  - Wired into the **streaming** `Para` writer (`json.rs` `stream_write_block`;
    note `write_with_config` uses the streaming path, not the tree `write_block`).
  - Tests: `block_attr` unit tests (semantics) + `test_paragraph_trailing_attr_emits_attr_key`
    (wire format). Full `pampa` suite green (3965), inert for existing docs (no
    paragraph carries a trailing `Inline::Attr` today).
  - **Merge semantics chosen:** trailing-run-only, id last-wins, classes
    accumulate (dedup), kv last-wins. Easy to revisit — it's localized to the
    helper.

### Remaining (next increments)

- [ ] `Plain` streaming writer: same treatment (reveal caption may be a `Plain`).
- [ ] **html.rs** `Paragraph`/`Plain` writers: call `split_trailing_block_attr`,
      `write_attr` on the `<p>`, write the retained content. (`q2 render` path.)
- [ ] **React** `Para.tsx`/`Plain.tsx` (`ts-packages/preview-renderer`): read the
      `attr` key and apply to the wrapper. Dumb read — no merge logic in TS.
- [ ] JSON **reader** fold-back (optional): teach `readers/json.rs` to restore a
      trailing `Inline::Attr` from the `attr` key, so a json→AST→json round-trip
      doesn't silently drop the block attr. Not needed for preview (no read-back).
- [ ] End-to-end parity test: reveal figure-caption fixture renders
      `<p class="caption">` in both `q2 render` and `q2 preview` (q2-slides).
- [ ] List items (`<li class>`) — distinct hoist mechanism; later sub-task.
- [ ] Decide `native.rs` behavior (still errors `Q-3-32`): faithfully print the
      resolved attr, or keep erroring as a debug-only format.



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

## Architecture (confirmed 2026-06-17 — this reframes the risk)

The render and preview paths are **two different renderers**, and the primary
consumer (reveal figure caption) flows through the *non-HTML* one:

- **`q2 render` (to disk)** → `crates/pampa/src/writers/html.rs`. Today it
  **drops** a stray `Inline::Attr` (`html.rs:1058`). It consumes the in-memory
  AST directly and can emit raw `<p class="…">` trivially.
- **`q2 preview` (live iframe)** → `crates/pampa/src/writers/json.rs` produces a
  **Pandoc-shaped JSON tree + an `astContext` sidecar**, which is rendered by a
  **separate React renderer**, `ts-packages/preview-renderer/` — *not* by
  `html.rs`. For `format: revealjs` the preview pseudo-format is `q2-slides`
  (`wasm-quarto-hub-client/src/lib.rs` `map_format_for_preview`): same JSON
  pipeline, React builds the reveal shell. **The figure-caption consumer
  (bd-38ioql41) is a reveal feature, so its preview is rendered by React, not
  by `html.rs`.**
  - Today `json.rs` **errors** (`Q-3-32`) on a stray `Inline::Attr` and emits an
    empty `Str` placeholder, so the node never reaches the JSON.
  - The React side has **no `Attr` inline** at all — an unregistered inline
    renders a "(not yet implemented)" placeholder
    (`ts-packages/preview-renderer/src/q2-preview/dispatchers.tsx`).
  - React block leaves read the Pandoc attr triple directly: `Header.tsx` reads
    `node.c[1] = [id, classes, kvs]`; `Div.tsx` reads `node.c[0]`. `Para.tsx`
    emits `<p>{children}</p>` with **no** attr; `Plain.tsx` a bare fragment;
    `BulletList.tsx`/`OrderedList.tsx` wrap each item's blocks in a bare `<li>`.

**Q2's JSON extension model:** node shapes stay Pandoc-valid; Quarto-only data
is either desugared to a valid Pandoc node (e.g. `Custom`→marker `Span`,
`Shortcode`→`Span`) **or** carried out-of-band in the `astContext` sidecar
(source info, attribution are already threaded this way and read by the React
renderer via `resolveSource`). A block-level attr on a paragraph is **not
expressible as a standard Pandoc node** (`Para` is `["Para",[inlines]]`), so
*any* approach that makes the preview render `<p class>` must use one of these
two extension channels and the React renderer must gain explicit handling.

## Design questions to resolve (the session's real work)

### 1. Where does collection happen? (the decisive decision)

Plan option (A) below ("writer-side scan") was the original lean, but the
two-renderer finding above reframes its cost: a writer-side scan means the
**merge rule is implemented twice — once in Rust (`html.rs`) and once in
TypeScript (React leaves)** — with `json.rs` emitting the raw node. That cross-
language duplication *is* the preview-parity divergence the project fears.

- **(A) Per-renderer scan.** `html.rs` scans its inlines, merges, applies,
  skips them; `json.rs` emits the `Inline::Attr` faithfully (desugared to a
  marker `Span` to stay Pandoc-valid); React `Para`/`Plain`/`li` scan children,
  merge, and apply. AST node types unchanged. **Merge logic duplicated
  Rust+TS.** Lowest type churn, highest divergence risk.
- **(B) Resolve once in Rust (recommended).** A shared late AST transform folds
  the trailing `Inline::Attr` run into a single resolved `Attr` and stores it on
  the block, so **both** writers read a resolved attr and neither re-merges.
  Merge semantics live in exactly one place (Rust). Two carrier sub-options:
  - **(B1) `astContext` sidecar keyed by the block's pool-id** (mirrors how
    attribution/source-info already reach React). Node tree stays valid Pandoc;
    `html.rs` reads the resolved attr; React looks up pool-id → applies, no TS
    merge logic. Most consistent with the existing extension model; most
    plumbing.
  - **(B2) Add an `attr: Attr` field to `Paragraph`/`Plain`** (our types — we
    already extend Pandoc heavily). Cleanest semantics: one transform fills it;
    `html.rs` and React read it like `Header`/`Div`. Cost: touches every
    `Paragraph`/`Plain` construction site (mechanical, via `Default`), and the
    `Para` *JSON node shape* would diverge from Pandoc unless we still sidecar
    it for `-t json` — i.e. a strict-vs-preview JSON emit split.
- **(C) Normalization into a different block (rejected).** Folding into a
  `Div`/`Span` changes the DOM (`<div class><p>…`), which is not the `<p class>`
  we want.

**Recommendation: (B).** Resolve in Rust so the merge rule is single-sourced;
lean **(B1 sidecar)** to keep JSON Pandoc-valid and reuse the React renderer's
existing `astContext` lookup, falling back to a `para.attr` field (B2) only if a
first-class attributed paragraph is judged worth the wider change. A hybrid is
also possible: a `para.attr` field internally (clean for `html.rs`) that
`json.rs` emits into the `astContext` sidecar (Pandoc-valid wire form).

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

- `Paragraph` first (unblocks the figure caption) — the attr rides *in* the
  paragraph's own inlines, so collection/application is local to the `<p>`.
- `Plain` next (same shape; the reveal caption may be a `Plain`, not `Para`).
- **List items** are a *different mechanism* and probably a later sub-task. A
  Pandoc list item is `[[Block]]` with no per-item attr; the injected
  `Inline::Attr` would live inside the item's first/last child block (a
  `Plain`/`Para`). Producing `<li class>` therefore means the list writer must
  **hoist** the resolved attr from the item's child block up to the `<li>`
  element — extra plumbing beyond the `<p>` case. (Note the existing
  `list_item_open` at `html.rs:1272` already special-cases `class="fragment"`
  for incremental reveal lists; an item attr must compose with that.) Keep the
  collection helper block-agnostic (returns `(content, Attr)`); the *hoist* is
  list-writer-specific.
- Consider `BlockQuote` and others later.

### 4. Source mapping & round-trip

- `InlineAttr` already carries `attr_source` + `source_info`. For filter-injected
  attrs the source is `Generated{by:…}`. The HTML change is output-only; the
  `qmd` writer already round-trips `Inline::Attr` (`qmd.rs:2393`). Verify the
  collected-and-applied path keeps source info coherent for the source map.

## Affected code (survey, verified 2026-06-17)

Rust:
- `crates/quarto-pandoc-types/src/inline.rs:332` — `InlineAttr { attr,
  attr_source, source_info }`; `inline.rs:41` the `Inline::Attr` variant.
  (B2 only) `Paragraph`/`Plain` in `block.rs` would gain an `attr` field.
- `crates/pampa/src/writers/html.rs` — `Inline::Attr` dropped at `:1058`; the
  attr emitter is `write_attr` (`:504`, used by `Header`/`Div`); `Paragraph`
  writer `:1286`, `Plain` `:1282`, `list_item_open` `:1272`.
- `crates/pampa/src/writers/json.rs:922` (and `:2846`) — currently **errors**
  `Q-3-32` on `Inline::Attr`. This is the preview transport; must change.
- `crates/pampa/src/writers/native.rs:384` — also errors `Q-3-32`; decide
  whether native faithfully prints the resolved attr or stays an error.
- `crates/pampa/src/writers/{qmd,incremental}.rs` — already **round-trip**
  `Inline::Attr` back to `{…}`; must stay correct (don't double-emit if the
  transform also strips/relocates the node).
- `crates/pampa/src/writers/{plaintext,ansi}.rs` — no-op on `Inline::Attr`; fine.
- `crates/pampa/src/pandoc/treesitter_utils/postprocess.rs:924-969` — the Header
  fold precedent to mirror (trailing-only pop + `trim_inlines`).
- A new shared collection helper / transform — location TBD by Decision 1
  (writer-local helper for (A); an `AstTransform` in `quarto-core` for (B)).

TypeScript (preview-renderer, `q2 preview` / `q2-slides`):
- `ts-packages/preview-renderer/src/q2-preview/blocks/{Para,Plain,BulletList,OrderedList}.tsx`
  — apply the resolved/collected attr to the `<p>`/`<li>` wrapper.
- `ts-packages/preview-renderer/src/framework/types.ts` — (A only) add an
  `Attr` inline type; (B1) the sidecar lookup reuses existing `astContext`.
- `ts-packages/preview-renderer/src/q2-preview/dispatchers.tsx` — (A only)
  register an `Attr` inline so it isn't a "(not yet implemented)" placeholder.

## Test plan (TDD)

Rust:
- `html.rs` unit tests (mirror `mod tests` there): `Paragraph` with a trailing
  `Inline::Attr{.caption}` → `<p class="caption">…</p>`; id + kvs applied;
  multiple attrs merge per rule; empty attr is a no-op; the `Space` before the
  attr is stripped (no trailing space in `<p>` text); `Plain` and a list item
  variant per scope.
- `json.rs` test: the same `Paragraph` serializes so the preview carries the
  resolved attr (node tree stays Pandoc-valid; attr in `astContext` for B1, or
  marker form for A) — guards the cross-renderer contract at the boundary.
- Round-trip: a parser-emitted `Inline::Attr` (heading/table path) still
  round-trips through `qmd`/`incremental` unaffected.
- **End-to-end** (per CLAUDE.md): drive a fixture through `render_document_to_file`
  for `q2 render` and assert `<p class="caption">` in the HTML; do not rely on
  `render_qmd_to_html` defaults.

TypeScript:
- preview-renderer test: AST with an attributed `Para`/`li` renders the wrapper
  with the class (vitest), so `q2 preview` matches `q2 render`.
- Real-render parity: render the reveal figure-caption fixture and confirm both
  `q2 render` HTML and the `q2 preview` (q2-slides) DOM show `<p class="caption">`.

## Relationship to other work

- **bd-38ioql41** (reveal figure auto-stretch) — Case 1's caption can then be a
  proper `<p class="caption">` (plan option (c)). That strand can ship first
  with a plain `<p>` and adopt this capability when ready; not a hard blocker,
  hence `discovered-from`/related rather than `blocks`.
- The `<li>` styling need (raised separately) is a second consumer; design the
  helper to serve both.
