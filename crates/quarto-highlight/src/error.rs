use thiserror::Error;

#[derive(Debug, Error)]
pub enum HighlightError {
    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter_highlight::Error),

    #[error("invalid highlight query file: {0}")]
    QueryParse(#[from] tree_sitter::QueryError),

    #[error("failed to serialize highlight spans to JSON: {0}")]
    Json(#[from] serde_json::Error),
}
