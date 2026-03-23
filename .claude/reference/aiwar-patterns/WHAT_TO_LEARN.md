# What This Project Does That Standard Quarto Doesn't

Source: AdaWorldAPI/aiwar-neo4j-harvest/aiwar-main

## 1. Interactive Graph IS the Document
index.qmd builds a force-directed graph from ODS data using NetworkX + gravis.
The graph isn't a figure in a report — it IS the page content.
gv.d3() produces interactive HTML: drag, zoom, hover tooltips, click details.

## 2. 3D Graph Visualization
three.qmd uses gv.three() — Three.js 3D force-directed layout with
x/y/z positioning forces. Quarto has zero native 3D support.

## 3. Data-Driven Nodes
Nodes load properties from multiple spreadsheet sheets (Stakeholders,
Systems, Civic, Historical, People). Each node has: name, icon/image,
category, metadata. Hover shows tooltip. Click shows details panel.

## 4. Interactive Tables (not static)
Uses itables + DataTables.js: search, sort, scroll, paginate.
Quarto's built-in tables are static markdown.

## 5. Plotly Iframes
Pre-rendered Plotly charts as standalone HTML iframes.
Interactive charts that survive the publish step.

## 6. Polyglot Code Execution
The quarto-rust extension (in quarto-rust-extension/) adds Rust
code blocks with a Run button. This project uses Python cells.
Together: proof that multi-language execution in Quarto works.

## What q2 Should Do Natively
All of the above, but without Python/NetworkX/gravis/itables dependencies.
The Rust binary reads graph data, renders interactive vis.js/Three.js,
produces filterable tables, executes code cells in any language —
and publishes as static HTML or PDF. One binary, zero Python.
