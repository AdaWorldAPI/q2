# Ship q2 — The Car

## What Exists (read before touching)

```bash
# The backend (DONE — compiles, serves, MCP works)
crates/quarto/src/main.rs           # CLI: q2 notebook serve → axum on :2718
crates/quarto/src/notebook_server.rs # 10 MCP tools, SSE, full CRUD
crates/quarto/src/publisher.rs       # HTML export, PDF via pandoc

# The cockpit design (DONE — 3 static prototypes, not wired)
cockpit-prototype/index.html         # vanilla JS, full layout
cockpit-prototype/cockpit-polished.html
cockpit-prototype/cockpit-tailwind.html
cockpit-prototype/cockpit.css
cockpit-prototype/cockpit.js

# The React infrastructure (DONE — but it's the Quarto Hub editor, not the cockpit)
hub-client/                          # Vite + React + TS, builds, has components

# The blueprint (DONE — architecture for the cockpit React app)
.claude/FRONTEND_BLUEPRINT.md        # Zustand state, SSE transport, panel roles

# The design reference
.claude/design-reference/            # 4 Palantir/Bloomberg screenshots
.claude/PRODUCT_VISION.md            # Cockpit energy, dense, professional

# The stub crates (placeholders for real transcodes)
crates/stubs/notebook-runtime/       # → will be replaced by AdaWorldAPI/marimo transcode
crates/stubs/notebook-query/         # → will be replaced by AdaWorldAPI/graph-notebook transcode
crates/stubs/notebook-kernel/        # → will be replaced by AdaWorldAPI/kernel-protocol transcode
crates/stubs/notebook-render/        # → will be replaced by AdaWorldAPI/aiwar transcode
crates/stubs/lance-graph/            # → will be replaced by real lance-graph
crates/stubs/q2-ndarray/             # → will be replaced by real ndarray
```

## What's Missing (the gap between "compiles" and "usable")

The backend works. The design exists as HTML prototypes. They're not connected.

A colleague should be able to:

1. `cargo install q2` (or `cargo build -p quarto`)
2. `q2 notebook serve`
3. Open `localhost:2718` in Chrome
4. See the cockpit (dark theme, graph panel, query bar, table, inspector)
5. Type a Gremlin query → see graph nodes appear
6. Click a node → inspector sidebar updates, table row highlights
7. Type an R cell → see table output
8. Press ⌘P → get a PDF

Right now step 3 shows a placeholder page with three endpoint URLs.

## What To Do

### Step 1: Build the cockpit as a React app

Create `cockpit/` at the repo root (separate from `hub-client/` which is the Quarto Hub editor).

```
cockpit/
├── package.json          # React, Vite, Zustand, TypeScript
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/
│   ├── main.tsx
│   ├── App.tsx           # Cockpit grid layout
│   ├── store.ts          # Zustand: notebook, cells, selection, execution
│   ├── transport.ts      # SSE from /mcp/sse, POST to /mcp/message
│   ├── components/
│   │   ├── QueryBar.tsx      # Language auto-detect chip, execute button
│   │   ├── GraphPanel.tsx    # vis-network or d3 force graph, click → select
│   │   ├── Inspector.tsx     # Selected node properties, connections
│   │   ├── ResultTable.tsx   # Dense sortable table, row click → select
│   │   ├── CellStrip.tsx     # Notebook cells below, reactive status dots
│   │   └── LeftRail.tsx      # Filters, parameters (optional for MVP)
│   └── styles/
│       └── cockpit.css       # Dark theme from cockpit-prototype/cockpit.css
```

Use `cockpit-prototype/cockpit.css` and `cockpit-prototype/cockpit.js` as the
design source. The colors, typography, layout, and interaction patterns are
all defined there. Translate them into React components.

The state model is in `.claude/FRONTEND_BLUEPRINT.md`. Read it.
Selection is global. Every panel derives from the same store.

### Step 2: Wire transport to MCP

The backend already serves MCP on `/mcp/sse` (GET) and `/mcp/message` (POST).
Read `notebook_server.rs` to see the exact request/response shapes.

```typescript
// transport.ts
const sse = new EventSource('/mcp/sse');
sse.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  // Update store with notebook state, cell results, etc.
};

async function callTool(name: string, args: object) {
  const res = await fetch('/mcp/message', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'tools/call',
      params: { name, arguments: args }
    })
  });
  return res.json();
}

// Example: execute a cell
const result = await callTool('cell_execute', {
  code: "g.V().hasLabel('server').outE().inV().path()",
  lang: 'gremlin'
});
```

### Step 3: Graph rendering

For MVP, use vis-network (npm `vis-network`). It handles force layout,
drag, zoom, click events, and looks professional with minimal config.

When `notebook-render` (AdaWorldAPI/aiwar) is transcoded, swap to its
output. For now, take the query result JSON and feed it to vis-network.

```typescript
// GraphPanel.tsx
import { Network } from 'vis-network';

// When cell result has nodes/edges, render them
useEffect(() => {
  if (result?.outputs?.some(o => o.type === 'graph')) {
    const network = new Network(container, { nodes, edges }, options);
    network.on('click', (params) => {
      if (params.nodes.length > 0) {
        store.selectNode(params.nodes[0]);
      }
    });
  }
}, [result]);
```

### Step 4: Embed in the binary

After `npm run build` in `cockpit/`, the output is `cockpit/dist/`.
Serve it from axum:

```rust
// notebook_server.rs — replace the placeholder
// Option A: static directory at runtime
app = app.nest_service("/", ServeDir::new("cockpit/dist"));

// Option B: embed at compile time
// use include_dir::include_dir;
// static FRONTEND: Dir = include_dir!("cockpit/dist");
```

For now, Option A is fine. The colleague runs:
```bash
cd cockpit && npm install && npm run build && cd ..
cargo run -p quarto -- notebook serve --frontend-dir cockpit/dist
```

Later, embed with `rust-embed` or `include_dir` for a true single binary.

### Step 5: Make the stub crates do something useful

Right now the stubs return placeholder data. Make them return enough
for the cockpit to look alive:

**notebook-query**: `execute()` should return a fake graph with 10 nodes
and 15 edges when it receives any Gremlin/Cypher query. Later, wire to
real Neo4j via bolt-client or to lance-graph for local execution.

**notebook-render**: `render_table()` should produce actual HTML with
DataTables.js. The function signature already exists in the stub.

**notebook-runtime**: `Runtime` already has cell CRUD. Make the DAG
tracking work — when a cell's output changes, mark downstream cells stale.

### Step 6: PDF export that works

`publisher.rs` already generates HTML and shells out to pandoc for PDF.
Make the HTML output use the cockpit dark theme with a print media query
that flips to light:

```css
@media print {
  body { background: white; color: black; }
  .graph-panel { /* render as static SVG */ }
  .table-panel { /* clean print table styles */ }
}
```

### Step 7: Verify the full loop

```bash
# Build
cd cockpit && npm install && npm run build && cd ..
cargo build -p quarto

# Run
./target/debug/quarto notebook serve --frontend-dir cockpit/dist --open

# Test
# 1. Browser opens localhost:2718
# 2. Type a Gremlin query in the query bar
# 3. See graph nodes appear in the center panel
# 4. Click a node → inspector updates
# 5. See the result table below
# 6. Press ⌘P → PDF preview looks professional
```

## What NOT To Do

- Don't touch hub-client/ — that's the Quarto Hub editor, separate product
- Don't rewrite the backend — it works
- Don't wait for real transcodes — use stubs with fake data
- Don't build a settings page, user management, or auth — ship first
- Don't build the LeftRail for MVP — add it after the core loop works
- Don't optimize — make it work, make it right, then make it fast
