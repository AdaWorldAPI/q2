/*
 * attribution_render.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Attribution render transform.
//!
//! Reads `ctx.attribution_data` (the sidecar `Arc<AttributionData>`),
//! walks the AST once, and produces two artefacts on
//! `ctx.format_options`:
//!
//! 1. `Vec<Option<AttributionRecord>>` indexed by `sourceInfoId`.
//!    Skips queries when the resolved `file_id != 0` (v1 single-doc).
//! 2. A pruned [`IdentityMap`] containing only the actors that appear
//!    in the lookup vec. Resolves identity **once per distinct actor**
//!    (interned during the AST walk); fires at most K diagnostics per
//!    render when the producer invariant is violated, not N.
//!
//! Registered as the **very last** transform in the Finalization
//! Phase, immediately after `ResourceCollectorTransform`. The entire
//! Finalization Phase runs between
//! [`AttributionGenerateTransform`](super::AttributionGenerateTransform)
//! and this stage.
//!
//! Reads and writes only [`RenderContext`] fields; never reaches for
//! `StageContext`. See `attribution_generate.rs` module docs for the
//! invocation-path invariant.
//!
//! [`IdentityMap`]: crate::attribution::IdentityMap

use quarto_pandoc_types::pandoc::Pandoc;

use crate::Result;
use crate::render::RenderContext;
use crate::transform::AstTransform;

pub struct AttributionRenderTransform;

impl AttributionRenderTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AttributionRenderTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for AttributionRenderTransform {
    fn name(&self) -> &str {
        "attribution-render"
    }

    async fn transform(&self, _ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        unimplemented!(
            "Phase 4c — single AST walk, build pre-baked lookup vec + interned actors table, \
             stash on ctx.format_options.html / .json"
        )
    }
}
