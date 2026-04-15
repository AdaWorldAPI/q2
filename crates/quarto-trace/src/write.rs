//! Writer helpers for trace files.
//!
//! The `quarto-core` crate's `JsonTraceObserver` owns the actual observer
//! implementation and drives the [`TraceDocument`] construction; this
//! module exposes the atomic "write-to-disk" step so the file-system
//! concern lives here rather than duplicated in the observer.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::TraceDocument;

/// Error type for trace write operations.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("I/O error writing {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Write a trace document to disk as pretty-printed JSON.
///
/// Creates parent directories as needed.
pub fn write_trace(doc: &TraceDocument, path: &Path) -> Result<(), WriteError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| WriteError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let file = File::create(path).map_err(|source| WriteError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, doc)?;
    Ok(())
}
