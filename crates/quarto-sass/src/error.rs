//! Error types for SASS operations.
//!
//! Copyright (c) 2025 Posit, PBC

use std::path::PathBuf;

use quarto_source_map::SourceInfo;
use thiserror::Error;

/// Errors that can occur during SASS operations
#[derive(Debug, Error)]
pub enum SassError {
    /// Layer parsing failed - no boundary markers found
    #[error("SCSS content doesn't contain any layer boundary markers (/*-- scss:defaults --*/, /*-- scss:rules --*/, etc.){}", .hint.as_ref().map(|h| format!(" in {}", h)).unwrap_or_default())]
    NoBoundaryMarkers { hint: Option<String> },

    /// SASS compilation failed
    #[error("SASS compilation failed: {message}")]
    CompilationFailed { message: String },

    /// Unknown theme name
    #[error("Unknown theme: {0}")]
    UnknownTheme(String),

    /// Theme file not found in embedded resources
    #[error("Theme file not found: {0}")]
    ThemeNotFound(String),

    /// Custom theme file not found on filesystem
    #[error("Custom theme file not found: {path}")]
    CustomThemeNotFound { path: PathBuf },

    /// Custom SCSS file doesn't have layer boundary markers
    #[error("Custom SCSS file doesn't have layer boundary markers: {path}")]
    InvalidScssFile { path: PathBuf },

    /// Invalid theme configuration in document/project config.
    ///
    /// `location` is the SourceInfo of the offending value in the
    /// declaring YAML (typically `_quarto.yml`). It's `Option<…>`
    /// because some internal call sites construct this error without
    /// a concrete ConfigValue in scope (brand parsing, etc.). When
    /// `Some`, the diagnostic path uses it to render an ariadne
    /// span at the relevant key.
    #[error("Invalid theme configuration: {message}")]
    InvalidThemeConfig {
        message: String,
        location: Option<SourceInfo>,
    },

    /// File I/O error
    #[error("Failed to read SASS file: {0}")]
    Io(#[from] std::io::Error),
}
