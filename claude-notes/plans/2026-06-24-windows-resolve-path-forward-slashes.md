# Fix Windows path-separator assumptions in pampa quarto_api path tests (bd-picv)

**Date:** 2026-06-24
**Braid:** bd-picv
**Worktree:** `.worktrees/bd-picv-fix-windows-path-separator` (branch `braid/bd-picv-fix-windows-path-separator`, based on `main`)
**Status:** Design settled with user (2026-06-24). Direction chosen: production normalize + `is_rooted`. **Implementation not yet started.**

## Triage verdict

**Ready to design → design now settled.** Root cause confirmed and reproduced; fix
direction chosen by user (option a: production-side forward-slash normalization +
`is_absolute()`→`is_rooted()` swap). Ready to implement TDD-first.

## Issue context

Strand filed 2026-04-28 (bug, P2, labels lua/pampa/windows). Original framing: ~10
tests in `crates/pampa/src/lua/quarto_api.rs` fail on Windows because expected paths
hardcode forward slashes while the Windows impl joins with backslashes via `Path::join`.
Strand offered a binary choice: (a) normalize output to forward slashes, or (b)
platform-correct test expectations.

**Fresh investigation reframed the strand:**

- **Native Windows is functionally fine — not a live bug.** `resolve_path` output flows
  only to `io.open`, `fs::read` (via `mediabag.fetch`), and `dofile`. All accept
  backslash paths on Windows. `q2 render` works on Windows; the backslashes are cosmetic.
  The only breakage is the test assertions.
- **There is a deeper, currently-masked bug.** `resolve_path` gates on `p.is_absolute()`
  (`quarto_api.rs:360`). The workspace's own `quarto-util::is_rooted` doc comment says use
  `is_rooted()` instead, because `is_absolute()` returns `false` on `wasm32` for
  `/project/...` VFS paths. bd-vxl8 already swept `io_wasm.rs`/`dofile_wasm.rs` to
  `is_rooted`; `resolve_path` was missed. No current caller passes an absolute path here
  (extension Lua uses relative names), so it's latent — but a real convention divergence.
- **Fix belongs in production.** `quarto_util::to_forward_slashes` already exists and
  bd-vxl8 set the precedent of normalizing separators at the boundary. Option (a) makes
  output deterministic cross-platform, matches quarto-cli `pathWithForwardSlashes` and this
  tree's convention, and lets tests assert forward slashes on all platforms with **no
  `#[cfg(windows)]` duplication**. Option (b) leaves production inconsistent and forces
  cfg-gated test expectations.

## Dependency graph

- **related → bd-3pe8** (closed). "Audit pampa production Lua code for Windows path
  escaping." Concluded production Lua *string interpolation* was safe (uses Lua C API, not
  `format!` into source); the test-side fix was correct there. bd-picv is the adjacent
  surface bd-3pe8 did **not** cover: the *return value* of a runtime function, not
  interpolation into source.
- The actual convention-setting strand is **bd-vxl8** (commit `b406cadc`): swapped
  `is_absolute`/`starts_with('/')` → `quarto_util::is_rooted` in `io_wasm.rs` /
  `dofile_wasm.rs`, and used `to_forward_slashes` when embedding native paths into Lua
  source strings in tests. This is the precedent bd-picv follows.

## What the code looks like today

Reproduced at HEAD on Windows (2026-06-24):

```
test_normalize_path_simple: left: "\\a\\b\\c"  right: "/a/b/c"
test_quarto_utils_resolve_path_relative_with_subdir: left: "\\ext\\dir\\sub\\data.json" right: "/ext/dir/sub/data.json"
```

Key code:

- `quarto_api.rs:358-369` — `resolve_path`: `if p.is_absolute() { return Ok(path); }`
  then `let resolved = PathBuf::from(&script_dir).join(&path); Ok(normalize_path(&resolved))`.
- `quarto_api.rs:476-496` — `normalize_path`: collapses `.`/`..`, then
  `result.to_string_lossy().to_string()` — emits native separators verbatim.
- Production `push_script_dir` callers feed native paths with backslashes:
  - `filter.rs:177-182` — `filter_path.parent().to_string_lossy()`
  - `shortcode.rs:136-142` — `script_path.parent().to_string_lossy()`
- Helper: `quarto-util/src/path.rs:23` `to_forward_slashes`, `:14` `is_rooted`
  (re-exported `quarto-util/src/lib.rs:8`).

The ~10 failing tests run twice (lib + bin/pampa integration), so ~18-20 visible failures
from one root cause.

## Proposed phases

### Phase 0 — Test plan (TDD)

The failing tests already encode the desired forward-slash behavior — they ARE the
failing-first tests. Confirm they fail at branch HEAD on Windows (done). Add coverage the
current tests lack:

- [ ] A test that `resolve_path` returns forward slashes when the **pushed script_dir
      itself contains backslashes** (simulates the real `filter.rs`/`shortcode.rs` input on
      Windows). Current tests push forward-slash dirs (`/some/extension/dir`), so they don't
      exercise the backslash-input path. Use a backslash literal dir and assert
      forward-slash output.
- [ ] A test that an absolute/rooted input is returned forward-slash-normalized (covers the
      `is_rooted` branch return value, e.g. `C:\abs\x.json` → `C:/abs/x.json` on Windows;
      `/abs/x.json` unchanged elsewhere).

### Phase 1 — Production fix in `quarto_api.rs`

- [ ] `resolve_path`: replace `p.is_absolute()` with `quarto_util::is_rooted(p)`; return
      `quarto_util::to_forward_slashes(p)` from the rooted branch (so a rooted input with
      backslashes is normalized, not returned verbatim).
- [ ] `normalize_path`: replace the trailing `result.to_string_lossy().to_string()` with
      `quarto_util::to_forward_slashes(&result)` so the collapsed path is forward-slash on
      all platforms.
- [ ] Decide the no-script-dir relative branch (`return Ok(path)` at :365): normalize it too
      for full determinism, or leave (input is already forward-slash from Lua source). See
      design note below — leaning normalize-for-consistency.

### Phase 2 — Verify tests pass with NO cfg-gating

- [ ] Existing assertions (`"/a/b/c"`, `"/ext/dir/sub/data.json"`) pass unchanged on all
      platforms. If any still need a `#[cfg(windows)]`, that signals the production fix is
      incomplete — investigate rather than cfg-gate.

### Phase 3 — Workspace verification

- [ ] `cargo nextest run -p pampa` (lib + integration) green on Windows.
- [ ] `cargo xtask verify` — full leg, since `quarto_api.rs` is in pampa which
      wasm-quarto-hub-client depends on; the `is_rooted` change touches WASM-relevant logic.
- [ ] Confirm `quarto-util` is a normal (non-dev) dependency of pampa (bd-3pe8 flagged this
      as a possible gate). Check `crates/pampa/Cargo.toml`.

## Open design questions for the user

Direction is settled. Two small residual choices, both with a lean — confirm or override
during implementation, not blocking:

1. **No-script-dir relative branch** (`resolve_path` :364-365). When no script dir is set,
   a relative input is returned as-is. Normalize it to forward slashes too (consistency:
   every return path is forward-slash), or leave verbatim (input from Lua source is already
   forward-slash; minimal change)? **Lean: normalize, for a single invariant — "resolve_path
   always returns forward slashes."**
2. **`quarto-util` dependency tier in pampa.** If `quarto_util` is currently a dev-dependency
   of pampa (used only in tests so far), this change promotes it to a regular dependency.
   Confirm that's acceptable (it should be — it's a tiny utility crate). **Lean: promote.**

## Risks / tradeoffs

- **Behavior change, low risk.** `resolve_path` output changes from backslash to
  forward-slash on Windows. All known consumers (`io.open`, `fs::read`, `dofile`) accept
  forward slashes on Windows, so no functional regression. WASM already produces
  forward-slash, unaffected.
- **`is_rooted` swap is behavior-neutral today** (no caller passes an absolute path), but
  forward-compatible: a future caller passing a `/project/...` VFS path in WASM would now be
  handled correctly instead of being wrongly joined to the script dir.
- **No DOM/document-output exposure found** — `resolve_path` output does not flow into
  hrefs/srcs in rendered output (only to file-read APIs), so this is not a rendered-artifact
  correctness fix, just a convention + latent-WASM-bug fix.
