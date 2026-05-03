# Empirical trace-size measurements (bd-5qnj)

## Setup

Three fixtures with `trace: true` added to frontmatter, rendered via debug `q2 render`:

- `tiny.qmd` — 124 bytes, single H1 + a list.
- `medium.qmd` — copy of `docs/projects/resources.qmd` (4501 bytes).
- `big.qmd` — copy of `docs/syntax/yaml-metadata.qmd` (6111 bytes; smaller than `navigation.qmd` but the latter currently fails to parse — bd-5qnj investigation does not depend on it).

Output trace at `.quarto/trace/<stem>/latest.json`, written via
`serde_json::to_writer_pretty` (see `crates/quarto-trace/src/write.rs:43`).

## Headline numbers

| fixture  | source | trace (pretty) | trace.gz | minified | minified.gz | amp pretty | amp minified.gz |
|----------|-------:|---------------:|---------:|---------:|------------:|-----------:|----------------:|
| tiny     | 124 B  | 620 KB         | 13 KB    | (n/m)    | (n/m)       | 5001×      | (n/m)           |
| medium   | 4.5 KB | 15.6 MB        | 845 KB   | (n/m)    | (n/m)       | 3456×      | (n/m)           |
| big      | 6.1 KB | 16.3 MB        | 926 KB   | 3.16 MB  | 627 KB      | 2660×      | 103×            |

Even the gzipped pretty file is ~150× the source. Minified-then-gzipped
is ~100× — still large but in a different category, and gzip is
schema-neutral (D8 of `2026-04-14-trace-viewer-design.md` already
plans for `latest.json.gz`).

## Where the bulk lives (big fixture)

47 entries total. Sum of `data` field bytes (compact tostring): **3.16 MB**.

| data_kind        | count | total data bytes | share |
|------------------|------:|-----------------:|------:|
| `DocumentAst`    | 42    | 2,978,274        | 94.3% |
| `AtProfile`      | 2     |   141,858        |  4.5% |
| `RenderedOutput` | 2     |    37,158        |  1.2% |
| `CrossrefIndex`  | 1     |     1,967        |  0.1% |

DocumentAst dominates by an order of magnitude.

## Distinct-AST count

42 DocumentAst entries → **6 distinct contents.** Of 42 snapshots, 36
are byte-for-byte identical to the snapshot in the previous DocumentAst
entry. Stages where the AST actually changed on this document:

1. `metadata-merge`              — first AST snapshot (69,572 B compact).
2. `transform:callout`           — −79 B  (callout shortcode → block transform).
3. `transform:metadata-normalize`— +133 B (normalizes some title/author fields).
4. `transform:sectionize`        — +1,860 B (wraps headings in sections + IDs).
5. `ast-transforms`              — +79 B  (transforms-stage cleanup).
6. `code-highlight`              — +6,083 B (adds highlighting markup).

All other 36 transforms — including all of the website-* and crossref-*
families on this non-website document — emit identical ASTs because
they're no-ops when the relevant metadata isn't set.

**Implication:** content-addressed dedup (every distinct AST stored once;
each entry holds a hash + a JSON-Patch delta when it differs) would
collapse 42 × 70 KB = 2.94 MB to roughly 6 distinct stored ASTs +
small deltas — eliminating the bulk before we even gzip.

## Pretty-print overhead

`serde_json::to_writer_pretty` writes 2-space indentation and one-token-
per-line. On the big fixture:

- pretty: 16.27 MB
- minified (`jq -c`): 3.17 MB (5.1× smaller)
- minified.gz: 627 KB

Pretty-printing alone accounts for **81% of file bytes**. Cheapest
intervention: drop pretty-print (or keep it as an opt-in for `quarto
trace show`); rely on the trace-viewer SPA to format on demand.

## Independence-of-pretty-print check

The data-field totals above are computed from the parsed JSON values (so
they don't include indentation), and they reproduce as ~3.16 MB whether
the file on disk is pretty or minified. The 16 MB → 3 MB compression
ratio is purely whitespace removal, not data loss.

## What replay actually needs (cross-reference to bd-45yw)

Per `claude-notes/plans/2026-05-03-replay-engine.md` (bd-45yw, on its own
branch), the replay engine needs:

- engine name (string),
- input markdown chunks the engine received,
- the engine's `ExecuteResult` (markdown output, `supporting_files`
  paths, `filters`, `includes`, `needs_postprocess`).

**None of that is in the current trace.** The current trace captures
DocumentAst before/after `engine-execution`, but the engine consumes/
emits markdown, not AST — the pre/post-engine ASTs are the result of
the AST→markdown→engine→markdown→AST round-trip, not the engine's
ExecuteResult.

So bd-45yw's Phase 1 needs to add an `EngineCapture` payload to the
trace regardless. The size question for bd-5qnj is whether *one*
artifact (diagnostic + replay) is feasible or whether replay should
get its own minimal artifact.

A minimal replay artifact for this fixture would be:

- engine name (`"markdown"` here),
- input chunks (small — for `engine: markdown`, this is the source
  itself, ~6 KB),
- output `ExecuteResult` (≤ source for non-executing engines, larger
  for jupyter/knitr because of `supporting_files` content),
- format target.

That's order-of-KB, not order-of-MB.

## Provisional size budgets

Targets for discussion with the user:

- **Checked-in CI fixture:** ≤ 100 KB on disk per fixture. This is the
  ceiling that keeps the repo's `.git/` from ballooning if we
  accumulate dozens of fixtures. The current 627 KB minified.gz is 6×
  over this; with dedup + ExecuteResult-only replay format, the
  replay-side artifact comfortably fits.
- **User-attached bug-report artifact:** ≤ 1 MB on disk
  (compressed). Fits a GitHub issue attachment without effort. The
  current 926 KB pretty-gz is borderline; minified.gz is comfortable.
- **Hub-client in-memory trace:** no hard budget yet; the WASM
  no-op observer (D7 invariant) sidesteps this until Phase 4.4 wires
  a VFS-backed observer.

## Methodology

Commands used (from a temp dir; preserved here for reproducibility):

```bash
# Add `trace: true` after the first `---` line in frontmatter
awk 'BEGIN{added=0} /^---$/{count++; print; if(count==1){print "trace: true"; added=1}; next} {print}' f.qmd > f.tmp && mv f.tmp f.qmd

# Render with debug q2; trace lands in .quarto/trace/<stem>/latest.json
q2 render f.qmd
TRACE=$(find .quarto/trace -name latest.json | head -1)

# Sizes
wc -c "$TRACE"
gzip -c "$TRACE" | wc -c
jq -c '.' "$TRACE" > min.json
gzip -c min.json | wc -c

# Per-stage / per-kind breakdown
jq -r '.pipeline | group_by(.data_kind) | map({k:.[0].data_kind,n:length,b:([.[].data|tostring|length]|add)}) | .[] | "\(.k)\t\(.n)\t\(.b)"' "$TRACE"

# Distinct AST snapshots
jq -c '.pipeline[] | select(.data_kind=="DocumentAst") | .data' "$TRACE" | sort -u | wc -l
```
