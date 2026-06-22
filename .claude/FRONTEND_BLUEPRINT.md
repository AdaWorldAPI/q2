# q2 frontend blueprint

## Verdict

Build the shell in **React + TypeScript**, bundle it once, and serve the static assets from the Rust binary.

That gives you the fastest path to the product you described:
- Rust owns the engine, notebook runtime, publisher, MCP, and local binary story.
- React owns the cockpit, linked-state UI, and fast iteration loop.
- The user still experiences **one binary**. They do not care that the paint was sprayed with a TypeScript gun.

If you chase a Rust-native frontend now, you risk building a concept car with no steering wheel.

## Why React wins the first lap

The UI is not a document viewer. It is a synchronized instrument cluster:
- query bar
- graph viewport
- detail sidebar
- dense result table
- reactive downstream cells
- exportable notebook document

That means you need:
- normalized shared state
- very fast selective re-renders
- mature graph and table integration
- a clean path for keyboard shortcuts, PDF mode, hydration, and SSR-ish static export behavior

React has the least friction here. Use it until the product earns a rewrite.

## Proposed stack

### App shell
- React
- TypeScript
- Vite for build
- Zustand for state
- TanStack Query only if you end up needing request caching beyond SSE snapshots

### Rendering
- Graph panel: `notebook-render` emits graph HTML/JS payloads, or use a thin D3/vis-network adapter for live mode
- Table panel: DataTables.js if you want parity with your spec quickly, or TanStack Table if you want stronger control and less jQuery gravity
- Charts/scalars: renderer-specific React wrappers around `notebook-render` payloads

### Transport
- SSE stream from `/mcp/sse`
- Command POSTs for edits and cell execution actions
- Client store merges notebook snapshots, cell states, and selection state

### Packaging
- `frontend/dist/*` embedded into the Rust binary with `include_dir`, `rust-embed`, or Axum static serving from compiled assets

## Screen architecture

Use a **cockpit grid**, not a notebook column.

### Layout
- Row 1: query bar + notebook controls
- Row 2: left overview rail, center graph, right intelligence sidebar
- Row 3: dense linked result table
- Row 4: downstream cell strip

### Panel roles

#### Top query bar
- single active-cell editor
- auto-detect language chip
- inline status: running, stale, synced
- notebook actions: export, save, open command palette

#### Left rail
- global filters
- metrics
- dataset scope
- parameter controls
- quick query presets

This is optional in the strict MVP, but it adds the Gotham/Bloomberg energy immediately.

#### Center graph
- dominant visual surface
- drag, zoom, lasso later
- click selection propagates everywhere
- graph is the primary interface, not decoration

#### Right sidebar
- selected node or edge details
- metadata
- provenance
- neighbor list
- mini trends
- “why is this selected?” breadcrumbs

#### Result table
- same entities as graph
- sortable
- filterable
- row click syncs to graph
- column color semantics for status

#### Cell strip
- upstream query cells
- downstream R / markdown / chart cells
- reactive DAG cues
- inline dependency status

## State model

Use one normalized store.

### Core entities
- `notebook`
- `cells`
- `results`
- `graphEntities`
- `tableRows`
- `selection`
- `layout`
- `transport`

### Key state slices

#### Notebook slice
- cell order
- dependency DAG
- active cell id
- notebook metadata

#### Execution slice
- execution state per cell: idle, queued, running, error, stale, fresh
- downstream invalidation map
- latest output revision

#### Selection slice
- selected node ids
- selected edge ids
- hovered ids
- focused row id
- brushed subset later

#### View slice
- graph camera
- sidebar tab
- table sort/filter state
- panel collapse state
- print/export mode

The crucial rule: **selection is global**, view-local rendering is derived.

## Interaction contract

Every panel subscribes to the same event grammar.

### Example events
- `CELL_EDITED`
- `CELL_EXECUTION_STARTED`
- `CELL_EXECUTION_FINISHED`
- `GRAPH_NODE_SELECTED`
- `TABLE_ROW_SELECTED`
- `FILTER_CHANGED`
- `NOTEBOOK_EXPORTED`

A node click should dispatch selection once. The graph, sidebar, table, and markdown note all derive from that same state. No panel should invent its own truth.

## MCP over SSE

Model the transport in two channels.

### Outbound
- `cell_create`
- `cell_update`
- `cell_execute`
- `cell_delete`
- `notebook_save`
- `notebook_load`
- `notebook_export`

### Inbound SSE events
- notebook snapshot
- cell status update
- cell result update
- graph selection hint if backend initiates it
- export completion / failure

Keep the wire protocol boring. The spectacle belongs in the UI.

## Rendering pipeline

### Live mode
1. user edits a cell
2. frontend emits `cell_update`
3. dependency graph marks downstream cells stale
4. runtime executes affected cells
5. SSE streams status transitions and rendered payloads
6. graph/table/sidebar update without page reload

### Publish mode
1. notebook snapshot frozen at revision N
2. frontend enters print/export layout
3. graphs render as SVG or static HTML fragments
4. tables switch to print-safe styles
5. code blocks syntax-highlighted
6. browser print or CLI publisher emits PDF

## PDF reality

For now, separate **interactive mode** from **publish mode**.

### Interactive mode
- hover states
- drag physics
- live filters
- animated transitions

### Publish mode
- no scrolling containers
- no clipped panels
- print CSS turns the cockpit into a structured report flow
- graph becomes a stable SVG block
- table paginates cleanly
- notes and code remain readable

Do not force the live cockpit layout directly into print. Freeze it into a composed reporting layout when exporting.

## Styling rules

### Palette
- background: `#0a0e17` to `#1a1f2e`
- primary accent: `#00bcd4` to `#4dd0e1`
- success: green
- warning: amber
- error: red

### Motion
- 200ms ease for panel and state transitions
- fade-in for new results
- subtle pulse for running cells
- avoid spinners unless there is no better status affordance

### Density
- no dead acreage
- no giant card padding
- no tab labyrinth
- monospace only where code or query text appears

## MVP cut that can actually ship

### Phase 1
- static cockpit layout
- fake graph + table linkage
- active query bar
- right sidebar updates on selection
- cell strip with fake reactive states

### Phase 2
- real SSE connection
- real cell execution lifecycle
- real notebook state
- graph renderer integration from `notebook-render`
- table renderer integration

### Phase 3
- print/export mode
- command palette
- saved notebooks
- multi-select and subgraph filtering
- lasso and brushing

## Recommendation in one line

**Rust for the machine. React for the dashboard. Ship the binary, not an ideology.**
