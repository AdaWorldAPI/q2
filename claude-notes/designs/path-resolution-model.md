# Path resolution model (authored paths → resolved → page-relative emit)

**Status:** Index / consolidation note. The model below is already implemented
in parts and discussed across several plans; this note ties them together and
records where the model is **not yet applied** (the theme/css/include gap).
Created 2026-06-10 (discovered while styling `.embed-example`, bd-15uump3h).

## The model (two rules)

Quarto 2 interprets a path written in source (`.qmd`, `_quarto.yml`,
`_metadata.yml`, …) by **two rules**:

1. **No leading `/` → relative to the directory of the file that *declared*
   the path.** Not "relative to the project root" in general — that is only the
   special case for `_quarto.yml`, because `_quarto.yml` *lives* at the project
   root. Concretely:
   - a path in `_quarto.yml` resolves against the project root,
   - a path in `docs/foo/_metadata.yml` resolves against `docs/foo/`,
   - a path in `docs/foo/bar.qmd`'s front matter resolves against `docs/foo/`.
   Getting this right requires **provenance**: the resolver must know *which
   file declared the value*, not which document is currently consuming it. In
   the AST this is carried as `SourceInfo`; for config values it means
   retaining the declaring file through the merge.

2. **Leading `/` → project-root-relative as authored, then translated on emit
   to be relative to the generated HTML file.** No emitted HTML ever contains a
   `/`-absolute link, so a built `_site/` is relocatable under any deploy
   subpath. The translation is page-depth-aware (a depth-2 page emits
   `../../examples/...`).

The pipeline shape is therefore: **authored → resolve to a project-root-relative
form (rule 1) → relativize to the consuming page on emit (rule 2).**

## Where each piece already lives

- **Resolution rule (the `/` interpretation + doc-dir base):**
  `claude-notes/designs/body-link-resolution-contract.md` (2026-04-27).
- **Declaring-file-relative resolution + the full lifecycle (provenance →
  resolve → page-relative):**
  `claude-notes/plans/2026-05-20-bd-qor9a-metadata-path-resolution.md` — the
  most comprehensive writeup; "implicit `!path` semantics for known
  path-shaped keys", resolve at Generate against `dirname(source_info.file)`,
  then `ResourceResolverContext::page_url_for()` on render.
- **Worked example of re-relativizing a declaring-file path (pathdiff):**
  `claude-notes/plans/2026-02-17-dir-metadata-path-resolution.md` —
  `_metadata.yml` `!path` values re-relativized from metadata-dir to doc-dir.
- **Project-relative → page-relative emit translation:**
  `claude-notes/plans/2026-04-29-bd-swpy-nav-href-relativization.md` and
  `claude-notes/plans/2026-04-30-sidebar-title-home-link-relativize.md`
  (`navigation_href.rs`, `ResourceResolverContext::page_url_for`).
- **Provenance infrastructure (SourceInfo on nodes):**
  `claude-notes/designs/provenance-contract.md`.
- **A correct end-to-end instance:** the `.embed-example-iframe` transform's
  `file="/examples/.../slides.html"` (leading `/`) is run through
  `resolve_static_resource_href` and emitted page-relative
  (`../../examples/...`). See `crates/quarto-core/src/transforms/example_embed.rs`.

## Where the model is NOT yet applied (the gap)

Format-level CSS/theme/include options declared in `_quarto.yml` do **not** use
rule 1. They are resolved against the **consuming document's** directory, so a
project-wide entry works only for root-level documents and is silently dropped
for documents in subdirectories:

- **Custom theme SCSS** (`theme: [cosmo, custom.scss]`) — resolved against
  `ThemeContext.document_dir` (`crates/quarto-sass/src/themes.rs:344,468`).
- **`include-in-header` / before-body / after-body** — document-relative
  (`crates/quarto-core/src/stage/stages/include_resolve.rs:38-42`).
- **`css:`** — additionally **never copied** to `_site/` for a website project
  (read + emitted as `<link>` only).

**Bugs tracking the gap:**
- **bd-oejuizi9** — project-wide CSS/theme/include paths resolve per-document-dir
  instead of the declaring file's dir (the rule-1 fix; needs config-value
  provenance threaded to the theme/include resolvers).
- **bd-r1y48cx0** — `css:` files never copied/emitted for website projects.

**Workaround in place (bd-15uump3h):** the `.embed-example` default styling
ships as a **built-in SCSS layer** (`resources/scss/html/templates/embed-example.scss`,
loaded in `crates/quarto-sass/src/compile.rs` alongside `highlight.scss`), which
is compiled into every theme bundle independent of any declaring/consuming
directory — so it sidesteps the gap entirely. That is the correct home for a
*built-in feature's* default look; it is **not** a substitute for fixing rule 1
for user-authored `_quarto.yml` paths.
