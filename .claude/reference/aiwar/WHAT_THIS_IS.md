# aiwar-neo4j-harvest — Full Reference

Source: AdaWorldAPI/aiwar-neo4j-harvest (cloned at full depth)

## It's Not Just a Quarto Site

It's a Rust-native graph knowledge system with 5 layers:

### Layer 1: Rust Crate (aiwar-neo4j)
- Cargo.toml, src/main.rs, src/model.rs, src/ingest.rs
- Reads JSON graph data + schema ontology
- Generates Cypher programmatically from typed Rust structs
- Ingests directly into Neo4j via neo4rs crate
- CLI: cypher | neo4j | analyze | chess-openings | chess-evals | chess-bridge | live-games

### Layer 2: Schema-as-Data Ontology
- data/schema.json — 12 taxonomy axes defined as DATA, not code
- AIRO (AI Risk Ontology) + VAIR framework
- Schema nodes are first-class graph citizens
- Hierarchical meta-edges: edge-tables are nodes in a meta-graph

### Layer 3: Cypher Enrichment Pipeline
- 30+ .cypher files, ~600KB of graph enrichment
- Palantir surveillance, geopolitical networks, chess knowledge
- Incremental patches (v31→v42) — like git for graph data
- Behavioral science schema on edges (receptor, mcclelland, rubicon)

### Layer 4: Chess Cross-Domain Bridge
- chess_model.rs — openings, positions, evaluations as graph nodes
- Lichess evaluation database ingestion
- ladybug-rs fingerprint bridging: chess ↔ AI war via VSA RESONATE
- Elo testing via Lichess Bot API

### Layer 5: Quarto Publishing
- aiwar-main/ — the visible web site
- Interactive force-directed graph (d3 + three.js via gravis)
- Interactive filterable tables (itables + DataTables.js)
- quarto-rust extension for executable Rust code blocks

## What q2 Should Learn From This

1. Graph data → programmatic Cypher generation (Layer 1) = notebook-query pattern
2. Schema-as-data (Layer 2) = the notebook's graph schema should be self-describing
3. Incremental enrichment (Layer 3) = notebook cells as enrichment patches
4. Cross-domain bridging (Layer 4) = query results connecting different knowledge domains
5. Interactive graph + tables as primary content (Layer 5) = the cockpit instruments
6. The quarto-rust extension pattern = execute code at view time, not just render time
