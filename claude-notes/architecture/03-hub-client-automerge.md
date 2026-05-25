# Diagram 3 — hub-client Automerge Structure & WASM Preview

**SVG:** [`automerge.svg`](./automerge.svg) · **Set index & conventions:** [`README.md`](./README.md)

Companion diagrams: [Render pipeline](./01-pipeline.md) ·
[Crate & package map](./02-crates.md) ·
[q2 vs hub-client (build & WASM)](./04-q2-preview-wasm.md).

---

## How to read this

Same three-tier drill-down (**diagram → guide → source**). The diagram has two
halves: **(A)** how a Quarto project is represented as an Automerge document
(the CRDT schema), and **(B)** the WASM rendering infrastructure that turns
those documents into a live preview. Numbered markers ①② point to the
[Notes](#notes).

`hub-client` is Quarto 2's collaborative writer. State is held in
[Automerge](https://automerge.org/) CRDT documents synced over WebSocket;
rendering happens in-browser via the `wasm-quarto-hub-client` `.wasm`
(see [diagram 2](./02-crates.md)).

## A. The Automerge document model

Schema types live in `ts-packages/quarto-automerge-schema/src/index.ts`.

### `ProjectSetDocument` — the user's project list (root)

Per-user, synced across browsers; each browser stores only *this document's id*
in IndexedDB.

```
ProjectSetDocument {
  projects: Record<key, ProjectSetEntry>   // key = indexDocId without 'automerge:' prefix
  version:  number                          // CURRENT_PROJECT_SET_SCHEMA_VERSION = 1
}
ProjectSetEntry {
  indexDocId:   string   // 'automerge:'-prefixed id of the project's IndexDocument
  syncServer:   string   // WebSocket URL hosting the project
  description:  string
  addedAt:      string   // ISO timestamp
  lastAccessed: string   // ISO timestamp
}
```

`projects[key].indexDocId` → an **`IndexDocument`**.

### `IndexDocument` — one project (root of a project)

```
IndexDocument {
  files:      Record<path, docId>            // path -> Automerge docId of the file
  version?:   number                          // CURRENT_SCHEMA_VERSION = 2
  identities?: Record<actorId, ActorIdentity> // collaborators (V1+)
  captures?:  Record<path, CaptureRef>        // engine-capture sidecar (V2+)
}
ActorIdentity { name: string; color: string }            // cursor color, e.g. "#E91E63"
CaptureRef    { captureDocId: string; staleness?: boolean;
                state?: 'idle'|'running'|'error'; lastError?: string }
```

- `files[path]` → a **file document** (separate Automerge doc, keyed by `docId`).
- `captures[path].captureDocId` → a **capture document** (serialized engine output).
- `migrateIndexDocument()` migrates V0→V1→V2 idempotently (see Note ①).

### File documents (one Automerge doc per file)

```
FileDocumentContent =
  | TextDocumentContent   { text: string }                       // Automerge Text (CRDT)
  | BinaryDocumentContent { content: Uint8Array; mimeType; hash } // hash = SHA-256, for dedup
```

Text files (`.qmd`, `.yml`) are collaborative `Text`; binaries (images, PDFs)
are content-addressed blobs.

### Capture documents (one per `captureDocId`)

A separate Automerge doc holding a serialized `EngineCapture` — recorded
post-engine output the preview replays instead of re-running engines in the
browser (`capture-splice` in [diagram 1](./01-pipeline.md); the `q2 preview`
server writes these — see [diagram 4](./04-q2-preview-wasm.md)).

## B. Live-preview WASM infrastructure

### Sync layer

`@quarto/quarto-sync-client` wraps an Automerge `Repo`
(`@automerge/automerge-repo`) configured with:

| Adapter kind | Browser | Node (auth/tests) |
|---|---|---|
| **Network** | `BrowserWebSocketClientAdapter` | `NodeWebSocketClientAdapter` |
| **Storage** | `IndexedDBStorageAdapter` | `MemoryStorageAdapter` |

The network adapter connects to a sync server's WebSocket endpoint (`/ws`). The
client exposes callbacks: `onFileAdded`, `onFileChanged(path, text, patches)`,
`onBinaryChanged`, `onFileRemoved`, `onIdentitiesChange`, `onCapturesChange`,
`onConnectionChange`. **Presence** (live cursors/selections) is a *separate*
ephemeral-message channel (`presenceService.ts`), not document changes.

### Render data flow (the numbered path)

1. An Automerge doc change fires `onFileChanged(path, text)` in the sync client.
2. The hub-client service mirrors it into the **WASM VFS**: `vfs_add_file(path, text)` (paths use the `/project/` prefix — see Note ②).
3. The preview calls a WASM render entry point, e.g. `render_page_in_project_with_attribution(path, grammars, attribution)`.
4. Inside WASM, the **same `quarto-core` pipeline** runs in q2-preview mode over the VFS (see [diagram 1](./01-pipeline.md) — drops the HTML-emitting stages, splices recorded captures).
5. WASM returns the **Pandoc AST as JSON** (plus diagnostics, theme fingerprint, attribution).
6. The React renderer (`@quarto/preview-renderer`) renders the AST in an iframe; `/.quarto/…` artifact requests are served from the VFS (no network).

### WASM entry-point surface (`wasm-quarto-hub-client`)

Authoritative `#[wasm_bindgen]` exports, grouped:

- **Render:** `render_qmd`, `render_qmd_content`, `render_page_in_project`, `render_page_in_project_with_attribution`, `render_page_for_preview`
- **Parse / convert:** `parse_qmd_content`, `parse_qmd_to_ast`, `parse_qmd_to_ast_with_attribution`, `ast_to_qmd`, `incremental_write_qmd`
- **VFS:** `vfs_add_file`, `vfs_add_binary_file`, `vfs_remove_file`, `vfs_clear`, `vfs_list_files`, `vfs_read_file`, `vfs_read_binary_file`, `vfs_set_runtime_metadata`, `vfs_get_runtime_metadata`
- **LSP:** `lsp_analyze_document`, `lsp_get_diagnostics`, `lsp_get_symbols`, `lsp_get_folding_ranges`
- **Theme / SCSS:** `compile_theme_css_by_name`, `compile_scss`, `compile_scss_with_bootstrap`, `compile_default_bootstrap_css`
- **Project:** `create_project`, `get_project_choices`, `get_builtin_template`, `prepare_template`, `init`

### hub-client services (`hub-client/src/services/`)

`authService`, `presenceService`, `projectSetService` / `projectSetReconciler` /
`projectSetStorage`, `projectStorage`, `resourceService`, `templateService`,
`intelligenceService` (LSP), `monacoProviders`, `userSettings`,
`attribution-runs`, `tsxTranspiler`, `debugApi`.

---

## Notes

### ① The Automerge schema is versioned and migrated in place — *detail*

`IndexDocument.version` is `2`; `migrateIndexDocument()` runs V0→V1 (initialize
`identities`) then V1→V2 (activate the `captures` sidecar) idempotently, inside
an Automerge `change()`. `ProjectSetDocument.version` is `1`. Old documents
lacking `version`/`identities` are valid and upgraded on first write.
→ `ts-packages/quarto-automerge-schema/src/index.ts`.

### ② The VFS is an ephemeral, one-way replica of Automerge — *amber*

The Automerge documents are the source of truth. The WASM **VFS** is an
ephemeral replica rebuilt from Automerge on connect: data flows **Automerge →
VFS** (via `vfs_add_file`), never the reverse. The VFS is cleared on disconnect.
All VFS paths use the `/project/` prefix; build artifacts are served from
`/.quarto/…` within the VFS. Do not treat the VFS as durable state.
→ `crates/wasm-quarto-hub-client/src/lib.rs`,
`hub-client/src/services/` (the sync→VFS mirroring).
