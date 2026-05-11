/*
 * attribution/source.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Traits for querying attribution and constructing it from a producer.
//!
//! - [`AttributionSourceProvider`] is the producer-side trait
//!   (`GitBlameProvider`, `PreBuiltAttributionProvider`). Implementations
//!   return a fully-built [`AttributionData`] that satisfies the Phase 6
//!   producer invariant (every actor referenced by `runs` has an entry
//!   in `identities`).
//! - [`AttributionSource`] is the consumer-side trait, blanket-impl'd
//!   for [`AttributionMap`]. Queries return the most-recent
//!   `(actor, time)` hit overlapping the byte range.

use super::types::{AttributionData, AttributionHit, AttributionMap};
use crate::Result;
use crate::render::RenderContext;

/// Producer-side trait: build a complete [`AttributionData`] for the
/// document under render.
///
/// **The method is sync, not async.** The only blocking implementor is
/// `GitBlameProvider`, which spawns one `git blame --porcelain`
/// subprocess (~tens of ms typical, ~1s on very large repos). v1's
/// native render is single-document-at-a-time, so the calling thread
/// has no other work to compete with. The WASM implementor
/// (`PreBuiltAttributionProvider`) is purely sync (JSON parse +
/// intern loop). A future caller that needs cooperative scheduling
/// can wrap the sync `build` in `tokio::task::spawn_blocking` at the
/// call site without touching this trait.
pub trait AttributionSourceProvider: Send + Sync {
    /// Build a complete attribution payload for `ctx`'s document.
    ///
    /// May block. Implementations that spawn subprocesses or do other
    /// blocking I/O should document expected latency. Currently:
    /// `GitBlameProvider` blocks on a `git blame --porcelain`
    /// subprocess; `PreBuiltAttributionProvider` is non-blocking.
    fn build(&self, ctx: &RenderContext) -> Result<AttributionData>;
}

/// Consumer-side trait: query the most-recent `(actor, time)` hit
/// overlapping a byte range.
///
/// No `file_id` parameter — single-doc invariant in v1. v2 multi-file
/// blame re-introduces it.
pub trait AttributionSource: Send + Sync {
    fn query_byte_range(&self, start: usize, end: usize) -> Option<AttributionHit>;
}

impl AttributionSource for AttributionMap {
    fn query_byte_range(&self, _start: usize, _end: usize) -> Option<AttributionHit> {
        unimplemented!("Phase 1 — binary-search impl over runs")
    }
}
