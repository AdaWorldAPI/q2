# Fix Windows test failures: lua system command tests use Unix-only programs

Strand: bd-c5bdf948
Date: 2026-06-25

## Verdict

**Tests problem, not tools problem.** The `exec_command` runtime is correct and
Pandoc-compatible; the three tests are non-hermetic and assume Unix coreutils on
`PATH`.

## Overview

Three tests in `crates/pampa/src/lua/system.rs` call programs that are not
standalone executables on a stock Windows install:

- `test_command_success` → `echo` (a `cmd`/PowerShell builtin, not a binary)
- `test_command_failure` → `false` (Unix utility)
- `test_command_with_input` → `cat` (Unix utility)

All three fail with `RuntimeError("I/O error: program not found")`.

## Investigation findings (evidence)

The failure is **PATH-dependent**, which is why it looked intermittent:

| Run environment | `echo`/`false`/`cat` on PATH? | Result |
|---|---|---|
| Bash tool (Git Bash) | Yes — `C:\Program Files\Git\usr\bin\{echo,false,cat}.exe` | **PASS** (8/8) |
| PowerShell tool (no Git usr/bin) | No | **FAIL** (3/3 command tests) |

`where.exe echo` → `C:\Program Files\Git\usr\bin\echo.exe`; the same for `false`
and `cat`. Git Bash puts `usr/bin` (MSYS2 coreutils) on PATH, so any process
spawned from a Git Bash shell inherits real `echo.exe`/`cat.exe`/`false.exe`.
PowerShell, `cmd`, and a clean CI runner do **not** — so `Command::new("echo")`
returns "program not found". That is the environment the strand was filed from.

### Why this is NOT a tools bug

`SystemRuntime::exec_command` (`crates/quarto-system-runtime/src/native.rs:216`)
is a direct `Command::new(command).args(args).spawn()` — no shell. This matches
Pandoc's `pandoc.system.command`, which also spawns the program directly with no
shell. On a stock Windows box a Lua filter author calling
`pandoc.system.command('echo', …)` would hit the same "program not found" — that
is correct, Pandoc-compatible behavior. Making the *tool* paper over missing
Unix programs would diverge from Pandoc and is the wrong fix.

### What the tests actually exercise

The `command` binding wiring, not the programs themselves:
- `test_command_success` — exit-code-0 maps to `Value::Boolean(false)`, stdout captured.
- `test_command_failure` — non-zero exit maps to `Value::Integer(code)`.
- `test_command_with_input` — stdin piping + stdout capture round-trips.

Any platform-appropriate program that produces the same observable behavior
satisfies these contracts.

## Draft phases (skeleton — design not finalized)

- [ ] Phase 1: make the three tests hermetic / platform-aware
  - [ ] `test_command_success`: Windows uses `cmd` + `["/C", "echo", "hello"]`; Unix keeps `echo`.
  - [ ] `test_command_failure`: Windows uses `cmd` + `["/C", "exit", "1"]`; Unix keeps `false`.
  - [ ] `test_command_with_input`: stdin echo. Windows candidate `cmd /C more` or `sort` (reads stdin, writes it back); Unix keeps `cat`. Needs a verified choice — see design questions.
- [ ] Phase 2: verify
  - [ ] Run the three tests from **PowerShell** (coreutils-free PATH) — must pass.
  - [ ] Run from Git Bash — must still pass.
  - [ ] `cargo xtask verify --skip-hub-build`.

## Design questions for Chris

1. **Shape of the fix.** Platform-gate the command strings with
   `cfg!(windows)` inside each test (smallest change), or factor a single
   `#[cfg(windows)]` / `#[cfg(unix)]` helper returning `(cmd, args, input)`
   tuples for the three cases (DRYer, one place to maintain)? Leaning toward
   the helper since all three share the same pattern.
2. **stdin-echo program on Windows.** `cmd /C more` mangles short input and
   adds a trailing form-feed in some shells; `sort` with no args reads stdin and
   echoes lines but reorders multi-line input (fine for the single-line test
   fixture). Which do you want — `sort`, `more`, `findstr "^"`, or a different
   approach? I'll verify the chosen one actually round-trips before committing.
3. **Scope.** This strand is the three `command` tests only. While running the
   full suite from PowerShell I can catch any *other* coreutils-dependent tests
   in the same pass — want me to file those as separate strands (the Windows
   test-fix campaign), or keep this one narrow?
