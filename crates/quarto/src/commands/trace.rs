//! `quarto trace` subcommand — list, show, and view pipeline traces.
//!
//! JSON-only output by default. Humans who want a pretty view use
//! `quarto trace view` (Phase 4.3+) or pipe to `jq` / `fx`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use quarto_trace::read::{TraceListing, list_traces, read_trace};
use serde_json::json;

/// Arguments shared by the `list` and `show` subcommands.
#[derive(Debug, Clone)]
pub struct TraceListArgs {
    /// Root directory to search. Defaults to `./.quarto/trace/`.
    pub trace_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TraceShowArgs {
    /// Root directory to search. Defaults to `./.quarto/trace/`.
    pub trace_dir: Option<PathBuf>,
    /// Document stem to show. If omitted and a single trace exists, that
    /// trace is used; otherwise the command errors.
    pub doc: Option<String>,
    /// Stage name to show. If omitted, prints the full trace.
    pub stage: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TraceViewArgs {
    pub trace_dir: Option<PathBuf>,
    /// Document to open in the viewer on startup. Reserved for future
    /// URL-fragment wiring; not currently used.
    pub doc: Option<String>,
    pub port: Option<u16>,
    pub host: Option<String>,
    /// If true, do not attempt to open a browser.
    pub no_browser: bool,
}

/// Produce the JSON value for `quarto trace list`.
///
/// Separated from [`execute_list`] so integration tests and future MCP tools
/// can reuse the logic without parsing captured stdout.
pub fn list_value(args: &TraceListArgs) -> Result<serde_json::Value> {
    let trace_dir = resolve_trace_dir(args.trace_dir.as_deref());
    let listings = list_traces(&trace_dir);

    let entries: Vec<_> = listings
        .iter()
        .map(
            |TraceListing {
                 doc_stem,
                 latest_path,
             }| {
                json!({
                    "doc": doc_stem,
                    "path": latest_path.display().to_string(),
                })
            },
        )
        .collect();

    Ok(json!({
        "trace_dir": trace_dir.display().to_string(),
        "traces": entries,
    }))
}

/// Entrypoint for `quarto trace list`.
pub fn execute_list(args: TraceListArgs) -> Result<()> {
    let value = list_value(&args)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Produce the JSON value for `quarto trace show`.
pub fn show_value(args: &TraceShowArgs) -> Result<serde_json::Value> {
    let trace_dir = resolve_trace_dir(args.trace_dir.as_deref());
    let listings = list_traces(&trace_dir);

    if listings.is_empty() {
        bail!(
            "No traces found under {}. Run with `trace: true` in document metadata to produce one.",
            trace_dir.display()
        );
    }

    let target = match &args.doc {
        Some(doc) => listings
            .iter()
            .find(|l| &l.doc_stem == doc)
            .with_context(|| {
                format!(
                    "No trace for doc {:?} under {}. Available: {}",
                    doc,
                    trace_dir.display(),
                    listings
                        .iter()
                        .map(|l| l.doc_stem.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?,
        None => {
            if listings.len() > 1 {
                bail!(
                    "Multiple traces available; pass --doc <stem>. Available: {}",
                    listings
                        .iter()
                        .map(|l| l.doc_stem.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            &listings[0]
        }
    };

    let doc = read_trace(&target.latest_path)
        .with_context(|| format!("Failed to read trace {}", target.latest_path.display()))?;

    if let Some(stage_name) = &args.stage {
        let entry = doc
            .pipeline
            .iter()
            .find(|e| &e.stage == stage_name)
            .with_context(|| {
                format!(
                    "No stage {:?} in trace. Available: {}",
                    stage_name,
                    doc.pipeline
                        .iter()
                        .map(|e| e.stage.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
        Ok(serde_json::to_value(entry)?)
    } else {
        Ok(serde_json::to_value(&doc)?)
    }
}

/// Entrypoint for `quarto trace show`.
pub fn execute_show(args: TraceShowArgs) -> Result<()> {
    let value = show_value(&args)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Entrypoint for `quarto trace view` — launches the SPA.
///
/// Starts the `quarto-trace-server` on a loopback address and serves the
/// embedded SPA plus the local `.quarto/trace/` tree. Blocks until the
/// process is signalled (Ctrl-C).
pub fn execute_view(args: TraceViewArgs) -> Result<()> {
    let trace_dir = resolve_trace_dir(args.trace_dir.as_deref());
    // `doc` preselection isn't wired into the SPA yet; accept it now so
    // the CLI shape is stable.
    let _ = args.doc;
    let _ = args.no_browser;

    let mut config = quarto_trace_server::ServerConfig::new(trace_dir);
    if let Some(host) = args.host {
        config.host = host;
    }
    if let Some(port) = args.port {
        config.port = port;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    runtime.block_on(quarto_trace_server::serve(config))?;
    Ok(())
}

fn resolve_trace_dir(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".quarto")
        .join("trace")
}
