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

    // Digest over an mmap, not `fs::read`: the warm path must not pay a
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
    if map.is_empty() || !map.len().is_multiple_of(NODE_ROW_STRIDE) {
        tracing::error!(
            path = %slab_path.display(), len = map.len(),
            "osm lance: slab is not a whole number of {NODE_ROW_STRIDE}-byte rows; refusing"
        );
        return None;
    }
    let rows = map.len() / NODE_ROW_STRIDE;
    let digest_hex = format!("{:016x}", osm_soa_bake::codebook::hash_slab(&map));
    drop(map);

    if let Some(warm) = reopen_if_warm(&dest, rows, &digest_hex).await {
        tracing::info!(
            path = %dest.display(), rows,
            "osm lance: warm — dataset already matches this slab's row count and digest"
        );
        return Some(warm);
    }

    // Cold path only: the write genuinely needs an owned Vec (adopted by
    // `Buffer::from_vec`), so the slab-sized allocation is paid exactly once
    // per conversion, never per boot.
    let bytes = match std::fs::read(slab_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(path = %slab_path.display(), error = %e, "osm lance: cannot read slab");
            return None;
        }
    };
    tracing::info!(
        path = %dest.display(), rows,
        "osm lance: converting the .soa slab into a Lance dataset (zero-copy import)"
    );
    write_lance(&dest, bytes, rows, &digest_hex, slab_path).await
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
/// `WriteMode::Overwrite` rather than `Create`: [`reopen_if_warm`] already
/// handled the "already correct" case, so by the time this runs `dest` is
/// either absent or holds a STALE dataset that must be replaced, not
/// appended onto.
async fn write_lance(
    dest: &Path,
    bytes: Vec<u8>,
    rows: usize,
    digest_hex: &str,
    slab_path: &Path,
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
    let schema_meta = HashMap::from([
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
    let schema = Arc::new(Schema::new_with_metadata(vec![field], schema_meta));

    // Zero-copy import: `Buffer::from_vec` ADOPTS this allocation rather
    // than copying it — asserted by pointer identity, not merely trusted
    // (same discipline `soa_to_lance` uses; a 1+ GiB slab makes a silent
    // copy here a real cost, not a rounding error).
    let before = bytes.as_ptr();
    let array = FixedSizeBinaryArray::new(
        NODE_ROW_STRIDE as i32,
        arrow::buffer::Buffer::from_vec(bytes),
        None,
    );
    debug_assert_eq!(
        array.value_data().as_ptr(),
        before,
        "Buffer::from_vec must ADOPT the allocation — a moved address means a copy crept in"
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

    tracing::info!(path = %dest.display(), rows, "osm lance: converted");
    Some(dest.to_path_buf())
}

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

    /// A synthetic slab whose rows are DISTINGUISHABLE — row `i`'s bytes are
    /// a function of `i` — so a tail anchor genuinely proves the last row is
    /// where it is claimed to be, rather than matching any repeated filler.
    fn distinct_slab(rows: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; rows * NODE_ROW_STRIDE];
        for (i, chunk) in bytes.chunks_exact_mut(NODE_ROW_STRIDE).enumerate() {
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
