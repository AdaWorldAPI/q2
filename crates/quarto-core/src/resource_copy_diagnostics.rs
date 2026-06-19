//! Structured diagnostics for user-resource copy failures (bd-bxrkxblx).
//!
//! A document can reference an on-disk resource (an image, etc.) that
//! Quarto must copy into the rendered output. Two things can go wrong,
//! and they deserve different treatment:
//!
//! - **The referenced source file is missing** — a user-content problem
//!   the author can fix. Detected at the drain site *before* the copy is
//!   attempted (`runtime.is_file(src)` is false). Reported as the
//!   **`Q-5-6`** *warning*, located at the reference so the renderer can
//!   underline the offending `![](…)`. The render continues without the
//!   file (matching Quarto 1's tolerant behavior).
//! - **The copy/write fails at [`OutputSink::flush`](crate::output_sink)
//!   for an environment reason** — permission denied, disk full, etc.
//!   Reported as the **`Q-5-7`** *error*. Span-less: the fault is the
//!   filesystem, not any particular reference. The specific OS condition
//!   is surfaced as advisory detail.
//!
//! Both functions mirror the precedent in
//! [`crate::project_resources::resource_error_to_parse_error`].

use quarto_error_reporting::{DiagnosticMessage, DiagnosticMessageBuilder};
use quarto_system_runtime::SystemRuntime;

use crate::error::{ParseError, QuartoError};
use crate::output_sink::{OutputSink, OutputSinkError};
use crate::render::ResourceCopyIntent;

/// Build the `Q-5-6` warning for a referenced resource whose source
/// file does not exist.
///
/// The diagnostic is located at the reference's `origin` span so the
/// renderer can underline the offending reference in the source `.qmd`.
/// The caller is responsible for rendering it against a source context
/// that contains the referencing file (the per-render
/// `RenderOutput::source_context` already does).
pub fn missing_resource_diagnostic(intent: &ResourceCopyIntent) -> DiagnosticMessage {
    DiagnosticMessageBuilder::warning("Referenced resource not found")
        .with_code("Q-5-6")
        .with_location(intent.origin.clone())
        .problem(format!(
            "The referenced file `{}` does not exist, so it was not copied \
             into the output. The render continued without it.",
            intent.src.display()
        ))
        .add_hint("Check the path in the reference, or add the missing file.")
        .build()
}

/// Build the `Q-5-7` error for a resource copy/write that failed at
/// flush.
///
/// Span-less: a flush failure is an environment condition (permission
/// denied, disk full), not a problem with any particular reference. The
/// underlying OS condition is surfaced as advisory detail where
/// `io::ErrorKind` is informative, falling back to the raw OS message.
pub fn copy_failure_diagnostic(err: &OutputSinkError) -> DiagnosticMessage {
    let (problem, advisory) = describe(err);
    let mut builder = DiagnosticMessageBuilder::error("Resource copy failed")
        .with_code("Q-5-7")
        .problem(problem);
    if let Some(advisory) = advisory {
        builder = builder.add_info(advisory);
    }
    builder.build()
}

/// Map an [`OutputSinkError`] to a problem statement and an optional
/// advisory detail. I/O-bearing variants get a path-aware problem plus
/// an `io::ErrorKind`-tailored advisory; the remaining (validation /
/// canonicalize) variants fall back to the error's own `Display`.
fn describe(err: &OutputSinkError) -> (String, Option<String>) {
    match err {
        OutputSinkError::Copy { src, dest, source } => (
            format!(
                "Could not copy `{}` to `{}`.",
                src.display(),
                dest.display()
            ),
            advisory_for_io(source),
        ),
        OutputSinkError::Write { dest, source } => (
            format!("Could not write `{}`.", dest.display()),
            advisory_for_io(source),
        ),
        OutputSinkError::CreateParent { parent, source } => (
            format!(
                "Could not create the output directory `{}`.",
                parent.display()
            ),
            advisory_for_io(source),
        ),
        OutputSinkError::Canonicalize { path, source } => (
            format!("Could not resolve the output path `{}`.", path.display()),
            advisory_for_io(source),
        ),
        // Enqueue-time contract violations (a producer fed a bad
        // destination). These indicate an internal bug rather than an
        // environment fault; surface the sink's own message verbatim.
        other @ (OutputSinkError::DestOutsideAllowedRoots { .. }
        | OutputSinkError::DestNotAbsolute { .. }) => (other.to_string(), None),
    }
}

/// Enqueue each copy intent whose source file exists into `sink`,
/// collecting a `Q-5-6` warning (and skipping the copy) for each intent
/// whose source is missing.
///
/// This is the shared body of both drain sites
/// (`render_document_to_file` and `pass2_renderer::flush_resource_copies`):
/// it implements the "detect missing source before attempting the copy"
/// policy at the one place that holds a [`SystemRuntime`]. Returns the
/// collected warnings for the caller to merge into the render's
/// diagnostics. Enqueue-time validation failures (a producer fed a
/// destination outside the allowed roots) propagate as `QuartoError`.
///
/// If the existence check itself errors, the source is *assumed present*
/// and the copy is enqueued — a genuine fault then surfaces as the
/// `Q-5-7` error at flush, rather than being masked as a spurious
/// "missing resource" warning.
pub fn enqueue_resource_copies(
    intents: Vec<ResourceCopyIntent>,
    sink: &mut OutputSink,
    runtime: &dyn SystemRuntime,
) -> Result<Vec<DiagnosticMessage>, QuartoError> {
    let mut warnings = Vec::new();
    for intent in intents {
        let exists = runtime.is_file(&intent.src).unwrap_or(true);
        if !exists {
            warnings.push(missing_resource_diagnostic(&intent));
            continue;
        }
        sink.copy(intent.src, intent.dest)
            .map_err(QuartoError::from)?;
    }
    Ok(warnings)
}

/// Map an [`OutputSinkError`] from a resource-copy flush into a
/// structured [`QuartoError::Parse`] carrying the `Q-5-7` diagnostic, so
/// it reaches the pretty reporting path (via `file_failure_from_error`)
/// rather than the legacy single-line fallback. Span-less, so an empty
/// source context suffices.
pub fn copy_failure_error(err: &OutputSinkError) -> QuartoError {
    QuartoError::Parse(ParseError::new(
        vec![copy_failure_diagnostic(err)],
        quarto_source_map::SourceContext::new(),
    ))
}

/// Tailor an advisory line from the underlying I/O error. `PermissionDenied`
/// gets a writable-directory nudge; everything else carries the raw OS
/// message so distinctive conditions (e.g. "No space left on device")
/// still reach the user.
fn advisory_for_io(source: &std::io::Error) -> Option<String> {
    match source.kind() {
        std::io::ErrorKind::PermissionDenied => Some(
            "The filesystem denied permission for this write. Check that the \
             output directory is writable."
                .to_string(),
        ),
        _ => Some(format!("The filesystem reported: {source}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quarto_error_reporting::DiagnosticKind;
    use quarto_source_map::{FileId, SourceInfo};
    use std::path::PathBuf;

    fn intent(src: &str) -> ResourceCopyIntent {
        ResourceCopyIntent {
            src: PathBuf::from(src),
            dest: PathBuf::from("/out/img.png"),
            origin: SourceInfo::original(FileId(0), 10, 30),
        }
    }

    #[test]
    fn missing_resource_is_q_5_6_warning_with_location() {
        let diag = missing_resource_diagnostic(&intent("/project/images/missing.png"));
        assert_eq!(diag.code.as_deref(), Some("Q-5-6"));
        assert_eq!(diag.kind, DiagnosticKind::Warning);
        assert!(
            diag.location.is_some(),
            "Q-5-6 must carry the reference's source location"
        );
        let text = diag.to_text(None);
        assert!(
            text.contains("missing.png"),
            "problem must name the missing source; got: {text}"
        );
    }

    #[test]
    fn copy_failure_permission_denied_is_q_5_7_error_with_advisory() {
        let err = OutputSinkError::Copy {
            src: PathBuf::from("/project/a.png"),
            dest: PathBuf::from("/out/a.png"),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        };
        let diag = copy_failure_diagnostic(&err);
        assert_eq!(diag.code.as_deref(), Some("Q-5-7"));
        assert_eq!(diag.kind, DiagnosticKind::Error);
        assert!(
            diag.location.is_none(),
            "Q-5-7 is span-less — the fault is the filesystem, not a reference"
        );
        let text = diag.to_text(None);
        assert!(
            text.to_lowercase().contains("permission"),
            "permission-denied advisory expected; got: {text}"
        );
    }

    #[test]
    fn copy_failure_disk_full_surfaces_raw_os_message() {
        // ENOSPC: on stable Rust this may not map to a dedicated
        // ErrorKind, so the advisory must fall through to the raw OS
        // message rather than swallowing it.
        let err = OutputSinkError::Copy {
            src: PathBuf::from("/project/big.bin"),
            dest: PathBuf::from("/out/big.bin"),
            source: std::io::Error::other("No space left on device (os error 28)"),
        };
        let diag = copy_failure_diagnostic(&err);
        assert_eq!(diag.code.as_deref(), Some("Q-5-7"));
        let text = diag.to_text(None);
        assert!(
            text.contains("No space left on device"),
            "disk-full message must reach the user; got: {text}"
        );
    }
}
