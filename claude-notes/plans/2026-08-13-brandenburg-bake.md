# Brandenburg bake — plan

**Status:** ready to execute · **PR: HOLD** (operator: open only after the next PR lands)
**Executor:** Sonnet (mechanical; every step below is a measured command, not a judgement call)

## Overview

Bake `brandenburg-latest.osm.pbf` into the V3 slab + sidecars, validate it the
same way the Berlin bake was validated on 2026-08-13, and publish it to
`q2/bakes/brandenburg-v1/`. Serving it is then `OSM_BAKE_REGION=brandenburg`
and a restart — **no code change**: `bake` is region-agnostic and the sidecars
resolve by extension from the slab's own stem.

This plan exists because the Berlin bake this session exposed four failure
modes that are invisible until they bite. Each has a gate below.

## Preconditions

- [ ] **Disk.** Outputs are ~4.7 GB (extrapolated, see Sizing). Free space
      first: `cargo clean` in `/home/user/q2` reclaims ~5.5 GB and is not
      needed for the bake — the baker lives in `openstreetmap-website-rs`,
      which is already built. Verify `df -h /` shows **≥ 8 GB** before starting.
- [ ] **Baker built:** `openstreetmap-website-rs/target/release/{bake,parity}`.
      If absent: `cargo build --release --bin bake --bin parity` in that repo
      (~31 s cold; it does *not* pull lance/datafusion).
- [ ] **S3 env present:** `AWS_S3_BUCKET_NAME`, `AWS_ACCESS_KEY_ID`,
      `AWS_SECRET_ACCESS_KEY`, `AWS_ENDPOINT_URL`. Never print their values.

## Sizing — extrapolated from Berlin, NOT measured

Berlin measured this session: 98,840,376 B PBF → 2,766,291 rows →
1,416,340,992 B slab, 62.7 MB books, 63.9 MB chains, **38.9 s**.

Brandenburg's PBF is 298,424,907 B = **3.02× Berlin**, so a linear read gives:

| | extrapolated |
|---|---|
| rows | ~8.4 M |
| slab | ~4.3 GB |
| books | ~190 MB |
| chains | ~193 MB |
| bake wall time | ~2 min |

**Treat these as predictions to be falsified, not facts.** Record the real
numbers in the Results section. Two reasons they may be wrong in either
direction: Brandenburg is rural, so its ways are longer with more nodes each
(rows could scale sub-linearly against PBF bytes while chains scale
super-linearly); and PBF size tracks features, not rows.

**Memory is the under-considered risk.** `read_features_with_chains` holds a
`coords: HashMap<i64, TileXy>` over *every* node — Berlin indexed 7.87 M, so
Brandenburg is ~24 M entries plus `way_chains`. Expect multiple GB of RSS. If
the bake is OOM-killed, that is the cause; it is not a disk problem and
retrying will not help.

## Work items

### 1. Reassemble the input
- [ ] Download `OSM/bb.part.00` … `bb.part.03` from the bucket.
- [ ] `cat bb.part.00 bb.part.01 bb.part.02 bb.part.03 > brandenburg-latest.osm.pbf`
      (plain byte split — name order restores it exactly).
- [ ] **Gate:** `sha256sum -c` against
      `0662b67825091986d45b8df070d1df43fa8048548e8defe680ec8df61fe1038c`
      must print `OK`. Do not proceed on failure.
- [ ] Delete the four parts once verified (saves ~298 MB while baking).

> Why from S3 and not a fresh download: an OSM `*-latest.osm.pbf` always
> serves *today's* snapshot, so the pinned bytes are **not re-downloadable**.
> This is the exact input the overflow-split rung was measured from.

### 2. Bake
- [ ] `bake brandenburg-latest.osm.pbf brandenburg`
      — output stem `brandenburg` (no extension) so the sidecars land as
      `brandenburg.books` / `brandenburg.chains`, which is what
      `with_extension()` resolves to on the serving side. Upload the stem file
      under the key `brandenburg.soa`.
- [ ] Record the reported `slab digest`, `ROWS`, `bytes`, and timing.

### 3. Validate — every gate is two-sided

- [ ] **Codebook format.** `brandenburg.books[0..8] == b"OSMCBK\0\x03"`.
      *This is the exact failure that took Berlin's map down for a day*: the
      deployed codebook was `\x02` after `openstreetmap-website-rs@3142a8d`
      bumped the format, so it was refused at the magic check before the
      digest check ever ran, and every shape fell back to `ShapeClass::Other`
      (grey map, HTTP 200, no error anywhere).
- [ ] **Digest agreement.** `books[24..32] == chains[8..16]` and both equal the
      digest `bake` reported. Books/chains headers: books is
      `magic(8) rows(8) slots(8) slab(8)`, chains is `magic(8) slab(8)`.
- [ ] **Row count.** `books.rows == slab_bytes / 512`, and `slab_bytes % 512 == 0`.
- [ ] **Parity.** `parity brandenburg-latest.osm.pbf brandenburg brandenburg.books`
      must print `VERDICT PARITY` with **`tags exact … bad 0`** and
      `NOT recovered 0`. This is the gate that actually proves the codebook
      resolves; the header checks only prove it is readable.

### 4. Publish — stage, verify, then cut over

**Never upload in place over a live prefix.** An earlier version of this plan
said "artifacts first, `SHA256SUMS` last", reasoning that sums-last minimises
the bad window. It does not close it. For the whole upload the live prefix
holds new artifacts under the *old* manifest, and `fetch_sums` gates every
artifact against it: `download_verified` rejects each mismatch and **leaves no
file behind**. Any boot in that window therefore finds no slab at all — a
total outage, silently, from a step whose own note already said so. Ordering
makes the window smaller; only staging removes it.

- [ ] Upload the three artifacts **and** `SHA256SUMS` to an immutable, dated
      staging prefix — `q2/bakes/brandenburg-v1-<YYYYMMDD-HHMM>/`. Nothing
      reads it yet, so a partial upload here is inert rather than an outage.
- [ ] Verify the STAGED prefix from the bucket, not from local state: ranged
      GET the books/chains headers, confirm the magic, confirm both digests
      agree, confirm sizes, and confirm `SHA256SUMS` content matches. (This
      catches a bad transfer; comparing to what you uploaded does not.)
- [ ] **Cut over only after the staged prefix verifies**, by pointing the
      runtime at it (`OSM_SLAB_S3_PREFIX`) — one atomic change, not a
      multi-object mutation. There is no moment where a reader sees a
      half-published prefix.
- [ ] **Keep the previous prefix** for rollback; it stays complete and
      self-consistent throughout, so reverting is the same one-value change.

> **Tooling.** `q2/bakes/tools/publish_bake.py` in the bake bucket now
> implements exactly this: it stages to `q2/bakes/<region>-v1-<stamp>/`,
> refuses to reuse a stamp, verifies the staged prefix **by reading it back
> from the bucket**, and prints the one `OSM_SLAB_S3_PREFIX=…` value to cut
> over. The previous in-place version is kept at
> `q2/bakes/tools/archive/publish_bake.in-place.py` — it is the script that
> produced the hazard described above, retained for provenance, not for use.

> If a same-prefix publish is ever genuinely unavoidable, the ONLY safe order
> is: delete `SHA256SUMS` first (readers then fail closed on a missing
> manifest and keep their warm volume), upload artifacts, upload sums last.
> Staging is still preferable — it never makes the live prefix unreadable at
> all.

### 5. Roll out (operator)
- [ ] Set `OSM_BAKE_REGION=brandenburg` and restart cockpit-server.
- [ ] **Gate:** `GET /api/osm/health` → `books.loaded: true` and
      `styling: "ok: …"`. If false, the `styling` line names which of the four
      causes it is — do not guess.
- [ ] **Gate:** a tile's class histogram shows real classes, not
      `{"other": N}`.

## Risks and the traps already paid for

1. **`OSM_SLAB_PATH` short-circuits hydration.** `ensure_slab_local` returns
   early if that env var is set and the file exists — before any S3 or
   checksum logic. If it is set in the deploy, a restart will reuse the stale
   region's slab forever. Unset it, or clear the volume.
2. **A restart is mandatory.** `open_books()` caches in a `OnceLock` and
   hydration runs at boot; uploading changes nothing until the process
   restarts.
3. **Cold-boot delay.** Hydration blocks the listener bind. Berlin's 1.42 GB
   took long enough to serve 502s; ~4.7 GB will take proportionally longer.
   Expect a visible outage window on the first boot after the switch — this is
   the hydrator working, not failing.
4. **Both regions cannot be served at once.** `OSM_BAKE_REGION` is a single
   value. Serving Brandenburg replaces Berlin; it does not add to it.
5. **Brandenburg is past Arrow's row ceiling, so it serves from the raw
   `.soa`, not from Lance.** This bit during rollout — see below.
6. **The persistent volume must hold ~4.13 GB, and nothing checks that it
   can.** Measured artifacts total 3,753,072,128 + 155,979,619 + 221,252,224
   = **4,130,303,971 B (3.85 GiB)** — **2.7x Berlin's ~1.54 GB**, and
   `osm_slab_hydrate`'s own docs are written around Berlin ("the 1.29 GiB
   artifact", "Berlin is ~1.42 GB"). There is **no free-space preflight** in
   that module: no `statvfs`, no capacity check, no ENOSPC branch.

   The failure mode if the volume is too small is quiet and self-inflicted:
   a download that runs out of space produces a truncated file, the checksum
   gate rejects it, `download_verified` **leaves no file behind**, and the
   next boot repeats it. The listener never binds, so from outside it is an
   indefinite 502 — **identical to a slow first hydration**, which is the
   benign case. The two cannot be told apart without the deploy logs.

   Before switching a region this large, check the volume's capacity against
   the artifact total, and on a persistent 502 read the logs rather than
   waiting: "still hydrating" resolves itself and "volume too small" never
   does. A free-space preflight that fails loudly with the two numbers would
   remove the ambiguity entirely; it does not exist yet.

## The rollout crash — Arrow's i32 array ceiling (measured 2026-08-14)

Setting `OSM_BAKE_REGION=brandenburg` put the service in a **startup
crash-loop**. Not a bake defect: every artifact verified, and the panic was
downstream of hydration in the Lance conversion.

`osm_lance` converts the slab into a one-column Lance dataset so tiles can be
served by mmap+offset. That column is a `FixedSizeBinaryArray`, which holds
every row in ONE flat `Buffer`, and Arrow's classic (non-`Large`) array format
bounds a buffer to `i32::MAX` bytes. At our 512-byte stride that is a hard
ceiling:

| | rows | vs ceiling |
|---|---|---|
| ceiling (`i32::MAX / 512`) | 4,194,303 | — |
| Berlin | 2,766,291 | 0.66× — fits |
| **Brandenburg** | **7,330,219** | **1.75× — panics** |

It is a **format constant, not a tunable**. Arrow's own validation panicked,
and the `.unwrap()` took the process down at boot — contradicting this module's
own design, where the Lance path is explicitly "a pure optimization... never a
hard requirement" and the caller already has a safe `None` arm.

Fixed in q2 PR #129: the row count is checked immediately after it is
computed — before a stale dataset is removed and before the 3.75 GB
`std::fs::read` — and an oversized region returns `None` to the **existing**
raw-`.soa` fallback. Position was as load-bearing as the check: two reviewers
independently caught that a downstream guard would delete a usable dataset and
read gigabytes to reach a decision that needs only a row count.

**Operator consequence:** Brandenburg serves correctly but **without the
mmap+offset fast path**. That is a performance difference, not a correctness
one. Restoring it needs multi-batch Lance writes whose row column stays ONE
contiguous run in ONE data file — `locate_row_column`'s checks 1 and 4 exist
precisely because a fragmented layout is unsafe to address by raw offset.
Deliberately deferred out of the incident; it is a design task, not a patch.

**Predicting it for the next region:** `rows > 4,194,303` is the whole test,
and rows are reported by `bake`. Any region past ~4.19 M rows takes the raw
path until the multi-batch work lands.

## Results — executed 2026-08-13

| | predicted | measured | ratio |
|---|---|---|---|
| rows | ~8.4 M | **7,330,219** | 0.87× |
| slab bytes | ~4.3 GB | **3,753,072,128** (3.50 GiB) | 0.87× |
| books | ~190 MB | **155,979,619** | 0.82× |
| chains | ~193 MB | **221,252,224** | **1.15×** |
| slab digest | — | `7bb1db0cc8794f79` | — |
| bake wall time | ~2 min | **127.6 s** | 1.06× |
| parity verdict | `PARITY` | **`VERDICT PARITY`** | ✓ |
| peak RSS | unknown | **2.34 GiB** | — |

Parity detail: `tags exact 6,732,666 bad 0`, `NOT recovered 0`, `unknown to
source 0`, `kind mismatches 0`, node/derived position `bad 0`, junction rows
`590,437 ok / 0 bad`.

### The prediction was falsified, in the direction this plan flagged

Rows, slab bytes and books all landed **under** the linear extrapolation
(0.82–0.87×) while chains landed **15% over** — the one metric that moved the
opposite way. That is exactly the Sizing section's stated hypothesis: rural
ways are longer with more nodes each, so **row count scales sub-linearly**
against PBF bytes while **per-way chain data scales super-linearly**. Reading
one number as "close enough" would have hidden that the two quantities move in
opposite directions; only the paired prediction exposes it.

Practical consequence for the next region: size the slab from a rows-per-PBF-
byte ratio, but size `.chains` from way *geometry*, not from PBF bytes. The two
are not interchangeable and a single scalar cannot carry both.

**Memory was the flagged risk and it did not bite:** 2.34 GiB peak against a
worry of "multiple GB", with ~24 M nodes indexed. No OOM.

### Notes from the run

- Reassembly sha256 gate passed before any bake work began.
- `q2/bakes/brandenburg-v1/` was empty, so the archive-copy step was a
  correct no-op. `berlin-v1/` was listed for visibility only — never copied,
  modified or deleted, and confirmed intact afterwards.
- Upload order held: three artifacts, then `SHA256SUMS`.
- All four objects were re-verified by ranged GET **from the bucket** (magic
  bytes, digest agreement, sizes, sums content) rather than by comparing to
  what was uploaded — the distinction that catches a bad transfer.
- Rollout (step 5) deliberately untouched; it is the operator's call.
- The run used download+`cat`, not the streaming fetch: the parts were already
  verified and deleted before that guidance landed. **Next bake should use
  `fetch_pbf.py`** — peak overhead ~8 MB instead of holding the input twice.

## References

- Berlin bake + the codebook-format outage: q2 PR #127, this session.
- Reassembly + provenance: `OSM/REASSEMBLE.md` in the bake bucket.
- Region is config, not code: `osm_slab_hydrate::{bake_region, artifacts,
  default_prefix}`.
