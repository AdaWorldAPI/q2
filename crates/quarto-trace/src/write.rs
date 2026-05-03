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

/// Write a trace document to disk.
///
/// On-disk traces are written as compact (un-indented) JSON: pretty-print
/// indentation accounted for ~80% of bytes on real traces (bd-5qnj).
/// If `path` ends in `.gz`, the JSON stream is gzipped; otherwise plain
/// JSON is emitted. Humans who want a pretty view use
/// `quarto trace show` (which formats from the parsed [`TraceDocument`])
/// or pipe the file through `jq` (after `gunzip` if applicable).
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
    let buffered = BufWriter::new(file);

    if has_gz_extension(path) {
        let gz = flate2::write::GzEncoder::new(buffered, flate2::Compression::default());
        serde_json::to_writer(gz, doc)?;
        // GzEncoder finishes its stream on drop; the BufWriter inside is
        // flushed when the GzEncoder drops it.
    } else {
        serde_json::to_writer(buffered, doc)?;
    }
    Ok(())
}

fn has_gz_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}
