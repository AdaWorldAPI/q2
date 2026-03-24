# q2 — Railway Dockerfile
# Multi-stage: Rust binary + React cockpit + aiwar data
#
# Railway settings:
#   Build: Dockerfile
#   Region: us-west (closest to GitHub)
#   Memory: 8GB+ (deno_core compiles are heavy)
#   Disk: 10GB (Cargo cache)

# ================================================================
# Stage 1: Build the Rust binary
# ================================================================
FROM rust:latest AS rust-builder

# Install system deps (protobuf for lance, cmake for tree-sitter)
RUN apt-get update && apt-get install -y \
    cmake \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Use nightly (q2 requires edition 2024)
RUN rustup default nightly
RUN rustup component add rust-src

WORKDIR /app

# Cache deps: copy only manifests first
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/stubs/ crates/stubs/

# Create dummy src files for all crates to cache deps
# (Railway caches Docker layers — this saves 10+ min on rebuilds)
RUN mkdir -p crates/quarto/src && echo "fn main() {}" > crates/quarto/src/main.rs
RUN mkdir -p crates/pampa/src && echo "" > crates/pampa/src/lib.rs && echo "fn main() {}" > crates/pampa/src/main.rs

# Copy all crate Cargo.tomls (but not src yet)
COPY crates/quarto/Cargo.toml crates/quarto/
COPY crates/pampa/Cargo.toml crates/pampa/
COPY crates/quarto-core/Cargo.toml crates/quarto-core/
COPY crates/quarto-hub/Cargo.toml crates/quarto-hub/
COPY crates/quarto-lsp/Cargo.toml crates/quarto-lsp/
COPY crates/quarto-lsp-core/Cargo.toml crates/quarto-lsp-core/
COPY crates/quarto-citeproc/Cargo.toml crates/quarto-citeproc/
COPY crates/quarto-csl/Cargo.toml crates/quarto-csl/
COPY crates/quarto-config/Cargo.toml crates/quarto-config/
COPY crates/quarto-doctemplate/Cargo.toml crates/quarto-doctemplate/
COPY crates/quarto-error-reporting/Cargo.toml crates/quarto-error-reporting/
COPY crates/quarto-pandoc-types/Cargo.toml crates/quarto-pandoc-types/
COPY crates/quarto-parse-errors/Cargo.toml crates/quarto-parse-errors/
COPY crates/quarto-sass/Cargo.toml crates/quarto-sass/
COPY crates/quarto-source-map/Cargo.toml crates/quarto-source-map/
COPY crates/quarto-system-runtime/Cargo.toml crates/quarto-system-runtime/
COPY crates/quarto-test/Cargo.toml crates/quarto-test/
COPY crates/quarto-treesitter-ast/Cargo.toml crates/quarto-treesitter-ast/
COPY crates/quarto-util/Cargo.toml crates/quarto-util/
COPY crates/quarto-xml/Cargo.toml crates/quarto-xml/
COPY crates/quarto-yaml/Cargo.toml crates/quarto-yaml/
COPY crates/quarto-yaml-validation/Cargo.toml crates/quarto-yaml-validation/
COPY crates/quarto-analysis/Cargo.toml crates/quarto-analysis/
COPY crates/quarto-ast-reconcile/Cargo.toml crates/quarto-ast-reconcile/
COPY crates/quarto-project-create/Cargo.toml crates/quarto-project-create/
COPY crates/comrak-to-pandoc/Cargo.toml crates/comrak-to-pandoc/
COPY crates/xtask/Cargo.toml crates/xtask/
COPY crates/tree-sitter-qmd/Cargo.toml crates/tree-sitter-qmd/
COPY crates/tree-sitter-doctemplate/Cargo.toml crates/tree-sitter-doctemplate/
COPY crates/validate-yaml/Cargo.toml crates/validate-yaml/
COPY crates/lua-src-wasm/Cargo.toml crates/lua-src-wasm/
COPY crates/experiments/reconcile-viewer/Cargo.toml crates/experiments/reconcile-viewer/
COPY crates/pampa/fuzz/Cargo.toml crates/pampa/fuzz/

# Now copy ALL source code
COPY crates/ crates/
COPY resources/ resources/
COPY external-sources/ external-sources/

# Build release (only quarto binary needed)
RUN cargo build --release -p quarto 2>&1 | tail -5

# Verify
RUN ./target/release/q2 --version

# ================================================================
# Stage 2: Build the React cockpit
# ================================================================
FROM node:20-slim AS frontend-builder

WORKDIR /app/cockpit
COPY cockpit/package*.json ./
RUN npm ci --production=false
COPY cockpit/ .
RUN npm run build

# ================================================================
# Stage 3: Minimal runtime image
# ================================================================
FROM debian:bookworm-slim

# Runtime deps only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Binary
COPY --from=rust-builder /app/target/release/q2 /usr/local/bin/q2

# Frontend
COPY --from=frontend-builder /app/cockpit/dist /opt/cockpit/dist

# Aiwar data (parquet files when ready, JSON for now)
COPY data/parquet/ /opt/data/parquet/

# Cockpit prototype as fallback frontend
COPY cockpit-prototype/q2-cockpit-standalone.html /opt/cockpit/fallback.html

# Health check
HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:${PORT:-2718}/health || exit 1

# Railway injects $PORT
ENV PORT=2718
EXPOSE 2718

CMD ["sh", "-c", "q2 notebook serve --host 0.0.0.0 --port ${PORT:-2718} --frontend-dir /opt/cockpit/dist"]
