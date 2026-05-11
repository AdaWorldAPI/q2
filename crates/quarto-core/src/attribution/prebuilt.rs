/*
 * attribution/prebuilt.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Provider that wraps a hub-client-supplied transport JSON string
//! and decodes it on demand.
//!
//! The JSON is parsed lazily inside [`AttributionSourceProvider::build`]
//! rather than at construction time so that:
//! - construction is infallible (no `Result` at the WASM entry point),
//!   and
//! - the parse + intern step lives behind the same provider trait
//!   surface as `GitBlameProvider`, so a future caller cannot
//!   distinguish the two by where errors surface.

use super::source::AttributionSourceProvider;
use super::types::AttributionData;
use crate::Result;
use crate::render::RenderContext;

/// Wraps a transport JSON string. Decodes via
/// [`super::types::TransportAttributionData`] then re-interns through
/// [`super::builder::AttributionDataBuilder`] in `build`.
#[derive(Debug, Clone)]
pub struct PreBuiltAttributionProvider {
    json: String,
}

impl PreBuiltAttributionProvider {
    pub fn new(json: String) -> Self {
        Self { json }
    }

    /// For testing: the raw transport JSON payload this provider was
    /// constructed with.
    pub fn json(&self) -> &str {
        &self.json
    }
}

impl AttributionSourceProvider for PreBuiltAttributionProvider {
    fn build(&self, _ctx: &RenderContext) -> Result<AttributionData> {
        unimplemented!(
            "Phase 3b — serde_json::from_str into TransportAttributionData, \
             feed through AttributionDataBuilder so the Arc<str> interning \
             invariant is restored on the way back in"
        )
    }
}
