# q2 — The Binary

## What This Is
q2 is the hull. The `main()`. The single binary that imports everything
and becomes the product. `cargo install q2` gives you a graph notebook
with cockpit UX.

## Architecture

```toml
# q2/Cargo.toml

# Always ship (the car)
notebook-runtime = { git = "AdaWorldAPI/marimo" }       # reactive cell DAG
notebook-query   = { git = "AdaWorldAPI/graph-notebook" } # Gremlin/Cypher/SPARQL
notebook-kernel  = { git = "AdaWorldAPI/kernel-protocol" } # R via ZMQ
notebook-render  = { git = "AdaWorldAPI/aiwar" }          # d3/three.js/tables → HTML
lance-graph      = { git = "AdaWorldAPI/lance-graph" }    # spine (storage + semiring)
ndarray          = { git = "AdaWorldAPI/ndarray" }        # SIMD kernels

# Optional (LangStudio orchestration)
rs-graph-llm     = { git = "AdaWorldAPI/rs-graph-llm", optional = true }

[features]
default = []
orchestrator = ["rs-graph-llm"]
```

lance-graph and ndarray are never optional. The spine and SIMD ship
with every build.

## What q2 Does On Top

q2 is not just glue. It adds:

1. **Publishing** — the Rust Quarto replacement. Notebook → PDF/HTML.
   No equivalent exists in Rust. This is from-scratch, hardest piece.
   Reference: quarto (TS/Deno) + quarto-r (R bindings).

2. **The binary** — main(), axum server, MCP endpoint, static file
   serving for the frontend.

3. **Auto-rendering** — looks at query result shape, picks the right
   notebook-render function (graph/table/chart/scalar).

4. **Language detection** — pattern match on first tokens, route to
   the right notebook-query executor.

## Publishing Reference

There is NO existing Rust Quarto. Sources to study:

- quarto (TS/Deno): github.com/quarto-dev/quarto — the real renderer
  Read this to understand Pandoc AST manipulation, cell execution,
  cross-references, format output. Reference only, not a dependency.

- quarto-r: github.com/AdaWorldAPI/quarto-r — R bindings that call
  the quarto CLI. Shows the user-facing API. Bardioc uses this.
  When q2 replaces the CLI, quarto-r calls q2 instead.

- jupyterlab-quarto: github.com/quarto-dev/jupyterlab-quarto —
  JupyterLab extension for Quarto preview. TS. Shows how a frontend
  integrates with the renderer.

- aiwar quarto patterns: .claude/reference/aiwar/quarto/ — shows
  what the output should look like (interactive graph as primary
  content, filterable tables, executable code blocks).

## What Publishing Needs to Do

```
Input:  notebook cells (code + results + markdown)
Output: HTML (interactive, vis.js/d3/three.js embedded)
        PDF  (static, graphs as SVG, tables typeset)

Pipeline:
  cells → Pandoc AST → apply filters → render to format

Filters:
  - Code cells: syntax highlight, fold/unfold
  - Graph results: embed vis.js HTML (HTML) or render SVG (PDF)
  - Tables: embed DataTables.js (HTML) or typeset (PDF)
  - Charts: embed plotly.js (HTML) or render PNG (PDF)
  - Markdown: standard Pandoc processing
  - Cross-references: figure/table numbering
```

## External Process: R Only

quarto-r stays R. It calls q2 as a CLI subprocess:
```r
quarto_render("notebook.qmd")  # calls `q2 render notebook.qmd`
```

Arrow IPC for data exchange between R cells and the binary.

## MCP Server

Port 2718, SSE transport. See rs-graph-llm SCOPE_F_mcp_server.md.
Claude Code connects and drives the notebook as an agent.
