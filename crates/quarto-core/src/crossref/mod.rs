/*
 * crossref/mod.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Crossref data structures and registry for Quarto 2.
 */

//! Crossref data structures for Quarto 2.
//!
//! This module provides the front-end representation of crossref state that is
//! built during the crossref phase of the transform pipeline and consumed by
//! both reference resolution and back-end renderers.
//!
//! Design lives in `claude-notes/plans/2026-04-15-crossref-design.md`.
//!
//! ## Trace emission
//!
//! The crossref index is surfaced to the pipeline trace through
//! [`PipelineObserver::on_auxiliary_data`] (see
//! `crates/quarto-core/src/stage/observer.rs`). Convention:
//!
//! - `stage`: `"crossref-index"` (the transform name).
//! - `kind`: `"CrossrefIndex"` (well-known tag — see
//!   [`TRACE_KIND_CROSSREF_INDEX`]).
//! - `data`: `serde_json::to_value(&CrossrefIndex)` — the same JSON shape
//!   that will later be persisted to `.quarto/xref/<file-id>.json` for
//!   multi-file merges. One shape for both pathways means Phase 4 is
//!   additive.
//!
//! `JsonTraceObserver` records this as a `TraceEntry` with `stage: "aux:..."`
//! and the `CrossrefIndex` payload. Phase 1.3's `CrossrefIndexTransform` is
//! the actual caller; this constant is the contract it commits to.

/// Well-known `kind` tag for the crossref-index payload on
/// [`crate::stage::PipelineObserver::on_auxiliary_data`].
pub const TRACE_KIND_CROSSREF_INDEX: &str = "CrossrefIndex";

pub mod codeblock_shorthand;
pub mod index;
pub mod metadata;
pub mod registry;
pub mod target;

#[cfg(test)]
mod roundtrip_tests;

pub use index::{CrossrefEntry, CrossrefIndex, HeadingRecord, Order, PromisedId, PromisedIdSource};
pub use metadata::{CrossrefMetadata, MetadataError};
pub use registry::{RefTypeDef, RefTypeRegistry, RefTypeSource};
pub use target::{CrossrefTargetView, crossref_target_view, identifier_of, ref_type_of};

/// The `type_name` used on `CustomNode` for float-ref targets.
///
/// Figures, tables, listings, and user-defined float categories all use this
/// custom node type post-sugaring. The specific category is stored in
/// `plain_data.kind` (display name) and `plain_data.ref_type` (id prefix).
pub const FLOAT_REF_TARGET: &str = "FloatRefTarget";

/// The `type_name` used on `CustomNode` for resolved crossref references in
/// the front-end AST.
///
/// Produced by `CrossrefResolveTransform` when it rewrites a `Cite` whose id
/// classifies as a crossref (per [`RefTypeRegistry`]). Back-end renderers
/// convert this into a format-specific link or reference.
pub const CROSSREF_RESOLVED_REF: &str = "CrossrefResolvedRef";
