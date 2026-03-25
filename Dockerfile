# ══════════════════════════════════════════════════════════════════════
# q2-cockpit — single-binary Dockerfile with live clone
# ══════════════════════════════════════════════════════════════════════
# Clones ALL AdaWorldAPI repos from GitHub at build time.
# Builds the cockpit frontend (Vite/React) and the Rust binary
# (lance-graph + ndarray + V8 JIT + notebook-query + Axum).
# Result: ONE binary, no runtime deps, no Node, no stubs.
#
# Versions pinned:
#   Rust 1.94.0 | Arrow 57 | DataFusion 51 | Node 22
#
# Railway settings:
#   dockerfilePath = "q2/Dockerfile"
#   Memory: 8GB+ (deno_core/V8 compiles are heavy)
#   Disk: 10GB (Cargo cache)
# ══════════════════════════════════════════════════════════════════════

# ── Stage 1: Build everything ─────────────────────────────────────────
FROM debian:bookworm AS builder

# System deps: git, C/C++ toolchain, OpenSSL, protobuf
RUN apt-get update && apt-get install -y \
    git curl build-essential cmake clang \
    libssl-dev pkg-config python3 \
    protobuf-compiler libprotobuf-dev \
    ca-certificates gnupg \
    && rm -rf /var/lib/apt/lists/*

# Node 22 (for cockpit Vite build)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust 1.94.0 — explicit version, NOT latest, NOT 1.93
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# ── Live clone all AdaWorldAPI repos at HEAD ──────────────────────────
RUN git clone --depth 1 https://github.com/AdaWorldAPI/q2.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/rs-graph-llm.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/neo4j-rs.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/aiwar.git

# ── Build the cockpit frontend ────────────────────────────────────────
WORKDIR /build/q2/cockpit
RUN npm install && npm run build && ls -la dist/
# cockpit/dist/ now exists for include_dir!

# ── Build the Rust binary ─────────────────────────────────────────────
WORKDIR /build/q2
RUN cargo build --release --package cockpit-server \
    && ls -lh target/release/q2-cockpit

# ── Stage 2: Minimal runtime ─────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# One binary — everything compiled in (seed data embedded via include_dir)
COPY --from=builder /build/q2/target/release/q2-cockpit ./q2-cockpit

# Health check
HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:${PORT:-2718}/health || exit 1

# Railway injects $PORT; default to 2718
ENV PORT=2718
EXPOSE 2718

CMD ["./q2-cockpit"]
