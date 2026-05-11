/*
 * attribution/palette.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Deterministic colour helpers shared with the hub-client TS
//! producer.
//!
//! Drift mitigation: the doc-comments cross-reference the TS
//! siblings (`actorColor` in
//! `hub-client/src/hooks/useReplayMode.ts:32` and `fnv1aHex8` added
//! alongside it in Phase 5). Anyone editing either is forced to
//! consider the other; if drift becomes a real concern in v2, upgrade
//! to a shared-fixture-based test.

/// Deterministic colour from an actor hash string.
///
/// Formula: parse the first 6 hex chars of the actor ID as an
/// integer, mod 360, emit `hsl(<hue>, 60%, 55%)`.
///
/// **MUST stay in sync with the TS `actorColor` in
/// `hub-client/src/hooks/useReplayMode.ts:32` — same formula.**
pub fn actor_color(_actor: &str) -> String {
    unimplemented!("Phase 6 — hex prefix → hue mod 360 → hsl string")
}

/// 32-bit FNV-1a hash, formatted as a left-padded 8-char hex string.
///
/// Used wherever an arbitrary actor string (e.g. an email) must be
/// reduced to a hex-prefix-safe input for [`actor_color`]. Caller:
/// `GitBlameProvider` (pre-hashes the author email). The TS sibling
/// `fnv1aHex8` plays the same role for Automerge actor IDs whose
/// first 6 chars aren't guaranteed hex or that need fallback colouring
/// when profile metadata is absent.
///
/// **MUST stay in sync with the TS `fnv1aHex8` (Phase 5
/// hub-client work item).**
pub fn fnv1a_hex8(_s: &str) -> String {
    unimplemented!("Phase 6 — 5-line FNV-1a, zero deps")
}
