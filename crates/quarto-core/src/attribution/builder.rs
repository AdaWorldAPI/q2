/*
 * attribution/builder.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! The single canonical-form constructor for [`AttributionData`].
//!
//! All three producer call-sites (the two providers and test fixtures)
//! go through this builder; no producer should construct
//! `AttributionRun` literals with ad-hoc `Arc::from(s)` calls. The
//! invariant the builder enforces by construction is:
//!
//! > Every `AttributionRun.actor` in the built `AttributionData` is
//! > `Arc::ptr_eq` to the corresponding key in [`IdentityMap`].

use std::collections::HashMap;
use std::sync::Arc;

use super::types::{AttributionData, AttributionMap, AttributionRun, Identity, IdentityMap};

/// Build an [`AttributionData`] while preserving the `Arc<str>`
/// interning invariant — every actor string allocates exactly once.
///
/// Convention (enforced by doc, not the type system): the `actor`
/// argument to [`Self::push_run`] and [`Self::set_identity`] MUST be
/// the value previously returned by [`Self::intern_actor`].
#[derive(Debug, Default)]
pub struct AttributionDataBuilder {
    runs: Vec<AttributionRun>,
    identities: IdentityMap,
    #[allow(dead_code)] // Phase 1: hot in intern_actor; quiet warning until then
    intern: HashMap<String, Arc<str>>,
}

impl AttributionDataBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the canonical `Arc<str>` for `actor`, allocating once
    /// on first sight and `Arc::clone`-ing thereafter. The returned
    /// Arc must be passed to subsequent [`Self::push_run`] /
    /// [`Self::set_identity`] calls for the same actor.
    pub fn intern_actor(&mut self, _actor: &str) -> Arc<str> {
        unimplemented!("Phase 1 — intern via the internal HashMap")
    }

    /// Append a run. `actor` must come from [`Self::intern_actor`].
    pub fn push_run(&mut self, _start: usize, _end: usize, _actor: Arc<str>, _time: i64) {
        unimplemented!("Phase 1 — push to internal Vec, preserve actor Arc identity")
    }

    /// Record an identity for `actor`. `actor` must come from
    /// [`Self::intern_actor`] so the resulting `IdentityMap` key is
    /// `Arc::ptr_eq` to every `AttributionRun.actor` for the same
    /// author.
    pub fn set_identity(&mut self, _actor: Arc<str>, _id: Identity) {
        unimplemented!("Phase 1 — insert into IdentityMap keyed by Arc<str>")
    }

    pub fn build(self) -> AttributionData {
        AttributionData {
            runs: AttributionMap(self.runs),
            identities: self.identities,
        }
    }
}
