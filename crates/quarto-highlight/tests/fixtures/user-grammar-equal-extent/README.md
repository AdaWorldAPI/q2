# user-grammar-equal-extent — test fixture

A **synthetic** user-grammar fixture whose `highlights.scm` deliberately
captures one node twice at the **same byte range**, producing a genuine
*equal-extent* capture collision. It exists to exercise `flatten_spans`'s
tie-break — the one case innermost-wins cannot decide on its own — through
the real `highlight_captures` code path (not a hand-built span list).

## Why this fixture exists

`flatten_spans` collapses nested/overlapping captures by "innermost
(narrowest) wins". Captures from one `Query` over one tree are
nested-or-disjoint, so for any byte the covering captures form a strict
nesting chain and "narrowest" is unambiguous — **except** when two
captures share *both* start and end (two patterns matching the same node).
That equal-extent case is the only residual ambiguity, and it needs a
deterministic tie-break.

No built-in-language golden in the corpus produces an equal-extent
collision. `user-grammar-toml` was previously assumed to, but it does
not: there `(bare_key) @type` is **nested inside** `(pair (bare_key))
@property` (different ends), and only *appears* equal-extent in the legacy
`collect_spans` output because the bd-98k6 over-wrap bug stretched `type`
to the pair's end. Node-exact extraction (`Query.captures()`) recovers
`type` at its own, narrower extent, so innermost-wins resolves it with no
tie. Hence this purpose-built fixture.

## What it captures

```scheme
(bare_key) @type        ; same node …
(bare_key) @property    ; … captured again → equal-extent collision
"=" @operator
(integer) @number
```

For the input `name = 1`, `name` (the `bare_key`, bytes 0–4) is emitted as
both `type` and `property` at `[0,4]`; `=` and `1` give disjoint
`operator` / `number` spans flanking it. The tie-break test asserts that
`flatten_spans` yields **exactly one** span over `[0,4]` (the deterministic
winner), leaving the flanking spans untouched and the result non-overlapping
and sorted.

## Provenance

The grammar binary is the **same** tree-sitter-toml grammar vendored in
`../user-grammar-toml`, reused purely as a vehicle for a query that
double-captures a node — TOML syntax is incidental.

The loader derives the class name from the `.wasm` stem **and** resolves the
grammar's entry point as `tree_sitter_<stem>`, so the stem must match the
symbol the binary actually exports. This binary exports `tree_sitter_toml`,
so the file is `toml.wasm` and the fixture registers the class `toml`. That
does **not** collide with the real `../user-grammar-toml` fixture: each is
loaded into its own `UserGrammars` instance in its own test, so the class
key `toml` is local to that instance and never in the built-in registry.

- **Grammar**: [tree-sitter-grammars/tree-sitter-toml](https://github.com/tree-sitter-grammars/tree-sitter-toml), tag v0.7.0, commit `64b56832c2cffe41758f28e05c756a3a98d16f41`, MIT.
- `toml.wasm` — byte-identical copy of `../user-grammar-toml/toml.wasm`.
- `highlights.scm` — hand-written for this fixture (NOT upstream); the
  double-capture is intentional.
