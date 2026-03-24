# Quarto 2

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/quarto-dev/q2)

> **Experimental** - This project is under active development. It's not yet ready for production use, and will not be for a while.

This repository is a Rust implementation of the next version of [Quarto](https://quarto.org). The goal is to replace parts of the TypeScript/Deno runtime with a unified Rust implementation, enabling:

- Shared validation logic between CLI and Language Server Protocol (LSP)
- Improved performance, particularly for LSP operations
- Single-binary distribution

## Why Rust?

Posit has been investing in Rust for developer tooling:

- [Air](https://github.com/posit-dev/air/) - An R formatter and LSP written in Rust
- [Ark](https://github.com/posit-dev/ark/) - An R kernel for Jupyter written in Rust

Rust offers compelling advantages for Quarto's tooling:

- **Performance** - Native compilation provides significant speedups for parsing and validation, critical for responsive LSP experiences
- **WebAssembly** - Rust compiles to WASM, enabling browser-based tooling and editor integrations without separate runtime dependencies
- **Single binary** - No runtime installation required; simpler distribution and deployment
- **Memory safety** - Eliminates entire classes of bugs without garbage collection overhead

## Key Crates

### pampa

The most mature crate in this workspace. **pampa** is our Rust port of [Pandoc](https://pandoc.org), the universal document converter. While not a feature-for-feature reimplementation, pampa offers many of the same APIs and will feel familiar to Pandoc users.

Currently, pampa focuses on parsing Quarto Markdown (QMD) and producing Pandoc AST output with full source location tracking.

```bash
# Parse QMD to Pandoc JSON
cargo run -p pampa -- input.qmd -t json

# Parse with verbose tree-sitter output (for debugging)
cargo run -p pampa -- input.qmd -t json -v
```

**Features:**
- Tree-sitter based parsing (block + inline grammars)
- Multiple output formats: JSON, HTML, ANSI, Markdown, plaintext
- Lua filter support (Pandoc-compatible)
- Source location tracking through all transformations

### Graph Notebook (new)

The graph notebook extends q2 with **graph-native cell execution** — query Neo4j/graph databases directly from notebook cells using Cypher, Gremlin, or SPARQL, with reactive dependency tracking and live visualization.

```bash
# Run the graph notebook server
cargo run -p quarto -- notebook --port 3000
```

**Stub crates** (under `crates/stubs/`, to be replaced with full implementations):

| Crate | Purpose |
|-------|---------|
| `notebook-runtime` | Reactive cell DAG with dependency tracking |
| `notebook-query` | Gremlin/Cypher/SPARQL query execution |
| `notebook-kernel` | R kernel protocol (Jupyter-compatible) |
| `notebook-render` | HTML rendering for graphs, tables, charts |
| `lance-graph` | Graph storage with vertex/edge CRUD |
| `q2-ndarray` | SIMD array operations (zeros, ones, matmul) |

The notebook server uses **axum** with SSE streaming and MCP (Model Context Protocol) support.

### Supporting Infrastructure

The crates in this workspace share a focus on **precise source location tracking** and **uniform error reporting**:

| Crate | Purpose |
|-------|---------|
| `quarto-source-map` | Unified source location tracking with transformation history |
| `quarto-error-reporting` | Structured diagnostics with tidyverse-style formatting |
| `quarto-yaml` | YAML parsing with fine-grained source locations |
| `quarto-xml` | XML parsing with source tracking (for CSL files) |
| `quarto-pandoc-types` | Pandoc AST type definitions |
| `quarto-doctemplate` | Pandoc-compatible document template engine |
| `quarto-citeproc` | Citation processing engine using CSL styles |
| `quarto-sass` | SCSS/Sass compilation |
| `quarto-hub` | Collaborative editing server |
| `quarto-lsp` | Language Server Protocol implementation |
| `quarto-config` | Project configuration management |

## Source Location Tracking

A core design principle: every semantic entity carries source location information through all transformations. This enables:

- Precise error messages pointing to exact locations in source files
- Provenance tracking through string extraction, concatenation, and filtering
- Serializable source info for LSP caching

```rust
// Source info tracks transformations
enum SourceInfo {
    Original { ... },           // Direct file position
    Substring { parent, ... },  // Extracted from parent
    Concat { pieces, ... },     // Multiple sources combined
    FilterProvenance { ... },   // Created by Lua filter
}
```

## Error Reporting

Errors use [ariadne](https://github.com/zesterer/ariadne) for precise, visually clear diagnostics:

```
$ echo '_hello world' | quarto-markdown-pandoc -t json

Error: [Q-2-5] Unclosed Underscore Emphasis
   ╭─[<stdin>:1:13]
   │
 1 │ _hello world
   │ ┬           ┬
   │ ╰────────────── This is the opening '_' mark.
   │             │
   │             ╰── I reached the end of the block before finding a closing '_' for the emphasis.
───╯
```

## Building

Requires Rust nightly (edition 2024).

```bash
# Build all Rust crates and binaries; build hub-client and its TS test suite
cargo xtask verify

# Run tests (uses nextest)
cargo nextest run
```

## Full Crate Index

### Binaries
| Crate | Description |
|-------|-------------|
| `quarto` | Main CLI binary (`q2`) |
| `pampa` | QMD parser binary |
| `qmd-syntax-helper` | QMD syntax migration tool |
| `validate-yaml` | YAML validation tool |

### Core Libraries (14)
| Crate | Description |
|-------|-------------|
| `quarto-core` | Core rendering infrastructure |
| `quarto-util` | Shared utilities |
| `quarto-error-reporting` | Uniform error messages (ariadne) |
| `quarto-source-map` | Source location tracking |
| `quarto-yaml` | YAML parser with source locations |
| `quarto-yaml-validation` | Schema-based YAML validation |
| `quarto-xml` | Source-tracked XML parsing |
| `quarto-pandoc-types` | Pandoc AST type definitions |
| `quarto-doctemplate` | Document template engine |
| `quarto-csl` | CSL parsing with source tracking |
| `quarto-citeproc` | Citation processing |
| `quarto-sass` | SCSS compilation |
| `quarto-config` | Project configuration |
| `quarto-parse-errors` | Parse error infrastructure |

### Grammars (3)
| Crate | Description |
|-------|-------------|
| `tree-sitter-qmd` | Tree-sitter grammar for QMD |
| `tree-sitter-doctemplate` | Tree-sitter grammar for templates |
| `quarto-treesitter-ast` | Generic tree-sitter AST utilities |

### Server/Infrastructure (5)
| Crate | Description |
|-------|-------------|
| `quarto-hub` | Collaborative editing server |
| `quarto-lsp` | Language Server Protocol |
| `quarto-lsp-core` | LSP core logic |
| `quarto-system-runtime` | System runtime abstraction |
| `quarto-test` | Testing utilities |

### Graph Notebook Stubs (6)
| Crate | Description |
|-------|-------------|
| `notebook-runtime` | Reactive cell DAG |
| `notebook-query` | Multi-language query execution |
| `notebook-kernel` | R kernel protocol |
| `notebook-render` | HTML rendering |
| `lance-graph` | Graph storage |
| `q2-ndarray` | SIMD array operations |

### WASM (3)
| Crate | Description |
|-------|-------------|
| `wasm-qmd-parser` | WASM entry points for pampa |
| `wasm-quarto-hub-client` | WASM hub client |
| `wasm-bindgen-futures-patch` | wasm-bindgen compatibility |

### Other (4)
| Crate | Description |
|-------|-------------|
| `comrak-to-pandoc` | Comrak AST to Pandoc conversion |
| `quarto-analysis` | Analysis tools |
| `quarto-ast-reconcile` | AST reconciliation |
| `quarto-project-create` | Project scaffolding |
| `xtask` | Build/lint/verify automation |
| `lua-src-wasm` | Lua WASM source |

## Related Repositories

| Repo | Role |
|------|------|
| [quarto](https://github.com/AdaWorldAPI/quarto) | VS Code extension, editor, LSP (TypeScript) |
| [quarto-r](https://github.com/AdaWorldAPI/quarto-r) | R language bindings |
| [neo4j-rs](https://github.com/AdaWorldAPI/neo4j-rs) | Graph database backend |
| [aiwar-neo4j-harvest](https://github.com/AdaWorldAPI/aiwar-neo4j-harvest) | Graph data pipeline |
| [aiwar](https://github.com/AdaWorldAPI/aiwar) | AI War Cloud dataset |

## Contributing

We welcome discussions about the project via GitHub issues.
However, the Quarto team will be working on this codebase internally before we're ready to accept outside contributions or make public binary releases/announcements.
Please feel free to open issues for questions, suggestions, or bug reports.

## Status

This is experimental software. All APIs should be considered unstable and may completely change.

## License

MIT - See [LICENSE](LICENSE) for details.
