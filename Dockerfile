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

# Pull the big FMA body wire (BSO2) from the q2 release into dist/ so include_dir!
# embeds it and the server serves it SAME-ORIGIN at /body.soa.gz. The browser cannot
# fetch the release URL directly (github.com/.../releases/download sends no CORS
# header on its redirect → "TypeError: Failed to fetch"), so /body fetches the
# same-origin copy. The asset stays in the release (downloaded at build), never git.
RUN curl -fSL https://github.com/AdaWorldAPI/q2/releases/download/fma-body-soa-v3-v1/body.soa.gz \
      -o /build/q2/cockpit/dist/body.soa.gz \
 && ls -lh /build/q2/cockpit/dist/body.soa.gz

# Sibling deps — clone from GitHub
# graph-flow stub is local (crates/stubs/graph-flow), no rs-graph-llm needed
#
# lance-graph + ndarray are cloned at their BRANCH HEAD (latest) — NOT a pinned,
# stale SHA. The repos at their tips are mutually consistent, so "use the latest of
# everything" is the rule: a pinned-old lance-graph (36059ce0) is exactly what
# lacked `guid-v3-tail` and broke the build. The `COPY . /build/q2` above changes on
# every q2 commit, invalidating this RUN layer too, so each build re-clones fresh
# (no stale-cache problem the old pin was guarding against).
#
# Sibling checkouts the path deps resolve against:
#   /build/lance-graph  → lance-graph @ main HEAD — carries guid-v2-tail +
#                         guid-v3-tail and the 65-concept ogar_codebook mirror.
#   /build/ndarray      → the REAL AdaWorldAPI/ndarray fork, consumed by BOTH
#                         lance-graph (../../../ndarray) AND q2-ndarray
#                         (../../../../ndarray). `--depth 1` WITHOUT
#                         --recurse-submodules: ndarray's workspace `exclude`s
#                         crates/burn, so the burn submodule (AdaWorldAPI/burn.git)
#                         is never needed — leaving it unfetched is correct.
#
# COUNT_FUSE: lance-graph-ogar asserts (E0080 on mismatch)
# CODEBOOK.len() == ogar_vocab::class_ids::ALL.len(). Both move together at HEAD —
# lance-graph main's 65-concept codebook mirror matches OGAR main's 65-concept vocab;
# q2's Cargo.lock pins ogar-vocab to OGAR main HEAD (a1fb170). Always bump the two
# repos' HEADs together, never one alone.
#
# neo4j-rs is intentionally NOT cloned — a discarded Neo4j-GUI experiment referenced
# by no manifest; the only neo4j path is the opt-in `neo4j-fallback` (crates.io neo4rs).
RUN git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git

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
