# bd-vxl8: Windows path handling in pampa synthetic Lua io/dofile — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the 9 failing `io_wasm` tests pass on Windows and fix a latent wasm-only `dofile` path-resolution bug, by replacing two inconsistent inline absolute-path checks with one shared, cross-target-correct predicate.

**Architecture:** Add `is_rooted()` (a documented `has_root()` wrapper) to `quarto-util`; use it at the two synthetic-Lua VFS-resolution sites (`io_wasm.rs`, `dofile_wasm.rs`); fix the 9 test scripts to embed forward-slash paths so Windows backslashes don't become Lua escape sequences.

**Tech Stack:** Rust, mlua, `cargo nextest`, `cargo xtask verify`.

## Why (context)

The strand framed this as one bug (Lua `\`-escaping). It is two:
1. **Escaping:** `file_path.display()` puts `\` into a Lua source string → `SyntaxError`.
2. **Path resolution:** `io.open` (`io_wasm.rs:41`) uses `path.starts_with('/')`; a forward-slashed Windows path `C:/Users/...` isn't matched → gets a `/project/` VFS prefix → `NativeRuntime` read fails.

`is_absolute()` is the *wrong* fix: on `wasm32-unknown-unknown` (no `target_family`) it returns `false` even for `/foo`. The correct primitive is `has_root()` — in-tree precedent `artifact.rs:119`, `output_sink.rs:230` (bd-cfl67).

`dofile_wasm.rs:31` already uses `is_absolute()`, which is **wrong on wasm32** — a rooted VFS path is mis-resolved. That is a real production bug (this fn runs only under `cfg(wasm32)`), fixed here for free by sharing the predicate.

Decision (externally reviewed, fresh context): keep these tests **native** — there is no Rust wasm32 test harness and building one is disproportionate; WASM is smoke-covered black-box via TS `*.wasm.test.ts`. Follow-up tracked as **bd-yd3uzx5a**.

## Global Constraints

- Test-first is mandatory (`crates/pampa/CLAUDE.md`): write test, confirm it fails, then fix.
- Use `cargo nextest run`, never `cargo test`; never pipe nextest through `tail`.
- `quarto_util` is already a dependency of `pampa`.
- Both `io_wasm.rs` and `dofile_wasm.rs` already `use std::path::Path;`.
- Branch: `braid/bd-vxl8-fix-windows-lua-path` (this worktree). Do not push without Chris's approval (repo GIT PUSH POLICY).

## File Structure

- `crates/quarto-util/src/path.rs` — add `is_rooted()` + its unit test (one responsibility: cross-platform path helpers).
- `crates/quarto-util/src/lib.rs` — re-export `is_rooted` at crate root.
- `crates/pampa/src/lua/io_wasm.rs` — production predicate swap at the `io.open` site + 9 test-script escaping fixes.
- `crates/pampa/src/lua/dofile_wasm.rs` — production predicate swap in `resolve_dofile_path` + one new direct unit test.

---

### Task 1: `is_rooted` predicate in `quarto-util`

**Files:**
- Modify: `crates/quarto-util/src/path.rs`
- Modify: `crates/quarto-util/src/lib.rs:8`
- Test: `crates/quarto-util/src/path.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `quarto_util::is_rooted(path: &std::path::Path) -> bool`

- [ ] **Step 1: Write the failing test**

In `crates/quarto-util/src/path.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn test_is_rooted_distinguishes_rooted_from_relative() {
        assert!(is_rooted(Path::new("/abs/file.txt")));
        assert!(!is_rooted(Path::new("relative/file.txt")));
    }
```

(`mod tests` already has `use super::*;`, so `is_rooted` and `Path` resolve.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p quarto-util test_is_rooted_distinguishes_rooted_from_relative`
Expected: FAIL — compile error, `cannot find function is_rooted`.

- [ ] **Step 3: Write minimal implementation**

In `crates/quarto-util/src/path.rs`, after `to_forward_slashes`, add:

```rust
/// Returns `true` if `path` has a root component.
///
/// Use this, not [`std::path::Path::is_absolute`], for "should this path be
/// used as-is, or resolved against a working directory?" decisions. On
/// `wasm32-unknown-unknown` (no `target_family`) `is_absolute()` returns
/// `false` even for rooted paths like `/foo`, whereas `has_root()` is correct
/// on both native and WASM targets. Same rationale as `quarto-core`'s
/// `artifact.rs` / `output_sink.rs` (bd-cfl67).
pub fn is_rooted(path: &Path) -> bool {
    path.has_root()
}
```

In `crates/quarto-util/src/lib.rs`, change line 8:

```rust
pub use path::{is_rooted, to_forward_slashes};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p quarto-util`
Expected: PASS (all quarto-util tests green).

- [ ] **Step 5: Commit**

```bash
git add crates/quarto-util/src/path.rs crates/quarto-util/src/lib.rs
git commit -m "feat(quarto-util): add is_rooted cross-target path predicate

has_root() is correct on wasm32-unknown-unknown where is_absolute()
returns false for rooted paths; gives one canonical predicate for VFS
path resolution to share between the synthetic Lua io and dofile sites."
```

---

### Task 2: Fix `io_wasm.rs` Windows failures

**Files:**
- Modify: `crates/pampa/src/lua/io_wasm.rs:41` (production `io.open` site)
- Modify: `crates/pampa/src/lua/io_wasm.rs` (`#[cfg(test)] mod tests`, ~line 476 + 9 scripts)

**Interfaces:**
- Consumes: `quarto_util::is_rooted` (Task 1), `quarto_util::to_forward_slashes`.

- [ ] **Step 1: Confirm the existing 9 tests fail (red baseline)**

Run: `cargo nextest run -p pampa io_wasm`
Expected: FAIL — `test_io_open_read_all`, `test_io_open_read_line`,
`test_io_open_read_number`, `test_io_open_read_bytes`, `test_io_type`,
`test_io_open_write_and_close`, `test_io_open_write_flush_incremental`,
`test_io_open_append`, `test_io_write_returns_handle_for_chaining` fail with a
Lua `SyntaxError` (backslash escape) on Windows.

- [ ] **Step 2: Apply the test-script escaping fix**

In `io_wasm.rs` `mod tests`, add to the `use` block (~line 476):

```rust
    use quarto_util::to_forward_slashes;
```

Then in each of the 9 test functions, replace the format argument
`file_path.display()` with `to_forward_slashes(&file_path)`. Pattern (example
from `test_io_open_read_all`):

```rust
        // before
        let script = format!(r#"... io.open("{}", "r") ..."#, file_path.display());
        // after
        let script = format!(r#"... io.open("{}", "r") ..."#, to_forward_slashes(&file_path));
```

The 9 functions to edit: `test_io_open_read_all`, `test_io_open_read_line`,
`test_io_open_read_number`, `test_io_open_read_bytes`, `test_io_type`,
`test_io_open_write_and_close`, `test_io_open_write_flush_incremental`,
`test_io_open_append`, `test_io_write_returns_handle_for_chaining`.

- [ ] **Step 3: Re-run to confirm the second bug (evidence step)**

Run: `cargo nextest run -p pampa io_wasm`
Expected: still FAIL, but the failure mode has **changed** — no longer a
`SyntaxError`; now the read/write fails / returns nil because the resolver
prepended `/project/` to `C:/Users/...`. This proves escaping alone is
insufficient.

- [ ] **Step 4: Apply the production predicate fix**

In `io_wasm.rs` `io.open` closure (~line 41), replace:

```rust
            let resolved_path = if path.starts_with('/') {
                path.clone()
            } else {
                format!("/project/{}", path)
            };
```

with:

```rust
            // `is_rooted` (has_root), not `starts_with('/')`/`is_absolute`:
            // correct on all targets incl. wasm32, where `is_absolute()` is
            // false for `/foo`. Shared with dofile_wasm. In production WASM
            // this site only sees `/`-rooted or relative paths, so behavior is
            // unchanged; recognizing native `C:\` paths additionally lets the
            // native test harness exercise this code. See bd-cfl67.
            let resolved_path = if quarto_util::is_rooted(Path::new(&path)) {
                path.clone()
            } else {
                format!("/project/{}", path)
            };
```

- [ ] **Step 5: Run to verify all io_wasm tests pass**

Run: `cargo nextest run -p pampa io_wasm`
Expected: PASS — all io_wasm tests green (the literal-path tests
`test_io_open_missing_file...`, `test_io_open_relative_path_resolves_to_project`,
`test_io_open_unsupported_mode` stay green).

- [ ] **Step 6: Commit**

```bash
git add crates/pampa/src/lua/io_wasm.rs
git commit -m "fix(pampa): handle native paths in synthetic Lua io tests on Windows

The 9 io_wasm tests drive register_wasm_io with NativeRuntime and real
tempfiles. On Windows two things broke them: display() leaked backslashes
into the Lua source (escape-sequence SyntaxError), and io.open's
starts_with('/') check misread a C:/... path as relative and prepended the
/project/ VFS root. Embed forward-slash paths and detect rootedness with the
shared has_root predicate; WASM behavior is unchanged."
```

---

### Task 3: Fix `dofile_wasm.rs` and add a direct resolver test

**Files:**
- Modify: `crates/pampa/src/lua/dofile_wasm.rs:31` (`resolve_dofile_path`)
- Test: `crates/pampa/src/lua/dofile_wasm.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `quarto_util::is_rooted` (Task 1); `crate::lua::quarto_api::init_script_dir_stack`; `super::resolve_dofile_path`.

Note: the 4 existing dofile tests go through `apply_lua_filter`, which on native
builds `Lua::new()` (real C `dofile`) and never registers the synthetic dofile —
so they do **not** reach `resolve_dofile_path`. They are untouched by this change.
The new test calls the resolver directly.

- [ ] **Step 1: Write the failing test**

In `dofile_wasm.rs` `mod tests`, add:

```rust
    #[test]
    fn test_resolve_dofile_path_returns_rooted_path_as_is() {
        use mlua::Lua;
        let lua = Lua::new();
        crate::lua::quarto_api::init_script_dir_stack(&lua).unwrap();
        // A rooted VFS path must be returned unchanged — not /project/-prefixed
        // nor joined under the script dir. is_absolute("/abs") is false on
        // Windows/wasm32, so this is red until the resolver uses has_root.
        let resolved = super::resolve_dofile_path(&lua, "/abs/helper.lua").unwrap();
        assert_eq!(resolved, "/abs/helper.lua");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p pampa test_resolve_dofile_path_returns_rooted_path_as_is`
Expected (on Windows): FAIL — `resolved` is `/project//abs/helper.lua` (empty
script dir + `is_absolute` false). (On Linux/macOS this passes pre-fix because
`is_absolute("/abs")` is true; it is still a valid post-fix regression guard.)

- [ ] **Step 3: Apply the production predicate fix**

In `dofile_wasm.rs` `resolve_dofile_path` (~line 30), replace:

```rust
    let p = Path::new(path);
    if p.is_absolute() {
        return Ok(path.to_string());
    }
```

with:

```rust
    let p = Path::new(path);
    // has_root (quarto_util::is_rooted), not is_absolute: is_absolute() is
    // false on wasm32 for rooted `/foo` paths, which would wrongly fall through
    // to the script_dir / `/project/` branches. This fn runs only under
    // cfg(wasm32). See bd-cfl67.
    if quarto_util::is_rooted(p) {
        return Ok(path.to_string());
    }
```

- [ ] **Step 4: Run to verify the new test passes and existing ones stay green**

Run: `cargo nextest run -p pampa dofile`
Expected: PASS — the new test plus the 4 existing dofile tests
(`test_dofile_executes_and_returns_values`, `test_loadfile_returns_callable_chunk`,
`test_dofile_nonexistent_errors`, `test_loadfile_nonexistent_returns_nil_and_error`).

- [ ] **Step 5: Commit**

```bash
git add crates/pampa/src/lua/dofile_wasm.rs
git commit -m "fix(pampa): resolve rooted dofile paths correctly on wasm32

resolve_dofile_path ran only under cfg(wasm32) and used is_absolute(),
which is false on wasm32 for rooted /foo paths — so a rooted VFS path was
mis-resolved (joined under the script dir or double /project/-prefixed).
Use the shared has_root predicate. Covered by a direct resolver unit test;
the existing dofile tests exercise the real C dofile and are unaffected."
```

---

### Task 4: Full workspace verification

**Files:** none (verification only).

- [ ] **Step 1: Crate-level regression check**

Run: `cargo nextest run -p pampa`
Expected: PASS — no regressions in pampa.

- [ ] **Step 2: Workspace tests**

Run: `cargo nextest run --workspace`
Expected: PASS.

- [ ] **Step 3: Full `cargo xtask verify`**

`pampa` feeds `wasm-quarto-hub-client`, so the WASM leg is in scope — run the
**full** verify (not `--skip-hub-build`), per repo CLAUDE.md. This is the
`-D warnings` / WASM-build gate that plain build/test miss.

Run: `cargo xtask verify`
Expected: PASS. (Chris runs this himself — not in the background — per
established preference.)

- [ ] **Step 4: Close the strand and hand off**

```bash
braid update bd-vxl8 --status in_progress   # if not already
braid close bd-vxl8 --reason "Shared is_rooted predicate fixes Windows io_wasm tests + latent wasm32 dofile resolution; full verify green."
```

Then show the diff stat and **stop** — do not push. Await Chris's approval, then:

```bash
git push -u origin braid/bd-vxl8-fix-windows-lua-path:feature/bd-vxl8-fix-windows-lua-path
```

## Self-Review

- **Spec coverage:** escaping fix (Task 2 Step 2), production `io.open` predicate (Task 2 Step 4), shared predicate (Task 1), dofile predicate + direct test (Task 3), scope guards honored (no URL `starts_with('/')` sites, no full resolver, no lossy-helper cleanup), full verify (Task 4). All covered.
- **Placeholder scan:** none — every code/command step has concrete content.
- **Type consistency:** `is_rooted(&Path) -> bool` defined Task 1, consumed verbatim in Tasks 2/3. `resolve_dofile_path(&Lua, &str) -> mlua::Result<String>` matches the existing signature; `init_script_dir_stack(&Lua) -> mlua::Result<()>` matches `quarto_api.rs:181`.
