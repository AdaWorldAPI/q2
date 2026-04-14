//! Reader helpers for trace files.
//!
//! Used by the CLI analyzer (`quarto trace list|show`) and by the viewer
//! backend to load `.quarto/trace/<doc>/latest.json` files into typed
//! [`TraceDocument`]s.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::TraceDocument;

/// Error type for trace read operations.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON parse error in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Load a trace document from disk.
pub fn read_trace(path: &Path) -> Result<TraceDocument, ReadError> {
    let file = File::open(path).map_err(|source| ReadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| ReadError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Discover trace files under a `.quarto/trace/` directory.
///
/// Returns the list of `(doc_stem, latest_json_path)` pairs, one per
/// subdirectory with a `latest.json` file. Order is unspecified.
///
/// Missing or inaccessible `trace_root` returns an empty list rather than
/// erroring — the common case for "no traces yet" should not be a hard
/// failure.
pub fn list_traces(trace_root: &Path) -> Vec<TraceListing> {
    let entries = match std::fs::read_dir(trace_root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let latest = path.join("latest.json");
        if latest.is_file() {
            let stem = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            out.push(TraceListing {
                doc_stem: stem,
                latest_path: latest,
            });
        }
    }
    out
}

/// One entry returned by [`list_traces`].
#[derive(Debug, Clone)]
pub struct TraceListing {
    /// The subdirectory name (the input document's file stem).
    pub doc_stem: String,
    /// Absolute path to the `latest.json` trace file.
    pub latest_path: PathBuf,
}
