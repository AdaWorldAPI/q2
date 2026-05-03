//! Tests for round-trip serialization, forward compatibility, and build metadata.

use quarto_trace::read::{list_traces, read_trace};
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

/// bd-5qnj Phase 1a: writer emits compact JSON on disk.
///
/// The on-disk artifact must not be pretty-printed — pretty-print accounts
/// for ~80% of bytes on real traces (`claude-notes/plans/5qnj-trace-size-investigation/measurements.md`).
/// Humans who want a pretty view use `quarto trace show` (which formats
/// from the parsed `TraceDocument`) or `jq` on the file.
#[test]
fn test_writer_emits_compact_json_on_disk() {
    let tmp = std::env::temp_dir().join(format!(
        "quarto-trace-compact-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("latest.json");

    write_trace(&sample_doc(), &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let s = std::str::from_utf8(&bytes).unwrap();

    // Compact JSON has no `\n` between top-level keys and no leading
    // indentation. `serde_json::to_writer_pretty` writes one token per
    // line with two-space indentation; both signatures are absent in
    // compact output.
    assert!(
        !s.contains("\n  "),
        "trace file appears to be pretty-printed (found indented line); first 200 bytes: {:?}",
        &s.chars().take(200).collect::<String>()
    );
    // Pretty output also starts with `{\n  "schema_version"`; compact
    // starts with `{"schema_version"`.
    assert!(
        s.starts_with("{\"schema_version\""),
        "expected compact start, got: {:?}",
        &s.chars().take(40).collect::<String>()
    );

    // Sanity: still parses back to an equivalent doc.
    let read_back = read_trace(&path).unwrap();
    assert_eq!(read_back.pipeline.len(), sample_doc().pipeline.len());
    let _ = std::fs::remove_dir_all(&tmp);
}

/// bd-5qnj Phase 1b: writer emits gzipped bytes when the path ends in
/// `.gz`, and `read_trace` transparently inflates them.
#[test]
fn test_roundtrip_through_gzipped_disk() {
    let tmp = std::env::temp_dir().join(format!(
        "quarto-trace-gz-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("latest.json.gz");

    let doc = sample_doc();
    write_trace(&doc, &path).unwrap();

    // Bytes on disk must look like a gzip stream (magic 0x1f 0x8b).
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b,
        "expected gzip magic at start, got first 4 bytes = {:x?}",
        &bytes[..bytes.len().min(4)]
    );

    // Reader recognizes the .gz extension and transparently inflates.
    let read_back = read_trace(&path).unwrap();
    assert_eq!(read_back.schema_version, SCHEMA_VERSION);
    assert_eq!(read_back.pipeline.len(), doc.pipeline.len());
    assert_eq!(read_back.pipeline[0].stage, "parse");
    assert_eq!(read_back.pipeline[1].status, StageStatus::Error);
    assert_eq!(read_back.pipeline[2].status, StageStatus::Skipped);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// bd-5qnj Phase 1b: legacy (pre-Phase-1) `latest.json` files written by
/// older `quarto` versions must still be readable. Pretty or compact —
/// either is valid input.
#[test]
fn test_read_legacy_uncompressed_json() {
    let tmp = std::env::temp_dir().join(format!(
        "quarto-trace-legacy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("latest.json");

    // Hand-write a pretty-printed legacy trace as if produced by an older
    // `quarto` version (mirrors the pre-Phase-1 on-disk format).
    let pretty = serde_json::to_string_pretty(&sample_doc()).unwrap();
    std::fs::write(&path, &pretty).unwrap();

    let read_back = read_trace(&path).unwrap();
    assert_eq!(read_back.schema_version, SCHEMA_VERSION);
    assert_eq!(read_back.pipeline.len(), sample_doc().pipeline.len());

    let _ = std::fs::remove_dir_all(&tmp);
}

/// bd-5qnj Phase 1b: `list_traces` discovers both `latest.json` and
/// `latest.json.gz` artifacts. New traces are gzipped; old uncompressed
/// traces co-existing with new ones must still be listed.
#[test]
fn test_list_traces_finds_gzipped_and_uncompressed() {
    let tmp = std::env::temp_dir().join(format!(
        "quarto-trace-list-mix-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // doc-a: gzipped (the new default).
    let dir_a = tmp.join("a");
    std::fs::create_dir_all(&dir_a).unwrap();
    write_trace(&sample_doc(), &dir_a.join("latest.json.gz")).unwrap();

    // doc-b: legacy uncompressed (simulates an existing trace dir).
    let dir_b = tmp.join("b");
    std::fs::create_dir_all(&dir_b).unwrap();
    let pretty = serde_json::to_string_pretty(&sample_doc()).unwrap();
    std::fs::write(dir_b.join("latest.json"), &pretty).unwrap();

    let listings = list_traces(&tmp);
    let stems: std::collections::BTreeSet<_> =
        listings.iter().map(|l| l.doc_stem.clone()).collect();
    assert!(
        stems.contains("a"),
        "missing gzipped trace listing: {:?}",
        stems
    );
    assert!(
        stems.contains("b"),
        "missing uncompressed trace listing: {:?}",
        stems
    );

    // Each listing's path must round-trip through read_trace.
    for l in &listings {
        let _ = read_trace(&l.latest_path).expect("listed trace must be readable");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

/// bd-5qnj Phase 1: when both `latest.json.gz` and a stale `latest.json`
/// exist in the same directory, `list_traces` must prefer the `.gz`
/// (newer) artifact. This guards against a future regression where a
/// pre-Phase-1 trace lingers next to a freshly-written gzipped one.
#[test]
fn test_list_traces_prefers_gz_when_both_present() {
    let tmp = std::env::temp_dir().join(format!(
        "quarto-trace-prefer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dir = tmp.join("doc");
    std::fs::create_dir_all(&dir).unwrap();

    // Stale uncompressed file with a marker we can recognize.
    let mut stale = sample_doc();
    stale.render.input_path = Some("STALE".into());
    let pretty = serde_json::to_string_pretty(&stale).unwrap();
    std::fs::write(dir.join("latest.json"), &pretty).unwrap();

    // Fresh gzipped file is what should be reported.
    let mut fresh = sample_doc();
    fresh.render.input_path = Some("FRESH".into());
    write_trace(&fresh, &dir.join("latest.json.gz")).unwrap();

    let listings = list_traces(&tmp);
    let entry = listings.iter().find(|l| l.doc_stem == "doc").unwrap();
    let read_back = read_trace(&entry.latest_path).unwrap();
    assert_eq!(
        read_back.render.input_path.as_deref(),
        Some("FRESH"),
        "expected list_traces to prefer the .gz file"
    );

    let _ = std::fs::remove_dir_all(&tmp);
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
