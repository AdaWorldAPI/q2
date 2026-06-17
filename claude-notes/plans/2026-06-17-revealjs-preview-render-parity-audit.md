# revealjs `q2 preview` ↔ `q2 render` parity audit

**Created:** 2026-06-17
**Epic:** bd-67yja58s (format: revealjs) · GA gate: bd-v053sk3s (Phase 1P)
**Related strands:** bd-vv8jft5n (section-id + config parity), bd-qn8yi1su
(automated golden parity test), bd-ocxnva1f (user Lua filters footer/logo)
**Audit strand:** bd-zaa4m4bd
**Footer/logo strand:** bd-n2w0sxgd (F3)

## Overview

`q2 preview`'s React `RevealDeck` (`ts-packages/preview-renderer/src/q2-preview/
RevealDeck.tsx`) is supposed to produce the same DOM as `q2 render`'s native
reveal scaffold (`crates/quarto-core/src/revealjs/assemble.rs`) for the same
deck. This audit compares the two pipelines across `examples/presentations/*`
and records every divergence so we can close them before the revealjs epic
lands (the GA gate is bd-v053sk3s, "q2 preview shows an in-parity Tier-1
slideshow").

The investigation that seeded this plan compared `10-footer-logo` and
`02-sections` empirically in Chrome (render served via `python -m http.server`
over `slides.html`; preview via `q2 preview … --no-browser`), and cataloged the
render-side `<section>` signatures for all 11 examples. Two divergence classes
are **systemic** (every deck); one is **deck-chrome-specific** (only
`10-footer-logo` today, but applies to any deck with `footer:`/`logo:`).

## Findings

### F1 — Section `id` dropped in preview (SYSTEMIC, partly tracked: bd-vv8jft5n)

Render emits an `id` on every `<section>` (derived from heading text, or absent
for `---`-only slides). Preview emits bare `<section>` with no `id`.

| | render | preview |
|---|---|---|
| title slide | `<section id="title-slide" …>` | `<section …>` (no id) |
| `## First topic` | `<section id="first-topic" …>` | `<section …>` (no id) |
| `# Part One` stack | `<section id="part-one" class="section stack">` | `<section class="stack">` |

**Impact:** breaks in-deck `#/id` deep links, cross-document anchors
(`other.html#first-topic`), and any cross-reference whose target is a slide.
bd-vv8jft5n already names this ("pass section ids onto `<Slide>`") but frames it
as "minor" because hash-nav is off in preview — cross-doc anchors make it not
minor.

### F2 — Section semantic classes dropped in preview (SYSTEMIC, tracked: bd-vv8jft5n)

Render puts `section` on every `<section>`, and `section title-slide center` on
the title slide. Preview emits only the classes reveal.js itself adds at runtime
(`present` / `future` / `stack`). The Quarto-authored classes are gone.

| logical slide | render `class` | preview `class` |
|---|---|---|
| title slide | `section title-slide center present` | `present` |
| content slide | `section future` | `future` |
| section divider stack | `section stack present` | `stack present` |

**Impact:**
- `center` drives reveal's vertical centering of the **title slide** — its
  absence visibly shifts the title-slide layout.
- `title-slide` is a theme-CSS hook (Quarto reveal theme keys off it).
- `section` is the generic slide-content hook.
- Any author-supplied per-slide class (`## Slide {.smaller}`,
  `{background-color="…"}`, etc.) rides the same `Attr` and is **also** dropped
  — so F2 is broader than the three built-in classes; it is "the section `Attr`
  is never forwarded onto `<Slide>`/`<Stack>` at all."

F1 and F2 share one root cause and should be fixed together (likely folded into
bd-vv8jft5n with its scope widened from "section-id" to "section `Attr`").

### F3 — Deck-level footer / logo absent in preview (`10-footer-logo`, tracked: bd-n2w0sxgd)

Render (`assemble.rs::render_revealjs_document`) places, as **direct children of
`.reveal` outside `.slides`**:

```html
<div class="reveal has-logo">
  <div class="slides"> … </div>
  <img class="slide-logo" src="logo.svg">
  <div class="footer footer-default">© 2026 … <a href="…">example.com</a></div>
</div>
```

Preview emits none of these: no `.footer`, no `.slide-logo`, and `.reveal` lacks
the `has-logo` class.

**The data is already available to preview.** The reveal footer/logo render
stage (`RevealFooterLogoTransform`, name `reveal-footer-logo`, selected by
`pipeline.rs::footer_render_stage(is_revealjs=true)`) runs in the q2-preview
pipeline — it is **not** in `Q2_PREVIEW_TRANSFORM_EXCLUDED` — so the preview's
`ast.meta` carries `rendered.reveal.footer` and `rendered.reveal.logo`.
`RevealDeck` simply never reads those slots, and the `<Deck>` wrapper never adds
`has-logo` to `.reveal`. The fix is purely in `RevealDeck.tsx`: read the two
meta slots and inject the markup outside `<Deck>`'s slides + add `has-logo`.

**Impact:** decks with `footer:`/`logo:` show no footer and no logo in preview —
a complete feature miss, not a style nudge. Tracked by **bd-n2w0sxgd**;
`bd-ocxnva1f` covers *Lua filters* manipulating footer/logo, a different
concern from this baseline "preview doesn't render them at all" gap.

**Status (2026-06-17): DOM fixed.** `RevealDeck` now emits the chrome.
`RevealChrome` reads `rendered.reveal.{footer,logo}` and injects the markup into
the `.reveal` element (outside `.slides`) via `useReveal()` →
`getRevealElement()`, plus `has-logo`. Verified in Chrome on `10-footer-logo`:
`.reveal.has-logo`, `img.slide-logo` + `.footer.footer-default` are direct
children of `.reveal`, not in `.slides`, footer link preserved. Tests:
`RevealDeck.integration.test.tsx` (7 cases). **Caveat → F4.**

### F4 — Preview reveal theme omits the Quarto chrome CSS layer (discovered via F3, tracked: bd-d0f9xoa6)

With F3's DOM in place, the footer/logo render **unstyled** (`position: static`)
because the preview's runtime-delivered theme (`<link data-q2-theme>` blob, 1784
rules) does **not** contain `.reveal .footer` / `.reveal .slide-logo`. Render
positions them `fixed` via rules in the per-doc `theme-<fp>.css` (compiled from
`resources/scss/revealjs/quarto-revealjs.scss`); the preview theme is missing
that chrome layer. This is a theme-**content** parity gap (sibling of the
theme-delivery work bd-y259zb57), independent of the DOM fix — fixing it likely
means either compiling the same SCSS partials into the preview theme or moving
the theme-independent chrome rules into the statically-imported
`resources/revealjs/quarto-reveal.css`.

## Root cause (F1 + F2)

`RevealDeck.tsx` maps each section `Div` onto `@revealjs/react`'s
`<Slide>` / `<Stack>` but passes only the Div's **children** (`div.c[1]`). The
Div's `Attr` (`div.c[0]` = `[id, classes, kvs]`) is read **only** to flip the
incremental scope (`SlideBody` inspects `sectionClasses` for
`incremental`/`nonincremental`); it is never emitted onto the `<section>`.
`@revealjs/react`'s `<Slide>`/`<Stack>` accept `id`/`className` props (they pass
through to the rendered `<section>`), so the fix is to forward them. The native
contract is `assemble.rs` (and the `RevealSlidesTransform` that produced the
`Div(.section)` tree shared by both paths).

## Render-side catalog (all 11 examples)

Every example renders `id` + `section` class on every `<section>`; title slides
get `section title-slide center`. Only `10` carries deck chrome. Feature markup
present per example (candidates for feature-specific preview divergence still to
verify):

| example | special markup to verify in preview |
|---|---|
| 01-creating-slides | title slide (`center`) |
| 02-sections | vertical `stack`s ✅ compared |
| 03-fragments | `.fragment` spans + `fragment-index` |
| 04-incremental-lists | incremental lists → `.fragment` |
| 05-columns | `.columns`/`.column` flex layout |
| 06-speaker-notes | `.notes` (does preview load the Notes plugin?) |
| 07-asides | `.aside` |
| 08-footnotes | per-slide footnote coalescing + `.aside` |
| 09-themes | `theme:` selection (theme parity in_progress: bd-y259zb57) |
| 10-footer-logo | footer + logo + `has-logo` ✅ compared (F3) |
| 11-auto-stretch | `.r-stretch` auto-stretch |

## Work items

### Phase A — fix the two systemic + one chrome divergence (TDD per /preview-parity)

- [ ] **F1+F2: forward section `Attr` onto `<Slide>`/`<Stack>`** (extend
  bd-vv8jft5n; widen scope from "section-id" to "section `Attr`: id + classes +
  kvs"). Failing test first in
  `ts-packages/preview-renderer/src/q2-preview/q2-preview.integration.test.tsx`:
  mount a deck AST, assert `<section id="…" class="section …">`. Mirror
  `assemble.rs`. Verify in Chrome on `01`/`02`/`10`.
- [x] **F3: render deck footer/logo + `has-logo` in `RevealDeck`** (bd-n2w0sxgd).
  DONE 2026-06-17 — `RevealChrome` + `revealChromeFromMeta` in `RevealDeck.tsx`,
  7-case `RevealDeck.integration.test.tsx`, verified in Chrome. DOM-only;
  styling blocked on F4.
- [ ] **F4: include the Quarto reveal chrome CSS layer in the preview theme**
  (bd-d0f9xoa6). Footer/logo emit but render `position: static` — the preview
  theme lacks `.reveal .footer` / `.slide-logo`. Decide under theme-parity
  (bd-y259zb57): compile `quarto-revealjs.scss` into the preview theme, or move
  the theme-independent chrome rules into the static `quarto-reveal.css`.

### Phase B — full per-example audit (the "other examples" sweep)

For each example NOT yet compared (01, 03, 04, 05, 06, 07, 08, 09, 11): render →
serve → `q2 preview` → Chrome DOM diff (the workflow in this plan's Overview).
Record any new divergence class as a finding here and file a focused strand.
Known suspects: `06-speaker-notes` (Notes plugin), `05-columns` (flex styles),
`09-themes` (theme delivery — already in_progress under bd-y259zb57).

### Phase C — lock it down

- [ ] Land **bd-qn8yi1su**: automated render↔preview golden parity test (the
  slides analogue of `/preview-render-parity`) covering the Tier-1 set, so Phase
  2 authoring features extend it per feature instead of re-auditing by hand.

## Verification recipe (used for this audit)

```bash
cargo build --bin q2 --release
./target/release/q2 render examples/presentations/<ex>/slides.qmd
(cd examples/presentations/<ex> && python3 -m http.server 8765 &)   # render
./target/release/q2 preview examples/presentations/<ex>/slides.qmd --port 8766 --no-browser &
# Chrome: compare http://127.0.0.1:8765/slides.html  vs  http://127.0.0.1:8766/?page=slides.qmd
# preview DOM is inside an <iframe> — query iframe.contentDocument.
```

## References

- Native reveal scaffold: `crates/quarto-core/src/revealjs/assemble.rs`
  (`render_revealjs_document`, `footer_logo_html`, `has-logo`).
- Footer/logo render stage: `crates/quarto-core/src/revealjs/footer_logo.rs`
  (`RevealFooterLogoTransform` → `rendered.reveal.{footer,logo}`).
- Preview deck: `ts-packages/preview-renderer/src/q2-preview/RevealDeck.tsx`.
- Pipeline seam: `crates/quarto-core/src/pipeline.rs`
  (`footer_render_stage`, `Q2_PREVIEW_TRANSFORM_EXCLUDED`,
  `build_q2_preview_pipeline_stages`).
- Epic plan: `claude-notes/plans/2026-06-08-revealjs-presentations.md` (Phase 1P).
- Skill: `/preview-render-parity`.
