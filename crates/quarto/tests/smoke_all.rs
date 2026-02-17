/*
 * smoke_all.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Integration tests for smoke-all document tests.
 *
 * These tests wrap the quarto-test infrastructure so that smoke-all
 * tests appear in `cargo nextest run` output.
 */

use std::path::Path;

use quarto_test::{TestResult, run_test_file};

/// Helper macro to generate smoke-all test functions.
///
/// Usage:
/// ```ignore
/// smoke_test!(test_name, "relative/path/to/file.qmd");
/// ```
macro_rules! smoke_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            let path = Path::new(manifest_dir).join("tests/smoke-all").join($path);

            let path = path.canonicalize().unwrap_or_else(|e| {
                panic!("Failed to resolve path {}: {}", path.display(), e);
            });

            match run_test_file(&path) {
                Ok(TestResult::Pass) => {}
                Ok(TestResult::Skipped(reason)) => {
                    eprintln!("Test skipped: {}", reason);
                }
                Ok(TestResult::Fail(failures)) => {
                    let messages: Vec<String> = failures
                        .iter()
                        .map(|f| format!("{} [{}]: {}", f.format, f.assertion, f.message))
                        .collect();
                    panic!("Test failed:\n{}", messages.join("\n"));
                }
                Err(e) => {
                    panic!("Test error: {}", e);
                }
            }
        }
    };
}

// ============================================================================
// Smoke-all tests
//
// Add new tests here as you create smoke-all/*.qmd files.
// ============================================================================

smoke_test!(basic_render, "basic-render.qmd");
smoke_test!(callout_note, "callout-note.qmd");
smoke_test!(clean_render, "clean-render.qmd");
smoke_test!(code_block, "code-block.qmd");
smoke_test!(expected_error, "expected-error.qmd");
smoke_test!(no_extra_files, "no-extra-files.qmd");
smoke_test!(output_files, "output-files.qmd");
