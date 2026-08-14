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
    ca-certificates lld zip unzip \
    && rm -rf /var/lib/apt/lists/*

# Rust 1.94.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# ── Release assets (~180 MB), fetched BEFORE `COPY . /build/q2` on purpose ────
#
# These three wires are served SAME-ORIGIN out of cockpit/dist/ (include_dir!
# embeds it). The browser cannot fetch the release URL directly — the
# github.com/.../releases/download redirect sends no CORS header, giving the
# "TypeError: Failed to fetch" once seen live on /geo — so the copy has to be
# ours. The assets stay in the release; they are never committed to git.
#
#   body.20260629c.soa.gz         -> /body.soa.gz   (BSO2; 20260629c re-bake:
#     teeth → skeleton + per-vessel diameter boundary, no stray fat branches).
#     Served AS body.soa.gz so /body picks it up; the old asset stays untouched.
#   body.20260629c.v6helix.soa.gz -> the /helix wire (BSO2 ver 6 = F16 pos + a
#     canonical Signed360 NORMAL column in one SoA), named by
#     cockpit/public/body.manifest.json (helix_latest).
#   berlin.helix.soa.gz           -> the /geo + /helix?scene=osm scene
#     (manifest osm_latest); 92 MB, so release-hosted rather than in git.
#
# WHY THIS SITS ABOVE `COPY . /build/q2` (2026-08-12): it used to sit below it,
# which put ~180 MB of downloads downstream of a layer that changes on EVERY
# commit. Docker invalidates from the changed layer onward, so each deploy —
# including doc-only ones — re-pulled all three, and each re-pull was another
# chance to hit the flaky path below. Above the COPY, the fetch layer is keyed
# only on these URLs: a code-only commit reuses it and downloads nothing. The
# copy into dist/ moves below the COPY, where it is a local file copy.
#
# WHY `--http1.1` (2026-08-12): the deploy failed repeatedly with
# `curl: (56) Connection died, tried 5 times before giving up` at 0 bytes
# received, while the same asset pulled intact from another host (HTTP 200,
# 61,787,404 bytes at 63 MB/s) — so the asset and the release were fine. That
# exact message is emitted by curl's HTTP/2 code path when the h2 connection
# dies mid-stream; pinning HTTP/1.1 avoids that path entirely. `--retry` alone
# does NOT cover exit 56 (curl classes only timeouts/5xx as transient), so
# `--retry-all-errors` is what makes the retries apply to the error that
# actually happened. One RUN = one cached layer for all three.
ARG CURL_RETRY="--http1.1 --retry 8 --retry-all-errors --retry-max-time 180 --connect-timeout 20"
ARG Q2_RELEASE=https://github.com/AdaWorldAPI/q2/releases/download/fma-body-soa-v3-v1
RUN mkdir -p /assets \
 && curl -fSL $CURL_RETRY $Q2_RELEASE/body.20260629c.soa.gz         -o /assets/body.soa.gz \
 && curl -fSL $CURL_RETRY $Q2_RELEASE/body.20260629c.v6helix.soa.gz -o /assets/body.20260629c.v6helix.soa.gz \
 && curl -fSL $CURL_RETRY $Q2_RELEASE/berlin.helix.soa.gz           -o /assets/berlin.helix.soa.gz \
 && ls -lh /assets

# q2 comes from the Railway build context (this repo, this branch)
COPY . /build/q2

# Copy the built Vite frontend into the cockpit/dist/ directory
# so include_dir! can embed it at compile time
COPY --from=frontend /build/dist/ /build/q2/cockpit/dist/

# Place the cached release assets into dist/. Local copy only — the download
# happened in the cached layer above, so this costs nothing on a code commit.
RUN cp /assets/body.soa.gz                     /build/q2/cockpit/dist/body.soa.gz \
 && cp /assets/body.20260629c.v6helix.soa.gz   /build/q2/cockpit/dist/body.20260629c.v6helix.soa.gz \
 && cp /assets/berlin.helix.soa.gz             /build/q2/cockpit/dist/berlin.helix.soa.gz \
 && ls -lh /build/q2/cockpit/dist/body.soa.gz \
           /build/q2/cockpit/dist/body.20260629c.v6helix.soa.gz \
           /build/q2/cockpit/dist/berlin.helix.soa.gz

# The Iceland DEM terrain bake (BSO2 ver 7 = F16 pos + Signed360 normal + per-vertex
# RGB texture): 16,515,072 verts, ESRI imagery draped as KIND × helix-residue colour.
# It ships IN-REPO under .claude/maps/, not from the release — this session type can't
# upload release assets, so the 151 MB artifact is version-controlled as a 2-part split
# zip (iceland_dem_16m.z01 + iceland_dem_16m.zip, each < GitHub's 100 MB blob limit).
# Reassemble both parts into one full zip, then extract iceland_dem.helix.soa.gz into
# dist/; include_dir! embeds cockpit/dist/, so BodyHelix resolves it SAME-ORIGIN
# (manifest iceland_latest = iceland_dem.helix.soa.gz). Same-origin is required for the
# same CORS reason as the body wires — the github releases redirect sends no CORS header.
# The build context already carries .claude/maps via `COPY . /build/q2` above.
RUN cd /build/q2/.claude/maps \
 && zip -s 0 iceland_dem_16m.zip --out /tmp/iceland_dem_full.zip \
 && unzip -o /tmp/iceland_dem_full.zip -d /build/q2/cockpit/dist/ \
 && rm -f /tmp/iceland_dem_full.zip \
 && ls -lh /build/q2/cockpit/dist/iceland_dem.helix.soa.gz

# (The Berlin OSM helix bake moved UP into the pre-COPY cached fetch layer with
# the two body wires — see the block above for why. BodyHelix still resolves it
# same-origin at /berlin.helix.soa.gz; without that, the fetch 404s and the
# fallback to the release URL is blocked by the CORS-less redirect.)

# Sibling deps — clone from GitHub
# graph-flow stub is local (crates/stubs/graph-flow), no rs-graph-llm needed
#
# lance-graph + ndarray are cloned at their BRANCH HEAD (latest) — NOT a pinned,
# stale SHA. The repos at their tips are mutually consistent, so "use the latest of
# everything" is the rule: a pinned-old lance-graph (36059ce0) is exactly what
# lacked `guid-v3-tail` and broke the build. The `COPY . /build/q2` above changes on
# every q2 commit, invalidating this RUN layer too, so each build re-clones fresh.
#
# ⚠ THE HOLE THAT LEAVES, measured 2026-08-14 — a REDEPLOY OF THE SAME q2 COMMIT
# REUSES STALE SIBLING CLONES. Docker busts this layer only when an input changes,
# and the sibling repos are not inputs — nothing here can observe that
# lance-graph's HEAD moved. So "re-clones fresh" holds per q2 COMMIT, never per
# DEPLOY, and this comment previously claimed the stronger thing.
#
# It cost a real outage. Merging q2 #129 and OGAR #268 in the SAME MINUTE started a
# build whose lance-graph clone had a codebook mirror one concept short of OGAR's,
# so `lance-graph-ogar`'s COUNT_FUSE panicked at const-eval (E0080) and the deploy
# died at compile — never reaching hydration. lance-graph #953 fixed main eight
# minutes later, but the REDEPLOY reused this cached layer and reproduced the exact
# same failure against a lance-graph that no longer had the bug.
#
# If a build fails on a sibling that is demonstrably green on main, this layer is
# the first suspect. Two ways out: push any q2 commit (busts COPY, busts this), or
# redeploy with the build cache disabled.
#
# THE FIX BELOW makes the sibling HEADs an actual layer input. Each `ADD` of a
# repo's `commits/main` fetches on every build and writes a file whose CONTENT is
# that repo's current commit; Docker hashes it, so the clone layer busts exactly
# when a sibling moves and stays cached when none did. Four small requests buy
# back the property this comment used to claim for free.
#
# Unauthenticated on purpose — the same URLs the `git clone` lines below already
# fetch without credentials, so these are reachable on the same terms.
#
# ⚠ The ATOM FEED, not `api.github.com`, and the difference is load-bearing.
# Unauthenticated api.github.com allows 60 requests/hour PER IP, and CI builders
# share egress IPs — so the API form would eventually 403, and a failed `ADD`
# FAILS THE BUILD. That trades a stale cache for a new outage cause, which is
# the opposite of the point. `github.com/<repo>/commits/main.atom` is served by
# the web frontend, is not under that quota, and its entries carry commit ids +
# commit dates, so its content changes exactly when HEAD moves.
#
# This does NOT replace the deeper choice — explicit SHA ARGs bumped
# deliberately, or removing the hand-maintained mirror via hotplug enumeration
# so the fuse's whole failure class disappears. Those are architectural calls,
# deliberately not made here. This one only restores "latest of everything" to
# being true per DEPLOY instead of per COMMIT.
ADD https://github.com/AdaWorldAPI/lance-graph/commits/main.atom /tmp/rev/lance-graph.atom
ADD https://github.com/AdaWorldAPI/ndarray/commits/main.atom /tmp/rev/ndarray.atom
ADD https://github.com/AdaWorldAPI/OGAR/commits/main.atom /tmp/rev/OGAR.atom
ADD https://github.com/AdaWorldAPI/openstreetmap-website-rs/commits/main.atom /tmp/rev/osm-website.atom
#
# Sibling checkouts the path deps (and the [patch] in q2's Cargo.toml) resolve against:
#   /build/lance-graph  → lance-graph @ main HEAD — carries guid-v2-tail +
#                         guid-v3-tail and the current ogar_codebook mirror.
#   /build/ndarray      → the REAL AdaWorldAPI/ndarray fork, consumed by BOTH
#                         lance-graph (../../../ndarray) AND q2-ndarray
#                         (../../../../ndarray). `--depth 1` WITHOUT
#                         --recurse-submodules: ndarray's workspace `exclude`s
#                         crates/burn, so the burn submodule (AdaWorldAPI/burn.git)
#                         is never needed — leaving it unfetched is correct.
#   /build/OGAR         → the OGAR fork. q2's Cargo.toml [patch]es the OGAR git
#                         source onto this clone, so ogar-vocab & friends are PATH
#                         deps — no git SHA in Cargo.lock, no pin. Same HEAD-tracking
#                         treatment as lance-graph + ndarray.
#
# COUNT_FUSE: lance-graph-ogar asserts (E0080 on mismatch)
# CODEBOOK.len() == ogar_vocab::class_ids::ALL.len(). With OGAR patched to the clone
# above, all three forks are path deps resolving to their current HEADs, so the mirror
# and the vocab move together — no stale git pin can wedge them apart. No pins anywhere.
#
# neo4j-rs is intentionally NOT cloned — a discarded Neo4j-GUI experiment referenced
# by no manifest; the only neo4j path is the opt-in `neo4j-fallback` (crates.io neo4rs).
#   /build/openstreetmap-website-rs
#                       → the OSM SoA bake crate (`osm-soa-bake`), which
#                         cockpit-server path-deps for `RowSlab` / `tms` /
#                         `identity` / `cluster` / `codebook` — the whole
#                         /api/osm/* surface. Cargo resolves the path dep
#                         BEFORE it builds anything, so a missing clone here is
#                         not a link error at the end but a manifest error at
#                         the start: "failed to read
#                         /build/openstreetmap-website-rs/Cargo.toml".
#                         It was missing from this list when the sibling dep
#                         was added, and the deploy failed on exactly that.
RUN git clone --depth 1 https://github.com/AdaWorldAPI/lance-graph.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/ndarray.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/OGAR.git \
 && git clone --depth 1 https://github.com/AdaWorldAPI/openstreetmap-website-rs.git

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

# `--start-period` is load-bearing, not decoration. On a COLD boot (empty
# volume) the OSM slab hydrates from S3 before the listener binds — measured
# ~60-90s for the 1.35 GB Berlin bake. Without a start period, Docker begins
# probing immediately and the default 3 retries at 30s intervals mark the
# container unhealthy inside ~90s, which overlaps the hydrate window exactly:
# the deploy would fail while doing precisely what it is supposed to do.
#
# Failures during the start period do not count, so this only widens the
# window for a legitimately slow FIRST start. It is not a mask for a hang:
# `ensure_slab_local` returns `None` fast on any error (no bucket, bad creds,
# missing checksum), so a long boot means a real transfer is in progress. A
# warm boot re-verifies the cached copy in ~0.8s and is unaffected.
#
# The cold path runs once per volume, not once per deploy — persisting across
# rebuilds is the volume's whole purpose.
HEALTHCHECK --interval=30s --timeout=3s --start-period=600s \
    CMD curl -f http://localhost:8080/health || exit 1

ENV PORT=8080
ENV AIWAR_DATA_PATH=/app/data/aiwar_graph.json
EXPOSE 8080
CMD ["./q2-cockpit"]
