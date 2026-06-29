# Handoff: extract the YAML stack (`quarto-yaml` + `quarto-yaml-validation`)

**Strand:** bd-egcyeym9 (final phase of the diagnostics/YAML extraction epic)
**Date:** 2026-06-29
**Audience:** an agent picking this up cold. You should not need to read the
session transcript — everything needed is here or in the linked docs.

---

## 1. Goal & motivation

Extract `quarto-yaml` and `quarto-yaml-validation` out of the q2 monorepo into a
**single new repository `posit-dev/quarto-yaml`, structured as a Rust workspace
with two crates**, and publish both to crates.io independently. The motivating
need: **invisible internal Posit consumers of `quarto-yaml-validation`** want a
standalone crate; and the error codes must follow the cross-package discipline.

This is the **last phase** of the epic. The diagnostics foundation is already
done and merged:
- `quarto-source-map 0.1.0` → `posit-dev/quarto-source-map` (crates.io) — PR #348.
- `quarto-error-reporting 0.1.0` → `posit-dev/quarto-error-reporting` (crates.io,
  catalog-agnostic, `json` feature-gated) — PRs #349 (carve-out) + #350 (cutover).

## 2. Preconditions (verify before starting)

- **PR #350 merged** (q2 consumes `quarto-error-reporting 0.1.0`). `git switch
  main && git pull`. Confirm `crates/quarto-error-reporting/` is **gone** and the
  workspace builds.
- Both foundation crates are live on crates.io at `0.1.0`.
- You have (or the user grants) the same setup used in Phases 1/3: GitHub `gh`
  auth with `posit-dev` org rights (SSH key SSO-authorized), and the **user**
  performs every `cargo publish` (crates.io credentials are theirs; publishing is
  irreversible).

## 3. Read these first (the mechanics are already proven)

- **`claude-notes/plans/2026-06-26-extract-error-reporting-foundation.md`** — the
  authoritative playbook. Phase 1 (source-map) is the leaf-extraction template you
  will mirror almost exactly; Phase 3 (error-reporting) is the second run with the
  gotchas. Read the **Risks** and the per-step completion notes.
- **`claude-notes/designs/cross-package-error-codes.md`** — the error-code
  discipline. Drives the one real design task (§6 below).
- **`claude-notes/plans/2026-06-26-extract-quarto-yaml-validation-design.md`** —
  the original YAML design doc. Note its top banner: parts about
  `error-reporting-core`/façade are superseded; the YAML-specific substance
  (origin codes, delete `validate-yaml`, the discipline application) still stands.

## 4. Repo structure (DECIDED: one two-crate workspace)

```
posit-dev/quarto-yaml/            (new repo)
  Cargo.toml                      # [workspace] members=["crates/*"]; [workspace.package]; [workspace.dependencies]
  crates/
    quarto-yaml/                  # the parser leaf
    quarto-yaml-validation/       # schema validation; depends on quarto-yaml
  LICENSE  README.md  .gitignore  .gitattributes  .github/workflows/ci.yml
```

- Use a **workspace** (unlike the single-crate foundation repos). Per-crate
  `Cargo.toml`s inherit `version`/`edition`/`license`/`repository` from
  `[workspace.package]` and pull shared deps from `[workspace.dependencies]`.
- **`.gitattributes` with `* text=auto eol=lf` from commit 1** — Phase 3's Windows
  CI caught a CRLF bug in committed JSON-vs-generated comparisons; start with LF
  enforced so you never hit it. (`quarto-yaml-validation` ships
  `test-fixtures/` YAML/JSON — same exposure.)
- **CI** (`.github/workflows/ci.yml`): mirror the foundation repos — matrix
  Linux/macOS/**Windows** on **stable** Rust, `fmt` + `clippy --all-targets -D
  warnings`, `cargo test`. (Both crates need no nightly.) Watch for **stable-clippy
  lints the q2 pinned-nightly tolerates** — Phase 3 hit `items_after_test_module`
  in `macros.rs`; fix in the new repo (q2 deletes its copy at cutover, so the
  standalone becomes the single source — no divergence).

## 5. Dependency & cutover facts (measured 2026-06-29 on main)

**`quarto-yaml`** (the leaf): deps `yaml-rust2`, `serde`, `thiserror`,
`quarto-source-map`. Its **only** quarto dep is the published source-map →
becomes `quarto-source-map = "0.1.0"`. Does **not** depend on
`quarto-error-reporting`. In-tree q2 consumers: **pampa, quarto-config,
quarto-core, quarto-lsp-core** (+ `validate-yaml`, being deleted).

**`quarto-yaml-validation`**: deps `anyhow`, `thiserror`, `serde`, `serde_json`,
`yaml-rust2`, `regex`, `quarto-yaml`, `quarto-source-map`,
`quarto-error-reporting`. The quarto deps become: `quarto-yaml` (intra-workspace),
`quarto-source-map = "0.1.0"`, `quarto-error-reporting = "0.1.0"`. **Only in-tree
consumer is `validate-yaml`** (the demo binary). **After deleting `validate-yaml`,
`quarto-yaml-validation` has ZERO q2 consumers** — q2 does not depend on it at all.

**Consequence for the q2 cutover:** q2 **deletes** `crates/quarto-yaml-validation`
AND `crates/validate-yaml`, keeps consuming `quarto-yaml` (now published), and
gains **no** dependency on the published `quarto-yaml-validation`. The latter is
published purely for the external Posit consumers.

### The WASM gotcha (read carefully — different from Phase 1)

`wasm-quarto-hub-client` is an *excluded standalone workspace*. It does **not**
directly depend on `quarto-yaml`; it gets it transitively via `pampa`/`quarto-core`
(path-included). Those crates use `quarto-yaml = { workspace = true }`, which
resolves against **q2's** `[workspace.dependencies]` (workspace inheritance follows
the crate's filesystem home — q2 root — even inside the WASM build). So:
- Convert the **path**-dep consumers (`pampa`, `quarto-config`) to
  `{ workspace = true }`; the `{ workspace = true }` ones (`quarto-core`,
  `quarto-lsp-core`) stay.
- Set `[workspace.dependencies.quarto-yaml]` → `version = "0.1.0"`.
- The WASM crate likely needs **no direct `quarto-yaml` dep** (it had a direct
  `quarto-source-map = "0.1.0"` only because it uses source-map directly). **Verify
  with the full `cargo xtask verify`** — the WASM build is the proof; if it can't
  resolve `quarto-yaml`, add a direct `quarto-yaml = "0.1.0"` to the wasm crate (as
  was needed for source-map).

## 6. The one design task: error codes (needs a user decision)

`quarto-yaml-validation/src/error.rs` `ValidationErrorKind::error_code()` currently
returns **Quarto presentation codes** `Q-1-10`, `Q-1-11`, … These do **not** belong
in a standalone library (they are q2's namespace, per the discipline). It has
**no** dependency on an installed catalog (no `get_docs_url`/`install_catalog`
refs), so the change is localized to `error_code()` + the ~15 `error.rs` tests that
assert `"Q-1-x"`.

Per `cross-package-error-codes.md`, change `error_code()` to **own, namespaced
origin codes** — e.g. `yaml-schema/missing-required`, `yaml-schema/type-mismatch`,
`yaml-schema/invalid-enum`, … (one per `ValidationErrorKind` variant). There is
**no q2 remap** to build (q2 doesn't consume the crate); the external consumers get
the origin codes and may remap to their own presentation codes.

> **⚠️ DECISION TO CONFIRM WITH THE USER before implementing.** Changing `Q-1-x` →
> `yaml-schema/*` is **breaking** for the invisible Posit consumers that currently
> see `Q-1-x`. Options:
> - **(A) Origin codes from `0.1.0`** — clean per the discipline; coordinate the
>   break with those consumers. *Recommended* (0.1.0 is a fresh public line; do it
>   right from the start).
> - **(B) Ship `Q-1-x` as-is in `0.1.0`, defer origin codes to `0.2.0`** —
>   non-breaking now, but ships Quarto codes in a "non-Quarto" crate (violates the
>   discipline) until later.
>
> Ask the user which, and (for A) capture the `Q-1-x` → `yaml-schema/*` mapping as
> a frozen table in the commit message so the lineage is recoverable.

## 7. Execution checklist

### Phase A — `quarto-yaml` (the leaf; publish first)
- [ ] Create `/Users/cscheid/repos/github/posit-dev/quarto-yaml/` as a **workspace**;
      copy `crates/quarto-yaml/` → `crates/quarto-yaml/`; add `LICENSE` (from q2
      root), `.gitignore` (`/target`), `.gitattributes` (`* text=auto eol=lf`),
      `README.md` (write fresh — the crate is a YAML parser with source tracking).
- [ ] Workspace `Cargo.toml`: `[workspace] members=["crates/*"]`,
      `[workspace.package]` (version `0.1.0`, edition `2024`, license `MIT`,
      `repository = https://github.com/posit-dev/quarto-yaml`, authors), and
      `[workspace.dependencies]` with `quarto-source-map = "0.1.0"` + the shared
      crates.io deps (pin versions to match q2's `[workspace.dependencies]`:
      `yaml-rust2`, `serde`, `thiserror`, …). Drop `[lints] workspace = true` or
      add a `[workspace.lints]` block (q2 has none).
- [ ] Build + `cargo test` + `cargo clippy --all-targets -- -D warnings` + `cargo
      fmt --check` + `cargo publish --dry-run -p quarto-yaml`. Fix any stable-clippy
      lints (see §4).
- [ ] External-consumer smoke test (separate crate, path dep, parse a YAML string,
      assert source info) — proves the public API is usable standalone.
- [ ] `gh repo create posit-dev/quarto-yaml --public --source=. --push`
      (public; mirror Phase 1/3). Confirm CI green on all 3 OSes.
- [ ] **USER**: `cargo publish -p quarto-yaml` (from the new repo).

### Phase B — `quarto-yaml-validation` (second crate, same repo)
- [ ] Copy `crates/quarto-yaml-validation/` (incl. `test-fixtures/`) into the
      workspace. Its `quarto-yaml` dep = `{ version = "0.1.0", path =
      "../quarto-yaml" }` (path for local dev, version so `cargo publish` resolves
      the now-published `quarto-yaml`); `quarto-source-map` / `quarto-error-reporting`
      = `"0.1.0"`.
- [ ] **Apply the error-code change** from §6 (after the user's A/B decision) +
      update the `error.rs` tests.
- [ ] If yaml-validation tests render diagnostics and asserted `Q-1-x` text, update
      them; with no catalog installed in the standalone repo, diagnostics render
      **code-only** (`EmptyCatalog`) — assert on the origin codes.
- [ ] Build + test + clippy + `cargo publish --dry-run -p quarto-yaml-validation` +
      smoke test (validate a doc against a schema, assert a `yaml-schema/*` code).
- [ ] CI green (3 OSes). **USER**: `cargo publish -p quarto-yaml-validation`
      (after `quarto-yaml` is live so the dep resolves).

### Phase C — q2 cutover (one PR, like #348/#350)
- [ ] Branch `braid/bd-egcyeym9-yaml-cutover` off updated main.
- [ ] `[workspace.dependencies.quarto-yaml]` `path` → `version = "0.1.0"`.
- [ ] Convert `quarto-yaml` path-deps (`pampa`, `quarto-config`) →
      `{ workspace = true }`; leave the existing `{ workspace = true }` ones.
- [ ] **Delete** `crates/quarto-yaml-validation/` and `crates/validate-yaml/` (and
      drop `[workspace.dependencies.quarto-yaml-validation]` from root `Cargo.toml`;
      check for any stray refs to `validate-yaml` in `xtask`/docs/CI).
- [ ] Delete in-tree `crates/quarto-yaml/`.
- [ ] `cargo build --workspace` → confirm Cargo.lock resolves `quarto-yaml 0.1.0`
      from the registry. `cargo nextest run --workspace`. **Full `cargo xtask
      verify`** (the WASM leg is the real gate — see §5 WASM gotcha; do NOT pipe it
      through `tail`, which masks the exit code — a Phase 1 lesson).
- [ ] Update `CLAUDE.md`: move `quarto-yaml` into the "Externalized foundation
      crates" section; remove `quarto-yaml-validation` and `validate-yaml` from the
      binaries/crate lists; the `validate-yaml` line under **Binaries** must go.
- [ ] Commit, push to `feature/…`, open PR against `main`, watch CI (5 checks),
      report. Merge is the user's call.

## 8. Proven gotchas (from Phases 1 & 3 — don't rediscover them)

- **CRLF on Windows** → `.gitattributes` `* text=auto eol=lf` from the first commit.
- **Stable clippy stricter than q2's nightly** → fix lints in the new repo
  (`items_after_test_module` etc.); the standalone repo becomes the single source.
- **WASM workspace resolution** → §5; verify with full `cargo xtask verify`, never
  `--skip-hub-build`.
- **`| tail` masks `cargo xtask verify`'s real exit code** → run it without a tail
  pipe (or check the file), or use `run_in_background`.
- **crates.io / GitHub are user/identity-gated and irreversible** → you prep & dry-
  run; the user publishes and (optionally) `cargo owner --add github:posit-dev:<team>`.

## 9. Open items to raise with the user

1. **Error-code policy (§6 A vs B)** — the one blocking decision.
2. Confirm the repo is `posit-dev/quarto-yaml` (workspace, two crates) — decided
   2026-06-29, but re-confirm before `gh repo create`.
3. Reuse the Phase-1 visibility/ownership choices (public repo; personal crates.io
   account now, `cargo owner --add posit-dev` on a weekday) unless told otherwise.
4. Whether to also relocate the deleted **`CONTRIBUTING-ERRORS.md`** intent / any
   q2-internal YAML docs (low priority).
