# End-to-end `q2 render` performance profiling (2026-06-01)

## Overview

Profiled a full end-to-end `q2 render` of the experimental qmd-plans
website (`claude-notes/qmd-plans/`, branch
`experiment/claude-plans-q2-website`). This is a *pure*
parse → convert → write → resource-copy workload (no knitr/jupyter
execution), so it cleanly represents the core render pipeline.

**Branch confirmed current with PR #247** (`51a19ad4`,
`fix(pampa): detect parse errors without the per-lex tree-sitter
logger (bd-b7eb7)`). Only 2 commits behind `origin/main`, both
unrelated (CI Playwright caching, a `render --help` wording trim).

## Fixture & baseline

- 565 `.qmd` files, 9.4 MB of markdown → Cosmo-themed HTML website.
- **Baseline: ~4.6 s wall** (`user 4.04 + sys 0.56 ≈ real 4.56`),
  essentially single-threaded. ~8 ms/file.
- Profiled with samply at 1 kHz (`release-perf`), 5073 samples.

## Findings

### Self-time buckets (single-threaded profile)

| Bucket | ~% | Notes |
|---|---|---|
| Tree-sitter parsing | ~17% | `ts_parser__advance`, cursor iteration, `ts_lex` — inherent |
| `memmove` (AST construction) | ~11% | diffuse `String::clone` / `Vec::extend` in tree→Pandoc visitor + postprocess |
| Filesystem syscalls | ~13% | `open` 5%, `read` 2.6%, `mkdir` 2%, `stat` 1.9%, `rename` 0.8%, `getattrlist` 0.7% |
| Regex DFA compilation | ~5% | `thompson::compiler` / `Utf8State` — regexes recompiled (a bug) |
| Hashing | ~4% | SipHash (~3%) + SHA-256 (~1%) for cache keys |

### #247 fixed the locale lock (verified, not assumed)

| profile | locale-lock waits |
|---|---|
| Old May-31 MT profile (bd-b7eb7 pathology) | **77%** (`__ulock_wait2` 53% + `__ulock_wake` 20% + `os_unfair_lock` ~4%) |
| Current profile (post-#247) | **0.65%** |

The previous-session caution that "MT scaling is capped by the locale
lock" is **obsolete** — the trap is gone.

### The render is single-threaded because Pass 2 is a serial loop

Two-pass rayon architecture:

- **Pass 1** (parallel profile extraction):
  `perf.pass1 docs=565 threads_used=16 wall_ms=19` — fully parallel,
  ~0.4 % of total wall. The `quarto-pass1-*` worker threads.
- **Pass 2** (qmd→HTML render-to-file): a **serial `for` loop** at
  `crates/quarto-core/src/project/orchestrator.rs:1013`, awaiting each
  page sequentially on the main thread. **~98 % of the 4.6 s.**

Worker-thread CPU share in the profile: main `q2` thread 93 %, all 16
`quarto-pass1-*` workers combined 6.9 %.

Pass-2 parallelism was *anticipated*: `pass2_renderer.rs:73` documents
"a **future** rayon-per-worker parallelism path," the `Pass2Renderer`
trait is deliberately `?Send`, and the per-render args are already
`Arc`-wrapped. The one real shared-state obstacle is the
`&mut self.project_artifacts` accumulator (each page drains
project-scoped artifacts into it) — a *mergeable* accumulator.

## Work items

- [ ] **bd-XXXX (#1)**: hoist `whitespace_re` to a module-level static
      in `crates/pampa/src/pandoc/treesitter.rs:606` so `\s+` compiles
      once, not per inline-processing call. Land first — a parallel
      Pass 2 would otherwise have every worker recompiling it.
- [ ] **bd-YYYY (#2)**: parallelize the Pass-2 render loop
      (`orchestrator.rs:1013`) with rayon + a merge-at-end artifact
      accumulator. Highest upside; unblocked now that the locale lock
      is gone.
- [ ] (deferred) filesystem/cache I/O constant-factor cleanup —
      per-entry `create_dir_all` in `cache_set`, per-lookup opens in
      `cache_get`.

## Experiment results (bd-2ercw regex fix)

Native driver: `crates/perf-harness/src/bin/parse_corpus.rs`
(`parse-corpus <dir> <threads> <iterations>`) parses every corpus file
through `quarto_core::pipeline::parse_qmd_to_ast` (the `native_visitor`
path), spreading files across N OS threads.

Buggy vs fixed binaries differ only in the `whitespace_re` hoist (both
keep the `WHITESPACE_RE_COMPILE_COUNT` counter). Buggy compiles `\s+`
**20,806 times** over the 565-file corpus; fixed compiles it **once**.

### Isolated parse path (median of 5 runs, release)

| variant | 1 thread | 16 threads |
|---|---|---|
| buggy | 2821.9 ms | 391.6 ms |
| fixed | 2496.0 ms | 352.7 ms |
| **delta** | **−326 ms (−11.5%)** | **−39 ms (−9.9%)** |

The parse path parallelizes **7.1×** on 16 threads (2496 → 352.7,
fixed) with no lock contention — empirical confirmation that bd-3gj56
(Pass-2 parallelization) is unblocked post-#247.

### End-to-end `q2 render` (median of 5, interleaved, release)

| variant | wall |
|---|---|
| buggy | 4720 ms |
| fixed | 3540 ms |
| **delta** | **−1180 ms (~−24%)** |

End-to-end beats the isolated-parse delta because a full render parses
each file **twice** (Pass 1 profile extraction + Pass 2 render), so the
per-parse regex cost is paid twice: 11.5% × 2 ≈ 23% ≈ measured 24%.

### Profile corroboration (samply, 1 kHz, release-perf)

| frame group | buggy | fixed |
|---|---|---|
| regex compile (`thompson`/`Utf8State`/`determinize`/…) | 14.27% | 0.02% |
| `_platform_memmove` | 11.31% | 12.54%* |
| malloc + drop | 12.99% | 14.42%* |
| **total CPU samples** | **5073** | **4212 (−17%)** |

\* memmove/malloc *percentages* rise only because the denominator
shrank; their absolute cost is ~flat, confirming the 14.3% regex-compile
was the genuinely removable work. (Note: my first-pass assessment
under-quoted this as "~5%" — that was only the `determinize` self-time
line; the full NFA/DFA compiler sums to ~14%.)

## Status

- [x] **bd-2ercw (#1)**: `whitespace_re` hoisted to module-level static
      `WHITESPACE_RE` in `crates/pampa/src/pandoc/treesitter.rs`.
      Regression test `test_whitespace_re_compile_once` asserts the
      `\s+` regex compiles ≤ 1× for the process. All 9496 workspace
      tests pass.
- [ ] **bd-3gj56 (#2)**: parallelize Pass 2 — *pending user decision.*
