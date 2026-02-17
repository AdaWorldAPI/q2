/*
 * quarto-test/src/runner.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Test runner for executing embedded document tests.
 */

//! Test runner that orchestrates rendering and verification.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml::Value;

use crate::assertions::{LogLevel, LogMessage, VerifyContext};
use crate::spec::{TestSpec, parse_test_specs};

/// Result of running tests on a single file.
#[derive(Debug)]
pub enum TestResult {
    /// All tests passed.
    Pass,
    /// One or more tests failed.
    Fail(Vec<FailureDetail>),
    /// Tests were skipped (with reason).
    Skipped(String),
}

/// Details about a test failure.
#[derive(Debug)]
pub struct FailureDetail {
    /// Format that failed.
    pub format: String,
    /// Assertion that failed.
    pub assertion: String,
    /// Error message.
    pub message: String,
}

/// Summary of running tests on multiple files.
#[derive(Debug, Default)]
pub struct TestSummary {
    /// Number of files that passed all tests.
    pub passed: usize,
    /// Number of files that had failures.
    pub failed: usize,
    /// Number of files that were skipped.
    pub skipped: usize,
    /// Details of all failures.
    pub failures: Vec<(PathBuf, Vec<FailureDetail>)>,
}

impl TestSummary {
    /// Check if all tests passed.
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// Run tests for a single QMD file.
///
/// This reads the `_quarto.tests` metadata, renders for each format,
/// and runs the specified assertions.
pub fn run_test_file(path: &Path) -> Result<TestResult> {
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to resolve path: {}", path.display()))?;

    // Read and parse YAML frontmatter
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;

    let metadata = extract_yaml_metadata(&content)?;

    // Parse test specifications
    let (run_config, specs) = parse_test_specs(&metadata, &path)?;

    // Check if tests should be skipped
    if let Some(config) = &run_config {
        if let Some(reason) = config.should_skip() {
            return Ok(TestResult::Skipped(reason));
        }
    }

    // If no test specs, nothing to do
    if specs.is_empty() {
        return Ok(TestResult::Skipped(
            "no test specifications found".to_string(),
        ));
    }

    // Run tests for each format
    let mut failures: Vec<FailureDetail> = Vec::new();

    for spec in specs {
        let format_failures = run_format_tests(&path, &spec)?;
        failures.extend(format_failures);
    }

    if failures.is_empty() {
        Ok(TestResult::Pass)
    } else {
        Ok(TestResult::Fail(failures))
    }
}

/// Run tests for multiple files.
pub fn run_test_files(paths: &[PathBuf]) -> Result<TestSummary> {
    let mut summary = TestSummary::default();

    for path in paths {
        match run_test_file(path) {
            Ok(TestResult::Pass) => {
                summary.passed += 1;
            }
            Ok(TestResult::Fail(failures)) => {
                summary.failed += 1;
                summary.failures.push((path.clone(), failures));
            }
            Ok(TestResult::Skipped(_)) => {
                summary.skipped += 1;
            }
            Err(e) => {
                summary.failed += 1;
                summary.failures.push((
                    path.clone(),
                    vec![FailureDetail {
                        format: "N/A".to_string(),
                        assertion: "setup".to_string(),
                        message: e.to_string(),
                    }],
                ));
            }
        }
    }

    Ok(summary)
}

/// Output from rendering a document.
struct RenderOutput {
    /// Path to the output file (may not exist if render failed).
    output_path: PathBuf,
    /// Error message if render failed.
    error: Option<String>,
    /// Log messages captured during rendering.
    messages: Vec<LogMessage>,
}

/// Run tests for a single format specification.
fn run_format_tests(input_path: &Path, spec: &TestSpec) -> Result<Vec<FailureDetail>> {
    let mut failures = Vec::new();

    // Try to render the document
    let render_output = render_document(input_path, &spec.format);

    // Create verification context
    let context = VerifyContext {
        output_path: render_output.output_path.clone(),
        input_path: input_path.to_path_buf(),
        format: spec.format.clone(),
        render_error: render_output.error.clone(),
        messages: render_output.messages,
    };

    // If render failed and we don't expect errors, that's a failure
    if render_output.error.is_some() && !spec.expects_error {
        failures.push(FailureDetail {
            format: spec.format.clone(),
            assertion: "render".to_string(),
            message: context.render_error.clone().unwrap(),
        });
        return Ok(failures);
    }

    // Run each assertion
    for assertion in &spec.assertions {
        if let Err(e) = assertion.verify(&context) {
            failures.push(FailureDetail {
                format: spec.format.clone(),
                assertion: assertion.name().to_string(),
                message: e.to_string(),
            });
        }
    }

    // Clean up output file (unless keeping outputs)
    if std::env::var("QUARTO_TEST_KEEP_OUTPUTS").is_err() {
        let _ = fs::remove_file(&render_output.output_path);
        // Also try to remove supporting files directory
        let support_dir = render_output
            .output_path
            .with_extension("")
            .to_string_lossy()
            .to_string()
            + "_files";
        let _ = fs::remove_dir_all(&support_dir);
    }

    Ok(failures)
}

/// Render a document to the specified format.
///
/// Returns a RenderOutput with the output path, any error, and captured messages.
fn render_document(input_path: &Path, format: &str) -> RenderOutput {
    use quarto_core::{
        BinaryDependencies, CalloutResolveTransform, CalloutTransform, DocumentInfo,
        MetadataNormalizeTransform, ProjectContext, RenderContext, RenderOptions,
        ResourceCollectorTransform, TransformPipeline,
    };
    use quarto_system_runtime::NativeRuntime;

    let mut messages = Vec::new();

    // Determine output path
    let output_dir = input_path.parent().unwrap_or(Path::new("."));
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let extension = match format {
        "html" => "html",
        "pdf" => "pdf",
        "docx" => "docx",
        "latex" | "tex" => "tex",
        "typst" => "typ",
        _ => "html", // Default to HTML
    };

    let output_path = output_dir.join(format!("{}.{}", stem, extension));

    // Create runtime
    let runtime = NativeRuntime::new();

    // Read input
    let input_content = match fs::read(input_path) {
        Ok(content) => content,
        Err(e) => {
            let error_msg = format!("failed to read: {}: {}", input_path.display(), e);
            messages.push(LogMessage {
                level: LogLevel::Error,
                message: error_msg.clone(),
            });
            return RenderOutput {
                output_path,
                error: Some(error_msg),
                messages,
            };
        }
    };

    // Parse QMD
    let input_path_str = input_path.to_string_lossy();
    let mut output_stream = std::io::sink();

    let (mut pandoc, _context, warnings) = match pampa::readers::qmd::read(
        &input_content,
        false, // loose mode
        &input_path_str,
        &mut output_stream,
        true, // track source locations
        None, // file_id
    ) {
        Ok(result) => result,
        Err(diagnostics) => {
            let error_msg = format!(
                "parse errors:\n{}",
                diagnostics
                    .iter()
                    .map(|d| d.to_text(None))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            messages.push(LogMessage {
                level: LogLevel::Error,
                message: error_msg.clone(),
            });
            return RenderOutput {
                output_path,
                error: Some(error_msg),
                messages,
            };
        }
    };

    // Capture warnings from parsing
    for warning in warnings {
        messages.push(LogMessage {
            level: LogLevel::Warn,
            message: warning.title.clone(),
        });
    }

    // Set up render context
    let project = match ProjectContext::discover(input_path, &runtime) {
        Ok(p) => p,
        Err(e) => {
            let error_msg = format!("failed to discover project context: {}", e);
            messages.push(LogMessage {
                level: LogLevel::Error,
                message: error_msg.clone(),
            });
            return RenderOutput {
                output_path,
                error: Some(error_msg),
                messages,
            };
        }
    };
    let doc_info = DocumentInfo::from_path(input_path);
    let render_format = format_from_name(format);
    let binaries = BinaryDependencies::new();
    let options = RenderOptions {
        verbose: false,
        execute: false,
        use_freeze: false,
        output_path: Some(output_path.clone()),
    };
    let mut ctx =
        RenderContext::new(&project, &doc_info, &render_format, &binaries).with_options(options);

    // Run transform pipeline
    let mut pipeline = TransformPipeline::new();
    pipeline.push(Box::new(CalloutTransform::new()));
    pipeline.push(Box::new(CalloutResolveTransform::new()));
    pipeline.push(Box::new(MetadataNormalizeTransform::new()));
    pipeline.push(Box::new(ResourceCollectorTransform::new()));
    if let Err(e) = pipeline.execute(&mut pandoc, &mut ctx) {
        let error_msg = format!("transform pipeline failed: {}", e);
        messages.push(LogMessage {
            level: LogLevel::Error,
            message: error_msg.clone(),
        });
        return RenderOutput {
            output_path,
            error: Some(error_msg),
            messages,
        };
    }

    // Get output directory and stem
    let output_stem = output_path.file_stem().unwrap().to_str().unwrap();

    // Write resources
    let resource_paths =
        match quarto_core::resources::write_html_resources(output_dir, output_stem, &runtime) {
            Ok(paths) => paths,
            Err(e) => {
                let error_msg = format!("failed to write resources: {}", e);
                messages.push(LogMessage {
                    level: LogLevel::Error,
                    message: error_msg.clone(),
                });
                return RenderOutput {
                    output_path,
                    error: Some(error_msg),
                    messages,
                };
            }
        };

    // Render HTML body using pampa's HTML writer
    let mut body_buf = Vec::new();
    if let Err(e) = pampa::writers::html::write_blocks_to(&pandoc.blocks, &mut body_buf) {
        let error_msg = format!("failed to render HTML body: {}", e);
        messages.push(LogMessage {
            level: LogLevel::Error,
            message: error_msg.clone(),
        });
        return RenderOutput {
            output_path,
            error: Some(error_msg),
            messages,
        };
    }
    let body = String::from_utf8_lossy(&body_buf).into_owned();

    // Render with template
    let html = match quarto_core::template::render_with_resources(
        &body,
        &pandoc.meta,
        &resource_paths.css,
    ) {
        Ok(h) => h,
        Err(e) => {
            let error_msg = format!("failed to apply template: {}", e);
            messages.push(LogMessage {
                level: LogLevel::Error,
                message: error_msg.clone(),
            });
            return RenderOutput {
                output_path,
                error: Some(error_msg),
                messages,
            };
        }
    };

    // Write output
    if let Err(e) = fs::create_dir_all(output_dir) {
        let error_msg = format!("failed to create output directory: {}", e);
        messages.push(LogMessage {
            level: LogLevel::Error,
            message: error_msg.clone(),
        });
        return RenderOutput {
            output_path,
            error: Some(error_msg),
            messages,
        };
    }
    if let Err(e) = fs::write(&output_path, html) {
        let error_msg = format!("failed to write output: {}", e);
        messages.push(LogMessage {
            level: LogLevel::Error,
            message: error_msg.clone(),
        });
        return RenderOutput {
            output_path,
            error: Some(error_msg),
            messages,
        };
    }

    // Success!
    RenderOutput {
        output_path,
        error: None,
        messages,
    }
}

/// Convert a format name string to a Format instance.
fn format_from_name(name: &str) -> quarto_core::Format {
    use quarto_core::Format;
    match name {
        "html" => Format::html(),
        "pdf" => Format::pdf(),
        "docx" => Format::docx(),
        // Default to HTML for unknown formats
        _ => Format::html(),
    }
}

/// Extract YAML metadata from QMD content.
fn extract_yaml_metadata(content: &str) -> Result<Value> {
    // Look for YAML frontmatter delimited by ---
    let content = content.trim_start();

    if !content.starts_with("---") {
        return Ok(Value::Mapping(Default::default()));
    }

    // Find the closing ---
    let rest = &content[3..];
    let end = rest
        .find("\n---")
        .or_else(|| rest.find("\r\n---"))
        .context("unterminated YAML frontmatter")?;

    let yaml_str = &rest[..end];

    serde_yaml::from_str(yaml_str).context("failed to parse YAML frontmatter")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_yaml_metadata() {
        let content = r#"---
title: Test
format: html
_quarto:
  tests:
    html:
      ensureFileRegexMatches:
        - ["pattern"]
---

Content here.
"#;

        let metadata = extract_yaml_metadata(content).unwrap();
        assert_eq!(metadata["title"].as_str(), Some("Test"));
        assert!(metadata["_quarto"]["tests"]["html"].is_mapping());
    }

    #[test]
    fn test_extract_yaml_metadata_no_frontmatter() {
        let content = "Just some content without frontmatter.";
        let metadata = extract_yaml_metadata(content).unwrap();
        assert!(metadata.as_mapping().unwrap().is_empty());
    }
}
