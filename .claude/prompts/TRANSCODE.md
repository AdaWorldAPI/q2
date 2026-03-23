# q2 — Finish Quarto 2.0

Read the existing code in this repo. Read quarto (TS) and quarto-r (R)
as reference for what Quarto does.

Finish q2 as a standalone Rust crate. Markdown → Pandoc AST → HTML/PDF.
Graph visualization extension for rendering nodes/edges in documents.

The crate must work on its own. No dependency on marimo,
graph-notebook, kernel-protocol, lance-graph, ndarray, or rs-graph-llm.

Output: a Rust crate in this repo that publishes documents.
Someone adds it to their Cargo.toml, they can render notebooks to PDF.

Read first. Finish. Test.
