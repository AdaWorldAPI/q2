# Step 1: extract the diagnostics foundation into two standalone `posit-dev/` repos — `quarto-source-map` first, then `quarto-error-reporting`

> **Naming (decided 2026-06-27):** both externalized crates **keep their current
> names** — `quarto-source-map` and `quarto-error-reporting` (and `quarto-yaml`
> later). No `error-reporting-core` rename. We rename only "if it comes to it."
> Consequence: there is **no q2-side façade** (the name belongs to the external
> crate); q2 depends on the external `quarto-error-reporting` directly. The only
> crate carved out q2-side is `quarto-error-catalog` (the `Q-*` policy). `json.rs` +
> `coalesce.rs` stay in the external crate behind a default-off `json` feature
> (see "The split").

**Strand:** bd-egcyeym9
**Date:** 2026-06-26
**Status:** Plan (not started)
**Design contract:** `claude-notes/designs/cross-package-error-codes.md`
**Sibling plan (gated behind this):**
  `claude-notes/plans/2026-06-26-extract-quarto-yaml-validation-design.md`

This is **step 1 of 3** in the agreed sequence (foundation → YAML stack → q2
migration). It extracts and publishes the diagnostics *host* — the crate that
defines the cross-package error-code discipline — and proves it standalone
*before* any client (yaml-validation) adopts it.

## Scope

**In scope:** extract `quarto-source-map` into its own `posit-dev/` repo and
publish it (leaf-first); make `quarto-error-reporting` catalog-agnostic by carving
its `Q-*` catalog data out into a new q2-side `quarto-error-catalog` policy crate;
extract the (now catalog-agnostic) `quarto-error-reporting` into a **separate**
`posit-dev/` repo (depending on the now-published `quarto-source-map`) and publish
it; cut q2 over to the published crates, incrementally (source-map first).

**Two repos, not one** (decided 2026-06-27 — see Decisions §1). The crates split
naturally: `quarto-source-map` is a general source-location leaf with 14 in-repo
dependents and no relationship to diagnostics; `quarto-error-reporting` is the
diagnostics host. Independent repos = independent release cadences.

**Explicitly NOT in scope:** `quarto-yaml` / `quarto-yaml-validation` (step 2),
deleting `validate-yaml` (step 2), wiring yaml validation into q2 config, the
`Q-1-x` remap (step 2/3). This plan touches **no YAML code**.

## Grounding facts (measured 2026-06-26)

- **The catalog coupling inside `error-reporting` is one line.**
  `crates/quarto-error-reporting/src/diagnostic.rs:290` —
  `DiagnosticMessage::docs_url()` calls `crate::catalog::get_docs_url(code)`. That
  is the *entire* hard seam between the renderer and the Quarto catalog.
- **Only two external q2 crates call the catalog free-functions directly:**
  `quarto-core/src/project_resources.rs` and `quarto-core/src/theme_diagnostic.rs`.
  Everything else goes through `DiagnosticMessage`.
- **Direct Cargo dependents:** `quarto-source-map` 14, `quarto-error-reporting` 9.
  (The research doc's 26/19 counted transitive reach.)
- **Module sizes** (`crates/quarto-error-reporting/src/`): `diagnostic.rs` 1187,
  `builder.rs` 595, `coalesce.rs` 461, `json.rs` 480, `catalog.rs` 317,
  `macros.rs` 98, `lib.rs` 87.
- **`quarto-source-map`** is a true leaf: deps `serde`, `serde_json`, `smallvec`;
  no `description` field (must add one to publish).
- **The dependency between the two is strictly one-directional:**
  `quarto-error-reporting → quarto-source-map`, pervasively (`SourceInfo` is a field
  on `DiagnosticMessage` at `diagnostic.rs:126`; `SourceContext` threads through
  rendering and the builder). `quarto-source-map` depends on *nothing* in
  error-reporting. **This forces leaf-first extraction order:** source-map must be
  published before `quarto-error-reporting` can be (crates.io rejects unpublished
  path deps).
- **Workspace metadata** all crates inherit (`[workspace.package]`): `version =
  "0.7.0"`, `repository = posit-dev/quarto-markdown-syntax`, `license = MIT`,
  `edition = 2024`. The externalized crates must stop inheriting q2's `version`
  and `repository` (they get their own in the new repo).
- Neither crate has ever been published to crates.io (no public version line yet).

## The split: what goes where

Only **one** thing actually leaves `quarto-error-reporting`: the `Q-*` catalog
*data*. Everything else stays in the (now catalog-agnostic) crate, so consumers'
imports are unchanged.

```
quarto-error-reporting  (EXTERNAL — keeps its name)   quarto-error-catalog (q2 — NEW)
  diagnostic.rs   DiagnosticMessage, render             error_catalog.json (Q-* data)
  builder.rs      DiagnosticMessageBuilder              ERROR_CATALOG static (moved here)
  macros.rs       convenience macros                    QuartoCatalog: CatalogProvider
  ErrorCodeInfo   (struct, moved from catalog.rs)       install() at startup
  CatalogProvider trait + OnceLock registry             (the Q-* policy + audit live here)
  get_docs_url/get_error_info/get_subsystem
    (now DELEGATE to the installed provider)
  json.rs    behind default-off `json` feature
  coalesce.rs (render-summary helper)
  deps: url, ariadne, thiserror, serde(_json);
        schemars ONLY under `json` feature
  NO error_catalog.json / ERROR_CATALOG static

quarto-source-map  (EXTERNAL — unchanged surface)
  leaf; serde/serde_json/smallvec
```

**Rationale for each boundary** (full version in the design note):

- `ErrorCodeInfo` (the *struct*) stays, because the `CatalogProvider` trait returns
  it. Only `ERROR_CATALOG` (the *static* + `error_catalog.json` loader) leaves →
  `quarto-error-catalog`, because the *data* is Quarto policy.
- `get_docs_url`/`get_error_info`/`get_subsystem` stay, reimplemented to delegate to
  the installed provider (was: read the static map). The two `quarto-core` callers
  and the `lib.rs` re-export are therefore **source-unchanged** (they still resolve
  against `quarto-error-reporting`). The one symbol that *does* move is the
  `ERROR_CATALOG` static re-export — audit for direct importers (the free functions
  cover almost all uses).
- `json.rs` + `coalesce.rs` **stay** in `quarto-error-reporting`, with `json` behind
  a **default-off `json` feature** so a non-Quarto consumer pulls neither the
  `schemars` dep nor the wire format. q2 enables `features = ["json"]`. This keeps
  every `use quarto_error_reporting::json::…` / `::coalesce::…` consumer
  source-unchanged. (Reverses the earlier Q4/Q5 "move json q2-side" — the
  feature-gate recovers the clean-build benefit with far less churn. Residual:
  the `quarto.org` `SCHEMA_URL` const ships behind the feature; revisit only if a
  non-Quarto user needs the wire format under a different scheme.)
- `quarto-source-map` moves with **no surface change** — only its manifest changes.

## The host seam (canonical definition — supersedes the sketch in the sibling plan)

```rust
// quarto-error-reporting (external, catalog-agnostic)
pub struct ErrorCodeInfo {                 // moved verbatim from catalog.rs
    pub subsystem: String,
    pub title: String,
    pub message_template: String,
    pub docs_url: Option<String>,
    pub since_version: String,
}

pub trait CatalogProvider: Send + Sync {
    fn lookup(&self, code: &str) -> Option<&ErrorCodeInfo>;
}

struct EmptyCatalog;                        // default: catalog-agnostic, standalone-usable
impl CatalogProvider for EmptyCatalog {
    fn lookup(&self, _: &str) -> Option<&ErrorCodeInfo> { None }
}

static CATALOG: std::sync::OnceLock<Box<dyn CatalogProvider>> = std::sync::OnceLock::new();

/// Embedders call once at startup; first write wins, later writes are no-ops.
pub fn install_catalog(p: Box<dyn CatalogProvider>) { let _ = CATALOG.set(p); }

fn catalog() -> &'static dyn CatalogProvider {
    CATALOG.get().map(|b| b.as_ref()).unwrap_or(&EmptyCatalog)
}

pub fn get_docs_url(code: &str) -> Option<&'static str> { /* via catalog() */ }
// get_error_info / get_subsystem likewise delegate.
```

```rust
// quarto-error-catalog (q2)
struct QuartoCatalog(std::collections::HashMap<String, ErrorCodeInfo>);
impl CatalogProvider for QuartoCatalog { /* HashMap::get */ }
pub fn install() {
    quarto_error_reporting::install_catalog(Box::new(QuartoCatalog::load_embedded()));
}
```

`std::sync::OnceLock` (not `once_cell`) so core needs no extra dep for the
registry. Init point: q2 binary `main` + the WASM bootstrap + a test helper call
`quarto_error_catalog::install()` before the first diagnostic renders. Double
install is harmless (first-write-wins).

The three phases below. **Phase 1** (extract source-map) and **Phase 2** (make
error-reporting catalog-agnostic in place) are independent and may overlap;
**Phase 3** (extract `quarto-error-reporting`) requires both — the carve-out done
*and* source-map published.

## Phase 1 — Extract `quarto-source-map` into its own repo (the leaf-first warmup)

The simplest possible extraction (a clean leaf, no split, no policy). Doing it
first proves the whole pipeline — `posit-dev/` repo setup, crates.io publish, and
the **WASM cutover** — on the easiest crate, before the harder error-reporting work.

- [x] **1a.** Created `posit-dev/quarto-source-map` (public,
      https://github.com/posit-dev/quarto-source-map): copied the 8 sources +
      `LICENSE`, wrote a standalone single-crate manifest (`version = "0.1.0"`,
      `edition = "2024"`, `description`, `repository`, `keywords`, `categories`,
      pinned dep versions serde 1.0.228 +rc / serde_json 1.0.149 / smallvec 1.13
      +serde), added `README.md` + `.gitignore`. Builds on **stable** rustc 1.95
      (no nightly needed).
- [x] **1b.** Local validation green: `cargo build`, `cargo test` (104 unit + 4
      doctests pass), `cargo publish --dry-run` clean. *(Gap: no GitHub Actions CI
      workflow committed to the new repo yet — tests were run locally. Add one.)*
- [x] **1c.** Published `quarto-source-map 0.1.0` to crates.io (Carlos's personal
      account; `cargo owner --add github:posit-dev:<team>` deferred — weekend).
- [x] **1d.** q2 cutover (branch `braid/bd-egcyeym9-source-map-extraction`):
      flipped `[workspace.dependencies.quarto-source-map]` `path` → `version =
      "0.1.0"`; consolidated the 13 *main-workspace* members onto
      `{ workspace = true }`; deleted the in-tree `crates/quarto-source-map`.
      **Gotcha:** `wasm-quarto-hub-client` is *excluded* from the main workspace
      and is its own standalone workspace (refs every q2 crate by `path`), so it
      can't inherit a workspace dep — it gets a **direct** `quarto-source-map =
      "0.1.0"`. (A blanket `crates/*/Cargo.toml` rewrite broke it first; the WASM
      build caught it — and a `| tail` had masked the first verify's real exit
      code. Lesson: don't pipe `cargo xtask verify` through `tail`.) Final state:
      `cargo build --workspace` ✅, `cargo nextest run --workspace` 10238 ✅,
      **full `cargo xtask verify` (all 14 steps, incl. WASM build + hub tests)
      ✅**. Both root and wasm-crate `Cargo.lock` resolve from the crates.io
      registry with matching checksum. *(Not yet committed — awaiting user
      go-ahead per GIT PUSH POLICY.)*

**Phase 1 is functionally complete** (crate published + q2 cut over + full verify
green). Only the new repo's CI workflow (1b gap) and the deferred crates.io
owner-add remain as tidy-ups.

## Phase 2 — Make `quarto-error-reporting` catalog-agnostic, in place (TDD; independent of Phase 1)

Goal: carve the `Q-*` catalog *data* out into `quarto-error-catalog` and route the
renderer through the installed provider, proving it compiles and is
behaviour-preserving *before* any code leaves the repo. This is the bulk of the
engineering and is valuable even if the repo move slipped. Does **not** depend on
Phase 1. There is **no façade** — `quarto-error-reporting` keeps its name and its
public surface (minus the moved catalog *data*).

- [ ] **2a (test first).** Pin the behaviour the carve-out must preserve, against
      the *current* code:
      - `get_docs_url("Q-0-1")` returns the quarto.org URL (with the q2 catalog
        active);
      - a new test asserting that with **no** catalog installed, `get_docs_url`
        returns `None` (passes only after the registry exists — write it now,
        expect red).
- [ ] **2b.** Introduce `CatalogProvider` + the `OnceLock` registry + delegating
      free-functions in `quarto-error-reporting`; keep `ErrorCodeInfo` here.
      Repoint `diagnostic.rs:290` at `catalog()`. Green.
- [ ] **2c.** Create `quarto-error-catalog` crate: move `error_catalog.json` + the
      `ERROR_CATALOG` static + the `QuartoCatalog` provider + `install()` there.
      Move the catalog-data tests with it. Wire `install()` into q2 binary/WASM/test
      entry points.
- [ ] **2d.** Put `json.rs` behind a default-off `json` feature in
      `quarto-error-reporting`; have q2's dependents that use it enable
      `features = ["json"]`. Confirm the 9 dependents + the 2 `quarto-core` catalog
      callers + all `json`/`coalesce` consumers compile **unchanged** (only feature
      flags and the `ERROR_CATALOG` import path may move).
- [ ] **2e.** Audit manifests: `schemars` becomes `json`-feature-only; drop
      `once_cell` if the registry's `OnceLock` makes it unused; `url` stays.
- [ ] **2f.** `cargo xtask verify` green (touches `quarto-core` → hub-client/WASM;
      do NOT `--skip-hub-build`).

> At the end of Phase 2, q2 still builds `quarto-error-reporting` from a path dep;
> it is now catalog-agnostic and cleanly carve-able.

## Phase 3 — Extract `quarto-error-reporting` into a *separate* repo and cut q2 over

Requires Phase 1 (source-map published) **and** Phase 2 (carve-out done).

- [ ] **3a.** Create `posit-dev/<error-reporting-repo>`; **copy** the
      `quarto-error-reporting` sources (fresh `git init`, no history). Own
      `[workspace.package]`. Crucially, its `quarto-source-map` dependency is now
      the **published version** dep (Phase 1c), *not* a path dep. (`json.rs` +
      `coalesce.rs` travel with it, `json` behind its feature; the catalog data
      already left in Phase 2c.)
- [ ] **3b.** Standalone CI: `cargo build` + `cargo nextest run` with the
      **`EmptyCatalog`** default — proving catalog-agnostic operation with zero
      Quarto policy present.
- [ ] **3c.** External-consumer smoke test: a throwaway crate builds a
      `DiagnosticMessage`, installs a trivial `CatalogProvider`, renders — proving
      the published API is usable with no Quarto context.
- [ ] **3d.** Publish `quarto-error-reporting` to crates.io.
- [ ] **3e.** q2 cutover: replace the in-tree `quarto-error-reporting` path dep with
      the published version dep (enable `features = ["json"]`); delete the in-tree
      copy. `quarto-error-catalog` stays in q2 and now depends on the external
      `quarto-error-reporting`. **Full `cargo xtask verify` incl. hub-build** (WASM
      risk surface again, now de-risked by Phase 1d).
- [ ] **3f.** Update `CLAUDE.md`'s crate-layout section + the workspace member list.

## Decisions

1. **Repo granularity — DECIDED (2026-06-27): two repos**, one per crate. The
   crates split naturally (leaf source-location utility vs. diagnostics host) and
   already have independent dependent sets (14 vs. 9); co-locating buys only weak
   CI cohesion and needlessly couples their release cadences. (The YAML stack is a
   third, separate repo in step 2.)

2. **Crate names — DECIDED (2026-06-27): keep current names.** Both externalized
   crates stay `quarto-source-map` and `quarto-error-reporting` (and `quarto-yaml`
   later). No rename now ("if it comes to it"). Consequence: no q2-side façade
   crate; the q2-only carve-out is `quarto-error-catalog`; `json`/`coalesce` stay in
   the external crate (`json` feature-gated). See the header note + "The split".

Open forks (settle before 1a / 3a):

3. **Version start** (each repo independently). Fresh `0.1.0` public line (honest —
   never published), vs. continue `0.7.0`. Recommend `0.1.0`.
4. **Distribution channel.** crates.io (recommended — that is the whole point for
   non-Quarto users) vs. git deps as an interim before the first publish.
5. **Repo names** for the two `posit-dev/…` repos (the source-map repo and the
   error-reporting repo — the *crate* names are fixed; the *repo* slugs are not).

## Risks

- **WASM cutover (1d, 3e).** Each external crate must build for
  `wasm32-unknown-unknown` (both should — pure Rust, no `std::fs`), and the
  async-trait/`?Send` rules in `.claude/rules/wasm.md` apply if any trait is
  touched. The `CatalogProvider` registry uses `OnceLock<Box<dyn CatalogProvider>>`;
  the `Send + Sync` bound is fine natively and irrelevant single-threaded in WASM,
  but confirm it compiles for the target. **Verify with the full
  `cargo xtask verify`, not just `cargo build`.** Phase 1d does this on the trivial
  leaf first, so 3e is already de-risked.
- **Install-ordering.** A diagnostic rendered before `install()` would silently use
  `EmptyCatalog` (no titles/URLs). Mitigation: install at the earliest entry
  points; a debug-only assertion or a test that renders a known `Q-*` code and
  checks its URL guards against a missing install.
- **Two copies during extract→cutover.** Each crate exists in both repos between
  publish and the q2 cutover that deletes the in-tree copy (1c→1d, 3d→3e). Keep the
  window short; pin the q2 dep to the exact published version.

## Test plan (TDD gates)

- *Behaviour-preservation (Phase 2):* the 2a tests — installed catalog reproduces
  today's `docs_url`; empty catalog returns `None`.
- *Catalog-agnostic (Phase 3):* `quarto-error-reporting`'s own test suite passes
  with `EmptyCatalog` and **no** dependency on any `Q-*` data.
- *External-consumer smoke (3c):* a throwaway crate builds a `DiagnosticMessage`,
  installs a trivial `CatalogProvider`, and renders — proving the published API is
  usable with zero Quarto context.
- *Full regression (1d, 3e):* `cargo xtask verify` (incl. hub-build) green with q2
  consuming each published crate in turn.
