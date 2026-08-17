# Chains/Books: replace the permanent owned-`Vec` load with lancedb blob-v2

## Overview

Railway RAM jumps to ~4.9 GB on the FIRST request after boot (not during
boot — boot-time hydration was already fixed in #135/#136/#137). Root
cause, found by correlating the Railway memory graph (GMT+2) against the
boot log (UTC): `osm_features.rs::open_chains()` and `open_books()` do a
full eager `std::fs::read()` of the `.chains`/`.books` sidecar files into
an **owned, permanently-resident `Vec<u8>` / `Vec<String>`**, held in a
`'static OnceLock` for the life of the process. Unlike `memmap2::Mmap`
(already used correctly by `open_slab()`), an owned heap `Vec` has no
page-cache backing — `MADV_DONTNEED` / eviction under memory pressure
cannot reclaim it. Brandenburg: 4,434,464 chains + 7,330,219 book rows,
loaded once on first request, resident forever.

**Explicit governing directive (user):** use `AdaWorldAPI/lance-graph`
directly — not a bespoke mmap-only rewrite of `Chains`/`Books` (this
session's original, wrong, proposal), and NOT the generic `lancedb`
wrapper crate either (a second wrong turn this session took, reading
"lancedb" as the crate name instead of "lance-graph" the repo). Per
`.claude/rules/architectural-compliance.md`: use the exact thing
specified. `osm_lance.rs` **already** depends on `lance_graph_contract`
(`NODE_ROW_STRIDE`, `ENVELOPE_LAYOUT_VERSION`) and the `lance` crate
directly (`lance::dataset::{Dataset, WriteMode, WriteParams}`) — the
exact pattern lance-graph's own `soa_to_lance.rs` example established,
and the one this fix extends. No new Cargo dependency is added.

## Key design decision — batched per-request fetch, not per-lookup

A synchronous full-file `lance::dataset::Dataset::take_rows`/`Scanner`
call per individual `chains.get(ordinal)` would still turn
`query_tile_shapes`'s hot loop — up to `geometry_row_budget(z)` rows per
tile, plus `query_feature`'s per-tag-facet `books.tag_keys.key()` /
`tag_values.key()` / `identities.key()` calls — into one Lance scan
**per lookup**. At hundreds of rows and several tags per row per tile
request, that is a severe latency/throughput regression against today's
O(1) synchronous index lookup, and is not an acceptable trade for fixing
the RAM bug.

**Fix shape:** two phases per request, not one Lance read per lookup.

1. **Gather** (at the top of the handler, before the existing sync
   functions run): scan the already-sampled rows once to collect every
   ordinal the request will actually need (chain ordinals;
   identity/tag-key/tag-value ordinals for the books). Issue **one
   `take_rows` call per table per request** — Lance's `take_rows` takes a
   row-address list and returns exactly those rows in one physical scan,
   the same primitive `osm_lance.rs`'s own row-slab reads build on. Build
   small, request-scoped `HashMap<u32, Vec<u8>>` (chains) /
   `HashMap<u32, String>` (per book) from the results.
2. **Unchanged sync CPU loop**: `query_feature` / `query_tile_shapes` keep
   their existing shape and complexity class, just reading from the
   transient per-request map (`HashMap::get`, O(1)) instead of a `'static`
   `Chains`/`Books`. The map is dropped when the request completes — no
   process-lifetime residency, which is what actually satisfies "zero
   copy storage concern" here: not a literal zero-copy read of a 50 MB
   file, but the elimination of a **permanent, unevictable, whole-dataset**
   copy in favor of a transient, per-request, bounded one, served through
   lance-graph's own storage layer instead of a second bespoke format.

This keeps the RAM fix (nothing pinned forever) without trading it for a
throughput regression (no per-tag Lance round trip).

## Scope — q2-side only, mirrors `osm_lance.rs`'s existing pattern EXACTLY

`osm_lance.rs` already established the precedent for the row slab: convert
a sidecar file into a Lance dataset **at boot, on the same local volume,
q2-side only**, using `lance::dataset::{Dataset, WriteMode, WriteParams}`
directly and `lance_graph_contract`'s canonical constants — never touching
`openstreetmap-website-rs`'s bake pipeline, never S3-backed for serving,
**and no new Cargo dependency** (both crates are already in
`cockpit-server/Cargo.toml`). Chains/Books follow the identical shape,
just with a variable-length Arrow column instead of `FixedSizeBinaryArray`:

- `openstreetmap-website-rs`'s `bake` binary keeps producing `.chains` /
  `.books` exactly as today — **no format change**, no bake-side PR
  required for the storage fix itself.
- One small, additive, non-breaking change IS needed there: `Chains` and
  `Books`'s current constructors (`Chains::from_bytes(bytes: Vec<u8>)`,
  `read_books<R: Read>`) take ownership / stream-parse, which is the right
  shape for the bake's own consumer but not for a boot-time conversion
  that wants to walk `(ordinal, byte-range)` pairs over a borrowed mmap
  slice without a second owned copy of the whole file. Add a borrowing
  entry-iterator (`Chains`: e.g. `pub fn iter_from_bytes(bytes: &[u8]) ->
  Result<(Header, impl Iterator<Item = (u32, &[u8])> + '_), ChainError>`;
  `Books`/`codebook.rs`: the equivalent per-book borrowing iterator) so
  the wire-format parsing logic stays owned by `osm_soa_bake` (the crate
  that already tests it) instead of being duplicated in q2.
- q2 gets a new module (`osm_chains_books_lance.rs`, sibling to
  `osm_lance.rs`, same imports: `lance::dataset::{Dataset, WriteMode,
  WriteParams}`) that, at boot, mmaps `.chains`/`.books`, walks entries
  via the new borrowing iterators, builds ONE Arrow `RecordBatch` per
  table — `ordinal: UInt32Array` + `value: LargeBinaryArray` (an
  offsets+values buffer, natively variable-length, unlike the row slab's
  `FixedSizeBinaryArray`) — in ascending ordinal order, and writes it via
  `Dataset::write` exactly as `write_lance` already does for the row slab.
  Releases the source mmap the same way `osm_lance.rs::release_after_write`
  already does.
- Row addressing: rows are written in ascending ordinal order in ONE
  batch, so row address `i` == ordinal `i` directly — `take_rows` is
  addressed by physical row address (fragment id + offset within
  fragment), which for a freshly written **single-fragment** dataset with
  no deletes is exactly the insertion index. Verified by a dedicated test
  (write N rows, `take_rows(&[0, 7, N-1])`, assert the returned ordinals
  match) — not assumed. `remove_stale_dataset`/single-fragment enforcement
  mirrors what `locate_row_column` already requires of the row-slab
  dataset, for the same reason.

## Checklist

- [x] `openstreetmap-website-rs`: add a borrowing entry-iterator to
      `chains.rs` (`Chains::iter() -> impl Iterator<Item = (u32, &[u8])>`)
      — zero-copy, yields RAW undecoded record bytes per ordinal in
      ascending storage order. `decode_chain` promoted from private to
      `pub` so a consumer holding raw bytes read back from elsewhere
      (Lance) decodes through the SAME function `Chains::get` uses
      internally — one codec, one place, per the module's own doctrine.
      **Books needed no equivalent change**: `IdentityCodebook::key(u32)
      -> Option<&str>` (`lance-graph-contract`) already borrows —
      `(0..book.len() as u32).filter_map(|o| book.key(o))` was already a
      zero-copy per-ordinal walk with no new API needed. TDD: 2 new tests
      in `chains.rs` (`iter_yields_raw_records_that_decode_to_the_same_chains_as_get`
      — cross-checks ascending storage order AND per-ordinal agreement
      with `get()`, not just that it compiles; `iter_on_an_empty_chains_file_yields_nothing`).
      Verified indirectly (see below) rather than via the sibling crate's
      own standalone `cargo test` — see the disk-constraint note below.
- [x] q2: **no new Cargo dependency** — confirmed; `lance` and
      `lance_graph_contract` are already in `cockpit-server/Cargo.toml`,
      nothing added.
- [x] q2: new `osm_chains_books_lance.rs` — the write/read primitives
      (`write_ordinal_blob_dataset`, `OrdinalIndex`, `take_by_row_index`,
      `take_by_ordinal_sparse`), same `lance::dataset::Dataset` +
      `WriteParams` recipe as `osm_lance.rs::write_lance`. **The
      sparse-vs-dense-ordinal addressing split (see module doc) was
      found and fixed during TDD** — an early draft assumed ordinal ==
      row position uniformly, which is true for Books (dense
      codebook ordinals) but FALSE for Chains (sparse — only tagged
      ways get an entry); a deliberately gapped fixture
      (ordinals 7/42/1000/1001) is what caught it.
- [x] q2: `ensure_chains_lance_local(chains_path) -> Option<(PathBuf,
      OrdinalIndex)>` and `ensure_books_lance_local(books_path) ->
      Option<[PathBuf; 4]>` — the actual boot-time conversion functions.
      Chains: reads the local (already S3-hydrated) `.chains` file,
      stores each entry's RAW encoded bytes via `Chains::iter()` (never
      decode-then-re-encode), writes one Lance dataset, returns the
      `OrdinalIndex` the sparse case needs. Books: reads the local
      `.books` file, writes FOUR sibling Lance datasets — one per
      codebook (`region.identities.lance` / `.tag_keys.lance` /
      `.tag_values.lance` / `.labels.lance`) — since each is its own
      dense `0..len` ordinal space (no index needed, row position IS the
      ordinal). Both degrade to `None` on any failure (missing file,
      malformed sidecar, Lance write failure) — best-effort, never a hard
      dependency; the existing eager-`Vec` read path keeps working
      unaffected. 7 tests total, all green — the 3 primitive tests above
      plus 4 new: `ensure_chains_lance_local_round_trips_a_real_chains_sidecar`
      (builds a real `.chains` file via `osm_soa_bake::chains::write_chains`
      with a deliberately gapped ordinal space, converts it, and proves
      every ordinal decodes to EXACTLY what `Chains::get()` returns on the
      original bytes — this is what verifies the sibling-repo `iter()` +
      `decode_chain` change end-to-end, see the note above),
      `ensure_books_lance_local_round_trips_all_four_dense_codebooks`
      (all four books, including a UTF-8 multibyte label — "Hauptstraße"
      — round-tripping byte-for-byte), plus the pre-existing
      `dense_ordinals_...`/`sparse_ordinals_...`/`empty_payload_...`.
- [x] q2: `main.rs` wiring — calls both conversions right after
      `osm_lance::ensure_lance_local`, deriving `.chains`/`.books` paths
      from the same hydrated slab path via `with_extension`. Same
      fail-open shape: any failure or skip just logs and falls through;
      **the read path (`osm_features.rs::open_chains`/`open_books`) does
      NOT yet consume these datasets** — this wiring only proves the
      datasets get built at boot, next to the slab's own `.lance`
      dataset. Confirmed via `cargo check -p cockpit-server` (clean,
      2.83s once the incremental cache was warm).
- [ ] q2: `osm_features.rs` — gather phase (batched `Dataset::take_rows`
      per table) + request-scoped `HashMap`-backed accessor types
      replacing `open_chains()`'s / `open_books()`'s `'static` singletons
      in the request path. `query_feature` and `query_tile_shapes` keep
      their sync shape, fed from the transient maps. **This is the
      remaining item** — everything above it is done and test-verified;
      this item is what actually stops the eager `Vec` from being
      allocated at all.
- [ ] Full verification per `CLAUDE.md`: `cargo build --workspace`,
      `cargo nextest run --workspace`, `cargo xtask verify
      --skip-hub-build` (Rust-only change, no WASM/hub-client surface
      touched). Not yet run — see the disk-constraint note below.
- [ ] Ask for push permission per `CLAUDE.md`'s GIT PUSH POLICY — do not
      push without explicit approval, same as #135/#136/#137.

## Disk-constraint note (this session, honest record)

This environment's effective disk quota is much smaller than `df`'s
nominal 252G suggests (usable space tracks close to `target/debug`'s
size — repeated `ENOSPC` at ~35-37G used). Two near-total exhaustions
happened this session: once from an incremental-cache-invalidating
`rm -rf` on `~/.cargo/registry/src` forcing a 24-minute full workspace
rebuild, and once from running `cargo test` **standalone inside the
`openstreetmap-website-rs` sibling checkout**, which has its OWN cold
`target/` and re-pulls a large overlapping-but-separate dependency
closure (`ogar-vocab`, `ogar-osm`, `lance-graph-contract`, `datafusion`,
etc. from git/registry) instead of reusing q2's already-warm cache.

**Lesson for future sessions touching this sidecar pair:** verify a
change to `openstreetmap-website-rs`'s source **through q2's own
workspace** (`cargo check -p cockpit-server`, which path-deps
`osm-soa-bake` and reuses q2's warm `target/debug/deps`), never via a
standalone `cargo test`/`cargo check` run inside
`/home/user/openstreetmap-website-rs` directly — that cold-starts a
second, separately-cached, multi-gigabyte dependency tree and risks
`ENOSPC`. Consequence for this session: `Chains::iter()`'s correctness
was verified **indirectly**, via q2's own
`ensure_chains_lance_local_round_trips_a_real_chains_sidecar` test
(which exercises `iter()` + `decode_chain` together end-to-end and
would fail if either were wrong) — not by running
`openstreetmap-website-rs`'s own `cargo test --lib chains::` directly.
That direct run is still worth doing when disk headroom allows it, as a
belt-and-suspenders confirmation, but is not currently blocking.

## Open risk, named rather than discovered later

`Dataset::take_rows`/`Scanner` reads go through Lance's own reader
machinery, not a raw mmap — so "zero copy" here means "no permanent
process-owned copy," not "literally zero bytes copied per read" (a small
per-request `Bytes`/`RecordBatch` allocation for the fetched rows is
unavoidable and correct; it is dropped at the end of the request, unlike
today's bug). This distinction is worth stating explicitly — the design
above satisfies the RAM-eviction property (matching what `osm_lance.rs`
already achieves for the row slab via mmap + Lance), not literal
zero-byte-copy on every single read.

## Correction log (this session)

This plan went through two wrong turns before landing here, recorded so
the mistake isn't repeated: (1) an initial proposal to mmap-back `Chains`/
`Books` directly with no Lance involvement at all — rejected by the user,
who wanted the existing lance-graph-based pattern extended, not bypassed;
(2) reading "lancedb" as the literal `lancedb` crate name and designing
around its blob-v2 feature (`lancedb::blob()`, `BlobFile`) — a real crate
with no relationship to lance-graph's own architecture; the user meant the
`AdaWorldAPI/lance-graph` repository, whose pattern (`lance::dataset::
Dataset` + `lance_graph_contract`, already wired into `osm_lance.rs`) is
what this plan now extends.
