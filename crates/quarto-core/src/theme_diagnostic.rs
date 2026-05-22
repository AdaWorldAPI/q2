//! Convert [`quarto_sass::SassError`] into the project's structured
//! [`ParseError`](crate::error::ParseError) so theme-config failures
//! can be rendered as ariadne reports with a source span pointing at
//! the offending YAML.
//!
//! Mirrors the pattern established by
//! [`crate::project_resources::resource_error_to_parse_error`]
//! (bd-c1et2 / Q-5-1..Q-5-3): a domain error carrying a
//! [`SourceInfo`] is lifted into a `ParseError` that owns the
//! diagnostic message + the file content the renderer needs.
//!
//! The "Parse" in `ParseError` is historical — the type is just a
//! `Vec<DiagnosticMessage>` + `SourceContext` envelope.

use std::path::Path;

use quarto_error_reporting::DiagnosticMessageBuilder;
use quarto_sass::SassError;
use quarto_source_map::{FileId, SourceContext};

use crate::error::ParseError;

/// Build a [`ParseError`] from a [`SassError`], loading
/// `source_file` so the resulting diagnostic can render an ariadne
/// snippet pointing at the offending YAML value.
///
/// Currently handles [`SassError::InvalidThemeConfig`] specifically;
/// other variants fall back to a span-less diagnostic carrying the
/// raw error message (still tidyverse-shaped, still better than the
/// legacy plain text).
///
/// If `source_file` cannot be read or the error's `location` has no
/// resolvable `FileId` (e.g. `Concat`, `FilterProvenance`, or `None`),
/// the diagnostic still renders — just without the source snippet.
pub fn sass_error_to_parse_error(err: &SassError, source_file: &Path) -> ParseError {
    // The location only applies to InvalidThemeConfig today; pull it out
    // up front so the SourceContext loader gets the right FileId.
    let location = match err {
        SassError::InvalidThemeConfig { location, .. } => location.clone(),
        _ => None,
    };

    let mut source_context = SourceContext::new();
    if let Some(loc) = &location
        && let Some((fid_usize, _, _)) = loc.resolve_byte_range()
    {
        let content = std::fs::read_to_string(source_file).ok();
        source_context.add_file_with_id(
            FileId(fid_usize),
            source_file.to_string_lossy().into_owned(),
            content,
        );
    }

    let diagnostic = match err {
        SassError::InvalidThemeConfig { message, location } => {
            let mut b = DiagnosticMessageBuilder::error("Invalid theme configuration")
                .with_code("Q-14-1")
                .problem(message.clone());
            if let Some(loc) = location {
                b = b.with_location(loc.clone());
            }
            b.build()
        }
        // Fallback for non-theme-config SassError variants. We don't
        // expect these on the user-facing render path today, but
        // returning *something* structured is better than the legacy
        // plain `e.to_string()` form. No code is assigned — the
        // catalog only covers theme-config errors at the moment.
        other => DiagnosticMessageBuilder::error("SASS error")
            .problem(other.to_string())
            .build(),
    };

    ParseError::new(vec![diagnostic], source_context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quarto_source_map::SourceInfo;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use tempfile::TempDir;

    /// `quarto_yaml::parse_file` derives a file's `FileId` by hashing
    /// its filename. We replicate that here so the SourceInfo we
    /// produce in tests is the one the renderer will look up.
    fn file_id_for(path: &Path) -> FileId {
        let filename = path.to_string_lossy().to_string();
        let mut hasher = DefaultHasher::new();
        filename.hash(&mut hasher);
        FileId(hasher.finish() as usize)
    }

    /// Strip ANSI SGR / hyperlink escapes so substring assertions
    /// against rendered diagnostics don't break on the interleaved
    /// color codes that ariadne emits.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // CSI: ESC '[' ... letter
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for nc in chars.by_ref() {
                        if nc.is_ascii_alphabetic() {
                            break;
                        }
                    }
                    continue;
                }
                // OSC 8 hyperlink: ESC ']' ... BEL (\x07) or ESC '\\'
                if chars.peek() == Some(&']') {
                    chars.next();
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc == '\x07' {
                            break;
                        }
                        if nc == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                    continue;
                }
            }
            out.push(c);
        }
        out
    }

    #[test]
    fn invalid_theme_config_renders_with_code_and_span() {
        // End-to-end: a SassError with a SourceInfo pointing into a
        // real on-disk _quarto.yml is turned into a ParseError whose
        // diagnostic carries the Q-14-1 code, the offending message,
        // and renders an ariadne snippet of the right line.
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let yaml_path = root.join("_quarto.yml");
        // Hand-crafted contents so we know the byte offsets. The
        // `theme:` value spans the mapping starting after `theme: `
        // on line 2 — though for the diagnostic we point at the
        // whole `theme:` key+value region.
        let contents = "project:\n  type: website\ntheme:\n  light: [cosmo]\n";
        std::fs::write(&yaml_path, contents).unwrap();

        let theme_start = contents.find("theme:").unwrap();
        let theme_end = contents.len(); // through end-of-file for simplicity
        let location = SourceInfo::Original {
            file_id: file_id_for(&yaml_path),
            start_offset: theme_start,
            end_offset: theme_end,
        };

        let err = SassError::InvalidThemeConfig {
            message: "theme must be a string or array of strings".to_string(),
            location: Some(location.clone()),
        };

        let parse_err = sass_error_to_parse_error(&err, &yaml_path);
        assert_eq!(parse_err.diagnostics.len(), 1);
        let d = &parse_err.diagnostics[0];
        assert_eq!(d.code.as_deref(), Some("Q-14-1"));
        assert!(
            d.title.contains("Invalid theme configuration"),
            "title was: {}",
            d.title
        );
        assert_eq!(d.location.as_ref(), Some(&location));

        // Render with hyperlinks disabled so the assertion is
        // path-independent. The ariadne snippet should mention the
        // file and an excerpt of the contents.
        let opts = quarto_error_reporting::TextRenderOptions {
            enable_hyperlinks: false,
        };
        let rendered = d.to_text_with_options(Some(&parse_err.source_context), &opts);
        assert!(
            rendered.contains("Q-14-1"),
            "rendered output missing code Q-14-1:\n{}",
            rendered
        );
        assert!(
            rendered.contains("string or array"),
            "rendered output missing problem text:\n{}",
            rendered
        );
        // ariadne includes the source line numbers in the snippet
        // header when the location resolves successfully. The
        // mapping is independently exercised by SourceContext tests;
        // the value here is just "did we get *some* source snippet
        // back, not the plain text fallback?". `3 │` is the line-3
        // marker for the `theme:` line in the fixture. We strip ANSI
        // because the renderer interleaves escape codes per glyph,
        // which would otherwise foil a literal substring match.
        let stripped = strip_ansi(&rendered);
        assert!(
            stripped.contains("3 │"),
            "rendered output missing line marker for the `theme:` line:\n{}",
            stripped,
        );
    }

    #[test]
    fn invalid_theme_config_without_location_renders_span_less() {
        // When the SassError has no location (internal variants like
        // brand_err), the helper still produces a structured
        // diagnostic — just without an ariadne snippet.
        let err = SassError::InvalidThemeConfig {
            message: "no source info available".to_string(),
            location: None,
        };
        let parse_err = sass_error_to_parse_error(&err, Path::new("/nonexistent"));
        assert_eq!(parse_err.diagnostics.len(), 1);
        let d = &parse_err.diagnostics[0];
        assert_eq!(d.code.as_deref(), Some("Q-14-1"));
        assert_eq!(d.location, None);
    }

    #[test]
    fn theme_diagnostic_code_is_registered_in_catalog() {
        // Belt-and-braces: Q-14-1 must exist in the shared error
        // catalog, under the 'theme' subsystem.
        assert!(
            quarto_error_reporting::catalog::get_error_info("Q-14-1").is_some(),
            "Q-14-1 is not registered in error_catalog.json",
        );
        assert_eq!(
            quarto_error_reporting::catalog::get_subsystem("Q-14-1"),
            Some("theme"),
        );
    }
}
