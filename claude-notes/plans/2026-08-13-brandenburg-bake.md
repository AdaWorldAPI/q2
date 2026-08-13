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

### 4. Publish
- [ ] Archive anything already at `q2/bakes/brandenburg-v1/` to
      `q2/bakes/archive/brandenburg-v1-<date>/` by **server-side copy**
      (`s3.copy`, no download). Copy — do not delete — so there is never a
      window without a bake.
- [ ] Generate `SHA256SUMS` in `sha256sum` format, naming the slab
      `brandenburg.soa` (not the local stem).
- [ ] Upload **artifacts first, `SHA256SUMS` LAST.** `fetch_sums` gates every
      artifact: new files under old sums fail `download_verified`, which
      "leaves no file behind" — i.e. a total outage, silently.
- [ ] Verify from the bucket: re-read the headers by ranged GET, confirm sizes
      match local and `SHA256SUMS` matches what was uploaded.

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
