/*
 * stage/trace.rs
 * Copyright (c) 2025 Posit, PBC
 *
 * Pipeline tracing observers for debugging and diagnostics.
 */

//! Concrete [`PipelineObserver`] implementations for tracing pipeline execution.
//!
//! - [`JsonTraceObserver`]: Captures full pipeline state at each stage boundary
//!   and writes a JSON trace file to `.quarto/trace/`.
//! - [`SummaryTraceObserver`]: Prints a human-readable summary to stderr.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use super::data::{PipelineData, PipelineDataKind};
use super::error::PipelineError;
use super::observer::{EventLevel, PipelineObserver};

// ─── Trace entry types ───────────────────────────────────────────────────────

/// A single trace entry, capturing the pipeline state after a stage or transform.
#[derive(Debug)]
struct TraceEntry {
    /// Name of the stage or transform
    name: String,
    /// Zero-based index within the pipeline (or transform pipeline)
    index: usize,
    /// What kind of data was produced
    data_kind: PipelineDataKind,
    /// Serialized data (JSON value)
    data_json: serde_json::Value,
    /// Wall-clock duration of this stage
    duration_ms: Option<f64>,
}

/// Internal mutable state for JsonTraceObserver.
#[derive(Debug)]
struct JsonTraceState {
    entries: Vec<TraceEntry>,
    /// When the current stage started (set in on_stage_start)
    stage_start: Option<Instant>,
    /// When the pipeline started
    pipeline_start: Option<Instant>,
}

// ─── JsonTraceObserver ───────────────────────────────────────────────────────

/// Observer that captures full pipeline state at each stage boundary.
///
/// After the pipeline completes, call [`JsonTraceObserver::write_trace`]
/// to write the captured data to a JSON file.
///
/// # Trace Format
///
/// The output is a JSON object with:
/// - `pipeline`: Array of trace entries, each with `stage`, `index`,
///   `data_kind`, `data`, and `duration_ms`.
/// - `total_duration_ms`: Total pipeline wall-clock time.
pub struct JsonTraceObserver {
    state: Mutex<JsonTraceState>,
    /// Path to write the trace file to.
    output_path: PathBuf,
}

impl JsonTraceObserver {
    /// Create a new JSON trace observer.
    ///
    /// The trace will be written to `output_path` when [`write_trace`] is called.
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            state: Mutex::new(JsonTraceState {
                entries: Vec::new(),
                stage_start: None,
                pipeline_start: None,
            }),
            output_path,
        }
    }

    /// Write the collected trace to the output file.
    ///
    /// This should be called after the pipeline completes (or fails).
    /// Creates parent directories as needed.
    pub fn write_trace(&self) -> std::io::Result<()> {
        let state = self.state.lock().unwrap();

        let total_duration_ms = state
            .pipeline_start
            .map(|start| start.elapsed().as_secs_f64() * 1000.0);

        let entries: Vec<serde_json::Value> = state
            .entries
            .iter()
            .map(|entry| {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "stage".into(),
                    serde_json::Value::String(entry.name.clone()),
                );
                obj.insert(
                    "index".into(),
                    serde_json::Value::Number(entry.index.into()),
                );
                obj.insert(
                    "data_kind".into(),
                    serde_json::Value::String(entry.data_kind.to_string()),
                );
                obj.insert("data".into(), entry.data_json.clone());
                if let Some(ms) = entry.duration_ms {
                    obj.insert(
                        "duration_ms".into(),
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(ms)
                                .unwrap_or_else(|| serde_json::Number::from(0)),
                        ),
                    );
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        let mut root = serde_json::Map::new();
        root.insert("pipeline".into(), serde_json::Value::Array(entries));
        if let Some(ms) = total_duration_ms {
            root.insert(
                "total_duration_ms".into(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(ms).unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            );
        }

        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(&self.output_path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &serde_json::Value::Object(root))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Get the output path.
    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}

impl PipelineObserver for JsonTraceObserver {
    fn on_pipeline_start(&self, _total_stages: usize) {
        let mut state = self.state.lock().unwrap();
        state.pipeline_start = Some(Instant::now());
    }

    fn on_stage_start(&self, _name: &str, _index: usize, _total: usize) {
        let mut state = self.state.lock().unwrap();
        state.stage_start = Some(Instant::now());
    }

    fn on_pipeline_input(&self, data: &PipelineData) {
        let data_json = serialize_pipeline_data(data);
        let mut state = self.state.lock().unwrap();
        state.entries.push(TraceEntry {
            name: "__input".to_string(),
            index: 0,
            data_kind: data.kind(),
            data_json,
            duration_ms: None,
        });
    }

    fn on_stage_data(&self, name: &str, index: usize, data: &PipelineData) {
        let data_json = serialize_pipeline_data(data);
        let mut state = self.state.lock().unwrap();
        let duration_ms = state
            .stage_start
            .map(|start| start.elapsed().as_secs_f64() * 1000.0);
        state.entries.push(TraceEntry {
            name: name.to_string(),
            index,
            data_kind: data.kind(),
            data_json,
            duration_ms,
        });
    }

    fn on_transform_data(
        &self,
        name: &str,
        index: usize,
        _total: usize,
        ast: &quarto_pandoc_types::pandoc::Pandoc,
    ) {
        let data_json = serialize_pandoc_ast(ast);
        let mut state = self.state.lock().unwrap();
        state.entries.push(TraceEntry {
            name: format!("transform:{}", name),
            index,
            data_kind: PipelineDataKind::DocumentAst,
            data_json,
            duration_ms: None,
        });
    }

    fn on_pipeline_complete(&self) {
        // Best-effort write on completion
        if let Err(e) = self.write_trace() {
            eprintln!("Warning: failed to write pipeline trace: {}", e);
        }
    }

    fn on_pipeline_error(&self, _error: &PipelineError) {
        // Still write what we have on error
        if let Err(e) = self.write_trace() {
            eprintln!("Warning: failed to write pipeline trace: {}", e);
        }
    }
}

impl std::fmt::Debug for JsonTraceObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonTraceObserver")
            .field("output_path", &self.output_path)
            .finish()
    }
}

// ─── SummaryTraceObserver ────────────────────────────────────────────────────

/// Internal mutable state for SummaryTraceObserver.
#[derive(Debug)]
struct SummaryTraceState {
    stage_start: Option<Instant>,
    pipeline_start: Option<Instant>,
}

/// Observer that prints a human-readable summary of pipeline execution to stderr.
///
/// Output includes stage names, data kinds, timing, and AST block counts
/// where available.
pub struct SummaryTraceObserver {
    state: Mutex<SummaryTraceState>,
}

impl SummaryTraceObserver {
    /// Create a new summary trace observer.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SummaryTraceState {
                stage_start: None,
                pipeline_start: None,
            }),
        }
    }
}

impl Default for SummaryTraceObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineObserver for SummaryTraceObserver {
    fn on_pipeline_start(&self, total_stages: usize) {
        let mut state = self.state.lock().unwrap();
        state.pipeline_start = Some(Instant::now());
        eprintln!("[trace] Pipeline starting ({} stages)", total_stages);
    }

    fn on_stage_start(&self, name: &str, index: usize, total: usize) {
        let mut state = self.state.lock().unwrap();
        state.stage_start = Some(Instant::now());
        eprintln!("[trace] [{}/{}] {} ...", index + 1, total, name);
    }

    fn on_stage_data(&self, name: &str, _index: usize, data: &PipelineData) {
        let state = self.state.lock().unwrap();
        let duration_str = match state.stage_start {
            Some(start) => format!(" ({:.1}ms)", start.elapsed().as_secs_f64() * 1000.0),
            None => String::new(),
        };

        let detail = pipeline_data_summary(data);
        eprintln!("[trace]   -> {}: {}{}", name, detail, duration_str);
    }

    fn on_transform_data(
        &self,
        name: &str,
        index: usize,
        total: usize,
        ast: &quarto_pandoc_types::pandoc::Pandoc,
    ) {
        let block_count = ast.blocks.len();
        eprintln!(
            "[trace]     transform [{}/{}] {}: {} blocks",
            index + 1,
            total,
            name,
            block_count
        );
    }

    fn on_pipeline_complete(&self) {
        let state = self.state.lock().unwrap();
        let duration_str = match state.pipeline_start {
            Some(start) => format!(" in {:.1}ms", start.elapsed().as_secs_f64() * 1000.0),
            None => String::new(),
        };
        eprintln!("[trace] Pipeline complete{}", duration_str);
    }

    fn on_pipeline_error(&self, error: &PipelineError) {
        eprintln!("[trace] Pipeline failed: {}", error);
    }

    fn on_event(&self, message: &str, level: EventLevel) {
        eprintln!("[trace] [{}] {}", level.as_str(), message);
    }
}

impl std::fmt::Debug for SummaryTraceObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SummaryTraceObserver").finish()
    }
}

// ─── Serialization helpers ───────────────────────────────────────────────────

/// Serialize `PipelineData` to a JSON value for tracing.
///
/// Each variant is serialized with as much detail as practical:
/// - `DocumentAst`: Full Pandoc JSON via pampa's JSON writer
/// - `LoadedSource`: Path + source type (not raw bytes)
/// - `RenderedOutput`: HTML content + metadata
/// - Others: Available fields
fn serialize_pipeline_data(data: &PipelineData) -> serde_json::Value {
    match data {
        PipelineData::LoadedSource(s) => {
            serde_json::json!({
                "path": s.path.display().to_string(),
                "source_type": format!("{:?}", s.source_type),
                "content_length": s.content.len(),
            })
        }
        PipelineData::DocumentSource(s) => {
            serde_json::json!({
                "path": s.path.display().to_string(),
                "markdown_length": s.markdown.len(),
                "markdown": s.markdown,
            })
        }
        PipelineData::DocumentAst(doc) => {
            let ast_json = serialize_pandoc_ast(&doc.ast);
            serde_json::json!({
                "path": doc.path.display().to_string(),
                "ast": ast_json,
                "warnings_count": doc.warnings.len(),
            })
        }
        PipelineData::ExecutedDocument(doc) => {
            serde_json::json!({
                "path": doc.path.display().to_string(),
                "markdown_length": doc.markdown.len(),
                "markdown": doc.markdown,
                "supporting_files": doc.supporting_files.iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
                "filters": doc.filters,
            })
        }
        PipelineData::RenderedOutput(r) => {
            serde_json::json!({
                "input_path": r.input_path.display().to_string(),
                "output_path": r.output_path.display().to_string(),
                "format": format!("{:?}", r.format.identifier),
                "content_length": r.content.len(),
                "content": r.content,
                "is_intermediate": r.is_intermediate,
                "supporting_files": r.supporting_files.iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
            })
        }
        PipelineData::FinalOutput(f) => {
            serde_json::json!({
                "input_path": f.input_path.display().to_string(),
                "output_path": f.output_path.display().to_string(),
                "format": format!("{:?}", f.format.identifier),
                "supporting_files": f.supporting_files.iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>(),
                "warnings_count": f.warnings.len(),
            })
        }
    }
}

/// Serialize a Pandoc AST to a JSON value using pampa's JSON writer.
///
/// Falls back to a summary if serialization fails.
fn serialize_pandoc_ast(ast: &quarto_pandoc_types::pandoc::Pandoc) -> serde_json::Value {
    let context = pampa::pandoc::ASTContext::anonymous();
    let mut buf = Vec::new();
    match pampa::writers::json::write(ast, &context, &mut buf) {
        Ok(()) => {
            // Parse the JSON bytes back into a serde_json::Value
            serde_json::from_slice(&buf).unwrap_or_else(|_| {
                serde_json::json!({
                    "__error": "Failed to parse JSON output",
                    "block_count": ast.blocks.len(),
                })
            })
        }
        Err(_) => {
            serde_json::json!({
                "__error": "Failed to serialize AST to JSON",
                "block_count": ast.blocks.len(),
            })
        }
    }
}

/// Produce a brief human-readable summary of pipeline data.
fn pipeline_data_summary(data: &PipelineData) -> String {
    match data {
        PipelineData::LoadedSource(s) => {
            format!(
                "LoadedSource({}, {:?}, {} bytes)",
                s.path.display(),
                s.source_type,
                s.content.len()
            )
        }
        PipelineData::DocumentSource(s) => {
            format!(
                "DocumentSource({}, {} chars)",
                s.path.display(),
                s.markdown.len()
            )
        }
        PipelineData::DocumentAst(doc) => {
            format!(
                "DocumentAst({}, {} blocks)",
                doc.path.display(),
                doc.ast.blocks.len()
            )
        }
        PipelineData::ExecutedDocument(doc) => {
            format!(
                "ExecutedDocument({}, {} chars, {} supporting files)",
                doc.path.display(),
                doc.markdown.len(),
                doc.supporting_files.len()
            )
        }
        PipelineData::RenderedOutput(r) => {
            format!(
                "RenderedOutput({}, {} chars)",
                r.output_path.display(),
                r.content.len()
            )
        }
        PipelineData::FinalOutput(f) => {
            format!("FinalOutput({})", f.output_path.display())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::data::LoadedSource;

    #[test]
    fn test_serialize_loaded_source() {
        let data = PipelineData::LoadedSource(LoadedSource::new(
            PathBuf::from("test.qmd"),
            b"# Hello".to_vec(),
        ));

        let json = serialize_pipeline_data(&data);
        assert_eq!(json["path"], "test.qmd");
        assert_eq!(json["content_length"], 7);
        assert_eq!(json["source_type"], "Qmd");
    }

    #[test]
    fn test_serialize_document_ast() {
        let ast = quarto_pandoc_types::pandoc::Pandoc::default();
        let doc = crate::stage::DocumentAst {
            path: PathBuf::from("test.qmd"),
            ast,
            ast_context: pampa::pandoc::ASTContext::default(),
            source_context: quarto_source_map::SourceContext::new(),
            warnings: vec![],
        };

        let data = PipelineData::DocumentAst(doc);
        let json = serialize_pipeline_data(&data);
        assert_eq!(json["path"], "test.qmd");
        assert!(json["ast"].is_object());
        assert_eq!(json["warnings_count"], 0);
    }

    #[test]
    fn test_pipeline_data_summary() {
        let data = PipelineData::LoadedSource(LoadedSource::new(
            PathBuf::from("test.qmd"),
            b"hello".to_vec(),
        ));

        let summary = pipeline_data_summary(&data);
        assert!(summary.contains("LoadedSource"));
        assert!(summary.contains("test.qmd"));
        assert!(summary.contains("5 bytes"));
    }

    #[test]
    fn test_json_trace_observer_collects_entries() {
        let observer = JsonTraceObserver::new(PathBuf::from("/tmp/test-trace.json"));

        let data = PipelineData::LoadedSource(LoadedSource::new(
            PathBuf::from("test.qmd"),
            b"# Hello".to_vec(),
        ));

        observer.on_pipeline_start(2);
        observer.on_pipeline_input(&data);
        observer.on_stage_start("parse", 0, 2);
        observer.on_stage_data("parse", 0, &data);
        observer.on_stage_start("transform", 1, 2);
        observer.on_stage_data("transform", 1, &data);

        let state = observer.state.lock().unwrap();
        // __input + parse + transform = 3 entries
        assert_eq!(state.entries.len(), 3);
        assert_eq!(state.entries[0].name, "__input");
        assert_eq!(state.entries[1].name, "parse");
        assert_eq!(state.entries[2].name, "transform");
    }

    #[test]
    fn test_json_trace_observer_writes_file() {
        let dir = std::env::temp_dir().join("quarto-trace-test");
        let output_path = dir.join("trace.json");

        // Clean up from previous runs
        let _ = std::fs::remove_dir_all(&dir);

        let observer = JsonTraceObserver::new(output_path.clone());

        let data = PipelineData::LoadedSource(LoadedSource::new(
            PathBuf::from("test.qmd"),
            b"# Hello".to_vec(),
        ));

        observer.on_pipeline_start(1);
        observer.on_pipeline_input(&data);
        observer.on_stage_start("parse", 0, 1);
        observer.on_stage_data("parse", 0, &data);

        observer.write_trace().unwrap();

        // Verify the file was written and is valid JSON
        let content = std::fs::read_to_string(&output_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["pipeline"].is_array());
        assert_eq!(parsed["pipeline"].as_array().unwrap().len(), 2);

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }
}
