# Syntax highlighting — Phase 3 (browser built-ins)

- **Parent plan**: `claude-notes/plans/2026-04-19-syntax-highlighting-design.md`
- **Beads**: bd-n7x2 (overall epic)
- **Status**: in progress, started 2026-04-20

## Goal

Make the 12 statically-linked grammar crates in `quarto-highlight` compile and run inside `wasm-quarto-hub-client`, and exercise the same annotation code path from the browser that native `quarto render` uses. Per the parent plan, Phase 3 has four acceptance items:

- [ ] 3A Statically linked grammar crates compile clean into `wasm-quarto-hub-client`
- [ ] 3B Bundle-size regression observation (record before/after; no numeric threshold)
- [ ] 3C Same highlight code path exercised from WASM (vitest + WASM export that goes through `Registry::global().highlight()`)
- [ ] 3D End-to-end browser verification (user-driven, fixture + instructions handed off)

Explicitly out of scope for Phase 3: user grammars on wasm32 (Phase 4), Playwright automation of 3D.

## Findings from pre-work audit

- **Only C-side blocker**: `towupper` is referenced in `tree-sitter-html` scanner.c (lines 106, 154). Not in the current `wasm-quarto-hub-client/src/c_shim.rs` or `wasm-sysroot/wctype.h`. All other scanner.c files use only `{iswspace, iswalnum, iswdigit, iswalpha, towlower}` which are already shimmed, or no wide-char functions at all.
- **Rust-side**: `quarto-highlight`'s Cargo.toml already gates the `tree-sitter` `wasm` feature (wasmtime) and the `user_grammar` module to native targets. `annotate.rs` already has wasm32-aware branches. `tree-sitter-highlight`'s `LazyLock` is fine on wasm32 per the research correction in `syntax-highlighting-wasm-compatibility.md`.
- **Pipeline plumbing that must change**:
  - `crates/quarto-core/Cargo.toml:40-49` — `quarto-highlight` is gated to native-only. Move to unconditional `[dependencies]`.
  - `crates/quarto-core/src/stage/stages/mod.rs:23-36` — `mod code_highlight` / `pub use CodeHighlightStage` are wasm-gated out. Remove the gate.
  - `crates/quarto-core/src/pipeline.rs:169-170` and `:244-245` — two `#[cfg(not(target_arch = "wasm32"))] stages.push(CodeHighlightStage…)` calls. Remove the gate.
- **Emit side**: `pampa`'s HTML writer already decodes `data-hl-spans` via the deps-light `quarto-highlight-encoding` crate. No changes needed on the emit side.

## Work phases

### Phase 3.1 — Make the crate compile for wasm32

- [x] Write failing test: run `npm run build:wasm` with `quarto-highlight` re-enabled. **Observed 2026-04-20 (exactly as predicted)**:

  ```
  src/scanner.c:106:31: error: call to undeclared function 'towupper';
    ISO C99 and later do not support implicit function declarations
  src/scanner.c:154:13: error: call to undeclared function 'towupper';
    ISO C99 and later do not support implicit function declarations
  ```

  This was the only C-side failure; no other grammar crate produced a compile error. The error originated in `tree-sitter-html-0.23.2`'s `src/scanner.c`. After this is fixed, Rust-side errors (if any) will become visible.

- [x] Un-gate quarto-highlight in `crates/quarto-core/Cargo.toml`, `stage/stages/mod.rs`, and `pipeline.rs` (done together with the observation step so the failing path existed).

- [x] Add `towupper` shim to `c_shim.rs` + declaration in `wasm-sysroot/wctype.h`. C-side compile succeeds after this.

- [x] **Unanticipated issue**: link stage fails with 8 duplicate-symbol errors on `snprintf`, `vsnprintf`, `fclose`, `fdopen`, `fputc`, `fputs`, `fwrite`, `fprintf`. Root cause: `tree-sitter-lua 0.5.0` and `tree-sitter-css 0.25.0` each compile `tree-sitter-language`'s upstream `wasm/src/stdio.c` on wasm32, which defines the same 8 symbols our `c_shim.rs` already defines (originally for the Lua runtime). See sub-plan **`claude-notes/plans/2026-04-20-wasm-shim-merge.md`** for the full analysis and fix (local patch of `tree-sitter-language` to empty `wasm/src/*.c` + merge our c_shim superset).

- [x] Completed the wasm-shim-merge sub-plan (`2026-04-20-wasm-shim-merge.md`). Summary:
  - Created `crates/tree-sitter-language-wasm-shim/` as a `[patch.crates-io]` drop-in that ships empty `wasm/src/*.c` files, so `tree-sitter-lua` / `tree-sitter-css` contribute no conflicting stdio symbols.
  - Extracted printf-family logic into a new `crates/wasm-printf-fmt/` crate so it can be unit-tested natively. 29 new tests pass — format specifiers `%d %i %u %ld %lld %zu %zd %x %X %p %s %c %% %g` plus flags / width / precision / truncation.
  - Refactored c_shim.rs to use the shared formatter via a `VaListSource` wrapper. Consolidated FFI unsafety to a single `slice::from_raw_parts_mut` boundary op per entry point; `finish` helper is a safe fn.
  - Changed `fputc`/`fputs`/`fwrite` from panic to no-op returning success-like values, matching upstream `tree-sitter-language` convention.
- [x] `cargo build --workspace` green.
- [x] `cargo nextest run --workspace` green (7626 tests pass, up from 7597 — +29 wasm-printf-fmt tests).
- [x] `cargo build --target wasm32-unknown-unknown --release` green for `wasm-quarto-hub-client`. Bundle size: **32.8 MB** uncompressed (~16 MB larger than the 17 MB pre-highlighting baseline from the research note, which matches the per-grammar estimate of ~1.5 MB × 12 grammars).
- [ ] Add `towupper` (c_int → c_int) implementation to `wasm-quarto-hub-client/src/c_shim.rs` using `char::to_uppercase` on the ASCII-or-passthrough case, matching the existing `towlower` shim shape.
- [ ] Add `wint_t towupper(wint_t wc);` declaration to `wasm-quarto-hub-client/wasm-sysroot/wctype.h`.
- [ ] Move `quarto-highlight` out of `target.'cfg(not(target_arch = "wasm32"))'.dependencies` and into unconditional `[dependencies]` in `crates/quarto-core/Cargo.toml`. Update the comment above the block to reflect the new reality.
- [ ] Remove `#[cfg(not(target_arch = "wasm32"))]` on `mod code_highlight` and `pub use CodeHighlightStage` in `crates/quarto-core/src/stage/stages/mod.rs`.
- [ ] Remove `#[cfg(not(target_arch = "wasm32"))]` on the two `stages.push(Box::new(CodeHighlightStage::new()))` calls in `crates/quarto-core/src/pipeline.rs`.
- [ ] `cargo build --workspace` passes (native sanity).
- [ ] `npm run build:wasm` passes.
- [ ] Record WASM bundle size before/after this phase in the plan.

### Phase 3.2 — WASM test harness exercising the real code path

- [ ] Factor the per-language fixture snippets used by `crates/quarto-highlight/tests/all_languages.rs` + `golden.rs` into a shared JSON fixture file at `crates/quarto-highlight/tests/fixtures/builtin-snippets.json` (or similar). Native tests read from this file; the wasm path re-uses it.
- [ ] Expose a test-only `#[wasm_bindgen] pub fn quarto_highlight_for_test(lang: &str, source: &str) -> Option<String>` from `wasm-quarto-hub-client` that calls `quarto_highlight::highlight(lang, source)` directly. Gate it behind a cargo feature `test-highlight` so production builds don't ship it — but the vitest config turns it on.
  - If feature-gating in `build-wasm.js` is too invasive, fall back to always-shipping the export named `__quarto_highlight_for_test` (underscore prefix = test-only convention) and note that in the plan.
- [ ] Add `hub-client/tests/wasm-highlight.vitest.ts` (or slot into existing `vitest.wasm.config.ts` target) that loads the WASM module, reads the shared fixture JSON, invokes `quarto_highlight_for_test` per language, and asserts `JSON.parse(result)` equals the parsed fixture expected output. At least one fixture per built-in grammar, matching the native golden set.
- [ ] `npm run test:wasm` passes.

### Phase 3.3 — End-to-end hub-client verification (user-driven)

- [x] Created `claude-notes/fixtures/phase3-highlight-check.qmd` with Python, R, JavaScript, Bash, JSON, YAML, CSS blocks plus one inline `` `print()`{.python} ``.

- [x] **User verification, round 1 (2026-04-20)**: highlighting worked, but only when `theme: flatly` (or any theme) was explicitly set in frontmatter. Without a theme, the `hl-*` span classes were emitted but had no colors — same regression pattern that bit the native CLI path at the start of Phase 2.

- [x] **Root-cause + fix** for the missing-theme case:
  - `crates/quarto-sass/src/compile.rs` has two variants of `compile_default_css` — a native `#[cfg(not(target_arch = "wasm32"))]` one (updated in commit 50745caa to load `highlight_layer`) and a wasm32 one (missed by that commit).
  - The wasm path was still assembling SCSS as `Bootstrap + Quarto + title-block` only, so the hub-client's no-theme default CSS had nothing for `.hl-*` selectors.
  - Fix: added `load_highlight_layer()` to the wasm32 `compile_default_css`, matching the native version's behavior. One-liner plus a doc comment explaining the parity with native.
  - Regression test added to `hub-client/src/services/themeCss.wasm.test.ts`: renders a theme-less document and asserts the combined CSS contains `.hl-keyword` and `.hl-function-builtin` selectors. Verified the test fails without the fix (stash → rebuild → run → fail) and passes with it.

- [ ] **User verification, round 2**: confirm the fixture renders with colored highlighting in a fresh hub-client session **without** adding a `theme:` entry.

### Phase 3.4 — Wrap-up

- [ ] Record bundle size delta (3B).
- [ ] Update parent plan (`2026-04-19-syntax-highlighting-design.md`) marking Phase 3's four checkboxes complete, with the phase-complete summary including:
  - The exact `npm run build:wasm` invocation used.
  - A snippet of the observed DOM from the user's verification.
  - Explicit confirmation that output was inspected (bar from the 2026-04-20 Phase 2 post-mortem).
- [ ] Stage and commit.

## Design decisions made during this phase

- `towupper` implementation strategy: keep it simple and correct for ASCII (which is what the tree-sitter-html scanner is actually matching — HTML tag delimiters and doctype keywords). For non-ASCII wint_t values, pass through unchanged. Matches the `towlower` shim's existing behavior.
- `test-highlight` cargo feature vs always-on export: default to the feature-gated approach. Production WASM bundles should not carry test-only exports. `build:wasm` gets a separate invocation (or an env var) for test builds. Revisit if the plumbing burden is too large.
- Fixture sharing: native tests use `include_str!` against the JSON; vitest reads it via `fs.readFileSync`. One source of truth.
