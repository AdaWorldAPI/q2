/*
 * attribution_generate.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Attribution generate transform.
//!
//! Reads `ctx.attribution_provider`, calls `build(ctx)?` to obtain an
//! [`AttributionData`], merges it with any user-authored
//! `meta.attribution.identities` (provider wins on
//! `AttributionRun.actor` Arc identity; user wins on identity value
//! on key collision; non-colliding user keys are dropped), and stores
//! the result on `ctx.attribution_data`.
//!
//! Registered at the **tail of the Navigation Phase**, immediately
//! after `FooterRenderTransform`. The entire Finalization Phase runs
//! between this stage and [`AttributionRenderTransform`].
//!
//! ## Two invocation paths
//!
//! - HTML CLI path: registered in `build_transform_pipeline`; runs as
//!   part of the full transform pipeline.
//! - q2-debug WASM path: invoked **directly** by
//!   `parse_qmd_to_ast_with_attribution` after the existing 3-stage
//!   parse.
//!
//! **Both paths must produce identical results.** This transform
//! reads and writes only [`RenderContext`] fields; it must never
//! reach for `StageContext`.
//!
//! [`AttributionData`]: crate::attribution::AttributionData
//! [`AttributionRenderTransform`]: super::AttributionRenderTransform

use quarto_pandoc_types::pandoc::Pandoc;

use crate::Result;
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// See module docs.
pub struct AttributionGenerateTransform;

impl AttributionGenerateTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AttributionGenerateTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for AttributionGenerateTransform {
    fn name(&self) -> &str {
        "attribution-generate"
    }

    async fn transform(&self, _ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        unimplemented!(
            "Phase 2 — skip ladder + provider.build() + identity merge; \
             ctx.attribution_data = Some(Arc::new(...))"
        )
    }
}
