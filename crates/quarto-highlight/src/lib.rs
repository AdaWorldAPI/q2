//! Syntax highlighting for Quarto 2 code blocks via tree-sitter.
//!
//! See `claude-notes/plans/2026-04-19-syntax-highlighting-design.md` for
//! design context. The public surface is intentionally small:
//!
//! - [`highlight`] — given a language class and source text, produce the
//!   JSON triple-array encoding (`[[start, end, capture], …]`) written to
//!   `CodeBlock` / `Code` nodes as the `data-hl-spans` attribute value.
//! - [`HighlightSpan`] — the in-memory form of one `[start, end, capture]`
//!   triple, used internally and by the pipeline stage.
//! - [`is_language_supported`] — test whether a class has a built-in
//!   grammar + query registered.

pub mod encoding;
pub mod error;
mod langs;
pub mod registry;

pub use encoding::HighlightSpan;
pub use error::HighlightError;

use crate::registry::Registry;

/// Attribute key used to carry highlight spans on `CodeBlock` and `Code`.
pub const SPANS_ATTR_KEY: &str = "data-hl-spans";

/// Return the JSON triple-array encoding for highlighting `source` as
/// `language_class`, or `None` if the class has no registered grammar.
///
/// The return value is suitable for direct placement in
/// `CodeBlock.attr`'s key-value list under [`SPANS_ATTR_KEY`].
pub fn highlight(language_class: &str, source: &str) -> Result<Option<String>, HighlightError> {
    Registry::global().highlight(language_class, source)
}

/// Whether a given language class resolves to a registered grammar.
pub fn is_language_supported(language_class: &str) -> bool {
    Registry::global().resolve(language_class).is_some()
}
