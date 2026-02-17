//! Test command implementation
//!
//! Runs embedded document tests defined via `_quarto.tests` YAML metadata.
//!
//! ## Usage
//!
//! ```bash
//! quarto call test <file1.qmd> [file2.qmd ...]
//! ```

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use quarto_test::run_test_files;

pub fn execute(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return Err(anyhow!(
            "Usage: quarto call test <file1.qmd> [file2.qmd ...]\n\n\
             Runs embedded document tests defined via _quarto.tests YAML metadata."
        ));
    }

    let paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();

    // Validate that all files exist
    for path in &paths {
        if !path.exists() {
            return Err(anyhow!("File not found: {}", path.display()));
        }
    }

    println!("Running tests for {} file(s)...\n", paths.len());

    let summary = run_test_files(&paths)?;

    // Print results for each file
    for (path, failures) in &summary.failures {
        println!("FAIL: {}", path.display());
        for failure in failures {
            println!(
                "  {} [{}]: {}",
                failure.format, failure.assertion, failure.message
            );
        }
        println!();
    }

    // Print summary
    println!(
        "Results: {} passed, {} failed, {} skipped",
        summary.passed, summary.failed, summary.skipped
    );

    if summary.all_passed() {
        Ok(())
    } else {
        Err(anyhow!("{} test(s) failed", summary.failed))
    }
}
