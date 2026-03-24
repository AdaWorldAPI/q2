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
    // TODO: implement real execution by routing to appropriate backend
    Ok(QueryResult {
        language,
        raw_output: format!("Stub execution of {:?} query", language),
        html: Some(format!("<pre>{}</pre>", source)),
        elapsed_ms: 0,
    })
}
