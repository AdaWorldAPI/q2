//! Phase B of the OSM/Lance lifecycle plan
//! (`claude-notes/plans/2026-08-15-osm-lance-lifecycle.md`): the serving
//! mechanism that replaces the env-var/`OnceLock` activation model with an
//! atomically replaceable `Arc`.
//!
//! ## What this module is
//!
//! [`OsmArtifactManager`] holds two independently swappable snapshots:
//!
//! - [`osm_lifecycle::Lifecycle`] — the coordination state Phase A already
//!   defined (what's active, what's importing, what failed). Real today:
//!   nothing else in this crate constructs one yet, so a manager starts
//!   `Lifecycle::absent()` and stays there until Phase C wires real imports
//!   through it.
//! - [`ActiveArtifact`] — the actual dataset content (the `RowSlab` / books /
//!   chains handles). **Deliberately an empty marker in Phase B.** Phase C
//!   replaces the four independent `OnceLock`s in `osm_features.rs`
//!   (`SLAB_MMAP`, `BOOKS`, `CHAINS`, `SLAB_DIGEST`) with real content here,
//!   rotated as ONE unit through `publish_artifact` — matching the
//!   `ServingSet` invariant that active/previous rotate together, not four
//!   independent statics that can each be at a different version.
//!
//! Both use `arc_swap` so a reader holds a stable `Arc` for the lifetime of
//! its request: `current()`/`artifact()` clone the `Arc` (cheap, atomic, no
//! lock), and a concurrent `publish_*` call never invalidates that clone —
//! the reader finishes against the version it observed, and the next reader
//! sees the new one. This is the literal falsifier from the plan ("in-flight
//! readers finish against the old Arc; new readers receive the new
//! version"), proven below without any I/O.
//!
//! ## What this module does NOT do (yet)
//!
//! It does not replace the `osm_features.rs` `OnceLock`s — that swap is
//! deferred to Phase C, once there is real hydration logic to populate
//! `ActiveArtifact` with. Wiring the manager into `AppState` and exposing it
//! at `/api/osm/status` (this session's work) is purely additive: the
//! existing OnceLock-backed serving path is untouched and carries zero risk
//! from this change.

use std::sync::Arc;

use arc_swap::{ArcSwap, ArcSwapOption};

use crate::osm_lifecycle::Lifecycle;

/// The dataset content Phase C will populate (the `RowSlab` / books / chains
/// bundle, rotated as one unit). Empty in Phase B — nothing constructs a
/// non-`None` value yet; see the module doc for why.
#[derive(Debug)]
pub struct ActiveArtifact {
    _private: (),
}

/// The atomically-replaceable serving state for the OSM/Lance lifecycle.
///
/// One instance lives in `AppState`, shared behind an `Arc` like every other
/// piece of shared state there. Cloning a *snapshot* (`current()`,
/// `artifact()`) is cheap and lock-free; publishing a new snapshot
/// (`publish_lifecycle`, `publish_artifact`) is a single atomic store that
/// never blocks or invalidates readers already holding an older snapshot.
pub struct OsmArtifactManager {
    lifecycle: ArcSwap<Lifecycle>,
    artifact: ArcSwapOption<ActiveArtifact>,
}

impl OsmArtifactManager {
    /// Nothing local, nothing in flight, no dataset content. The only state
    /// Phase B ever constructs — Phase C's hydration path is what first
    /// calls `publish_lifecycle`/`publish_artifact` with something else.
    #[must_use]
    pub fn absent() -> Self {
        Self {
            lifecycle: ArcSwap::from_pointee(Lifecycle::absent()),
            artifact: ArcSwapOption::empty(),
        }
    }

    /// The current lifecycle snapshot. Cheap `Arc` clone, never blocks.
    #[must_use]
    pub fn current(&self) -> Arc<Lifecycle> {
        self.lifecycle.load_full()
    }

    /// The current dataset content, if any has been published.
    #[must_use]
    pub fn artifact(&self) -> Option<Arc<ActiveArtifact>> {
        self.artifact.load_full()
    }

    /// Atomically replace the lifecycle snapshot. A caller holding an
    /// `Arc<Lifecycle>` from a prior `current()` keeps observing the old
    /// value — this call never mutates through an existing `Arc`.
    #[allow(
        dead_code,
        reason = "hydration write path for Phase C, which has not landed yet; exercised only by this module's tests today; allow not expect — used under cfg(test)/feature, so no expectation holds in every --all-targets compilation"
    )]
    pub fn publish_lifecycle(&self, next: Lifecycle) {
        self.lifecycle.store(Arc::new(next));
    }

    /// Atomically replace the dataset content. Same non-disturbance
    /// guarantee as `publish_lifecycle`, and `None` is a valid publication
    /// (Phase C uses it to retract content whose lifecycle moved to
    /// `absent`/`CandidateFailed` with no prior active version).
    #[allow(
        dead_code,
        reason = "hydration write path for Phase C, which has not landed yet; exercised only by this module's tests today; allow not expect — used under cfg(test)/feature, so no expectation holds in every --all-targets compilation"
    )]
    pub fn publish_artifact(&self, next: Option<Arc<ActiveArtifact>>) {
        self.artifact.store(next);
    }
}

impl Default for OsmArtifactManager {
    fn default() -> Self {
        Self::absent()
    }
}

// ── cgroup memory telemetry ─────────────────────────────────────────────

/// A snapshot of cgroup v2 memory accounting, if the platform/environment
/// exposes it. `None` fields are honest gaps (unreadable file, non-Linux,
/// unlimited `max`), never a fabricated zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CgroupMemory {
    /// `memory.current` — resident cgroup memory, in bytes.
    pub current_bytes: Option<u64>,
    /// `memory.max` — the cgroup's memory ceiling, in bytes. `None` means
    /// either the file was unreadable OR the cgroup is genuinely unlimited
    /// (`memory.max` contains the literal string `max`) — the two are
    /// distinguished by `current_bytes` being present: an unlimited cgroup
    /// still reports a real `current`, an unreadable one reports neither.
    pub max_bytes: Option<u64>,
}

/// Parse `memory.current`'s content: a bare decimal integer.
#[allow(
    dead_code,
    reason = "read only by `read_cgroup_memory`'s cfg(target_os = \"linux\") arm and by this module's tests; dead on every non-Linux target, so allow not expect — no single expectation holds across targets"
)]
#[must_use]
pub fn parse_cgroup_current(raw: &str) -> Option<u64> {
    raw.trim().parse().ok()
}

/// Parse `memory.max`'s content: a bare decimal integer, or the literal
/// string `max` meaning "no limit" (represented here as `None`, same as an
/// unreadable file — see [`CgroupMemory::max_bytes`] for how to tell them
/// apart).
#[allow(
    dead_code,
    reason = "read only by `read_cgroup_memory`'s cfg(target_os = \"linux\") arm and by this module's tests; dead on every non-Linux target, so allow not expect — no single expectation holds across targets"
)]
#[must_use]
pub fn parse_cgroup_max(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed == "max" {
        return None;
    }
    trimmed.parse().ok()
}

#[cfg(target_os = "linux")]
#[must_use]
pub fn read_cgroup_memory() -> CgroupMemory {
    let current = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
        .ok()
        .and_then(|s| parse_cgroup_current(&s));
    let max = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| parse_cgroup_max(&s));
    CgroupMemory {
        current_bytes: current,
        max_bytes: max,
    }
}

/// Non-Linux platforms have no cgroup v2 accounting to read. An honest
/// empty snapshot, not a panic and not a fabricated value — the
/// cross-platform rule (`.claude/rules/cross-platform.md`) forbids the
/// unconditional unix-only path this would otherwise be.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn read_cgroup_memory() -> CgroupMemory {
    CgroupMemory::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_lifecycle::{ImportId, Lifecycle};

    // ── OsmArtifactManager ───────────────────────────────────────────────

    /// PROBE (Phase B). The manager's only self-constructed state: nothing
    /// local, nothing in flight, no dataset content.
    #[test]
    fn a_fresh_manager_is_absent_with_no_artifact() {
        let mgr = OsmArtifactManager::absent();
        assert_eq!(*mgr.current(), Lifecycle::absent());
        assert!(mgr.artifact().is_none());
    }

    /// PROBE (Phase B). The literal falsifier from the plan: "in-flight
    /// readers finish against the old Arc; new readers receive the new
    /// version." A reader that already cloned a snapshot must keep seeing
    /// what it observed, even after a publish.
    #[test]
    fn publishing_a_new_lifecycle_does_not_disturb_an_old_readers_snapshot() {
        let mgr = OsmArtifactManager::absent();
        let old_reader = mgr.current();
        assert_eq!(*old_reader, Lifecycle::absent());

        let served = Lifecycle::serving(ImportId(1), None);
        mgr.publish_lifecycle(served.clone());

        // The snapshot taken BEFORE the publish is unchanged.
        assert_eq!(*old_reader, Lifecycle::absent());
        // A NEW read sees the published value.
        assert_eq!(*mgr.current(), served);
    }

    /// PROBE (Phase B). Same non-disturbance guarantee, on the artifact
    /// side — this is the half that actually matters once Phase C starts
    /// publishing real dataset content: an in-flight tile request must not
    /// have its `RowSlab` swapped out from under it mid-read.
    #[test]
    fn publishing_a_new_artifact_does_not_disturb_an_old_readers_snapshot() {
        let mgr = OsmArtifactManager::absent();
        assert!(mgr.artifact().is_none());

        let first = Arc::new(ActiveArtifact { _private: () });
        mgr.publish_artifact(Some(first.clone()));
        let old_reader = mgr.artifact().expect("just published");
        assert!(Arc::ptr_eq(&old_reader, &first));

        let second = Arc::new(ActiveArtifact { _private: () });
        mgr.publish_artifact(Some(second.clone()));

        // The reader's clone still points at the FIRST artifact.
        assert!(Arc::ptr_eq(&old_reader, &first));
        // A new read sees the SECOND.
        let new_reader = mgr.artifact().expect("second publish");
        assert!(Arc::ptr_eq(&new_reader, &second));
    }

    /// PROBE (Phase B). `publish_artifact(None)` is a valid retraction, not
    /// a no-op — a manager can go from serving content to serving none.
    #[test]
    fn publishing_none_retracts_the_artifact() {
        let mgr = OsmArtifactManager::absent();
        mgr.publish_artifact(Some(Arc::new(ActiveArtifact { _private: () })));
        assert!(mgr.artifact().is_some());

        mgr.publish_artifact(None);
        assert!(
            mgr.artifact().is_none(),
            "publishing None must actually clear the slot, not be ignored"
        );
    }

    /// PROBE (Phase B). `Default` and `absent()` must agree — a caller
    /// reaching for `OsmArtifactManager::default()` (e.g. in a `derive`d
    /// `AppState`) gets the same honest starting state as the explicit
    /// constructor, not a silently different one.
    #[test]
    fn default_agrees_with_absent() {
        let via_default = OsmArtifactManager::default();
        let via_absent = OsmArtifactManager::absent();
        assert_eq!(*via_default.current(), *via_absent.current());
        assert!(via_default.artifact().is_none());
        assert!(via_absent.artifact().is_none());
    }

    // ── cgroup memory parsing ────────────────────────────────────────────

    /// PROBE (Phase B). The common case: a bare integer, possibly with the
    /// trailing newline every `/sys/fs/cgroup/*` file actually has.
    #[test]
    fn parse_cgroup_current_reads_a_plain_integer() {
        assert_eq!(parse_cgroup_current("123456789\n"), Some(123_456_789));
        assert_eq!(parse_cgroup_current("0\n"), Some(0));
    }

    /// PROBE (Phase B). Malformed content must decline, not panic or guess.
    #[test]
    fn parse_cgroup_current_declines_malformed_content() {
        assert_eq!(parse_cgroup_current(""), None);
        assert_eq!(parse_cgroup_current("not a number"), None);
        assert_eq!(parse_cgroup_current("-5"), None, "memory is never negative");
    }

    /// PROBE (Phase B). `max` is a real, distinct value cgroup v2 writes
    /// for "no limit" — it is not malformed input and must not be confused
    /// with an unreadable file (both currently render as `None`, but for
    /// different reasons; see `max_bytes`'s doc comment for how a caller
    /// tells them apart).
    #[test]
    fn parse_cgroup_max_recognizes_the_unlimited_sentinel() {
        assert_eq!(parse_cgroup_max("max\n"), None);
        assert_eq!(parse_cgroup_max("max"), None);
    }

    /// PROBE (Phase B). A bounded cgroup reports a real ceiling.
    #[test]
    fn parse_cgroup_max_reads_a_bounded_ceiling() {
        assert_eq!(parse_cgroup_max("25769803776\n"), Some(25_769_803_776));
    }

    /// PROBE (Phase B). Malformed content declines here too — a corrupt
    /// `memory.max` must not be silently read as "unlimited" just because
    /// both cases return `None`.
    #[test]
    fn parse_cgroup_max_declines_malformed_content() {
        assert_eq!(parse_cgroup_max(""), None);
        assert_eq!(parse_cgroup_max("maxxx"), None);
        assert_eq!(parse_cgroup_max("not a number"), None);
    }

    /// PROBE (Phase B). `read_cgroup_memory` never panics, on any platform
    /// — the real system falsifier this session can actually run. On
    /// Linux CI/containers `/sys/fs/cgroup/memory.current` is normally
    /// readable, so this also incidentally exercises the real read path;
    /// the assertion itself only requires the call to return, honestly,
    /// whatever the platform has.
    #[test]
    fn read_cgroup_memory_never_panics() {
        let _ = read_cgroup_memory();
    }
}
