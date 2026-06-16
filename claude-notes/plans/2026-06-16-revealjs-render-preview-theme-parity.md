# Reveal render/preview theme parity

**Strand:** bd-y259zb57
**Found:** 2026-06-16, during e2e testing on `main` (post-#297).

## Problem

Two render/preview divergences, **one root cause**: `q2 render` uses the
*compiled Quarto reveal theme*; `q2 preview` uses *stock vendored `white.css`*.
They diverge in opposite directions:

| | `q2 render` | `q2 preview` |
|---|---|---|
| Theme | compiled Quarto theme | stock `white.css` + `quarto-reveal.css` |
| Text align | left ✓ | center ✗ |
| Headings | none ✓ | uppercase ✗ |
| Font | (was) Helvetica ✗ → now SSP via @import ✓ | SSP (white.css embeds it) ✓ |

## Bug 1 — render fonts — DONE (commit 5b46b50e)

The compiled theme named `Source Sans Pro` but loaded nothing → Helvetica.
**Fixed**: the Quarto reveal layer (`resources/scss/revealjs/quarto-revealjs.scss`
`scss:defaults`) now `@import`s Source Sans Pro from Google Fonts (foundry
import, no bundling — user decision, matches Q1's `_brand.yml` font model).
Sass hoists the CSS `@import` to the top. Test:
`default_theme_imports_source_sans_pro_from_google`. E2E: Chrome confirmed the
SSP 400 face loads + applies on render.

## Bug 2 — preview centered + uppercase (Level 2: full per-document parity) — TODO

The q2-slides preview **never applies the Quarto reveal layer**, for *any*
theme/brand. So preview shows reveal's stock defaults (centered, uppercase) and,
once Level 2 lands, must instead compile and apply the document's actual reveal
theme (default / named / custom `.scss` / `_brand.yml`).

### Corrected architecture map (the 2026-06-16 Explore agent got two key things wrong)

- **WRONG (agent):** "the reveal branch in `CompileThemeCssStage` fires for
  q2-slides." **Reality:** that branch is gated on
  `ctx.format.identifier == FormatIdentifier::Revealjs`
  (`compile_theme_css.rs:267`); `q2-slides` → `("html", preview)`
  (`format.rs:125`), so `identifier == Html`. **The preview never compiles the
  reveal theme at all.** (The agent confused the `is_revealjs_target()` helper —
  true for `q2-slides` — with the identifier check.)
- **WRONG (agent):** "reveal registers `css:theme:<fp>` like HTML." **Reality:**
  `register_reveal_assets` uses `css:revealjs:*` keys (`assemble.rs:153`); the
  WASM `extract_theme_fingerprint` looks for `css:theme:` (`wasm-…/lib.rs:1463`).
  They do not match.

### HTML theme-delivery model to mirror (verified parts)

1. `CompileThemeCssStage` compiles the HTML theme → `store_css` registers
   `css:theme:<fingerprint>` (`compile_theme_css.rs`).
2. WASM `render_page_for_preview` (`wasm-quarto-hub-client/src/lib.rs:1188`) →
   `extract_theme_fingerprint` (1461) → `RenderResponse.theme_fingerprint`.
3. SPA reads VFS bytes at `DEFAULT_CSS_ARTIFACT_PATH`
   (`/.quarto/project-artifacts/styles.css`), mints a blob URL, posts
   `UPDATE_THEME` to the iframe (`Q2PreviewIframe.tsx:281`).
4. Iframe applies it.

### WASM reveal compile — AVAILABLE

`compile_reveal_theme_css` is cfg-split: native `grass`
(`quarto-sass/src/compile.rs:373`), WASM dart-sass via the JS bridge (`:575`,
async). So the preview can compile the reveal theme on WASM.

### Work (each TDD; verify through real `q2 preview` + Chrome)

- [ ] **L2.1 (Rust).** Make the preview pipeline compile the document's reveal
      theme. The reveal branch must fire for the reveal-preview pseudo-format.
      Decide the detection signal (target_format `q2-slides` /
      `is_revealjs_target`, vs `ast.meta` `format: revealjs`) since
      `format.identifier` is `Html` in preview. Register the compiled theme so
      the WASM extractor + VFS flush find it (either reuse `css:theme:<fp>` +
      the styles.css artifact path, or extend the extractor for the reveal key).
- [ ] **L2.2 (WASM).** Ensure `render_page_for_preview` surfaces the reveal
      theme (fingerprint + bytes flushed to the VFS artifact path the SPA reads).
- [ ] **L2.3 (hub-client).** The preview reveal renderer must consume the theme
      (UPDATE_THEME / prop), inject it as a stylesheet, and **drop the static
      `white.css` import** (keep reset + reveal core + quarto-reveal). Bundle the
      Source Sans Pro `@import` comes along via the compiled theme automatically.
      Files: `ts-packages/preview-renderer/src/q2-preview/RevealDeck.tsx`,
      `Q2PreviewIframe.tsx`, q2-preview-spa entry. **hub-client change → two-commit
      changelog; WASM/SPA rebuild required.**
- [ ] **L2.4.** Tests + E2E parity check (`q2 render` vs `q2 preview` on a deck
      with default, a named theme, and `_brand.yml` — all should match).

### Notes / gotchas

- Per-document theme means the SPA can no longer statically import one theme;
  it must inject the per-document compiled CSS at runtime (blob/style), like the
  HTML path does.
- `q2 preview` embeds the SPA bundle; after hub-client changes, rebuild WASM +
  SPA + re-embed (`hub-client npm run build:wasm`, `cargo xtask
  build-q2-preview-spa`, `cargo build --bin q2`) — see CLAUDE.md "Verifying Rust
  changes in q2 preview".
- Existing related strands: bd-v053sk3s (preview revealjs Tier-1 parity GA
  gate), bd-qn8yi1su (golden render↔preview parity test).
