# ══════════════════════════════════════════════════════════════════════
# q2 — single Rust binary, live .qmd rendering
# ══════════════════════════════════════════════════════════════════════
# `q2 notebook serve` runs the full stack:
#   lance-graph parser → DataFusion planner → LanceDB
#   quarto-core + deno_core (V8 JIT) → live .qmd rendering
#   ndarray → SIMD compute
#   MCP over SSE with 16 tools
#
# The Palantir cockpit (React/Vite) is embedded into the binary at
# compile time via include_dir!. Routes:
#   /       → Palantir cockpit with aiwar graph (221 nodes)
#   /demo   → Infrastructure demo (24 seed nodes)
#   /debug  → Neural debugger (18,763 functions across 4 repos)
#   /mcp/*  → MCP endpoints (lance-graph)
#
# Pinned: Rust 1.94.0 | Arrow 57 | DataFusion 51
# ══════════════════════════════════════════════════════════════════════

# ── Stage 1: Build the Vite frontend ─────────────────────────────────
FROM node:22-alpine AS frontend

WORKDIR /build
COPY cockpit/package.json cockpit/package-lock.json ./
RUN npm ci
COPY cockpit/ .
RUN npm run build && ls -la dist/

# ── Stage 2: Build the Rust binary ───────────────────────────────────
FROM debian:bookworm AS builder

RUN apt-get update && apt-get install -y \
    git curl build-essential cmake clang \
    libssl-dev pkg-config python3 \
    protobuf-compiler libprotobuf-dev \
    ca-certificates lld \
    && rm -rf /var/lib/apt/lists/*

# Rust 1.94.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# q2 comes from the Railway build context (this repo, this branch)
COPY . /build/q2

# Copy the built Vite frontend into the cockpit/dist/ directory
# so include_dir! can embed it at compile time
COPY --from=frontend /build/dist/ /build/q2/cockpit/dist/

# Sibling deps — clone from GitHub
# graph-flow stub is local (crates/stubs/graph-flow), no rs-graph-llm needed
RUN git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/neo4j-rs.git

# Build the q2 binary with embedded frontend
WORKDIR /build/q2
RUN cargo build --release -p cockpit-server --features embed-cockpit \
    && ls -lh target/release/q2-cockpit

# ── Runtime ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/q2/target/release/q2-cockpit ./q2-cockpit

# Aiwar data for lance-graph hydration at startup
COPY --from=builder /build/q2/cockpit/public/aiwar_graph.json ./data/aiwar_graph.json
COPY --from=builder /build/q2/cockpit/public/aiwar_weapons.json ./data/aiwar_weapons.json

HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:8080/health || exit 1

ENV PORT=8080
ENV AIWAR_DATA_PATH=/app/data/aiwar_graph.json
EXPOSE 8080
CMD ["./q2-cockpit"]
