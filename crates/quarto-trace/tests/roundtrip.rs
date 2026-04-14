//! Tests for round-trip serialization, forward compatibility, and build metadata.

use quarto_trace::read::read_trace;
use quarto_trace::write::write_trace;
use quarto_trace::{
    BUILD_GIT_HASH, RenderInfo, SCHEMA_VERSION, StageErrorInfo, StageStatus, TraceDocument,
    TraceEntry,
};
use serde_json::json;

fn sample_doc() -> TraceDocument {
    let render = RenderInfo {
        input_path: Some("doc.qmd".into()),
        output_path: Some("doc.html".into()),
        format_target: Some("html".into()),
        started_at_unix_ms: Some(1_799_200_496_000.0),
        git_hash: Some("abc1234-dirty".into()),
        total_duration_ms: Some(123.4),
    };

    let mut doc = TraceDocument::new(render);
    doc.pipeline.push(TraceEntry {
        stage: "parse".into(),
        index: 0,
        data_kind: Some("DocumentAst".into()),
        data: Some(json!({"blocks": []})),
        duration_ms: Some(1.2),
        status: StageStatus::Ok,
        error: None,
    });
    doc.pipeline.push(TraceEntry {
        stage: "engine-execution".into(),
        index: 1,
        data_kind: None,
        data: None,
        duration_ms: None,
        status: StageStatus::Error,
        error: Some(StageErrorInfo {
            message: "jupyter kernel died".into(),
        }),
    });
    doc.pipeline.push(TraceEntry {
        stage: "render-html-body".into(),
        index: 2,
        data_kind: None,
        data: None,
        duration_ms: None,
        status: StageStatus::Skipped,
        error: None,
    });
    doc
}

#[test]
fn test_roundtrip_through_disk() {
    let tmp = std::env::temp_dir().join("quarto-trace-roundtrip");
    let _ = std::fs::remove_dir_all(&tmp);
    let path = tmp.join("latest.json");

    let doc = sample_doc();
    write_trace(&doc, &path).unwrap();

    let read_back = read_trace(&path).unwrap();

    assert_eq!(read_back.schema_version, SCHEMA_VERSION);
    assert_eq!(read_back.render.input_path, doc.render.input_path);
    assert_eq!(read_back.render.git_hash, doc.render.git_hash);
    assert_eq!(read_back.pipeline.len(), 3);

    assert_eq!(read_back.pipeline[0].stage, "parse");
    assert_eq!(read_back.pipeline[0].status, StageStatus::Ok);
    assert!(read_back.pipeline[0].data.is_some());

    assert_eq!(read_back.pipeline[1].stage, "engine-execution");
    assert_eq!(read_back.pipeline[1].status, StageStatus::Error);
    assert!(read_back.pipeline[1].data.is_none());
    assert_eq!(
        read_back.pipeline[1].error.as_ref().unwrap().message,
        "jupyter kernel died"
    );

    assert_eq!(read_back.pipeline[2].status, StageStatus::Skipped);
}

#[test]
fn test_forward_compat_unknown_status() {
    // A trace written by a future version that includes a status variant we
    // don't recognize yet should deserialize with `Unknown`, not fail.
    let json_text = r#"{
      "schema_version": 1,
      "render": {},
      "pipeline": [
        { "stage": "future-stage", "index": 0, "status": "partially-executed" }
      ]
    }"#;
    let doc: TraceDocument = serde_json::from_str(json_text).unwrap();
    assert_eq!(doc.pipeline[0].status, StageStatus::Unknown);
}

#[test]
fn test_forward_compat_unknown_fields() {
    // Unknown fields at any level should not cause deserialization to fail —
    // future writers can add new metadata without breaking today's readers.
    let json_text = r#"{
      "schema_version": 2,
      "render": {"new_future_field": 42},
      "pipeline": [
        { "stage": "parse", "index": 0, "status": "ok",
          "speculative_delta_base": 0 }
      ],
      "new_top_level_field": "hi"
    }"#;
    let doc: TraceDocument = serde_json::from_str(json_text).unwrap();
    assert_eq!(doc.schema_version, 2);
    assert_eq!(doc.pipeline[0].stage, "parse");
}

#[test]
fn test_legacy_trace_without_status_defaults_to_ok() {
    // Pre-status traces should default to Ok.
    let json_text = r#"{
      "schema_version": 1,
      "render": {},
      "pipeline": [ { "stage": "parse", "index": 0 } ]
    }"#;
    let doc: TraceDocument = serde_json::from_str(json_text).unwrap();
    assert_eq!(doc.pipeline[0].status, StageStatus::Ok);
}

#[test]
fn test_build_git_hash_populated() {
    // The env! captured at build time should never be empty.
    assert!(!BUILD_GIT_HASH.is_empty());
    // In a normal dev/CI build the hash looks like 7 hex chars, optionally
    // with `-dirty`. In tarball builds it's `unknown`. All three are OK.
    let is_unknown = BUILD_GIT_HASH == "unknown";
    let core = BUILD_GIT_HASH.trim_end_matches("-dirty");
    let looks_like_hash = core.len() >= 7 && core.chars().all(|c| c.is_ascii_hexdigit());
    assert!(
        is_unknown || looks_like_hash,
        "BUILD_GIT_HASH = {:?}",
        BUILD_GIT_HASH
    );
}
