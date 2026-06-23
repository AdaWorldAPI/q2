// THROWAWAY SPIKE (bd-sjb4pzx8) — qmd corpus for the markdown round-trip feasibility test.
// See claude-notes/plans/2026-06-23-tiptap-rich-text-block-editor.md. Safe to delete.

export interface Fixture {
  name: string;
  qmd: string;
  // Constructs we expect to require an opaque "chip" (verbatim passthrough).
  expectChips?: boolean;
  notes?: string;
}

export const FIXTURES: Fixture[] = [
  {
    name: 'plain-para',
    qmd: 'A simple paragraph of prose with nothing special in it.\n',
  },
  {
    name: 'para-inline-formatting',
    qmd: 'A paragraph with **bold**, *italic*, `inline code`, and a [link](https://example.com).\n',
  },
  {
    name: 'atx-heading',
    qmd: '## A second-level heading\n',
  },
  {
    name: 'bullet-list',
    qmd: '- first item\n- second item\n- third item\n',
  },
  {
    name: 'ordered-list',
    qmd: '1. alpha\n2. beta\n3. gamma\n',
  },
  {
    name: 'nested-list',
    qmd: '- top\n    - nested a\n    - nested b\n- back to top\n',
  },
  {
    name: 'blockquote',
    qmd: '> a quoted line\n> continued on the next line\n',
  },
  {
    name: 'code-block-plain',
    qmd: '```\nplain code\nno language\n```\n',
  },
  {
    name: 'code-block-quarto',
    qmd: '```{python}\nprint("hi")\n```\n',
    expectChips: false,
    notes: 'Quarto executable cell: the `{python}` info string must survive.',
  },
  {
    name: 'shortcode',
    qmd: 'Watch this: {{< video https://youtu.be/abc >}} for details.\n',
    expectChips: true,
  },
  {
    name: 'inline-math',
    qmd: 'The famous identity $e^{i\\pi} + 1 = 0$ is elegant.\n',
    expectChips: true,
  },
  {
    name: 'display-math',
    qmd: '$$\\int_0^1 x^2 \\, dx = \\frac{1}{3}$$\n',
    expectChips: true,
  },
  {
    name: 'crossref',
    qmd: 'As shown in @fig-plot, the trend is clear.\n',
    expectChips: true,
  },
  {
    name: 'citation',
    qmd: 'This is well established [@knuth1984].\n',
    expectChips: true,
  },
  {
    name: 'callout-div',
    qmd: '::: {.callout-note}\nThis is a note.\n:::\n',
    expectChips: true,
    notes: 'In real integration we reach INTO the div; here we test whole-slice round-trip.',
  },
  {
    name: 'raw-html-inline',
    qmd: 'Some text with <span class="x">raw html</span> inside.\n',
    expectChips: true,
  },
];
