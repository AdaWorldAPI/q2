/*
 * attribution/git_blame.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Native `git blame --porcelain` attribution provider.
//!
//! Shells out to `git` via `RenderContext::binaries.git` (so
//! `QUARTO_GIT` overrides work the same way as `QUARTO_PANDOC` etc.).
//! Pure-Rust port of the TS `attribution-gitblame.ts` adapter from
//! `feat/node-attribution`; multi-byte UTF-8 line lengths are computed
//! via `s.as_bytes().len()` (TextEncoder equivalent).

use super::source::AttributionSourceProvider;
use super::types::AttributionData;
use crate::Result;
use crate::render::RenderContext;

/// One parsed porcelain record per source line.
///
/// `author_mail` has the angle brackets stripped — used as the actor
/// identifier (matching the TS prototype).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub author: String,
    pub author_mail: String,
    pub author_time: i64,
}

/// A line-level blame record expanded to a byte range against the
/// source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameRun {
    pub byte_start: usize,
    pub byte_end: usize,
    pub actor: String,
    pub time: i64,
}

/// Parse `git blame --porcelain` output into one [`BlameLine`] per
/// source line. Commit metadata is emitted only on the first
/// appearance of each commit; the parser caches by commit hash so
/// every line record is fully populated.
pub fn parse_blame_porcelain(_output: &str) -> Vec<BlameLine> {
    unimplemented!("Phase 3a — porcelain state machine; mirror TS attribution-gitblame.ts")
}

/// Expand line-level blame records into byte-ranged runs using the
/// in-memory source text as the source of truth for per-line byte
/// lengths. UTF-8 is handled via `s.as_bytes().len()` — the
/// porcelain's tab-prefixed content is never trusted for byte
/// arithmetic.
pub fn build_blame_runs(_blame: &[BlameLine], _text: &str) -> Result<Vec<BlameRun>> {
    unimplemented!("Phase 3a — line-to-byte expansion with explicit newline accounting")
}

/// Shells out to `git blame --porcelain` (via `ctx.binaries.git`)
/// and returns a complete [`AttributionData`] for the document under
/// render.
///
/// Graceful degradation: when git is unavailable (binary not found,
/// document not in a working tree, etc.), emits a diagnostic warning
/// and returns an empty `AttributionData`; the pipeline behaves as
/// if attribution were off.
#[derive(Debug, Clone, Default)]
pub struct GitBlameProvider;

impl GitBlameProvider {
    pub fn new() -> Self {
        Self
    }
}

impl AttributionSourceProvider for GitBlameProvider {
    fn build(&self, _ctx: &RenderContext) -> Result<AttributionData> {
        unimplemented!(
            "Phase 3a — spawn git blame --porcelain, parse, build runs, synthesize identities, \
             route through AttributionDataBuilder"
        )
    }
}
