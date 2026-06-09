# `verbose_to_filter` matches no first-party workspace targets

**Date:** 2026-05-24
**Status:** documented, no fix attempted
**Discovered while:** adding a post-render summary line to `q2 render`
(see `crates/quarto/src/commands/render.rs` migration to
`quarto_util::user_status!`)

## TL;DR

The `-v` flag on `q2` and `hub` does not actually surface any
first-party `tracing` events from the workspace. The directives
emitted by `quarto_util::verbose_to_filter` (`quarto=warn`,
`quarto=info`, etc.) refer to a tracing target prefix `quarto`, but
every first-party crate that uses `tracing` emits events under a
different target (`q2::…`, `hub::…`, `quarto_core::…`,
`quarto_preview::…`). The directives match no first-party target
and only ever enable third-party deps (`samod`, `tower_http`).

This is a **silent observability gap**: the macros run, the events
are constructed, the subscriber is installed — and then `EnvFilter`
discards them. No error, no warning, no output.

The fix needs design: which crates should `-v` enable, at what
verbosity levels, and should non-`q2` binaries (`hub`, `pampa`,
`qmd-syntax-helper`, `validate-yaml`) share the mapping or have
their own. This document records the situation; design is out of
scope.

## How `EnvFilter` target matching works

A directive like `quarto=info` means "set max level `info` for events
whose target starts with the path segment `quarto`". The matching
is on `::`-separated path segments. `quarto=info` matches a target of
exactly `quarto`, or anything beginning with `quarto::` (e.g.
`quarto::commands::render`). It does **not** match `quarto_core`,
`quarto_hub`, `quarto_util`, etc. — those are distinct top-level
identifiers, not children of `quarto`.

Each `tracing` macro defaults its `target` to the `module_path!()`
of the call site. The first segment of `module_path!()` is the
**crate root name**:

- For a `[lib]` crate, the root name is the package name with
  hyphens converted to underscores (e.g. `quarto-core` →
  `quarto_core`).
- For a `[[bin]]` target, the root name is the **bin's `name`**,
  not the package name.

## What the workspace actually emits

### Subscriber-installing binaries

Only two binaries install a `tracing-subscriber` today, and both
route their `-v` flag through `quarto_util::verbose_to_filter`:

| Binary | Package | Bin name | Root segment of `module_path!()` | Matched by `quarto=…`? |
|---|---|---|---|---|
| `q2` | `quarto` | `q2` | `q2` | **no** |
| `hub` | `quarto-hub` | `hub` | `hub` | **no** |

`crates/quarto/src/main.rs:560-566` and
`crates/quarto-hub/src/main.rs:131-136` both build:

```rust
tracing_subscriber::registry()
    .with(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| quarto_util::verbose_to_filter(verbose).into()),
    )
    .with(tracing_subscriber::fmt::layer())
    .init();
```

`verbose_to_filter` (`crates/quarto-util/src/verbose.rs:29-36`):

```rust
pub fn verbose_to_filter(count: u8) -> &'static str {
    match count {
        0 => "quarto=warn",
        1 => "quarto=info",
        2 => "quarto=debug,samod=info",
        _ => "quarto=trace,samod=debug,tower_http=debug",
    }
}
```

### Non-installing binaries

`pampa`, `qmd-syntax-helper`, `validate-yaml`, and the `xtask` /
`perf-harness` / `reconcile-viewer` helpers do not install any
subscriber at all. Their `tracing` events are dropped at the
no-op global subscriber. Separate bug; not the focus of this note.

### Crates that emit `tracing` events

Approximate call-site counts (grep over `crates/*/src/`,
`info!|warn!|debug!|trace!|error!`, excluding tests and the macro
import line):

| Crate | Calls | Root target segment | Matched today? |
|---|---|---|---|
| `quarto` (bin `q2`) | 9 | `q2` | no |
| `quarto-core` (lib) | 31 | `quarto_core` | no |
| `quarto-hub` (bin `hub`) | 110 | `hub` | no |
| `quarto-preview` (lib) | 19 | `quarto_preview` | no |

(Other first-party crates use `tracing` only via dependencies; no
direct calls.)

Third-party crates that **are** enabled today:

- `samod` (collaborative editing): `info` at `-vv`, `debug` at `-vvv`.
- `tower_http` (hub HTTP server): `debug` at `-vvv`.

## Empirical confirmation

Reproduced on `main` at commit `2b21ee60`:

| Command | Per-file `Output: ...` lines visible? |
|---|---|
| `q2 render docs/` | no |
| `q2 render docs/ -v` (resolves to `quarto=info`) | no |
| `q2 render docs/ -vvv` (resolves to `quarto=trace,…`) | no |
| `RUST_LOG=quarto=trace q2 render docs/` | no |
| `RUST_LOG=q2=info q2 render docs/` | **yes** |

The `info!("Output: {}", …)` call at
`crates/quarto/src/commands/render.rs:823` emits an event with
target `q2::commands::render`. None of the `quarto=…` directives
match it. Only an explicit `RUST_LOG=q2=…` (with the bin name)
surfaces it.

The same applies to the `quarto-core` `info!`/`debug!` calls used
during rendering — they have target `quarto_core::…`, which also
fails to match `quarto=…`.

## Consequences

1. **`-v` is a no-op for first-party observability.** Users
   reaching for `-v` to "see what's happening" get nothing new
   (until `-vv` or higher, which enables `samod` if they happen to
   be running a hub).
2. **Newly added `tracing::info!` calls have no audience.** A dev
   adds `info!("did the thing")`, sees nothing with `-v`, may
   resort to `eprintln!`, then later wonder why. This is exactly
   how the `Output: ...` per-file lines and the "Rendering
   project: ..." banner ended up effectively dead-code-for-users
   despite looking intentional in source.
3. **The `user_status!` migration (2026-05-24) sidesteps the bug
   for user-facing messages** but does not fix it. Developer
   telemetry still has no working channel below `RUST_LOG`.
4. **No subscriber on `pampa` / `qmd-syntax-helper` / etc.** means
   their events go to the no-op global subscriber. Separate problem
   in the same family.

## Design questions for the audit

1. **Which crates should `-v` enable, and at what levels?** Today's
   directive uses `quarto` as the umbrella — but the workspace has
   many crates and "umbrella" is conceptual, not structural.
   Candidates:
   - A wildcard target list per level (`q2,quarto_core,quarto_hub,
     quarto_preview,pampa,…=info`). Concrete but brittle as new
     crates land.
   - A custom `Filter` (not just `EnvFilter`) that recognises a
     "first-party" predicate over `metadata().target()` — e.g. any
     target whose first segment is in a curated set.
   - Move all first-party tracing under a synthetic prefix via
     explicit `target: "quarto::…"` on every macro call. Most
     invasive; defeats the auto-`module_path!()` ergonomics.
2. **Should each binary share `verbose_to_filter`, or define its
   own?** `q2` and `hub` have very different observability needs
   (CLI render vs long-running server). A shared mapping that's
   good for one tends to be wrong for the other (e.g. `samod=info`
   is irrelevant to `q2`).
3. **Should non-subscriber binaries install one?** `pampa`,
   `qmd-syntax-helper`, etc. silently drop events today. Either
   install a subscriber (and pay the per-binary boilerplate cost)
   or accept that those binaries are not instrumented.
4. **What does `--quiet` mean for `tracing`?** Today it's checked
   only at explicit call sites (now via `user_status!`). Should
   `--quiet` also raise the `EnvFilter` floor to `warn` (or
   `error`) so a third-party `info!` from a dep doesn't leak past
   it?
5. **`RUST_LOG` precedence: should we keep `try_from_default_env`
   as the override hatch?** It is today. Worth documenting in a
   user-facing `q2 --help` note once the default mapping actually
   does something.

## Why this isn't being fixed in the same change

The summary-line change (`render_summary_line` + `user_status!`)
needed a working channel for the post-render summary. The straight
path was a small `user_status!` helper that bypasses `EnvFilter`
entirely. Fixing the filter mapping correctly requires:

- An audit of which existing `tracing` calls in `quarto` /
  `quarto-core` / `quarto-hub` / `quarto-preview` are intended for
  user visibility vs developer telemetry (some of the
  `quarto-core` `debug!` calls are legitimate debug output;
  surfacing them at `-v` would be wrong).
- A decision on the structural question (curated target list vs
  custom filter vs target-prefix renaming).
- Coordination with the hub binary, which has 110 call sites and
  its own audience.

That's a bigger change than warrants bundling with a CLI-summary
addition. Filed as a beads issue.

## Files / locations referenced

- `crates/quarto-util/src/verbose.rs:29-36` — `verbose_to_filter`.
- `crates/quarto/src/main.rs:560-566` — `q2` subscriber init.
- `crates/quarto-hub/src/main.rs:131-136` — `hub` subscriber init.
- `crates/quarto/src/commands/render.rs:823` — example
  `info!("Output: …")` call that is currently unreachable from `-v`.
- `crates/quarto-util/src/user_status.rs` — the user-facing-output
  workaround introduced 2026-05-24.
