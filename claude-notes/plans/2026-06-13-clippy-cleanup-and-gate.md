# Clean up clippy debt and gate clippy in CI (bd-3zst4hwy)

## Overview

CI enforces `RUSTFLAGS="-D warnings"` (rustc warnings) but **never runs
`cargo clippy`**, so clippy lint violations accumulated unchecked. This
plan clears them and adds a CI gate so the workspace stays clean.

### Scope (measured, not guessed)

The workspace already has a curated `[workspace.lints.clippy]` table
(root `Cargo.toml`) that **allows** the architectural/stylistic lints
(`result_large_err`, `large_enum_variant`, `too_many_arguments`,
`type_complexity`, `ptr_arg`, `wrong_self_convention`, …) and sets a set
of specific lints to `warn`. All 41 workspace crates opt in via
`[lints] workspace = true`.

Respecting that allow-policy (`cargo clippy --workspace --all-targets --
-D warnings`), the **true** burn-down is **69 violations across 11
crates**:

| crate | n |
|---|---|
| quarto-hub | 42 |
| xtask | 11 |
| quarto-system-runtime | 7 |
| quarto-highlight-encoding | 2 |
| quarto-trace-server, quarto-source-map, quarto-sass, quarto-preview, quarto-mcp-launcher, quarto-brand, lua-src-wasm | 1 each |

By lint:

| lint | n | auto-fix? |
|---|---|---|
| collapsible_if | 32 | yes |
| map_unwrap_or | 13 | yes |
| needless_borrows_for_generic_args | 5 | yes |
| implicit_clone | 4 | yes |
| get_unwrap | 3 | usually |
| io_other_error | 2 | yes |
| derivable_impls | 2 | yes |
| collapsible_match | 2 | yes |
| unreadable_literal, unnecessary_map_or, single_component_path_imports, should_implement_trait, print_literal, naive_bytecount | 1 each | mixed |

> ⚠️ Earlier raw counts of ~560 / ~1811 were an artifact of running
> clippy with `-W clippy::all`, which **re-enables the deliberately
> allowed** architectural lints. Those stay allowed; do not "fix" them.

### Judgment-call lints (per the agreed policy: `#[allow]` + note, file
follow-ups only if a real refactor is warranted)

- `should_implement_trait` @ `quarto-source-map/src/source_info.rs:163`
  — an inherent method shadowing a trait method name; likely a
  deliberate API. `#[allow]` with a note unless trivially renameable.
- `naive_bytecount` @ `xtask/src/braid_snapshot.rs:60`,
  `xtask/src/create_worktree.rs:1132` — would pull in the `bytecount`
  crate for a dev-tool line count; `#[allow]` (not worth a dep).

## Gate mechanism (gate-and-grow)

The opt-in lever (`[lints] workspace = true`) is already fully consumed —
all crates are in. So the gate is simply: **fix all 69, then enforce
`-D warnings` for clippy in CI + xtask.** No per-crate baseline-allow
scaffolding is needed at this size. (Had the count been in the hundreds,
we'd have baselined per-crate allows and burned them down; documented
here in case the policy widens later.)

Enforcement points to add once green:
1. `.github/workflows/test-suite.yml` — a clippy step:
   `cargo clippy --workspace --all-targets -- -D warnings`.
2. `cargo xtask verify` / `lint` — same invocation, so local matches CI
   (mirrors the existing `-D warnings` RUSTFLAGS pattern).

## Work items

- [x] Measure true scope; strand + plan (bd-3zst4hwy)
- [ ] Phase 1 — `cargo clippy --fix --workspace --all-targets` (auto-fix
      the mechanical bulk); inspect the diff
- [ ] Phase 2 — hand-fix / `#[allow]` the remainder (judgment lints above)
- [ ] Phase 3 — confirm `cargo clippy --workspace --all-targets --
      -D warnings` reports **zero**
- [ ] Phase 4 — full regression: `cargo nextest run --workspace`
      (clippy --fix can subtly change behavior; must stay green) +
      `cargo build --workspace`
- [ ] Phase 5 — add the CI step + xtask leg (the gate)
- [ ] Phase 6 — `cargo xtask verify --skip-hub-build` (or full if WASM
      buildable here)

## Verification record

_(filled in as phases complete)_
