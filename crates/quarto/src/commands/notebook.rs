//! Graph notebook subcommand — `q2 notebook serve` and `q2 notebook render`
//!
//! Starts an axum server on port 2718 with:
//! - GET /           → serve frontend static files
//! - GET /health     → { "status": "ok" }
//! - GET /mcp/sse    → MCP server via Server-Sent Events
//! - POST /mcp/message → MCP tool invocations
//!
//! MCP tools: cell_execute, cell_get, cells_list, cell_create, cell_update,
//!            cell_delete, dag_get, notebook_save, notebook_load, notebook_export

use std::path::PathBuf;

use anyhow::Result;

pub struct NotebookServeArgs {
    pub port: u16,
    pub host: String,
    #[allow(dead_code)]
    pub open: bool,
    pub frontend_dir: Option<PathBuf>,
}

pub struct NotebookRenderArgs {
    pub input: String,
    pub format: String,
    pub output: Option<String>,
}

/// Start the graph notebook server on the specified port.
pub fn execute_serve(args: NotebookServeArgs) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        crate::notebook_server::serve(args).await
    })
}

/// Render a notebook file to HTML or PDF.
pub fn execute_render(args: NotebookRenderArgs) -> Result<()> {
    crate::publisher::render(args)
}
