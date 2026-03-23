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

---

## Reference: Existing quarto-rust Extension

See `.claude/reference/quarto-rust-extension/` — a Quarto Lua filter from
`AdaWorldAPI/aiwar-neo4j-harvest` that makes Rust code blocks executable
in Quarto documents.

What it does:
- Lua filter detects `{playground-rust}` code blocks
- Adds a "Run" button
- Sends code to play.rust-lang.org via fetch
- Renders result inline

What q2 does differently:
- Executes locally (the binary IS the runtime, no external playground)
- Native Rust, not Lua filters
- Polyglot: Rust, Gremlin, Cypher, R, SPARQL — not just Rust
- Graph results render as vis.js, not text output

This extension is proof the pattern works. q2 makes it native.
