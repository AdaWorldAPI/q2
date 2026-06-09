# Quarto 2

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/quarto-dev/q2)

> **Experimental** - This project is under active development. It's not yet ready for production use, and will not be for a while.

This repository is a Rust implementation of the next version of [Quarto](https://quarto.org). The goal is to replace parts of the TypeScript/Deno runtime with a unified Rust implementation.

## Building

Requires Rust nightly (edition 2024).

```bash
# Installs nodejs dependencies for the vite/WASM dependencies

npm install
# Build all Rust crates and binaries and test; build hub-client and its TS test suite
cargo xtask verify

# Just build everything instead (use --release for optimized binaries)
cargo xtask build-all

# Run tests (uses nextest)
cargo nextest run
```

## Contributing

We welcome discussions about the project via GitHub issues.
However, the Quarto team will be working on this codebase internally before we're ready to accept outside contributions or make public binary releases/announcements.
Please feel free to use the discussions page for questions and suggestions.

## Status

This is experimental software. All API should be considered unstable and may completely change.

## License

MIT - See [LICENSE](LICENSE) for details.
