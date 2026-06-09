# Cargo dependency pins and known transitive incompatibilities

This document is read by the `upgrade-cargo-deps` skill on every survey
run. The goal: every pin and every known-bad version pair has a written
reason and a written removal condition, so the skill can periodically
re-check whether the pin is still load-bearing.

When the skill runs (see `SKILL.md`, step 4: "Identify excluded /
vendored crates"), it reads this file, lists each entry under the
"Skipped" section of the survey plan, and **explicitly checks the
removal condition**. If the condition is met, the survey plan should
flag the pin as removable so the next session can land the cleanup.

## Format

Each entry is a level-3 heading with these required fields:

- **Where**: file path(s) and line(s) where the pin lives.
- **Pin**: the literal cargo constraint or the affected versions.
- **Reason**: why this pin exists (one paragraph max).
- **Removal condition**: a concrete, observable trigger for un-pinning.
  The skill checks this on each run.
- **Last reviewed**: YYYY-MM-DD of the most recent skill run that
  re-checked this entry.

If you discover a new pin or transitive incompatibility, add an entry
here in the same format. If a pin is removed, leave a tombstone entry
under "Resolved" (below) for one or two skill cycles, then delete.

## Active pins

### wasm-bindgen-futures `=0.4.58` (and transitively wasm-bindgen `=0.2.108`, js-sys `=0.3.85`)

**Where**:
- `crates/quarto-system-runtime/Cargo.toml` (in the `cfg(target_arch =
  "wasm32")` deps section).
- `crates/wasm-quarto-hub-client/Cargo.toml` (top-level `[dependencies]`).
- The `[patch.crates-io]` entry in
  `crates/wasm-quarto-hub-client/Cargo.toml` redirects
  `wasm-bindgen-futures` to the vendored
  `crates/wasm-bindgen-futures-patch/`, which carries the
  corresponding `wasm-bindgen = "=0.2.108"` and `js-sys = "=0.3.85"`
  exact pins inside its own Cargo.toml.

**Pin**: `wasm-bindgen-futures = "=0.4.58"` at both call sites.

**Reason**: We vendor a copy of `wasm-bindgen-futures` in
`crates/wasm-bindgen-futures-patch/` to substitute upstream's
implementation via `[patch.crates-io]`. The vendored copy is at
version 0.4.58 and pins the wasm-bindgen ecosystem to `=0.2.108` /
`=0.3.85` (matching the locally-installed `wasm-bindgen-cli` tooling
the project's `npm run build:wasm` script invokes). Without the `=`
constraint at the call sites, cargo's resolver prefers the highest
matching version on crates.io (currently 0.4.70) over the patch's
0.4.58, silently dropping the patch and the transitive exact pins.
The result is a wasm-bindgen 0.2.120 in the lockfile that mismatches
the installed CLI and breaks `npm run build:wasm` with a "version
mismatch" error.

**Removal condition**: One of:

1. The vendored `wasm-bindgen-futures-patch` is bumped to a current
   upstream version (whatever wasm-bindgen-futures is at on
   crates.io). After re-vendoring:
   - Update both pin sites to match the new patch version (or relax
     to a caret if no exact pin is needed for the substitution).
   - Run `cargo install -f wasm-bindgen-cli --version <new>` to align
     the CLI.
   - Verify `npm run build:wasm` still succeeds.
2. The reason for vendoring is gone — i.e., we no longer need any
   custom wasm-bindgen-futures behavior. In that case delete the
   `wasm-bindgen-futures-patch/` crate, drop the
   `[patch.crates-io]` entry, drop both pins. (If this is true, the
   patch's diff vs. upstream should be empty or trivially mergeable.)

**Why the patch exists in the first place**: see
`claude-notes/plans/2026-04-20-wasm-shim-merge.md` and the comment in
`crates/wasm-bindgen-futures-patch/Cargo.toml`. (As of 2026-05-04 the
patch is the upstream `wasm-bindgen-futures 0.4.58` source verbatim;
if the diff vs. upstream really is empty, condition 2 above applies
and the patch can simply be deleted — confirm by re-vendoring fresh
and `diff`-ing.)

**Last reviewed**: 2026-05-04 (added).

## Known transitive incompatibilities

### temporal_rs 0.1.2 ↔ icu_calendar 2.2.x (semver violation upstream)

**Where**: dep chain `deno_core v0.376 → v8 v142 → temporal_capi
v0.1.2 → temporal_rs v0.1.2 → icu_calendar ^2.1`. We don't depend on
icu_calendar directly.

**Symptom**: when `Cargo.lock` is regenerated from scratch (e.g.
`rm Cargo.lock && cargo build`, or `cargo update --aggressive`),
cargo picks the latest `icu_calendar 2.2.x` under the `^2.1`
constraint. `temporal_rs 0.1.2`'s source uses
`icu_calendar::cal::AnyCalendarDifferenceError`,
`icu_calendar::types::DateDurationUnit`, and several
`DateFromFieldsError::*` variants that were removed in icu_calendar
2.2. The build fails with ~9 `E0599` / `E0432` errors in `temporal_rs`.
`Cargo.lock` happens to pin icu_calendar at `2.1.1` from a prior
resolve, so day-to-day builds work — the breakage only surfaces on
regeneration.

**Reason it's not pinned by us**: the bad version (`icu_calendar
2.2.x`) is technically a minor release on a 1.x crate (semver-
compatible with 2.1), so it should not have removed public APIs.
This is an upstream semver violation, not something we need to
defend against indefinitely. Pinning it ourselves would mean adding
icu_calendar as a direct workspace dep we don't otherwise use, which
is invasive for a bug that has a natural upstream resolution.

**Removal condition**: bd-nl5q (deno_core 0.376 → 0.400) lands. The
newer deno_core/v8 stack pulls in newer `temporal_rs` (0.2.x at
time of writing), which is compiled against current icu_calendar
APIs. Once deno_core is bumped, run `rm Cargo.lock && cargo build
--workspace` from a fresh state — if it succeeds, this entry can be
deleted.

**Workaround until then**: don't regenerate `Cargo.lock` from
scratch. If a merge or operation forces it, restore the pre-merge
lockfile (`git checkout HEAD -- Cargo.lock`) and let cargo update
incrementally rather than from-scratch.

**Last reviewed**: 2026-05-04 (added).

## Resolved

(empty — populate with tombstones when a pin or workaround is removed)
