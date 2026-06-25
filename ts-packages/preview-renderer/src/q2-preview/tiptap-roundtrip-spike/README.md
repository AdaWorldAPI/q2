# tiptap round-trip spike (THROWAWAY)

Strand **bd-sjb4pzx8** · plan
`claude-notes/plans/2026-06-23-tiptap-rich-text-block-editor.md`.

**This is a throwaway feasibility spike, not production code.** It answers one
question: *can we faithfully round-trip prose-rich + Quarto-opaque qmd through a
ProseMirror document?* Verdict: **yes** (15/16 exact, 1 benign reformat, 0
broken). Quarantine or delete before any non-experimental merge.

## What it does

```
qmd → pampa (untransformed, source-tracked AST) → astToPm → ProseMirror doc
    → prosemirror-markdown serialize → md_out → pampa → AST → compare (semantic)
```

Opaque constructs (shortcodes, math, `@crossref`, `[@cite]`, raw inline, and —
for the spike only — whole `Div`/`RawBlock`/`Table` blocks) become atomic
**chip** nodes that re-emit their exact source bytes.

Note: the spike is **headless ProseMirror** (`prosemirror-model` +
`prosemirror-markdown`). tiptap itself is the React editing UI on top; it is
*not* exercised here. The round-trip fidelity question lives in the model +
serializer, which is what this measures.

## Run

```bash
cd ts-packages/preview-renderer
npx vitest run src/q2-preview/tiptap-roundtrip-spike/
```

Oracle = the native `pampa` binary (built on first run); no WASM init needed.
Results (per-fixture `qmd → md_out`) are written to `RESULTS.md`.

## Files

- `fixtures.ts` — the qmd corpus
- `pampa.ts` — native-pampa oracle + byte-slice helpers
- `schema.ts` — PM schema = markdown schema + `chip` node
- `astToPm.ts` — Pandoc AST → PM doc (chip detection by typed-AST walk)
- `pmToMarkdown.ts` — PM doc → markdown (stock serializer + chip rule)
- `canonical.ts` — source-stripping / whitespace-normalizing comparator
- `roundtrip.test.ts` — the oracle + findings table + `RESULTS.md` writer
