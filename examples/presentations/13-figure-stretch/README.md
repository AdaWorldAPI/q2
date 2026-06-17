# 13-figure-stretch — Auto-stretch for captioned & cross-referenceable figures

Sizing a lone *figure* (image **with a caption**) to fill the available space.

This is the figure counterpart to [`11-auto-stretch`](../11-auto-stretch),
which covers bare images. Figures are harder because the image is wrapped in a
`<figure>`/caption structure, and reveal.js only sizes `.r-stretch` elements
that are **direct children of the `<section>`**.

## What this demonstrates

- **Captioned figure (markdown syntax).** `![caption](img)` on an otherwise
  empty slide should stretch the image and show the caption beneath it.
- **Cross-referenceable figure (div syntax).** `::: {#fig-id} … :::` should
  stretch the image, keep the numbered "Figure N: …" caption, and remain a
  valid `@fig-id` cross-reference target.
- **Opting out.** A `.nostretch` figure keeps its natural size.

## How to run

```bash
cargo run --bin q2 -- render examples/presentations/13-figure-stretch
```

## Status

Tracked by braid strand **bd-38ioql41** (follow-up to bd-zkstclhl). See
`claude-notes/plans/2026-06-17-revealjs-autostretch-figures.md` for the design
and which cases are implemented.
