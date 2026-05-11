/*
 * attribution/types.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Canonical attribution data types.
//!
//! The canonical in-memory shape (`AttributionData`) is held as
//! `Arc<AttributionData>` on `RenderContext.attribution_data` — the
//! sidecar. It is **never** stored in `ast.meta`. The sole serialization
//! path is the WASM transport boundary; see [`prebuilt`] and
//! [`builder`] for the round-trip discipline.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::format::Format;

/// A contiguous byte-range run attributed to a single author at a
/// single point in time.
///
/// `actor` is `Arc<str>` (not `String`) so the same Arc is shared
/// across every run by the same author. For a doc with 5
/// contributors and 1000 runs this is 5 string allocations + 1000
/// cheap pointer clones, not 1000 string allocations. The
/// interning invariant is enforced by [`super::builder::AttributionDataBuilder`];
/// every `AttributionRun.actor` Arc in a built `AttributionData` is
/// `Arc::ptr_eq` to the corresponding key in
/// [`IdentityMap`].
///
/// `time` is Unix epoch **milliseconds**. Automerge uses ms natively;
/// the git provider multiplies its seconds-since-epoch timestamp by
/// 1000 before populating this field.
///
/// **`Serialize` only**, no `Deserialize` derive: deserialization
/// goes through [`TransportAttributionRun`] (a `String`-actor mirror)
/// then through [`super::builder::AttributionDataBuilder`], which
/// restores the interning invariant a plain
/// `Deserialize for Arc<str>` would have destroyed (each
/// `Arc::from(s)` during deserialize would otherwise allocate
/// per-occurrence).
#[derive(Debug, Clone, Serialize)]
pub struct AttributionRun {
    pub start: usize,
    pub end: usize,
    pub actor: Arc<str>,
    pub time: i64,
}

/// Transparent newtype around `Vec<AttributionRun>`.
///
/// Sorted by `start`, non-overlapping, contiguous. The in-memory
/// queryable form for `query_byte_range` — see
/// [`super::source::AttributionSource`].
///
/// **Single-document only in v1.** v2 (multi-file via includes)
/// replaces the field type with a path-keyed map. The transparent
/// newtype is `Serialize`-only for the same reason as
/// [`AttributionRun`].
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct AttributionMap(pub Vec<AttributionRun>);

impl AttributionMap {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn as_slice(&self) -> &[AttributionRun] {
        &self.0
    }
}

/// Resolved identity for an actor: a display name and a CSS-compatible
/// colour string.
///
/// Wire shape (q2-debug JSON, HTML `data-attr-*` attributes) uses
/// `name` not `display_name` — the Rust field follows the in-code
/// convention; the serde rename keeps the wire faithful.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    #[serde(rename = "name")]
    pub display_name: String,
    pub color: String,
}

/// `HashMap<Arc<str>, Identity>` keyed by the same `Arc<str>` used in
/// `AttributionRun.actor`. The merged result of
/// `meta.attribution.identities` (user override) ∪ provider-supplied
/// identities. Built by `AttributionGenerateTransform`; consumed by
/// `AttributionRenderTransform`. Empty when no source supplied
/// identities; unmapped actors fall back to the render-side warning
/// path placeholder.
pub type IdentityMap = HashMap<Arc<str>, Identity>;

/// The canonical in-memory shape, held as
/// `Arc<AttributionData>` on `RenderContext.attribution_data` (the
/// sidecar). Not stored in `ast.meta`.
///
/// `Serialize` derive exists *solely* for the WASM transport
/// boundary; both fields use `#[serde(default, skip_serializing_if)]`
/// so runs-only and identities-only transport payloads serialize
/// compactly.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AttributionData {
    #[serde(default, skip_serializing_if = "AttributionMap::is_empty")]
    pub runs: AttributionMap,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identities: IdentityMap,
}

/// Transport-only mirror of [`AttributionRun`] used at the WASM
/// boundary. Plain `String` actor field so `serde_json::from_str`
/// works without re-interning machinery; the canonical
/// `Arc<str>` shape is restored via
/// [`super::builder::AttributionDataBuilder`] inside the prebuilt
/// provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportAttributionRun {
    pub start: usize,
    pub end: usize,
    pub actor: String,
    pub time: i64,
}

/// Transport-only mirror of [`AttributionData`].
///
/// The wire shape is identical to the canonical type's `Serialize`
/// form (`Arc<str>` and `String` both serialize as JSON strings), so
/// round-tripping `AttributionData → JSON → TransportAttributionData
/// → AttributionDataBuilder → AttributionData` preserves data; the
/// only thing the transport detour buys is a clean place to re-intern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportAttributionData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<TransportAttributionRun>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identities: HashMap<String, Identity>,
}

/// Per-node attribution record carried on the writer config.
///
/// `Arc<str>` is pointer-equal to the corresponding key in
/// `attribution_actors` / `attribution_identities`, sharing the
/// interning invariant. Default `Serialize` for `Arc<str>` emits a
/// JSON string, so the wire shape `{ "s": ..., "actor": ..., "time": ... }`
/// uses the field name `actor` for this struct's `actor` value.
#[derive(Debug, Clone, Serialize)]
pub struct AttributionRecord {
    pub actor: Arc<str>,
    pub time: i64,
}

/// Hit returned from `AttributionSource::query_byte_range`.
pub type AttributionHit = AttributionRecord;

/// Whether the given format's writer consumes the attribution lookup.
///
/// Used by `AttributionGenerateTransform`'s skip ladder to
/// short-circuit before invoking the provider; opting in to
/// attribution on a non-consuming format would otherwise fire a
/// `git blame` subprocess whose output goes nowhere visible.
///
/// In v1 returns `true` for HTML and q2-debug JSON only.
pub fn format_supports_attribution(_format: &Format) -> bool {
    // Phase 0: stub returning false. Phase 1 will key off the Format
    // discriminant. Tests reach this from RenderContext::format which
    // can be HTML in fixtures.
    unimplemented!("Phase 1 (Phase 0 stub) — format_supports_attribution")
}

/// Read user-authored `meta.attribution.identities` (a small
/// `ConfigValue::Map` from YAML parse) into an [`IdentityMap`] for
/// the Phase 2 merge step.
///
/// This is the *only* attribution-related `ConfigValue` → Rust-struct
/// converter the plan ships; the bulk `runs` path never visits
/// `ConfigValue`. Returns an empty map when the key is absent.
pub fn from_config_value(_meta: &quarto_pandoc_types::ConfigValue) -> IdentityMap {
    unimplemented!("Phase 1 (Phase 0 stub) — from_config_value")
}
