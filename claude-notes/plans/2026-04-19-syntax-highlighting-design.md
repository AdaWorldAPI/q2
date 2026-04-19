# Syntax highlighting design for Quarto 2

- **Beads**: bd-n7x2
- **Status**: Design decisions locked 2026-04-19 — ready for implementation phases
- **Research notes**:
  - `claude-notes/research/syntax-highlighting-pandoc.md` — Pandoc + skylighting
  - `claude-notes/research/syntax-highlighting-hugo-chroma.md` — Hugo + chroma (Pygments heritage)
  - `claude-notes/research/syntax-highlighting-tree-sitter.md` — tree-sitter-highlight API, capture conventions
  - `claude-notes/research/syntax-highlighting-ecosystem-survey.md` — Shiki, Prism, highlight.js, GitHub
  - `claude-notes/research/syntax-highlighting-wasm-compatibility.md` — wasm32-unknown-unknown audit (no blockers)

## Goals

1. A **generic mechanism** that annotates contiguous textual ranges ("spans") inside a `CodeBlock` or `Code` inline with semantic names (e.g. `keyword`, `string.escape`, `function.builtin`). The encoding must be:
   - **On the AST node itself** — in the node's attributes or metadata — so it survives filter pipelines and is addressable from user filters.
   - **Filter-authorable** — a user filter producing the same encoding must work identically to the built-in stage.
   - **Pandoc-AST friendly** — we don't invent new AST variants; we ride on the existing `Attr = (id, classes, kvs)` slot.
2. A **built-in pipeline stage** that uses **tree-sitter grammars + `highlights.scm` queries** to produce the annotations, driven by the code block's infostring/class.
3. A **format-specific writer stage** that consumes the annotations and emits the right markup:
   - **HTML**: nested `<span class="…">` with classes driven by SCSS theming.
   - **Pandoc-bridged formats** (latex, typst, docx, …): v1 no-op; Pandoc runs its own skylighting. (See decision 5.)

This separates *what* is highlighted from *how* it is rendered, mirroring skylighting's "token stream is the stable interchange, formatters are format-specific" pattern but with tree-sitter producing the token stream instead of Kate XML lexers.

## Key findings from research (one-liners)

- **Pandoc/skylighting** never modifies the AST: `CodeBlock` stays a `CodeBlock`, and each writer calls skylighting at emit time to produce literal markup (`Html`, `Text`, etc.). Token taxonomy: 25 fixed types derived from KDE Kate (`KeywordTok`, `DataTypeTok`, …). HTML classes: short 2-letter (`kw`, `dt`, `co`). ([pandoc research](../research/syntax-highlighting-pandoc.md))
- **Hugo/chroma** has a richer Pygments-style hierarchical taxonomy (`KeywordDeclaration`, `NameFunction`, `LiteralStringDouble`) with numeric parent/category arithmetic. HTML: short classes (`k`, `nf`, `s1`). Themes are XML. Line-number/highlight are **formatter options**, not token-level. ([chroma research](../research/syntax-highlighting-hugo-chroma.md))
- **tree-sitter-highlight** is the natural Rust API: ~45 canonical capture names (`keyword`, `function.builtin`, `string.escape`, `markup.*`, etc.), longest-match resolution, nested event stream (`HighlightStart` / `HighlightEnd` / `Source`), language injection, locals tracking. Used in production at github.com. ([tree-sitter research](../research/syntax-highlighting-tree-sitter.md))
- **Ecosystem**: Three families — Pygments-derived (skylighting, chroma, rouge), TextMate-grammar (Shiki), tree-sitter. GitHub migrated from Pygments to tree-sitter. Tree-sitter is strictly more accurate (multi-line, context-aware, injections). ([survey](../research/syntax-highlighting-ecosystem-survey.md))

## Design sketch

### 1. Span-annotation encoding (on the AST)

A `CodeBlock` or inline `Code` whose content is highlighted carries two things in its `Attr`:

1. The usual language class (`python`, `rust`, …) — no change.
2. A single key-value pair `data-hl-spans` whose **value is a JSON array of highlight triples** over the node's `text`.

Encoding: **JSON array of `[start_byte, end_byte, capture_name]` triples** (triples may grow a 4th `{}` object later for extensions like scope tracking or confidence, without breaking existing consumers). Example for `def foo():`

```json
[[0,3,"keyword"],[4,7,"function"]]
```

A line with a range that encloses a shorter one is understood to nest around it (tree-sitter's native event shape is preserved; the writer decides whether to emit nested spans or flatten). Lua filters produce/consume this with `pandoc.json.encode` / `pandoc.json.decode` which are already registered in our Lua runtime (`crates/pampa/src/lua/json.rs`).

**Why this shape:**
- **Attr KV** is filter-visible and roundtrips through Pandoc AST JSON.
- **Byte offsets** (not char offsets) match tree-sitter and Rust `str` slicing; stable across UTF-8.
- **Nested, not flat**: preserves tree-sitter's semantics; the writer decides.
- **JSON triples**: `pandoc.json` makes it trivial from Lua; triples keep size tight; 4th-slot object leaves room for forward-extensions without version bumps.
- **Capture-name constraint**: tree-sitter's query parser restricts capture-name characters to `[a-zA-Z0-9_\-.]+` (verified in `external-sources/tree-sitter/lib/src/query.c:399-411`), so no embedded spaces/quotes/special chars to escape.

### 2. Built-in pipeline stage: `annotate_code_highlights`

Runs in the Quarto 2 pipeline after any filter that might **rewrite** code (so we highlight the final text) but before any filter that might **consume** the highlights.

Responsibilities:
1. Walk the AST; for each `CodeBlock` **and inline `Code`**:
   - If `data-hl-spans` already present → skip (user/filter already annotated).
   - Resolve the node's first class against a **language registry** → tree-sitter `Language` + `HighlightConfiguration`.
   - If unknown language → skip (writer falls back to plain `<pre><code>` / `<code>`).
2. Run `Highlighter::highlight()` on `node.text` with the config.
3. Collect `HighlightEvent`s; serialize into `data-hl-spans`; attach to `node.attr`.

**Inline `Code` note**: Pandoc's inline `Code` carries the same `Attr` tuple as `CodeBlock`, so the encoding and pipeline are identical. Users write `` `foo()`{.python} `` to opt an inline code span into highlighting. No semantic surprises; the writer emits `<code class="sourceCode language-python">…spans…</code>`.

Infrastructure:
- **Language registry** (static `OnceLock<HashMap<&str, LanguageEntry>>`): maps class → grammar + queries. Aliases resolved at lookup time.
- **Initial built-in language set** (14 classes, 12 grammar crates):
  - `r` (tree-sitter-r), `python`/`py` (tree-sitter-python), `javascript`/`js` + `jsx` (tree-sitter-javascript — one grammar, JSX parseable by default), `typescript`/`ts` (tree-sitter-typescript language_typescript), `tsx` (tree-sitter-typescript language_tsx), `bash`/`sh` (tree-sitter-bash), `sql` (tree-sitter-sql), `html` (tree-sitter-html), `css` (tree-sitter-css), `json` (tree-sitter-json), `yaml` (tree-sitter-yaml), `julia` (tree-sitter-julia), `lua` (tree-sitter-lua).
- **`highlights.scm` sourcing** (revised 2026-04-19 during implementation): prefer each grammar crate's own **exposed `HIGHLIGHTS_QUERY` constant** where available — this guarantees the query matches the parser bundled in the same crate version, eliminating drift. Fall back to vendoring `resources/highlights/<lang>/highlights.scm` only when:
  1. The grammar crate doesn't expose a query constant (so far: `tree-sitter-julia` 0.23.1).
  2. A grammar's own query is low-quality and we want to substitute a curated version (e.g. [Helix](https://github.com/helix-editor/helix/tree/master/runtime/queries) MPL-2.0, [Zed](https://github.com/zed-industries/zed) MIT, [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) Apache-2.0).
  Composite queries (JS+JSX, TS inherits JS, TSX inherits TS+JSX+JS) are built at runtime from multiple crate constants via `format!()` concat — also drift-free as long as the crate versions are pinned.
  Provenance: each `langs/<lang>.rs` documents the crate version + source in a comment; vendored files carry full provenance headers.
- **Custom user queries for built-ins**: Phase 5 — a `syntax-highlighting.highlights-path` config key pointing to override files.
- **Language injections**: Out of scope for v1. The architecture supports them natively via tree-sitter; we add them when a user file demands it.
- **Locals query**: Include when the bundled `locals.scm` is small and low-risk per language. Optional per grammar.
- **Caching**: `HighlightConfiguration` is built once and reused; `Highlighter` is thread-local.

### 3. Format-specific writers

#### HTML writer (v1)

Replace the current `<pre><code>{escaped_text}</code></pre>` path in `crates/pampa/src/writers/html.rs:819-826` with a span-aware emitter. Mirror for inline `Code` with `<code>…</code>` instead of `<pre><code>`.

- Parse `data-hl-spans`, walk the event stream, emit nested `<span class="hl-{capture}">` wrapping escaped text chunks.
- Class naming: take the tree-sitter capture, replace `.` with `-`, prefix with `hl-`.
  - `keyword` → `hl-keyword`
  - `function.builtin` → `hl-function-builtin`
  - `string.escape` → `hl-string-escape`
- Keep the outer `<pre class="sourceCode language-python">` (or `<code class="sourceCode language-python">` for inline) for theme compatibility at the container level. Pandoc themes key off `.sourceCode`.
- **Clean break from Pandoc's short classes.** We do **not** emit `.kw`/`.co`/`.fu` alongside `hl-…`. If users want drop-in compat with existing Pandoc themes, that's done via a future Lua filter that rewrites `data-hl-spans` into a Pandoc-compat encoding before the HTML writer runs — leaving the default clean.
- SCSS theming: emit a default `resources/scss/_highlight.scss` with rules for the standard capture set, pullable into user themes via `@use`. Users override colors without touching markup.

#### Pandoc-bridged writers (typst / latex / docx)

**Resolved 2026-04-19: v1 no-op.** Quarto 2 doesn't drive these formats via Pandoc in the current CLI. When we add those output paths, Pandoc runs its own skylighting pipeline over the bare `CodeBlock`; our `data-hl-spans` attribute is passed through but ignored by Pandoc's writers. If/when parity matters, we revisit with the "translate spans into per-format RawBlocks" approach.

## Execution phases

- **Phase 0 — alignment**: ✅ complete (see "Resolved decisions" section below).

- **Phase 1 — annotate stage (native)**:
  - [x] New `quarto-highlight` crate with `Language` registry + `HighlightConfiguration` cache
  - [x] 12 built-in grammar crates wired in (14 user-facing classes: `bash`/`sh`, `css`, `html`, `javascript`/`js`/`jsx`, `json`, `julia`/`jl`, `lua`, `python`/`py`, `r`, `sql`, `tsx`, `typescript`/`ts`, `yaml`/`yml`). Each grammar validated by smoke test: query parses against its bundled parser and emits a representative capture.
  - [x] Queries sourced from crate-exposed `HIGHLIGHTS_QUERY` constants (drift-free). Only Julia is vendored under `resources/highlights/julia/` because the `tree-sitter-julia` crate doesn't expose a query constant. JS/TS/TSX compose multiple constants at runtime.
  - [x] Native user-grammar loader via `tree_sitter::WasmStore` (wasmtime always included on native). Scans a parent directory for grammar sub-dirs; each sub-dir has a `<name>.wasm` + `highlights.scm`. API: `UserGrammars::new()`, `load_from_directory()`, `load_all_from_parent()`. Public function `highlight_with_user()` composes with user grammars overriding built-ins on class collision. End-to-end test uses tree-sitter-toml v0.7.0 as a fixture (not in built-in set).
  - [x] Pipeline stage: `CodeHighlightStage` in `quarto-core`. Runs between `UserFiltersStage::post()` and `RenderHtmlBodyStage`. Annotation walker lives in `quarto_highlight::annotate_pandoc` (CodeBlock + inline Code + recursion through container blocks/inlines). Filter-authored annotations win (skip if `data-hl-spans` is already set). Highlighting failures become warning diagnostics instead of aborting the pipeline.
  - [x] **WASM/hub-client scope note**: `quarto-highlight` is gated as a native-only dep in `quarto-core` because grammar-crate scanners (e.g. `tree-sitter-html`'s `towupper`) don't compile against the hub-client's wasm32 sysroot. On wasm32 the `CodeHighlightStage` is absent from the pipeline and the HTML writer emits plain `<pre><code>`. This is consistent with Phase 3's scope: browser built-ins ship later, after wasm32 scanner issues are addressed.
  - [x] Golden `insta` snapshots for all 13 user-facing class names + one user-grammar fixture (tree-sitter-toml). `tests/golden.rs` + `tests/snapshots/`. Locks exact capture names + byte ranges per tiny fixture snippet — upstream grammar-crate bumps surface as reviewable diffs.

**Phase 1 complete.** 22 tests in `quarto-highlight` across encoding, python-basic, all-languages smoke, annotate walker, user-grammar-toml, and golden snapshots. Workspace build/tests/lint all green.

- **Phase 2 — HTML writer**:
  - [ ] `data-hl-spans` parser using `pandoc.json`-compatible parse (just `serde_json` on the Rust side)
  - [ ] Nested span emission: `<span class="hl-function-builtin">…</span>`
  - [ ] Container still `<pre class="sourceCode language-X">` / `<code class="sourceCode language-X">` for theme hooks
  - [ ] Default `resources/scss/_highlight.scss` covering the standard capture set
  - [ ] SCSS pipeline integration (theme overrides color/style, never markup)
  - [ ] Snapshot tests across the language set

- **Phase 3 — browser built-ins**:
  - [ ] Statically linked grammar crates compile clean into `wasm-quarto-hub-client`
  - [ ] Bundle-size regression test (upper-bound per grammar)
  - [ ] Same snapshot tests run in the WASM harness

- **Phase 4 — browser user grammars (minimal v1)**:
  - [ ] `web-tree-sitter` as npm dep in hub-client
  - [ ] wasm-bindgen JS-interop shim: `load_user_grammar(name, wasm_bytes, highlights_scm)` → opaque handle
  - [ ] JS-side parse + query + span-event emission; results returned as the same `data-hl-spans` JSON
  - [ ] End-to-end test with one hand-loaded `.wasm` grammar
  - [ ] Full hub-client user-grammar UX (multi-grammar discovery, sync transport, upload flow) → Phase 6

- **Phase 5 — extension features**:
  - [ ] Language injections
  - [ ] User-override `highlights.scm` for built-in languages
  - [ ] Line numbers and line-highlight directives (chroma-style `{linenos=inline,hl_lines=[2,3]}`)

- **Phase 6 — browser user-grammar UX polish** (post-v1):
  - [ ] Hub-client discovery flow for `_quarto/grammars/*.wasm`
  - [ ] Sync-channel transport of grammar `.wasm` + query files
  - [ ] Upload UX in hub-client UI

- **Phase 7 — Pandoc-bridge parity** (deferred until Pandoc output lands / user demands):
  - [ ] Translate `data-hl-spans` → per-format RawBlock for latex / typst / docx
  - [ ] Taxonomy-adapter layer (tree-sitter captures → skylighting token classes)

## Grammar-loading architecture (native + browser)

Motivated by the requirement to support **user-supplied tree-sitter grammars** (drop a grammar into `_quarto/grammars/` and have it picked up). Static linking alone can't do this. Tree-sitter has a canonical answer: the `.wasm` grammar format, loadable at runtime on both deployment targets. See `claude-notes/research/syntax-highlighting-dynamic-grammars.md`.

**Unified grammar representation:** all grammars — built-in and user — are eventually surfaced to highlighting code as a `tree_sitter::Language`. `HighlightConfiguration::new()` (tree-sitter-highlight) takes a `Language` by value and is agnostic about how it was obtained. This means **one highlighting code path** across all grammar sources and both deployment targets. (Verified in `external-sources/tree-sitter/crates/highlight/src/highlight.rs:353` and `external-sources/tree-sitter/lib/binding_rust/wasm_language.rs:64`.)

### Loading paths

| Source          | Native `quarto`                                          | Browser `wasm-quarto-hub-client`                          |
|-----------------|----------------------------------------------------------|-----------------------------------------------------------|
| Built-in grammar | Statically linked Rust crate (`tree-sitter-python`, …)   | Statically linked Rust crate (same)                       |
| User grammar    | `.wasm` file loaded at runtime via `tree_sitter::WasmStore` (wasmtime under the hood) | `.wasm` file loaded at runtime via `web-tree-sitter` through wasm-bindgen JS interop |

**Built-in grammars stay statically linked** because they're small, fast to parse (direct function call vs wasmtime JIT), and have a simpler compile-time code path. User grammars go through the `.wasm` loader path.

### Where the cost lives

- **Native `quarto` binary**: adding the wasmtime runtime adds ~8–12 MB. The current `quarto` CLI is already ~95 MB (batteries-included); wasmtime is noise against that baseline. **No Cargo feature gate** — wasm grammar loading is always compiled in. Downstream library consumers (LSP, hub server) who don't need user grammars can depend on a thinner subcrate if that becomes necessary later, but we don't pre-build that split.
- **Browser hub-client**: web-tree-sitter adds ~1.5–2 MB compressed. Acceptable.
- **Per user grammar**: 50–200 KB for a typical grammar `.wasm` file on disk; loaded on demand.

### User grammar workflow (target v1 UX)

```
_quarto/
  grammars/
    my-lang/
      my-lang.wasm        # produced by `tree-sitter build --wasm`
      highlights.scm
      injections.scm      # optional
      locals.scm          # optional
```

Then `` ```my-lang `` code blocks pick up the grammar automatically. The alias → grammar mapping is read from the directory name + optional frontmatter in a `grammar.yml`.

### Implementation split

- A new crate `quarto-highlight`:
  - Owns the `Language` registry (built-in statically linked languages + a dynamic `WasmStore` on native).
  - Owns the per-language `HighlightConfiguration` cache.
  - Exposes a stable API that's the same on native and wasm32-unknown-unknown for built-ins; extends with user-grammar discovery on each target.
- Native: user-grammar loading from disk via `WasmStore`. Included by default. `cfg(not(target_arch = "wasm32"))`.
- Browser: user-grammar loading delegated to a JS helper that calls `web-tree-sitter`. JS side returns highlight events as JSON; Rust side serializes into the same `data-hl-spans` encoding.

### Phase mapping

- **Phase 1 (v1 native)**: built-in statically linked grammars + **native user-grammar loading via `WasmStore`**. These share nearly all code except the loader.
- **Phase 2 (v1 browser — built-ins)**: built-in statically linked grammars on browser.
- **Phase 3 (v1 browser — minimal user-grammar loading)**: wire `web-tree-sitter` into hub-client via wasm-bindgen JS interop. Goal in v1: a single `.wasm` user grammar can be loaded and produces correct highlights end-to-end. Full polish (multi-grammar discovery, sync-channel transport of grammar files, hub-side upload UX) is Phase 6.

## Resolved decisions (log)

All locked 2026-04-19 unless otherwise noted. Rationale condensed; see conversation history for full context.

1. **Class-name scheme**: clean-break `hl-{capture-with-dots-replaced-by-hyphens}` (e.g. `hl-function-builtin`). No Pandoc-compat short classes by default. Users who need Pandoc-theme compatibility write a Lua filter that rewrites `data-hl-spans` before the HTML writer runs.

2. **Encoding**: JSON array of `[start_byte, end_byte, capture_name]` triples. 4th positional slot reserved for a future optional extras object, added without version bumping.

3. **Inline `Code`**: in scope for v1. Same `data-hl-spans` encoding, same pipeline stage, same class-naming convention. User writes `` `foo()`{.python} `` to opt in.

4. **Initial language set** (14 classes, 12 grammar crates):
   - `r`, `python` (+ `py` alias), `javascript` (+ `js` alias, handles `jsx`), `jsx` (alias to JS grammar), `typescript` (+ `ts` alias), `tsx`, `bash` (+ `sh` alias), `sql`, `html`, `css`, `json`, `yaml`, `julia`, `lua`.
   - Explicitly dropped from my earlier proposal: Rust, C, C++ (can be added later; user prioritized data-science + web-stack).

5. **Pandoc-bridge writers**: v1 no-op. Pandoc runs its own skylighting for typst/latex/docx when we eventually add those output paths. `data-hl-spans` passes through and is ignored by Pandoc. Parity work deferred to Phase 7.

6. **Highlight query provenance**: start with each built-in grammar's own upstream `highlights.scm`, vendored under `resources/highlights/<lang>/` with commit-hash + license provenance comments. Upgrade per language to [Helix](https://github.com/helix-editor/helix/tree/master/runtime/queries)'s (MPL-2.0), [Zed](https://github.com/zed-industries/zed)'s (MIT), or [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter)'s (Apache-2.0) curated version only when a specific grammar's shipped queries prove inadequate. Keeps licensing decision per-file.

7. **New `quarto-highlight` crate**: yes. Isolates grammar crate deps + wasmtime from `quarto-core`. Exports the pipeline stage + encoding + (on native) the `WasmStore`-based loader.

8. **Browser user grammars in v1**: minimal version — web-tree-sitter npm dep + wasm-bindgen JS-interop shim + one end-to-end test with a hand-loaded grammar. Full hub-client UX (discovery, sync transport, upload UI) moves to Phase 6.

9. **Wasmtime binary-size budget**: accepted as noise against the ~95 MB batteries-included baseline. No Cargo feature gate; wasm-grammar-loading always compiled into native builds.

## Out-of-scope for v1

- Language injections (e.g. JS in HTML `<script>`) — Phase 5.
- Line numbers, line-highlight directives (`hl_lines=[2,3]`, `linenos=inline`) — Phase 5.
- User-override `highlights.scm` for built-in languages — Phase 5.
- Full hub-client user-grammar UX (discovery, sync transport, upload flow) — Phase 6. (v1 gets a minimal one-grammar-at-a-time JS-interop path in Phase 4.)
- Pandoc-bridge parity for typst/latex/docx — Phase 7.
- Theme import from Kate XML / Chroma XML / VS Code JSON themes.
- Client-side (browser-runtime) highlighting in the *output document* — Quarto 2 highlights at build time, period. (Separate from hub-client, which is an authoring tool.)
