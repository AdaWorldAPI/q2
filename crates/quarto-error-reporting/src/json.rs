//! JSON-transport shape for [`DiagnosticMessage`] (bd-b9kzg).
//!
//! Lifted from `wasm-quarto-hub-client` so two callers can share
//! one wire format:
//!
//!   * the WASM render bridge (returns `RenderResponse.warnings`
//!     to hub-client and the q2-preview SPA),
//!   * the `q2 preview` server's
//!     [`/api/preview/diagnostics`](https://quarto.org) endpoint
//!     (surfaces server-side `capture_driver` / `deps` /
//!     `re_execute` diagnostics to the SPA).
//!
//! Both sites emit the same JSON shape so the SPA can merge the
//! two feeds without a translation layer. The shape matches
//! Monaco's 1-based `IMarkerData`-style line/column convention.
//!
//! ## Public surface
//!
//! * [`JsonDiagnostic`] — top-level diagnostic.
//! * [`JsonDiagnosticDetail`] — nested detail (1..N per diagnostic).
//! * [`JsonPass1Failure`] — sibling-page parse failure (bd-rqba).
//! * [`diagnostic_to_json`] — `DiagnosticMessage → JsonDiagnostic`,
//!   resolving byte offsets to 1-based line/column via
//!   [`SourceContext`].
//! * [`with_source_file`] — tag a `JsonDiagnostic` with the file
//!   it came from (used by sibling Pass-1 failures, see bd-rqba).

use serde::Serialize;

use crate::diagnostic::{DetailKind, DiagnosticKind, DiagnosticMessage};
use quarto_source_map::SourceContext;

/// One detail item in a [`JsonDiagnostic`].
#[derive(Debug, Clone, Serialize)]
pub struct JsonDiagnosticDetail {
    pub kind: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

/// A diagnostic message in transport-friendly JSON form.
///
/// Line and column numbers are 1-based to match Monaco.
#[derive(Debug, Clone, Serialize)]
pub struct JsonDiagnostic {
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    /// Source-file attribution for project-scoped diagnostics
    /// (bd-rqba). When the project pipeline emits a warning that
    /// originates in *another* file (e.g., a sidebar entry that
    /// references a sibling page), this field carries that
    /// sibling's path so the in-app overlay can label the warning
    /// with its source instead of free-floating text. `None` for
    /// page-local diagnostics whose location already pins them
    /// to the active page's source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    pub details: Vec<JsonDiagnosticDetail>,
}

/// A Pass-1 failure (parse error or metadata error) in a project
/// file *other than* the active page (bd-rqba). Active-page
/// failures take the page-render error path; siblings flow through
/// here so the overlay can render them with source attribution
/// without forcing the lenient preview to abort.
///
/// Strict-vs-lenient policy lives at the consumer (Decision D1):
/// `quarto preview` / hub-client surfaces these as warnings and
/// keeps rendering; `quarto render` (CLI) treats any non-empty
/// `pass1_failures` as a non-zero exit (`bd-creo`).
#[derive(Debug, Clone, Serialize)]
pub struct JsonPass1Failure {
    pub source_file: String,
    pub error: String,
    pub diagnostics: Vec<JsonDiagnostic>,
}

/// Convert a [`DiagnosticMessage`] to a [`JsonDiagnostic`], using
/// the [`SourceContext`] to map byte offsets to 1-based
/// line/column numbers.
pub fn diagnostic_to_json(diag: &DiagnosticMessage, ctx: &SourceContext) -> JsonDiagnostic {
    // Map the main location
    let (start_line, start_column, end_line, end_column) = if let Some(loc) = &diag.location {
        // Map start position (offset 0 relative to this SourceInfo)
        let start = loc.map_offset(0, ctx);
        // Map end position (offset = length of span)
        let end = loc
            .map_offset(loc.length(), ctx)
            .or_else(|| {
                // Fallback: if end mapping fails, try length-1
                if loc.length() > 0 {
                    loc.map_offset(loc.length() - 1, ctx)
                } else {
                    None
                }
            })
            .or_else(|| start.clone());

        match (start, end) {
            (Some(s), Some(e)) => (
                Some((s.location.row + 1) as u32),    // 1-based line
                Some((s.location.column + 1) as u32), // 1-based column
                Some((e.location.row + 1) as u32),
                Some((e.location.column + 1) as u32),
            ),
            (Some(s), None) => (
                Some((s.location.row + 1) as u32),
                Some((s.location.column + 1) as u32),
                None,
                None,
            ),
            _ => (None, None, None, None),
        }
    } else {
        (None, None, None, None)
    };

    // Convert details
    let details: Vec<JsonDiagnosticDetail> = diag
        .details
        .iter()
        .map(|detail| {
            let (d_start_line, d_start_col, d_end_line, d_end_col) =
                if let Some(loc) = &detail.location {
                    let start = loc.map_offset(0, ctx);
                    let end = loc.map_offset(loc.length(), ctx).or_else(|| start.clone());

                    match (start, end) {
                        (Some(s), Some(e)) => (
                            Some((s.location.row + 1) as u32),
                            Some((s.location.column + 1) as u32),
                            Some((e.location.row + 1) as u32),
                            Some((e.location.column + 1) as u32),
                        ),
                        (Some(s), None) => (
                            Some((s.location.row + 1) as u32),
                            Some((s.location.column + 1) as u32),
                            None,
                            None,
                        ),
                        _ => (None, None, None, None),
                    }
                } else {
                    (None, None, None, None)
                };

            let kind_str = match detail.kind {
                DetailKind::Error => "error",
                DetailKind::Info => "info",
                DetailKind::Note | DetailKind::Faded => "note",
            };

            JsonDiagnosticDetail {
                kind: kind_str.to_string(),
                content: detail.content.as_str().to_string(),
                start_line: d_start_line,
                start_column: d_start_col,
                end_line: d_end_line,
                end_column: d_end_col,
            }
        })
        .collect();

    let kind_str = match diag.kind {
        DiagnosticKind::Error => "error",
        DiagnosticKind::Warning => "warning",
        DiagnosticKind::Info => "info",
        DiagnosticKind::Note => "note",
    };

    let hints: Vec<String> = diag.hints.iter().map(|h| h.as_str().to_string()).collect();

    JsonDiagnostic {
        kind: kind_str.to_string(),
        title: diag.title.clone(),
        code: diag.code.clone(),
        problem: diag.problem.as_ref().map(|p| p.as_str().to_string()),
        hints,
        start_line,
        start_column,
        end_line,
        end_column,
        // Default unattributed; callers that know the source file
        // (e.g., the Pass-1 failure path) tag it explicitly via
        // [`with_source_file`].
        source_file: None,
        details,
    }
}

/// Tag a [`JsonDiagnostic`] with its source file (bd-rqba). Used
/// when surfacing project-scoped warnings that originate in a
/// file other than the active page.
pub fn with_source_file(mut diag: JsonDiagnostic, source_file: String) -> JsonDiagnostic {
    diag.source_file = Some(source_file);
    diag
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagnosticMessage;

    #[test]
    fn warning_with_no_location_serializes_without_position_fields() {
        let diag = DiagnosticMessage::warning("Test warning").with_code("Q-1-1");
        let ctx = SourceContext::new();
        let json = diagnostic_to_json(&diag, &ctx);
        assert_eq!(json.kind, "warning");
        assert_eq!(json.title, "Test warning");
        assert_eq!(json.code.as_deref(), Some("Q-1-1"));
        assert!(json.start_line.is_none());
        assert!(json.start_column.is_none());
    }

    #[test]
    fn error_kind_serializes_as_lowercase() {
        let diag = DiagnosticMessage::error("Boom");
        let ctx = SourceContext::new();
        assert_eq!(diagnostic_to_json(&diag, &ctx).kind, "error");
    }

    #[test]
    fn info_and_note_kinds_serialize() {
        let ctx = SourceContext::new();
        assert_eq!(
            diagnostic_to_json(&DiagnosticMessage::info("i"), &ctx).kind,
            "info"
        );
        assert_eq!(
            diagnostic_to_json(&DiagnosticMessage::new(DiagnosticKind::Note, "n"), &ctx).kind,
            "note"
        );
    }

    #[test]
    fn with_source_file_tags_the_diagnostic() {
        let json = diagnostic_to_json(
            &DiagnosticMessage::warning("Bad sibling"),
            &SourceContext::new(),
        );
        let tagged = with_source_file(json, "other.qmd".to_string());
        assert_eq!(tagged.source_file.as_deref(), Some("other.qmd"));
    }
}
