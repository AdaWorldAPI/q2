# Diagram 1 — Render Pipeline

**SVG:** [`pipeline.svg`](./pipeline.svg) · **Set index & conventions:** [`README.md`](./README.md)

Companion diagrams: [Crate & package map](./02-crates.md) ·
[hub-client Automerge structure](./03-hub-client-automerge.md) ·
[q2 vs hub-client (build & WASM)](./04-q2-preview-wasm.md).

---

## How to read this

This guide is the **middle tier** of a three-tier drill-down:

> **diagram** (the shape) → **this guide** (what each part is) → **source** (the code).

A reader skims [`pipeline.svg`](./pipeline.svg) for the overall shape, reaches
for this document when a box needs explaining, and follows the crate/file path
in each entry down to the actual module. Every box in the diagram prints its
own source file in monospace, so the diagram already points the way; the tables
below are the index.

**Numbered markers** in the SVG (small circles ①②③, at the top-right corner of
the element they annotate) point to the [Notes](#notes) at the bottom of this
guide:

- **Indigo** marker — a note with extra detail.
- **Amber** marker — *the diagram idealizes here; the current implementation
  differs.* Read the note before trusting the box at face value.

## One-sentence summary

Quarto 2 renders a project in **two passes** over one shared stage graph
(`crates/quarto-core`): **Pass 1** is front-end work across all files (parse,
validate, extract a per-file profile, resolve inter-file dependencies);
**Pass 2** is per-file AST processing, internally split into format-agnostic
**generate** steps and format-specific **render** steps, ending in an HTML
document. The same stage graph — three stages dropped, one inserted — powers
`q2 preview` in WASM (see [diagram 4](./04-q2-preview-wasm.md)).

## Box-by-box → source

### Pass 1 — front-end (marker ① on the band)

Per file, run in parallel with rayon; the head pipeline lives in
`orchestrator.rs` → `pass1_profile_single_file_live()`. All paths below are
under `crates/quarto-core/src/`.

| Box | Source | Role |
|---|---|---|
| `parse-document` | `stage/stages/parse_document.rs` (parser: `pampa`) | qmd → Pandoc AST |
| `metadata-merge` | `stage/stages/metadata_merge.rs` | merge project/dir/doc/runtime metadata |
| `include-expansion` | `stage/stages/include_expansion.rs` | splice `{{< include … >}}` bodies in |
| `document-profile` **(checkpoint, marker ②)** | `document_profile.rs` | extract serializable `DocumentProfile` → `PipelineData::AtProfile` |
| `link-resolution` | `stage/stages/link_resolution.rs` | read-only AST walk → `profile.body_link_targets` |

Output: each file's `DocumentProfile` (title, outline, includes, link targets,
resources) is collected into a shared `ProjectIndex` (`project/index.rs`).

### Between passes

`ProjectType::pre_render()` (`project/orchestrator.rs`); then
`ProjectDependencyGraph` (`project/dependency_graph.rs`) selects the render set —
`RenderMode::{Full, Subset, ActivePage}`. `ActivePage` is the single-page mode
used by `q2 preview` and the hub-client live preview.

### Pass 2 — AST processing (marker ③ on the entry)

The post-checkpoint spine of the full pipeline
(`pipeline.rs` → `build_html_pipeline_stages_with_options()`). Paths under
`crates/quarto-core/src/`.

| Box | Source | Notes |
|---|---|---|
| `unwrap-profile` | `stage/stages/unwrap_profile.rs` | logical resume of the AST — **see Note ③** |
| `pre-engine-sugaring` | `stage/stages/pre_engine_sugaring.rs` | seed crossref registry, desugar shorthand |
| `engine-execution` | `stage/stages/engine_execution.rs` | run code cells (Jupyter, Knitr, markdown) |
| `compile-theme-css` | `stage/stages/compile_theme_css.rs` | Bootstrap SCSS → CSS (`quarto-sass`) |
| `asset-injection` *(native only)* | `stage/stages/bootstrap_js.rs`, `clipboard_js.rs` | inject bootstrap.js / clipboard.js as project artifacts |
| `attribution-generate` | `stage/stages/attribution_generate.rs` | populate author-attribution sidecar |
| `user-filters-pre` | `stage/stages/user_filters.rs` | user **Lua / JSON / citeproc** filters, before Quarto transforms |
| `ast-transforms` | `stage/stages/ast_transforms.rs` | the Quarto feature pipeline (below) |
| `user-filters-post` | `stage/stages/user_filters.rs` | user filters, after Quarto transforms |
| `resource-report` | `stage/stages/resource_report.rs` | finalize per-doc resource report |
| `code-highlight` | `stage/stages/code_highlight.rs` | annotate `data-hl-spans` on code |
| `math-js` | `stage/stages/math_js.rs` | populate `meta.math` (MathJax/KaTeX loader) |
| `render-html-body` | `stage/stages/render_html.rs` (writer in `pampa`) | AST → HTML body |
| `apply-template` | `stage/stages/apply_template.rs` (`quarto-doctemplate`) | wrap body in template |

### `ast-transforms` — the generate/render split

`build_transform_pipeline()` (`pipeline.rs`) runs five phases; transforms live
in `crates/quarto-core/src/transforms/*.rs`. The first three **generate**
format-agnostic structures; the last two **render** them to HTML:

1. **Normalization** *(generate)* — callouts, shortcodes, metadata-normalize, title-block, sectionize, footnotes, theorem/proof/float sugar, equation labels.
2. **Cross-references** *(generate)* — `crossref-index` → `crossref-resolve` (assign numbers, build registry).
3. **Navigation** *(generate)* — toc, navbar, sidebar, page-nav, footer, listing → structured data.
4. **Navigation** *(render)* — `*-render` transforms, listing render, categories, RSS feeds → HTML strings.
5. **Finalization** *(render)* — link-rewrite (cross-doc), appendix, crossref-render, code-block-render, resource-collector, table classes, attribution-render.

The generate/render duality recurs as **paired transforms**:
`TocGenerate`/`TocRender`, `ListingGenerate`/`ListingRender`,
`CrossrefIndex`/`CrossrefRender`, `CodeBlockGenerate`/`CodeBlockRender`. The
single `ast-transforms` stage dispatches a different transform list for HTML vs.
q2-preview, keyed on `ctx.format.pipeline_kind`.

## Feature → pipeline location

| Feature | Where |
|---|---|
| Includes | `include-expansion` (Pass 1), `include-resolve` (full pipeline — see Note ①) |
| Cross-references | `pre-engine-sugaring` + Crossref transform phase |
| Lua / JSON / **citeproc** filters | `user-filters-pre` / `user-filters-post` |
| Code execution (Jupyter/Knitr) | `engine-execution` |
| Shortcodes, callouts | Normalization phase of `ast-transforms` |
| Navigation (navbar/sidebar/TOC/footer) | Navigation generate + render phases |
| Listings (+ RSS feeds) | `listing-item-info` (full pipeline — see Note ①) + Navigation phases |
| Syntax highlighting | `code-highlight` |
| Math | `math-js` |
| Theme CSS / Bootstrap JS | `compile-theme-css` / `asset-injection` |

## Data types flowing through (`PipelineData`)

Defined in `crates/quarto-core/src/stage/data.rs`:

`LoadedSource` → `DocumentAst` → **`AtProfile`** (checkpoint) → `DocumentAst`
→ … → `RenderedOutput` → final HTML. (`DocumentSource`, `ExecutedDocument`, and
`FinalOutput` variants also exist; the first is largely vestigial post-parse,
the latter two are reserved for future engine/SSG work.)

## q2 preview — pipeline variant

Same stage graph, reused in WASM (`wasm-quarto-hub-client`) for the live
in-browser preview. Built by `build_q2_preview_pipeline_stages()`; the dropped
set is `Q2_PREVIEW_STAGE_EXCLUDED`. See [diagram 4](./04-q2-preview-wasm.md) for
how this runs inside `q2 preview`.

- Drops the three HTML-emitting stages: `math-js`, `render-html-body`, `apply-template`.
- Inserts `capture-splice` before `engine-execution` (`stage/capture_splice.rs`) — replays server-recorded engine output instead of re-running engines in the browser.
- Returns the Pandoc AST as JSON to the React renderer (`ts-packages/preview-renderer`), not an HTML string.
- Keeps `code-highlight` (AST-level `data-hl-spans`, read by the React `CodeBlock`).

---

## Notes

These match the numbered markers in [`pipeline.svg`](./pipeline.svg). Notes ①
and ③ are **amber** in the diagram: the figure shows the idealized two-pass
*design*; the current implementation differs as described here.

### ① Pass-1 head pipeline is a 5-stage subset — *amber*

`pass1_profile_single_file_live()` (`project/orchestrator.rs`) runs only five
stages: `parse-document → metadata-merge → include-expansion → document-profile
→ link-resolution`. The diagram's Pass-1 band shows exactly these.

The **full single-document pipeline**
(`build_html_pipeline_stages_with_options`) runs two *additional* pre-checkpoint
stages — `include-resolve` and `listing-item-info` — whose own comments say they
run "pre-checkpoint so values land in `DocumentProfile`." In project mode those
two execute during **Pass 2's** full-pipeline run, not during the
index-building Pass 1. So a profile built in Pass 1 does not reflect them.
→ `crates/quarto-core/src/project/orchestrator.rs`,
`crates/quarto-core/src/pipeline.rs`.

### ② `document-profile` is the checkpoint — *detail*

`DocumentProfileStage` (`document_profile.rs`) extracts a typed, serializable
`DocumentProfile` into `PipelineData::AtProfile`, then `unwrap-profile` hands
the AST straight back. Profiles are **read-only**; project-scoped features
(sidebars, cross-document links, incremental rebuilds, eventual `freeze`)
consume the profile without re-running engines or user filters. Full contract:
[`claude-notes/designs/document-profile-contract.md`](../designs/document-profile-contract.md).
→ `crates/quarto-core/src/document_profile.rs`.

### ③ Pass 2 re-runs rather than resumes — *amber*

The design intent is that Pass 2 resumes each file from the cloned
`PipelineData::AtProfile` produced in Pass 1. The current **v1** implementation
(`orchestrator.rs`, module docs §"Pass-2 resumption (v1)") instead **re-runs the
head pipeline** per file (re-parse + re-merge), accepted as a scoped-rewiring
trade-off with a follow-up tracked. The diagram draws `unwrap-profile` as the
*logical* resume point.
→ `crates/quarto-core/src/project/orchestrator.rs` (see the module-level
"Two passes" / "Pass-2 resumption (v1)" docs).
