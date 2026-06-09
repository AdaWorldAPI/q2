# Perf: WASM render re-flushes ALL artifacts into the VFS on every render (bd-q3bxnq2e)

**Date:** 2026-06-09
**Beads:** bd-q3bxnq2e
**Worktree:** main checkout (branch `main`, based on `main` @ `ade34bed`)
**Status:** Investigation — pending design alignment with user. **Do not start implementation until the user gives the go-ahead.**

## Triage verdict

**Ready to design.** The code matches the strand's description, the blast-radius
question (Automerge propagation) is now answered — artifacts are purely
in-memory-ephemeral — and the fix direction is clear and small. The main open
work is quantification (per the performance-profiling playbook, measure before
fixing) and a handful of scoping decisions listed below.

## Issue context

Filed 2026-06-09 (same day, fresh) by Carlos, priority 1, type task, label
`revealjs`. Discovered while scoping the embed-iframe preview work
(bd-z1smhvuo). Claim: the WASM render entry point re-flushes the **entire**
artifact set into the VFS on **every** render with no change detection —
`artifact.content.clone()` + HashMap insert per artifact per render — so every
edit-triggered re-render pays CPU/alloc cost proportional to *total artifact
bytes*, not *changed bytes*. Strand asks to (1) quantify, (2) pin down whether
the writes propagate into Automerge, (3) design a change-detection fix,
(4) preserve the bd-3gtn empty-content skip and the iframe post-processor's
read-back contract.

## Dependency graph

- **discovered-from: bd-z1smhvuo** (Embed mechanism for
  `.embed-example-iframe` doc placeholders, in_progress). Phases 1–2 of that
  feature landed (commits `de20ca96`, `867aa7c1`); its remaining item is
  verifying `q2 preview` serves staged static assets via the VFS. This strand
  surfaced while reading that VFS path. The `revealjs` label is inherited from
  that context — **reveal.js itself is not implicated** (its assets are
  inlined into the HTML via `include_str!`, see `crates/quarto-core/src/revealjs/assemble.rs:26-32`;
  they never become artifacts).
- No blockers, no dependents. Fresh strand, no staleness concerns.

## What the code looks like today

All paths verified at `main` @ `ade34bed`. Pre-flight
`cargo xtask verify --skip-hub-build` is green.

### Three flush sites, all unconditional

1. **Single-doc render tail** — `crates/wasm-quarto-hub-client/src/lib.rs:1417-1425`
   (exactly as the strand quotes). Used by `render_qmd` /
   `render_qmd_content` and by `render_page_in_project`'s no-project
   fall-through. Flushes **all** of `ctx.artifacts` (page- and project-scope;
   the single-doc path never drains): one `content.clone()` + insert per
   artifact per render.

2. **Project render, page-scoped artifacts** —
   `crates/wasm-quarto-hub-client/src/lib.rs:1626-1643`. Same loop over
   `active_output.page_artifacts` (engine figures, resource copies for the
   active page).

3. **Project render, project-scoped artifacts** —
   `WebsiteProjectType::post_render` → `flush_site_libs`
   (`crates/quarto-core/src/project/website_post_render.rs:81-109`), which runs on
   **every** `ProjectPipeline::run` (orchestrator.rs: Pass 1 → pre_render →
   Pass 2 → post_render — i.e., every preview render). Each artifact is cloned
   **twice**: `sink.write(on_disk, artifact.content.clone())` into the
   `OutputSink`, then `sink.flush(runtime)` → `WasmRuntime::file_write` →
   `contents.to_vec()` (`crates/quarto-system-runtime/src/wasm.rs:596-598`).

   (The `RenderToHtmlRenderer` default-project branch at
   `pass2_renderer.rs:826-838` routes through the same `flush_site_libs`.)

### Upstream of the flush: producers also rebuild the bytes per render

The flush is the *second* per-render copy of mostly-static bytes. The
producers re-store them into `ctx.artifacts` every render:

| Artifact | Producer | Size | Per-render source |
| --- | --- | ---: | --- |
| Theme CSS `quarto/quarto-theme-<fp>.css` | `compile_theme_css.rs` | ~200–400 KB (Bootstrap-based) | SASS LRU cache hit → clone of cached bytes (compile itself **is** cached) |
| `bootstrap.bundle.min.js` | `bootstrap_js.rs:77` | 81 KB | `include_bytes!` static → `.to_vec()` |
| bootstrap-icons CSS | `website_bootstrap_icons.rs:37` | 99 KB | `include_bytes!` static → `.to_vec()` |
| bootstrap-icons woff | `website_bootstrap_icons.rs:41` | 180 KB | `include_bytes!` static → `.to_vec()` |
| clipboard JS ×2 | `clipboard_js.rs:71,81` | ~10 KB | `include_bytes!` static → `.to_vec()` |
| listing JS/CSS | `listing_render.rs:153-159` | small | per render |
| plot images, resource copies | engines / `ResourceCollectorTransform` | unbounded | page-scoped |

So a theme-heavy website page costs roughly **0.6–1 MB of byte cloning ×2–3
copies per keystroke render**, plus HashMap/PathBuf churn, regardless of
whether anything changed. On top of that, the TS side re-reads and re-hashes
CSS from the VFS per render for `cssVersion`
(`ts-packages/preview-runtime/src/wasmRenderer.ts:1091-1099`).

### Strand question 2 — Automerge blast radius: ANSWERED, benign

The VFS writes are **purely in-memory-ephemeral**. Evidence (Explore-agent
sweep, 2026-06-09):

- Sync is strictly one-way Automerge → VFS
  (`ts-packages/preview-runtime/src/automergeSync.ts:88-108`: `onFileAdded` /
  `onFileChanged` / `onBinaryChanged` / `onFileRemoved` all call `vfsAddFile`-family;
  no reverse callback exists).
- `WasmRuntime`'s VFS is a `HashMap<PathBuf, Vec<u8>>` behind an `RwLock`
  (`crates/quarto-system-runtime/src/wasm.rs:229`), no persistence hooks.
- The only VFS artifact read-backs are the iframe post-processor
  (`hub-client/src/components/render/ReactAstSlideRenderer.tsx:770-775`, →
  data: URIs) and the `cssVersion` hash above. Neither writes to Automerge or
  IndexedDB. IndexedDB holds only project metadata, session state, and the
  Pass-1 profile cache.

So the cost is **CPU/alloc only** — no document growth, no sync traffic. This
caps the severity: the "much worse" branch of the strand did not materialize.

### Severity caveat (why we measure before fixing)

Memcpy of ~1–2 MB is sub-millisecond native and low-single-digit ms in WASM.
That is real per-keystroke waste but plausibly **not** the dominant preview
cost (each render also re-runs the whole project pipeline). Per
`claude-notes/instructions/performance-profiling.md`, Phase 1 quantifies with
a scaled fixture *before* the fix is designed in detail; if the flush turns
out to be <~5% of per-render time, we report that honestly and still decide
(question 1 below) whether the cheap fix is worth landing.

## Proposed fix direction (draft, pending measurements)

**Skip-if-byte-equal at the VFS boundary.** Add a compare-before-clone write,
e.g. `VirtualFileSystem::add_file_if_changed(&Path, &[u8]) -> bool` (and a
`WasmRuntime` passthrough), then route all three flush sites through it:

- Equal content → memcmp only (cheap, no alloc, no insert churn).
- Changed/new content → clone + insert exactly as today.

Properties: generic across artifact types (fingerprinted or not), no new
state to invalidate, preserves the bd-3gtn empty-content skip untouched, and
trivially preserves the iframe read-back contract (bytes at the path are
always present and current — we only skip writes that would be byte-identical
no-ops). The theme artifact's fingerprinted filename would make an O(1)
presence check possible, but it covers only one artifact; byte-compare covers
all of them at negligible extra cost.

Alternatives considered and deprioritized:
- **Flush-once-per-session epochs** for project-scope artifacts: artifacts
  *can* legitimately change mid-session (`_quarto.yml` theme edit), so this
  still needs change detection — it collapses into the same mechanism with
  more bookkeeping.
- **Content-hash registry**: avoids O(n) compare but adds state and hashing;
  memcmp on equal bytes is already ~as fast as hashing one side.

## Proposed phases (draft)

Skeleton only — contents wait on the design discussion.

- **Phase 0 — Quantify (playbook steps 1–4).**
  - [x] Un-gate `VirtualFileSystem` for native builds (decision 2): moved to
        target-agnostic `quarto-system-runtime/src/vfs.rs`; `WasmRuntime`
        stays wasm-only. The 7 `test_vfs_*` unit tests now run natively
        (they were dead under the wasm gate). Native + wasm32 both compile
        clean; workspace tests green.
  - [x] `perf-harness` driver `vfs-flush` (`crates/perf-harness/src/bin/vfs_flush.rs`):
        renders the committed theme-heavy fixture
        (`claude-notes/plans/wasm-vfs-artifact-reflush-investigation/theme-heavy.qmd`)
        in a loop against a session-persistent `VirtualFileSystem`,
        mirroring the wasm `render_qmd` tail byte-for-byte (incl. the
        bd-3gtn skip); `pad_bytes` arg scales total artifact bytes.
        Functional check: every iteration re-flushes 4 artifacts /
        400,692 B; flush ≈10 µs vs render ≈50 ms native (PRELIMINARY —
        busy machine, not the recorded numbers).
  - [x] Instrumentation (QUARTO_PERF_STATS=1, playbook conventions):
        gauge `perf.vfs-write` — counters on `VirtualFileSystem`
        (writes/bytes_written/skipped_writes/bytes_skipped; skip counters
        wired now, stay zero until Phase 2, so before/after share one
        format; `write_stats()` accessor for per-render diffing) — and
        gauge `perf.artifact-store` — producer-side counters on
        `ArtifactStore::store()` (stores/bytes_stored; drain/merge moves
        not counted) so bd-w5qyuzeg inherits real numbers. Smoke-tested:
        one themed render stores 4 artifacts / 400,692 bytes
        (`perf.artifact-store stores=4 bytes_stored=400692`).
  - [ ] Geometric scaling of total artifact bytes via `pad_bytes`; record
        Findings table in this plan. **Deferred to a quiet-machine session**
        (user note 2026-06-09: parallel agents make timings unreliable);
        will run as a single before/after session once Phase 2's
        `--mode` flag exists. Fix direction is already settled by
        decision 1 (land the skip regardless), so Phases 1–2 proceed
        meanwhile.
- **Phase 1 — Test plan (TDD).**
  - [ ] Unit tests for `add_file_if_changed` semantics (new / changed /
        identical / empty).
  - [ ] Behavioral test: two consecutive renders of an unchanged doc → second
        flush reports `bytes_skipped == bytes_total` (counter-observable);
        changed theme → theme artifact rewritten.
  - [ ] Regression guard: iframe read-back path still finds artifacts after a
        skipped flush.
- **Phase 2 — Implement.**
  - [ ] `add_file_if_changed` in `quarto-system-runtime`.
  - [ ] Switch flush sites 1 and 2 (`wasm-quarto-hub-client/src/lib.rs`).
  - [ ] Switch site 3 (`flush_site_libs` / `OutputSink`) per question 3's
        scoping decision.
- **Phase 3 — Verify.**
  - [ ] Native before/after numbers across scales (complexity-class table).
  - [ ] Full `cargo xtask verify` (WASM leg affected).
  - [ ] Browser cross-check per playbook step 8 (sanity check only); record
        the end-to-end example here.

## Design decisions (settled with user, 2026-06-09)

1. **Measure-then-fix gate: land the skip regardless of measured share**,
   with the measured share stated honestly in Findings — *unless* the fix
   turns out to require an architectural change that could paint us into a
   future corner. (`add_file_if_changed` as drafted is not architectural; if
   Phase 0 pushes us toward something bigger, stop and re-align.)
2. **Native proxy: yes**, un-gate `VirtualFileSystem` (not `WasmRuntime`) for
   native builds so the perf-harness driver exercises the actual flush code.
   Framing note from the user: the perf concern is **entirely hub-client
   per-keystroke latency feel** — native builds are ~40–50× faster than
   Quarto 1 and not a worry. The native proxy exists to measure/iterate, not
   because native has a problem to fix.
3. **Scope of site 3: change detection only in the in-memory VFS layer**;
   native disk writes unchanged. User noted a slight preference for single
   code paths but accepts this as one of the unavoidable native-vs-wasm cfg
   splits.
4. **Producer-side clones: out of scope, filed as bd-w5qyuzeg**
   (discovered-from this strand). This strand stays flush-only. The
   `Artifact.content: Vec<u8>` → `Cow<'static, [u8]>` / `Arc<[u8]>` /
   `bytes::Bytes` refactor is gated on Phase 0's data: the instrumentation
   here will measure producer-clone cost alongside flush cost, and
   bd-w5qyuzeg is picked up only if the residual copy is a meaningful share
   of per-keystroke latency.
5. **TS-side `cssVersion`: filed as bd-rrnn3se8** (discovered-from this
   strand) — switch `cssVersion` to `RenderResponse.theme_fingerprint`,
   delete the per-render VFS read + hash.

All five design questions are now settled. Next step: Phase 0
(instrumentation + native proxy + measurements), on user go-ahead.

## Risks / tradeoffs (draft)

- **Phase-9 VFS contract** (`crates/wasm-quarto-hub-client/CLAUDE.md`): the VFS
  is load-bearing across renders; skipping byte-identical writes is contract-
  preserving by construction, but any test that asserts "write happened"
  (rather than "bytes present") could need updating.
- **bd-3gtn**: the empty-content skip must stay ahead of the new compare —
  empty content means "manifest entry, never write", not "write empty bytes".
- **`OutputSink` allowed-roots validation** (bd-cfl67) runs at `sink.write`
  time; if site 3 short-circuits before the sink, we must not lose the
  validation for the artifacts that *do* get written.
- The flush may turn out to be a minor contributor to the observed preview
  slowness — in that case the bigger fish (full pipeline re-run per keystroke)
  belongs in a separate strand; this plan deliberately does not grow to cover it.
