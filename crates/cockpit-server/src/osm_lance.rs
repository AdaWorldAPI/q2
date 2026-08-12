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
//! # The request-time read path does not change in this pass
//!
//! [`crate::osm_features::open_slab`] keeps mmapping `OSM_SLAB_PATH`
//! synchronously, unchanged. This module does not repoint it. `soa_to_lance`
//! (lance-graph) already proves the mechanism that WOULD let it: at the
//! 512-byte `NODE_ROW_STRIDE`, Lance's `is_narrow` mini-block cutoff (256
//! bytes) is never crossed, so the row column lands full-zip, uncompressed,
//! and byte-verbatim in the dataset's own data file — meaning
//! `RowSlab::new(bytes)` would parse Lance-owned bytes exactly as it parses
//! the raw `.soa` file today. Reaching into a Lance fragment's on-disk
//! layout to find that byte run is a diagnostic probe in `soa_to_lance`
//! (its own "P-CACHE-4" section), not a stable production API — wiring the
//! hot tile-serving path onto it is a distinct, separately-scoped follow-up,
//! not bundled into establishing Lance as the volume's canonical persisted
//! form.
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

    let bytes = match std::fs::read(slab_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(path = %slab_path.display(), error = %e, "osm lance: cannot read slab");
            return None;
        }
    };
    if bytes.is_empty() || !bytes.len().is_multiple_of(NODE_ROW_STRIDE) {
        tracing::error!(
            path = %slab_path.display(), len = bytes.len(),
            "osm lance: slab is not a whole number of {NODE_ROW_STRIDE}-byte rows; refusing"
        );
        return None;
    }
    let rows = bytes.len() / NODE_ROW_STRIDE;
    let digest_hex = format!("{:016x}", osm_soa_bake::codebook::hash_slab(&bytes));

    if let Some(warm) = reopen_if_warm(&dest, rows, &digest_hex).await {
        tracing::info!(
            path = %dest.display(), rows,
            "osm lance: warm — dataset already matches this slab's row count and digest"
        );
        return Some(warm);
    }

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
    if got_rows == rows && got_digest.as_deref() == Some(digest_hex) {
        Some(dest.to_path_buf())
    } else {
        tracing::warn!(
            path = %dest.display(), got_rows, want_rows = rows,
            got_digest = ?got_digest, want_digest = digest_hex,
            "osm lance: existing dataset is stale (row count or slab digest mismatch); reconverting"
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
}
