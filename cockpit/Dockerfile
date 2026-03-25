# ══════════════════════════════════════════════════════════════════════
# q2-cockpit — single-binary Dockerfile with live clone
# ══════════════════════════════════════════════════════════════════════
# Clones all AdaWorldAPI repos at build time, compiles the Rust binary
# (lance-graph + ndarray + V8 JIT + notebook-query) with the cockpit
# UI embedded via include_dir. Result: ONE binary, no runtime deps.

# ── Stage 1: Build frontend ──────────────────────────────────────────
FROM node:22-alpine AS frontend

WORKDIR /build/cockpit
COPY cockpit/package.json ./
RUN npm install --ignore-scripts
COPY cockpit/ .
RUN npm run build

# ── Stage 2: Clone repos + build Rust binary ─────────────────────────
# Start from bookworm with build tools, install Rust 1.94 explicitly
FROM debian:bookworm AS rust-build

# System deps for V8 (deno_core), OpenSSL, git, Rust install
RUN apt-get update && apt-get install -y \
    git cmake clang libssl-dev pkg-config python3 curl build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install Rust 1.94.0 explicitly — NOT latest, NOT 1.93
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# Live clone — all AdaWorldAPI repos at HEAD
RUN git clone --depth 1 https://github.com/AdaWorldAPI/q2.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/rs-graph-llm.git

# Copy the built frontend assets into the tree so include_dir! can find them
COPY --from=frontend /build/cockpit/dist /build/q2/cockpit/dist

# Build the single binary
WORKDIR /build/q2
RUN cargo build --release --package cockpit-server

# ── Stage 3: Minimal runtime ─────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the single binary
COPY --from=rust-build /build/q2/target/release/q2-cockpit ./q2-cockpit

# Railway injects PORT; default to 2718
ENV PORT=2718
EXPOSE 2718

CMD ["./q2-cockpit"]
