//! Boot-time hydration of the baked OSM slab from S3 onto a persistent volume.
//!
//! # The topology
//!
//! ```text
//!   S3  (durable source of truth)
//!    │   s3://$AWS_S3_BUCKET_NAME/<prefix>/{berlin.soa, berlin.books, SHA256SUMS}
//!    ▼
//!   $RAILWAY_VOL  (persistence across container rebuilds — a CACHE, not truth)
//!    │   <vol>/osm/{berlin.soa, berlin.books}
//!    ▼
//!   mmap  ([`crate::osm_features::open_slab`], unchanged)
//! ```
//!
//! The volume exists so the 1.29 GiB artifact survives a rebuild — that is the
//! whole of its job. Deleting it costs a re-download and nothing else, which is
//! why every decision here treats the volume as disposable and S3 as
//! authoritative.
//!
//! # Why this runs at boot and not on first request
//!
//! `main()` is already `async`, so hydration is a step before the listener
//! binds. That keeps [`crate::osm_features::open_slab`] **synchronous and
//! untouched** — it still just mmaps `OSM_SLAB_PATH`. The alternative (hydrate
//! lazily inside the `OnceLock`) would have forced the whole read path async
//! for a one-shot boot concern, and would have made the first request pay a
//! multi-minute download while holding the initializer.
//!
//! # Checksum pinning
//!
//! `SHA256SUMS` is fetched from the same prefix and every file is verified
//! against it before use — on download AND on a cache hit. There is no
//! unverified path: a corrupt or truncated volume copy is re-fetched rather
//! than mmap'd. This mirrors `MedCare-rs`'s `scripts/fetch-frontend-assets.sh`
//! discipline, where a URL bump requires a checksum bump in the same edit.
//!
//! Verifying on a cache HIT (not only after download) is deliberate: the
//! failure this guards is a half-written file from a container killed
//! mid-download, which is exactly the case a "we already have it" check would
//! otherwise wave through. Downloads land on a `.part` file and are renamed
//! only after the hash matches, so a killed container leaves no file that
//! looks complete — but the volume outlives this code, so the check stays.
//!
//! # Absent configuration is not an error
//!
//! No bucket, no volume, no credentials ⇒ `None`, and the endpoint keeps
//! answering 503 exactly as it did before this module existed. Local
//! development sets `OSM_SLAB_PATH` directly and never reaches S3.

use std::path::{Path, PathBuf};

use object_store::aws::AmazonS3Builder;
use object_store::{ObjectStore, ObjectStoreExt};
use sha2::{Digest, Sha256};

/// Default S3 prefix holding the bake. Overridable with `OSM_SLAB_S3_PREFIX`.
const DEFAULT_PREFIX: &str = "q2/bakes/berlin-v1";

/// The slab and its codebook sidecar. Both are required: `RowSlab` can read
/// positions without the books, but identity resolution needs them, and a slab
/// without its books is what the bake itself calls unreadable.
const ARTIFACTS: [&str; 2] = ["berlin.soa", "berlin.books"];

/// Where the hydrated copy lives, given the volume root.
fn cache_dir(vol: &str) -> PathBuf {
    Path::new(vol).join("osm")
}

/// Resolve a local, verified slab path, hydrating from S3 if needed.
///
/// Returns the path to set as `OSM_SLAB_PATH`, or `None` when the feature is
/// not configured — never panics, and never returns a path that failed its
/// checksum.
pub async fn ensure_slab_local() -> Option<PathBuf> {
    // 1. An explicit local path always wins. This is the local-dev and
    //    already-hydrated case, and it must not require S3 credentials.
    if let Ok(p) = std::env::var("OSM_SLAB_PATH") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            tracing::info!(path = %p, "osm slab: using OSM_SLAB_PATH directly");
            return Some(path);
        }
        tracing::warn!(path = %p, "osm slab: OSM_SLAB_PATH set but not a file; trying S3");
    }

    // 2. Otherwise hydrate. Both a bucket and a destination are required.
    let bucket = std::env::var("AWS_S3_BUCKET_NAME").ok()?;
    // `OSM_SLAB_CACHE_DIR` overrides the volume — it is what makes this
    // testable off-Railway, where /volume01 does not exist.
    let vol = std::env::var("OSM_SLAB_CACHE_DIR")
        .or_else(|_| std::env::var("RAILWAY_VOL"))
        .ok()?;
    let prefix = std::env::var("OSM_SLAB_S3_PREFIX").unwrap_or_else(|_| DEFAULT_PREFIX.to_string());

    let dir = cache_dir(&vol);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(dir = %dir.display(), error = %e, "osm slab: cannot create cache dir");
        return None;
    }

    // `from_env()` reads AWS_ENDPOINT_URL / AWS_ACCESS_KEY_ID /
    // AWS_SECRET_ACCESS_KEY / AWS_DEFAULT_REGION with no glue — `aws_endpoint_url`
    // is an accepted alias for the endpoint key, so a non-AWS S3 endpoint needs
    // no special casing.
    let store = match AmazonS3Builder::from_env()
        .with_bucket_name(&bucket)
        .build()
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "osm slab: S3 client build failed");
            return None;
        }
    };

    let sums = match fetch_sums(&store, &prefix).await {
        Some(s) => s,
        None => return None,
    };

    for name in ARTIFACTS {
        let want = match sums.iter().find(|(k, _)| k == name).map(|(_, h)| h.clone()) {
            Some(h) => h,
            None => {
                tracing::error!(artifact = name, "osm slab: no checksum pinned; refusing");
                return None;
            }
        };
        let dest = dir.join(name);

        // Cache hit, but only if it still hashes correctly — see module docs.
        if dest.is_file() {
            match sha256_file(&dest) {
                Ok(got) if got == want => {
                    tracing::info!(artifact = name, "osm slab: cache hit, checksum verified");
                    continue;
                }
                Ok(got) => tracing::warn!(
                    artifact = name, %got, %want,
                    "osm slab: cached copy failed its checksum; re-fetching"
                ),
                Err(e) => {
                    tracing::warn!(artifact = name, error = %e, "osm slab: cannot hash cached copy; re-fetching")
                }
            }
        }

        if !download_verified(&store, &prefix, name, &dest, &want).await {
            return None;
        }
    }

    let slab = dir.join(ARTIFACTS[0]);
    tracing::info!(path = %slab.display(), "osm slab: hydrated and verified");
    Some(slab)
}

/// Fetch and parse `SHA256SUMS` — `<hex>  <name>` per line, the `sha256sum`
/// format the bucket already uses for the MedCare bakes.
async fn fetch_sums(store: &impl ObjectStore, prefix: &str) -> Option<Vec<(String, String)>> {
    let path = object_store::path::Path::from(format!("{prefix}/SHA256SUMS"));
    let bytes = match store.get(&path).await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "osm slab: SHA256SUMS body read failed");
                return None;
            }
        },
        Err(e) => {
            tracing::error!(error = %e, %prefix, "osm slab: SHA256SUMS not readable");
            return None;
        }
    };
    Some(parse_sums(&String::from_utf8_lossy(&bytes)))
}

/// Parse `sha256sum` output. Tolerates the `*name` binary marker and blank
/// lines; ignores anything that is not `<hex> <name>`.
fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let hash = it.next()?;
            let name = it.next()?.trim_start_matches('*');
            if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                Some((name.to_string(), hash.to_ascii_lowercase()))
            } else {
                None
            }
        })
        .collect()
}

/// Stream one object to `<dest>.part`, hash while writing, and rename into
/// place only if it matches. A mismatch leaves no file behind.
async fn download_verified(
    store: &impl ObjectStore,
    prefix: &str,
    name: &str,
    dest: &Path,
    want: &str,
) -> bool {
    use futures::StreamExt;
    use std::io::Write;

    let path = object_store::path::Path::from(format!("{prefix}/{name}"));
    let result = match store.get(&path).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(artifact = name, error = %e, "osm slab: download failed");
            return false;
        }
    };

    let part = dest.with_extension("part");
    let mut file = match std::fs::File::create(&part) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(artifact = name, error = %e, "osm slab: cannot create .part");
            return false;
        }
    };

    let mut hasher = Sha256::new();
    let mut stream = result.into_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(artifact = name, error = %e, "osm slab: stream error");
                let _ = std::fs::remove_file(&part);
                return false;
            }
        };
        hasher.update(&chunk);
        if let Err(e) = file.write_all(&chunk) {
            tracing::error!(artifact = name, error = %e, "osm slab: write error");
            let _ = std::fs::remove_file(&part);
            return false;
        }
        written += chunk.len() as u64;
    }
    if let Err(e) = file.flush() {
        tracing::error!(artifact = name, error = %e, "osm slab: flush error");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    drop(file);

    let got = hex::encode(hasher.finalize());
    if got != want {
        tracing::error!(artifact = name, %got, %want, "osm slab: checksum mismatch; discarding");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    if let Err(e) = std::fs::rename(&part, dest) {
        tracing::error!(artifact = name, error = %e, "osm slab: rename into place failed");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    tracing::info!(
        artifact = name,
        bytes = written,
        "osm slab: downloaded and verified"
    );
    true
}

/// SHA-256 of a file, streamed — the artifact is 1.29 GiB and must not be read
/// into memory to be hashed.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sums_reads_the_sha256sum_format() {
        let text = "\
cbf5989ab45bc921d8a85fdbdb71c8e5029cd904a3d230a898c2b5eb81d7ebe7  berlin.soa
d12bc8a15270f9a61290fb7117c92621e1f88229a85bdfdfc4d217481addde7f *berlin.books

not-a-hash                        junk.txt
";
        let got = parse_sums(text);
        assert_eq!(got.len(), 2, "the junk line must not parse as a pin");
        assert_eq!(
            got[0],
            (
                "berlin.soa".to_string(),
                "cbf5989ab45bc921d8a85fdbdb71c8e5029cd904a3d230a898c2b5eb81d7ebe7".to_string()
            )
        );
        // The `*` binary marker is stripped, not treated as part of the name.
        assert_eq!(got[1].0, "berlin.books");
    }

    /// A short hex string is not a pin. Without this, a truncated SHA256SUMS
    /// could yield an entry that silently matches nothing and reads as "no
    /// checksum for this artifact" — which `ensure_slab_local` refuses on, but
    /// only because the entry is absent rather than malformed.
    #[test]
    fn parse_sums_rejects_a_short_hash() {
        assert!(parse_sums("abc123  berlin.soa").is_empty());
        // ...and accepts it at exactly 64, so the length check is the reason
        // and not some other property of the fixture.
        let sixty_four = "a".repeat(64);
        assert_eq!(parse_sums(&format!("{sixty_four}  berlin.soa")).len(), 1);
    }

    #[test]
    fn sha256_file_matches_the_known_digest_of_its_bytes() {
        let dir = std::env::temp_dir().join("q2-hydrate-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("probe.bin");
        std::fs::write(&p, b"abc").unwrap();
        // The published SHA-256 of "abc" — an external anchor, not a value
        // this code produced.
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn cache_dir_is_under_the_volume_root() {
        assert_eq!(cache_dir("/volume01"), PathBuf::from("/volume01/osm"));
    }
}
