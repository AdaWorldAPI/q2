# Quarto Hub MCP — bundle handoff guide

*Written 2026-06-19. Goal: hand off everything a colleague needs to bundle the
Quarto Hub MCP server into a separate TypeScript application.*

## TL;DR

The MCP server is a **standalone TypeScript npm package**:
`ts-packages/quarto-hub-mcp` (package name `@quarto/hub-mcp`). It speaks MCP over
**stdio** and is fully self-contained except for one native addon
(`@napi-rs/keyring`). It is *not* coupled to the `q2` binary — `q2 mcp` is just a
thin Rust launcher that embeds a pre-built esbuild bundle and shells out to Node.
A colleague embedding the MCP elsewhere should consume the **TypeScript package
directly** (or the `dist-bundle/` artifact), and can ignore the Rust launcher
entirely.

## 1. The MCP server package — `ts-packages/quarto-hub-mcp`

`package.json` (`ts-packages/quarto-hub-mcp/package.json`):

```json
{
  "name": "@quarto/hub-mcp",
  "version": "0.0.1",
  "private": true,
  "type": "module",
  "main": "dist/index.js",
  "types": "src/index.ts",
  "bin": { "quarto-hub-mcp": "./dist/index.js" },
  "exports": {
    ".": {
      "types": "./src/index.ts",
      "source": "./src/index.ts",
      "import": "./dist/index.js"
    }
  },
  "files": ["dist"],
  "scripts": {
    "build": "tsc && chmod 0755 dist/index.js",
    "bundle": "node scripts/bundle.mjs",
    "typecheck": "tsc --noEmit",
    "clean": "rm -rf dist dist-bundle",
    "test": "vitest run"
  },
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.12.1",
    "@napi-rs/keyring": "^1.3.0",
    "@quarto/quarto-automerge-schema": "*",
    "@quarto/quarto-sync-client": "*",
    "jose": "^6.0.11",
    "oauth4webapi": "^3.5.5"
  }
}
```

Key facts:

- **MCP SDK**: `@modelcontextprotocol/sdk` ^1.12.1, using `Server` +
  `StdioServerTransport` (`src/index.ts`). Transport is **stdio** — stdout is the
  JSON-RPC channel, so nothing in the server may write to stdout outside the
  protocol.
- **Entry point**: `src/index.ts` (shebang `#!/usr/bin/env node`). It parses args
  (`--server`, `--read-only`, `--redirect-port`), wires up
  `ConnectionManager` (`src/connection-manager.ts`), registers tools
  (`src/tools.ts` → `registerTools`), and sets up OAuth/auth state
  (`src/auth/*`).
- **Default sync server**: `DEFAULT_SERVER_URL = 'wss://quarto-hub.com/ws'`
  (exported from `src/index.ts`), overridable via `--server` or
  `QUARTO_HUB_SERVER`.
- **Two intra-repo workspace deps**, consumed by npm workspace wildcard `"*"`:
  - `@quarto/quarto-sync-client` (`ts-packages/quarto-sync-client`) — automerge
    sync client / websocket transport.
  - `@quarto/quarto-automerge-schema` (`ts-packages/quarto-automerge-schema`) —
    the shared document schema.
  These are pulled in **from TypeScript source** during bundling (see the
  `source` esbuild condition below), so the bundle never embeds a stale `dist/`
  of either.

### Tool surface (`src/tools.ts`)

The server registers these MCP tools (plus the auth tools from
`src/auth/auth-tools.ts`):

`connect_project`, `list_files`, `read_file`, `wait_for_change`, `write_file`,
`patch_file`, `create_file`, `delete_file`, `rename_file`, `create_project`.

These match the `mcp__quarto-hub__*` tools surfaced in this session, plus auth
tools (`authenticate`, `authenticate_clear`).

## 2. How the bundle is built — `scripts/bundle.mjs`

`npm run bundle` → `node scripts/bundle.mjs`. It produces a self-contained
directory `dist-bundle/`:

```
dist-bundle/
  index.mjs                       — the bundled server (esbuild, ~5 MB)
  build-info.json                 — git commit + build time + node target + keyring pkgs
  node_modules/@napi-rs/keyring*  — the keyring native addon (kept EXTERNAL)
```

The esbuild invocation (verbatim from `scripts/bundle.mjs`):

```js
await esbuild.build({
  entryPoints: [join(pkgRoot, 'src/index.ts')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  outfile: join(outDir, 'index.mjs'),
  conditions: ['source'],            // compile workspace deps from TS source
  external: ['@napi-rs/keyring'],    // native addon stays out of the bundle
  banner: {                          // CJS deps (e.g. ws) need a require shim in ESM
    js: [
      "import { createRequire as __q2bundleCreateRequire } from 'node:module';",
      'const require = __q2bundleCreateRequire(import.meta.url);',
    ].join('\n'),
  },
  plugins: [automergeBase64Plugin],  // steer @automerge/automerge to its base64 wasm entry
  logLevel: 'info',
});
```

Three non-obvious bundling decisions (all documented in the script header) that a
re-bundler **must replicate**:

1. **`@napi-rs/keyring` is external.** It's a native `.node` addon and cannot be
   inlined. The script copies the loader package plus the matching
   platform-specific `.node` package(s) into a mini `node_modules/@napi-rs/`
   *inside* `dist-bundle/`, so Node's normal resolution (relative to
   `index.mjs`) finds them at runtime. Staging logic lives in
   `scripts/stage-keyring.mjs`:
   - Dev machine (no `KEYRING_PLATFORMS` env): stages whatever platform package
     the local lockfile installed (one on a normal box).
   - Release jobs set `KEYRING_PLATFORMS` (e.g. `darwin-x64,darwin-arm64`) and the
     script `npm pack`s any missing platform package at the loader's exact
     version. **Fail-closed**: a requested platform that can't be staged aborts
     the build; every staged platform package must carry a `.node`.
2. **`@automerge/automerge` wasm.** Its default `node` export loads wasm via
   `__dirname`-relative `readFileSync`, which can't survive bundling. The
   `automergeBase64Plugin` resolves the bare `@automerge/automerge` import to the
   sibling `fullfat_base64.js` entrypoint (wasm inlined as base64). `/slim`
   subpaths are left alone (they share the singleton). If you re-bundle, keep
   this plugin or wasm loading breaks at runtime.
3. **`source` condition** makes esbuild compile the `@quarto/*` workspace deps
   from their `.ts` sources directly, so the bundle can't embed a stale `dist/`.

`build-info.json` stamps `gitCommit`, `gitDirty`, `builtAt`, `nodeTarget`
(`node24`), and `keyringPackages` — the stale-embed tripwire.

### Where it's wired in xtask

`cargo xtask build-hub-mcp-bundle` (`crates/xtask/src/build_hub_mcp_bundle.rs`)
just runs `npm run bundle` in `ts-packages/quarto-hub-mcp`. It's also part of
`cargo xtask build-all` (ordered before the Rust build). `dist-bundle/` is
**gitignored** (not committed) — it's a build artifact.

`cargo xtask verify` separately builds the package's `dist/` (plain `tsc`) and
smoke-checks `node dist/index.js --help` to ensure the module graph resolves
under ESM — see `crates/xtask/src/verify.rs:223-272`.

## 3. How `q2 mcp` embeds + runs the bundle (Rust launcher)

Only relevant if the colleague wants to mirror the *embedding* trick; for reuse
in a TS app it's irrelevant. Crates:

- `crates/quarto/src/commands/mcp.rs` — 1-line shim: `quarto_mcp_launcher::run(args)`.
- `crates/quarto-mcp-launcher/` — the real launcher:
  - `build.rs` — at compile time, sets `QUARTO_HUB_MCP_EMBED_DIR` to
    `ts-packages/quarto-hub-mcp/dist-bundle/` **if `index.mjs` exists**, else to a
    placeholder dir containing a `BUNDLE_NOT_BUILT` marker (so fresh clones
    still `cargo build`; `q2 mcp` then fails at runtime with an actionable
    message). Emits per-file `rerun-if-changed`.
  - `src/bundle.rs` — `include_dir!("$QUARTO_HUB_MCP_EMBED_DIR")` embeds the
    whole directory into the binary. `content_hash()` = 16 hex of SHA-256 over
    the sorted (path, len, contents) — used as the cache dir name.
  - `src/lib.rs::run()` — extracts the embedded files to a per-user cache
    (`QUARTO_MCP_CACHE_DIR` override), finds Node, and **delegates** to
    `node <cache>/index.mjs <args…>` (on Unix it `exec`s and never returns; on
    Windows it forwards the child exit code). All args pass through verbatim.
  - `src/node.rs` — Node discovery. **`MIN_NODE_MAJOR = 24`** (matches the
    `node24` esbuild target). `QUARTO_NODE` overrides PATH discovery.
  - `src/delegate.rs`, `src/cache.rs`, `src/defaults.rs` — exec/locking/GC and
    bundled `quarto-hub.com` env-var defaults (release builds inject hub
    connection defaults for any var the user hasn't set).

### Launcher cache + node discovery details (if mirroring the embed pattern)

- **Cache layout** (`src/cache.rs`): `~/.cache/quarto/hub-mcp/<content-hash>/`
  (via the `dirs` crate; on macOS `~/Library/Caches/quarto/hub-mcp/`), override
  with `QUARTO_MCP_CACHE_DIR`. Extraction is crash-safe (extract to `.tmp-*`,
  atomic rename) and idempotent (reuse if `<hash>/` exists). Each running
  instance holds a **shared advisory lock** on `<hash>/.lock`; the fd survives
  `exec` into node (FD_CLOEXEC cleared on Unix) so the kernel releases it
  exactly when node exits. GC is opportunistic at launch — try-exclusive-lock
  each sibling hash dir, delete if unused and older than 14 days
  (`DEFAULT_MAX_AGE`).
- **Layered node discovery** (`src/node.rs`): `QUARTO_NODE` env → `node` on PATH
  → well-known locations (`/opt/homebrew/bin`, `/usr/local/bin`, volta/fnm/nvm
  version-manager dirs, Windows Program Files). This layering exists because
  **GUI-launched MCP hosts (e.g. Claude Desktop) run with a minimal environment,
  not the user's shell PATH** — so a bare `node`-on-PATH lookup often fails
  there and `QUARTO_NODE` is the escape hatch.

Diagnostics: `q2 mcp --launcher-info` prints the embedded bundle hash,
`build-info.json`, cache root, and discovered node — the stale-embed tripwire.
`q2 mcp --print-config` prints a ready-to-paste `.mcp.json`. The launcher **never
writes stdout** except for these explicit pre-protocol queries.

> **Stale-embed gotcha** (from CLAUDE.md): a plain `cargo build --bin q2`
> re-embeds whatever `dist-bundle/` was last produced. After changing the TS,
> run `cargo xtask build-hub-mcp-bundle` *then* `cargo build --bin q2`.

## 4. Runtime requirements

- **Ambient Node.js ≥ 24** (the bundle targets `node24`; the launcher enforces
  major ≥ 24). No bundled Node — the host must have it on PATH (or via
  `QUARTO_NODE` for the Rust launcher path).
- **The keyring native addon** is the only non-inlinable dependency. Whatever
  packaging a downstream app uses, the platform-matching
  `@napi-rs/keyring-<platform>` `.node` must sit in a `node_modules/@napi-rs/`
  resolvable relative to `index.mjs`. If the consuming app already has
  `@napi-rs/keyring` in its own `node_modules`, that resolves it too — keyring is
  used for OS credential storage (`src/auth/credential-store.ts`).
- Everything else (MCP SDK, jose, oauth4webapi, automerge, ws, the `@quarto/*`
  workspace deps) is inlined into `index.mjs`.

## 5. Auth / config a re-bundler must know (`src/auth/*`, design doc bd-81cfshmw)

Env vars the server reads (from `src/index.ts` header):

| Variable | Meaning |
|---|---|
| `QUARTO_HUB_SERVER` | Sync server URL (overridden by `--server`) |
| `QUARTO_HUB_MCP_CLIENT_ID` | Operator-supplied Google OAuth client id |
| `QUARTO_HUB_MCP_CLIENT_SECRET` | Matching client secret |
| `QUARTO_HUB_MCP_ISSUER` | OIDC issuer (default `https://accounts.google.com`) |
| `QUARTO_HUB_MCP_ALLOW_INSECURE_AUTH` | `1` to allow Bearer over plain HTTP / http loopback issuers (dev only) |

The server does OAuth (PKCE loopback flow — `src/auth/loopback.ts`,
`src/auth/pkce.ts`, `src/auth/browser.ts`), stores tokens in the OS keyring via
`@napi-rs/keyring`, and refreshes them (`src/auth/refresh-manager.ts`). A
separate app reusing the server still needs OAuth client credentials and a
reachable hub. CLI flags: `--server <wss-url>`, `--read-only`, `--redirect-port
<1024-65535>`.

Full design rationale (why TS-canonical + Rust thin launcher, auth flow, npx
channel plan): `claude-notes/plans/2026-06-11-q2-mcp-hub-auth.md`.

## Recommended handoff path for the colleague

1. **Reuse the npm package directly.** Depend on `@quarto/hub-mcp` (or vendor
   `ts-packages/quarto-hub-mcp`) and import from `src/index.ts` / run
   `dist/index.js`. The server is transport-agnostic at the MCP layer (stdio).
2. **Or reuse `dist-bundle/index.mjs`** as a self-contained drop-in — just ship
   the sibling `node_modules/@napi-rs/keyring*` for the target platform(s) and run
   it with Node ≥ 24.
3. Replicate the **three bundling decisions** (keyring external + staged,
   automerge base64 plugin, `source` condition) if re-running esbuild themselves;
   `scripts/bundle.mjs` is the canonical reference.
4. Ignore the Rust launcher unless they specifically want the embed-in-a-binary
   pattern (`include_dir!` + per-user cache + node delegation).
