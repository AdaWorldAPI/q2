# q2 — Railway Dockerfile
# Multi-stage: Rust binary + React cockpit + aiwar data + lance-graph
#
# Railway settings:
#   Build: Dockerfile
#   Region: us-west (closest to GitHub)
#   Memory: 8GB+ (deno_core compiles are heavy)
#   Disk: 10GB (Cargo cache)
#
# Build context must include lance-graph and aiwar-neo4j-harvest as siblings.
# Use docker build with the parent directory as context:
#
#   cd .. && docker build -f q2/Dockerfile \
#     --build-arg LANCE_GRAPH_DIR=lance-graph \
#     --build-arg AIWAR_DIR=aiwar-neo4j-harvest \
#     -t q2-notebook .
#
# Or on Railway, use a monorepo root with all repos checked out.

# ================================================================
# Stage 1: Build the Rust binary (with lance-graph)
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

ARG LANCE_GRAPH_DIR=lance-graph
ARG AIWAR_DIR=aiwar-neo4j-harvest

WORKDIR /app

# ── lance-graph source (sibling dependency) ──
# Copy the full lance-graph crate tree so Cargo.toml path deps resolve.
COPY ${LANCE_GRAPH_DIR}/Cargo.toml ${LANCE_GRAPH_DIR}/Cargo.lock /app/${LANCE_GRAPH_DIR}/
COPY ${LANCE_GRAPH_DIR}/crates/ /app/${LANCE_GRAPH_DIR}/crates/

# ── q2 workspace manifests (for dep caching) ──
COPY q2/Cargo.toml q2/Cargo.lock q2/rust-toolchain.toml /app/q2/
COPY q2/crates/stubs/ /app/q2/crates/stubs/

# Create dummy src files for all crates to cache deps
# (Railway caches Docker layers — this saves 10+ min on rebuilds)
RUN mkdir -p q2/crates/quarto/src && echo "fn main() {}" > q2/crates/quarto/src/main.rs
RUN mkdir -p q2/crates/pampa/src && echo "" > q2/crates/pampa/src/lib.rs && echo "fn main() {}" > q2/crates/pampa/src/main.rs

# Copy all crate Cargo.tomls (but not src yet)
COPY q2/crates/quarto/Cargo.toml q2/crates/quarto/
COPY q2/crates/pampa/Cargo.toml q2/crates/pampa/
COPY q2/crates/quarto-core/Cargo.toml q2/crates/quarto-core/
COPY q2/crates/quarto-hub/Cargo.toml q2/crates/quarto-hub/
COPY q2/crates/quarto-lsp/Cargo.toml q2/crates/quarto-lsp/
COPY q2/crates/quarto-lsp-core/Cargo.toml q2/crates/quarto-lsp-core/
COPY q2/crates/quarto-citeproc/Cargo.toml q2/crates/quarto-citeproc/
COPY q2/crates/quarto-csl/Cargo.toml q2/crates/quarto-csl/
COPY q2/crates/quarto-config/Cargo.toml q2/crates/quarto-config/
COPY q2/crates/quarto-doctemplate/Cargo.toml q2/crates/quarto-doctemplate/
COPY q2/crates/quarto-error-reporting/Cargo.toml q2/crates/quarto-error-reporting/
COPY q2/crates/quarto-pandoc-types/Cargo.toml q2/crates/quarto-pandoc-types/
COPY q2/crates/quarto-parse-errors/Cargo.toml q2/crates/quarto-parse-errors/
COPY q2/crates/quarto-sass/Cargo.toml q2/crates/quarto-sass/
COPY q2/crates/quarto-source-map/Cargo.toml q2/crates/quarto-source-map/
COPY q2/crates/quarto-system-runtime/Cargo.toml q2/crates/quarto-system-runtime/
COPY q2/crates/quarto-test/Cargo.toml q2/crates/quarto-test/
COPY q2/crates/quarto-treesitter-ast/Cargo.toml q2/crates/quarto-treesitter-ast/
COPY q2/crates/quarto-util/Cargo.toml q2/crates/quarto-util/
COPY q2/crates/quarto-xml/Cargo.toml q2/crates/quarto-xml/
COPY q2/crates/quarto-yaml/Cargo.toml q2/crates/quarto-yaml/
COPY q2/crates/quarto-yaml-validation/Cargo.toml q2/crates/quarto-yaml-validation/
COPY q2/crates/quarto-analysis/Cargo.toml q2/crates/quarto-analysis/
COPY q2/crates/quarto-ast-reconcile/Cargo.toml q2/crates/quarto-ast-reconcile/
COPY q2/crates/quarto-project-create/Cargo.toml q2/crates/quarto-project-create/
COPY q2/crates/comrak-to-pandoc/Cargo.toml q2/crates/comrak-to-pandoc/
COPY q2/crates/xtask/Cargo.toml q2/crates/xtask/
COPY q2/crates/tree-sitter-qmd/Cargo.toml q2/crates/tree-sitter-qmd/
COPY q2/crates/tree-sitter-doctemplate/Cargo.toml q2/crates/tree-sitter-doctemplate/
COPY q2/crates/validate-yaml/Cargo.toml q2/crates/validate-yaml/
COPY q2/crates/lua-src-wasm/Cargo.toml q2/crates/lua-src-wasm/
COPY q2/crates/experiments/reconcile-viewer/Cargo.toml q2/crates/experiments/reconcile-viewer/
COPY q2/crates/pampa/fuzz/Cargo.toml q2/crates/pampa/fuzz/

# Now copy ALL q2 source code
COPY q2/crates/ q2/crates/
COPY q2/resources/ q2/resources/

# Build release (only quarto binary needed)
WORKDIR /app/q2
RUN cargo build --release -p quarto 2>&1 | tail -5

# Verify
RUN ./target/release/q2 --version

# ================================================================
# Stage 2: Build the React cockpit
# ================================================================
FROM node:20-slim AS frontend-builder

WORKDIR /app/cockpit
COPY q2/cockpit/package*.json ./
RUN npm ci --production=false
COPY q2/cockpit/ .
RUN npm run build

# ================================================================
# Stage 3: Minimal runtime image
# ================================================================
FROM debian:bookworm-slim

# Runtime deps only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Binary
COPY --from=rust-builder /app/q2/target/release/q2 /usr/local/bin/q2

# Frontend
COPY --from=frontend-builder /app/cockpit/dist /opt/cockpit/dist

# Aiwar graph data (JSON — the primary test dataset)
ARG AIWAR_DIR=aiwar-neo4j-harvest
COPY ${AIWAR_DIR}/data/aiwar_graph.json /opt/data/aiwar_graph.json

# Cockpit prototype as fallback frontend
COPY q2/cockpit-prototype/q2-cockpit-standalone.html /opt/cockpit/fallback.html

# Tell notebook-query where to find aiwar data
ENV AIWAR_DATA_PATH=/opt/data/aiwar_graph.json

# Health check
HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:${PORT:-2718}/health || exit 1

# Railway injects $PORT
ENV PORT=2718
EXPOSE 2718

CMD ["sh", "-c", "q2 notebook serve --host 0.0.0.0 --port ${PORT:-2718} --frontend-dir /opt/cockpit/dist"]
