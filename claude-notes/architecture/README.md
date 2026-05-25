# Quarto 2 — Architecture Diagrams

High-level architecture of Quarto 2 for team members, advanced users, and
future contributors. Each diagram is a hand-authored SVG plus a markdown
**content spec** that records what the figure shows and where every claim is
grounded in source.

These figures are intended to be embedded in HTML (a Quarto 2 website or
slides), so each SVG is authored for the browser, not for Illustrator/Inkscape
(see *Conventions* below).

## The set

| # | Diagram | SVG | Spec | Status |
|---|---|---|---|---|
| 1 | **Render pipeline** — two-pass processing, generate/render split, feature locations, q2-preview variant | [`pipeline.svg`](./pipeline.svg) | [`01-pipeline.md`](./01-pipeline.md) | ✅ drafted |
| 2 | **Crate & package map** — Rust crates grouped into subsystems + TS packages | [`crates.svg`](./crates.svg) | [`02-crates.md`](./02-crates.md) | ✅ drafted |
| 3 | **hub-client Automerge structure** — project-as-CRDT schema + WASM preview infra | [`automerge.svg`](./automerge.svg) | [`03-hub-client-automerge.md`](./03-hub-client-automerge.md) | ✅ drafted |
| 4 | **q2 vs hub-client** — build chain, embedded SPA, `q2 preview` ephemeral server, WASM-inside-native | [`q2-preview-wasm.svg`](./q2-preview-wasm.svg) | [`04-q2-preview-wasm.md`](./04-q2-preview-wasm.md) | ✅ drafted |

## Reading model — three tiers

Each diagram is designed to be read as a drill-down:

> **diagram** (the shape) → **guide** (what each part is) → **source** (the code).

A reader skims the SVG, reaches for the companion markdown when a part needs
explaining, and follows the crate/file path there down to the module. To support
this:

- **Every box prints its own source file** (monospace) in the SVG, so the
  diagram itself points toward the code.
- **Numbered markers** (small drawn circles ①②③) sit at the **top-right
  corner** of an element that has a matching entry in the guide's *Notes*
  section (corner placement keeps them off the labels):
  - **indigo** = a note with extra detail;
  - **amber** = *the diagram idealizes here; the current implementation differs*
    — read the note before trusting the box at face value.
- A **companion-guide pointer** in the SVG header names the markdown file.

These documents **cross-link** each other and are written to eventually live in
`docs/` as user-facing Quarto 2 pages, so links assume joint consumption.

## Conventions (apply to every SVG here)

**Authoring**
- **Hand-authored SVG** is the source of truth (no DSL/toolchain step).
- A single `<style>` block with CSS classes holds colors/typography; per-element
  attributes are kept minimal so restyling is one edit.
- **Comments must not contain `--`** (illegal in XML/SVG); use `==` instead.

**For the browser**
- `viewBox` only — no fixed `width`/`height` — plus
  `preserveAspectRatio="xMidYMid meet"`, so the figure scales to its HTML container.
- **System font stack** (`ui-sans-serif, system-ui, …`) and `ui-monospace` for
  code refs — no embedded/loaded fonts.
- No external references (no `<image href>`, no web fonts, no external CSS).
- `role="img"` + `<title>`/`<desc>` for accessibility and search indexing.

**Shared palette**
- Pass 1 / front-end: blue `#e9f2fb` fill, `#4a8fd4` stroke
- Pass 2 / AST: amber `#fdf4e3` fill, `#d6a23c` stroke
- Checkpoint: violet `#efe7fb` / `#7a4fbf`
- **Generate** (format-agnostic): green `#e8f5ee` / `#5aa66f`
- **Render** (format-specific): blue `#eef2fb` / `#6a7bd0`
- native-only stage: gray dashed `#f3f6f9` / `#aeb9c6`
- q2-preview / WASM accents: teal `#3fa3a3`
- stage box: white `#ffffff` / `#cdd5df`; ink `#16202e`

## Editing / previewing

```bash
# validate well-formedness (catches the -- in comments, unescaped &, etc.)
xmllint --noout claude-notes/architecture/pipeline.svg

# preview in the real target (a browser); the SVG renders at viewBox size
open claude-notes/architecture/pipeline.svg          # macOS default app
```

When embedding in a Quarto doc, reference the SVG as an image
(`![Render pipeline](pipeline.svg)`); it will scale to the column width.

## Grounding & caveats

Specs cite exact crate/file paths. Where the **current implementation diverges
from an idealized design** (e.g. the project Pass-2 currently re-runs the head
pipeline rather than resuming from the profile checkpoint), the spec flags it
and the SVG depicts the design with a footnote. See the "Source-vs-design
notes" section in each spec. The earlier `claude-notes/quarto-dependencies.dot`
/ `project-overview.md` artifacts predate the current architecture (they
reference `quarto-markdown`, Pandoc-as-external-tool, the "Kyoto" exploration
phase) and are **superseded** by these diagrams.
