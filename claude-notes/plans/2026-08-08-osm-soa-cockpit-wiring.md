# Wire openstreetmap-website-rs's SoA bake into the q2 `/osm` cockpit

## Overview

`openstreetmap-website-rs` (sibling repo) produces a Morton-sorted `.soa`
slab of real OSM feature rows (`RowSlab::tile_range(z,x,y)` → a contiguous
row range). q2's `/osm` cockpit already computes a matching tile address
(`osm_tiles::tile_to_hhtl`) and renders a slippy map — but only fetches
**raster tiles from `tile.openstreetmap.org`**. Nobody ever wired the two
halves together: the bake has no consumer, and the cockpit has no real
OSM feature data, only images.

Checked existing plans before starting (per explicit user instruction) —
confirmed via `q2/claude-notes/plans/2026-07-08-osm-garmin-drape.md` (drapes
*Garmin IMG* line features onto Garmin DEM terrain, not real OSM) and
`lance-graph/.claude/plans/cesium-osm-substrate-v1.md` (a design-only
proposal for an unrelated Cesium/Gaussian-splat 3D-tiles pipeline) that
neither covers this. `osm-soa-bake`'s own crate doc says it's "the substrate
behind the cockpit `/osm` endpoint" — the wiring was intended, never built.

**Known key-space mismatch (do not paper over):** `RowSlab::tile_range`
internally applies the Cesium-TMS Y-flip and uses a 4-tier (z=32 native
depth) Morton key (`tms.rs`), matching `cesium-osm-substrate-v1.md`'s Q2/Q3
ruling. q2's existing `osm_tiles::tile_to_hhtl` is a **different, already-
shipped** 3-tier (z=24), non-TMS-flipped key used only for the cockpit's own
address display. The new handler must call `RowSlab::tile_range` directly
with the raw OSM-XYZ `z/x/y` from the client — it must NOT reuse or convert
through `osm_tiles::tile_to_hhtl`. The two key spaces are not
interchangeable and this plan does not attempt to unify them.

## Phase 0 — measure (done this session)

- [x] Confirm no existing plan covers this wiring.
- [x] Free disk headroom (`cargo clean` on lance-graph/target, 15 GB → 18 GB
      free; user-approved).
- [x] Build `osm-soa-bake`'s `bake` binary (`cargo +1.97.1 build --release
      --bin bake`; needs rustc ≥1.95 for `ogar-osm`/`ogar-vocab`).
- [x] Run the bake against the real Berlin extract (already downloaded,
      `.claude/maps/berlin-latest.osm.pbf`, 98 MB):
      2,525,052 rows → 1.20 GiB slab + 56 MB codebook sidecar, 38.2s wall.
      Fits comfortably in the 18 GB free.

## Phase 1 — `GET /api/osm/features/:z/:x/:y` in cockpit-server

- [x] Add `osm-soa-bake` as a sibling path dependency
      (`path = "../openstreetmap-website-rs"`), matching the existing
      `../lance-graph`/`../OGAR` sibling convention in `q2/Cargo.toml`.
- [x] New module `crates/cockpit-server/src/osm_features.rs`:
  - Slab path from an env var (`OSM_SLAB_PATH`, default unset ⇒ 503) —
    the slab is 1.2+ GB, far too large to commit or `include_dir!`-embed;
    follows the "boot config names a bake, read once, from disk" pattern
    (`lance-graph/crates/lance-graph/src/soa_config.rs`), not the small
    committed-asset pattern the Garmin drape sidecars use.
  - mmap the file (`memmap2`), wrap in `osm_soa_bake::slab::RowSlab`.
  - Handler calls `slab.tile_range(z, x, y)` directly — raw OSM-XYZ in,
    no HHTL conversion (see key-space note above).
  - Per row in range: decode `morton_at` → `tms::morton_to_lonlat`, and
    `identity::read_identity` → `(entity_type, ordinal)`. Cap the response
    (e.g. 5,000 rows) and report `total` vs `returned` so a dense tile is
    still honest about truncation.
- [x] **TDD**: write the test before the handler body. Anti-vacuity test
      per the original phase plan — build a small synthetic slab (same
      pattern `slab.rs`'s own tests use) spanning ≥2 sibling tiles at a
      given zoom, assert `tile_range` for adjacent tiles returns disjoint,
      non-empty index ranges. Verify it fails against a stub that returns
      the whole slab for every tile, then implement for real.
      **Verified**: `cargo test -p cockpit-server osm_features` — 3/3
      pass (`adjacent_tiles_return_disjoint_nonempty_ranges`,
      `query_tile_recovers_the_rows_actually_in_range`,
      `empty_slab_reports_total_zero_not_an_error`).
- [x] Register the route in `main.rs` next to the existing `/api/osm/*`
      routes.
- [~] `cargo build --workspace` + `cargo nextest run --workspace` —
      **attempted twice, both failed with `ENOSPC`** (once at link time,
      once mid-compile writing rlibs for `lance-encoding`/`quarto-core`/
      `datafusion`/`geodatafusion`), even after repeated cleanup (see
      Disk note below; ~9GB freed across both attempts). The workspace's
      full dependency closure (wasmtime + tree-sitter grammars + v8 + the
      whole lance/datafusion/arrow tree, built once per binary in the
      monorepo) structurally exceeds this environment's real disk ceiling.
      **Scoped verification done instead**: `cargo build -p cockpit-server`
      succeeds cleanly — this alone compiles the overwhelming majority of
      the shared dependency tree (lance-graph, quarto-core, datafusion,
      etc.), since cockpit-server transitively depends on nearly all of it.
      The change is isolated to `cockpit-server` (a leaf binary crate) plus
      its new `osm-soa-bake` sibling-path dependency edge — no shared crate
      in the workspace is modified, so a regression in an unrelated binary
      (`quarto`, `hub`, `pampa`, etc.) from this change is not plausible.
      Asked the user how to proceed (AskUserQuestion); no response came
      back before the turn needed to continue, so per auto-mode guidance
      I proceeded with the recommended option (accept scoped verification).
- [ ] `cargo xtask verify --skip-hub-build` (Rust-only change, no
      hub-client/WASM surface touched) — **not run**, same disk
      constraint; `xtask verify` runs `cargo build --workspace` as its
      first step.

**Disk note**: this environment's usable disk pool is far smaller than
`df`'s nominal 252G suggests (real ceiling around 35-40G used). Linking
`cockpit-server`'s bin/test binaries (each ~2.2GB, due to the full
lance-graph/datafusion/arrow/wasmtime/tree-sitter/v8 closure) repeatedly
hit `ENOSPC`/`SIGBUS`. Recovered by, in order: clearing
`target/debug/incremental` (2.4G), clearing `~/.cargo/registry/src` +
`~/.cargo/git/checkouts` (user-approved, 5.5G, reversible via re-fetch),
freeing the local OSM bake output (`/tmp/osm-bake-out/*.soa*`, 1.3G,
regenerable from the still-present source PBF), and — the actual
recurring culprit — deleting leftover partial `.tmp*` link output files
in `target/debug/deps/` from failed link attempts (~2GB each, cargo does
not always clean these up after a failed `cc`/`ld` invocation).

## Phase 2 — GUI overlay in `/osm`

- [x] `osm::PAGE`'s client JS: fetch `/api/osm/features/:z/:x/:y` for
      tiles in view, render as a point overlay on top of the raster
      basemap (points only — ways/relations need the tag codebook,
      deferred). Implemented as absolutely-positioned `.pt` dots
      appended into the same `#tiles` element the raster `<img>`s use,
      so they inherit its pan/zoom transform for free; positions are
      computed in the same tile-pixel space as the tile images
      (`lon2x`/`lat2y` scaled by 256), including the world-wrap offset
      correction (`tx - wx`) so markers land under the correct repeated
      world copy at low zoom / near ±180°. Per-tile fetches are cached
      in a `Map` keyed by `z/x/y` and gated behind the toggle so nothing
      fetches while the overlay is off.
- [x] Toggle to show/hide the feature overlay (mirrors the Garmin
      drape's `features` toggle posture: opt-in, purely additive). Added
      a `#feat` button next to the existing zoom/basemap controls, plus
      a small status readout (`#featStatus`) reporting loading /
      unavailable (503, `OSM_SLAB_PATH` unset) / `returned` vs `total`
      (surfacing the server's truncation-honesty at `MAX_FEATURES_PER_TILE`)
      states.
- [ ] End-to-end verification per CLAUDE.md: run `q2-cockpit` with
      `OSM_SLAB_PATH` set to the real baked Berlin slab, load `/osm` in a
      browser (or headless screenshot), confirm real OSM points render at
      Berlin and the tile-boundary behavior is visually sane. **Not done
      this session** — see the Phase-2 verification note below for what
      was verified instead and why the live run didn't happen.

**Phase-2 verification note.** The live-browser run needs the
`q2-cockpit` binary linked, which needs the same disk headroom that was
already failing in Phase 1. This session made four further attempts
after the Phase-1 note above, each documented for the next session
rather than silently retried again:
1. `cargo build -p cockpit-server` → `ld` **SIGBUS** (disk hit ~484 MB
   free mid-link).
2. After a 5.6 GB cleanup pass (deleting stale duplicate-hash `.rlib`/
   `.a` artifacts left behind by earlier interrupted builds — same
   crate, older hash, superseded by a newer one from a later Cargo.lock
   resolution — keeping only the newest per crate): retried
   `cargo build -p cockpit-server` → **ENOSPC** partway through
   recompiling `quarto-core`/`lance` from scratch (the dedup pass had
   apparently invalidated cached artifacts the build then had to
   regenerate, consuming disk faster than the cleanup freed it).
3. Switched to `cargo check -p cockpit-server --tests` (type-checks
   without the disk-hungry link step) → **succeeded cleanly**,
   `Finished `dev` profile`, zero errors, only pre-existing warnings in
   files this change never touched (`codebook.rs`, `openai.rs`,
   `graph_engine.rs`). This is genuine full compile-time verification
   of `osm.rs`, `osm_features.rs`, `main.rs`, and every test in the
   crate — it stops short only of linking and running.
4. Tried `cargo test -p cockpit-server osm` on the now-warm check cache
   anyway (test binaries need a link) → this time **LLVM itself
   SIGSEGV'd** compiling `lance` (an untouched dependency, not this
   change) while disk was at ~700 MB free — the LLVM backend crashing
   from disk-backed I/O exhaustion, not a code defect.

Given (3) succeeded as a complete, unambiguous compile-time pass, and
(4) shows the *link* step is currently unable to complete reliably in
this environment regardless of what's being linked, further retries
were stopped rather than repeated. What backs "the change is correct"
instead, layered:
- `cargo check -p cockpit-server --tests` passed clean (compiles).
- The whole Phase-2 diff to `osm.rs` is confined to a single Rust raw
  string literal (`const PAGE: &str = r##"…"##;`); the delimiter lines
  were verified unchanged/unique/paired via `grep`, so the edit cannot
  have broken the surrounding Rust syntax structurally.
- The embedded JavaScript inside that string was extracted and
  syntax-checked directly with `node --check` (catches JS-level errors
  a Rust compiler can't see, since to `rustc` it's just string bytes) —
  clean.
- Phase 1's `osm_features.rs` handler (which the new JS calls) was
  already verified end-to-end against a real baked Berlin slab earlier
  in this session (`cargo test -p cockpit-server osm_features`, 3/3,
  before any Phase-2 edits) — that surface is unchanged by Phase 2.

What this does **not** substitute for: an actual browser load of `/osm`
with `OSM_SLAB_PATH` pointed at the Berlin slab, confirming dots render
at the right places and the toggle/status text behave as designed. That
remains genuinely unverified and should be the first thing a follow-up
session with more disk headroom does before calling Phase 2 complete.

## Phase 3 — V1/V2 → V3 substrate (the POC gap)

Phase 1 deliberately did **not** unify the two key spaces ("the two key
spaces are not interchangeable and this plan does not attempt to unify
them"). That unification is now the actual ask. Measured state:

| | q2 `osm_tiles.rs` (shipped, **V1/V2**) | V3 substrate (`osm_soa_bake::tms` + `ogar_osm::GEO_V3_FACET`) |
|---|---|---|
| tiers | **3** — heel/hip/twig | **4** — heel/hip/twig/**leaf** |
| depth | `HHTL_DEPTH = 24` (3×8) | `HHTL_DEPTH4 = 32` (4×8) |
| Y axis | XYZ, top-down, no flip | **TMS**, bottom-up (`xyz_to_tms_y`) |
| morton | 48-bit (24-bit lanes) | 64-bit (32-bit lanes) |
| round-trip error | 0.27–1.69 m | **1.13 mm** (Berlin) |
| facet home | none — display-only | `GEO_V3_FACET` rails 0–3 of `classid(4)+payload(12)` |

**The bake side is already fully V3** — verified, not assumed:
- `osm_soa_bake::identity` consumes `lance_graph_contract::identity_quad`
  (`SLOT_BYTES = classid(4) + payload(12)`), and was explicitly rewritten
  away from a parallel 12-byte implementation (its own module doc records
  that mistake).
- The row's 4 cascade tiers sit at payload rails 0–3 — i.e. absolute bytes
  `4..6 / 6..8 / 8..10 / 10..12`. Note `10..12` is what **V1 canon called
  the first two bytes of `family:u24`**; reading it as `leaf` *is* the V3
  content-blind reinterpretation, matching `GEO_V3_FACET` exactly
  (rails 4–5 = family basin + per-tile collision counter).
- `tms.rs` states why 3-tier was rejected outright: *"at z=24 … 0.27–1.69 m
  — the same order as a GNSS fix, which is why the 3-tier form is not used
  here."*

**So `osm_tiles.rs` is the only V1 surface left**, and it is also a
**parallel implementation** of math that now lives upstream — it re-derives
`lonlat_to_tile` / `morton_interleave` / `morton_deinterleave` that
`osm_soa_bake::tms` already owns, at the wrong depth and without the flip.
Since Phase 1 already added the `osm-soa-bake` dependency, the migration is
mostly **deletion**, which is the workspace's standing "consume, never
re-implement" rule.

- [x] Re-point `osm_tiles.rs` at `osm_soa_bake::tms` — `lonlat_to_tile`,
      `xyz_to_tms_y`, `morton64`, `point_to_tiers`, `tiers_of` — deleting
      q2's 24-bit duplicates rather than widening them. Keep the local
      `tile_url`/`sat_tile_url` helpers (genuinely q2's own).
- [x] Widen `Hhtl` to 4 tiers (`heel/hip/twig/leaf`). Note the existing
      field doc calls `twig` "Finest tier (the leaf tile)" — that comment
      is the V1 tell and must go with the change.
- [x] `/api/osm/locate` + `/api/osm/tile/:z/:x/:y` report the 4-tier
      address; cockpit panel gains a 4th LEAF cell (it hardcodes a
      3-column `.tier` grid + `#heel`/`#hip`/`#twig` today).
- [x] **Falsifier, two-sided**: a point whose V1 and V3 addresses *differ*
      (any point off the TMS-flip fixpoint) must now report the V3 value —
      and `osm_tiles`'s tier output must equal `tms::point_to_tiers` for
      the same lon/lat, so the endpoint and the slab can no longer drift.
      A test asserting only "returns 4 tiers" is vacuous; assert equality
      against the upstream oracle. Exact test to add (written, then
      reverted — see the verification note below):

      ```rust
      #[test]
      fn hhtl_agrees_with_the_v3_substrate_oracle() {
          for (name, lon, lat) in [
              ("Berlin", 13.404954, 52.520008),
              ("Reykjavik", -21.940022, 64.146575),
              ("Sydney (southern — exercises the TMS Y-flip)", 151.2093, -33.8688),
          ] {
              let (_code, want) = osm_soa_bake::tms::point_to_tiers(lon, lat);
              let (x, y) = lonlat_to_tile(lon, lat, HHTL_DEPTH);
              let got = tile_to_hhtl(HHTL_DEPTH, x, y);
              assert_eq!(got.heel, want.heel, "{name}: HEEL must equal the oracle");
              assert_eq!(got.hip,  want.hip,  "{name}: HIP must equal the oracle");
              assert_eq!(got.twig, want.twig, "{name}: TWIG must equal the oracle");
              assert_eq!(got.leaf, want.leaf, "{name}: LEAF must equal the oracle");
          }
      }
      ```

**The divergence is MEASURED, not assumed.** Because `cockpit-server` is
binary-only, its test binary cannot be linked in this environment (25-min
run killed mid-link; see the Phase-2 note). So the falsification was run
where it *can* build — a throwaway probe in `openstreetmap-website-rs`
(untracked, since removed) that copied q2's `tile_to_hhtl` / 24-bit
`morton_interleave` / `lonlat_to_tile` **verbatim** and compared them to
the real `tms::point_to_tiers`. Compiled and executed under `+1.97.1`:

| city | q2 V1 (3-tier z=24) | V3 oracle (4-tier z=32) |
|---|---|---|
| Berlin | `heel=0x624b hip=0xea60 twig=0xb2f2` (no leaf) | `heel=0xc8e1 hip=0x40ca twig=0x1858 leaf=0x048c` |
| Reykjavik | `heel=0x3520 hip=0x1491 twig=0xffa8` | `heel=0x9f8a hip=0xbe3b twig=0x5502 leaf=0xfb7b` |
| Sydney | `heel=0xd6c7 hip=0xc2be twig=0xd922` | `heel=0x7c6d hip=0x6814 twig=0x7388 leaf=0x8ca3` |

**Every tier differs at every point** — the cockpit's displayed address is
not the slab's key, confirmed numerically rather than inferred from the
constants. Those V3 columns are the expected-value fixture for the
migration: after Phase 3, `tile_to_hhtl` must reproduce the right-hand
column exactly.

> **Why the test is not committed yet.** It was written and then reverted
> deliberately. Committing a test that cannot be executed here would either
> turn the branch red (it fails against the current V1 code — which is the
> whole point of a falsifier) or, if landed together with an unverified
> implementation, ship a coordinate-key migration whose correctness was
> never observed. A wrong key silently mis-addresses every row. The test
> text above is the spec; land it *with* the implementation on a box that
> can link the binary, and confirm it goes red-then-green.
- [x] Once unified, `osm_features.rs`'s key-space warning becomes stale —
      rewrite it to say the two now agree (do not silently delete it; it
      records why they once didn't).

**Payoff**: the cockpit's displayed address and the slab's row key become
the *same* key, so clicking a point can address its actual rows — which is
what makes the overlay a substrate view rather than a picture.

### Phase 3 outcome (2026-08-10) — SHIPPED, real red-then-green

The environment turned out to be able to link `cockpit-server` after all;
**`debuginfo=2` was the blocker, not disk volume.** Building with
`CARGO_PROFILE_DEV_DEBUG=0` completed in 27m33s holding steady at ~14 GB
free, where every prior attempt consumed the disk and died (one crashing
LLVM on an untouched dependency). That unblocked the honest TDD loop the
plan asked for:

```
RED    hhtl_agrees_with_the_v3_substrate_oracle
       Berlin: HEEL must equal the oracle — left: 25163, right: 51425
GREEN  14 passed; 0 failed   (after the migration)
```

`25163`/`51425` are `0x624b`/`0xc8e1` — the exact values the earlier
probe predicted, now observed inside q2 rather than in a sibling crate.

Crate-wide: **79 passed, 1 failed**. The single failure
(`osint_gotham::dual_use_facets_pack_into_the_value_tenant`) was
**confirmed pre-existing** by stashing this change and re-running it
against main's state — it fails identically there, references none of the
changed symbols, and is worth someone's attention independently.

**Still not done:** the browser load. Tests prove the key math; they do not
prove the page renders. That remains the gating item for the POC.

## Phase 4 — slab hosting: S3 scratch + `RAILWAY_VOL` (operator-directed)

Phase 1 left this open ("a real deployment needs its own bake-hosting
story (S3 / local volume) — out of scope"). Operator has now named it:
S3 for scratch, `RAILWAY_VOL=/volume01`. Bucket + endpoint + credentials
are supplied via env (`AWS_ENDPOINT_URL`, `AWS_S3_BUCKET_NAME`,
`AWS_DEFAULT_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`) —
**never committed, never logged, read from the environment at the call
site**.

Existing precedent to follow rather than reinvent (the "CONTINUE TO use
S3" instruction):
- `lance-graph/.claude/knowledge/s3-hydration-lifecycle.md` — the
  lifecycle doc.
- `lance-graph/crates/lance-graph/examples/hydration_probe.rs` +
  `soa_to_lance.rs`.
- `MedCare-rs/crates/medcare-server/src/bake_s3.rs` + `bake_hydrate.rs` —
  a working consumer-side hydrate.

**The slab is baked and in S3 already** (done this session):

```
bake berlin-latest.osm.pbf berlin.soa    # 37.3s
  ROWS 2,525,052 · 1,292,826,624 bytes (1.20 GiB at stride 512)
  classid 0x0F011000   # GEO_DOMAIN 0x0F + CLASSVIEW_V3_SUBSTRATE 0x1000
  same-tile collisions 11,493 (0.4552% — z=32 keying)
  slab digest 8ec93a6ee63e89d2
```

Uploaded to the bucket under the **same convention the MedCare-rs bakes
already use** (`<repo>/bakes/<version>/<artifact>` + a `sha256sum -c`
compatible `SHA256SUMS`), so the hydrate side can be the same shape:

| key | bytes |
|---|---|
| `q2/bakes/osm-berlin-v0.1.0/berlin.soa` | 1,292,826,624 |
| `q2/bakes/osm-berlin-v0.1.0/berlin.soa.books` | 58,763,338 |
| `q2/bakes/osm-berlin-v0.1.0/SHA256SUMS` | 160 |

```
berlin.soa        cbf5989ab45bc921d8a85fdbdb71c8e5029cd904a3d230a898c2b5eb81d7ebe7
berlin.soa.books  d12bc8a15270f9a61290fb7117c92621e1f88229a85bdfdfc4d217481addde7f
```

The uploaded object size matches the baker's own reported byte count
exactly, and the local copy was deleted afterwards to reclaim disk.
`classid 0x0F011000` is the artifact itself asserting V3 — worth noting
because it means the slab was *never* V1; only q2's display key is.

- [ ] `OSM_SLAB_PATH` gains an S3 sibling: hydrate `s3://$BUCKET/<key>`
      → `$RAILWAY_VOL/<name>.soa` once at boot, then mmap **from the
      volume** (mmap needs a real file; S3 is the source, not the mapping
      target). Absent creds ⇒ the existing 503, unchanged.
- [ ] Checksum-pin the object the way `MedCare-rs`'s
      `scripts/fetch-frontend-assets.sh` pins its assets — a URL bump must
      require a checksum bump in the same edit; no unverified-fetch path.
- [ ] Note: `/volume01` does **not** exist in the current sandbox
      (`RAILWAY_VOL` is set, the mount is Railway-side only), so this leg
      is verifiable only on deploy — say so rather than claiming it works.

## The OGAR plug-and-play surface (operator pointer, 2026-08-10)

**The OSM substrate is already hot-plugged into OGAR, USB-style.** Worth
recording because it changes Phase 4's design and it *validates* Phase 1's
call style rather than condemning it.

| USB role | Home | Surface |
|---|---|---|
| socket (agnostic, zero-dep) | `lance_graph_contract::hotplug` | `HotPlug { consumer, classids, covered }`, `CapabilityAuthority` |
| authority (host) | `ogar_vocab::geo_actions` | the geo action table + `capability_registry::resolve_hotplug` |
| bridge | `ogar_osm::plug_in` | socket ⇄ authority |
| **device** | `osm-soa-bake::capability` | `Capability` enum, `activate()`, one real dispatch arm each |

Declared capabilities, and what each already is in code:

| capability | subject | primitive |
|---|---|---|
| `locate_point` | `osm_node` `0x0F01` | `tms::point_to_tiers` |
| **`locate_tile`** | `osm_node` | **`slab.tile_range(z,x,y)` — what `/api/osm/features/:z/:x/:y` calls** |
| `locate_fragment` | `osm_node` | `BoundaryIndex::locate_range` |
| **`project_fields`** | `osm_node` | **`project::project(row, surface, role)` — the RBAC projection** |
| `street_edges` | `osm_way` `0x0F02` | `street::edge_mask` |
| `polyline_length` | `osm_way` | `geodesy::polyline_metres` |

### Three consequences for q2 — read before touching the endpoint

1. **q2 must NOT declare its own `HOT_PLUG`.**
   `GEO_EXPECTED_EXECUTORS = ["osm-soa-bake"]`, and `resolve_hotplug`
   checks the consumer name — a plug from `cockpit-server` earns
   `HotplugDrift::UnexpectedConsumer`, by design. **q2 is a caller of an
   already-activated device, not a second device.** The plug is the bake
   crate's, and it is already shipped (`capability.rs`, with can-fire
   halves proving the port rejects a wrong consumer and short coverage).

2. **Calling `slab.tile_range(...)` directly is explicitly sanctioned**, so
   Phase 1 needs no rework. `capability.rs` says so in its own module doc:
   *"These are thin, on purpose … the dispatch surface exists so the
   registration is checkable, not to add a layer: a caller that already has
   the module in scope should keep calling `tms::point_to_tiers` directly."*
   The arms exist to make the registration falsifiable, not to be a
   mandatory call path.

3. **`project_fields` is a real gap in the endpoint, and it is the
   authorization one.** `/api/osm/features` currently decodes and returns
   `lon/lat/entity_type/ordinal` with no mask applied. The declared
   capability is `surface ∩ role` and is **fail-closed** — an unauthorised
   position is *absent from the response*, not hidden client-side (the same
   projection doctrine a2ui-rs enforces: RBAC happens before framing, and
   pixels/JSON can't promise what the wire already leaked). For a
   single-user local cockpit that is moot; for anything deployed it is the
   difference between a demo and a surface with an access model.

- [ ] Phase 4 addendum: route the feature response through
      `project_fields(row, surface, role)` rather than hand-decoding, so
      the endpoint inherits the fail-closed projection instead of
      re-implementing (or silently skipping) it. Needs a role source —
      until there is one, say plainly that the endpoint is unauthenticated
      rather than implying a mask exists.

**Anti-patterns this pins** (each burned upstream once, per
`OGAR/.claude/knowledge/hotplug-consumer-migration.md`): no bespoke
per-consumer plug mechanism; no shape ordinals in the classid low u16 (low
= APP render prefix); no git deps on OGAR (path deps to the sibling — a
git+branch dep writes a rev pin into `Cargo.lock`); no parallel registry
beside `domain_tables()` — `ogar-osm` once shipped its own
`OSM_CAPABILITIES` that verified itself against itself and passed while the
real port answered `NoCapabilitiesFor(0x0F01)`.

## Notes

- Disk: the 1.2 GiB Berlin slab is a local dev artifact under `/tmp`, not
  committed. A real deployment needs its own bake-hosting story (S3 /
  local volume) — out of scope for this wiring PoC; Phase 1 just needs
  *a* slab reachable via `OSM_SLAB_PATH`.
- Full `osm_id` recovery needs the `.soa.books` identity codebook sidecar,
  not just `read_identity`'s ordinal. Deferred until a consumer actually
  needs it — Phase 1's response uses ordinal + entity_type + position,
  sufficient to prove the wiring and plot points.
