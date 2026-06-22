# hub-client revealjs ≠ q2 render/preview — assessment & plan

**Strand:** bd-vwp4y5ku
**Date:** 2026-06-22
**Status:** assessment — awaiting go-ahead to implement

## Symptom

Recent Q2 revealjs work (the reveal.js 6 theming/authoring epic, esp.
`650cbddc` "apply the document's compiled reveal theme in q2 preview") made
`format: revealjs` decks render with Quarto's opinionated defaults —
non-uppercase headings, left-aligned content, the document's compiled theme.
These show up correctly in **both** `q2 render` and `q2 preview`.

They do **not** show up in **hub-client** (and therefore not on quarto-hub.com).
A bare deck:

```
---
format: revealjs
---

## A slide
* A thing
```

renders in the hub-client live editor with **uppercase titles** and
**centered content** — reveal.js's stock `white.css` look.

### Live confirmation (2026-06-22, localhost:5173)

Inspected the live editor deck via Chrome DevTools on the running hub-client:

- `.revealjs-container` **is present** → the active renderer is the hand-rolled
  `RevealjsSlideAst`, *not* the shared preview iframe.
- `<h2>` "A slide" → computed `text-transform: uppercase`.
- `.reveal .slides` → computed `text-align: center`.
- 7 stylesheet rules in the DOM carry `text-transform: uppercase`.

Both symptoms trace directly to the stock `white.css` that the hand-rolled
component imports.

## Root cause

hub-client has **two** revealjs surfaces, and the editor picks the wrong one:

1. **Shared, render-accurate path** — `@quarto/preview-renderer`
   `PreviewRoot` (`ts-packages/preview-renderer/src/q2-preview/PreviewRoot.tsx`),
   reached through `Q2PreviewIframe` when the format is `q2-preview`. This is
   the **same code `q2 preview` embeds and runs**. It already treats
   `revealjs`/`q2-slides` as slides (`isSlides`, PreviewRoot.tsx:1396), runs the
   full WASM q2-preview pipeline (shortcodes, Lua, transforms, **`CompileThemeCssStage`**),
   and applies the document's compiled reveal theme via the existing
   `css:theme:<fp>` → styles.css transport. This is the path `650cbddc` fixed.

2. **Hand-rolled React deck** —
   `hub-client/src/components/render/RevealjsReactAstSlideRenderer.tsx`
   (`RevealjsSlideAst`). A **parallel TypeScript/React reimplementation** of the
   revealjs writer: it re-parses the Pandoc AST in JS (`parseSlides` /
   `renderBlock` from `ReactAstSlideRenderer.tsx`), renders into
   `@revealjs/react` `<Deck>`/`<Slide>`, **hardcodes** `white.css`
   (line 11) and the reveal config object (lines 125-164), hardcodes inline
   `<h1>`/author styling on title slides (lines 43-60), and **never receives a
   `themeFingerprint`** — so it cannot apply the per-document compiled theme.

The routing that sends bare revealjs to surface #2:

- `getQ2Format.ts` returns `'revealjs'` for `format: revealjs`.
- `PreviewRouter.tsx` → `ReactPreview` (because the format is non-null).
- `ReactRenderer.tsx:305-319`: `format === 'q2-preview'` → `Q2PreviewIframe`
  (surface #1); **everything else revealjs** → `RevealjsSlideAst` (surface #2).

So a plain `format: revealjs` deck in the editor *always* lands on the
hand-rolled deck and never touches the shared, theme-aware pipeline.

### Why this is the answer to "why don't they share code paths?"

`q2 render` and `q2 preview` both consume the **Rust writer / Rust pipeline**
as the single source of truth (preview via the WASM build of the same crates).
hub-client's *live editor*, however, predates that convergence: for revealjs it
runs an **independent JS reimplementation** of slide assembly and styling. The
shared path exists and is wired up (`Q2PreviewIframe`), but the editor only
routes the `q2-preview` pseudo-format to it — not `revealjs`. The divergence is
not a stale-WASM/build artifact problem; it is two genuinely different
renderers, and the editor chose the legacy one for this format.

## Evidence (file:line)

Native / shared (correct):
- `crates/quarto-core/src/revealjs/assemble.rs` — `reveal_config_json`
  (~188-289) builds the reveal init from merged metadata; `register_reveal_assets`
  emits theme links in cascade order.
- `crates/quarto-core/src/stage/stages/compile_theme_css.rs:275-343` — compiles
  the reveal theme, gated on `is_revealjs_target(...)` (true for both `revealjs`
  and the `q2-slides` preview pseudo-format, post-`650cbddc`).
- `crates/quarto-core/src/pipeline.rs` `build_q2_preview_pipeline_stages` —
  preview pipeline includes `CompileThemeCssStage` + `AstTransformsStage`.
- `ts-packages/preview-renderer/src/q2-preview/PreviewRoot.tsx:1396` — shared
  renderer already handles `revealjs`/`q2-slides` as slides.

hub-client (divergent):
- `hub-client/src/components/render/RevealjsReactAstSlideRenderer.tsx:11` —
  hardcoded `white.css`; `:125-164` hardcoded reveal config; `:43-60` hardcoded
  title styles; component signature has **no** `themeFingerprint`.
- `hub-client/src/components/render/ReactRenderer.tsx:305-319` — routes
  non-`q2-preview` revealjs to `RevealjsSlideAst` without `themeFingerprint`.
- `hub-client/src/components/render/getQ2Format.ts:14` &
  `PreviewRouter.tsx` — format classification that keeps `revealjs` as `revealjs`.

## Fix options

### Option A — Converge the editor onto the shared preview path (recommended)

Route `format: revealjs` in the hub-client editor through the same shared
`@quarto/preview-renderer` path that `q2 preview` uses (the `q2-preview` /
`q2-slides` flow → `Q2PreviewIframe` → `PreviewRoot` `isSlides`), and **retire
`RevealjsSlideAst`** (and the reveal-specific parts of `ReactAstSlideRenderer`).

- **Pro:** single source of truth. Every present and future Rust-side reveal
  change (theme, defaults, transforms, crossrefs, auto-stretch) appears in
  hub-client automatically. Eliminates the divergence class, not just this
  instance. Matches the project's stated "no parallel reimplementation" stance
  and the `q2 render`/`q2 preview` architecture.
- **Con / open questions:** must verify the iframe path covers the editor
  affordances the hand-rolled deck provides today:
  - controlled slide navigation (`currentSlide` / `onSlideChange`) synced with
    the editor and the slide thumbnail rail (`Editor.tsx`, `enabled:
    currentFormat === 'q2-slides'`),
  - slide thumbnail generation,
  - click-to-edit / nested editing (the iframe path *has* `usePreviewEdit`),
  - presence / attribution overlays (the iframe path *has* `useAttribution`).
  Editing + attribution already exist on the shared path; **slide navigation and
  thumbnails are the items to confirm** before committing to A.

### Option B — Patch the hand-rolled deck (tactical stopgap)

Thread `themeFingerprint` into `RevealjsSlideAst`, drop the static `white.css`
import, inject the compiled theme CSS via the same `css:theme:<fp>` → styles.css
transport `Q2PreviewIframe` uses, and drive the reveal config from document
metadata instead of the hardcoded object.

- **Pro:** small, localized; fixes the visible symptom quickly.
- **Con:** perpetuates the parallel renderer — a **divergence treadmill** where
  every future Rust reveal change must be hand-mirrored in TS (this incident is
  exactly that treadmill failing). Does not address the architectural root cause
  and conflicts with the single-source-of-truth principle.

**Recommendation:** pursue **A**. Use **B** only as a short-lived stopgap if A's
affordance gaps (slide nav / thumbnails on the iframe path) turn out to be large
and we need the visual fix shipped sooner.

## Open questions for the user

1. **A vs B.** Converge on the shared iframe path (A, recommended) or patch the
   hand-rolled deck (B)? If A, are we OK potentially deferring/reworking slide
   thumbnails and in-editor slide navigation to match the iframe path?
2. Is `RevealjsSlideAst` (and the reveal branch of `ReactAstSlideRenderer`)
   load-bearing for anything beyond what `Q2PreviewIframe` already does? (I will
   audit, but a steer helps.)

## Plan (TDD — to be refined after the A/B decision)

### Phase 0 — Decision & affordance audit
- [ ] Confirm A vs B with the user.
- [ ] (If A) Audit `Q2PreviewIframe`/`PreviewRoot` for slide navigation,
      thumbnails, click-to-edit, presence — list any gaps vs `RevealjsSlideAst`.

### Phase 1 — Tests first
- [ ] Extend `hub-client/src/components/render/parity.integration.test.tsx` (or a
      new test) to assert the editor's revealjs output uses the **compiled theme**
      (non-uppercase `<h2>`, left-aligned slides) — i.e. a failing test that
      reproduces the current uppercase/centered symptom.
- [ ] (If A) Test that `format: revealjs` routes to the shared preview surface,
      not `RevealjsSlideAst`.

### Phase 2 — Implement (A)
- [ ] Route `revealjs`/`q2-slides` through `Q2PreviewIframe` in
      `ReactRenderer.tsx`; classify accordingly in `getQ2Format`/`PreviewRouter`.
- [ ] Port any missing affordances (slide nav, thumbnails) onto the shared path.
- [ ] Remove `RevealjsSlideAst` + reveal branch of `ReactAstSlideRenderer` +
      the `white.css` import once nothing references them.

### Phase 2' — Implement (B, only if chosen)
- [ ] Thread `themeFingerprint` from `ReactRenderer` into `RevealjsSlideAst`.
- [ ] Remove the static `white.css` import; inject compiled theme via
      `css:theme:<fp>` → styles.css.
- [ ] Drive reveal config from metadata instead of the hardcoded object.

### Phase 3 — Verify end-to-end
- [ ] `npm run build:all` (hub-client) + `npm run test:ci`.
- [ ] Browser check on the running hub-client: heading `text-transform: none`,
      slides left-aligned, compiled theme applied — record computed styles, same
      method as the live confirmation above.
- [ ] Cross-check against `q2 preview` of the same deck for visual parity.
- [ ] `cargo xtask verify` if any Rust/WASM leg is touched (Option A may not
      touch Rust at all; confirm).

## References
- `650cbddc` fix(revealjs): apply the document's compiled reveal theme in q2
  preview (Level 2) — the sibling fix for `q2 preview`; this strand is the
  hub-client editor counterpart.
- `claude-notes/plans/2026-06-16-revealjs-render-preview-theme-parity.md`
- `claude-notes/designs/transform-pipeline-phases.md`
- `.claude/skills` → `preview-render-parity`
