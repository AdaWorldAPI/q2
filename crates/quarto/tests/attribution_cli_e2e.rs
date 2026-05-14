/*
 * attribution_cli_e2e.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! End-to-end regression test for `q2 render --attribution=git`.
//!
//! Builds a temp git repo on every invocation (`tempdir` + `git init`
//! + two scripted commits by distinct authors), with
//! `GIT_AUTHOR_DATE` / `GIT_COMMITTER_DATE` / author identities pinned
//! so the porcelain output and commit hashes are bit-deterministic.
//! Copies `crates/quarto-core/tests/fixtures/attribution-blame/doc.qmd`
//! into the tempdir, then runs
//! `q2 render <tempdir>/doc.qmd --to html --attribution=git` and
//! asserts the full `data-attr-*` contract on the produced HTML:
//!
//! * `data-attr-actor` — the author email (per-commit blame credit).
//! * `data-attr-time`  — Unix epoch **seconds** for the git provider.
//!                       (Automerge / hub-client uses ms; the unit is
//!                       part of the wire contract — see
//!                       `docs/authoring/attribution.qmd`.)
//! * `data-attr-name`  — derived display name (mail-local-part).
//! * `data-attr-color` — deterministic `hsl(...)` from the email hash.
//!
//! This is the one test that exercises the live `git blame --porcelain`
//! shell-out (`GitBlameProvider::build` in
//! `crates/quarto-core/src/attribution/git_blame.rs`); the fixture-
//! based unit tests in `attribution_gitblame.rs` only cover the
//! parser. Any regression in CLI flag wiring, working-directory
//! resolution, or porcelain handling on real git output should surface
//! here first.

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

    // data-attr-actor — author email per commit blame credit.
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

    // data-attr-time — Unix epoch SECONDS for the git provider. The
    // scripted commit times (@1700000000, @1700100000) flow through
    // `git blame --porcelain`'s author-time and must arrive verbatim.
    // A regression to milliseconds would shift to 13-digit values
    // (1_700_000_000_000) and fail this assertion.
    assert!(
        html.contains("data-attr-time=\"1700000000\""),
        "alice's commit time (seconds) must appear as data-attr-time; html:\n{}",
        html
    );
    assert!(
        html.contains("data-attr-time=\"1700100000\""),
        "bob's commit time (seconds) must appear as data-attr-time; html:\n{}",
        html
    );

    // data-attr-name — display name derived from the email
    // local-part. Pins the derivation that
    // `docs/authoring/attribution.qmd` advertises ("mail-local-part
    // plus a deterministic HSL colour").
    assert!(
        html.contains("data-attr-name=\"alice\""),
        "alice's display name must appear; html:\n{}",
        html
    );
    assert!(
        html.contains("data-attr-name=\"bob\""),
        "bob's display name must appear; html:\n{}",
        html
    );

    // data-attr-color — deterministic hsl() from the email hash. We
    // don't pin specific hue values (the palette function may evolve)
    // but the wire format is part of the contract: it must be an
    // `hsl(...)` triple, distinct between the two authors so the
    // per-actor derivation is exercised end-to-end.
    let alice_color =
        extract_attr_value(&html, ALICE_EMAIL, "data-attr-color").expect("alice color present");
    let bob_color =
        extract_attr_value(&html, BOB_EMAIL, "data-attr-color").expect("bob color present");
    assert!(
        alice_color.starts_with("hsl("),
        "alice's data-attr-color must be hsl(); got {alice_color}"
    );
    assert!(
        bob_color.starts_with("hsl("),
        "bob's data-attr-color must be hsl(); got {bob_color}"
    );
    assert_ne!(
        alice_color, bob_color,
        "per-actor color derivation must yield distinct hues"
    );
}

/// Look up the value of `attr` on the same element that carries
/// `data-attr-actor="<email>"`. Returns the substring between the
/// quotes after `attr=`, or `None` if the pairing isn't found.
///
/// Used by the color assertions to extract values for comparison
/// without hard-coding the palette function's output — keeps the test
/// stable across deterministic palette tweaks while still pinning the
/// per-actor distinctness contract.
fn extract_attr_value(html: &str, actor_email: &str, attr: &str) -> Option<String> {
    let actor_marker = format!("data-attr-actor=\"{}\"", actor_email);
    let needle = format!("{}=\"", attr);
    // Walk every occurrence of the actor marker; the matching attr
    // sits on the same tag, which in this writer means within the
    // same `<...>` element opener.
    for actor_at in html.match_indices(&actor_marker).map(|(i, _)| i) {
        let tag_start = html[..actor_at].rfind('<')?;
        let tag_end = html[tag_start..].find('>')? + tag_start;
        let tag = &html[tag_start..=tag_end];
        if let Some(attr_at) = tag.find(&needle) {
            let value_start = attr_at + needle.len();
            let value_end = tag[value_start..].find('"')? + value_start;
            return Some(tag[value_start..value_end].to_string());
        }
    }
    None
}
