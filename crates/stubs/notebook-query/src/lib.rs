// TODO: replace when crate is transcoded from AdaWorldAPI/graph-notebook

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryLanguage {
    Gremlin,
    Cypher,
    Sparql,
    R,
    Rust,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub language: QueryLanguage,
    pub raw_output: String,
    pub html: Option<String>,
    /// JSON with `{ "nodes": [...], "edges": [...] }` for graph queries.
    /// The frontend renders this with vis-network.
    pub graph_json: Option<String>,
    pub elapsed_ms: u64,
}

pub fn detect_language(source: &str) -> QueryLanguage {
    let trimmed = source.trim();
    if trimmed.starts_with("g.")
        || trimmed.contains(".hasLabel(")
        || trimmed.contains(".outE(")
        || trimmed.contains(".inV(")
    {
        QueryLanguage::Gremlin
    } else if trimmed.starts_with("MATCH (") || trimmed.starts_with("MATCH(") {
        QueryLanguage::Cypher
    } else if trimmed.starts_with("PREFIX ") || trimmed.starts_with("SELECT ?") {
        QueryLanguage::Sparql
    } else if trimmed.contains("%>%") || trimmed.contains("<-") || trimmed.starts_with("library(")
    {
        QueryLanguage::R
    } else if trimmed.contains("let ") || trimmed.contains("fn ") {
        QueryLanguage::Rust
    } else {
        QueryLanguage::Markdown
    }
}

pub fn execute(source: &str, language: QueryLanguage) -> Result<QueryResult, String> {
    match language {
        QueryLanguage::Gremlin | QueryLanguage::Cypher | QueryLanguage::Sparql => {
            Ok(QueryResult {
                language,
                raw_output: format!("Executed {:?} query: {}", language, source),
                html: Some(format!("<pre>{}</pre>", source)),
                graph_json: Some(demo_network_topology()),
                elapsed_ms: 42,
            })
        }
        QueryLanguage::R => Ok(QueryResult {
            language,
            raw_output: format!("R output for: {}", source),
            html: Some(demo_r_table()),
            graph_json: None,
            elapsed_ms: 120,
        }),
        _ => Ok(QueryResult {
            language,
            raw_output: format!("Stub execution of {:?} query", language),
            html: Some(format!("<pre>{}</pre>", source)),
            graph_json: None,
            elapsed_ms: 0,
        }),
    }
}

/// Fake network topology — same data as cockpit-prototype/cockpit.js
/// so the demo looks identical to the static prototype.
fn demo_network_topology() -> String {
    r#"{
  "nodes": [
    { "id": "srv-001", "label": "web-server-01",   "type": "Server",       "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.67, "memory": 28.4, "connections": 5  } },
    { "id": "srv-002", "label": "web-server-02",   "type": "Server",       "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.54, "memory": 24.1, "connections": 4  } },
    { "id": "srv-003", "label": "web-server-03",   "type": "Server",       "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.42, "memory": 31.2, "connections": 5  } },
    { "id": "srv-004", "label": "web-server-04",   "type": "Server",       "properties": { "region": "eu-west-1", "status": "warning",  "cpu": 0.81, "memory": 29.8, "connections": 3  } },
    { "id": "api-001", "label": "api-gateway-01",  "type": "Gateway",      "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.31, "memory": 8.2,  "connections": 8  } },
    { "id": "api-002", "label": "api-gateway-02",  "type": "Gateway",      "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.28, "memory": 7.9,  "connections": 7  } },
    { "id": "db-001",  "label": "db-postgres-01",  "type": "Database",     "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.45, "memory": 62.3, "connections": 6  } },
    { "id": "db-002",  "label": "db-postgres-02",  "type": "Database",     "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.38, "memory": 58.7, "connections": 5  } },
    { "id": "cache-01","label": "cache-redis-01",  "type": "Cache",        "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.12, "memory": 16.0, "connections": 6  } },
    { "id": "cache-02","label": "cache-redis-02",  "type": "Cache",        "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.09, "memory": 16.0, "connections": 5  } },
    { "id": "lb-001",  "label": "lb-haproxy-01",   "type": "LoadBalancer", "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.22, "memory": 4.1,  "connections": 6  } },
    { "id": "lb-002",  "label": "lb-haproxy-02",   "type": "LoadBalancer", "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.19, "memory": 3.8,  "connections": 5  } },
    { "id": "mon-001", "label": "prometheus-01",   "type": "Monitor",      "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.55, "memory": 12.4, "connections": 10 } },
    { "id": "msg-001", "label": "kafka-broker-01", "type": "Queue",        "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.61, "memory": 32.0, "connections": 8  } },
    { "id": "msg-002", "label": "kafka-broker-02", "type": "Queue",        "properties": { "region": "eu-west-1", "status": "warning",  "cpu": 0.78, "memory": 30.5, "connections": 7  } },
    { "id": "cdn-001", "label": "cdn-edge-01",     "type": "CDN",          "properties": { "region": "global",    "status": "healthy",  "cpu": 0.15, "memory": 2.1,  "connections": 4  } },
    { "id": "dns-001", "label": "dns-resolver-01", "type": "DNS",          "properties": { "region": "global",    "status": "healthy",  "cpu": 0.08, "memory": 1.2,  "connections": 3  } },
    { "id": "vault-01","label": "vault-01",        "type": "Secrets",      "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.05, "memory": 2.0,  "connections": 4  } },
    { "id": "log-001", "label": "elasticsearch-01","type": "Search",       "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.72, "memory": 48.0, "connections": 5  } },
    { "id": "svc-001", "label": "auth-service-01", "type": "Service",      "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.33, "memory": 8.8,  "connections": 5  } },
    { "id": "svc-002", "label": "user-service-01", "type": "Service",      "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.29, "memory": 7.2,  "connections": 4  } },
    { "id": "svc-003", "label": "order-service-01","type": "Service",      "properties": { "region": "eu-west-1", "status": "critical", "cpu": 0.92, "memory": 14.5, "connections": 6  } },
    { "id": "wrk-001", "label": "worker-batch-01", "type": "Worker",       "properties": { "region": "us-east-1", "status": "healthy",  "cpu": 0.48, "memory": 16.3, "connections": 3  } },
    { "id": "wrk-002", "label": "worker-batch-02", "type": "Worker",       "properties": { "region": "eu-west-1", "status": "healthy",  "cpu": 0.44, "memory": 15.9, "connections": 3  } }
  ],
  "edges": [
    { "source": "lb-001",  "target": "srv-001", "label": "ROUTES_TO" },
    { "source": "lb-001",  "target": "srv-002", "label": "ROUTES_TO" },
    { "source": "lb-002",  "target": "srv-003", "label": "ROUTES_TO" },
    { "source": "lb-002",  "target": "srv-004", "label": "ROUTES_TO" },
    { "source": "srv-001", "target": "api-001", "label": "SERVES" },
    { "source": "srv-002", "target": "api-001", "label": "SERVES" },
    { "source": "srv-003", "target": "api-002", "label": "SERVES" },
    { "source": "srv-004", "target": "api-002", "label": "SERVES" },
    { "source": "api-001", "target": "svc-001", "label": "CALLS" },
    { "source": "api-001", "target": "svc-002", "label": "CALLS" },
    { "source": "api-002", "target": "svc-003", "label": "CALLS" },
    { "source": "svc-001", "target": "db-001",  "label": "QUERIES" },
    { "source": "svc-002", "target": "db-001",  "label": "QUERIES" },
    { "source": "svc-003", "target": "db-002",  "label": "QUERIES" },
    { "source": "srv-001", "target": "cache-01","label": "READS_FROM" },
    { "source": "srv-002", "target": "cache-01","label": "READS_FROM" },
    { "source": "srv-003", "target": "cache-02","label": "READS_FROM" },
    { "source": "srv-004", "target": "cache-02","label": "READS_FROM" },
    { "source": "msg-001", "target": "wrk-001", "label": "DELIVERS" },
    { "source": "msg-002", "target": "wrk-002", "label": "DELIVERS" },
    { "source": "svc-003", "target": "msg-002", "label": "PUBLISHES" },
    { "source": "svc-002", "target": "msg-001", "label": "PUBLISHES" },
    { "source": "mon-001", "target": "srv-001", "label": "MONITORS" },
    { "source": "mon-001", "target": "srv-002", "label": "MONITORS" },
    { "source": "mon-001", "target": "srv-003", "label": "MONITORS" },
    { "source": "mon-001", "target": "srv-004", "label": "MONITORS" },
    { "source": "mon-001", "target": "db-001",  "label": "MONITORS" },
    { "source": "mon-001", "target": "db-002",  "label": "MONITORS" },
    { "source": "dns-001", "target": "cdn-001", "label": "RESOLVES" },
    { "source": "cdn-001", "target": "lb-001",  "label": "FORWARDS" },
    { "source": "cdn-001", "target": "lb-002",  "label": "FORWARDS" }
  ]
}"#
    .to_string()
}

/// Demo R table output
fn demo_r_table() -> String {
    r#"<table class="mini-table">
<tr><td>web-server-01</td><td>0.67</td><td>28.4 GB</td></tr>
<tr><td>web-server-02</td><td>0.54</td><td>24.1 GB</td></tr>
<tr><td>web-server-03</td><td>0.42</td><td>31.2 GB</td></tr>
<tr><td>web-server-04</td><td>0.81</td><td>29.8 GB</td></tr>
</table>"#
        .to_string()
}
