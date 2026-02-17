/*
 * quarto-test/src/assertions/should_error.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Assertion that expects rendering to fail.
 */

//! `shouldError` assertion implementation.

use anyhow::bail;

use super::{Assertion, VerifyContext};

/// Assertion that expects rendering to fail.
///
/// This corresponds to Quarto 1's `shouldError` verification function.
/// The test passes if rendering produced an error, and fails if
/// rendering succeeded.
#[derive(Debug)]
pub struct ShouldError;

impl ShouldError {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShouldError {
    fn default() -> Self {
        Self::new()
    }
}

impl Assertion for ShouldError {
    fn name(&self) -> &str {
        "shouldError"
    }

    fn verify(&self, context: &VerifyContext) -> anyhow::Result<()> {
        match &context.render_error {
            Some(_) => {
                // Render failed as expected
                Ok(())
            }
            None => {
                // Render succeeded when it should have failed
                bail!(
                    "Expected rendering to fail, but it succeeded. Output: {}",
                    context.output_path.display()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_should_error_passes_when_error() {
        let assertion = ShouldError::new();
        let context = VerifyContext {
            output_path: PathBuf::from("/tmp/test.html"),
            input_path: PathBuf::from("/tmp/test.qmd"),
            format: "html".to_string(),
            render_error: Some("Parse error".to_string()),
            messages: vec![],
        };

        assert!(assertion.verify(&context).is_ok());
    }

    #[test]
    fn test_should_error_fails_when_success() {
        let assertion = ShouldError::new();
        let context = VerifyContext {
            output_path: PathBuf::from("/tmp/test.html"),
            input_path: PathBuf::from("/tmp/test.qmd"),
            format: "html".to_string(),
            render_error: None,
            messages: vec![],
        };

        let result = assertion.verify(&context);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Expected rendering to fail")
        );
    }
}
