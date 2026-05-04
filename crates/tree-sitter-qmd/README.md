# tree-sitter-qmd

`tree-sitter-qmd` is the tree-sitter grammar for Quarto Markdown
(`.qmd`). It is an internal Quarto-Rust crate, written entirely to
support `quarto-markdown-pandoc`, and is not published outside this
workspace.

The grammar was originally derived from
[MDeiml's `tree-sitter-markdown`](https://github.com/MDeiml/tree-sitter-markdown)
(MIT-licensed — see `LICENSE` for the original copyright notice). It
has since been developed independently and is no longer kept in sync
with upstream; report Quarto-specific issues against this repo, not
the upstream project.

For more information on the syntax supported by Quarto Markdown,
see the top-level docs folder, and specifically
[`dev-docs/syntax-notes.md`](../../dev-docs/syntax-notes.md).

## Architecture

This crate uses a **unified grammar**
(`tree-sitter-markdown/grammar.js`) that parses both block structure
and inline content in a single pass, producing one syntax tree with
all nodes (block and inline).
