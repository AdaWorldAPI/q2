//! Reader helpers for trace files.
//!
//! Used by the CLI analyzer (`quarto trace list|show`) and by the viewer
//! backend to load `.quarto/trace/<doc>/latest.json` (or `latest.json.gz`)
//! files into typed [`TraceDocument`]s.

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
///
/// If the path ends in `.gz`, the file is treated as gzipped JSON and
/// decompressed transparently; otherwise it is parsed as plain JSON.
/// Both compact and pretty-printed JSON inputs are accepted (legacy
/// pre-bd-5qnj traces are pretty-printed).
pub fn read_trace(path: &Path) -> Result<TraceDocument, ReadError> {
    let file = File::open(path).map_err(|source| ReadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let buffered = BufReader::new(file);
    let to_read_err = |source| ReadError::Json {
        path: path.to_path_buf(),
        source,
    };

    if has_gz_extension(path) {
        let gz = flate2::read::GzDecoder::new(buffered);
        serde_json::from_reader(BufReader::new(gz)).map_err(to_read_err)
    } else {
        serde_json::from_reader(buffered).map_err(to_read_err)
    }
}

/// Discover trace files under a `.quarto/trace/` directory.
///
/// Returns the list of `(doc_stem, latest_path)` pairs, one per
/// subdirectory containing either a `latest.json.gz` or `latest.json`
/// file. When both are present in the same directory, the gzipped file
/// is preferred (it is the post-bd-5qnj on-disk format; the
/// uncompressed file may be a stale pre-Phase-1 artifact). Order is
/// unspecified.
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
        let gz = path.join("latest.json.gz");
        let plain = path.join("latest.json");
        let latest = if gz.is_file() {
            gz
        } else if plain.is_file() {
            plain
        } else {
            continue;
        };
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
    out
}

/// One entry returned by [`list_traces`].
#[derive(Debug, Clone)]
pub struct TraceListing {
    /// The subdirectory name (the input document's file stem).
    pub doc_stem: String,
    /// Absolute path to the `latest.json` or `latest.json.gz` trace file.
    pub latest_path: PathBuf,
}

fn has_gz_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}
