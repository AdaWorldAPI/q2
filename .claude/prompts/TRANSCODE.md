# q2 — Build the binary

This repo is the product. One `cargo build` produces one binary that
is a graph notebook with cockpit UX.

## Step 1: Create Cargo.toml with these dependencies

```toml
[dependencies]
notebook-runtime = { git = "https://github.com/AdaWorldAPI/marimo" }
notebook-query   = { git = "https://github.com/AdaWorldAPI/graph-notebook" }
notebook-kernel  = { git = "https://github.com/AdaWorldAPI/kernel-protocol" }
notebook-render  = { git = "https://github.com/AdaWorldAPI/aiwar" }
lance-graph      = { git = "https://github.com/AdaWorldAPI/lance-graph" }
ndarray          = { git = "https://github.com/AdaWorldAPI/ndarray" }
axum             = "0.7"
tokio            = { version = "1", features = ["full"] }

[dependencies.rs-graph-llm]
git = "https://github.com/AdaWorldAPI/rs-graph-llm"
optional = true

[features]
default = []
orchestrator = ["rs-graph-llm"]
```

These crates may not exist yet as Rust crates. If a dependency doesn't
compile, stub it with a local crate that has the right name and empty
public API. Mark it `// TODO: replace when crate is transcoded`.

## Step 2: Write main.rs

Start an axum server on port 2718. Wire:

- `GET /` → serve frontend static files (placeholder index.html for now)
- `GET /health` → `{ "status": "ok" }`
- `POST /mcp/sse` → MCP server (see rs-graph-llm .claude/prompts/SCOPE_F_mcp_server.md)
- MCP tools: cell_execute, cell_get, cells_list, cell_create, cell_update,
  cell_delete, dag_get, notebook_save, notebook_load, notebook_export

cell_execute takes code + optional lang. If lang is omitted, detect it:
- `g.V()` → gremlin
- `MATCH (` → cypher
- `PREFIX` or `SELECT ?` → sparql
- `%>%` or `<-` → r
- `let` / `fn` → rust
- Everything else → markdown

Route to the right crate. Render result through notebook-render.

## Step 3: Write the publisher

This is the hardest part. No Rust Quarto exists.

Read quarto-dev/quarto (TS) to understand what it does. Read
.claude/reference/aiwar/quarto/ to see what output looks like.

For now, implement the minimum:
- `q2 render notebook.nb --format html` → HTML file with embedded results
- `q2 render notebook.nb --format pdf` → shell out to pandoc + wkhtmltopdf

The HTML renderer takes notebook cells and produces a single HTML file:
- Code cells: syntax highlighted, collapsible
- Graph results: embed vis.js/d3 HTML from notebook-render
- Table results: embed DataTables.js HTML from notebook-render
- Markdown cells: render to HTML
- Dark cockpit theme (see rs-graph-llm .claude/PRODUCT_VISION.md)

quarto-r must work with this: `q2 render notebook.qmd` as CLI.

## Step 4: Verify

```bash
cargo build                    # must compile
./target/debug/q2 serve        # starts on :2718
curl localhost:2718/health      # returns ok
./target/debug/q2 render --help # shows render subcommand
```

## What NOT to do

- Don't rewrite lance-graph or ndarray internals
- Don't implement a full Pandoc in Rust (shell out for now)
- Don't build the frontend (placeholder HTML is fine)
- Don't make lance-graph or ndarray optional — they always ship
