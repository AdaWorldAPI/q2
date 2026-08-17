//! Boot-time conversion of the `.chains` / `.books` OSM sidecars into Lance
//! datasets — extending [`crate::osm_lance`]'s established pattern (the same
//! `lance::dataset` + `lance_graph_contract` recipe already used for the
//! fixed-stride row slab, per `AdaWorldAPI/lance-graph`'s own
//! `soa_to_lance.rs` example) to *variable-length* per-ordinal blobs.
//!
//! # Why this exists
//!
//! `osm_features.rs::open_chains()` / `open_books()` currently do a full
//! eager `std::fs::read()` of the sidecar into an OWNED, permanently-resident
//! `Vec<u8>` / `Vec<String>`, held in a `'static OnceLock` for the process
//! lifetime. Unlike `memmap2::Mmap` (already used correctly by
//! `open_slab()`), an owned heap `Vec` has no page-cache backing — nothing
//! can reclaim it under memory pressure. This module gives chains/books the
//! same treatment `osm_lance.rs` already gives the row slab: converted once
//! at boot into a Lance dataset on the same local volume, read back on
//! demand instead of held resident forever.
//!
//! # Sparse vs. dense ordinals — the addressing split
//!
//! [`Dataset::take`] indexes by **row POSITION** (0-based offset into a
//! single-fragment dataset), never by a stored column value. Writing entries
//! in ascending ordinal order in ONE batch makes row position a
//! *monotonic function of* ordinal — but position == ordinal ONLY when the
//! ordinal space itself is dense (0, 1, 2, … with no gaps):
//!
//! - **Books** (`Books`'s four `IdentityCodebook`s): dense by construction —
//!   `read_book`/`write_book` in `openstreetmap-website-rs`'s `codebook.rs`
//!   assign ordinals `0..n` with no gaps. Row position == ordinal directly;
//!   no extra index needed.
//! - **Chains**: sparse — `.chains` stores an entry only for "tagged ways",
//!   a subset of the full identity-ordinal space (nodes and untagged
//!   elements never appear). Ordinal 42 is not necessarily at row 42; it is
//!   at whatever position it landed after the gaps before it. Looking this
//!   up needs the small, ascending **ordinal index** — a `Vec<u32>` of
//!   *stored* ordinals, position == row index, built once (from the write
//!   entries on a cold boot, or read back from the dataset's `ordinal`
//!   column on a warm one) and binary-searched. At ~4.4M chains this index
//!   is ~17.6 MB — bounded, small, and fine to keep resident; it is the
//!   ~50+ MB of *payload bytes* the current bug pins forever that this
//!   module exists to stop doing.
//!
//! See `claude-notes/plans/2026-08-16-chains-books-lancedb-blob.md` for the
//! full design (including the batched-gather-then-sync-CPU-loop shape
//! `osm_features.rs`'s hot paths need to avoid a per-lookup Lance read).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Array, LargeBinaryArray, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use lance::dataset::{Dataset, WriteMode, WriteParams};

/// Ordinal column name — the STORED (possibly sparse) identity ordinal.
const ORDINAL_COLUMN: &str = "ordinal";
/// The per-ordinal raw bytes, verbatim — undecoded on both the chains and
/// books side, exactly as the sidecar stores them.
const VALUE_COLUMN: &str = "value";

fn dataset_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new(ORDINAL_COLUMN, DataType::UInt32, false),
        Field::new(VALUE_COLUMN, DataType::LargeBinary, true),
    ]))
}

/// Write `entries` — `(ordinal, raw bytes)`, already in ascending-ordinal
/// order — into a fresh, single-fragment Lance dataset at `dest`. Any
/// existing dataset at `dest` is overwritten (one fragment always, mirroring
/// `osm_lance.rs::write_lance`'s reasoning: mixed old/new fragments would
/// break position-addressed reads).
///
/// Returns the written ordinals, in the SAME order they were inserted —
/// i.e. position `i` of the returned `Vec` is the ordinal now sitting at row
/// index `i` of the dataset. A caller with a sparse ordinal space (chains)
/// keeps this as its [`OrdinalIndex`]; a caller with a dense one (books)
/// can discard it (ordinal == row index already).
///
/// # Errors
///
/// Any Lance write failure (propagated, not swallowed — the boot-time
/// caller decides how to degrade).
async fn write_ordinal_blob_dataset(
    dest: &Path,
    entries: &[(u32, Vec<u8>)],
) -> Result<Vec<u32>, lance::Error> {
    let schema = dataset_schema();

    let ordinals: UInt32Array = entries.iter().map(|(o, _)| *o).collect();
    let values: LargeBinaryArray = entries.iter().map(|(_, v)| Some(v.as_slice())).collect();

    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(ordinals), Arc::new(values)])
        .map_err(lance::Error::from)?;

    let uri = dest.to_string_lossy().into_owned();
    let rows = entries.len().max(1);
    let params = WriteParams {
        mode: WriteMode::Overwrite,
        // ONE fragment, whatever the row count — position addressing (see
        // the module doc) requires it, same reasoning as
        // `osm_lance.rs::write_lance`'s row-slab dataset.
        max_rows_per_file: rows,
        ..Default::default()
    };
    Dataset::write(
        arrow::array::RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema),
        &uri,
        Some(params),
    )
    .await?;

    Ok(entries.iter().map(|(o, _)| *o).collect())
}

/// The sparse-ordinal lookup: which row index holds a given (possibly
/// absent) ordinal, for a dataset written by [`write_ordinal_blob_dataset`].
///
/// Bounded and small by design — see the module doc's "Sparse vs. dense"
/// section for why this is the resident structure and the payload bytes are
/// not.
pub struct OrdinalIndex {
    /// STORED ordinals, ascending, position == row index. Never the full
    /// identity-ordinal space — only what this dataset actually has a row
    /// for.
    sorted_ordinals: Vec<u32>,
}

impl OrdinalIndex {
    /// # Panics
    ///
    /// Debug-asserts `sorted_ordinals` is strictly ascending — a caller
    /// building this from anything other than a single ordinal-ordered
    /// write (this module's only writer) has violated the precondition
    /// binary search depends on.
    pub fn new(sorted_ordinals: Vec<u32>) -> Self {
        debug_assert!(
            sorted_ordinals.windows(2).all(|w| w[0] < w[1]),
            "OrdinalIndex requires strictly ascending, deduplicated ordinals"
        );
        Self { sorted_ordinals }
    }

    /// The row index holding `ordinal`, or `None` if this dataset has no
    /// row for it (a real, expected answer — not every identity ordinal
    /// has a chain).
    pub fn row_index(&self, ordinal: u32) -> Option<u64> {
        self.sorted_ordinals
            .binary_search(&ordinal)
            .ok()
            .map(|i| i as u64)
    }
}

/// Fetch the raw bytes at `row_indices` from a dataset written by
/// [`write_ordinal_blob_dataset`], via ONE `Dataset::take` call — never one
/// call per row (see the module doc's batched-gather rationale). Returned
/// keyed by the STORED ordinal (read back from the `ordinal` column of the
/// fetched rows), not by the input row index, so a caller never has to
/// re-derive which ordinal a given row corresponds to.
///
/// A null value at a fetched row (should not occur for chains/books today,
/// since every write entry carries `Some` bytes — but the schema allows
/// nulls, so this is handled rather than assumed away) is simply absent
/// from the returned map.
///
/// # Errors
///
/// Any Lance read failure (propagated).
pub async fn take_by_row_index(
    dataset: &Dataset,
    row_indices: &[u64],
) -> Result<HashMap<u32, Vec<u8>>, lance::Error> {
    if row_indices.is_empty() {
        return Ok(HashMap::new());
    }
    let projection = dataset.schema().clone();
    let batch = dataset.take(row_indices, projection).await?;

    let ordinal_col = batch
        .column_by_name(ORDINAL_COLUMN)
        .and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
    let value_col = batch
        .column_by_name(VALUE_COLUMN)
        .and_then(|c| c.as_any().downcast_ref::<LargeBinaryArray>());

    let (Some(ordinal_col), Some(value_col)) = (ordinal_col, value_col) else {
        return Ok(HashMap::new());
    };

    let mut out = HashMap::with_capacity(batch.num_rows());
    for i in 0..batch.num_rows() {
        if value_col.is_null(i) {
            continue;
        }
        out.insert(ordinal_col.value(i), value_col.value(i).to_vec());
    }
    Ok(out)
}

/// Fetch the raw bytes for a batch of (possibly sparse) `ordinals` in ONE
/// Lance read, translating through `index` first.
///
/// # Errors
///
/// Any Lance read failure (propagated).
pub async fn take_by_ordinal_sparse(
    dataset: &Dataset,
    index: &OrdinalIndex,
    ordinals: &[u32],
) -> Result<HashMap<u32, Vec<u8>>, lance::Error> {
    let row_indices: Vec<u64> = ordinals.iter().filter_map(|o| index.row_index(*o)).collect();
    take_by_row_index(dataset, &row_indices).await
}

/// Where a given book's Lance dataset lives, derived from the `.books`
/// sidecar path the same way [`ensure_ordinal_lance_dest`]'s siblings derive
/// theirs — one extension per book, so all four sit beside the sidecar
/// without a naming collision.
fn book_dataset_path(books_path: &Path, book: &str) -> PathBuf {
    books_path.with_extension(format!("{book}.lance"))
}

/// Boot-time conversion of the `.chains` sidecar into a Lance dataset.
///
/// Reads the (already S3-hydrated, per [`crate::osm_slab_hydrate`]) local
/// `.chains` file, stores each chain's RAW encoded bytes verbatim (never
/// decoded to `Vec<TileXy>` and re-encoded — one wire format, one place that
/// understands it: `osm_soa_bake::chains::Chains::get`/`decode_chain`), and
/// returns the dataset path plus the [`OrdinalIndex`] a sparse-ordinal
/// lookup needs (see the module doc's "Sparse vs. dense" section).
///
/// Degrades to `None` on any failure — a missing/malformed sidecar, or a
/// Lance write failure — mirroring `osm_lance::ensure_lance_local`'s
/// contract: this is a best-effort optimization, never a hard dependency for
/// serving chains (the existing eager `open_chains` path keeps working).
pub async fn ensure_chains_lance_local(chains_path: &Path) -> Option<(PathBuf, OrdinalIndex)> {
    let bytes = match std::fs::read(chains_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %chains_path.display(), error = %e,
                "osm chains lance: cannot read .chains sidecar; skipping Lance conversion"
            );
            return None;
        }
    };
    let chains = match osm_soa_bake::chains::Chains::from_bytes(bytes) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                path = %chains_path.display(), error = ?e,
                "osm chains lance: .chains sidecar is malformed; skipping Lance conversion"
            );
            return None;
        }
    };

    let entries: Vec<(u32, Vec<u8>)> =
        chains.iter().map(|(ordinal, raw)| (ordinal, raw.to_vec())).collect();
    let dest = book_dataset_path(chains_path, "chains");
    match write_ordinal_blob_dataset(&dest, &entries).await {
        Ok(written_ordinals) => {
            tracing::info!(
                path = %dest.display(), count = written_ordinals.len(),
                "osm chains lance: converted .chains sidecar to a Lance dataset"
            );
            Some((dest, OrdinalIndex::new(written_ordinals)))
        }
        Err(e) => {
            tracing::warn!(
                path = %dest.display(), error = %e,
                "osm chains lance: Lance write failed; skipping — the eager .chains \
                 read path keeps serving"
            );
            None
        }
    }
}

/// Boot-time conversion of the `.books` sidecar's four codebooks into four
/// sibling Lance datasets — `identities` / `tag_keys` / `tag_values` /
/// `labels`, one dataset each, since each is its OWN dense `0..len` ordinal
/// space (see the module doc: no [`OrdinalIndex`] needed — row position IS
/// the ordinal for every one of them).
///
/// Returns `[identities, tag_keys, tag_values, labels]` dataset paths, in
/// that order. Degrades to `None` on any failure — same best-effort
/// contract as [`ensure_chains_lance_local`].
pub async fn ensure_books_lance_local(books_path: &Path) -> Option<[PathBuf; 4]> {
    let file = match std::fs::File::open(books_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(
                path = %books_path.display(), error = %e,
                "osm books lance: cannot read .books sidecar; skipping Lance conversion"
            );
            return None;
        }
    };
    let mut reader = std::io::BufReader::new(file);
    let (_header, books) = match osm_soa_bake::codebook::read_books(&mut reader) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                path = %books_path.display(), error = ?e,
                "osm books lance: .books sidecar is malformed; skipping Lance conversion"
            );
            return None;
        }
    };

    let book_kinds: [(&str, &lance_graph_contract::identity_quad::IdentityCodebook); 4] = [
        ("identities", &books.identities),
        ("tag_keys", &books.tag_keys),
        ("tag_values", &books.tag_values),
        ("labels", &books.labels),
    ];

    let mut out = Vec::with_capacity(4);
    for (name, book) in book_kinds {
        let entries: Vec<(u32, Vec<u8>)> = (0..book.len() as u32)
            .filter_map(|ordinal| book.key(ordinal).map(|k| (ordinal, k.as_bytes().to_vec())))
            .collect();
        let dest = book_dataset_path(books_path, name);
        match write_ordinal_blob_dataset(&dest, &entries).await {
            Ok(_) => {
                tracing::info!(
                    path = %dest.display(), count = entries.len(), book = name,
                    "osm books lance: converted a codebook to a Lance dataset"
                );
                out.push(dest);
            }
            Err(e) => {
                tracing::warn!(
                    path = %dest.display(), error = %e, book = name,
                    "osm books lance: Lance write failed; skipping ALL four books — a \
                     partial set (some converted, some not) is worse than none, since a \
                     caller with only 3 of 4 datasets has no way to tell which is missing"
                );
                return None;
            }
        }
    }

    out.try_into().ok()
}

// ── request-scoped gather layer ─────────────────────────────────────
//
// Everything above this point is boot-time (conversion) or general-purpose
// (the take_* primitives). What follows is what actually removes the
// permanent `Vec`/`String` residency `osm_features.rs::open_chains()` /
// `open_books()` hold today: a request calls `gather_chains`/`gather_books`
// with exactly the ordinals it needs, gets back a small owned map that is
// DROPPED at the end of the request, and the dataset handles themselves
// (cheap metadata, not row data — same as `osm_lance.rs`'s own cached
// `Dataset`) are the only thing that lives for the process lifetime.

/// A single codebook's request-scoped lookup: ordinal → owned string. Mirrors
/// `lance_graph_contract::identity_quad::IdentityCodebook::key`'s read shape
/// exactly (`fn key(&self, ordinal: u32) -> Option<&str>`), so call sites
/// written against the resident codebook need no change beyond the type of
/// what they hold.
pub struct Lookup {
    map: HashMap<u32, String>,
}

impl Lookup {
    pub fn key(&self, ordinal: u32) -> Option<&str> {
        self.map.get(&ordinal).map(String::as_str)
    }
}

/// The three dense codebooks the hot request path (`query_feature` /
/// `query_tile_shapes`) actually reads. `labels` is never read by that path
/// (grep-confirmed in `osm_features.rs`), so it is not gathered here — a
/// caller that needs labels reads the eager `Books` singleton for that one
/// field, same as today.
pub struct RequestBooks {
    pub identities: Lookup,
    pub tag_keys: Lookup,
    pub tag_values: Lookup,
}

/// Sparse chain lookup, request-scoped — mirrors
/// `osm_soa_bake::chains::Chains::get`'s `Result<Option<Vec<TileXy>>,
/// ChainError>` shape, so a caller holding a `RequestChains` instead of the
/// resident `Chains` needs no change to its call syntax.
pub struct RequestChains {
    raw: HashMap<u32, Vec<u8>>,
}

impl RequestChains {
    pub fn get(
        &self,
        ordinal: u32,
    ) -> Result<Option<Vec<osm_soa_bake::tms::TileXy>>, osm_soa_bake::chains::ChainError> {
        self.raw
            .get(&ordinal)
            .map(|rec| osm_soa_bake::chains::decode_chain(rec))
            .transpose()
    }
}

static CHAINS_DATASET: tokio::sync::OnceCell<Option<Dataset>> = tokio::sync::OnceCell::const_new();
static CHAINS_INDEX: std::sync::OnceLock<Option<OrdinalIndex>> = std::sync::OnceLock::new();
static IDENTITIES_DATASET: tokio::sync::OnceCell<Option<Dataset>> =
    tokio::sync::OnceCell::const_new();
static TAG_KEYS_DATASET: tokio::sync::OnceCell<Option<Dataset>> = tokio::sync::OnceCell::const_new();
static TAG_VALUES_DATASET: tokio::sync::OnceCell<Option<Dataset>> =
    tokio::sync::OnceCell::const_new();

/// Boot calls this once, right after [`ensure_chains_lance_local`] succeeds —
/// the `OrdinalIndex` is the resident structure the module doc's "Sparse vs.
/// dense" section names; this just gives request-time gather a place to find
/// it without re-parsing the `.chains` file's index a second time on the
/// first request. A no-op if called twice (first write wins), matching every
/// other boot-time `OnceLock` in this crate.
pub fn set_chains_index(index: OrdinalIndex) {
    let _ = CHAINS_INDEX.set(Some(index));
}

async fn open_cached(cell: &'static tokio::sync::OnceCell<Option<Dataset>>, path: &Path) -> Option<&'static Dataset> {
    cell.get_or_init(|| async {
        let path_str = path.to_str()?;
        Dataset::open(path_str).await.ok()
    })
    .await
    .as_ref()
}

/// Gather every requested chain ordinal in ONE batched Lance read into a
/// request-scoped map. `None` when the dataset or the index isn't available
/// (conversion failed, was skipped, or hasn't run yet) — the caller falls
/// back to the eager `Chains` singleton in that case, same fail-open
/// contract as the rest of this module.
pub async fn gather_chains(chains_dataset_path: &Path, ordinals: &[u32]) -> Option<RequestChains> {
    if ordinals.is_empty() {
        return Some(RequestChains { raw: HashMap::new() });
    }
    let index = CHAINS_INDEX.get()?.as_ref()?;
    let dataset = open_cached(&CHAINS_DATASET, chains_dataset_path).await?;
    let raw = take_by_ordinal_sparse(dataset, index, ordinals).await.ok()?;
    Some(RequestChains { raw })
}

async fn gather_lookup(
    cell: &'static tokio::sync::OnceCell<Option<Dataset>>,
    path: &Path,
    ordinals: &[u32],
) -> Option<Lookup> {
    if ordinals.is_empty() {
        return Some(Lookup { map: HashMap::new() });
    }
    let dataset = open_cached(cell, path).await?;
    // Dense addressing: row index == ordinal directly (see the module doc).
    let row_indices: Vec<u64> = ordinals.iter().map(|&o| u64::from(o)).collect();
    let raw = take_by_row_index(dataset, &row_indices).await.ok()?;
    let map = raw
        .into_iter()
        .filter_map(|(ordinal, bytes)| String::from_utf8(bytes).ok().map(|s| (ordinal, s)))
        .collect();
    Some(Lookup { map })
}

/// Gather every requested identity/tag-key/tag-value ordinal in three
/// batched Lance reads (one per codebook) into a request-scoped
/// [`RequestBooks`]. `None` when ANY of the three datasets is unavailable —
/// a partial books set is worse than none, same reasoning as
/// [`ensure_books_lance_local`]'s all-or-nothing write.
pub async fn gather_books(
    identities_path: &Path,
    tag_keys_path: &Path,
    tag_values_path: &Path,
    identity_ordinals: &[u32],
    key_ordinals: &[u32],
    value_ordinals: &[u32],
) -> Option<RequestBooks> {
    let identities = gather_lookup(&IDENTITIES_DATASET, identities_path, identity_ordinals).await?;
    let tag_keys = gather_lookup(&TAG_KEYS_DATASET, tag_keys_path, key_ordinals).await?;
    let tag_values = gather_lookup(&TAG_VALUES_DATASET, tag_values_path, value_ordinals).await?;
    Some(RequestBooks {
        identities,
        tag_keys,
        tag_values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The foundational assumption a DENSE-ordinal caller (books) depends
    /// on: for a fresh, single-fragment, ordinal-ordered write of a
    /// contiguous `0..n` ordinal space, row index == ordinal, so a caller
    /// can skip [`OrdinalIndex`] entirely and pass the ordinal straight to
    /// [`take_by_row_index`]. Measured, not merely documented.
    #[tokio::test]
    async fn dense_ordinals_need_no_index_row_position_is_the_ordinal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("books_test.lance");
        let entries: Vec<(u32, Vec<u8>)> = vec![
            (0, b"alpha".to_vec()),
            (1, b"bravo".to_vec()),
            (2, b"charlie".to_vec()),
        ];
        write_ordinal_blob_dataset(&dest, &entries)
            .await
            .expect("write");
        let dataset = Dataset::open(dest.to_str().unwrap()).await.expect("open");

        // Ordinal 2 requested via its own value as the row index directly —
        // no OrdinalIndex involved, proving dense addressing needs none.
        let got = take_by_row_index(&dataset, &[2]).await.expect("take");
        assert_eq!(got.get(&2), Some(&b"charlie".to_vec()));
    }

    /// The finding this module exists to encode: for a SPARSE ordinal space
    /// (chains — only tagged ways get an entry), row index is NOT the
    /// ordinal. A gapped fixture (7, 42, 1000, 1001 — nothing near their own
    /// values as a row count) is the falsifier: naive `ordinal-as-index`
    /// addressing would either miss every row (index out of range) or,
    /// worse, silently return the WRONG entry for any dataset that happens
    /// to have >= ordinal rows. `OrdinalIndex` must translate correctly.
    #[tokio::test]
    async fn sparse_ordinals_require_the_ordinal_index_row_position_is_not_the_ordinal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("chains_test.lance");
        let entries: Vec<(u32, Vec<u8>)> = vec![
            (7, vec![1, 2, 3]),
            (42, vec![9, 9, 9, 9, 9, 9, 9]),
            (1_000, vec![]),
            (1_001, vec![255]),
        ];
        let written_ordinals = write_ordinal_blob_dataset(&dest, &entries)
            .await
            .expect("write");
        assert_eq!(
            written_ordinals,
            vec![7, 42, 1_000, 1_001],
            "write returns ordinals in insertion (row) order"
        );
        let index = OrdinalIndex::new(written_ordinals);

        // Ordinal 42 lives at row index 1 (the SECOND entry written), not
        // at row index 42 — the assertion this whole test exists for.
        assert_eq!(index.row_index(42), Some(1));
        assert_eq!(index.row_index(1_000), Some(2));
        assert_eq!(
            index.row_index(8),
            None,
            "an unstored ordinal between two real ones is a real absence, not an error"
        );

        let dataset = Dataset::open(dest.to_str().unwrap()).await.expect("open");
        let got = take_by_ordinal_sparse(&dataset, &index, &[42, 1_000, 8])
            .await
            .expect("take");
        assert_eq!(got.get(&42), Some(&vec![9, 9, 9, 9, 9, 9, 9]));
        assert_eq!(got.get(&1_000), Some(&vec![]));
        assert_eq!(
            got.get(&8),
            None,
            "an unstored ordinal is silently absent from the result, mirroring \
             Chains::get's existing Ok(None) contract — never a panic or an error"
        );
        assert_eq!(got.len(), 2, "exactly the two real ordinals came back, nothing extra");
    }

    /// Anti-vacuity for the round trip: bytes come back byte-for-byte,
    /// including a genuinely empty payload (ordinal 1_000 above) — proving
    /// the empty-Vec case is a real stored zero-length value, not an
    /// artifact of a missing/null row being silently treated as empty.
    /// End-to-end: a real `.chains` sidecar (built with the sibling crate's
    /// own [`osm_soa_bake::chains::write_chains`]) on disk, converted, and
    /// read back — proving [`ensure_chains_lance_local`] round-trips the
    /// EXACT same bytes [`osm_soa_bake::chains::Chains::get`] would decode,
    /// via the raw-record path (never re-encoding).
    #[tokio::test]
    async fn ensure_chains_lance_local_round_trips_a_real_chains_sidecar() {
        use osm_soa_bake::chains::{decode_chain, Chains};
        use osm_soa_bake::tms::TileXy;

        fn c(x: u32, y: u32) -> TileXy {
            TileXy { x, y_xyz: y }
        }
        let ring = vec![c(10, 20), c(11, 25), c(9, 30), c(10, 20)];

        // A gapped, non-contiguous ordinal space — the sparse case this
        // module exists for (see the "sparse_ordinals" test above).
        let mut chains = vec![(3u32, ring.clone()), (500, vec![c(1, 1)]), (501, ring.clone())];
        let mut buf = Vec::new();
        osm_soa_bake::chains::write_chains(&mut buf, 0xABCD_1234, &mut chains).expect("write");

        let dir = tempfile::tempdir().expect("tempdir");
        let chains_path = dir.path().join("region.chains");
        std::fs::write(&chains_path, &buf).expect("write fixture");

        let (dataset_path, index) = ensure_chains_lance_local(&chains_path)
            .await
            .expect("conversion should succeed on a well-formed sidecar");
        assert_eq!(dataset_path, dir.path().join("region.chains.lance"));

        // Cross-check against the ORIGINAL reader: every ordinal the Lance
        // dataset now serves must decode to exactly what Chains::get()
        // returns for that same ordinal from the original bytes.
        let original = Chains::from_bytes(buf).expect("parse original");
        let dataset = Dataset::open(dataset_path.to_str().unwrap()).await.expect("open");

        for ordinal in [3u32, 500, 501] {
            let row = index.row_index(ordinal).expect("ordinal was written");
            let got = take_by_row_index(&dataset, &[row]).await.expect("take");
            let raw = got.get(&ordinal).expect("present");
            let decoded = decode_chain(raw).expect("decode");
            assert_eq!(
                Some(decoded),
                original.get(ordinal).expect("original decode")
            );
        }
        // A gap in the ordinal space (nothing stored at 4) must not resolve.
        assert_eq!(index.row_index(4), None);
    }

    /// End-to-end for books: dense ordinals, four separate datasets, and
    /// (per the module doc) row position == ordinal directly — no
    /// [`OrdinalIndex`] needed on this side.
    #[tokio::test]
    async fn ensure_books_lance_local_round_trips_all_four_dense_codebooks() {
        use lance_graph_contract::identity_quad::IdentityCodebook;
        use osm_soa_bake::codebook::{Books, Header};
        use osm_soa_bake::tms::AnchorRounding;

        let identities = IdentityCodebook::try_new(vec!["node/1".into(), "way/2".into()]).unwrap();
        let tag_keys = IdentityCodebook::try_new(vec!["highway".into()]).unwrap();
        let tag_values =
            IdentityCodebook::try_new(vec!["primary".into(), "residential".into()]).unwrap();
        let labels = IdentityCodebook::try_new(vec!["Hauptstraße".into()]).unwrap();

        let books = Books {
            identities,
            tag_keys,
            tag_values,
            labels,
        };
        let header = Header {
            rows: 2,
            slots_written: 3,
            slab: 0x1122_3344,
            rounding: AnchorRounding::CURRENT,
        };
        let mut buf = Vec::new();
        osm_soa_bake::codebook::write_books(&mut buf, &header, &books).expect("write");

        let dir = tempfile::tempdir().expect("tempdir");
        let books_path = dir.path().join("region.books");
        std::fs::write(&books_path, &buf).expect("write fixture");

        let [id_path, keys_path, values_path, labels_path] =
            ensure_books_lance_local(&books_path).await.expect("conversion should succeed");
        assert_eq!(id_path, dir.path().join("region.identities.lance"));
        assert_eq!(keys_path, dir.path().join("region.tag_keys.lance"));
        assert_eq!(values_path, dir.path().join("region.tag_values.lance"));
        assert_eq!(labels_path, dir.path().join("region.labels.lance"));

        // Dense addressing: ordinal 1 of tag_values ("residential") sits at
        // row 1 directly — no index translation, per the module doc.
        let values_ds = Dataset::open(values_path.to_str().unwrap()).await.expect("open");
        let got = take_by_row_index(&values_ds, &[0, 1]).await.expect("take");
        assert_eq!(got.get(&0), Some(&b"primary".to_vec()));
        assert_eq!(got.get(&1), Some(&b"residential".to_vec()));

        // UTF-8 multibyte content (the German ß) survives verbatim — labels
        // carry street names, not ASCII-only tag vocabulary.
        let labels_ds = Dataset::open(labels_path.to_str().unwrap()).await.expect("open");
        let got = take_by_row_index(&labels_ds, &[0]).await.expect("take");
        assert_eq!(got.get(&0), Some(&"Hauptstraße".as_bytes().to_vec()));
    }

    /// The actual request-scoped path: convert once, then `gather_chains`
    /// with a SUBSET of ordinals — proving the gathered `RequestChains`
    /// decodes correctly for what was asked, stays silent on a real gap
    /// (ordinal 4, never stored), AND doesn't leak an ordinal that exists in
    /// the dataset but wasn't requested (the anti-vacuity check: a broken
    /// gather that just returned "everything" would still pass a
    /// requested-ordinals-only assertion).
    ///
    /// Uses the crate's OWN process-global caches
    /// (`CHAINS_DATASET`/`CHAINS_INDEX`), which is safe here ONLY because
    /// nextest runs every test in its own process (per this repo's
    /// `.claude/rules/integration-tests.md`) — a `cargo test` run sharing one
    /// process across tests would collide on these statics.
    #[tokio::test]
    async fn gather_chains_returns_exactly_the_requested_ordinals() {
        use osm_soa_bake::tms::TileXy;

        fn c(x: u32, y: u32) -> TileXy {
            TileXy { x, y_xyz: y }
        }
        let ring = vec![c(10, 20), c(11, 25), c(9, 30), c(10, 20)];
        let mut chains = vec![(3u32, ring.clone()), (500, vec![c(1, 1)]), (501, ring)];
        let mut buf = Vec::new();
        osm_soa_bake::chains::write_chains(&mut buf, 0xAAAA, &mut chains).expect("write");

        let dir = tempfile::tempdir().expect("tempdir");
        let chains_path = dir.path().join("gather.chains");
        std::fs::write(&chains_path, &buf).expect("write fixture");

        let (dataset_path, index) =
            ensure_chains_lance_local(&chains_path).await.expect("conversion");
        set_chains_index(index);

        // Ask for 3 and 500 (present), 4 (a real gap), and NOT 501 (present
        // in the dataset but never requested).
        let gathered = gather_chains(&dataset_path, &[3, 500, 4])
            .await
            .expect("gather should succeed once the index is set");

        assert_eq!(
            gathered.get(3).unwrap(),
            Some(vec![c(10, 20), c(11, 25), c(9, 30), c(10, 20)])
        );
        assert_eq!(gathered.get(500).unwrap(), Some(vec![c(1, 1)]));
        assert_eq!(gathered.get(4).unwrap(), None, "4 was never stored");
        assert_eq!(
            gathered.get(501).unwrap(),
            None,
            "501 exists in the dataset but was not in the requested set — a \
             gather that leaked it would still pass every assertion above"
        );
    }

    /// `gather_books` end-to-end: convert all four codebooks, gather a
    /// subset across the three the hot path reads, and prove `labels` is
    /// genuinely not part of this surface (no field for it on
    /// `RequestBooks`) while `identities`/`tag_keys`/`tag_values` resolve
    /// exactly the requested ordinals.
    #[tokio::test]
    async fn gather_books_resolves_the_three_hot_path_codebooks() {
        use lance_graph_contract::identity_quad::IdentityCodebook;
        use osm_soa_bake::codebook::{Books, Header};
        use osm_soa_bake::tms::AnchorRounding;

        let identities =
            IdentityCodebook::try_new(vec!["node/1".into(), "way/2".into()]).unwrap();
        let tag_keys = IdentityCodebook::try_new(vec!["highway".into()]).unwrap();
        let tag_values =
            IdentityCodebook::try_new(vec!["primary".into(), "residential".into()]).unwrap();
        let labels = IdentityCodebook::try_new(vec!["Hauptstraße".into()]).unwrap();
        let books = Books {
            identities,
            tag_keys,
            tag_values,
            labels,
        };
        let header = Header {
            rows: 2,
            slots_written: 3,
            slab: 0x5566,
            rounding: AnchorRounding::CURRENT,
        };
        let mut buf = Vec::new();
        osm_soa_bake::codebook::write_books(&mut buf, &header, &books).expect("write");

        let dir = tempfile::tempdir().expect("tempdir");
        let books_path = dir.path().join("gather.books");
        std::fs::write(&books_path, &buf).expect("write fixture");

        let [id_path, keys_path, values_path, _labels_path] =
            ensure_books_lance_local(&books_path).await.expect("conversion");

        let gathered = gather_books(&id_path, &keys_path, &values_path, &[1], &[0], &[1])
            .await
            .expect("gather should succeed");

        assert_eq!(gathered.identities.key(1), Some("way/2"));
        assert_eq!(gathered.identities.key(0), None, "0 was not requested");
        assert_eq!(gathered.tag_keys.key(0), Some("highway"));
        assert_eq!(gathered.tag_values.key(1), Some("residential"));
        assert_eq!(gathered.tag_values.key(0), None, "0 was not requested");
    }

    #[tokio::test]
    async fn empty_payload_round_trips_as_present_not_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("chains_empty_test.lance");
        let entries: Vec<(u32, Vec<u8>)> = vec![(5, vec![])];
        let written = write_ordinal_blob_dataset(&dest, &entries)
            .await
            .expect("write");
        let index = OrdinalIndex::new(written);
        let dataset = Dataset::open(dest.to_str().unwrap()).await.expect("open");
        let got = take_by_ordinal_sparse(&dataset, &index, &[5])
            .await
            .expect("take");
        // Present (Some(&vec![])), not absent (None) — the distinction a
        // careless `if bytes.is_empty() { treat as missing }` would erase.
        assert_eq!(got.get(&5), Some(&Vec::<u8>::new()));
        assert!(got.contains_key(&5));
    }
}
