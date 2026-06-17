# `{{< include >}}` shortcode path resolution: the "no retargeting" design

**Captured:** 2026-06-16 (while planning bd-9cyza5vy, single-file preview deps).
**Why this exists in the repo, not in a personal memory:** this is a project-wide
design fact + a latent code/intent discrepancy that any contributor touching
include expansion, resource collection, or preview VFS population needs to know.

## The intended design (authoritative, from Carlos)

Quarto include shortcodes do **no path retargeting**. Every path a document
references — image/resource URLs **and** nested `{{< include >}}` arguments —
is meant to be resolved relative to the **original (top-level) file**, not the
file the path textually appears in.

History: Quarto 1's first include implementation *did* retarget paths (resolve
them relative to the included file). That was abandoned because users could not
reason about the boundary between:

- image paths parsed as markdown (retargetable in principle),
- raw-HTML image `src`s authored by the user (retargetable in principle),
- raw-HTML image `src`s emitted by an **executed code cell** (produced *after*
  shortcode resolution — *cannot* be retargeted even in principle).

The simpler, teachable rule won: **no retargeting at all.** The recommended
authoring pattern in projects is **project-root-relative notation with a
leading slash** — `{{< include /path/with/leading/slash.qmd >}}` and
`![](/assets/img.png)` — because then the anchor is irrelevant and the
ambiguity disappears.

Docs: <https://quarto.org/docs/authoring/includes.html> (Carlos authored this
feature).

**Confirmed against the Quarto 1 implementation.**
`external-sources/quarto-cli/src/core/handlers/include-standalone.ts`
(`standaloneInclude`) resolves every include via the *same fixed*
`handlerContext.resolvePath(filename)`; the nested call
`retrieveInclude(params[0])` reuses the *same* `handlerContext`, whose anchor is
the original document's `target.source`. So all include paths — top-level and
nested — resolve against the **original** document. No retargeting. Q1 also
splices raw **text** (`mappedConcat(textFragments)`) and parses the whole
concatenation **once**, so image/resource paths are likewise un-retargeted by
construction. q2 instead splices at the **AST** level
(`expand_includes_in_blocks`), parsing each included file separately — which is
exactly where the nested-include asymmetry crept in (images stayed correct
because `ResourceCollectorTransform` re-anchors at the original deck dir).

## What q2 actually does today (empirically verified)

Fixture (`main.qmd` at root includes `sub/part.qmd`; `sub/part.qmd` contains
both an image and a nested include; `other.qmd`/`img.png` exist at BOTH root
and `sub/`):

```
main.qmd            {{< include sub/part.qmd >}}
sub/part.qmd        ![](img.png)   +   {{< include other.qmd >}}
other.qmd           ROOT-OTHER       sub/other.qmd   SUB-OTHER
img.png             ROOTPNG          sub/img.png     SUBPNG
```

`q2 render main.qmd --to html` produced:

- `<img src="img.png">` in `main.html` (at root) → resolves to **root `img.png`
  (ROOTPNG)**. **No retargeting** — matches the intended design. ✓
- Included text was **`SUB-OTHER`** → the nested `{{< include other.qmd >}}`
  resolved relative to **the including file (`sub/`)**, i.e. it **WAS
  retargeted**. ✗ contradicts the intended design.

### So there is an asymmetry / latent bug

- **Images / resources:** resolved by `ResourceCollectorTransform` over the
  *fully expanded* AST using `input_dir = ctx.document.input.parent()` — the
  *original* deck dir — and `expand_includes_in_blocks` never rewrites URL
  strings. ⇒ correctly **un-retargeted**.
  (`crates/quarto-core/src/transforms/resource_collector.rs:79`,
  `:440` `collect_referenced_asset_urls`.)
- **Nested includes:** `expand_includes_in_blocks` recurses with
  `current_file = &resolved` (the just-included file), so the next level's
  `base_dir = current_file.parent()` is the *including* file's dir — a
  retarget. ⇒ **violates** the no-retargeting design for nested includes in
  subdirectories.
  (`crates/quarto-core/src/stage/stages/include_expansion.rs:90` and the
  recursive call at `:248`.)

A single-level include from the deck root is unaffected (the anchor is the deck
dir either way); the discrepancy only bites a nested include whose *including*
file lives in a subdirectory and whose argument is a relative path.

### Leading-slash caveat (separate, also worth a look)

`base_dir.join("/foo.qmd")` on Unix yields `/foo.qmd` (filesystem root), not
project-root. So the recommended `{{< include /... >}}` notation does **not**
resolve to the project root through this code path as-is — there must be
(or must be added) explicit project-root handling upstream of the bare
`join`. Verify before relying on leading-slash includes; not exercised by the
fixture above.

## Consequence for preview ↔ render parity (bd-9cyza5vy)

`q2 preview` must match `q2 render` **bug-for-bug**, including this asymmetry,
until render itself is changed. The safest way for the single-file preview
dependency resolver to stay in lock-step is to **reuse the renderer's actual
`IncludeExpansionStage`** (run it natively against the real filesystem) rather
than re-deriving include path resolution in a parallel walker — otherwise the
resolver would have to hard-code one side of a contested rule and could drift
from render now or after a future fix. See
`claude-notes/plans/2026-06-16-single-file-preview-transitive-deps.md`.

## Follow-up (filed)

**bd-udrn0q47** (bug, render-side): align nested-include resolution with the
documented no-retargeting design (anchor the recursion at the original
top-level document dir, not the including file), and fix/confirm leading-slash
project-root includes. That is a **render** behaviour change, distinct from the
preview parity work (bd-9cyza5vy), and should land on the render side so preview
inherits it.
