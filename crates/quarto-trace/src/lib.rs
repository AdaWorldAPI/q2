//! Typed schema and reader/writer library for Quarto pipeline execution traces.
//!
//! This crate defines the on-disk schema for trace files written by
//! `quarto`'s pipeline observers, along with thin reader/writer helpers
//! that are shared by the writer side (`quarto-core`'s `JsonTraceObserver`),
//! the CLI analyzer (`quarto trace list|show`), and the viewer backend
//! (`quarto-trace-server`).
//!
//! # Schema at a glance
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "render": {
//!     "input_path": "doc.qmd",
//!     "output_path": "doc.html",
//!     "format_target": "html",
//!     "started_at_unix_ms": 1799200496000.0,
//!     "git_hash": "abc1234",
//!     "total_duration_ms": 123.4
//!   },
//!   "pipeline": [
//!     { "stage": "parse", "index": 0, "data_kind": "DocumentAst", "data": {...},
//!       "duration_ms": 1.2, "status": "ok" },
//!     { "stage": "engine-execution", "index": 1, "status": "error",
//!       "error": {"message": "..."} },
//!     { "stage": "render-html-body", "index": 2, "status": "skipped" }
//!   ]
//! }
//! ```
//!
//! Unknown `status` values and unknown fields are tolerated by readers for
//! forward compatibility.

use serde::{Deserialize, Serialize};

pub mod read;
pub mod write;

/// Git short hash + optional `-dirty` suffix, captured at build time.
///
/// Falls back to `"unknown"` when `git` is not available (e.g. tarball
/// builds via `cargo package` without a `.git` directory).
pub const BUILD_GIT_HASH: &str = env!("QUARTO_GIT_HASH");

/// Current trace schema version.
///
/// Bumped only when entry-shape changes are introduced (e.g. delta-encoded
/// `DocumentAst` entries in Phase 4.6). Additive changes (new optional
/// fields) don't bump the version.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level trace document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceDocument {
    pub schema_version: u32,
    pub render: RenderInfo,
    pub pipeline: Vec<TraceEntry>,
}

impl TraceDocument {
    /// Construct a new empty trace document stamped with the current schema
    /// version.
    pub fn new(render: RenderInfo) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            render,
            pipeline: Vec::new(),
        }
    }
}

/// Top-level metadata about a render invocation.
///
/// Captured once per trace, populated progressively as the pipeline runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderInfo {
    /// Path to the input document, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_path: Option<String>,
    /// Path to the final output, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    /// Target format identifier (e.g. `"html"`, `"pdf"`), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_target: Option<String>,
    /// Milliseconds since the Unix epoch when the pipeline started.
    /// A number rather than a formatted string so no date library is
    /// required to produce it; viewers can format via
    /// `new Date(ms).toISOString()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<f64>,
    /// Git short hash of the `quarto` build that produced this trace, with
    /// `-dirty` suffix if the working tree was dirty at build time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    /// Total pipeline wall-clock duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_duration_ms: Option<f64>,
}

/// One entry in the pipeline array.
///
/// Entries with `status == Ok` carry `data` and `data_kind`; entries with
/// `status == Error` carry `error`; entries with `status == Skipped`
/// carry neither.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    /// Human-readable stage name (e.g. `"parse"`, `"metadata-merge"`).
    ///
    /// Synthetic names are also used: `"__input"` for the pipeline input,
    /// `"transform:<name>"` for individual AST transforms within
    /// `AstTransformsStage`.
    pub stage: String,

    /// Zero-based index of the stage in the outer pipeline.
    pub index: usize,

    /// Kind tag for the data payload. Present whenever `data` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_kind: Option<String>,

    /// Data payload — serialized pipeline data (AST JSON, markdown, HTML, etc.).
    /// Absent on errored and skipped stages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Wall-clock duration for this stage in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,

    /// Status marker. Defaults to `Ok` on older traces that pre-date the
    /// field.
    #[serde(default)]
    pub status: StageStatus,

    /// Error payload, present when `status == Error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<StageErrorInfo>,
}

/// Status of a stage within a trace.
///
/// `Unknown` is used by readers when deserializing a newer trace that
/// adds a status variant we don't know about yet — this keeps readers
/// forward-compatible.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StageStatus {
    #[default]
    Ok,
    Error,
    Skipped,
    /// Unknown status value produced by a newer writer.
    #[serde(other)]
    Unknown,
}

/// Error information attached to an errored stage entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageErrorInfo {
    /// Human-readable error message.
    pub message: String,
}
