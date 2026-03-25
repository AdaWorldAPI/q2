# ══════════════════════════════════════════════════════════════════════
# q2-cockpit — single Rust binary, live .qmd rendering
# ══════════════════════════════════════════════════════════════════════
# lance-graph parser → DataFusion planner → LanceDB storage
# quarto-core + deno_core (V8 JIT) → live .qmd notebook rendering
# ndarray (AdaWorldAPI fork) → SIMD compute
# neo4j-rs → fallback only (Neo4j Aura for live demos)
#
# No static frontend build. No Node at runtime. One binary.
#
# Pinned: Rust 1.94.0 | Arrow 57 | DataFusion 51
# ══════════════════════════════════════════════════════════════════════

FROM debian:bookworm AS builder

RUN apt-get update && apt-get install -y \
    git curl build-essential cmake clang \
    libssl-dev pkg-config python3 \
    protobuf-compiler libprotobuf-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Rust 1.94.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# Live clone — compile deps only
RUN git clone --depth 1 https://github.com/AdaWorldAPI/q2.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/rs-graph-llm.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/neo4j-rs.git

# Build the single binary — no frontend build step needed
WORKDIR /build/q2
RUN cargo build --release --package cockpit-server \
    && ls -lh target/release/q2-cockpit

# ── Runtime ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/q2/target/release/q2-cockpit ./q2-cockpit

HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:${PORT:-2718}/health || exit 1

ENV PORT=2718
EXPOSE 2718
CMD ["./q2-cockpit"]
