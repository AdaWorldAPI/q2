# q2 — The Binary + Rust Quarto

## Part 1: The Hull

q2 is `main()`. It imports 6 crates and becomes the product.

```rust
fn main() {
    // Start axum server on :2718
    // Serve frontend static files
    // MCP endpoint at /mcp/sse
    // Route cell execution through notebook-runtime
    // Route queries through notebook-query
    // Route R cells through notebook-kernel
    // Render results through notebook-render (AdaWorldAPI/aiwar)
    // Publish through the publishing pipeline (Part 2)
}
```

This part is glue. Thin. The crates do the work.

## Part 2: The Publisher (from scratch — no Rust Quarto exists)

Read the TS quarto (quarto-dev/quarto) to understand what Quarto does.
Read quarto-r to understand the user-facing API.
Read .claude/reference/aiwar/quarto/ to understand what output looks like.

Then build a Rust publishing pipeline:

```
notebook cells → Pandoc AST → filters → HTML or PDF
```

Options for Pandoc integration:
A) Shell out to pandoc binary (simplest, pandoc is ubiquitous)
B) Use pandoc as a library via C FFI (complex but no subprocess)
C) Implement a subset of Pandoc AST in Rust (hardest but pure Rust)

Start with A. Move to C when it matters.

For HTML output: embed the interactive JS from notebook-render.
For PDF output: render graphs as SVG, tables as typeset, use tectonic
or wkhtmltopdf or weasyprint for final PDF generation.

## Part 3: Integration

Wire lance-graph (spine) and ndarray (SIMD) as non-optional deps.
Wire rs-graph-llm as optional behind `--features orchestrator`.

Local %%cypher goes through lance-graph's semiring planner.
Remote %%gremlin/%%cypher goes through notebook-query's Bolt/WS clients.
R goes through notebook-kernel's ZMQ.
All results render through notebook-render (AdaWorldAPI/aiwar).

## Constraints
- lance-graph and ndarray are NEVER optional
- rs-graph-llm IS optional (feature flag)
- quarto-r must work: `q2 render notebook.qmd` CLI interface
- MCP server on :2718 for Claude Code
- Frontend: serve marimo's JS as static files (from notebook-runtime)

Read first. Build. Test.
