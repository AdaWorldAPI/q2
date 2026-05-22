//! JSON Schema drift detection for the diagnostic wire shapes
//! (bd-iey8o).
//!
//! The schema files at `crates/quarto-error-reporting/schemas/` are
//! the source-of-truth contract published to the docs site (and
//! referenced by every emitted diagnostic's `$schema` field). These
//! files MUST be in sync with the Rust types in `src/json.rs`.
//!
//! This test detects drift by re-generating each schema in-memory
//! from the current Rust type definitions and comparing to the
//! checked-in JSON. On mismatch:
//!
//! - With `QUARTO_REGEN_SCHEMAS=1` set, the test overwrites the
//!   checked-in file and passes — that's how you regenerate after a
//!   wire-shape change.
//! - Without it, the test fails with a clear message pointing at
//!   the regenerate command.
//!
//! Idiomatic invocation:
//!
//! ```text
//! # Verify (default — what CI does):
//! cargo nextest run -p quarto-error-reporting --test schema_drift
//!
//! # Regenerate after editing JsonDiagnostic / JsonPass1Failure:
//! QUARTO_REGEN_SCHEMAS=1 cargo nextest run -p quarto-error-reporting --test schema_drift
//! ```

use std::path::PathBuf;

use quarto_error_reporting::{JsonDiagnostic, JsonPass1Failure};
use serde_json::Value;

/// Recursively sort the keys of every JSON object in `value` so the
/// resulting text representation is independent of upstream map
/// iteration order.
///
/// Why: `serde_json`'s `preserve_order` feature is workspace-wide.
/// When *any* crate in the workspace activates it (transitively),
/// schemars' output ordering flips between sorted (BTreeMap) and
/// insertion-order (IndexMap). The checked-in schema files MUST be
/// stable under both. Canonical-sorting the keys here decouples the
/// file content from the feature configuration.
fn canonicalize_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, canonicalize_keys(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_keys).collect()),
        other => other,
    }
}

/// Generate the schema for type `T` and return it as a
/// pretty-printed JSON string with a trailing newline. Object keys
/// are sorted lexicographically at every depth (see
/// [`canonicalize_keys`]) so the output is independent of the
/// `serde_json/preserve_order` feature.
fn render_schema<T: schemars::JsonSchema>() -> String {
    let schema = schemars::schema_for!(T);
    let value = serde_json::to_value(&schema).expect("schema must serialize to Value");
    let canonical = canonicalize_keys(value);
    let mut s = serde_json::to_string_pretty(&canonical).expect("schema must serialize");
    s.push('\n');
    s
}

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas")
}

/// Check a single shape against its checked-in schema file. On
/// mismatch, behavior depends on `QUARTO_REGEN_SCHEMAS`:
/// `=1` writes and passes; otherwise this panics with a diff
/// summary and the regenerate command.
fn check_or_regenerate(file_name: &str, generated: &str) {
    let path = schemas_dir().join(file_name);
    let regen = std::env::var("QUARTO_REGEN_SCHEMAS").is_ok_and(|v| v == "1");

    let existing = std::fs::read_to_string(&path).ok();

    if existing.as_deref() == Some(generated) {
        return;
    }

    if regen {
        // Ensure the directory exists (first-time generation).
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create schemas/ dir");
        }
        std::fs::write(&path, generated)
            .unwrap_or_else(|e| panic!("failed to write schema to {}: {}", path.display(), e));
        eprintln!("regenerated {}", path.display());
        return;
    }

    // Build a useful failure message.
    let existing_summary = match &existing {
        None => "(no file on disk)".to_string(),
        Some(s) => format!("({} bytes)", s.len()),
    };
    panic!(
        "JSON Schema drift for {}.\n\
         The checked-in file {} {} does not match the schema generated\n\
         from the current Rust types in src/json.rs. To regenerate, run:\n\
         \n\
         \tQUARTO_REGEN_SCHEMAS=1 cargo nextest run -p quarto-error-reporting --test schema_drift\n\
         \n\
         Then review the diff before committing.",
        file_name,
        path.display(),
        existing_summary,
    );
}

#[test]
fn json_diagnostic_schema_matches_committed() {
    check_or_regenerate("json-diagnostic.json", &render_schema::<JsonDiagnostic>());
}

#[test]
fn json_pass1_failure_schema_matches_committed() {
    check_or_regenerate(
        "json-pass1-failure.json",
        &render_schema::<JsonPass1Failure>(),
    );
}
