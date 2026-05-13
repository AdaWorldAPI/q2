/*
 * attribution/mod.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Per-node authorship attribution.
//!
//! See `claude-notes/plans/CURRENT.md` for the full design. The
//! single-paragraph summary:
//!
//! - The canonical in-memory shape is [`AttributionData`], held as
//!   `Arc<AttributionData>` on [`RenderContext::attribution_data`] —
//!   the sidecar. It is NEVER stored in `ast.meta`.
//! - [`AttributionGenerateTransform`] (registered at the tail of the
//!   Navigation Phase) calls the installed
//!   [`AttributionSourceProvider`] and merges the result with any
//!   user-authored `meta.attribution.identities`, then stores the
//!   sidecar on `RenderContext`.
//! - [`AttributionRenderTransform`] (registered last in the
//!   Finalization Phase) walks the AST once, builds a writer-side
//!   pre-baked lookup table (`Arc<[Option<AttributionRecord>]>`),
//!   and resolves identity exactly once per distinct actor.
//! - The format-specialised writer (HTML body, q2-debug JSON) emits
//!   the lookup output.
//!
//! Both Generate and Render are no-ops when no provider has been
//! installed on `RenderContext` — same code path as the unflagged
//! default.
//!
//! [`AttributionGenerateTransform`]: crate::transforms::AttributionGenerateTransform
//! [`AttributionRenderTransform`]: crate::transforms::AttributionRenderTransform
//! [`RenderContext::attribution_data`]: crate::render::RenderContext::attribution_data

pub mod builder;
pub mod git_blame;
pub mod mode;
pub mod palette;
pub mod pampa_bridge;
pub mod prebuilt;
pub mod source;
pub mod types;

pub use builder::AttributionDataBuilder;
pub use git_blame::{
    BlameLine, BlameRun, GitBlameProvider, attribution_from_porcelain, build_blame_runs,
    parse_blame_porcelain,
};
pub use mode::{AttributionMode, resolve_attribution_mode};
pub use palette::{actor_color, fnv1a_hex8};
pub use pampa_bridge::{html_attribution_fields, json_attribution_fields};
pub use prebuilt::PreBuiltAttributionProvider;
pub use source::{AttributionSource, AttributionSourceProvider};
pub use types::{
    AttributionData, AttributionHit, AttributionMap, AttributionRecord, AttributionRun, Identity,
    IdentityMap, TransportAttributionData, TransportAttributionRun, format_supports_attribution,
    from_config_value,
};
