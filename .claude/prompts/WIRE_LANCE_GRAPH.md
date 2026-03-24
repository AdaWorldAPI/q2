# Wire lance-graph Hot Path into notebook-query

## MANDATORY: Read .claude/rules/architectural-compliance.md FIRST

## Context

The workspace `Cargo.toml` now points lance-graph at the real crate:
```toml
[workspace.dependencies.lance-graph]
git = "https://github.com/AdaWorldAPI/lance-graph"
```

This is a full Cypher engine with DataFusion planner. 70+ source files.
NOT a stub. NOT neo4j-rs. NOT a substitute. THIS is the hot path.

## Step 1: Make it compile

```bash
cargo check -p quarto 2>&1
```

The real lance-graph depends on `arrow 57`, `datafusion 51`, `lance 2`.
These may conflict with existing deps or need additions to the workspace.
Fix version conflicts. Do NOT remove lance-graph to "fix" the build.

If lance-graph-catalog has a path dep issue (it references `../lance-graph-catalog`
which doesn't exist in a git dep), you may need to add lance-graph-catalog
as a separate workspace dependency.

## Step 2: Wire notebook-query to use lance-graph's Cypher parser

The stub `crates/stubs/notebook-query/src/lib.rs` currently returns fake data.
Replace the Cypher path with the real lance-graph API:

```rust
// Add to Cargo.toml of notebook-query stub:
// [dependencies]
// lance-graph.workspace = true
// arrow = "57"
// tokio = { version = "1", features = ["rt-multi-thread"] }
// serde_json = "1"

use lance_graph::{CypherQuery, GraphConfig};

pub fn execute(source: &str, language: QueryLanguage) -> Result<QueryResult, String> {
    match language {
        QueryLanguage::Cypher => execute_cypher(source),
        // ... other languages stay as stubs for now
    }
}

fn execute_cypher(source: &str) -> Result<QueryResult, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let config = aiwar_graph_config();
        let ctx = aiwar_session().await?;
        let query = CypherQuery::new(source)
            .map_err(|e| e.to_string())?
            .with_config(config);
        let batches = query.execute(&ctx).await.map_err(|e| e.to_string())?;
        Ok(batches_to_query_result(batches))
    })
}
```

## Step 3: Load aiwar data into DataFusion

The data lives in `aiwar-neo4j-harvest/data/aiwar_graph.json` (221 nodes, 356 edges).
Convert to Arrow RecordBatches and register with DataFusion.

Read the JSON structure (already documented in PREP_DUAL_PATH.md):
- N_Systems: 65 items, 20 columns
- N_Stakeholders: 114 items
- N_Civic: 23 items
- etc.

Register each as a DataFusion table. Then lance-graph's Cypher planner
queries them via DataFusion SQL under the hood.

```rust
use datafusion::prelude::*;

async fn aiwar_session() -> Result<SessionContext, String> {
    let ctx = SessionContext::new();
    // Register tables from aiwar JSON (or Parquet if pre-converted)
    // ...
    Ok(ctx)
}

fn aiwar_graph_config() -> GraphConfig {
    GraphConfig::builder()
        .with_node_label("System", "id")
        .with_node_label("Stakeholder", "id")
        .with_node_label("Civic", "id")
        .with_node_label("Historical", "id")
        .with_node_label("Person", "id")
        .with_relationship("CONNECTED_TO", "source", "target")
        .with_relationship("DEVELOPED_BY", "source", "target")
        .with_relationship("DEPLOYED_BY", "source", "target")
        .with_relationship("USED_IN", "source", "target")
        .with_relationship("PERSON_LINK", "source", "target")
        .with_relationship("HIERARCHICAL", "source", "target")
        .build()
        .expect("valid config")
}
```

## Step 4: Convert RecordBatches to cockpit JSON

The cockpit frontend expects `{ "nodes": [...], "edges": [...] }`.
Write a converter from Arrow RecordBatches to this format.

Look at the existing `demo_network_topology()` in the stub for the
exact JSON shape the frontend expects.

## Step 5: Cold path (Neo4j Aura fallback)

Add `neo4rs` as an optional dependency to notebook-query:

```toml
[dependencies]
neo4rs = { version = "0.8", optional = true }

[features]
default = []
neo4j-fallback = ["dep:neo4rs"]
```

```rust
#[cfg(feature = "neo4j-fallback")]
async fn execute_cold(source: &str) -> Result<QueryResult, String> {
    let graph = neo4rs::Graph::new(
        &std::env::var("NEO4J_URI").map_err(|e| e.to_string())?,
        "neo4j",
        &std::env::var("NEO4J_PASSWORD").map_err(|e| e.to_string())?,
    ).await.map_err(|e| e.to_string())?;
    let mut result = graph.execute(neo4rs::query(source)).await.map_err(|e| e.to_string())?;
    // Convert to QueryResult...
}
```

## Step 6: Verify

```bash
cargo build -p quarto
cargo run -p quarto -- notebook serve
# Open localhost:2718
# Type: MATCH (s:System) RETURN s.name, s.type
# See real aiwar data in the cockpit
```

## What NOT to do

- Do NOT remove lance-graph and substitute another engine
- Do NOT skip DataFusion — lance-graph's planner IS DataFusion
- Do NOT ignore compilation errors by emptying the lance-graph dep
- If something doesn't compile, FIX IT or ASK — don't replace
