/*
 * attribution_viewer.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Auto-inject the default attribution viewer CSS + JS pair into
//! `rendered.includes.{header,after-body}`.
//!
//! Runs only when the upstream [`AttributionRenderTransform`] populated
//! `format_options.html.attribution_by_node` (i.e. attribution is
//! active for this HTML render) and the YAML opt-out
//! `attribution: { source: git, viewer: false }` was not set.
//!
//! The injected CSS gives `[data-attr-actor]` regions a dotted
//! underline; the JS attaches delegated `mouseover`/`mouseout`
//! listeners that surface a small floating badge built from each
//! element's `data-attr-*` attributes. Both payloads carry an
//! HTML-comment sentinel so re-running the transform on the same
//! `ast.meta` does not double-inject.
//!
//! Mirrors the shape of [`WebsiteFaviconTransform`](super::WebsiteFaviconTransform):
//! append HTML literals to the canonical
//! `meta.rendered.includes.{header,after-body}` lists; the
//! `quarto-core` HTML template wires those slots into `<head>` and
//! before-`</body>` respectively.
//!
//! CLI-only by design: hub-client renders React components and binds
//! events on props, so it shares only the CSS asset (imported via
//! Vite's `?raw`) and ignores `rendered.includes.*` entirely. The
//! `"attribution-viewer"` name is on
//! [`Q2_PREVIEW_TRANSFORM_EXCLUDED`](super::super::pipeline::Q2_PREVIEW_TRANSFORM_EXCLUDED)
//! to enforce the design statement rather than rely on surface-level
//! no-op.

use quarto_pandoc_types::ConfigValue;
use quarto_pandoc_types::config_value::ConfigValueKind;
use quarto_pandoc_types::pandoc::Pandoc;

use crate::Result;
use crate::attribution::{VIEWER_CSS, VIEWER_JS};
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// HTML-comment sentinel embedded in the injected `<style>` block.
/// Used by the dedup scan so a transform re-run is idempotent.
const CSS_SENTINEL: &str = "<!-- quarto-attribution-viewer-css -->";

/// HTML-comment sentinel embedded in the injected `<script>` block.
const JS_SENTINEL: &str = "<!-- quarto-attribution-viewer-js -->";

pub struct AttributionViewerTransform;

impl AttributionViewerTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AttributionViewerTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for AttributionViewerTransform {
    fn name(&self) -> &str {
        "attribution-viewer"
    }

    async fn transform(&self, ast: &mut Pandoc, ctx: &mut RenderContext) -> Result<()> {
        // First gating signal: `AttributionRenderTransform` populated
        // the per-node lookup. Without it there are no wrappers in
        // the body, so the CSS/JS would have nothing to act on.
        if ctx.format_options.html.attribution_by_node.is_none() {
            return Ok(());
        }
        // Second gating signal: YAML opt-out. Default `true` so the
        // feature is discoverable; `viewer: false` flips it.
        if !ctx.format_options.html.attribution_viewer_enabled {
            return Ok(());
        }

        let css_payload = format!("{}\n<style>\n{}</style>", CSS_SENTINEL, VIEWER_CSS);
        let js_payload = format!("{}\n<script>\n{}</script>", JS_SENTINEL, VIEWER_JS);

        append_with_sentinel(&mut ast.meta, "header", CSS_SENTINEL, css_payload);
        append_with_sentinel(&mut ast.meta, "after-body", JS_SENTINEL, js_payload);

        Ok(())
    }
}

/// Append `payload` to `meta.rendered.includes.<slot>`, skipping if
/// any existing string in that slot already contains `sentinel`. The
/// dedup keeps the transform idempotent under accidental double
/// invocation (e.g. tests that rerun the same transform).
fn append_with_sentinel(meta: &mut ConfigValue, slot: &str, sentinel: &str, payload: String) {
    if !matches!(&meta.value, ConfigValueKind::Map(_)) {
        return;
    }
    let source_info = meta.source_info.clone();

    if !meta.contains_path(&["rendered", "includes", slot]) {
        meta.insert_path(
            &["rendered", "includes", slot],
            ConfigValue::new_array(vec![], source_info.clone()),
        );
    }

    let Some(target) = meta.get_path_mut(&["rendered", "includes", slot]) else {
        return;
    };
    let ConfigValueKind::Array(items) = &mut target.value else {
        return;
    };
    if items
        .iter()
        .any(|item| item.as_str().is_some_and(|s| s.contains(sentinel)))
    {
        return;
    }
    items.push(ConfigValue::new_string(payload, source_info));
}
