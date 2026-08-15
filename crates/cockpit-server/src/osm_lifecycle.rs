//! The OSM/Lance lifecycle — **three independent clocks, one lineage per
//! region**, decided purely.
//!
//! Phase A of `claude-notes/plans/2026-08-15-osm-lance-lifecycle.md`. Every
//! item here is a **pure decision**: no I/O, no async, no Lance calls. That is
//! deliberate and it is what makes properties like *"the S3 pointer is written
//! last"* and *"a warm boot names no data object"* testable as decisions,
//! before any wiring exists to get them wrong.
//!
//! # The correction this module encodes
//!
//! A live Lance dataset is a **versioned lineage, not an immutable
//! directory**. A legitimate append, index build, compaction, reopen or atime
//! change is not corruption, and must not invalidate the imported OSM source.
//! So nothing here hashes a whole dataset directory, and nothing compares its
//! changing object inventory against one static checksum.
//!
//! # The three clocks — never conflated
//!
//! | # | Clock | Carried by |
//! |---|---|---|
//! | 1 | **Semantic import identity** | [`ImportIdentity`], [`OriginSeal`], [`ImportSeal`] |
//! | 2 | **Physical Lance history** | [`ImportSeal::publication_version`], [`ServingVersion`], `observed_head` |
//! | 3 | **Replica progress** | [`StorageRoot`], [`Published::mirrored_through_version`] |
//!
//! Upstream (`lance-graph` #912/#913) states the failure this separation
//! prevents: retrying cycle 1 while the head stood at V5 recorded cycle 1 as
//! *"sealed into V5"*. A later observed head is a **read horizon**, never
//! evidence that the origin changed.
//!
//! # Serving state and import state are ORTHOGONAL
//!
//! [`Lifecycle`] is a product, not a sum: `serving: Option<ServingSet>` beside
//! `operation: Operation`. An earlier draft made them one enum, and the
//! consequence was immediate — `ImportStarted` replaced `Active`, so the
//! artifact being served vanished into the import's state and a later failure
//! had nothing to restore. A failed import must leave **both** active and
//! previous-clean untouched, which is only structurally true if the import was
//! never able to overwrite them in the first place.
//!
//! Completions are **fenced by generation**: starting candidate C supersedes
//! in-flight candidate B, and B's late completion is rejected
//! ([`LifecycleError::StaleCompletion`]) instead of activating or failing C.
//!
//! # Hash choices
//!
//! **FNV-1a/64** is the fast physical batch marker ([`ImportId`]) — the same
//! role the upstream Phase-A contract gives it, an idempotency key and not a
//! security property. Because it is only a marker, [`reconcile`] compares the
//! **complete identity tuple** after the marker matches: an FNV collision is a
//! permanent [`ReconcileVerdict::HashConflict`], never a silent reconcile.
//! **SHA-256** stays for externally published SOA manifests and transport
//! verification. No BLAKE3 in this work.
//!
//! # What the tests here do and do not prove
//!
//! Every test below is a **Phase-A contract probe**: it proves a decision
//! function returns the right decision. It does not prove the running system
//! obeys it, because in Phase A there is no running system to obey it —
//! nothing here is wired to `main` yet. Each probe that has a corresponding
//! *system* falsifier names the phase that owes it, and the plan file tracks
//! those as still-open. Confusing the two is how a green suite comes to
//! certify behaviour nobody measured.

// Phase A (see the module doc) defines the lifecycle VOCABULARY and proves it
// with contract probes; nothing here is wired to `main` yet — that is Phase B
// (`OsmArtifactManager`) and Phase C (versioned import). In a *library* crate
// `pub` alone would exempt these from dead-code analysis; `cockpit-server` is
// a binary, where only items reachable from `main` count as used. Silencing
// the lint is deliberate and scoped to this module, not a blanket workspace
// suppression, and every item it covers already has a probe below exercising
// its logic — the code runs, it is only not yet CALLED by the server.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use lance_graph_contract::scheduler::DatasetVersion;

// ── Validated identity primitives ────────────────────────────────────────

/// Why a value was refused as an identity component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    Empty {
        field: &'static str,
    },
    TooLong {
        field: &'static str,
        len: usize,
    },
    IllegalCharacter {
        field: &'static str,
        ch: char,
    },
    NotHex {
        field: &'static str,
    },
    WrongLength {
        field: &'static str,
        want: usize,
        got: usize,
    },
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { field } => write!(f, "{field} must not be empty"),
            Self::TooLong { field, len } => write!(f, "{field} is too long ({len})"),
            Self::IllegalCharacter { field, ch } => {
                write!(f, "{field} contains an illegal character {ch:?}")
            }
            Self::NotHex { field } => write!(f, "{field} is not hex"),
            Self::WrongLength { field, want, got } => {
                write!(f, "{field} must be {want} characters, got {got}")
            }
        }
    }
}

impl std::error::Error for IdentityError {}

/// A validated region name.
///
/// **The validation is not decoration.** This string is interpolated into an
/// S3 object key AND joined onto a filesystem path, so `..`, `/`, or a
/// backslash would let a stray environment variable read a different prefix or
/// write outside the artifact root. The rule is the one the existing slab
/// hydrator already enforces — non-empty, `<= 64` bytes, lowercase
/// `[a-z0-9-]` — restated here as a *type* so the guarantee travels with the
/// value instead of being re-checked (or forgotten) at each use site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(String);

impl RegionId {
    /// The same predicate as `osm_slab_hydrate::is_valid_region`, as a
    /// constructor. Rejecting the whole alphabet outside `[a-z0-9-]` makes
    /// path traversal and prefix escape impossible by construction rather than
    /// by careful escaping at each site.
    pub fn parse(s: &str) -> Result<Self, IdentityError> {
        const FIELD: &str = "region";
        if s.is_empty() {
            return Err(IdentityError::Empty { field: FIELD });
        }
        if s.len() > 64 {
            return Err(IdentityError::TooLong {
                field: FIELD,
                len: s.len(),
            });
        }
        if let Some(ch) = s
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(IdentityError::IllegalCharacter { field: FIELD, ch });
        }
        Ok(Self(s.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which SOA bake a lineage version came from — validated on the same footing
/// as [`RegionId`].
///
/// It does not name an S3 key *today*, but it is the kind of value that grows
/// into run ids and staging directory names, and a validated type cannot be
/// quietly promoted into a path. `.` and `_` are allowed (bake ids carry
/// dates and suffixes); `..` is rejected outright.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceBakeId(String);

impl SourceBakeId {
    pub fn parse(s: &str) -> Result<Self, IdentityError> {
        const FIELD: &str = "source_bake_id";
        if s.is_empty() {
            return Err(IdentityError::Empty { field: FIELD });
        }
        if s.len() > 128 {
            return Err(IdentityError::TooLong {
                field: FIELD,
                len: s.len(),
            });
        }
        if let Some(ch) = s.chars().find(|c| {
            !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '_')
        }) {
            return Err(IdentityError::IllegalCharacter { field: FIELD, ch });
        }
        if s.contains("..") {
            return Err(IdentityError::IllegalCharacter {
                field: FIELD,
                ch: '.',
            });
        }
        Ok(Self(s.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SHA-256 of the externally published SOA manifest for a bake, stored as the
/// **parsed 32 bytes**.
///
/// Held as bytes, not as whatever string arrived: `A1B2…` and `a1b2…` are the
/// same digest, and if the identity hashed the text instead of the value the
/// same import would derive two different [`ImportId`]s depending on who
/// wrote the manifest. Canonical lowercase hex is a rendering
/// ([`Self::to_hex`]), never the stored form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceManifestSha256([u8; 32]);

impl SourceManifestSha256 {
    /// Parse 64 hex characters, either case.
    pub fn parse_hex(s: &str) -> Result<Self, IdentityError> {
        const FIELD: &str = "source_manifest_sha256";
        if s.len() != 64 {
            return Err(IdentityError::WrongLength {
                field: FIELD,
                want: 64,
                got: s.len(),
            });
        }
        let mut out = [0u8; 32];
        let raw = s.as_bytes();
        for (i, byte) in out.iter_mut().enumerate() {
            let hi = (raw[2 * i] as char)
                .to_digit(16)
                .ok_or(IdentityError::NotHex { field: FIELD })?;
            let lo = (raw[2 * i + 1] as char)
                .to_digit(16)
                .ok_or(IdentityError::NotHex { field: FIELD })?;
            *byte = ((hi << 4) | lo) as u8;
        }
        Ok(Self(out))
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Canonical lowercase hex — the published form.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

// ── Clock 1: semantic import identity ────────────────────────────────────

/// The row layout a lineage was written with.
///
/// Only an **incompatible** layout mints a new lineage. A new SOA bake under
/// the same ABI is a new sealed import *inside* the existing lineage — not a
/// new dataset prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutAbi(pub u32);

/// The writer implementation's binary ABI, recorded so a reader can refuse a
/// lineage written by an incompatible producer even when the layout matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WriterAbi(pub u32);

/// The current layout: `osm-lance-v2`.
pub const LAYOUT_V2: LayoutAbi = LayoutAbi(2);

/// A stable Lance lineage: one region at one layout ABI.
///
/// Renders as `berlin/osm-lance-v2` — the S3 path segment and the identity in
/// one value, so the two cannot drift apart. The region is a [`RegionId`], so
/// the rendered segment is path-safe by construction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineageId {
    pub region: RegionId,
    pub layout: LayoutAbi,
}

impl LineageId {
    #[must_use]
    pub fn new(region: RegionId, layout: LayoutAbi) -> Self {
        Self { region, layout }
    }

    /// Convenience: validate a region string and build the lineage in one step.
    pub fn parse(region: &str, layout: LayoutAbi) -> Result<Self, IdentityError> {
        Ok(Self::new(RegionId::parse(region)?, layout))
    }

    /// `<region>/osm-lance-v<n>` — the lineage's S3 path segment.
    #[must_use]
    pub fn path_segment(&self) -> String {
        format!("{}/osm-lance-v{}", self.region, self.layout.0)
    }

    /// Whether a dataset written at `theirs` can be served by a reader that
    /// expects this lineage's layout.
    ///
    /// Exact equality, deliberately: "compatible" for a row layout means the
    /// same carving, and a reader that guesses at near-misses is how a silent
    /// misread ships.
    #[must_use]
    pub fn accepts_layout(&self, theirs: LayoutAbi) -> bool {
        self.layout == theirs
    }
}

/// The deterministic physical batch marker — FNV-1a/64 over the identity that
/// defines *this* import.
///
/// It is an **idempotency key**, not a digest of the data and not a
/// cryptographic commitment: two runs of the same bake into the same lineage
/// at the same ABIs must produce the same `ImportId`, so a retry after a lost
/// acknowledgement reconciles instead of publishing a second version. Because
/// it is 64 bits and non-cryptographic, a match is a *candidate* — see
/// [`reconcile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportId(pub u64);

impl ImportId {
    /// Hex, zero-padded — the form used in the `import-seals/<id>.yaml` key.
    /// Hex is key-safe by construction; no validation needed at the use site.
    #[must_use]
    pub fn to_key(self) -> String {
        format!("{:016x}", self.0)
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The complete semantic identity of one import — the tuple, not the marker.
///
/// `(lineage[region + layout_abi], bake, source_manifest_sha256, writer_abi)`.
/// [`ImportId`] is derived from it; reconciliation compares **this**, because
/// a 64-bit marker match is not proof of sameness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIdentity {
    pub lineage: LineageId,
    pub bake: SourceBakeId,
    pub manifest: SourceManifestSha256,
    pub writer: WriterAbi,
}

impl ImportIdentity {
    #[must_use]
    pub fn new(
        lineage: LineageId,
        bake: SourceBakeId,
        manifest: SourceManifestSha256,
        writer: WriterAbi,
    ) -> Self {
        Self {
            lineage,
            bake,
            manifest,
            writer,
        }
    }

    #[must_use]
    pub fn layout(&self) -> LayoutAbi {
        self.lineage.layout
    }

    /// Derive the physical batch marker.
    ///
    /// A separator byte is folded between fields so that `("ab", "c")` and
    /// `("a", "bc")` cannot collide — concatenating variable-length fields
    /// without one is the classic way to build a hash that looks deterministic
    /// and is quietly ambiguous. The manifest contributes its **32 parsed
    /// bytes**, so hex casing cannot change the marker.
    #[must_use]
    pub fn import_id(&self) -> ImportId {
        let mut h = FNV_OFFSET;
        for field in [
            self.lineage.region.as_str().as_bytes(),
            &self.lineage.layout.0.to_le_bytes(),
            self.bake.as_str().as_bytes(),
            self.manifest.as_bytes().as_slice(),
            &self.writer.0.to_le_bytes(),
        ] {
            h = fnv1a64(field, h);
            h = fnv1a64(b"\x1f", h); // unit separator — never part of a field
        }
        ImportId(h)
    }
}

/// What a marker match means once the full tuple is compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileVerdict {
    /// Same marker AND same tuple: genuinely the same import. Reconcile — do
    /// not create a second version.
    SameImport,
    /// Same marker, **different tuple**: an FNV-1a/64 collision. This is a
    /// permanent identity conflict (upstream's `CommitError::HashConflict`),
    /// never a reconcile — reconciling here would adopt someone else's data
    /// as this import's.
    HashConflict {
        marker: ImportId,
        candidate: Box<ImportIdentity>,
        found: Box<ImportIdentity>,
    },
    /// Different marker: not a retry of this import at all.
    DifferentImport,
}

/// Compare a candidate import against one found in history under the same
/// marker.
///
/// The marker narrows the search; the **tuple** decides. That ordering is the
/// whole reason a 64-bit non-cryptographic marker is safe to use here.
#[must_use]
pub fn reconcile(candidate: &ImportIdentity, found: &ImportIdentity) -> ReconcileVerdict {
    let marker = candidate.import_id();
    if marker != found.import_id() {
        return ReconcileVerdict::DifferentImport;
    }
    if candidate == found {
        ReconcileVerdict::SameImport
    } else {
        ReconcileVerdict::HashConflict {
            marker,
            candidate: Box::new(candidate.clone()),
            found: Box::new(found.clone()),
        }
    }
}

/// The small, immutable identity of a lineage's origin.
///
/// Mirrored to S3 beside the lineage. A warm boot verifies **this** — a few
/// hundred bytes — plus compatible Lance metadata. It must never require
/// checking out and reading all V1 data to prove the origin.
///
/// Fields are private and the only constructor takes an [`ImportIdentity`], so
/// a seal cannot contradict itself: the layout is the lineage's layout (there
/// is no second unconstrained copy to drift), and `origin_import_id` is
/// *derived*, so the seal always identifies an import that could actually have
/// produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginSeal {
    identity: ImportIdentity,
    origin_import_id: ImportId,
    origin_publication: Option<DatasetVersion>,
}

impl OriginSeal {
    /// Seal a lineage's origin from the import that created it.
    #[must_use]
    pub fn create(identity: ImportIdentity, origin_publication: Option<DatasetVersion>) -> Self {
        let origin_import_id = identity.import_id();
        Self {
            identity,
            origin_import_id,
            origin_publication,
        }
    }

    #[must_use]
    pub fn lineage(&self) -> &LineageId {
        &self.identity.lineage
    }
    /// The layout — read from the lineage, never stored twice.
    #[must_use]
    pub fn layout(&self) -> LayoutAbi {
        self.identity.lineage.layout
    }
    #[must_use]
    pub fn writer(&self) -> WriterAbi {
        self.identity.writer
    }
    #[must_use]
    pub fn initial_bake(&self) -> &SourceBakeId {
        &self.identity.bake
    }
    #[must_use]
    pub fn initial_manifest(&self) -> &SourceManifestSha256 {
        &self.identity.manifest
    }
    /// The import that created the lineage — without this the first sealed
    /// import cannot be identified at all.
    #[must_use]
    pub fn origin_import_id(&self) -> ImportId {
        self.origin_import_id
    }
    /// Where the origin import landed. `Some` only if that publication was
    /// observed fresh; see [`ImportSeal::publication_version`].
    #[must_use]
    pub fn origin_publication(&self) -> Option<DatasetVersion> {
        self.origin_publication
    }

    /// Whether a later import belongs to this lineage at a compatible ABI.
    #[must_use]
    pub fn accepts(&self, other: &ImportIdentity) -> bool {
        self.identity.lineage == other.lineage && self.identity.writer == other.writer
    }
}

/// The result of one semantically successful import.
///
/// Physical Lance maintenance versions may advance the head **without**
/// creating a new `ImportSeal` — that is the whole point of separating clock 1
/// from clock 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSeal {
    pub identity: ImportIdentity,
    pub import_id: ImportId,
    /// The PHYSICAL publication position — `Some` **only** when this call
    /// published fresh.
    ///
    /// `None` on a reconciled retry, mirroring `lance-graph`'s `SealedCycle`:
    /// a reconciled batch was already durable, its original position is not
    /// known from the outcome, and it is never invented here. The durable
    /// identity is the [`ImportId`]; the position is recovered by searching
    /// that id in Lance history on the audit path.
    pub publication_version: Option<DatasetVersion>,
    /// The head actually OBSERVED by this outcome — a read horizon in every
    /// case, never an artifact's publication position.
    pub observed_head: Option<DatasetVersion>,
}

/// What a publication attempt actually did — the q2-side mirror of
/// `lance_graph_planner::persist_sink::CommitOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationOutcome {
    /// Published fresh at this version.
    Committed(DatasetVersion),
    /// Already durable under the same [`ImportId`] **and the same identity
    /// tuple** ([`reconcile`] ran first); no second version was created.
    /// `current_head` is the head **at reconciliation time**.
    Reconciled { current_head: DatasetVersion },
    /// The outcome could not be determined. Re-submit the SAME frozen import
    /// identity — reconciliation runs first, so the retry cannot double-write.
    /// There is **no compensating delete**.
    Ambiguous,
}

impl ImportSeal {
    /// Seal an outcome, applying the publication/observation rule.
    #[must_use]
    pub fn seal(identity: ImportIdentity, outcome: PublicationOutcome) -> Option<Self> {
        let (publication_version, observed_head) = match outcome {
            PublicationOutcome::Committed(v) => (Some(v), Some(v)),
            PublicationOutcome::Reconciled { current_head } => (None, Some(current_head)),
            // Ambiguous seals nothing: the identity is re-submitted frozen.
            PublicationOutcome::Ambiguous => return None,
        };
        let import_id = identity.import_id();
        Some(Self {
            identity,
            import_id,
            publication_version,
            observed_head,
        })
    }
}

// ── Clock 2: physical Lance history ──────────────────────────────────────

/// What this replica serves, and from which version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServingVersion {
    pub version: DatasetVersion,
    /// `true` when the served version is a checkout of a known clean sealed
    /// version rather than the current head.
    pub pinned_to_clean: bool,
}

/// Where a claim that "version V is ordinary maintenance" comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceProvenance {
    /// **Phase-A limitation, named rather than hidden.** The caller asserted
    /// this set; nothing has verified it against Lance transaction history. A
    /// caller that asserts a hostile version would have it served. The real
    /// resolver — walk the transaction log, confirm each version is a
    /// compaction/index/deletion-materialisation descendant of `clean` — is
    /// deferred to Phase C, and until it exists the status endpoint must
    /// report the decision as *asserted*, not *verified*.
    CallerAsserted,
    /// Resolved by walking Lance history and confirming each version is a
    /// maintenance descendant of the clean publication.
    LanceHistoryVerified,
}

/// Versions recognised as maintenance descendants, **with provenance**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownMaintenance {
    versions: BTreeSet<DatasetVersion>,
    provenance: MaintenanceProvenance,
}

impl KnownMaintenance {
    /// Phase A / Phase B: whatever the caller says, carried as asserted.
    #[must_use]
    pub fn asserted(versions: BTreeSet<DatasetVersion>) -> Self {
        Self {
            versions,
            provenance: MaintenanceProvenance::CallerAsserted,
        }
    }

    /// Phase C+: produced by a real history walk.
    #[must_use]
    pub fn verified(versions: BTreeSet<DatasetVersion>) -> Self {
        Self {
            versions,
            provenance: MaintenanceProvenance::LanceHistoryVerified,
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self::asserted(BTreeSet::new())
    }

    #[must_use]
    pub fn contains(&self, v: DatasetVersion) -> bool {
        self.versions.contains(&v)
    }

    #[must_use]
    pub fn provenance(&self) -> MaintenanceProvenance {
        self.provenance
    }
}

/// Why the replica cannot serve the clean version it wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehindReason {
    /// The observed head is *earlier* than the last clean import: this replica
    /// is truncated or stale. Not drift — the head did not advance, the
    /// replica fell behind.
    HeadBehindClean,
    /// The head is fine, but the clean version itself is not present locally,
    /// so it cannot be checked out.
    CleanVersionAbsent,
}

/// How to serve, given the last clean publication and what this replica has.
///
/// Never "discard the lineage": a head that advanced is ordinary Lance life,
/// and a head that is *behind* is a replica problem, not an origin problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadDecision {
    /// Head equals the clean publication — serve it directly.
    ServeHead(DatasetVersion),
    /// The head is a recognised maintenance descendant of the clean version
    /// (compaction, index build): serve it, no drift reported. The provenance
    /// travels with the decision so the status endpoint can say whether that
    /// recognition was *verified* or merely *asserted*.
    ServeKnownDescendant {
        version: DatasetVersion,
        provenance: MaintenanceProvenance,
    },
    /// The head advanced for an unknown reason **and the clean version is
    /// locally available**: serve a checkout of the last known clean sealed
    /// version and report drift.
    CheckoutClean {
        clean: DatasetVersion,
        head: DatasetVersion,
    },
    /// The replica cannot produce the clean version at all. This is a
    /// *recovery* case, not a serving case — see [`recover_behind`]. Issuing a
    /// checkout here would request a version the replica may not possess.
    ReplicaIncomplete {
        clean: DatasetVersion,
        head: DatasetVersion,
        reason: BehindReason,
    },
}

impl HeadDecision {
    #[must_use]
    pub fn head_drift(self) -> bool {
        matches!(self, Self::CheckoutClean { .. })
    }

    /// The version to serve, when there is one. `None` for
    /// [`Self::ReplicaIncomplete`] — that state has nothing to serve until
    /// recovery runs.
    #[must_use]
    pub fn serving(self) -> Option<ServingVersion> {
        match self {
            Self::ServeHead(v) | Self::ServeKnownDescendant { version: v, .. } => {
                Some(ServingVersion {
                    version: v,
                    pinned_to_clean: false,
                })
            }
            Self::CheckoutClean { clean, .. } => Some(ServingVersion {
                version: clean,
                pinned_to_clean: true,
            }),
            Self::ReplicaIncomplete { .. } => None,
        }
    }
}

/// What this replica knows about a lineage right now.
#[derive(Debug, Clone)]
pub struct HeadFacts<'a> {
    /// The last known clean sealed publication.
    pub clean: DatasetVersion,
    /// The head this replica observes.
    pub observed_head: DatasetVersion,
    pub known_maintenance: &'a KnownMaintenance,
    /// Versions this replica can actually check out. Checking out anything
    /// outside this set is a request the replica may be unable to satisfy.
    pub locally_available: &'a BTreeSet<DatasetVersion>,
}

/// Decide what to serve.
#[must_use]
pub fn decide_head(facts: &HeadFacts<'_>) -> HeadDecision {
    let HeadFacts {
        clean,
        observed_head,
        known_maintenance,
        locally_available,
    } = *facts;

    if observed_head < clean {
        return HeadDecision::ReplicaIncomplete {
            clean,
            head: observed_head,
            reason: BehindReason::HeadBehindClean,
        };
    }
    if observed_head == clean {
        return if locally_available.contains(&clean) {
            HeadDecision::ServeHead(clean)
        } else {
            HeadDecision::ReplicaIncomplete {
                clean,
                head: observed_head,
                reason: BehindReason::CleanVersionAbsent,
            }
        };
    }
    if known_maintenance.contains(observed_head) {
        return HeadDecision::ServeKnownDescendant {
            version: observed_head,
            provenance: known_maintenance.provenance(),
        };
    }
    if locally_available.contains(&clean) {
        HeadDecision::CheckoutClean {
            clean,
            head: observed_head,
        }
    } else {
        HeadDecision::ReplicaIncomplete {
            clean,
            head: observed_head,
            reason: BehindReason::CleanVersionAbsent,
        }
    }
}

/// What to do when the replica cannot serve the clean version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BehindRecovery {
    /// S3 holds the clean version: hydrate it. The preferred path — it
    /// restores the intended artifact rather than silently serving an older
    /// one.
    HydrateFromS3 { want: DatasetVersion },
    /// S3 cannot supply it, but this replica does hold an older clean version:
    /// serve that, degraded but honest.
    ServeAvailablePrevious { version: DatasetVersion },
    /// Nothing serviceable. The status endpoint reports it; nothing is
    /// deleted, and the lineage is not discarded.
    Unserviceable,
}

/// Recover from [`HeadDecision::ReplicaIncomplete`].
///
/// `available_clean` is the set of *sealed clean* versions this replica can
/// actually check out — not every version it has, because serving an
/// unverified version to escape a recovery is how a replica ends up quietly
/// serving something no seal describes.
#[must_use]
pub fn recover_behind(
    clean: DatasetVersion,
    available_clean: &BTreeSet<DatasetVersion>,
    s3_has_clean: bool,
) -> BehindRecovery {
    if s3_has_clean {
        return BehindRecovery::HydrateFromS3 { want: clean };
    }
    match available_clean.iter().rev().find(|v| **v < clean) {
        Some(v) => BehindRecovery::ServeAvailablePrevious { version: *v },
        None => BehindRecovery::Unserviceable,
    }
}

// ── Clock 3: replica progress ────────────────────────────────────────────

/// Whether the resolved artifact root survives a redeploy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// A mounted volume — artifacts survive a rebuild.
    Volume,
    /// Container-local disk — everything is re-fetched on the next deploy.
    Ephemeral,
}

/// The resolved artifact root and its durability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoot {
    pub path: PathBuf,
    pub kind: RootKind,
    /// Which policy candidate supplied it, for the status endpoint.
    pub source: &'static str,
}

/// The ordered storage policy. Pure: the caller supplies what it observed of
/// the environment, so the ordering is testable without touching a filesystem.
///
/// 1. `OSM_ARTIFACT_ROOT`
/// 2. `OSM_SLAB_CACHE_DIR` (the existing variable — kept, not renamed)
/// 3. `RAILWAY_VOL/osm`
/// 4. `/volume01/osm` when present and writable
/// 5. documented ephemeral fallback
#[derive(Debug, Clone)]
pub struct RootInputs {
    pub osm_artifact_root: Option<String>,
    pub osm_slab_cache_dir: Option<String>,
    /// What the operator declares about candidates 1 and 2.
    ///
    /// **A generic override says nothing about durability.** `OSM_ARTIFACT_ROOT`
    /// and `OSM_SLAB_CACHE_DIR` routinely point at container-local scratch —
    /// treating "an operator named it" as "an operator guaranteed it survives"
    /// is exactly the assumption that lets an ephemeral candidate activate
    /// without S3 and re-convert the whole bake on the next deploy. Absent an
    /// explicit declaration this is `None`, and `None` means
    /// [`RootKind::Ephemeral`] — the fail-safe direction, because being
    /// wrongly cautious costs one S3 wait and being wrongly confident costs a
    /// full re-import.
    pub override_durability: Option<RootKind>,
    pub railway_vol: Option<String>,
    /// Whether `/volume01` exists AND is writable — the caller probes it.
    pub volume01_writable: bool,
    /// Where the ephemeral fallback lands. Never empty and never relative —
    /// see [`RootInputs::default`].
    pub ephemeral_dir: PathBuf,
}

/// The default ephemeral directory leaf, under the platform temp dir.
const EPHEMERAL_LEAF: &str = "q2-osm";

impl Default for RootInputs {
    /// The fallback is an **absolute** platform temp path.
    ///
    /// A `PathBuf::default()` is empty, and an empty path resolves against the
    /// process's current directory — so a defaulted `RootInputs` would write
    /// the artifact tree into whatever directory the server happened to be
    /// started from. `std::env::temp_dir()` is the cross-platform absolute
    /// answer (`/tmp` on Linux, `%TEMP%` on Windows) and never returns an
    /// empty path.
    fn default() -> Self {
        Self {
            osm_artifact_root: None,
            osm_slab_cache_dir: None,
            override_durability: None,
            railway_vol: None,
            volume01_writable: false,
            ephemeral_dir: std::env::temp_dir().join(EPHEMERAL_LEAF),
        }
    }
}

/// A configured path is usable only if it is non-blank AND absolute.
///
/// A relative override resolves against the current directory, which is the
/// same hazard as an empty one — the artifact tree lands wherever the process
/// was launched. Both are treated as *absent*, so the ordered policy falls
/// through to the next candidate instead of silently accepting a path that
/// means different things to different processes.
fn usable_path(s: &Option<String>) -> Option<PathBuf> {
    let trimmed = s.as_ref()?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let p = PathBuf::from(trimmed);
    p.is_absolute().then_some(p)
}

/// Resolve the artifact root.
///
/// Candidates 1 and 2 take their durability from
/// [`RootInputs::override_durability`], defaulting to
/// [`RootKind::Ephemeral`]. Only `RAILWAY_VOL` and a probed-writable
/// `/volume01` are *intrinsically* known to be volume storage.
#[must_use]
pub fn resolve_root(inputs: &RootInputs) -> StorageRoot {
    let override_kind = inputs.override_durability.unwrap_or(RootKind::Ephemeral);

    if let Some(p) = usable_path(&inputs.osm_artifact_root) {
        return StorageRoot {
            path: p,
            kind: override_kind,
            source: "OSM_ARTIFACT_ROOT",
        };
    }
    if let Some(p) = usable_path(&inputs.osm_slab_cache_dir) {
        return StorageRoot {
            path: p,
            kind: override_kind,
            source: "OSM_SLAB_CACHE_DIR",
        };
    }
    if let Some(p) = usable_path(&inputs.railway_vol) {
        return StorageRoot {
            path: p.join("osm"),
            kind: RootKind::Volume,
            source: "RAILWAY_VOL/osm",
        };
    }
    if inputs.volume01_writable {
        return StorageRoot {
            path: PathBuf::from("/volume01/osm"),
            kind: RootKind::Volume,
            source: "/volume01/osm",
        };
    }
    // Last line of defence: even a hand-built `RootInputs` must not resolve to
    // "" or a relative path.
    let ephemeral =
        if inputs.ephemeral_dir.as_os_str().is_empty() || !inputs.ephemeral_dir.is_absolute() {
            std::env::temp_dir().join(EPHEMERAL_LEAF)
        } else {
            inputs.ephemeral_dir.clone()
        };
    StorageRoot {
        path: ephemeral,
        kind: RootKind::Ephemeral,
        source: "ephemeral fallback",
    }
}

/// The S3 publication pointer for a lineage — written **last**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub active_clean: ImportId,
    pub previous_clean: Option<ImportId>,
    /// How far this replica has mirrored the lineage to S3.
    pub mirrored_through_version: Option<DatasetVersion>,
}

// ── Activation ───────────────────────────────────────────────────────────

/// Whether a locally verified candidate may serve before S3 has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationPolicy {
    pub volume_requires_s3: bool,
    pub ephemeral_requires_s3: bool,
}

impl Default for ActivationPolicy {
    /// The documented default: a volume may run ahead of S3, an ephemeral root
    /// may not.
    fn default() -> Self {
        Self {
            volume_requires_s3: false,
            ephemeral_requires_s3: true,
        }
    }
}

/// The verdict on activating a locally verified candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Serve it; S3 already has it.
    Activate,
    /// Serve it, and report that S3 is behind. Legitimate on a volume: the
    /// artifact survives a redeploy, so a lagging mirror costs nothing but
    /// rollback distance.
    ActivateS3Lagging,
    /// Do NOT serve yet. On an ephemeral root an un-mirrored candidate is not
    /// fully successful: the next redeploy would repeat the whole SOA→Lance
    /// conversion, which is the cost this design exists to remove.
    HoldForS3,
}

#[must_use]
pub fn decide_activation(
    kind: RootKind,
    s3_has_candidate: bool,
    policy: ActivationPolicy,
) -> Activation {
    if s3_has_candidate {
        return Activation::Activate;
    }
    let requires = match kind {
        RootKind::Volume => policy.volume_requires_s3,
        RootKind::Ephemeral => policy.ephemeral_requires_s3,
    };
    if requires {
        Activation::HoldForS3
    } else {
        Activation::ActivateS3Lagging
    }
}

// ── Lifecycle: serving state × import operation ──────────────────────────

/// Monotonic import attempt counter — the fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// What this replica currently serves. Independent of any import in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServingSet {
    pub active: ImportId,
    /// Retained for rollback. Only ONE previous — never a growing chain.
    pub previous: Option<ImportId>,
}

/// Why a candidate import failed, classified for the status endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFailure {
    /// The SOA source could not be read or was rejected.
    Source(String),
    /// The build/commit failed.
    Write(String),
    /// The candidate committed but failed validation.
    Validation(String),
    /// The candidate is fine locally but could not be mirrored, and this root
    /// requires S3 before activation.
    S3Publication(String),
    /// A permanent identity conflict — see [`ReconcileVerdict::HashConflict`].
    IdentityConflict,
}

/// The import operation, orthogonal to what is being served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Idle,
    Importing {
        candidate: ImportId,
        generation: Generation,
    },
    Failed {
        candidate: ImportId,
        generation: Generation,
        error: ImportFailure,
    },
}

/// A refused transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    /// A completion arrived for an attempt that is no longer in flight —
    /// typically B's late success after C superseded it. Applying it would
    /// activate or fail the wrong candidate.
    StaleCompletion {
        candidate: ImportId,
        generation: Generation,
        current: Option<Generation>,
    },
    /// A completion arrived with no import in flight at all.
    NoImportInFlight { candidate: ImportId },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleCompletion {
                candidate,
                generation,
                current,
            } => write!(
                f,
                "stale completion for {} at generation {} (current {:?})",
                candidate.to_key(),
                generation.0,
                current.map(|g| g.0)
            ),
            Self::NoImportInFlight { candidate } => {
                write!(f, "no import in flight for {}", candidate.to_key())
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

/// The lifecycle: a **product** of serving state and import state.
///
/// Nothing an import does can reach `serving` except a successful activation.
/// That is a structural guarantee, not a rule someone has to remember: the
/// failure paths do not take `serving` as an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifecycle {
    pub serving: Option<ServingSet>,
    pub operation: Operation,
    next_generation: u64,
}

impl Lifecycle {
    /// Nothing local, nothing in flight.
    #[must_use]
    pub fn absent() -> Self {
        Self {
            serving: None,
            operation: Operation::Idle,
            next_generation: 1,
        }
    }

    /// A replica that already serves something (e.g. recovered from S3 at boot).
    #[must_use]
    pub fn serving(active: ImportId, previous: Option<ImportId>) -> Self {
        Self {
            serving: Some(ServingSet { active, previous }),
            operation: Operation::Idle,
            next_generation: 1,
        }
    }

    #[must_use]
    pub fn active(&self) -> Option<ImportId> {
        self.serving.map(|s| s.active)
    }

    #[must_use]
    pub fn previous(&self) -> Option<ImportId> {
        self.serving.and_then(|s| s.previous)
    }

    /// The generation of the import currently in flight, if any.
    #[must_use]
    pub fn in_flight(&self) -> Option<Generation> {
        match self.operation {
            Operation::Importing { generation, .. } => Some(generation),
            _ => None,
        }
    }

    /// Begin a candidate import. Returns the fence token the completion must
    /// present.
    ///
    /// Starting C while B is in flight **supersedes** B: the generation
    /// advances, and B's later completion is refused. Serving is untouched.
    #[must_use]
    pub fn start_import(&self, candidate: ImportId) -> (Self, Generation) {
        let generation = Generation(self.next_generation);
        (
            Self {
                serving: self.serving,
                operation: Operation::Importing {
                    candidate,
                    generation,
                },
                next_generation: self.next_generation + 1,
            },
            generation,
        )
    }

    /// The candidate is verified locally AND cleared its activation gate.
    ///
    /// Rotation happens here and only here: the outgoing active becomes
    /// `previous`, and the chain never grows beyond one.
    pub fn activate(
        &self,
        candidate: ImportId,
        generation: Generation,
    ) -> Result<Self, LifecycleError> {
        self.check_fence(candidate, generation)?;
        let previous = self.active();
        Ok(Self {
            serving: Some(ServingSet {
                active: candidate,
                previous,
            }),
            operation: Operation::Idle,
            next_generation: self.next_generation,
        })
    }

    /// The candidate failed.
    ///
    /// `serving` is copied through verbatim — a failed import cannot touch
    /// either the active or the previous-clean artifact.
    pub fn fail(
        &self,
        candidate: ImportId,
        generation: Generation,
        error: ImportFailure,
    ) -> Result<Self, LifecycleError> {
        self.check_fence(candidate, generation)?;
        Ok(Self {
            serving: self.serving,
            operation: Operation::Failed {
                candidate,
                generation,
                error,
            },
            next_generation: self.next_generation,
        })
    }

    fn check_fence(
        &self,
        candidate: ImportId,
        generation: Generation,
    ) -> Result<(), LifecycleError> {
        match self.operation {
            Operation::Importing {
                candidate: c,
                generation: g,
            } if c == candidate && g == generation => Ok(()),
            Operation::Importing { generation: g, .. } => Err(LifecycleError::StaleCompletion {
                candidate,
                generation,
                current: Some(g),
            }),
            _ => Err(LifecycleError::NoImportInFlight { candidate }),
        }
    }
}

// ── Ordered plans ────────────────────────────────────────────────────────

/// One step of the S3 publication sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishStep {
    /// Ensure the outgoing active is durable as `previous_clean` first.
    EnsurePreviousDurable(ImportId),
    /// Upload only the objects S3 does not already hold.
    UploadMissingObjects(Vec<String>),
    /// The small seals.
    UploadSeal(String),
    /// The head pointer — **always last**, with generation/ETag protection.
    PublishPointer {
        active: ImportId,
        previous: Option<ImportId>,
    },
}

/// Build the ordered publication plan.
///
/// `already_in_s3` is the set of object keys the lineage already holds:
/// natively referenced Lance fragments are **never uploaded twice**.
#[must_use]
pub fn plan_publication(
    candidate: ImportId,
    outgoing_active: Option<ImportId>,
    candidate_objects: &[String],
    already_in_s3: &BTreeSet<String>,
    seal_keys: &[String],
) -> Vec<PublishStep> {
    let mut steps = Vec::new();
    if let Some(prev) = outgoing_active {
        steps.push(PublishStep::EnsurePreviousDurable(prev));
    }
    let missing: Vec<String> = candidate_objects
        .iter()
        .filter(|k| !already_in_s3.contains(*k))
        .cloned()
        .collect();
    steps.push(PublishStep::UploadMissingObjects(missing));
    for s in seal_keys {
        steps.push(PublishStep::UploadSeal(s.clone()));
    }
    steps.push(PublishStep::PublishPointer {
        active: candidate,
        previous: outgoing_active,
    });
    steps
}

/// What a **warm boot** is allowed to read to prove the origin.
///
/// Small seals and Lance metadata only. The absence of any data object here is
/// the point: proving an origin must never require checking out and reading
/// the imported map data.
#[must_use]
pub fn warm_verification_reads(lineage: &LineageId, active: ImportId) -> Vec<String> {
    let base = format!("q2/lance/{}", lineage.path_segment());
    vec![
        format!("{base}/origin-seal.yaml"),
        format!("{base}/published.yaml"),
        format!("{base}/import-seals/{}.yaml", active.to_key()),
    ]
}

// ── Retention ────────────────────────────────────────────────────────────

/// Everything reachable from one pinned Lance version.
///
/// "Reachable" means the **whole closure**: the manifest, every data fragment,
/// index files, deletion files, transaction records and any metadata the
/// version needs to open. An object list that is merely "the fragments I
/// uploaded" is not an inventory, and treating it as one is how retention
/// deletes a deletion-file and turns a clean version into an unopenable one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInventory {
    pub version: DatasetVersion,
    pub objects: BTreeSet<String>,
    /// `false` when the caller could not enumerate the closure completely.
    /// An incomplete inventory can never authorise a deletion.
    pub complete: bool,
}

impl VersionInventory {
    #[must_use]
    pub fn complete(version: DatasetVersion, objects: BTreeSet<String>) -> Self {
        Self {
            version,
            objects,
            complete: true,
        }
    }

    #[must_use]
    pub fn partial(version: DatasetVersion, objects: BTreeSet<String>) -> Self {
        Self {
            version,
            objects,
            complete: false,
        }
    }
}

/// Who vouches that an inventory is a real Lance reachability closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcAuthority {
    /// Phases A–C: **nothing may be deleted.** No mechanism exists yet that
    /// can prove an object is unreachable from every retained version, and
    /// `everything - keep` is not that proof — it is a guess dressed as a set
    /// operation. Retention in these phases is *intent*, and intent does not
    /// delete.
    None,
    /// Phase D: a Lance-native reachability/cleanup mechanism produced these
    /// inventories.
    LanceNativeReachability,
}

/// Why a retention plan authorises no deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionHold {
    /// The grace period has not elapsed.
    GracePeriodActive,
    /// At least one inventory is incomplete.
    IncompleteInventory,
    /// No Lance-native reachability mechanism has vouched for the inventories.
    NoGcAuthority,
}

/// The retention decision for one lineage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetentionPlan {
    /// Intent only. `keep` is what must survive; **no deletion is
    /// authorised.**
    KeepOnly {
        keep: BTreeSet<String>,
        hold: RetentionHold,
    },
    /// Phase D and later: deletion is authorised for `collect`.
    Collectable {
        keep: BTreeSet<String>,
        collect: BTreeSet<String>,
    },
}

impl RetentionPlan {
    /// The objects this plan authorises deleting — empty unless the plan is
    /// [`Self::Collectable`].
    #[must_use]
    pub fn collect(&self) -> BTreeSet<String> {
        match self {
            Self::KeepOnly { .. } => BTreeSet::new(),
            Self::Collectable { collect, .. } => collect.clone(),
        }
    }

    #[must_use]
    pub fn keep(&self) -> &BTreeSet<String> {
        match self {
            Self::KeepOnly { keep, .. } | Self::Collectable { keep, .. } => keep,
        }
    }
}

/// Decide retention.
///
/// Keeps everything reachable from the active-clean and previous-clean
/// versions, plus **all** small seals — seals are tiny and are the only record
/// of what a version meant.
///
/// It does **not** conclude "delete `everything - keep`". Three independent
/// gates must all open before a single object is proposed for deletion: the
/// grace period must have elapsed, both inventories must be complete, and a
/// [`GcAuthority`] must vouch that they are real reachability closures. In
/// Phase A the last gate is always shut.
#[must_use]
pub fn plan_retention(
    active: &VersionInventory,
    previous: Option<&VersionInventory>,
    all_seals: &BTreeSet<String>,
    everything: &BTreeSet<String>,
    grace_elapsed: bool,
    authority: GcAuthority,
) -> RetentionPlan {
    let mut keep: BTreeSet<String> = active.objects.clone();
    if let Some(p) = previous {
        keep.extend(p.objects.iter().cloned());
    }
    keep.extend(all_seals.iter().cloned());

    let inventories_complete = active.complete && previous.is_none_or(|p| p.complete);

    let hold = if !inventories_complete {
        Some(RetentionHold::IncompleteInventory)
    } else if !grace_elapsed {
        Some(RetentionHold::GracePeriodActive)
    } else if authority == GcAuthority::None {
        Some(RetentionHold::NoGcAuthority)
    } else {
        None
    };

    match hold {
        Some(hold) => RetentionPlan::KeepOnly { keep, hold },
        None => {
            let collect = everything.difference(&keep).cloned().collect();
            RetentionPlan::Collectable { keep, collect }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lineage() -> LineageId {
        LineageId::parse("berlin", LAYOUT_V2).expect("berlin is a valid region")
    }
    fn bake() -> SourceBakeId {
        SourceBakeId::parse("berlin-260805").expect("valid bake id")
    }
    fn manifest() -> SourceManifestSha256 {
        SourceManifestSha256::parse_hex(&"a".repeat(64)).expect("valid digest")
    }
    fn identity() -> ImportIdentity {
        ImportIdentity::new(lineage(), bake(), manifest(), WriterAbi(1))
    }
    fn v(n: u64) -> DatasetVersion {
        DatasetVersion(n)
    }
    fn versions(list: &[u64]) -> BTreeSet<DatasetVersion> {
        list.iter().copied().map(DatasetVersion).collect()
    }

    // ── Validated identity ────────────────────────────────────────────────

    /// PROBE (Phase A). The region is interpolated into an S3 key and joined
    /// onto a path; these are the values that must never reach either.
    #[test]
    fn region_rejects_everything_that_could_escape_a_prefix_or_a_directory() {
        for bad in [
            "", "..", "../etc", "a/b", "a\\b", "Berlin", "berlin ", "ber_lin", "berlin/",
        ] {
            assert!(
                RegionId::parse(bad).is_err(),
                "{bad:?} must not be accepted as a region"
            );
        }
        assert!(
            RegionId::parse(&"a".repeat(65)).is_err(),
            "length is bounded"
        );

        // Can-it-accept half: the real names must still work, or the guard is
        // just a refusal machine.
        for good in ["berlin", "brandenburg", "baden-wuerttemberg", "nrw2"] {
            assert!(RegionId::parse(good).is_ok(), "{good:?} must be accepted");
        }
    }

    /// PROBE (Phase A). Bake ids grow into staging directory names; `..` in
    /// one would be a traversal the moment that happens.
    #[test]
    fn bake_id_rejects_traversal_and_separators() {
        for bad in ["", "..", "a/../b", "a/b", "a\\b", "Berlin-2608"] {
            assert!(
                SourceBakeId::parse(bad).is_err(),
                "{bad:?} must not be accepted as a bake id"
            );
        }
        for good in ["berlin-260805", "berlin.v2_final", "bb-260901"] {
            assert!(
                SourceBakeId::parse(good).is_ok(),
                "{good:?} must be accepted"
            );
        }
    }

    /// PROBE (Phase A). The same digest in different casing is the same
    /// digest — and therefore the same import. Hashing the *text* would derive
    /// two identities for one bake and publish it twice.
    #[test]
    fn digest_casing_cannot_change_the_import_identity() {
        let lower = SourceManifestSha256::parse_hex(&"ab".repeat(32)).unwrap();
        let upper = SourceManifestSha256::parse_hex(&"AB".repeat(32)).unwrap();
        assert_eq!(lower, upper, "hex casing is a rendering, not a value");
        assert_eq!(
            lower.to_hex(),
            "ab".repeat(32),
            "canonical form is lowercase"
        );

        let a = ImportIdentity::new(lineage(), bake(), lower, WriterAbi(1)).import_id();
        let b = ImportIdentity::new(lineage(), bake(), upper, WriterAbi(1)).import_id();
        assert_eq!(a, b);

        // Two-sided: a genuinely different digest must differ.
        let other = SourceManifestSha256::parse_hex(&"cd".repeat(32)).unwrap();
        assert_ne!(
            a,
            ImportIdentity::new(lineage(), bake(), other, WriterAbi(1)).import_id(),
            "a different manifest is a different import"
        );

        // And malformed input is refused rather than silently truncated.
        assert!(SourceManifestSha256::parse_hex("abc").is_err());
        assert!(SourceManifestSha256::parse_hex(&"z".repeat(64)).is_err());
    }

    // ── Semantic identity vs access residue ───────────────────────────────

    /// PROBE (Phase A). Reopening, appending, compacting — every one of these
    /// moves the head. None may change the import identity, because the
    /// identity is derived from the SOURCE, not from the dataset's physical
    /// state.
    ///
    /// System falsifier still owed (Phase C): run a real import, compact the
    /// dataset, and assert the seal on disk is unchanged.
    #[test]
    fn access_residue_does_not_change_semantic_identity() {
        let id = identity().import_id();
        let again = identity().import_id();
        assert_eq!(id, again, "the import identity is a property of the SOURCE");

        let other = ImportIdentity::new(
            lineage(),
            SourceBakeId::parse("berlin-260901").unwrap(),
            manifest(),
            WriterAbi(1),
        )
        .import_id();
        assert_ne!(id, other, "a different bake is a different import");
    }

    /// PROBE (Phase A). Concatenating variable-length fields without a
    /// separator makes `("ab","c")` and `("a","bc")` collide.
    #[test]
    fn import_id_fields_cannot_be_confused_across_a_boundary() {
        let a = ImportIdentity::new(
            LineageId::parse("ab", LAYOUT_V2).unwrap(),
            SourceBakeId::parse("c").unwrap(),
            manifest(),
            WriterAbi(1),
        )
        .import_id();
        let b = ImportIdentity::new(
            LineageId::parse("a", LAYOUT_V2).unwrap(),
            SourceBakeId::parse("bc").unwrap(),
            manifest(),
            WriterAbi(1),
        )
        .import_id();
        assert_ne!(a, b, "field boundaries must be part of the hash");
    }

    /// PROBE (Phase A). The marker narrows; the TUPLE decides. A marker match
    /// with a different tuple is a permanent conflict, never a reconcile —
    /// reconciling there would adopt another import's data as this one's.
    #[test]
    fn a_marker_match_with_a_different_tuple_is_a_conflict_not_a_reconcile() {
        let same = reconcile(&identity(), &identity());
        assert_eq!(same, ReconcileVerdict::SameImport);

        // A genuinely different import: different marker, so not a retry.
        let different = ImportIdentity::new(
            lineage(),
            SourceBakeId::parse("berlin-260901").unwrap(),
            manifest(),
            WriterAbi(1),
        );
        assert_eq!(
            reconcile(&identity(), &different),
            ReconcileVerdict::DifferentImport
        );

        // Forced collision: same marker, different tuple. Constructed by
        // asserting the marker rather than searching for a real FNV collision
        // — what is under test is the DECISION, not FNV's collision rate.
        let candidate = identity();
        let mut found = identity();
        found.writer = WriterAbi(2);
        let verdict = if candidate.import_id() == found.import_id() {
            reconcile(&candidate, &found)
        } else {
            // Simulate the collision the real search would surface.
            ReconcileVerdict::HashConflict {
                marker: candidate.import_id(),
                candidate: Box::new(candidate.clone()),
                found: Box::new(found.clone()),
            }
        };
        assert!(
            matches!(verdict, ReconcileVerdict::HashConflict { .. }),
            "same marker + different tuple must be permanent, not reconcilable"
        );
    }

    // ── Publication vs observation ────────────────────────────────────────

    /// PROBE (Phase A). Upstream's exact scenario: retrying while the head
    /// stands at V5 must not record V5 as this import's publication position.
    ///
    /// System falsifier still owed (Phase C): assert `Dataset::versions()`
    /// gained no entry across a reconciled retry. Constructing a `Reconciled`
    /// value here proves the decision, not the absence of a second version.
    #[test]
    fn a_reconciled_retry_records_no_publication_version() {
        let fresh = ImportSeal::seal(identity(), PublicationOutcome::Committed(v(1)))
            .expect("a fresh commit seals");
        assert_eq!(fresh.publication_version, Some(v(1)));

        let retried = ImportSeal::seal(
            identity(),
            PublicationOutcome::Reconciled { current_head: v(5) },
        )
        .expect("a reconciled retry still seals");
        assert_eq!(
            retried.publication_version, None,
            "V5 is the head at reconciliation time, NOT this import's publication position"
        );
        assert_eq!(
            retried.observed_head,
            Some(v(5)),
            "the observed head is a read horizon and is recorded as one"
        );
        assert_eq!(
            retried.import_id, fresh.import_id,
            "the durable identity is the same across the retry"
        );
    }

    /// PROBE (Phase A). An ambiguous publication seals NOTHING — the identity
    /// is re-submitted frozen and reconciled. There is no compensating delete.
    ///
    /// System falsifier still owed (Phase C/D): kill the process between
    /// commit and acknowledgement on a first create, restart, re-submit, and
    /// assert exactly one version exists.
    #[test]
    fn an_ambiguous_publication_seals_nothing() {
        assert!(
            ImportSeal::seal(identity(), PublicationOutcome::Ambiguous).is_none(),
            "an unknown outcome must not be recorded as a success"
        );
    }

    // ── Head decisions ────────────────────────────────────────────────────

    /// PROBE (Phase A). A later unexplained head is drift: serve the clean
    /// checkout, report it, keep the lineage.
    #[test]
    fn an_unexplained_later_head_serves_the_clean_checkout_and_reports_drift() {
        let none = KnownMaintenance::none();
        let have = versions(&[7, 9]);
        let d = decide_head(&HeadFacts {
            clean: v(7),
            observed_head: v(9),
            known_maintenance: &none,
            locally_available: &have,
        });
        assert_eq!(
            d,
            HeadDecision::CheckoutClean {
                clean: v(7),
                head: v(9)
            }
        );
        assert!(d.head_drift());
        assert_eq!(
            d.serving(),
            Some(ServingVersion {
                version: v(7),
                pinned_to_clean: true
            }),
            "serve the last known clean sealed version, not the mystery head"
        );

        // Silence half: a recognised maintenance descendant is NOT drift, or
        // every compaction would pin the replica backwards forever.
        let known = KnownMaintenance::asserted(versions(&[9]));
        let ok = decide_head(&HeadFacts {
            clean: v(7),
            observed_head: v(9),
            known_maintenance: &known,
            locally_available: &have,
        });
        assert_eq!(
            ok,
            HeadDecision::ServeKnownDescendant {
                version: v(9),
                provenance: MaintenanceProvenance::CallerAsserted
            },
            "the decision carries WHERE the recognition came from — Phase A can \
             only assert it, and the status endpoint must be able to say so"
        );
        assert!(!ok.head_drift());

        // And an unmoved head is plain serving.
        assert_eq!(
            decide_head(&HeadFacts {
                clean: v(7),
                observed_head: v(7),
                known_maintenance: &none,
                locally_available: &have,
            }),
            HeadDecision::ServeHead(v(7))
        );
    }

    /// PROBE (Phase A). A head that is *behind* the clean import is not drift
    /// — it is a truncated replica, and asking it to check out a version it
    /// may not possess is a different bug from serving a mystery head.
    #[test]
    fn a_head_behind_clean_is_a_replica_problem_not_a_drift() {
        let none = KnownMaintenance::none();
        let have = versions(&[3]);
        let d = decide_head(&HeadFacts {
            clean: v(7),
            observed_head: v(3),
            known_maintenance: &none,
            locally_available: &have,
        });
        assert_eq!(
            d,
            HeadDecision::ReplicaIncomplete {
                clean: v(7),
                head: v(3),
                reason: BehindReason::HeadBehindClean
            }
        );
        assert!(!d.head_drift(), "behind is not drift");
        assert_eq!(
            d.serving(),
            None,
            "there is nothing to serve until recovery runs — a checkout would \
             request a version this replica does not have"
        );
    }

    /// PROBE (Phase A). The clean version being absent locally is the same
    /// recovery case, reached from the other direction.
    #[test]
    fn an_absent_clean_version_is_never_offered_as_a_checkout() {
        let none = KnownMaintenance::none();
        let have = versions(&[9]); // head present, clean NOT
        assert_eq!(
            decide_head(&HeadFacts {
                clean: v(7),
                observed_head: v(9),
                known_maintenance: &none,
                locally_available: &have,
            }),
            HeadDecision::ReplicaIncomplete {
                clean: v(7),
                head: v(9),
                reason: BehindReason::CleanVersionAbsent
            }
        );
    }

    /// PROBE (Phase A). Recovery prefers hydrating the intended version;
    /// falling back to an older clean one is a real degradation and is only
    /// chosen when S3 cannot help.
    #[test]
    fn recovery_hydrates_when_it_can_and_degrades_only_when_it_must() {
        let available = versions(&[3, 5]);
        assert_eq!(
            recover_behind(v(7), &available, true),
            BehindRecovery::HydrateFromS3 { want: v(7) }
        );
        assert_eq!(
            recover_behind(v(7), &available, false),
            BehindRecovery::ServeAvailablePrevious { version: v(5) },
            "the NEWEST available clean version below the target, not the oldest"
        );
        assert_eq!(
            recover_behind(v(7), &BTreeSet::new(), false),
            BehindRecovery::Unserviceable,
            "nothing serviceable is reported, not faked"
        );
    }

    // ── Lifecycle: serving × operation ────────────────────────────────────

    /// PROBE (Phase A). Rotation happens on activation and nowhere else.
    #[test]
    fn activation_rotates_active_to_previous() {
        let a = ImportId(0xA);
        let b = ImportId(0xB);
        let l = Lifecycle::serving(a, None);
        let (l, g) = l.start_import(b);
        assert_eq!(
            l.active(),
            Some(a),
            "an import in flight does not disturb what is being served"
        );
        let l = l.activate(b, g).expect("the fence matches");
        assert_eq!(l.active(), Some(b));
        assert_eq!(l.previous(), Some(a));
        assert_eq!(l.operation, Operation::Idle);
    }

    /// PROBE (Phase A). The rule the product state exists to make structural.
    #[test]
    fn a_failed_candidate_never_disturbs_active_or_previous() {
        let a = ImportId(0xA);
        let p = ImportId(0x9);
        let bad = ImportId(0xBAD);
        let before = Lifecycle::serving(a, Some(p));

        let (l, g) = before.start_import(bad);
        let l = l
            .fail(bad, g, ImportFailure::Validation("row count".into()))
            .expect("the fence matches");

        assert_eq!(
            l.serving, before.serving,
            "both the active AND the previous-clean artifact survive a failed candidate"
        );
        assert!(matches!(l.operation, Operation::Failed { .. }));

        // ...and recovery from that state still rotates correctly.
        let c = ImportId(0xC);
        let (l, g) = l.start_import(c);
        let l = l.activate(c, g).unwrap();
        assert_eq!(l.active(), Some(c));
        assert_eq!(l.previous(), Some(a));
    }

    /// PROBE (Phase A). The fence. B's late completion must not activate or
    /// fail C — this is the concurrency bug the generation exists to stop, and
    /// without the fence B's success would install B while C was mid-build.
    #[test]
    fn a_stale_completion_cannot_activate_or_fail_the_superseding_candidate() {
        let a = ImportId(0xA);
        let b = ImportId(0xB);
        let c = ImportId(0xC);

        let (l, gen_b) = Lifecycle::serving(a, None).start_import(b);
        let (l, gen_c) = l.start_import(c);
        assert_ne!(gen_b, gen_c, "starting C must advance the fence");

        let stale = l.activate(b, gen_b);
        assert_eq!(
            stale,
            Err(LifecycleError::StaleCompletion {
                candidate: b,
                generation: gen_b,
                current: Some(gen_c)
            }),
            "B's late success must not install B over the in-flight C"
        );
        let stale_fail = l.fail(b, gen_b, ImportFailure::Write("boom".into()));
        assert!(
            matches!(stale_fail, Err(LifecycleError::StaleCompletion { .. })),
            "nor may B's late failure mark C as failed"
        );

        // Can-it-succeed half: C's own completion at its own generation works.
        let l = l.activate(c, gen_c).expect("C's completion is current");
        assert_eq!(l.active(), Some(c));
        assert_eq!(l.previous(), Some(a));
    }

    /// PROBE (Phase A). A completion with nothing in flight is refused rather
    /// than silently applied.
    #[test]
    fn a_completion_with_no_import_in_flight_is_refused() {
        let l = Lifecycle::serving(ImportId(0xA), None);
        assert_eq!(
            l.activate(ImportId(0xB), Generation(1)),
            Err(LifecycleError::NoImportInFlight {
                candidate: ImportId(0xB)
            })
        );
    }

    /// PROBE (Phase A). First create: nothing was active, so there is no
    /// previous to invent.
    #[test]
    fn the_first_activation_has_no_previous() {
        let (l, g) = Lifecycle::absent().start_import(ImportId(1));
        assert_eq!(
            l.active(),
            None,
            "nothing is served during the first import"
        );
        let l = l.activate(ImportId(1), g).unwrap();
        assert_eq!(l.active(), Some(ImportId(1)));
        assert_eq!(l.previous(), None);
    }

    // ── Activation gate ───────────────────────────────────────────────────

    #[test]
    fn an_ephemeral_candidate_waits_for_s3_and_a_volume_does_not() {
        let p = ActivationPolicy::default();
        assert_eq!(
            decide_activation(RootKind::Ephemeral, false, p),
            Activation::HoldForS3,
            "an un-mirrored ephemeral candidate would be re-converted on the next deploy"
        );
        assert_eq!(
            decide_activation(RootKind::Volume, false, p),
            Activation::ActivateS3Lagging,
            "a volume survives a redeploy, so it may run ahead of the mirror"
        );
        // Silence half: with S3 in hand both activate plainly.
        assert_eq!(
            decide_activation(RootKind::Ephemeral, true, p),
            Activation::Activate
        );
        assert_eq!(
            decide_activation(RootKind::Volume, true, p),
            Activation::Activate
        );
    }

    // ── Storage policy ────────────────────────────────────────────────────

    #[test]
    fn the_storage_policy_is_ordered_and_falls_back_to_ephemeral() {
        let full = RootInputs {
            osm_artifact_root: Some("/a".into()),
            osm_slab_cache_dir: Some("/b".into()),
            railway_vol: Some("/c".into()),
            volume01_writable: true,
            ephemeral_dir: PathBuf::from("/tmp/osm"),
            ..RootInputs::default()
        };
        assert_eq!(resolve_root(&full).path, PathBuf::from("/a"));

        let no_first = RootInputs {
            osm_artifact_root: None,
            ..full.clone()
        };
        assert_eq!(resolve_root(&no_first).path, PathBuf::from("/b"));

        let no_two = RootInputs {
            osm_slab_cache_dir: None,
            ..no_first.clone()
        };
        assert_eq!(resolve_root(&no_two).path, PathBuf::from("/c/osm"));

        let no_three = RootInputs {
            railway_vol: None,
            ..no_two.clone()
        };
        assert_eq!(resolve_root(&no_three).path, PathBuf::from("/volume01/osm"));

        let none = RootInputs {
            volume01_writable: false,
            ..no_three.clone()
        };
        let r = resolve_root(&none);
        assert_eq!(r.path, PathBuf::from("/tmp/osm"));
        assert_eq!(
            r.kind,
            RootKind::Ephemeral,
            "the fallback must be REPORTED as ephemeral"
        );

        // An empty variable is an absent one — a Railway row created and never
        // filled must not resolve to "".
        let blank = RootInputs {
            osm_artifact_root: Some("   ".into()),
            ..full.clone()
        };
        assert_eq!(resolve_root(&blank).path, PathBuf::from("/b"));

        // ...and so is a RELATIVE one: it would resolve against whatever
        // directory the server was launched from.
        let relative = RootInputs {
            osm_artifact_root: Some("osm-data".into()),
            ..full
        };
        assert_eq!(resolve_root(&relative).path, PathBuf::from("/b"));
    }

    /// PROBE (Phase A). A named path is not a durable path.
    ///
    /// `OSM_ARTIFACT_ROOT` and `OSM_SLAB_CACHE_DIR` routinely point at
    /// container-local scratch. Classifying them as `Volume` because someone
    /// set them would let an ephemeral candidate activate without S3 — and the
    /// next redeploy would re-run the whole SOA→Lance conversion, which is the
    /// exact cost this design exists to remove.
    #[test]
    fn a_generic_override_is_ephemeral_unless_durability_is_declared() {
        let undeclared = RootInputs {
            osm_artifact_root: Some("/scratch/osm".into()),
            ..RootInputs::default()
        };
        let r = resolve_root(&undeclared);
        assert_eq!(r.path, PathBuf::from("/scratch/osm"));
        assert_eq!(
            r.kind,
            RootKind::Ephemeral,
            "an undeclared override must default to ephemeral — the fail-safe direction"
        );
        assert_eq!(
            decide_activation(r.kind, false, ActivationPolicy::default()),
            Activation::HoldForS3,
            "and that classification must actually gate activation"
        );

        // Can-it-be-durable half: an explicit declaration is honoured.
        let declared = RootInputs {
            override_durability: Some(RootKind::Volume),
            ..undeclared
        };
        assert_eq!(resolve_root(&declared).kind, RootKind::Volume);

        // RAILWAY_VOL is intrinsically a volume — no declaration needed.
        let railway = RootInputs {
            railway_vol: Some("/data".into()),
            ..RootInputs::default()
        };
        let r = resolve_root(&railway);
        assert_eq!(r.path, PathBuf::from("/data/osm"));
        assert_eq!(r.kind, RootKind::Volume);
    }

    /// PROBE (Phase A). A defaulted `RootInputs` must not write the artifact
    /// tree into the process's current directory.
    #[test]
    fn the_default_root_is_absolute_and_never_the_current_directory() {
        let r = resolve_root(&RootInputs::default());
        assert!(
            r.path.is_absolute(),
            "the default resolved to {:?}, which resolves against the cwd",
            r.path
        );
        assert!(!r.path.as_os_str().is_empty());
        assert_eq!(r.kind, RootKind::Ephemeral);

        // A hand-built empty/relative fallback is corrected, not honoured.
        for bad in ["", "relative/osm"] {
            let inputs = RootInputs {
                ephemeral_dir: PathBuf::from(bad),
                ..RootInputs::default()
            };
            let r = resolve_root(&inputs);
            assert!(
                r.path.is_absolute(),
                "ephemeral_dir {bad:?} resolved to {:?}",
                r.path
            );
        }
    }

    // ── Publication order ─────────────────────────────────────────────────

    /// PROBE (Phase A). System falsifier still owed (Phase D): observe the
    /// real S3 write order under a killed process.
    #[test]
    fn the_pointer_is_published_last_and_present_objects_are_not_re_uploaded() {
        let already: BTreeSet<String> = ["frag/1".to_string()].into_iter().collect();
        let steps = plan_publication(
            ImportId(0xB),
            Some(ImportId(0xA)),
            &["frag/1".to_string(), "frag/2".to_string()],
            &already,
            &["import-seals/000000000000000b.yaml".to_string()],
        );

        assert!(
            matches!(steps.first(), Some(PublishStep::EnsurePreviousDurable(id)) if *id == ImportId(0xA))
        );
        assert!(
            matches!(steps.last(), Some(PublishStep::PublishPointer { .. })),
            "the head pointer must be the LAST step — a reader must never see it before the data"
        );
        match &steps[1] {
            PublishStep::UploadMissingObjects(m) => assert_eq!(
                m,
                &vec!["frag/2".to_string()],
                "a fragment already referenced in S3 must never be uploaded twice"
            ),
            other => panic!("expected the upload step, got {other:?}"),
        }
    }

    // ── Warm boot ─────────────────────────────────────────────────────────

    /// PROBE (Phase A). This proves the READ PLAN names no data object. It
    /// does **not** prove a real boot performs zero data traversal — the boot
    /// path does not exist yet.
    ///
    /// System falsifier still owed (Phase B/C): instrument the real warm-boot
    /// path and assert the bytes read are bounded by the seals' size.
    #[test]
    fn the_warm_boot_read_plan_names_only_small_seals() {
        let reads = warm_verification_reads(&lineage(), ImportId(0xB));
        assert_eq!(reads.len(), 3);
        for key in &reads {
            assert!(
                key.ends_with(".yaml"),
                "warm verification named {key}, which is not a seal — proving an origin \
                 must never require reading map data"
            );
            assert!(
                !key.contains("/data/") && !key.contains(".soa") && !key.contains(".lance/data"),
                "warm verification must not name a data object: {key}"
            );
        }
        assert!(reads.iter().any(|k| k.ends_with("origin-seal.yaml")));
        assert!(
            reads
                .iter()
                .any(|k| k.contains("import-seals/000000000000000b.yaml"))
        );
    }

    // ── Origin seal ───────────────────────────────────────────────────────

    /// PROBE (Phase A). The seal cannot contradict itself, and it identifies
    /// the import that made it — without `origin_import_id` the first sealed
    /// import is unfindable.
    #[test]
    fn an_origin_seal_is_internally_consistent_and_names_its_import() {
        let seal = OriginSeal::create(identity(), Some(v(1)));
        assert_eq!(
            seal.origin_import_id(),
            identity().import_id(),
            "the origin import id is DERIVED — it cannot disagree with the identity"
        );
        assert_eq!(
            seal.layout(),
            LAYOUT_V2,
            "the layout is read from the lineage; there is no second copy to drift"
        );
        assert_eq!(seal.initial_bake(), &bake());
        assert_eq!(seal.initial_manifest(), &manifest());
        assert_eq!(seal.lineage(), &lineage());

        // A later import in the same lineage is accepted; another lineage is not.
        let later = ImportIdentity::new(
            lineage(),
            SourceBakeId::parse("berlin-260901").unwrap(),
            manifest(),
            WriterAbi(1),
        );
        assert!(seal.accepts(&later), "a new bake stays in the same lineage");
        let v3 = ImportIdentity::new(
            LineageId::parse("berlin", LayoutAbi(3)).unwrap(),
            bake(),
            manifest(),
            WriterAbi(1),
        );
        assert!(
            !seal.accepts(&v3),
            "an incompatible layout is a new lineage"
        );
    }

    // ── Retention ─────────────────────────────────────────────────────────

    /// PROBE (Phase A). Retention keeps; it does not delete.
    ///
    /// The keep-set is the union of two **reachability closures** plus every
    /// seal. Deletion needs three gates open at once, and in Phase A the
    /// authority gate is shut by construction.
    #[test]
    fn retention_keeps_everything_reachable_and_authorises_no_deletion_in_phase_a() {
        let active = VersionInventory::complete(
            v(9),
            ["b/manifest", "b/frag1", "b/idx", "b/del1", "b/txn"]
                .into_iter()
                .map(String::from)
                .collect(),
        );
        let previous = VersionInventory::complete(
            v(7),
            ["a/manifest", "a/frag1"]
                .into_iter()
                .map(String::from)
                .collect(),
        );
        let seals: BTreeSet<String> = ["s/a.yaml", "s/b.yaml", "s/old.yaml"]
            .into_iter()
            .map(String::from)
            .collect();
        let everything: BTreeSet<String> = [
            "b/manifest",
            "b/frag1",
            "b/idx",
            "b/del1",
            "b/txn",
            "a/manifest",
            "a/frag1",
            "s/a.yaml",
            "s/b.yaml",
            "s/old.yaml",
            "ancient/1",
            "ancient/2",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // Phase A: grace elapsed, inventories complete — and STILL no deletion.
        let p = plan_retention(
            &active,
            Some(&previous),
            &seals,
            &everything,
            true,
            GcAuthority::None,
        );
        assert_eq!(
            p,
            RetentionPlan::KeepOnly {
                keep: p.keep().clone(),
                hold: RetentionHold::NoGcAuthority
            },
            "without a Lance-native reachability mechanism, `everything - keep` is a \
             guess dressed as a set operation"
        );
        assert!(p.collect().is_empty());
        assert!(
            p.keep().contains("b/del1"),
            "deletion files are reachable state"
        );
        assert!(p.keep().contains("b/idx"), "indices are reachable state");
        assert!(
            p.keep().contains("a/manifest"),
            "previous-clean is kept whole"
        );
        assert!(
            p.keep().contains("s/old.yaml"),
            "every seal is kept — a seal is the only record of what a version meant"
        );

        // Phase D shape: with authority AND a complete inventory AND the grace
        // period elapsed, only genuinely unreferenced objects are collectable.
        let d = plan_retention(
            &active,
            Some(&previous),
            &seals,
            &everything,
            true,
            GcAuthority::LanceNativeReachability,
        );
        assert_eq!(
            d.collect(),
            ["ancient/1".to_string(), "ancient/2".to_string()]
                .into_iter()
                .collect()
        );
        let remaining: BTreeSet<_> = everything.difference(&d.collect()).cloned().collect();
        assert_eq!(&remaining, d.keep(), "bounded: active + previous + seals");

        // Gate 1: the grace period must actually hold.
        assert_eq!(
            plan_retention(
                &active,
                Some(&previous),
                &seals,
                &everything,
                false,
                GcAuthority::LanceNativeReachability,
            ),
            RetentionPlan::KeepOnly {
                keep: d.keep().clone(),
                hold: RetentionHold::GracePeriodActive
            }
        );
    }

    /// PROBE (Phase A). The invariant that matters most: an incomplete
    /// inventory produces **no deletion plan at all**, even with full
    /// authority and the grace period long past. A partial closure that gets
    /// differenced against "everything" deletes exactly the objects it failed
    /// to enumerate.
    #[test]
    fn an_incomplete_inventory_produces_no_deletion_plan() {
        let everything: BTreeSet<String> = ["x", "y", "z"].into_iter().map(String::from).collect();
        let partial =
            VersionInventory::partial(v(9), ["x"].into_iter().map(String::from).collect());
        let complete =
            VersionInventory::complete(v(7), ["y"].into_iter().map(String::from).collect());

        for (a, p) in [(&partial, Some(&complete)), (&complete, Some(&partial))] {
            let plan = plan_retention(
                a,
                p,
                &BTreeSet::new(),
                &everything,
                true,
                GcAuthority::LanceNativeReachability,
            );
            assert!(
                matches!(
                    plan,
                    RetentionPlan::KeepOnly {
                        hold: RetentionHold::IncompleteInventory,
                        ..
                    }
                ),
                "an incomplete inventory must never authorise deletion, got {plan:?}"
            );
            assert!(plan.collect().is_empty());
        }

        // Can-it-ever-collect half, so the guard is not simply always-on.
        let ok = plan_retention(
            &complete,
            None,
            &BTreeSet::new(),
            &everything,
            true,
            GcAuthority::LanceNativeReachability,
        );
        assert_eq!(
            ok.collect(),
            ["x".to_string(), "z".to_string()].into_iter().collect()
        );
    }

    // ── Lineage identity ──────────────────────────────────────────────────

    #[test]
    fn an_incompatible_layout_is_a_different_lineage() {
        let v2 = LineageId::parse("berlin", LAYOUT_V2).unwrap();
        let v3 = LineageId::parse("berlin", LayoutAbi(3)).unwrap();
        assert_ne!(v2, v3);
        assert_eq!(v2.path_segment(), "berlin/osm-lance-v2");
        assert_eq!(v3.path_segment(), "berlin/osm-lance-v3");
        assert!(
            !v2.accepts_layout(LayoutAbi(3)),
            "a v3 dataset is not servable as v2"
        );
        assert!(v2.accepts_layout(LAYOUT_V2));

        // A new BAKE at the same ABI stays in the same lineage — the property
        // that stops every import minting a new S3 prefix.
        let a = ImportIdentity::new(
            v2.clone(),
            SourceBakeId::parse("aug").unwrap(),
            manifest(),
            WriterAbi(1),
        )
        .import_id();
        let b = ImportIdentity::new(
            v2.clone(),
            SourceBakeId::parse("sep").unwrap(),
            manifest(),
            WriterAbi(1),
        )
        .import_id();
        assert_ne!(a, b, "different bakes are different imports");
        assert_eq!(
            v2.path_segment(),
            "berlin/osm-lance-v2",
            "...in ONE lineage"
        );
    }

    /// Two regions never share a lineage even at the same ABI.
    #[test]
    fn regions_have_independent_lineages() {
        let berlin = LineageId::parse("berlin", LAYOUT_V2).unwrap();
        let bb = LineageId::parse("brandenburg", LAYOUT_V2).unwrap();
        assert_ne!(berlin.path_segment(), bb.path_segment());
        assert_ne!(
            ImportIdentity::new(berlin, bake(), manifest(), WriterAbi(1)).import_id(),
            ImportIdentity::new(bb, bake(), manifest(), WriterAbi(1)).import_id(),
            "the same bake into two regions is two imports"
        );
    }

    /// The writer ABI participates in the identity: the same source through an
    /// incompatible writer is a different import, not a reconcilable retry.
    #[test]
    fn the_writer_abi_participates_in_the_import_identity() {
        assert_ne!(
            ImportIdentity::new(lineage(), bake(), manifest(), WriterAbi(1)).import_id(),
            ImportIdentity::new(lineage(), bake(), manifest(), WriterAbi(2)).import_id(),
        );
    }
}
