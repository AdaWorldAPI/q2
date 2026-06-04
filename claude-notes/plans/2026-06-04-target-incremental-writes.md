# Target Incremental Writes — Research Plan

**Date:** 2026-06-04
**Branch:** feature/provenance (research; no implementation yet)
**Status:** Research plan. Needs exploration before it can become a development plan.
**Follows:** `incremental-writer-unwind.md` (the revert of the Plan 7 approach)

## Vision

Instead of reconciling a user-modified *transformed* AST against the original (Plan 7's
approach), the new model treats a user edit as a change to the **original, pre-pipeline
AST**. The content provided by a user action is interpreted as raw QMD text. The Rust
side holds the original AST, applies the edit there, and re-renders through the full
pipeline to produce the new output.

Three properties follow:

1. **No provenance stamping on user edits.** There is no `Generated{by: user_edit}`
   concept. The user is authoring raw QMD; the result has fresh, accurate `source_info`
   from the re-parse. `stampUserEdits` and `USER_EDIT_SOURCE_INFO_ID` go away permanently.
2. **No soft-drop.** Shortcodes, filters, and other pipeline-generated content live in
   the original AST as invocation tokens. Editing a rendered shortcode paragraph edits the
   shortcode itself, not the resolved content.
3. **`preimage_in` is the edit-position bridge.** Given a rendered node, `preimage_in(file_id)`
   returns the byte range in the original QMD that produced it. The edit replaces those
   bytes with the user's raw QMD fragment, then re-renders.

## What is already in place

- **`preimage_in`** (`quarto-source-map/src/source_info.rs`) — maps a node's `source_info`
  to a `Range<usize>` in the original QMD. Accurate to byte level after Plan 7g's tiling
  fixes. This is the core primitive the new model needs.
- **`is_atomic_kind`** — identifies pipeline-generated node kinds; still useful to inform
  the UI about which nodes map to invocation tokens vs. literal text.
- **Wire format + provenance stamping** (Plans 4–6) — the rendered AST already carries
  `source_info` for every node, so `preimage_in` can answer "where in the source is this
  rendered node?" for any displayed node.
- **`setLocalAst` infrastructure** in the React framework — the callback mechanism for
  components to signal local edits still exists; the new model will repurpose or replace it.

## What needs to be designed and built

### Layer 1: WASM state model — holding the original AST

The pipeline currently consumes the parsed AST and produces a rendered output; the
pre-pipeline AST is not retained between calls.

- [ ] **Audit the WASM module's current state model.** What does `parseQmdContent` return
  and what does the module hold after a parse? Is there an existing mechanism (e.g. the
  Automerge document cache, the reconciler's `original_ast`) that already stores a
  pre-pipeline snapshot?
- [ ] **Decide retention strategy.** Options:
  - (A) Store the pre-pipeline AST in WASM module state alongside the rendered result.
    Client calls parse → WASM stores original; later setAst calls use the stored original.
  - (B) Return the pre-pipeline AST to the caller as part of the parse result; caller
    passes it back in on setAst calls.
  - (C) Re-derive it lazily from the original QMD string (re-parse without running the
    pipeline). Simplest but costs a re-parse on every edit.
  - Consider the Automerge integration: option B is most compatible with Automerge's
    document model (the client holds state explicitly).
- [ ] **Determine precisely what "original AST" means.** Which pipeline stage is the
  boundary?  Candidate: after tree-sitter parse + Pandoc AST construction, before
  shortcode resolution, engine execution, or any transforms. This is the earliest
  stable point whose nodes have accurate `source_info` into the source QMD.

### Layer 2: Edit application — from source_info to modified QMD

- [ ] **Prototype the core operation.** Given `(original_qmd: &str, source_info: SourceInfo, new_qmd_fragment: &str)`:
  1. Call `preimage_in(file_id)` on `source_info` to get `Range<usize>`.
  2. Replace `original_qmd[range]` with `new_qmd_fragment`.
  3. Re-parse the modified QMD from scratch.
  4. Re-render through the full pipeline.
  5. Return the new rendered AST.
- [ ] **Decide the WASM function signature.** Candidate: `apply_source_edit(source_info_json: &str, new_qmd_fragment: &str) -> String`. The function looks up the original QMD from module state, applies the edit, re-renders, and returns the new rendered AST JSON.
- [ ] **Handle nodes with no preimage.** `preimage_in` returns `None` for pure pipeline-generated nodes (sectionize wrappers, navigation elements, footnote backlinks). These are not editable. The UI must either hide the edit affordance for such nodes or surface a clear "read-only" signal. Decide whether `preimage_in` returning `None` is the right editability predicate or whether a separate `is_user_editable(node)` function is cleaner.
- [ ] **Handle multi-file documents.** `preimage_in(file_id)` is per-file. If a rendered node came from an included file, its preimage is in that file, not the main document. Cross-file edits require either (a) restricting editing to the current file only, or (b) surfacing the include source for editing. Scope decision needed.

### Layer 3: Re-render strategy — performance

- [ ] **Measure full re-render cost on realistic documents.** A 500-block document with shortcodes: how long does parse + full pipeline take? Compare against the Plan 7 incremental writer's O(n) reconcile. The answer determines whether full re-render is viable on every keystroke or whether we need debouncing.
- [ ] **Assess the replay engine's role.** The existing `ReplayEngine` (`bd-45yw`) replays recorded engine output (Jupyter/Knitr cells) without re-executing. If engine output is replayed rather than re-run, the re-render cost is dominated by parse + non-engine pipeline stages, which are much cheaper.
- [ ] **Decide the edit commit model.** Options:
  - Optimistic: apply the edit immediately in the React component's local state; commit to WASM after a debounce (e.g. 300ms idle). The local React state shows the user's raw QMD text in a simple pre/code view while the WASM processes.
  - Pessimistic: block the UI until WASM responds. Simpler but user-visible latency.
  - The right choice depends on the re-render latency measurement above.

### Layer 4: React component contract

- [ ] **Decide what a component passes in a `setAst` call.**
  - Old model: a modified Pandoc AST node (TypeScript object).
  - New model candidate A: a raw QMD string fragment. The component is responsible for constructing valid QMD for the block it is editing (e.g. a Para editor provides `"New paragraph text\n"`).
  - New model candidate B: a structured "edit operation" object: `{ sourceInfo: SourceInfo, newQmd: string }`. The framework resolves the source position; the component provides the replacement text.
  - Candidate B is cleaner because components don't need to know their own source position — the framework has it from the rendered node's `source_info`.
- [ ] **Decide what the framework exposes to components.** Does a component receive its node's `source_info` as a prop? Does it receive a boolean `isEditable` derived from `preimage_in` returning non-null? What does calling `setAst` do visually before the WASM responds?
- [ ] **Decide whether `setLocalAst` survives in any form.** The current `setLocalAst` prop lets a component provide a fully modified AST node for its subtree. In the new model, the component provides raw QMD text, not a node. `setLocalAst` may be replaced by a `setSourceEdit(newQmd: string)` call that travels through the framework to the WASM bridge, rather than staying local to the component tree.

### Layer 5: Automerge integration

- [ ] **Understand how edits flow through Automerge in the current architecture.** When a user types in a component today, how does that reach the Automerge document? Is there already a "file content" representation in Automerge, or is the document entirely AST-based?
- [ ] **Determine whether the original QMD string lives in Automerge.** If yes, the raw-QMD edit model fits naturally: the edit is a text change in the Automerge document (a CRDT text edit), and Automerge handles conflict resolution automatically. If no, we need to decide where the source-of-truth for the QMD text lives.
- [ ] **Consider collaborative editing implications.** Two users editing overlapping byte ranges in the original QMD simultaneously: how does Automerge's CRDT handle this? Is byte-range-based editing compatible with Automerge's text editing primitives?

### Layer 6: Editability semantics — shortcodes and atomicity

- [ ] **Decide the shortcode editing UX.** In the new model, `preimage_in` on a shortcode-resolved paragraph returns the range covering `{{< lipsum 3 >}}`. The replacement QMD is the user's new content. This is correct — the user is editing the source — but the UI needs to make this comprehensible. Does the component show the shortcode token in an editable field when clicked? Or does clicking open a structured editor for the shortcode arguments?
- [ ] **Decide whether `is_atomic_kind` still applies.** In Plan 7, atomic nodes were soft-dropped to prevent user edits from leaking pipeline content. In the new model there is no leak risk (the user is authoring raw QMD), so "atomic" nodes are still editable — just differently. The atomicity concept might inform UI affordances (e.g. "this block is a shortcode — edit the invocation") rather than write-back safety.

## Known constraints and non-questions

- `preimage_in` already exists and is accurate post-7g. No changes needed to it.
- The full pipeline must remain runnable from Rust in WASM context. This is already the case for `parseQmdContent` + render.
- The QMD write-format (round-trip fidelity) is out of scope for this model; we are replacing bytes in the *original QMD string*, not serializing an AST back to QMD. The qmd writer (`incremental.rs`, `qmd.rs`) is not involved.
- Multi-block structural edits (e.g. drag-and-drop reorder) are out of scope for the initial version. The model handles single-block content replacement.

## Deliverables this research must produce

Before this becomes a development plan, we need:

1. **State retention decision** (Layer 1): which option (A/B/C), with prototype or measurement
2. **`apply_source_edit` prototype**: end-to-end demo of the core operation in a test, including the WASM function signature
3. **Re-render latency numbers** (Layer 3): measured on a representative fixture with replay engine enabled
4. **React component contract draft** (Layer 4): written-out API proposal for `setAst` in the new model, including what the framework provides to components and what components pass back
5. **Automerge answer** (Layer 5): where the original QMD string lives and whether text CRDT edits apply directly
6. **Editability predicate** (Layer 2): a precise rule for which rendered nodes are user-editable and how that is surfaced in the UI

## References

- `preimage_in`: `quarto-source-map/src/source_info.rs`
- `is_atomic_kind`: `quarto-pandoc-types/src/atomic_custom_nodes.rs`
- Wire format and provenance: Plans 4–6 on `feature/provenance`
- Source-range tiling (accuracy of `preimage_in`): Plan 7g
- Replay engine: `crates/quarto-core/src/engine/capture_splice.rs`, `bd-45yw`
- Unwind plan (predecessor): `2026-06-04-incremental-writer-unwind.md`
