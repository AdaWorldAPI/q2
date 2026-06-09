/*
 * test_whitespace_re_compile_once.rs
 *
 * Regression test for bd-2ercw: the shared whitespace-splitting regex
 * (`\s+`) used in `native_visitor` must be compiled exactly once for the
 * life of the process, not once per visited tree-sitter node.
 *
 * Before the fix it was a function-local `Lazy<Regex>`, so a fresh `Lazy`
 * was constructed (and `\s+` recompiled the first time it was touched) on
 * every `native_visitor` call that reached the inline whitespace check. A
 * samply profile of a 565-file website render attributed ~5% of all
 * samples to the regex NFA compiler under `native_visitor`. See
 * `claude-notes/plans/2026-06-01-render-perf-profiling.md`.
 *
 * The whitespace check fires while flattening the text children of inline
 * containers such as emphasis/strong, so an emphasis-heavy document drives
 * the per-node recompile count up under the bug. `WHITESPACE_RE_COMPILE_COUNT`
 * is a process-global counter incremented inside the regex-building closure;
 * parsing such a document must not push it past 1.
 */

use pampa::pandoc::treesitter::WHITESPACE_RE_COMPILE_COUNT;
use pampa::pandoc::{ASTContext, treesitter_to_pandoc};
use pampa::utils::diagnostic_collector::DiagnosticCollector;
use std::sync::atomic::Ordering;
use tree_sitter_qmd::MarkdownParser;

fn parse_qmd(input: &str) {
    let mut parser = MarkdownParser::default();
    let input_bytes = input.as_bytes();
    let tree = parser
        .parse(input_bytes, None)
        .expect("Failed to parse input");
    let mut error_collector = DiagnosticCollector::new();
    treesitter_to_pandoc(
        &mut std::io::sink(),
        &tree,
        input_bytes,
        &ASTContext::anonymous(),
        &mut error_collector,
    )
    .expect("Failed to convert to Pandoc AST");
}

/// Build a document with many emphasis/strong spans, each carrying
/// multi-word text — every one exercises the inline whitespace check that
/// touches the shared `\s+` regex.
fn emphasis_heavy_document(spans: usize) -> String {
    let mut s = String::new();
    for i in 0..spans {
        s.push_str(&format!(
            "Paragraph {i}: *emphasized words here* and **strong words there**.\n\n"
        ));
    }
    s
}

#[test]
fn whitespace_regex_compiles_at_most_once() {
    // Parse several emphasis-heavy documents (hundreds of inline whitespace
    // checks). Under the per-node recompile bug the counter climbs with the
    // number of visited inline nodes; fixed, it never exceeds 1 for the
    // whole process no matter how many nodes are visited.
    for _ in 0..3 {
        parse_qmd(&emphasis_heavy_document(40));
    }

    let compiles = WHITESPACE_RE_COMPILE_COUNT.load(Ordering::Relaxed);
    assert!(
        compiles <= 1,
        "whitespace `\\s+` regex compiled {compiles} times; expected at most 1 \
         (per-node recompile regression — see bd-2ercw)"
    );
}
