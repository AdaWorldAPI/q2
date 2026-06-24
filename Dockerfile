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
#
# lance-graph is PINNED to an explicit commit (NOT `--depth 1 main`) for two
# reasons:
#   1. Cache-bust. A `--depth 1 main` clone lives in its own Docker layer that
#      an empty/unrelated q2 commit does NOT invalidate, so Railway reuses a
#      STALE lance-graph from an earlier build. Bumping this SHA changes the
#      RUN and forces a fresh clone.
#   2. COUNT_FUSE lockstep. lance-graph-ogar compile-asserts (E0080 on mismatch)
#      that lance_graph_contract::ogar_codebook::CODEBOOK.len() ==
#      ogar_vocab::class_ids::ALL.len(). q2's Cargo.lock pins ogar-vocab to a
#      fixed OGAR SHA (302c284 = 43 concepts); the lance-graph clone MUST carry
#      the matching 43-concept mirror. 36059ce0 is the #595 merge (ogar_codebook
#      synced to 43) — the matched pair of the ogar-vocab pin.
# WHEN OGAR MINTS CONCEPTS: bump ogar-vocab in q2's Cargo.lock AND this SHA
# together (after the lance-graph mirror lands), or the fuse trips again.
ARG LANCE_GRAPH_REF=36059ce0
RUN git clone https://github.com/AdaWorldAPI/lance-graph.git \
 && git -C lance-graph checkout "${LANCE_GRAPH_REF}" \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/neo4j-rs.git

# CPU baseline: x86-64-v4 (the 4th microarch level — AVX-512F/BW/CD/DQ/VL on top
# of v3's AVX2+FMA). This is the compile FLOOR; it flips on `target_feature =
# "avx512f"`, so q2-ndarray's `simd.rs` dispatch selects its native `simd_avx512`
# backend (`__m512`/`__m512d`/`__m512i`) instead of the v3 AVX2 default.
#
# BF16 + AMX 16x16 tile GEMM are NOT gated by this flag — they ride q2-ndarray's
# CPU-AGNOSTIC runtime autodetect polyfill (`simd_caps()` + the AMX `arch_prctl`
# XTILEDATA enable + CPU-model detect). The polyfill opportunistically lights them
# up only when the *runtime* host actually has them, and always keeps the AVX2 /
# scalar paths it compiled in as fallback. So: AVX-512 = compile baseline here;
# BF16/AMX = runtime-detected; everything below v4 = polyfill fallback.
#
# ⚠ REQUIREMENT: a v4 floor makes the binary REQUIRE AVX-512 at run time — it
# SIGILLs on the first `__m512` op on a host without it (the PR #170 failure mode,
# one level up). The Railway *build* machine needs no AVX-512 (compiling != run),
# but the *deploy* host does. AMX additionally needs a Sapphire/Emerald/Granite
# Rapids Xeon at run time; on anything older the autodetect simply skips AMX (that
# is the agnostic polyfill working as intended, not an error). If a deploy target
# may lack AVX-512, drop this to `x86-64-v3` and rely on runtime dispatch for the
# AVX-512/AMX paths — one portable binary, same hot paths when the silicon allows.
ENV CARGO_BUILD_RUSTFLAGS="-C target-cpu=x86-64-v4"

# Build the q2 binary with embedded frontend
WORKDIR /build/q2
RUN cargo build --release -p cockpit-server --features embed-cockpit,planner \
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
