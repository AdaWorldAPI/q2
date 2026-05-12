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
/// integer, mod 360, emit `hsl(<hue>, 60%, 55%)`. Non-hex input
/// (or an empty string) collapses to hue `0`.
///
/// **MUST stay in sync with the TS `actorColor` in
/// `hub-client/src/hooks/useReplayMode.ts:32` — same formula.**
pub fn actor_color(actor: &str) -> String {
    // Mirror TS `actor.slice(0, 6)`: first 6 Unicode scalar values
    // (effectively bytes for the hex inputs the producer contracts
    // feed us).
    let prefix: String = actor.chars().take(6).collect();
    let hue = u32::from_str_radix(&prefix, 16).unwrap_or(0) % 360;
    format!("hsl({hue}, 60%, 55%)")
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
pub fn fnv1a_hex8(s: &str) -> String {
    const FNV_OFFSET_BASIS_32: u32 = 0x811c_9dc5;
    const FNV_PRIME_32: u32 = 0x0100_0193;
    let mut hash: u32 = FNV_OFFSET_BASIS_32;
    for b in s.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(FNV_PRIME_32);
    }
    format!("{hash:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_color_emits_hsl_string() {
        let c = actor_color("aabbccdd");
        assert!(c.starts_with("hsl("));
        assert!(c.ends_with(", 60%, 55%)"));
    }

    #[test]
    fn actor_color_matches_ts_for_known_inputs() {
        // parseInt("aabbcc", 16) = 0xaabbcc = 11_189_196; % 360 = 36.
        assert_eq!(actor_color("aabbccdd"), "hsl(36, 60%, 55%)");
        assert_eq!(actor_color("00000000"), "hsl(0, 60%, 55%)");
    }

    #[test]
    fn actor_color_handles_empty_and_non_hex_input() {
        assert_eq!(actor_color(""), "hsl(0, 60%, 55%)");
        assert_eq!(actor_color("zzz"), "hsl(0, 60%, 55%)");
    }

    #[test]
    fn fnv1a_hex8_known_vectors() {
        // Reference values for FNV-1a 32-bit.
        assert_eq!(fnv1a_hex8(""), "811c9dc5");
        assert_eq!(fnv1a_hex8("a"), "e40c292c");
        assert_eq!(fnv1a_hex8("foobar"), "bf9cf968");
    }

    #[test]
    fn fnv1a_hex8_is_eight_chars_lowercase_hex() {
        let h = fnv1a_hex8("alice@example.com");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
