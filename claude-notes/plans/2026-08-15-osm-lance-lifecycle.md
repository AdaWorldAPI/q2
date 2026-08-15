# OSM/Lance lifecycle — three clocks, one lineage per region

**Status:** Phase A implemented, corrected once, tests re-run. Phases B–F not
started (Phase B observational surface not begun this session either).
**Operator directive:** 2026-08-15, this session.
**Upstream contracts:** `AdaWorldAPI/lance-graph` PRs **#912** (baseline) and
**#913** (the five semantic gates). #911 is historical context and is
superseded wherever the contracts differ.
**Correction pass:** 2026-08-15, same session — nine structural corrections to
the first Phase-A draft (product-state lifecycle, validated identity types,
retention authority gating, head-behind handling, contract-probe labeling).
Recorded below in place; see § Correction pass for the full list. This is a
correction **within** the three-clock/one-lineage architecture, not a
redesign of it.

## The correction this plan exists to encode

A live Lance dataset is a **versioned lineage, not an immutable directory**. A
legitimate append, index build, compaction, reopen, or atime change must not
invalidate the imported OSM source. So:

- **Never** hash the whole Lance directory on every boot.
- **Never** compare its changing object inventory to one static checksum.

The current code does the moral equivalent — a full SHA-256 of every artifact
on every boot (even on a warm cache hit) plus a second full pass to hash the
slab for the dataset-freshness check. That is the measured cause of "memory
climbs back to ~4 GB after a restart with no map traffic".

## The three clocks — never conflated

| # | Clock | Fields |
|---|---|---|
| 1 | **Semantic import identity** | `source_bake_id`, `source_manifest_sha256`, `import_id`, `origin_seal`, `import_seal` |
| 2 | **Physical Lance history** | `publication_version`, `serving_version`, `observed_head` |
| 3 | **Replica progress** | storage root (volume/ephemeral), `mirrored_through_version`, S3 lag |

The failure mode this prevents, in lance-graph's own words (#913): retrying
cycle 1 while the head stood at V5 recorded cycle 1 as *"sealed into V5"*. A
later observed head is a **read horizon**, not evidence that the origin changed.

## Origin seal — internally non-contradictory (correction)

The first draft stored `OriginSeal.lineage: LineageId` (which already carries
`layout`) **and** a separate, independently-settable `OriginSeal.layout` — two
copies of the same fact with no constraint tying them together, so a
hand-built seal could disagree with itself. It also had no field naming the
import that *created* the lineage, which means the first sealed import was
unfindable from the seal alone.

Fixed by construction: `OriginSeal` fields are private; the only constructor
is `OriginSeal::create(identity: ImportIdentity, origin_publication)`. `layout`
is now an accessor that reads `identity.lineage.layout` — there is no second
storage location to drift. `origin_import_id` is *derived*
(`identity.import_id()`) and exposed as an accessor, so it can never disagree
with the identity that produced it, and it is now what makes the origin
import findable.

## Publication semantics (mirrored from lance-graph #913)

- Fresh successful import → `publication_version = Some(actual_version)`.
- Reconciled retry → `publication_version = None`, `observed_head = Some(head)`.
- Never assign a reconciled import's current head as its historical publication
  position.
- Recover an unknown historical position by **searching the sealed `import_id`
  in Lance history**, never by inference.

## One lineage per region

```
berlin/osm-lance-v2
brandenburg/osm-lance-v2
```

A new SOA bake inside `osm-lance-v2` is a **new sealed import version in the
same lineage**. Only an incompatible layout mints `osm-lance-v3`. No new S3
dataset prefix per import.

**Correction: the region is a validated `RegionId`, not an arbitrary
`String`.** It is interpolated into an S3 key and joined onto a filesystem
path, so `..`, `/`, and `\` would let a stray environment variable read a
different prefix or write outside the artifact root. `RegionId::parse`
enforces the same rule the existing slab hydrator already applies —
non-empty, `<= 64` bytes, lowercase `[a-z0-9-]` — as a *type*, so the
guarantee travels with the value instead of being re-checked (or forgotten)
at each new use site. `SourceBakeId` gets the equivalent treatment
(`[a-z0-9_.-]`, no `..`) since it is the kind of value that grows into staging
directory names.

## S3 key separation

```
.config/q2/osm/config.yaml                                    desired state
q2/lance/<region>/<lineage>/                                  the native lineage
q2/lance/<region>/<lineage>/origin-seal.yaml
q2/lance/<region>/<lineage>/import-seals/<import-id>.yaml
q2/lance/<region>/<lineage>/published.yaml                    pointer, written LAST
.state/q2/osm/<env>/<region>/status.yaml
.state/q2/osm/<env>/<region>/last-error.yaml
.runs/q2/osm/<env>/<region>/<run-id>/part-*.jsonl
```

Desired config carries **no mutable `imported: true/false`**. Completion is a
valid sealed import identity, not a flag.

## Hash choices — deliberate, not incidental

- **FNV-1a/64** stays the fast physical batch marker (`import_id` / batch hash),
  matching the existing Phase-A contract. Not a security property; an
  idempotency key.
- **SHA-256** stays for externally published SOA manifests and transport
  verification, held as **parsed 32 bytes** (`SourceManifestSha256`), not a
  string — so `AB12…` and `ab12…` derive the *same* import identity instead of
  two, which a text-hashed field would silently allow.
- **No BLAKE3 in this work.**
- **Correction: the marker narrows, the tuple decides.** Because FNV-1a/64 is
  64-bit and non-cryptographic, a marker match is a *candidate*, not a proof.
  `reconcile(candidate, found)` compares the full identity tuple
  `(lineage, bake, source_manifest_sha256, layout_abi, writer_abi)` after the
  marker matches, and returns `SameImport` / `HashConflict` /
  `DifferentImport`. A same-marker-different-tuple result is a **permanent**
  conflict (mirrors `lance-graph`'s `CommitError::HashConflict`) — reconciling
  it would silently adopt another import's data as this one's.

## Storage policy (ordered)

1. `OSM_ARTIFACT_ROOT`
2. `OSM_SLAB_CACHE_DIR` (existing)
3. `RAILWAY_VOL/osm`
4. `/volume01/osm` when present and writable
5. documented ephemeral fallback

The selected root records **`volume` or `ephemeral`** — that kind drives the
activation gate below.

**Correction: a named path is not a durable path.** The first draft classified
candidates 1 and 2 as `Volume` unconditionally, on the reasoning that "an
operator who names a root is asserting durability." That reasoning does not
hold: `OSM_ARTIFACT_ROOT` and `OSM_SLAB_CACHE_DIR` are generic overrides that
routinely point at container-local scratch, and misclassifying one as
`Volume` would let an ephemeral candidate activate without S3 — silently
reintroducing the exact cost (re-running the whole SOA→Lance conversion on
every redeploy) this design exists to remove. Only `RAILWAY_VOL` and a
probed-writable `/volume01` are *intrinsically* known to be volume storage.
Candidates 1 and 2 now take their kind from an explicit
`RootInputs::override_durability: Option<RootKind>`, defaulting to
`Ephemeral` when the operator hasn't said — the fail-safe direction, since
being wrongly cautious costs one S3 wait and being wrongly confident costs a
full re-import. Also fixed in the same pass: `RootInputs::default()` — which
`Default::derive` would leave as an empty `PathBuf` resolving against the
process's current directory — now resolves the ephemeral fallback to
`std::env::temp_dir()`, and `resolve_root` refuses a blank *or relative*
override path (both mean "different processes, different places"), falling
through to the next candidate instead.

## Activation gate

```yaml
activation:
  volume:
    require_s3_before_activation: false
  ephemeral:
    require_s3_before_activation: true
```

A persistent volume may activate a locally verified candidate while S3 catches
up, reporting `s3_lagging`. An **ephemeral** candidate is not fully successful
until S3 publication succeeds — otherwise the next redeploy repeats the whole
SOA→Lance conversion, which is the cost this design exists to remove.

## Update A → B, ordered

1. Build B in unique local staging.
2. **Reconcile `import_id` before creating another version.**
3. Commit B through one owned writer.
4. Validate B.
5. Ensure A is durable on S3 as `previous_clean`.
6. Upload only Lance objects **absent** from the S3 lineage.
7. Publish the S3 head pointer **last**, with generation/ETag protection.
8. Atomically activate B locally.
9. Mark A `previous_clean`.
10. Release Arrow buffers, writer/session state, source mappings, clean
    page-cache ranges.
11. GC older unreferenced versions only after a grace period.

**There is no compensating delete after an ambiguous publication.** Re-submit
the same frozen import identity and reconcile first. Orphaned unreferenced S3
objects are harmless and collected later.

## Retention

Keep active clean, previous clean, and **all** small seals. Never retain a full
independent dataset directory per import. Never upload a fragment already
referenced in S3.

**Correction: "keep" is a reachability closure, and destructive GC is
deferred, not merely gated by a grace period.** The first draft modeled
retention as `everything - keep` once a grace period elapsed. Two problems:

1. `keep` must be **everything reachable from a pinned Lance version** —
   manifest, fragments, indices, **deletion files**, transaction records,
   required metadata — not "the fragments I happened to upload." A partial
   inventory differenced against "everything" deletes exactly the objects it
   failed to enumerate, which is worse than not collecting at all.
2. No mechanism in Phase A–C can actually *prove* an object is unreachable
   from every retained version. `plan_retention` now takes a
   [`GcAuthority`]: `None` (Phases A–C — nothing may be deleted, full stop)
   or `LanceNativeReachability` (Phase D). `RetentionPlan` is
   `KeepOnly { keep, hold }` until all three gates open together — complete
   inventories, elapsed grace period, and `GcAuthority` — at which point it
   becomes `Collectable { keep, collect }`. **An incomplete inventory holds
   regardless of the other two gates**, tested directly.

Phase A therefore *specifies retention intent* (what must be kept, and why)
and produces **zero deletion plans**, by construction, until Phase D ships the
real reachability mechanism. That is a stronger statement than "the grace
period holds" — it is "there is no code path in Phase A–C that can return a
non-empty `collect` set."

**Honest limit, unchanged by the correction:** while the importer performs a
full `Overwrite`, active + previous clean still costs ~**two complete map
datasets**. Version metadata cannot deduplicate two entirely rewritten
fragment sets. Fixing that needs the Phase-E native-columnar Morton-shard
update — it is not a retention-policy problem and this plan does not pretend
otherwise.

## Serving state and import state are orthogonal (correction)

The first draft made `LifecycleState` a single sum type where
`ImportStarted` replaced `Active` outright — so a candidate import's failure
had nothing to restore *to*: the artifact being served had been absorbed into
the import's own state and vanished with it. Caught by
`a_failed_candidate_never_disturbs_the_active_artifact` failing.

The fix is a **product**, not a smarter sum:

- `Lifecycle.serving: Option<ServingSet { active, previous }>` — what this
  replica serves, full stop.
- `Lifecycle.operation: Operation::{Idle, Importing{candidate, generation},
  Failed{candidate, generation, error}}` — what an import is doing, orthogonal
  to serving.

`activate()`/`fail()` take `&self` and return `Result<Self, LifecycleError>`
— they never mutate `serving` except `activate()`'s own rotation, which is the
only place rotation happens. Both are **fenced by generation**: starting
candidate C advances a monotonic counter, and a late completion for a
superseded candidate B is rejected as `LifecycleError::StaleCompletion`
rather than activating or failing C. This is the concurrency case the earlier
draft had no way to express at all.

## Serving model

Replace the env-var/`OnceLock` activation with an `OsmArtifactManager` in
`AppState` holding an atomically replaceable `Arc` to the active dataset.
In-flight readers finish against the old `Arc`; new readers get the new one.
**The listener and global `/health` must not wait for hydration.**

`/api/osm/status` reports lifecycle state, lineage + origin seal, desired /
active / previous-clean import identities, publication / serving / observed
versions, replica state, S3 lag, import progress, seal verification, last
classified error, and the cgroup memory split — **without touching map data
pages**.

A checksum mismatch in an obsolete whole-directory inventory is **not**
corruption. A missing or unreadable object actually referenced by the selected
sealed version **is** a candidate failure → fall back to `previous_clean`;
never discard unrelated valid history.

## Head drift, and head-BEHIND (correction)

If the observed head is later than the known clean import:

- recognise known maintenance descendants when possible — and **say whether
  that recognition is verified or merely asserted** (`MaintenanceProvenance`;
  Phase A has no history walker, so it is always `CallerAsserted` today, named
  as a limitation rather than hidden);
- otherwise serve a **checkout of the last known clean sealed version**;
- report `head_drift`;
- never discard the lineage merely because the head advanced.

**A head *behind* the clean import is a different failure and was folded into
"drift" in the first draft — that conflation is wrong.** `observed_head <
clean` is not an unexplained later head; it means the local replica is
truncated or stale, or the requested clean version simply is not present here.
`decide_head` now returns `HeadDecision::ReplicaIncomplete { reason:
HeadBehindClean | CleanVersionAbsent }` for both cases, and — critically —
does **not** offer a checkout of a version the replica may not possess.
`recover_behind` decides separately: hydrate the clean version from S3 if it
has it, else serve the newest available clean version strictly below the
target (a real degradation, chosen only when S3 cannot help), else report
`Unserviceable`. Nothing is deleted and the lineage is not discarded in any of
these paths.

## Phases (PR-sized)

- **A.** Pure lifecycle types, state machine, manifests, storage resolution,
  falsifiers. ← *this session*
- **B.** `OsmArtifactManager`, fast listener startup, status endpoint, memory
  telemetry. ← *observational parts only, if cleanly separable*
- **C.** Versioned local import with origin/import seals and reconciliation.
- **D.** Incremental S3 lineage mirroring, publication pointer, recovery,
  retention.
- **E.** Native Lance column layout + Morton-shard updates; removes the single
  `FixedSizeBinary`/raw-offset contract and the Brandenburg row ceiling.
- **F.** UI status/banner wiring through q2 / a2ui.

## Falsifiers — contract probes (Phase A, shipped) vs. system falsifiers (still open)

**Correction: every item below was a decision-function test, and Phase A has
no running system for a decision to be *wrong about*.** The first draft
listed these as flatly "written before implementation" / done, without
distinguishing "this proves the decision function returns the right answer"
from "this proves the real boot/import/publish path obeys it." They are not
the same claim, and conflating them is how a green suite comes to certify
behaviour nobody measured. Each row below now names both halves; the second
column is **owed, not shipped**, and stays open until the phase that can
actually observe the real system lands it.

| Property | Phase-A contract probe | System falsifier (owed) |
|---|---|---|
| zero full-data traversal on warm boot | [x] `the_warm_boot_read_plan_names_only_small_seals` — the *plan* names no data object | [ ] Phase B/C: instrument the real boot path, assert bytes read are bounded by seal size |
| access residue does not change semantic identity | [x] `access_residue_does_not_change_semantic_identity` | [ ] Phase C: run a real import, compact the dataset, assert the seal on disk is unchanged |
| observed head later than publication | [x] `a_reconciled_retry_records_no_publication_version` | [ ] Phase C: assert against a real `Dataset::versions()` |
| reconciled retry creates no second version | [x] constructing `Reconciled` applies the rule | [ ] Phase C: assert `Dataset::versions()` gained no entry across a real reconciled retry |
| successful active→previous rotation | [x] `activation_rotates_active_to_previous` | [ ] Phase C: real activation, real filesystem/S3 state |
| failed candidate preserves active AND previous | [x] `a_failed_candidate_never_disturbs_active_or_previous` | [ ] Phase C: real failure path |
| stale completion cannot activate/fail a superseding candidate | [x] `a_stale_completion_cannot_activate_or_fail_the_superseding_candidate` (new — the concurrency case the first draft could not express) | [ ] Phase C: real concurrent imports |
| pointer-last S3 publication | [x] `the_pointer_is_published_last_and_present_objects_are_not_re_uploaded` | [ ] Phase D: observe real S3 write order under a killed process |
| ephemeral S3 activation gate | [x] `an_ephemeral_candidate_waits_for_s3_and_a_volume_does_not` | [ ] Phase D: real S3 state |
| generic override is ephemeral unless durability is declared | [x] `a_generic_override_is_ephemeral_unless_durability_is_declared` (new) | [ ] Phase B: real env-var wiring |
| retention keeps active/previous/seals; authorises no deletion in A–C | [x] `retention_keeps_everything_reachable_and_authorises_no_deletion_in_phase_a` | [ ] Phase D: real Lance-native reachability mechanism |
| incomplete inventory ⇒ no deletion plan, ever | [x] `an_incomplete_inventory_produces_no_deletion_plan` (new) | [ ] Phase D |
| bounded active+previous storage after cleanup | [x] proven inside the retention test above | [ ] Phase D |
| unknown-head checkout of last clean | [x] `an_unexplained_later_head_serves_the_clean_checkout_and_reports_drift` | [ ] Phase C: real Lance history |
| head behind clean is a replica problem, not drift (new) | [x] `a_head_behind_clean_is_a_replica_problem_not_a_drift`, `an_absent_clean_version_is_never_offered_as_a_checkout` | [ ] Phase D: real recovery path |
| recovery hydrates before degrading (new) | [x] `recovery_hydrates_when_it_can_and_degrades_only_when_it_must` | [ ] Phase D |
| layout incompatibility creates a new lineage | [x] `an_incompatible_layout_is_a_different_lineage` | — (pure identity property, no system half owed) |
| first-create ambiguity | [x] `the_first_activation_has_no_previous`; `an_ambiguous_publication_seals_nothing` proves `Ambiguous ⇒ None` | [ ] Phase C/D: kill the process between commit and ack on a first create, restart, re-submit, assert exactly one version — `Ambiguous ⇒ None` is NOT this falsifier by itself |
| telemetry failure does not affect serving | — not attempted this session | [ ] Phase B: no telemetry sink exists yet; open |
| a marker match is a candidate, the tuple decides (new) | [x] `a_marker_match_with_a_different_tuple_is_a_conflict_not_a_reconcile` | [ ] Phase C: a real FNV collision (or a forced one) against real history |
| digest casing cannot change identity (new) | [x] `digest_casing_cannot_change_the_import_identity` | — (pure identity property) |
| region/bake id reject path traversal (new) | [x] `region_rejects_everything_that_could_escape_a_prefix_or_a_directory`, `bake_id_rejects_traversal_and_separators` | [ ] Phase C/D: a real S3 key built from a rejected value must never be attempted |
| origin seal is internally non-contradictory (new) | [x] `an_origin_seal_is_internally_consistent_and_names_its_import` | — (pure identity property) |
| default root is never the current directory (new) | [x] `the_default_root_is_absolute_and_never_the_current_directory` | — (pure identity property) |

## Work items

### Phase A
- [x] `RegionId`, `SourceBakeId`, `SourceManifestSha256` — validated identity
      primitives (path-traversal-safe, casing-invariant)
- [x] `LineageId`, `ImportIdentity`, `OriginSeal` (internally consistent —
      `layout` and `origin_import_id` are derived, not duplicated), `ImportSeal`
- [x] `ImportId` (FNV-1a/64, deterministic) + `reconcile` (marker narrows,
      tuple decides; `HashConflict` is permanent)
- [x] `PublicationOutcome` mirroring lance-graph `CommitOutcome`
- [x] `Lifecycle` — **product** of `serving: Option<ServingSet>` and
      `operation: Operation`, fenced by `Generation`; `LifecycleError` for
      stale completions and completions with nothing in flight
- [x] `HeadDecision` — drift AND head-behind (`ReplicaIncomplete`), with
      `recover_behind` as the separate recovery decision
- [x] `KnownMaintenance` with `MaintenanceProvenance` (asserted vs verified)
- [x] `StorageRoot` resolution (5 ordered candidates; only `RAILWAY_VOL` and a
      probed `/volume01` are intrinsically `Volume`; candidates 1–2 need an
      explicit `override_durability` or default to `Ephemeral`)
- [x] `ActivationPolicy` gate
- [x] `plan_publication` (ordered S3 steps, pointer last)
- [x] `VersionInventory` + `GcAuthority` + `RetentionPlan` (`KeepOnly` in
      Phases A–C by construction; `Collectable` only in Phase D)
- [x] `warm_verification_reads` (small seals only)
- [x] contract probes for all of the above (see table)
- [x] **correction pass** (2026-08-15, same session) — see § Correction pass;
      re-ran the full `osm_lifecycle` test module after

### Phase B (observational only this session — not started)
- [ ] `OsmArtifactManager` with atomically replaceable `Arc`
- [ ] `/api/osm/status`
- [ ] cgroup memory split + phase deltas
- [ ] listener/`/health` independent of hydration
- [ ] a real `MaintenanceProvenance::LanceHistoryVerified` resolver, OR name it
      as still deferred to Phase C at the status-endpoint level

### Deferred, named
- [ ] braid epic — **the `braid` CLI is not installed in this container**; the
      work items above are the importable form.

## Correction pass (2026-08-15, same session)

An external review of the first Phase-A draft (relayed via the operator,
explicitly flagged as possibly-stale) found nine structural issues. All nine
were within the three-clock/one-lineage architecture — none required
reopening the architecture itself:

1. Serving state and import-operation state must be orthogonal — see
   § Serving state and import state are orthogonal.
2. Not every explicitly-configured path is a volume — see § Storage policy.
3. The region/bake path-safety boundary needed a validated type, not a
   re-checked `String` — see § One lineage per region.
4. Identity inputs (the digest especially) needed to be parsed/canonical, not
   stringly-typed — see § Hash choices.
5. `OriginSeal` had two independently-settable copies of `layout` and no
   `origin_import_id` — see § Origin seal.
6. Retention was modeled as permission to delete `everything - keep` — see
   § Retention.
7. Every "falsifier" was a decision-function test, not a system falsifier —
   see the table above.
8. `observed_head < clean` was folded into "drift" instead of being its own
   failure mode — see § Head drift, and head-BEHIND.
9. `known_maintenance` needed to carry *provenance*, not just membership —
   see `KnownMaintenance`/`MaintenanceProvenance` above; the real resolver is
   still deferred, named rather than hidden.

All nine are implemented in `crates/cockpit-server/src/osm_lifecycle.rs` and
covered by contract probes (module rewritten in full; the module doc's
"What the tests here do and do not prove" section states the probe/falsifier
distinction inline, not only in this plan). The currently-failing test named
in the review request no longer exists in the corrected model — the failure
mode it targeted (`ImportStarted` clobbering `Active`) is now structurally
unreachable because `Operation` cannot touch `serving` at all.

## Notes

- `DatasetVersion` is reused from `lance_graph_contract::scheduler`, not
  re-declared — the same type lance-graph's contracts use.
- Phase A is **pure**: no I/O, no async, no Lance calls. That is what makes
  "pointer last" and "zero traversal on warm boot" testable as decisions
  before any wiring exists — and also why every probe in this file proves a
  decision, never a running system; see the falsifiers table.
