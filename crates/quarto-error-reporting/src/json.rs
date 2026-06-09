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

use schemars::JsonSchema;
use serde::Serialize;

use crate::diagnostic::{DetailKind, DiagnosticKind, DiagnosticMessage};
use quarto_source_map::SourceContext;

/// One detail item in a [`JsonDiagnostic`].
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
///
/// ## `$schema` field (bd-iey8o)
///
/// Each instance carries a `$schema` field pointing at
/// [`JsonDiagnostic::SCHEMA_URL`] so that consumers reading the
/// diagnostic over the wire (CLI stderr, WASM bridge, preview API)
/// can discover the JSON Schema describing this shape without prior
/// knowledge. The field is a static-string field with a default
/// matching the const URL — the only place `JsonDiagnostic` is
/// constructed (`diagnostic_to_json`) sets it, and downstream
/// transforms like `with_source_file` preserve it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct JsonDiagnostic {
    /// JSON Schema URI describing this object's shape. Const value
    /// is [`JsonDiagnostic::SCHEMA_URL`]; included on the wire so
    /// consumers can self-discover the contract.
    #[serde(rename = "$schema")]
    #[schemars(rename = "$schema")]
    pub schema: &'static str,
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
    /// Pre-rendered ariadne source-context snippet (bd-352bh).
    /// Populated when the diagnostic carries a `location` and the
    /// converting site has a [`SourceContext`] to draw from
    /// (i.e. always, in [`diagnostic_to_json`]). Same text the
    /// `q2 render` CLI prints to stdout — ANSI-coded; strip on the
    /// JS side for browser display. Consumers can render this
    /// verbatim in a `<pre>` block for the rich source-context
    /// view, or ignore it and fall back to the structured fields
    /// for a compact summary. `None` for unlocated diagnostics
    /// (rare but possible for project-level errors with no span).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<String>,
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
///
/// `$schema` carries [`JsonPass1Failure::SCHEMA_URL`] so consumers
/// can distinguish this shape from a plain `JsonDiagnostic` line on
/// a mixed stderr stream and self-discover its contract.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct JsonPass1Failure {
    /// JSON Schema URI describing this object's shape. Const value
    /// is [`JsonPass1Failure::SCHEMA_URL`].
    #[serde(rename = "$schema")]
    #[schemars(rename = "$schema")]
    pub schema: &'static str,
    pub source_file: String,
    pub error: String,
    pub diagnostics: Vec<JsonDiagnostic>,
}

impl JsonDiagnostic {
    /// JSON Schema URI for the `JsonDiagnostic` wire shape.
    /// Versioned under `/v1/` so future incompatible changes get a
    /// new URL rather than silently breaking old consumers.
    pub const SCHEMA_URL: &'static str = "https://quarto.org/schemas/v1/json-diagnostic.json";
}

impl JsonPass1Failure {
    /// JSON Schema URI for the `JsonPass1Failure` wire shape.
    pub const SCHEMA_URL: &'static str = "https://quarto.org/schemas/v1/json-pass1-failure.json";

    /// Build a `JsonPass1Failure` for a sibling Pass-1 failure
    /// (parse or metadata error) whose diagnostics have already been
    /// converted to [`JsonDiagnostic`] form. The `$schema` field is
    /// populated from the const.
    pub fn new(source_file: String, error: String, diagnostics: Vec<JsonDiagnostic>) -> Self {
        Self {
            schema: Self::SCHEMA_URL,
            source_file,
            error,
            diagnostics,
        }
    }
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

    // bd-352bh: pre-render the ariadne source-context snippet for
    // diagnostics that have a location. `DiagnosticMessage::to_text`
    // delegates to ariadne when both the diagnostic's location AND
    // the supplied `SourceContext` are present (see
    // `crates/quarto-error-reporting/src/diagnostic.rs`'s
    // `to_text_with_options`); for locationless diagnostics the
    // function would produce a tidyverse text block instead, which
    // duplicates what the structured fields already carry. So we
    // gate on `diag.location.is_some()` to avoid shipping that
    // redundant text on the wire.
    let rendered = if diag.location.is_some() {
        Some(diag.to_text(Some(ctx)))
    } else {
        None
    };

    JsonDiagnostic {
        schema: JsonDiagnostic::SCHEMA_URL,
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
        rendered,
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

    /// bd-iey8o: every emitted `JsonDiagnostic` carries a `$schema`
    /// field with the const URL, and it survives `with_source_file`.
    #[test]
    fn diagnostic_carries_schema_url() {
        let json = diagnostic_to_json(
            &DiagnosticMessage::warning("with schema"),
            &SourceContext::new(),
        );
        assert_eq!(json.schema, JsonDiagnostic::SCHEMA_URL);

        let tagged = with_source_file(json, "a.qmd".to_string());
        assert_eq!(tagged.schema, JsonDiagnostic::SCHEMA_URL);
    }

    /// bd-iey8o: `JsonDiagnostic::SCHEMA_URL` is the value seen on
    /// the wire under the `$schema` key (note the `$` prefix from
    /// the serde rename).
    #[test]
    fn diagnostic_serializes_schema_field_as_dollar_schema() {
        let json = diagnostic_to_json(
            &DiagnosticMessage::warning("wire form"),
            &SourceContext::new(),
        );
        let s = serde_json::to_value(&json).unwrap();
        assert_eq!(
            s.get("$schema").and_then(|v| v.as_str()),
            Some(JsonDiagnostic::SCHEMA_URL)
        );
        assert!(
            s.get("schema").is_none(),
            "the serde rename should suppress the un-renamed `schema` key"
        );
    }

    /// bd-iey8o: `JsonPass1Failure::new` populates `$schema` from
    /// the const, and the wire form uses the `$` prefix.
    #[test]
    fn pass1_failure_carries_schema_url() {
        let f = JsonPass1Failure::new("other.qmd".to_string(), "boom".to_string(), vec![]);
        assert_eq!(f.schema, JsonPass1Failure::SCHEMA_URL);
        let s = serde_json::to_value(&f).unwrap();
        assert_eq!(
            s.get("$schema").and_then(|v| v.as_str()),
            Some(JsonPass1Failure::SCHEMA_URL)
        );
    }

    // ─── bd-352bh: ariadne `rendered` field ──────────────────────

    /// Build a `(DiagnosticMessage, SourceContext)` pair where the
    /// diagnostic has a location pointing into a registered file
    /// — enough for ariadne to draw a source-context box.
    fn synth_located_diag() -> (DiagnosticMessage, SourceContext) {
        use quarto_source_map::{
            SourceInfo,
            types::{Location, Range},
        };
        let mut ctx = SourceContext::new();
        let file_id = ctx.add_file(
            "fixture.qmd".to_string(),
            Some("# Title\n\nA paragraph that has _unclosed emphasis.\n".to_string()),
        );
        // Point at the underscore on row 2. Exact span isn't
        // important for the rendered-vs-not assertions.
        let info = SourceInfo::from_range(
            file_id,
            Range {
                start: Location {
                    offset: 28,
                    row: 2,
                    column: 19,
                },
                end: Location {
                    offset: 29,
                    row: 2,
                    column: 20,
                },
            },
        );
        let mut diag =
            DiagnosticMessage::warning("Unclosed Underscore Emphasis").with_code("Q-2-5");
        diag.location = Some(info);
        (diag, ctx)
    }

    #[test]
    fn rendered_is_some_when_location_present() {
        let (diag, ctx) = synth_located_diag();
        let json = diagnostic_to_json(&diag, &ctx);
        let rendered = json
            .rendered
            .as_deref()
            .expect("rendered should be populated when the diagnostic has a location");
        // Ariadne's source-context box always opens with the
        // U+256D "BOX DRAWINGS LIGHT ARC DOWN AND RIGHT" character.
        // Pinning that single byte sequence is robust to font /
        // padding tweaks while still proving "ariadne ran."
        assert!(
            rendered.contains('\u{256D}'),
            "rendered text should contain ariadne's box-drawing chars; got: {rendered:?}",
        );
        // The diagnostic's title and code should also appear.
        assert!(rendered.contains("Unclosed Underscore Emphasis"));
        assert!(rendered.contains("Q-2-5"));
    }

    #[test]
    fn rendered_is_none_when_location_absent() {
        let diag = DiagnosticMessage::warning("Floating warning").with_code("Q-9-9");
        let ctx = SourceContext::new();
        let json = diagnostic_to_json(&diag, &ctx);
        assert!(
            json.rendered.is_none(),
            "rendered should be None for a diagnostic without a location; got: {:?}",
            json.rendered,
        );
    }

    #[test]
    fn rendered_skipped_in_json_when_none() {
        // The serde attribute `skip_serializing_if = "Option::is_none"`
        // keeps the wire shape clean for diagnostics that don't have
        // a location — the field shouldn't appear in the JSON at all.
        let diag = DiagnosticMessage::warning("Floating warning");
        let ctx = SourceContext::new();
        let json = diagnostic_to_json(&diag, &ctx);
        let serialized = serde_json::to_string(&json).unwrap();
        assert!(
            !serialized.contains("\"rendered\""),
            "JSON should omit `rendered` when None; got: {serialized}",
        );
    }
}
