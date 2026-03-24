# Dual-Path Prep — Hot (lance-graph) + Cold (Neo4j)

## The Data

`aiwar-neo4j-harvest/data/aiwar_graph.json`:
- 221 nodes: 65 Systems, 23 Civic, 7 Historical, 114 Stakeholders, 12 People
- 356 edges: 95 connection, 114 developed, 79 deployed, 21 place, 22 people, 25 hierarchical
- 31-row schema ontology (12 taxonomy axes)
- Plus 30 Cypher enrichment files (~600KB additional graph data)

## Hot Path: lance-graph DataFusion (primary)

### Step 1: Convert aiwar JSON → Parquet

Read `data/aiwar_graph.json`. Produce 5 node tables + 6 edge tables as Parquet files.

```
data/parquet/
├── nodes_systems.parquet      # 65 rows, 20 columns
├── nodes_civic.parquet        # 23 rows
├── nodes_historical.parquet   # 7 rows
├── nodes_stakeholders.parquet # 114 rows
├── nodes_people.parquet       # 12 rows
├── edges_connection.parquet   # 95 rows (source, target, label, weight, hover, reference)
├── edges_developed.parquet    # 114 rows
├── edges_deployed.parquet     # 79 rows
├── edges_place.parquet        # 21 rows
├── edges_people.parquet       # 22 rows
└── edges_hierarchical.parquet # 25 rows (source, target only)
```

Write a Rust binary (`crates/aiwar-ingest/src/main.rs`) that does this.
Use `arrow` + `parquet` crates. No Python.

```rust
// Read JSON, build Arrow RecordBatches, write Parquet
let systems: Vec<System> = serde_json::from_value(graph["N_Systems"].clone())?;
let batch = systems_to_record_batch(&systems)?;
let file = File::create("data/parquet/nodes_systems.parquet")?;
let writer = ArrowWriter::try_new(file, batch.schema(), None)?;
writer.write(&batch)?;
writer.close()?;
```

### Step 2: Wire GraphConfig for lance-graph

lance-graph needs a `GraphConfig` mapping labels to tables:

```rust
let config = GraphConfig::builder()
    // Nodes
    .with_node_label("System", "id")        // nodes_systems.parquet
    .with_node_label("Civic", "id")         // nodes_civic.parquet
    .with_node_label("Historical", "id")    // nodes_historical.parquet
    .with_node_label("Stakeholder", "id")   // nodes_stakeholders.parquet
    .with_node_label("Person", "id")        // nodes_people.parquet
    // Edges
    .with_relationship("CONNECTED_TO", "source", "target")  // edges_connection
    .with_relationship("DEVELOPED_BY", "source", "target")  // edges_developed
    .with_relationship("DEPLOYED_BY", "source", "target")   // edges_deployed
    .with_relationship("USED_IN", "source", "target")       // edges_place
    .with_relationship("PERSON_LINK", "source", "target")   // edges_people
    .with_relationship("HIERARCHICAL", "source", "target")  // edges_hierarchical
    .build()?;
```

### Step 3: Register Parquet tables with DataFusion

```rust
use datafusion::prelude::*;

let ctx = SessionContext::new();
ctx.register_parquet("nodes_systems", "data/parquet/nodes_systems.parquet", ParquetReadOptions::default()).await?;
ctx.register_parquet("nodes_stakeholders", "data/parquet/nodes_stakeholders.parquet", ParquetReadOptions::default()).await?;
ctx.register_parquet("edges_developed", "data/parquet/edges_developed.parquet", ParquetReadOptions::default()).await?;
// ... all 11 tables

// Now Cypher queries work:
let query = CypherQuery::new("MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s.name, st.name")?
    .with_config(config);
let batches = query.execute(&ctx).await?;
```

### Step 4: blasgraph projection (bgz17/rabitQ)

After Parquet tables are loaded, project into blasgraph's columnar format
for HHTL (Heel-Hip-Twig-Leaf) compressed storage:

```rust
use lance_graph::graph::blasgraph::columnar::NodeSchema;
use lance_graph::graph::blasgraph::BlasGraph;

// Build blasgraph from the DataFusion tables
let blasgraph = BlasGraph::from_datafusion(&ctx, &config).await?;
// Now semiring operations work on compressed columnar data
// Neighborhood queries use rabitQ-compatible distance
```

This is the fast path. Queries hit Arrow memory, not a network database.

### Step 5: Wire into q2 notebook-query stub

Replace `crates/stubs/notebook-query/` with real lance-graph execution:

```rust
// notebook-query/src/lib.rs
pub fn execute(code: &str, lang: QueryLanguage) -> Result<QueryResult> {
    match lang {
        QueryLanguage::Cypher => {
            let query = CypherQuery::new(code)?
                .with_config(HOT_CONFIG.clone());
            let batches = RUNTIME.block_on(query.execute(&SESSION))?;
            Ok(batches_to_query_result(batches))
        }
        QueryLanguage::Gremlin => {
            // Translate Gremlin to Cypher, then execute
            // (or use lance-graph's native planner if it supports Gremlin)
            todo!("Gremlin→Cypher transpiler")
        }
        _ => { /* ... */ }
    }
}
```

## Cold Path: Neo4j Aura (fallback)

### Step 1: Load into Neo4j (already works)

```bash
cd aiwar-neo4j-harvest
NEO4J_URI="neo4j+s://7e137e6e.databases.neo4j.io" \
NEO4J_USER="neo4j" \
NEO4J_PASSWORD="O-EXvpDXZBoIIH9SvmCiXobcGcMt81oEgmpS405hs1o" \
cargo run -- neo4j
```

This runs the existing `cmd_cypher()` → `cmd_neo4j()` pipeline.
Loads all 30 Cypher files. Already tested.

### Step 2: neo4j-rs fallback in notebook-query

```rust
// If hot path fails, fall back to cold
pub async fn execute_with_fallback(code: &str) -> Result<QueryResult> {
    // Try hot path first
    match execute_hot(code).await {
        Ok(result) => Ok(result),
        Err(hot_err) => {
            tracing::warn!("Hot path failed: {}, falling back to Neo4j", hot_err);
            execute_cold(code).await
        }
    }
}

async fn execute_cold(code: &str) -> Result<QueryResult> {
    let graph = neo4rs::Graph::new(
        "neo4j+s://7e137e6e.databases.neo4j.io",
        "neo4j",
        &std::env::var("NEO4J_PASSWORD")?
    ).await?;
    let mut result = graph.execute(neo4rs::query(code)).await?;
    // Convert neo4rs rows → QueryResult
}
```

### Step 3: Sync job (keep cold current)

When hot path data changes, push to Neo4j:

```rust
async fn sync_to_neo4j(hot_batches: &[RecordBatch]) -> Result<()> {
    let graph = neo4rs::Graph::new(/* ... */).await?;
    for batch in hot_batches {
        let cypher = batch_to_merge_cypher(batch);
        graph.run(neo4rs::query(&cypher)).await?;
    }
    Ok(())
}
```

## Railway Deployment

### Service: q2-notebook

```toml
# railway.toml
[build]
builder = "dockerfile"

[deploy]
startCommand = "./target/release/quarto notebook serve --host 0.0.0.0 --port $PORT --frontend-dir cockpit/dist"
healthcheckPath = "/health"
```

```dockerfile
FROM rust:1.85 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p quarto

FROM node:20 AS frontend
WORKDIR /app/cockpit
COPY cockpit/ .
RUN npm install && npm run build

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/quarto /usr/local/bin/
COPY --from=frontend /app/cockpit/dist /opt/cockpit/dist
COPY data/parquet/ /opt/data/parquet/
ENV PORT=2718
CMD ["quarto", "notebook", "serve", "--host", "0.0.0.0", "--port", "2718", "--frontend-dir", "/opt/cockpit/dist"]
```

### Environment variables

```
NEO4J_URI=neo4j+s://7e137e6e.databases.neo4j.io
NEO4J_USER=neo4j
NEO4J_PASSWORD=***
AIWAR_DATA_DIR=/opt/data/parquet
```

## Execution Order

1. `aiwar-ingest` — convert JSON → Parquet (run once, commit Parquet files)
2. Wire lance-graph `GraphConfig` + DataFusion session in q2
3. Replace notebook-query stub with real lance-graph execution
4. Test locally: `q2 notebook serve` → type Cypher → see real aiwar graph
5. Load Neo4j Aura as cold backup
6. Deploy to Railway
7. Present: type `MATCH (s:System)-[:DEVELOPED_BY]->(st:Stakeholder) RETURN s, st` → see graph

## What This Proves

A data engineer types Cypher into the cockpit. The query hits lance-graph
DataFusion (Arrow in memory, no network hop). Results render as an interactive
force-directed graph with 221 nodes and 356 edges. If lance-graph fails,
Neo4j Aura takes over transparently. No Grafana needed — the cockpit IS
the dashboard.
