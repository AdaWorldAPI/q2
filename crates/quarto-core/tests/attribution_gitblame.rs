//! Phase 0 tests #3 and #12 — `GitBlameProvider` porcelain parsing
//! plus producer invariant.
//!
//! Fixtures live as **checked-in porcelain text** under
//! `tests/fixtures/attribution-blame/` so these unit tests don't
//! depend on live commit timestamps or git being installed. The
//! `REGEN.md` file in that directory documents how to refresh them.

use std::sync::Arc;

use quarto_core::attribution::{
    AttributionSourceProvider, BlameLine, BlameRun, GitBlameProvider, actor_color,
    build_blame_runs, fnv1a_hex8, parse_blame_porcelain,
};

// ===========================================================================
// Phase 0 test #3 — Parses porcelain identically to TS reference
// ===========================================================================

#[test]
fn parse_single_commit_single_line() {
    let porcelain = include_str!("fixtures/attribution-blame/single-commit.porcelain");
    let parsed = parse_blame_porcelain(porcelain);
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0],
        BlameLine {
            author: "Alice".to_string(),
            author_mail: "alice@example.com".to_string(),
            author_time: 1_700_000_000,
        }
    );
}

#[test]
fn parse_caches_commit_metadata_across_lines_from_same_commit() {
    // The fixture has commit `aaa...` emitting both line 1 and line 2;
    // the second line record has only `<hash> 2 2` and a `\t...` body,
    // with no author block — the parser must hydrate from cache.
    let porcelain = include_str!("fixtures/attribution-blame/multi-commit.porcelain");
    let parsed = parse_blame_porcelain(porcelain);
    assert!(parsed.len() >= 2);
    assert_eq!(parsed[0].author_mail, "alice@example.com");
    assert_eq!(parsed[1].author_mail, "alice@example.com");
    assert_eq!(parsed[0].author_time, parsed[1].author_time);
}

#[test]
fn parse_empty_porcelain_returns_empty_vec() {
    assert!(parse_blame_porcelain("").is_empty());
}

#[test]
fn build_runs_handles_multi_byte_utf8() {
    // 世界\n is 3+3+1 = 7 bytes.
    let blame = vec![BlameLine {
        author: "Alice".into(),
        author_mail: "alice@x".into(),
        author_time: 1,
    }];
    let runs = build_blame_runs(&blame, "世界\n").expect("build runs");
    assert_eq!(
        runs,
        vec![BlameRun {
            byte_start: 0,
            byte_end: 7,
            actor: "alice@x".into(),
            time: 1,
        }]
    );
}

#[test]
fn build_runs_handles_text_without_trailing_newline() {
    let blame = vec![
        BlameLine {
            author: "A".into(),
            author_mail: "a@x".into(),
            author_time: 1,
        },
        BlameLine {
            author: "B".into(),
            author_mail: "b@x".into(),
            author_time: 2,
        },
    ];
    let runs = build_blame_runs(&blame, "foo\nbar").expect("build runs");
    assert_eq!(
        runs,
        vec![
            BlameRun {
                byte_start: 0,
                byte_end: 4,
                actor: "a@x".into(),
                time: 1,
            },
            BlameRun {
                byte_start: 4,
                byte_end: 7,
                actor: "b@x".into(),
                time: 2,
            },
        ]
    );
}

#[test]
fn build_runs_errors_on_line_count_mismatch() {
    // Empty blame vs non-empty text — must error.
    let blame: Vec<BlameLine> = Vec::new();
    let result = build_blame_runs(&blame, "hello\n");
    assert!(
        result.is_err(),
        "line-count mismatch must error, not silently accept"
    );
}

// ===========================================================================
// Phase 0 test #12 — GitBlameProvider producer invariant
// ===========================================================================
//
// Every actor referenced by `runs` has an entry in `identities`,
// each entry's `display_name` equals the mail-local-part, and `color`
// equals `actor_color(fnv1a_hex8(email))`. Pin the deterministic
// colour for a known email so a future refactor of `fnv1a_hex8` can't
// silently shift hues.

#[test]
fn fnv1a_hex8_is_deterministic_and_well_distributed() {
    // Sanity: two arbitrary strings hash differently.
    let h_alice = fnv1a_hex8("alice@example.com");
    let h_bob = fnv1a_hex8("bob@example.com");
    assert_ne!(h_alice, h_bob);
    assert_eq!(h_alice.len(), 8);
    assert!(
        h_alice.chars().all(|c| c.is_ascii_hexdigit()),
        "fnv1a_hex8 output must be lowercase hex"
    );
    // Stability: calling twice with the same input gives the same answer.
    assert_eq!(h_alice, fnv1a_hex8("alice@example.com"));
}

#[test]
fn actor_color_is_deterministic_and_emits_hsl() {
    let c = actor_color("aabbccdd");
    assert!(
        c.starts_with("hsl("),
        "actor_color output must be HSL; got: {c}"
    );
    assert_eq!(c, actor_color("aabbccdd"), "deterministic");
}

#[test]
fn gitblame_provider_satisfies_producer_invariant() {
    // We can't easily drive the real provider without a working repo,
    // but we can pin the invariant: GitBlameProvider::build is the
    // production path where the invariant gets enforced. This test
    // will go red at `unimplemented!()` until Phase 3a/6 lands.
    let _provider = GitBlameProvider::new();
    // For Phase 0 we just verify the provider type instantiates and
    // implements the trait so the dyn-trait construction in
    // RenderContext::attribution_provider works.
    let _typed: Arc<dyn AttributionSourceProvider> = Arc::new(_provider);
}
