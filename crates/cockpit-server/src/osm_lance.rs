//! Convert the hydrated OSM `.soa` slab into a Lance dataset on the same
//! persistent volume — the same recipe `lance-graph`'s `soa_to_lance`
//! example established and proved (zero-copy import, the 512-byte SoA
//! contract persisted once as Arrow schema metadata, the P-CACHE-4
//! verbatim-row-column guarantee that lets a synchronous mmap keep serving
//! the bytes unchanged). Duplicated here rather than called as a library
//! function: `soa_to_lance` is currently an example BINARY, not a lib
//! export, and this module needs the same recipe from inside an
//! already-running server's boot sequence, not a subprocess. The two
//! contract constants it depends on (`NODE_ROW_STRIDE`,
//! `ENVELOPE_LAYOUT_VERSION`) are imported from `lance_graph_contract`,
//! never restated, so the two copies cannot drift on the CONTRACT — only on
//! plumbing around it.
//!
//! # Why the write lands on volume01, never S3
//!
//! [`crate::osm_slab_hydrate`] already resolved the persistent volume
//! (`OSM_SLAB_CACHE_DIR` / `RAILWAY_VOL` — Railway's own default mount is
//! `/volume01`) to land the raw `.soa`/`.books`/`.chains` bake. The `.lance`
//! dataset is written into that SAME directory, for the reason
//! `medcare-rs::medcare_db::LanceStore::open`'s own doc gives for keeping
//! its store local by default: at one replica, an S3-backed Lance dataset
//! turns every read into a network round-trip and gives up mmap/page-cache
//! zero-copy. S3 stays the HYDRATION source; it is never the serving
//! location — this module never targets an `s3://` uri.
//!
//! # The request-time read path — now wired, still synchronous
//!
//! [`crate::osm_features::open_slab`] keeps mmapping bytes synchronously —
//! it is simply handed a DIFFERENT file when [`locate_row_column`] finds
//! one. `soa_to_lance` (lance-graph) is what proves the mechanism this
//! module productionizes: at the 512-byte `NODE_ROW_STRIDE`, Lance's
//! `is_narrow` mini-block cutoff (256 bytes) is never crossed, so the row
//! column lands full-zip, uncompressed, and byte-verbatim in the dataset's
//! own on-disk data file — meaning `RowSlab::new(bytes)` parses
//! Lance-owned bytes exactly as it parses the raw `.soa` file. Reaching
//! into a Lance fragment's on-disk layout to find that byte run is a
//! diagnostic probe in `soa_to_lance` (its own "P-CACHE-4" section, which
//! reads the WHOLE file to prove the point once, by hand) — here it is a
//! bounded, boot-time, fail-closed resolver: [`locate_row_column`] searches
//! only the first few megabytes of each data file (row 0 is never
//! gigabytes in), and `open_slab` falls back to the raw `.soa` file
//! unconditionally on `None`, so a resolver miss costs nothing but the
//! optimization itself.
//!
//! # What "loaded into lance-graph" means here
//!
//! The bake does not yet mint an OGAR classid space for OSM node rows (see
//! lance-graph PR #902's federation-model gap, which explicitly leaves this
//! open for every bake outside its own). [`OSM_BAKE_CLASSID`] is therefore
//! the canon's OWN documented bootstrap value — lance-graph `CLAUDE.md`'s
//! zero-fallback ladder: *"classid == 0x0000_0000 → default class, no
//! prefix routing (dormant)"* — never an invented placeholder. Minting a
//! real classid for OSM rows is future work, not something this module
//! should improvise.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::FixedSizeBinaryArray;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use lance::dataset::{Dataset, WriteMode, WriteParams};
use lance_graph_contract::canonical_node::NODE_ROW_STRIDE;
use lance_graph_contract::soa_envelope::ENVELOPE_LAYOUT_VERSION;

/// Schema-metadata keys — identical to `soa_to_lance`'s, namespaced `soa:`
/// so they never collide with Lance's own (`lance-encoding:*`) or Arrow's.
const K_LAYOUT: &str = "soa:envelope_layout_version";
const K_STRIDE: &str = "soa:row_stride";
const K_CARVING: &str = "soa:row_carving";
const K_ENDIAN: &str = "soa:endianness";
const K_CLASSID: &str = "soa:classid";
const K_DIGEST: &str = "soa:slab_digest";
const K_SOURCE: &str = "soa:source";
// The "hot but idle" fast-path identity — see `reopen_if_unchanged`'s doc
// comment. Recorded alongside `K_DIGEST` at write time; a later boot whose
// slab's CURRENT stat matches both never touches slab bytes at all.
const K_SLAB_MTIME: &str = "soa:slab_mtime_nanos";
const K_SLAB_LEN: &str = "soa:slab_len";

const ROW_COLUMN: &str = "row";

/// See the module doc's "What 'loaded into lance-graph' means here" — the
/// canon's own dormant-default classid, not a mint.
const OSM_BAKE_CLASSID: &str = "00000000";

/// Ensure a Lance dataset exists at `<slab>.lance`, converting from the
/// verified `.soa` slab (already hydrated onto the persistent volume by
/// [`crate::osm_slab_hydrate::ensure_slab_local`]) if absent or stale.
///
/// Never panics; a failure returns `None` and the caller keeps mmapping the
/// raw `.soa` file exactly as it did before this module existed — the
/// Lance conversion is a best-effort addition, not a new hard dependency
/// for serving the map.
///
/// Mirrors `medcare-rs::bake_hydrate::hydrate_crystal`'s warm-skip gate: a
/// dataset whose row count AND slab digest already match this slab costs
/// one `count_rows` + one metadata read, never a rewrite of gigabytes.
pub async fn ensure_lance_local(slab_path: &Path) -> Option<PathBuf> {
    let dest = slab_path.with_extension("lance");

    // Stat-only, no bytes read yet. `rows` (needed for the Arrow ceiling
    // check below) and the slab's filesystem identity (needed for the
    // hot-but-idle fast path just after) both come from this alone.
    let meta = match std::fs::metadata(slab_path) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(path = %slab_path.display(), error = %e, "osm lance: cannot stat slab");
            return None;
        }
    };
    let len = meta.len() as usize;
    if len == 0 || !len.is_multiple_of(NODE_ROW_STRIDE) {
        tracing::error!(
            path = %slab_path.display(), len,
            "osm lance: slab is not a whole number of {NODE_ROW_STRIDE}-byte rows; refusing"
        );
        return None;
    }
    let rows = len / NODE_ROW_STRIDE;

    // The Arrow ceiling is decided by `rows` ALONE, so decide it here — the
    // earliest point `rows` exists — rather than downstream in `write_lance`.
    //
    // `FixedSizeBinaryArray` holds the whole row column in one flat `Buffer`,
    // and Arrow's classic (non-`Large`) array format bounds that buffer to
    // `i32::MAX` bytes. At our 512-byte stride that is a hard ceiling of
    // 4,194,303 rows — not a tunable. Berlin's 2,766,291 rows sit under it;
    // Brandenburg's 7,330,219 are 1.75x over, which panicked startup in a
    // crash-loop until this guard existed.
    //
    // Position matters as much as the check. Placed downstream, this guard
    // was reached only AFTER `remove_stale_dataset` had deleted a perfectly
    // good existing dataset and after the whole 3.75 GB slab had been
    // allocated — every boot, forever, to reach a decision that needed
    // only a row count. That trades a panic for an OOM risk plus a
    // guaranteed-useless multi-gigabyte read, which is not a fix. Deciding
    // before both is what makes the fallback actually cheap.
    if !row_count_fits_arrow_array(rows) {
        tracing::warn!(
            rows,
            max_rows_per_array = arrow_max_rows_per_array(),
            stride = NODE_ROW_STRIDE,
            "osm lance: row count exceeds FixedSizeBinaryArray's i32 byte-length ceiling \
             (Arrow's classic-array limit, not tunable); skipping the Lance mmap-offset \
             optimization without reading the slab — the raw .soa slab keeps serving the map"
        );
        return None;
    }

    // ── Hot-but-idle fast path ───────────────────────────────────────────
    // Trust an already-warm dataset's OWN recorded slab identity (mtime+len)
    // without ever opening or mmapping the slab. See `reopen_if_unchanged`'s
    // doc comment for why this is sound. `slab_mtime_nanos` returning `None`
    // (a platform/filesystem without mtime support) just skips the fast
    // path — the slower, always-correct path below still runs. Hoisted into
    // a variable (not just an `if let` scope) so a fresh conversion at the
    // end of this function can also record it for the NEXT boot's fast path.
    let mtime_nanos = slab_mtime_nanos(&meta);
    if let Some(mtime_nanos) = mtime_nanos
        && let Some(dest_out) = reopen_if_unchanged(&dest, rows, mtime_nanos, len as u64).await
    {
        tracing::info!(
            path = %dest_out.display(), rows,
            "osm lance: hot — dataset's recorded slab identity (mtime+len) matches; \
             skipped mmap+hash entirely"
        );
        return Some(dest_out);
    }

    // ── Below here: the slower, content-hash-based path — unchanged from
    // before this fast path existed, reached now only when the fast path
    // above declined (cold boot, touched mtime, or a dataset predating this
    // fix). Digest over an mmap, not `fs::read`: this path must not pay a
    // slab-sized heap allocation (1.29 GiB for Berlin) just to compute a
    // hash — the pages stream through the page cache and stay reclaimable.
    let file = match std::fs::File::open(slab_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(path = %slab_path.display(), error = %e, "osm lance: cannot open slab");
            return None;
        }
    };
    // SAFETY: read-only mapping of the baked, immutable artifact.
    let map = match unsafe { memmap2::Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(path = %slab_path.display(), error = %e, "osm lance: cannot mmap slab");
            return None;
        }
    };
    #[cfg(test)]
    SLOW_PATH_HASH_ATTEMPTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let digest_hex = format!("{:016x}", osm_soa_bake::codebook::hash_slab(&map));
    // The map is KEPT, not dropped. It used to be released here and the file
    // read again into an owned `Vec` for the write — a slab-sized ANONYMOUS
    // allocation (1.4 GB for Berlin, 3.75 GB for Brandenburg) that no memory
    // pressure can ever reclaim. Reusing these pages makes the conversion's
    // source cost page cache instead: still charged to the cgroup while
    // touched, but evictable, which anonymous memory is not.
    let map = std::sync::Arc::new(map);

    if let Some(warm) = reopen_if_warm(&dest, rows, &digest_hex).await {
        tracing::info!(
            path = %dest.display(), rows,
            "osm lance: warm — dataset already matches this slab's row count and digest"
        );
        // The warm path pays the SAME mmap+full-hash cost as a rebuild (it
        // has to, to know it's warm) but until this line it paid that cost
        // and then just dropped `map` — no `MADV_DONTNEED`, unlike the
        // rebuild path's `release_after_write` below. On a redeploy where
        // nothing changed (the common case in steady state), that left the
        // whole slab resident with nothing to reclaim it under a memory
        // limit far above what's needed to create pressure. Same fix,
        // same reasoning, the other call site — see `release_after_write`'s
        // own doc comment for the mechanism.
        release_after_write(map, &dest);
        return Some(warm);
    }

    // A stale dataset is REMOVED before reconversion, not overwritten in
    // place. `WriteMode::Overwrite` only writes a new manifest VERSION —
    // Lance keeps the previous version's fragment files on disk, so
    // overwriting the pre-fix three-fragment Berlin dataset would leave
    // `data/` holding 4 `.lance` files, and [`locate_row_column`]'s
    // sole-data-file precondition would then fall back to the raw `.soa`
    // forever (plus 1.3 GB of dead fragments squatting on the volume).
    // The dataset is a derived cache of the slab — version history has no
    // value here; the slab itself is the recovery path. Fail-closed: if
    // the stale directory cannot be removed, do NOT write into it (that
    // is exactly how a 4-file `data/` happens) — keep serving from the
    // `.soa` unchanged.
    if dest.exists() {
        if let Err(e) = remove_stale_dataset(&dest) {
            tracing::error!(
                path = %dest.display(), error = %e,
                "osm lance: cannot remove the stale dataset; refusing to write \
                 into it — the raw .soa slab keeps serving the map"
            );
            return None;
        }
    }

    tracing::info!(
        path = %dest.display(), rows,
        "osm lance: converting the .soa slab into a Lance dataset (zero-copy import)"
    );
    write_lance(
        &dest,
        map,
        rows,
        &digest_hex,
        slab_path,
        mtime_nanos,
        len as u64,
    )
    .await
}

/// Most rows one `FixedSizeBinaryArray` can hold at [`NODE_ROW_STRIDE`].
///
/// The array has no offsets buffer — the whole column is one flat `Buffer`
/// addressed as `stride * index` — but Arrow's classic (non-`Large`) format
/// still bounds that buffer's byte length to `i32::MAX`. At our 512-byte
/// stride that is **4,194,303 rows**, and it is a property of the format, not
/// a tunable.
#[must_use]
fn arrow_max_rows_per_array() -> usize {
    usize::try_from(i32::MAX).unwrap_or(usize::MAX) / NODE_ROW_STRIDE
}

/// Whether `rows` can be built as a single Arrow row array.
///
/// Extracted so the production guard and its tests are the SAME predicate.
/// The first version of this fix inlined the comparison and tested it by
/// re-deriving the formula in the test — which proves the arithmetic agrees
/// with itself, not that the shipped code rejects the row count that crashed
/// production. A test that cannot fail when the guard is deleted is not a
/// test of the guard.
#[must_use]
fn row_count_fits_arrow_array(rows: usize) -> bool {
    rows <= arrow_max_rows_per_array()
}

/// Remove a stale dataset directory in full, so the follow-up write starts
/// from nothing and `data/` ends up holding EXACTLY the one fragment file
/// the write produces. Split out for the falsifier below; guarded at the
/// call site by `dest.exists()`.
fn remove_stale_dataset(dest: &Path) -> std::io::Result<()> {
    tracing::info!(
        path = %dest.display(),
        "osm lance: removing the stale dataset before reconversion \
         (version history has no value for a derived cache)"
    );
    std::fs::remove_dir_all(dest)
}

/// `mtime` as nanoseconds since the Unix epoch, or `None` on any failure to
/// read it (a platform without mtime support, or a clock before 1970 — the
/// caller treats `None` as "cannot use the fast path", never as an error).
#[must_use]
fn slab_mtime_nanos(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

/// Reachability counter for the slow, content-hash-based path (test builds
/// only) — proves whether [`ensure_lance_local`] actually opened+mmapped the
/// slab, which `/proc/self/statm` and this sandbox's absent
/// `/sys/fs/cgroup/*` cannot show directly. See
/// `a_hot_boot_skips_the_full_slab_hash_entirely` below, and
/// `EVICTIONS_ATTEMPTED`'s doc comment for why a reachability counter is the
/// right substitute for a memory measurement here.
#[cfg(test)]
static SLOW_PATH_HASH_ATTEMPTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// `Some(dest)` iff a dataset already exists at `dest`, is a single
/// fragment, has exactly `rows` rows, AND its own recorded slab identity —
/// `soa:slab_mtime_nanos` + `soa:slab_len` — matches the CURRENT slab's,
/// all proven **without reading a single byte of the slab**.
///
/// This is sound, not merely convenient: any write to a file updates its
/// mtime, so `(mtime, len)` unchanged since the dataset was written is
/// conclusive proof the slab's bytes are unchanged too — the same
/// heuristic `make`, `rsync` (default mode), `cargo`, and `ccache` all rely
/// on for exactly this reason (hashing every input on every run is their
/// slow, explicit opt-in, never the default). It does NOT try to prove
/// anything about content that genuinely changed; a mismatch here is not a
/// verdict, only "cannot fast-path" — [`ensure_lance_local`] falls through
/// to the slower, always-correct mmap+[`osm_soa_bake::codebook::hash_slab`]
/// path below, which is untouched by this function's existence.
///
/// **Named limitation:** a dataset written before this fast path existed
/// (or one whose slab's mtime was touched without its content changing —
/// e.g. a re-download of byte-identical bytes) has no stored identity to
/// match, or a permanently-mismatching one, and falls back to the slow path
/// FOREVER until the slab is genuinely rebuilt. That is a missed
/// optimization, never a correctness problem: the slow path is exactly
/// today's already-correct behaviour.
async fn reopen_if_unchanged(
    dest: &Path,
    rows: usize,
    mtime_nanos: u128,
    len: u64,
) -> Option<PathBuf> {
    if !dest.exists() {
        return None;
    }
    let uri = dest.to_string_lossy().into_owned();
    let ds = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
        .ok()?;
    let got_rows = ds.count_rows(None).await.ok()?;
    let got_mtime: Option<u128> = ds
        .schema()
        .metadata
        .get(K_SLAB_MTIME)
        .and_then(|s| s.parse().ok());
    let got_len: Option<u64> = ds
        .schema()
        .metadata
        .get(K_SLAB_LEN)
        .and_then(|s| s.parse().ok());
    let fragments = ds.get_fragments().len();
    if got_rows == rows && got_mtime == Some(mtime_nanos) && got_len == Some(len) && fragments == 1
    {
        Some(dest.to_path_buf())
    } else {
        None
    }
}

/// `Some(dest)` iff a dataset already exists at `dest` with exactly `rows`
/// rows and a `soa:slab_digest` header matching `digest_hex` — the ONLY two
/// facts that must hold before skipping the write. Any other outcome
/// (missing, unreadable, wrong row count, wrong digest) returns `None` so
/// the caller reconverts rather than serve stale or half-written data.
async fn reopen_if_warm(dest: &Path, rows: usize, digest_hex: &str) -> Option<PathBuf> {
    if !dest.exists() {
        return None;
    }
    let uri = dest.to_string_lossy().into_owned();
    let ds = match lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
    {
        Ok(ds) => ds,
        Err(e) => {
            tracing::warn!(path = %dest.display(), error = %e, "osm lance: existing dataset unreadable; reconverting");
            return None;
        }
    };
    let got_rows = match ds.count_rows(None).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(path = %dest.display(), error = %e, "osm lance: count_rows failed; reconverting");
            return None;
        }
    };
    let got_digest = ds.schema().metadata.get(K_DIGEST).cloned();
    // Single-fragment is a serving precondition, not a preference: the
    // mmap+offset read path is only sound over ONE data file holding the
    // whole row column ("a split changes what an mmap offset means" —
    // lance-graph's own soa_verbatim test). A multi-fragment dataset (the
    // pre-fix default split Berlin's 2.5M rows into three) must be
    // reconverted even though rows and digest match.
    let fragments = ds.get_fragments().len();
    if got_rows == rows && got_digest.as_deref() == Some(digest_hex) && fragments == 1 {
        Some(dest.to_path_buf())
    } else {
        tracing::warn!(
            path = %dest.display(), got_rows, want_rows = rows,
            got_digest = ?got_digest, want_digest = digest_hex, fragments,
            "osm lance: existing dataset is stale (row count, slab digest, or \
             fragment count mismatch); reconverting"
        );
        None
    }
}

/// The write itself — `lance-graph`'s `soa_to_lance` example recipe,
/// local-uri only (this module never targets `s3://`; see the module doc).
/// By the time this runs `dest` is always ABSENT ([`ensure_lance_local`]
/// removes a stale dataset outright rather than overwriting it in place —
/// `Overwrite` keeps the prior version's fragment files on disk, which
/// breaks the sole-data-file serving precondition). `WriteMode::Overwrite`
/// is kept as belt-and-braces over `Create`: on a fresh path the two are
/// identical, and if a half-removed directory ever survives, replacing
/// beats erroring out and leaving it. [`reopen_if_warm`] already handled
/// the "already correct" case, so this is never a rewrite of good data.
async fn write_lance(
    dest: &Path,
    map: std::sync::Arc<memmap2::Mmap>,
    rows: usize,
    digest_hex: &str,
    slab_path: &Path,
    mtime_nanos: Option<u128>,
    len: u64,
) -> Option<PathBuf> {
    let field = Field::new(
        ROW_COLUMN,
        DataType::FixedSizeBinary(NODE_ROW_STRIDE as i32),
        false,
    )
    .with_metadata(HashMap::from([(
        // Inert at this stride (512 ≥ the 256-byte mini-block cutoff ⇒
        // full-zip ⇒ never consulted) — kept as a defensive pin, matching
        // `soa_to_lance`'s own module doc on why this is a backstop and not
        // the actual mechanism that keeps the column verbatim.
        "lance-encoding:compression".to_string(),
        "none".to_string(),
    )]));
    let mut schema_meta = HashMap::from([
        (K_LAYOUT.to_string(), ENVELOPE_LAYOUT_VERSION.to_string()),
        (K_STRIDE.to_string(), NODE_ROW_STRIDE.to_string()),
        (
            K_CARVING.to_string(),
            format!("key:0..16|edges:16..32|value:32..{NODE_ROW_STRIDE}"),
        ),
        (K_ENDIAN.to_string(), "le".to_string()),
        (K_CLASSID.to_string(), OSM_BAKE_CLASSID.to_string()),
        (K_DIGEST.to_string(), digest_hex.to_string()),
        (
            K_SOURCE.to_string(),
            format!("{} rows={rows}", slab_path.display()),
        ),
    ]);
    // The hot-but-idle fast-path identity — only written when the platform
    // actually reported an mtime (`ensure_lance_local`'s `slab_mtime_nanos`
    // returned `Some`). Absent here means `reopen_if_unchanged` can never
    // fast-path THIS dataset, which is a correctness-preserving degrade
    // (see that function's doc comment), never a hard failure.
    if let Some(mtime_nanos) = mtime_nanos {
        schema_meta.insert(K_SLAB_MTIME.to_string(), mtime_nanos.to_string());
        schema_meta.insert(K_SLAB_LEN.to_string(), len.to_string());
    }
    let schema = Arc::new(Schema::new_with_metadata(vec![field], schema_meta));

    // The Arrow i32 row ceiling is enforced by `ensure_lance_local` BEFORE
    // the slab is read, so an oversized region never reaches this function.
    // The check is not repeated here: two copies of the same arithmetic in
    // two places is how they drift apart. `try_new` below is the backstop
    // that keeps this function honest regardless.
    //
    // Splitting into multiple Arrow batches was considered and rejected: the
    // read path's mmap+offset serving requires the row column to be ONE
    // contiguous byte run in ONE data file (`locate_row_column`'s own doc,
    // check 1 and check 4 — the tail-anchor verification exists BECAUSE a
    // fragmented layout is unsafe to address by raw offset). Multiple
    // batches risk landing non-contiguously on Lance's own disk layout,
    // which the tail anchor would then catch — but only after paying a
    // real design/verification cost this incident's urgency doesn't afford.
    // Skipping the optimization for oversized regions is strictly safer and
    // already fully supported.

    // Zero-copy import over the MAPPING: Arrow addresses the slab's own pages
    // rather than a heap copy of them, asserted by pointer identity below and
    // not merely trusted (same discipline `soa_to_lance` uses).
    let before = map.as_ptr();
    // SAFETY: `ptr`/`len` describe the whole read-only mapping, and the `Arc`
    // handed to `from_custom_allocation` keeps it alive for exactly as long as
    // the `Buffer` — so the pages cannot be unmapped underneath Arrow. The
    // artifact itself is immutable: it is verified by digest before this
    // point and never written after publication.
    let buffer = unsafe {
        arrow::buffer::Buffer::from_custom_allocation(
            std::ptr::NonNull::new_unchecked(map.as_ptr() as *mut u8),
            map.len(),
            map.clone(),
        )
    };
    let array = match FixedSizeBinaryArray::try_new(NODE_ROW_STRIDE as i32, buffer, None) {
        Ok(a) => a,
        Err(e) => {
            // Defense-in-depth below the row-count bound checked above: ANY
            // other Arrow validation failure degrades the same way, never a
            // panic. `try_new`, never `new` — `new` is `try_new(..).unwrap()`
            // and this function exists specifically to stop doing that.
            tracing::error!(error = %e, "osm lance: building the row array failed");
            return None;
        }
    };
    debug_assert_eq!(
        array.value_data().as_ptr(),
        before,
        "the Buffer must address the MAPPING itself — a moved address means a copy crept in"
    );
    let batch = match RecordBatch::try_new(schema.clone(), vec![Arc::new(array)]) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "osm lance: building the record batch failed");
            return None;
        }
    };

    let uri = dest.to_string_lossy().into_owned();
    let params = WriteParams {
        mode: WriteMode::Overwrite,
        // ONE fragment, whatever the row count. The defaults (1M rows /
        // 90 GiB per file) split Berlin's 2.5M rows across three data
        // files, and a split breaks the mmap+offset serving contract —
        // the exact production outage this line exists to prevent.
        // `max_bytes_per_file` is raised alongside because either limit
        // alone can force a split.
        max_rows_per_file: rows.max(1),
        max_bytes_per_file: (rows.max(1)) * NODE_ROW_STRIDE + (64 << 20),
        ..Default::default()
    };
    if let Err(e) = Dataset::write(
        arrow::array::RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema),
        &uri,
        Some(params),
    )
    .await
    {
        tracing::error!(path = %dest.display(), error = %e, "osm lance: write failed");
        return None;
    }

    // ── Evict the bake from RAM, immediately ────────────────────────────
    //
    // The dataset is on disk now; the slab's pages have done their job. Left
    // alone they simply STAY — a 24 GB memory limit over a 1.4 GB file means
    // nothing ever creates the pressure that would reclaim them, so the
    // conversion's footprint would sit in the bill until the process exits.
    // At Railway's $10/GB-month that is ~$14/month for Berlin and ~$37 for
    // Brandenburg, charged for pages nobody is reading.
    //
    // `drop` first, because the advice cannot apply while Arrow still holds
    // the `Buffer` that owns the mapping: the batch was moved into the writer
    // above and is already gone, and `map` is the last strong reference.
    release_after_write(map, dest);

    tracing::info!(path = %dest.display(), rows, "osm lance: converted");
    Some(dest.to_path_buf())
}

/// Reachability counter for [`release_after_write`], test builds only.
///
/// `release_after_write`'s own return value already proves the MECHANISM
/// (evict when sole holder, decline otherwise) — see
/// `the_slab_mapping_is_released_only_when_nothing_else_holds_it` below.
/// What that test cannot prove is whether a given CALL SITE inside
/// `ensure_lance_local` actually reaches it: the warm-reopen branch used to
/// silently skip the call entirely, and process RSS cannot tell "evicted"
/// from "just dropped, kernel unmapped it anyway" apart (`Drop`'s `munmap`
/// removes this process's page-table entries either way — the difference
/// `MADV_DONTNEED` makes is to the kernel's PAGE CACHE / cgroup memcg
/// charge, which is invisible to `/proc/self/statm` and unavailable in
/// this sandbox at all: `/sys/fs/cgroup/memory.current` does not exist
/// here). This counter is the honest substitute: it proves the call site
/// is reached, which is exactly what regressed.
#[cfg(test)]
static EVICTIONS_ATTEMPTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Release the slab mapping once the dataset is written, if we are the last
/// holder of it.
///
/// Returns whether the release actually happened, so a test can tell "evicted"
/// from "someone else still had it" — the difference decides whether this
/// whole path does anything, and a version that silently never fires would
/// look identical from the outside.
///
/// A surviving reference is NOT an error: it means something downstream still
/// reads these pages, and yanking them would be wrong. It is logged rather
/// than evicted-anyway or passed over in silence.
fn release_after_write(map: std::sync::Arc<memmap2::Mmap>, dest: &Path) -> bool {
    #[cfg(test)]
    EVICTIONS_ATTEMPTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match std::sync::Arc::try_unwrap(map) {
        Ok(m) => {
            advise_dontneed(&m, dest);
            true
        }
        Err(still_shared) => {
            tracing::debug!(
                refs = std::sync::Arc::strong_count(&still_shared),
                "osm lance: slab mapping still referenced; skipping eviction"
            );
            false
        }
    }
}

/// Tell the kernel the slab's pages are no longer wanted.
///
/// `MADV_DONTNEED` on a read-only file mapping drops the mapping's page-table
/// entries; the underlying page-cache pages stay valid (they can be re-read
/// from the file at any time) but stop being mapped, which is what lets them
/// be reclaimed instead of counting as this process's resident set forever.
///
/// **What it does NOT promise:** that the bytes leave the page cache the
/// instant this returns. It is a hint, and the honest claim is "stops holding
/// them", not "guarantees they are gone". The serving path re-faults whatever
/// it actually touches, which is the point — pages tracking the query rather
/// than the dataset.
///
/// Unix-only by nature; elsewhere the conversion simply keeps today's
/// behaviour rather than pretending to evict.
#[cfg(unix)]
fn advise_dontneed(map: &memmap2::Mmap, dest: &Path) {
    match map.advise(memmap2::Advice::DontNeed) {
        Ok(()) => tracing::info!(
            path = %dest.display(),
            mapped_bytes = map.len(),
            "osm lance: released the slab mapping after conversion"
        ),
        // A refused hint is not a failure of the conversion — the dataset is
        // already written. Report it, do not fail on it.
        Err(e) => tracing::warn!(error = %e, "osm lance: could not release the slab mapping"),
    }
}

#[cfg(not(unix))]
fn advise_dontneed(_map: &memmap2::Mmap, _dest: &Path) {}

/// How far into a Lance data file to search for row 0's bytes. Generous
/// margin over Lance's own header overhead (KB-scale) — row 0 sits near
/// the start of the file, never gigabytes in, so this keeps even a cold
/// search bounded and fast rather than reading a multi-GiB file whole
/// (which is what `soa_to_lance`'s own one-shot diagnostic does — fine for
/// a manual check, not for something run on every boot).
const SEARCH_WINDOW: usize = 8 << 20; // 8 MiB

/// Locate the Lance dataset's on-disk data file holding the row column
/// verbatim, returning the exact `(offset, length)` of that run. This is
/// lance-graph's proven P-CACHE-4 mechanism (`tests/soa_verbatim.rs`)
/// productionized — every check below is one of that test's own
/// assertions, and skipping ANY of them has already caused a production
/// outage (the pre-fix version skipped all four and served 400 on every
/// tile):
///
/// 1. **Exactly one data file.** *"a split changes what an mmap offset
///    means"* (`sole_data_file`). A multi-fragment dataset holds only part
///    of the slab per file; an offset into one fragment cannot address the
///    whole column.
/// 2. **Row 0 found near the file head** — the same bounded probe as
///    before, but now merely the first anchor, never the whole proof.
/// 3. **The run starts stride-aligned** (`off % NODE_ROW_STRIDE == 0`).
///    The mmap base is page-aligned, so this is what makes
///    `RowSlab::rows()`'s 64-byte-alignment projection available over the
///    mapped slice.
/// 4. **The LAST row anchors too**: the slab's final 512 bytes must sit at
///    `off + length - 512` inside the data file, and the file must be at
///    least `off + length` long. A coincidental row-0 collision cannot
///    also match the tail at the exact computed distance, and a fragment
///    holding only a prefix of the slab fails the bounds check outright.
///
/// `None` on any doubt — this path is a pure optimization over serving
/// from the raw `.soa` file, never a hard requirement.
#[must_use]
pub fn locate_row_column(dataset_dir: &Path, slab_path: &Path) -> Option<(PathBuf, usize, usize)> {
    use std::io::{Read, Seek, SeekFrom};

    let length = std::fs::metadata(slab_path).ok()?.len() as usize;
    if length == 0 || !length.is_multiple_of(NODE_ROW_STRIDE) {
        return None;
    }
    let mut slab = std::fs::File::open(slab_path).ok()?;
    let mut first_row = vec![0u8; NODE_ROW_STRIDE];
    slab.read_exact(&mut first_row).ok()?;
    let mut last_row = vec![0u8; NODE_ROW_STRIDE];
    slab.seek(SeekFrom::Start((length - NODE_ROW_STRIDE) as u64))
        .ok()?;
    slab.read_exact(&mut last_row).ok()?;

    // (1) Exactly one .lance data file.
    let data_dir = dataset_dir.join("data");
    let mut data_files: Vec<PathBuf> = std::fs::read_dir(&data_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lance"))
        .collect();
    if data_files.len() != 1 {
        tracing::warn!(
            dir = %data_dir.display(), count = data_files.len(),
            "osm lance: dataset has {} data files, need exactly 1 for mmap serving; \
             the raw .soa slab keeps serving the map",
            data_files.len()
        );
        return None;
    }
    let p = data_files.pop().expect("len checked");

    // (2) Row 0 near the head.
    let Some(off) = search_head_for_probe(&p, &first_row) else {
        tracing::warn!(
            file = %p.display(),
            "osm lance: row 0 not found verbatim in the data file head; \
             the raw .soa slab keeps serving the map"
        );
        return None;
    };

    // (3) Stride-aligned start.
    if !off.is_multiple_of(NODE_ROW_STRIDE) {
        tracing::warn!(
            file = %p.display(), offset = off,
            "osm lance: row run starts unaligned to the row stride; \
             the raw .soa slab keeps serving the map"
        );
        return None;
    }

    // (4) Bounds + tail anchor.
    let file_len = std::fs::metadata(&p).ok()?.len() as usize;
    let end = off.checked_add(length)?;
    if end > file_len {
        tracing::warn!(
            file = %p.display(), offset = off, length, file_len,
            "osm lance: data file is shorter than offset + slab length \
             (a fragment holding only part of the column); \
             the raw .soa slab keeps serving the map"
        );
        return None;
    }
    let mut tail = vec![0u8; NODE_ROW_STRIDE];
    let mut data = std::fs::File::open(&p).ok()?;
    data.seek(SeekFrom::Start((end - NODE_ROW_STRIDE) as u64))
        .ok()?;
    data.read_exact(&mut tail).ok()?;
    if tail != last_row {
        tracing::warn!(
            file = %p.display(), offset = off, length,
            "osm lance: the slab's last row does not anchor at offset + length; \
             the head match was coincidental; the raw .soa slab keeps serving the map"
        );
        return None;
    }

    tracing::info!(
        file = %p.display(), offset = off, length,
        "osm lance: row column verified verbatim inside the Lance data file \
         (sole fragment, aligned start, head + tail anchors)"
    );
    Some((p, off, length))
}

/// The bounded byte search itself, split out so it can be unit-tested
/// without a real Lance dataset on disk.
fn search_head_for_probe(file_path: &Path, probe: &[u8]) -> Option<usize> {
    use std::io::Read;
    let mut file = std::fs::File::open(file_path).ok()?;
    let mut head = Vec::with_capacity(SEARCH_WINDOW);
    file.by_ref()
        .take(SEARCH_WINDOW as u64)
        .read_to_end(&mut head)
        .ok()?;
    head.windows(probe.len()).position(|w| w == probe)
}

#[cfg(test)]
mod tests {
    use super::*;
    // `len()` on a FixedSizeBinaryArray comes from the `Array` trait, not
    // from the struct — the production code never calls it, so the import
    // is test-only. Worth naming: `cargo check` does NOT compile
    // `#[cfg(test)]` blocks, so a missing import here passes a check run
    // and only fails under `cargo test`.
    use arrow::array::Array;

    /// The classid constant must stay the canon's dormant-default value —
    /// this is a documentation-of-intent test, not a behavioural one: a
    /// change here should be a deliberate classid mint, not a typo.
    #[test]
    fn the_classid_is_the_canonical_dormant_default() {
        assert_eq!(OSM_BAKE_CLASSID, "00000000");
    }

    /// `reopen_if_warm` must decline when nothing is there yet — the base
    /// case a cold volume hits on every first boot.
    #[tokio::test]
    async fn reopen_if_warm_declines_a_missing_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("nope.lance");
        assert!(reopen_if_warm(&dest, 10, "deadbeef").await.is_none());
    }

    /// The fast path's own base case — the same "nothing there yet" boundary
    /// [`reopen_if_warm`] has above, for the identity-only precondition.
    #[tokio::test]
    async fn reopen_if_unchanged_declines_a_missing_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("nope.lance");
        assert!(reopen_if_unchanged(&dest, 10, 12345, 5120).await.is_none());
    }

    /// The exact boundary this incident crossed: Brandenburg's real
    /// `rows=7_330_219` PANICKED the process at startup
    /// (`arrow-array-58.3.0/src/array/fixed_size_binary_array.rs:106`,
    /// `value size 512 * length 7330219 exceeds maximum valid offset of
    /// 2147483647`) instead of returning `None`, even though the caller in
    /// `main.rs` already has a fully safe `None` arm ("the raw .soa slab
    /// keeps serving the map").
    ///
    /// Drives `ensure_lance_local` — the real entry point — NOT `write_lance`.
    /// The first version of this test called `write_lance` directly, which
    /// would now pass while proving nothing: the guard that matters lives
    /// upstream, and a test that skips it cannot catch the guard being
    /// mispositioned (which is exactly the defect review caught).
    ///
    /// Uses a real oversized slab file rather than a stub, because the
    /// property under test is that the ceiling is decided from the row count
    /// ALONE. A sparse file makes 3.75 GB of apparent length cost no real
    /// disk, so this stays cheap while remaining a genuine end-to-end check.
    #[tokio::test]
    async fn oversized_slab_declines_without_reading_or_destroying_anything() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("brandenburg.soa");

        // Sparse file: Brandenburg's exact byte length, ~no blocks allocated.
        let real_incident_rows = 7_330_219u64;
        let len = real_incident_rows * NODE_ROW_STRIDE as u64;
        let f = std::fs::File::create(&slab_path).expect("create slab");
        f.set_len(len).expect("set sparse length");
        drop(f);

        // A pre-existing dataset dir that MUST survive: the mispositioned
        // guard deleted this before declining, which is the destructive half
        // of the defect and is invisible to a row-count-only assertion.
        let dest = slab_path.with_extension("lance");
        std::fs::create_dir_all(dest.join("data")).expect("mkdir dataset");
        std::fs::write(dest.join("data").join("keepme.lance"), b"pre-existing")
            .expect("write sentinel");

        let got = ensure_lance_local(&slab_path).await;
        assert!(
            got.is_none(),
            "must return None, not panic, at the row count that crashed production"
        );
        assert!(
            dest.join("data").join("keepme.lance").exists(),
            "declining must not delete an existing dataset — the guard has to run \
             before remove_stale_dataset, not after it"
        );
    }

    /// The two-sided half of the boundary: this module exists to let VALID
    /// bakes through, not merely to reject oversized ones. A guard that
    /// fires unconditionally would pass the test above for the wrong
    /// reason — this proves rows comfortably under the ceiling still reach
    /// (and clear) Arrow's own array construction.
    /// The eviction fires when this is the last holder — and does NOT when it
    /// is not. Both halves, because a `release_after_write` that always
    /// returned `false` (Lance still holding the buffer, say) would be inert
    /// and look exactly like a working one from outside.
    #[test]
    fn the_slab_mapping_is_released_only_when_nothing_else_holds_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tiny.soa");
        std::fs::write(&path, vec![0u8; NODE_ROW_STRIDE * 4]).expect("write slab");
        let file = std::fs::File::open(&path).expect("open");
        // SAFETY: read-only mapping of a file this test just wrote and owns.
        let map = std::sync::Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("mmap"));

        let held = map.clone();
        assert!(
            !release_after_write(map.clone(), dir.path()),
            "a mapping something else still holds must NOT be released"
        );
        drop(held);
        drop(map);

        let file2 = std::fs::File::open(&path).expect("reopen");
        // SAFETY: as above.
        let sole = std::sync::Arc::new(unsafe { memmap2::Mmap::map(&file2) }.expect("mmap"));
        assert!(
            release_after_write(sole, dir.path()),
            "the sole holder must release — otherwise the eviction is inert"
        );
    }

    /// PROBE. The bug this test exists to catch: `ensure_lance_local`'s
    /// WARM path (an already-current Lance dataset, so `reopen_if_warm`
    /// succeeds) mmaps and hashes the FULL slab to decide it is warm — it
    /// has to, that is how it knows — but used to just drop the mapping
    /// afterward with no `MADV_DONTNEED`, unlike the rebuild path's
    /// `release_after_write`. Measured in production (Railway's memory
    /// graph, 2026-08-15/16): a redeploy against an already-warm dataset
    /// held the slab's pages resident for 1-3.5 HOURS before the kernel's
    /// own passive reclaim finally cleared them — nothing forces reclaim
    /// under a 24 GB limit over a ~1.3 GB file, since that never creates
    /// real memory pressure.
    ///
    /// This is a reachability probe, not a memory measurement, and that is
    /// a deliberate downgrade from a first attempt at this test: a version
    /// that measured `/proc/self/statm` RSS before/after a warm reopen
    /// PASSED even with the fix reverted, because `Drop`'s `munmap` already
    /// removes this process's page-table entries regardless of whether
    /// `MADV_DONTNEED` ran — RSS cannot see the difference. The actual
    /// production effect is in the kernel's page cache / cgroup memcg
    /// charge, which `/sys/fs/cgroup/memory.current` reports and this
    /// sandbox does not expose at all. `EVICTIONS_ATTEMPTED` proves the
    /// one fact this environment CAN prove: that the warm branch reaches
    /// `release_after_write` at all, which is exactly what regressed.
    ///
    /// Drives `ensure_lance_local` — the real entry point — twice: a cold
    /// build (1 attempt, already covered on its own by
    /// `the_slab_mapping_is_released_only_when_nothing_else_holds_it`),
    /// then a reopen that must land on the DIGEST-based `reopen_if_warm`
    /// branch (must add exactly 1 more).
    ///
    /// **Updated by the "hot but idle" fast path, honestly, not silently.**
    /// This test originally reopened the SAME untouched slab for its second
    /// call — which was the only warm path that existed at the time. Since
    /// `reopen_if_unchanged` landed, that exact scenario (unchanged mtime
    /// AND length) is now caught by the FASTER fast path, which never mmaps
    /// the slab at all and therefore has nothing to evict —
    /// `a_hot_boot_skips_the_full_slab_hash_entirely` covers that case
    /// directly. This test now touches the slab's mtime before the second
    /// call (content byte-identical) specifically to DEFEAT the fast path,
    /// so it keeps exercising the branch it was written for: the
    /// digest-based `reopen_if_warm` path still reached whenever the fast
    /// path can't apply. Mirrors `a_touched_mtime_declines_the_fast_path_
    /// but_the_slow_path_still_succeeds`'s setup, which proves the digest
    /// path is reached at all; this test proves eviction still happens once
    /// it is.
    #[tokio::test]
    async fn the_warm_reopen_path_attempts_eviction_too() {
        let rows = 4usize;
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("warm.soa");
        std::fs::write(&slab_path, vec![0u8; rows * NODE_ROW_STRIDE]).expect("write slab");

        let before = EVICTIONS_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        let first = ensure_lance_local(&slab_path).await;
        assert!(first.is_some(), "cold conversion must succeed");
        let after_cold = EVICTIONS_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_cold - before,
            1,
            "the cold/rebuild path must attempt eviction exactly once"
        );

        // Defeat the fast path (see the doc comment above): same content,
        // different mtime, so `reopen_if_unchanged` declines and the call
        // falls through to the digest-based `reopen_if_warm` — the branch
        // this test exists to cover.
        let touched = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        std::fs::File::open(&slab_path)
            .expect("reopen")
            .set_modified(touched)
            .expect("set_modified");

        let second = ensure_lance_local(&slab_path).await;
        assert_eq!(
            second, first,
            "an unchanged slab must reopen the SAME dataset warm"
        );
        let after_warm = EVICTIONS_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_warm - after_cold,
            1,
            "the warm-reopen path must ALSO attempt eviction — this is the regression: \
             it used to silently drop the mapping without ever calling \
             `release_after_write`, leaving the slab's pages resident with nothing to \
             reclaim them until the kernel's own passive reclaim eventually noticed"
        );
    }

    /// **The "hot but idle" falsifier.** #135/#136 stopped this warm path
    /// from LEAVING the slab resident; this proves it now skips the
    /// mmap+hash of the slab ENTIRELY on a genuinely unchanged boot — the
    /// operator's own framing ("why don't you wire volume01 as hot but idle
    /// … instead of what appears to be … pulling it into RAM").
    ///
    /// A cold conversion must still hash once (there is nothing to trust
    /// yet). A second call against the SAME unchanged file — same path,
    /// same bytes, same mtime, nothing touched it in between — must resolve
    /// to the identical dataset WITHOUT the slow path ever running again.
    #[tokio::test]
    async fn a_hot_boot_skips_the_full_slab_hash_entirely() {
        let rows = 4usize;
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("hot.soa");
        std::fs::write(&slab_path, vec![0u8; rows * NODE_ROW_STRIDE]).expect("write slab");

        let before = SLOW_PATH_HASH_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        let first = ensure_lance_local(&slab_path).await;
        assert!(first.is_some(), "cold conversion must succeed");
        let after_cold = SLOW_PATH_HASH_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_cold - before,
            1,
            "a cold build has nothing to trust yet — it must hash exactly once"
        );

        let second = ensure_lance_local(&slab_path).await;
        assert_eq!(
            second, first,
            "an unchanged slab must resolve to the SAME dataset"
        );
        let after_hot = SLOW_PATH_HASH_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_hot, after_cold,
            "a hot boot (identical mtime+len) must NOT re-enter the mmap+hash path at \
             all — this is the whole point of the fast path: on a genuinely unchanged \
             slab, zero bytes of it are ever touched"
        );
    }

    /// The correctness twin: when the fast path genuinely CANNOT trust the
    /// dataset (here, the slab's mtime was touched — e.g. a redundant
    /// re-download landed byte-identical content with a fresh timestamp),
    /// it must decline and fall through to the slow path, which must still
    /// reach the right answer (this dataset is warm) via content digest —
    /// not silently serve something wrong, and not panic.
    #[tokio::test]
    async fn a_touched_mtime_declines_the_fast_path_but_the_slow_path_still_succeeds() {
        let rows = 4usize;
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("touched.soa");
        std::fs::write(&slab_path, vec![0u8; rows * NODE_ROW_STRIDE]).expect("write slab");

        let first = ensure_lance_local(&slab_path).await;
        assert!(first.is_some(), "cold conversion must succeed");
        let after_cold = SLOW_PATH_HASH_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        // Touch mtime forward without changing a single byte of content.
        let touched = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        std::fs::File::open(&slab_path)
            .expect("reopen")
            .set_modified(touched)
            .expect("set_modified");

        let second = ensure_lance_local(&slab_path).await;
        assert_eq!(
            second, first,
            "content is unchanged, so the slow path's digest match must still \
             resolve to the SAME dataset"
        );
        let after_touch = SLOW_PATH_HASH_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_touch - after_cold,
            1,
            "the fast path must decline on a touched mtime — it is not entitled to \
             trust identity that no longer matches — and the slow path must be the \
             one that recovers the correct (warm) answer"
        );
    }

    #[test]
    fn a_row_count_under_the_ceiling_builds_a_valid_array() {
        let rows = 1_000usize; // Berlin-scale, nowhere near the 4.19M cap
        let bytes = vec![0u8; rows * NODE_ROW_STRIDE];
        let array = FixedSizeBinaryArray::try_new(
            NODE_ROW_STRIDE as i32,
            arrow::buffer::Buffer::from_vec(bytes),
            None,
        );
        assert!(array.is_ok(), "an ordinary row count must not be rejected");
        assert_eq!(array.unwrap().len(), rows);
    }

    /// Both sides of the ceiling, through the PRODUCTION predicate rather
    /// than a re-derivation of its formula.
    ///
    /// The earlier version of this test recomputed `i32::MAX / STRIDE` and
    /// asserted the result equalled 4,194,303 — which proves the arithmetic
    /// agrees with itself and nothing about the shipped guard. Deleting the
    /// guard entirely would have left it green. This calls
    /// `row_count_fits_arrow_array`, so it fails if the real predicate ever
    /// stops rejecting oversized counts, and it costs no allocation.
    #[test]
    fn the_row_ceiling_predicate_accepts_the_last_row_and_rejects_the_first_over() {
        let max = arrow_max_rows_per_array();
        assert_eq!(
            max, 4_194_303,
            "512-byte stride under Arrow's i32 byte ceiling"
        );

        assert!(
            row_count_fits_arrow_array(max),
            "the last fitting row count must be accepted"
        );
        assert!(
            !row_count_fits_arrow_array(max + 1),
            "one row past the ceiling must be rejected — this is the exact boundary, \
             and an off-by-one here is a panic in production"
        );

        // The two real regions, on the sides the incident put them.
        assert!(
            row_count_fits_arrow_array(2_766_291),
            "Berlin must still take the fast path"
        );
        assert!(
            !row_count_fits_arrow_array(7_330_219),
            "Brandenburg's real row count is what crashed production"
        );
    }

    /// The falsifier for [`search_head_for_probe`]: a real hit, surrounded
    /// by bytes that must NOT be mistaken for it. Distinct leading/trailing
    /// padding — same byte would let a false match at the wrong offset
    /// still pass by accident.
    #[test]
    fn search_head_for_probe_finds_the_exact_offset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("frag.lance");
        let probe = vec![0xABu8; 512];
        let mut content = vec![0x11u8; 200]; // Lance's own leading header bytes
        content.extend_from_slice(&probe);
        content.extend(vec![0x22u8; 300]); // trailing footer/metadata bytes
        std::fs::write(&file_path, &content).expect("write");

        assert_eq!(search_head_for_probe(&file_path, &probe), Some(200));
    }

    /// …and the silent twin: bytes that never contain the probe must
    /// return `None`, not a spurious offset.
    #[test]
    fn search_head_for_probe_declines_when_the_probe_is_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("frag.lance");
        std::fs::write(&file_path, vec![0x11u8; 4096]).expect("write");
        let probe = vec![0xABu8; 512];

        assert!(search_head_for_probe(&file_path, &probe).is_none());
    }

    /// A stale dataset is removed WHOLE — including the prior version's
    /// fragment files that `WriteMode::Overwrite` would have left behind.
    /// This is the falsifier for the 4-files-in-`data/` failure: overwrite
    /// the pre-fix three-fragment dataset in place and `locate_row_column`'s
    /// sole-data-file check falls back to the raw `.soa` forever.
    #[test]
    fn remove_stale_dataset_clears_old_fragments_entirely() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dataset_dir = dir.path().join("region.lance");
        let data_dir = dataset_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("mkdir");
        // The pre-fix shape: three fragment files plus Lance's own
        // bookkeeping directories.
        for name in ["frag-0.lance", "frag-1.lance", "frag-2.lance"] {
            std::fs::write(data_dir.join(name), vec![0x77u8; 1024]).expect("write");
        }
        std::fs::create_dir_all(dataset_dir.join("_versions")).expect("mkdir");

        remove_stale_dataset(&dataset_dir).expect("removal succeeds");
        assert!(
            !dataset_dir.exists(),
            "the stale dataset directory must be GONE, so the rewrite starts from nothing"
        );
    }

    /// A synthetic slab whose rows are DISTINGUISHABLE — row `i`'s bytes are
    /// a function of `i` — so a tail anchor genuinely proves the last row is
    /// where it is claimed to be, rather than matching any repeated filler.
    fn distinct_slab(rows: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; rows * NODE_ROW_STRIDE];
        for (i, chunk) in bytes
            .as_chunks_mut::<NODE_ROW_STRIDE>()
            .0
            .iter_mut()
            .enumerate()
        {
            for (j, b) in chunk.iter_mut().enumerate() {
                *b = ((i * 31 + j * 7) % 251) as u8;
            }
        }
        bytes
    }

    /// Lay out `<dataset>/data/` the way `Dataset::write` does, with the slab
    /// embedded at a chosen (stride-aligned) offset in a sole fragment.
    fn fake_dataset(
        dir: &Path,
        slab_bytes: &[u8],
        header: usize,
        footer: usize,
    ) -> (PathBuf, PathBuf) {
        let dataset_dir = dir.join("region.lance");
        let data_dir = dataset_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("mkdir");
        // A non-`.lance` file must be skipped, not mistaken for a fragment.
        std::fs::write(data_dir.join("README.txt"), b"not a fragment").expect("write");
        let mut fragment = vec![0x99u8; header];
        fragment.extend_from_slice(slab_bytes);
        fragment.extend(vec![0x88u8; footer]);
        let frag_path = data_dir.join("frag-0.lance");
        std::fs::write(&frag_path, &fragment).expect("write fragment");
        (dataset_dir, frag_path)
    }

    /// End-to-end over [`locate_row_column`]: sole fragment, aligned start,
    /// head + tail anchors — all four verifications passing together.
    #[test]
    fn locate_row_column_finds_the_fragment_and_reports_the_slabs_own_length() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        let rows = 3usize;
        let slab_bytes = distinct_slab(rows);
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");

        // Header must be stride-aligned for check (3) — a real Lance data
        // file puts the run at offset 0 (measured); 512 exercises off != 0.
        let (dataset_dir, frag_path) = fake_dataset(dir.path(), &slab_bytes, NODE_ROW_STRIDE, 16);

        let (found_path, offset, length) =
            locate_row_column(&dataset_dir, &slab_path).expect("must locate the fragment");
        assert_eq!(found_path, frag_path);
        assert_eq!(offset, NODE_ROW_STRIDE);
        assert_eq!(length, rows * NODE_ROW_STRIDE);
    }

    /// A dataset directory with no matching fragment must decline cleanly —
    /// the optimization is skipped, not panicked into.
    #[test]
    fn locate_row_column_declines_when_no_fragment_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        std::fs::write(&slab_path, distinct_slab(1)).expect("write slab");

        let dataset_dir = dir.path().join("region.lance");
        let data_dir = dataset_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("mkdir");
        std::fs::write(data_dir.join("frag-0.lance"), vec![0x00u8; 4096]).expect("write");

        assert!(locate_row_column(&dataset_dir, &slab_path).is_none());
    }

    /// THE production outage's falsifier: a multi-fragment dataset — each
    /// data file holding only part of the slab — must be declined outright,
    /// even though row 0 IS found verbatim in the first fragment. The
    /// pre-fix version accepted this and reported the full slab length
    /// against a fragment that couldn't hold it; every tile request then
    /// failed with 400.
    #[test]
    fn locate_row_column_declines_a_multi_fragment_split() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        let slab_bytes = distinct_slab(4);
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");

        let dataset_dir = dir.path().join("region.lance");
        let data_dir = dataset_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("mkdir");
        let half = slab_bytes.len() / 2;
        std::fs::write(data_dir.join("frag-0.lance"), &slab_bytes[..half]).expect("write");
        std::fs::write(data_dir.join("frag-1.lance"), &slab_bytes[half..]).expect("write");

        assert!(locate_row_column(&dataset_dir, &slab_path).is_none());
    }

    /// A sole fragment that holds only a PREFIX of the slab (same failure
    /// mode as the split, reached via the bounds check instead of the file
    /// count) must be declined — offset + slab length exceeds the file.
    #[test]
    fn locate_row_column_declines_a_truncated_fragment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        let slab_bytes = distinct_slab(4);
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");

        let half = slab_bytes.len() / 2;
        let (dataset_dir, _) = fake_dataset(dir.path(), &slab_bytes[..half], 0, 0);

        assert!(locate_row_column(&dataset_dir, &slab_path).is_none());
    }

    /// A head match whose TAIL does not anchor must be declined: the file
    /// is long enough, row 0 matches, but the bytes at offset + length - 512
    /// are not the slab's last row — the coincidental-collision case the
    /// two-anchor design exists to kill.
    #[test]
    fn locate_row_column_declines_when_the_tail_does_not_anchor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        let slab_bytes = distinct_slab(3);
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");

        // Fragment = row 0 verbatim, then garbage of the right total size.
        let mut fragment = slab_bytes[..NODE_ROW_STRIDE].to_vec();
        fragment.extend(vec![0xEEu8; slab_bytes.len() - NODE_ROW_STRIDE + 64]);
        let dataset_dir = dir.path().join("region.lance");
        let data_dir = dataset_dir.join("data");
        std::fs::create_dir_all(&data_dir).expect("mkdir");
        std::fs::write(data_dir.join("frag-0.lance"), &fragment).expect("write");

        assert!(locate_row_column(&dataset_dir, &slab_path).is_none());
    }

    /// An unaligned run start must be declined — `RowSlab::rows()` needs the
    /// mapped slice 64-byte aligned, and a page-aligned mmap base plus a
    /// stride-aligned offset is what guarantees it.
    #[test]
    fn locate_row_column_declines_an_unaligned_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slab_path = dir.path().join("region.soa");
        let slab_bytes = distinct_slab(3);
        std::fs::write(&slab_path, &slab_bytes).expect("write slab");

        let (dataset_dir, _) = fake_dataset(dir.path(), &slab_bytes, 100, 16);

        assert!(locate_row_column(&dataset_dir, &slab_path).is_none());
    }
}
