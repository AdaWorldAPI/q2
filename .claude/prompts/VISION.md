# q2 — Design the Frontend

## What This Is

q2 is a graph notebook. One Rust binary. Data engineers open it in a
browser, type Gremlin/Cypher/R/SPARQL, see interactive graphs, export
to PDF. Think: if Palantir Gotham and Neo4j Browser had a baby that
ran locally as a single binary.

## The Problem

We have the engine. We don't have the car. Our colleague said: "even
if you have a plasma engine it doesn't help if you don't have a car."

Right now nobody can use what we built. A data engineer at our partner
company (Bardioc) knows Gremlin and R. They need to type a query, see
their graph, click around, export a report. They don't care about
semiring algebra or SIMD kernels.

## Design References

Look at these screenshots in rs-graph-llm .claude/design-reference/:

### ref_01_palantir_gotham.png
Military C2 interface. Dark theme, teal/cyan accents. Multi-panel:
situation panel left, map center, course of action right, timeline
bottom. Four views of the same data, all live, all linked. Click a
node → every panel updates.

THIS IS THE ENERGY. Not a chat window. Not a blank page. A cockpit.

### ref_02_financial_dashboard.png
Bloomberg-level information density. Charts, gauges, metrics,
projections — everything on one screen. No scrolling. No tabs.
Color = meaning (green/red/amber).

### ref_03_risk_map.png
The scatter plot IS the interface. Select points → table filters →
sidebar updates. Visualization is primary. Data table is secondary.
Selection drives everything.

### ref_04_metabase.png
Parameters on the left drive views on the right. Histogram, map,
table — all linked. Change a filter, everything reflows. Clean,
non-technical users feel comfortable.

## What the Frontend Must Do

A data engineer opens localhost:2718 in their browser. They see:

1. A query bar at the top. They type. Language auto-detects.
   `g.V().hasLabel('server').outE().inV().path()` → Gremlin.
   A subtle chip says `gremlin ▾`. One tap to override if wrong.

2. The main area shows the graph result. Force-directed, interactive.
   Drag nodes. Zoom. Hover for tooltips. Click a node → sidebar
   updates with properties and connections.

3. A properties/detail panel on the right. Shows selected node info.
   Edges, types, metadata. Updates on every click.

4. A result table below. Dense, sortable, full-width. Same data as
   the graph but tabular. Click a row → node highlights in graph.

5. Below that, more cells. R code, more queries, markdown notes.
   Each cell result renders as the right instrument: graph, table,
   chart, or scalar.

6. Everything is reactive. Change a cell → downstream cells re-execute →
   all views update. No "Run All" button.

7. ⌘P → PDF. The notebook IS the document. Graphs render as SVG,
   tables typeset, code syntax-highlighted.

## The Feel

- Dark background (#0a0e17 to #1a1f2e)
- Teal/cyan primary accent (#00bcd4 to #4dd0e1)
- Amber warnings, red errors, green healthy
- Clean sans-serif typography (Inter or similar)
- Monospace only inside code cells
- Panels, not a cell stack — CSS grid multi-panel layout
- Graph layout settles like leaves on water (force simulation)
- Panel transitions 200ms ease
- New results fade in
- Loading: subtle pulse on cell border, not a spinner
- Premium. A manager sees this in a demo and says "enterprise."

## What Already Exists

The backend is built (5 Rust crates compile). The frontend is the gap.

| Backend piece | Status |
|---|---|
| Reactive cell runtime | notebook-runtime crate (exists) |
| Gremlin/Cypher/SPARQL executors | notebook-query crate (exists) |
| R kernel protocol (ZMQ) | notebook-kernel crate (exists) |
| Graph/table/chart rendering → HTML | notebook-render crate (exists) |
| Publisher (HTML/PDF) | notebook-publish (in progress) |
| Local graph engine | lance-graph (exists, 121 tests) |
| SIMD kernels | ndarray (exists, 890 tests) |
| MCP server for AI copilot | defined (SCOPE F) |

## What Needs to Be Designed

A frontend that:
- Serves from the Rust binary as static files (axum on :2718)
- Talks to the backend via MCP over SSE (/mcp/sse)
- Uses vis.js or d3 for graph rendering (notebook-render produces the JS)
- Uses DataTables.js for tables
- Implements the cockpit layout described above
- Implements the dark theme described above
- Works in Chrome, Firefox, Safari
- Is responsive enough for laptop screens (1280px minimum)

Technology: React (marimo's frontend is React) or vanilla JS + web components.
Whatever ships fastest and looks best.

## Deliverable

Design mockups or working HTML/CSS/JS prototype of the cockpit layout.
Show: query bar, graph panel, properties sidebar, result table, cell stack.
Dark theme. Teal accents. Dense. Professional. Cockpit energy.

This is the car. Make it look like a Ferrari.
