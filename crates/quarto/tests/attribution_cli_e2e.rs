/*
 * attribution_cli_e2e.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Phase 0 test #9 — end-to-end CLI fixture with two-author git history.
//!
//! The test builds a temp git repo on every invocation
//! (`tempdir` + `git init` + two scripted commits by distinct
//! authors), with `GIT_AUTHOR_DATE` / `GIT_COMMITTER_DATE` / author
//! identities pinned so the porcelain output and commit hashes are
//! bit-deterministic. It copies
//! `crates/quarto-core/tests/fixtures/attribution-blame/doc.qmd` into
//! the tempdir, then runs
//! `cargo run --bin q2 -- render <tempdir>/doc.qmd --to html
//! --attribution=git`. Asserts the produced HTML contains
//! `data-attr-actor="<email>"` strings matching the two scripted
//! author emails.
//!
//! **Phase 0 status: RED.** Until Phase 3c lands the `--attribution`
//! flag and Phase 3a lands `GitBlameProvider`, the binary will reject
//! the flag with a clap usage error and the test will fail.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const Q2_BIN: &str = env!("CARGO_BIN_EXE_q2");

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn run_git(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd);
    // Use mutable args so we can prefix `-c commit.gpgsign=false` for
    // commit operations; it's harmless for other subcommands.
    cmd.args(["-c", "commit.gpgsign=false"]);
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1");
    // Cross-platform "no global config" — /dev/null on unix, NUL on
    // windows. Using a missing path also works.
    #[cfg(unix)]
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    #[cfg(windows)]
    cmd.env("GIT_CONFIG_GLOBAL", "NUL");
    cmd.output().expect("spawn git")
}

const ALICE_EMAIL: &str = "alice@example.com";
const BOB_EMAIL: &str = "bob@example.com";

/// Build a deterministic two-author git history under `dir`.
///
/// The first commit (Alice) contains everything up to and including
/// the first body paragraph; the second commit (Bob) appends the
/// rest. Splitting on a line boundary guarantees `git blame` credits
/// at least one rendered-body line to each author — splitting
/// mid-line would let Bob's "completion" of a partial line absorb
/// what was nominally Alice's contribution.
fn scripted_repo(dir: &Path, doc_qmd: &str) {
    let split = doc_qmd
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .find(|&i| i >= doc_qmd.len() / 2)
        .expect("doc.qmd must have at least one newline past its midpoint");
    write_file(&dir.join("doc.qmd"), &doc_qmd[..split]);

    let init = run_git(dir, &["init", "-q", "-b", "main"], &[]);
    assert!(init.status.success(), "git init failed: {:?}", init);
    let add = run_git(dir, &["add", "doc.qmd"], &[]);
    assert!(add.status.success());
    let alice_env = [
        ("GIT_AUTHOR_NAME", "Alice"),
        ("GIT_AUTHOR_EMAIL", ALICE_EMAIL),
        ("GIT_COMMITTER_NAME", "Alice"),
        ("GIT_COMMITTER_EMAIL", ALICE_EMAIL),
        ("GIT_AUTHOR_DATE", "@1700000000 +0000"),
        ("GIT_COMMITTER_DATE", "@1700000000 +0000"),
    ];
    let commit = run_git(dir, &["commit", "-q", "-m", "alice: initial"], &alice_env);
    assert!(commit.status.success(), "git commit failed: {:?}", commit);

    // Second commit: full doc, attributed to Bob.
    write_file(&dir.join("doc.qmd"), doc_qmd);
    run_git(dir, &["add", "doc.qmd"], &[]);
    let bob_env = [
        ("GIT_AUTHOR_NAME", "Bob"),
        ("GIT_AUTHOR_EMAIL", BOB_EMAIL),
        ("GIT_COMMITTER_NAME", "Bob"),
        ("GIT_COMMITTER_EMAIL", BOB_EMAIL),
        ("GIT_AUTHOR_DATE", "@1700100000 +0000"),
        ("GIT_COMMITTER_DATE", "@1700100000 +0000"),
    ];
    let commit = run_git(dir, &["commit", "-q", "-m", "bob: append"], &bob_env);
    assert!(
        commit.status.success(),
        "second commit failed: {:?}",
        commit
    );
}

fn locate_fixture() -> PathBuf {
    // Resolve relative to this test's source position: we're in
    // `crates/quarto/tests/`, the fixture is in
    // `crates/quarto-core/tests/fixtures/`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("quarto-core")
        .join("tests")
        .join("fixtures")
        .join("attribution-blame")
        .join("doc.qmd")
}

#[test]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos")),
    ignore = "git scripting fixture is unix-only for Phase 0"
)]
fn cli_attribution_git_emits_data_attr_actor_for_both_authors() {
    let fixture = locate_fixture();
    assert!(
        fixture.exists(),
        "expected fixture at {}",
        fixture.display()
    );
    let doc_qmd = std::fs::read_to_string(&fixture).expect("read fixture");

    let tmp = TempDir::new().expect("tempdir");
    scripted_repo(tmp.path(), &doc_qmd);

    let output = Command::new(Q2_BIN)
        .arg("render")
        .arg(tmp.path().join("doc.qmd"))
        .args(["--to", "html"])
        .arg("--attribution=git")
        .output()
        .expect("spawn q2");

    assert!(
        output.status.success(),
        "q2 render --attribution=git must succeed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The output html lives next to the input by default; find it.
    let html_path = tmp.path().join("doc.html");
    let html = std::fs::read_to_string(&html_path).expect("read rendered html");
    assert!(
        html.contains(&format!("data-attr-actor=\"{}\"", ALICE_EMAIL)),
        "alice's email must appear as data-attr-actor; html:\n{}",
        html
    );
    assert!(
        html.contains(&format!("data-attr-actor=\"{}\"", BOB_EMAIL)),
        "bob's email must appear as data-attr-actor; html:\n{}",
        html
    );
}
