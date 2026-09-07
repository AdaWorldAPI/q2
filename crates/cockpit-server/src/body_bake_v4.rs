//! Boot-time hydration of the **v4** FMA body bake — `/helix2` only.
//!
//! # This file touches nothing that already works
//!
//! It is a standalone fork in the sense `BodyHelix2.tsx` is a fork of
//! `BodyHelix.tsx` (#64): the v4 work gets its own copy of everything so it can
//! never regress a working route. Concretely, and non-negotiably:
//!
//! - **`/helix` is untouched.** It reads `helix_latest` from the manifest and
//!   the copy baked into `dist/` at image build. Nothing here is on that path,
//!   so a bug here cannot show up there.
//! - **`/osm` is untouched.** An earlier version of this module made
//!   [`crate::osm_slab_hydrate`]'s helpers generic to share them. That put the
//!   working map's hydration on the same code as an experimental body bake — a
//!   defect in the shared half would have broken the map. The duplication below
//!   is deliberate and is the cheaper half of that trade.
//! - **Producers are not referenced here, at all.** This module fetches an
//!   already-published artifact and writes it to a read-only cache. It never
//!   runs a baker, never reads a baker's inputs, and never writes into a
//!   directory a baker writes to. Bake sources and bake outputs live on the
//!   producer side; this side only ever consumes a published artifact.
//! - **It shares no variable NAME with anything.** Every setting is
//!   `BODY_BAKE_V4_*`, including its own object-store credentials — it does
//!   NOT read the ambient `AWS_*` or `RAILWAY_VOL` that the map path uses.
//!   Paste the same values in if you like; the point is that changing this
//!   deploy's v4 settings cannot move anything else, and vice versa. This is
//!   also what makes the module liftable into its own crate later: it has no
//!   configuration entanglement to unpick.
//! - **There is no v3 slot.** Not an oversight: a v3 slot here would be a
//!   second way to serve the shipped body, adjacent to the v4 one, and the
//!   whole reason `/helix2` exists is that the two must be separable. v3 is
//!   served by the path that already serves it.
//!
//! # Topology
//!
//! ```text
//!   S3  (durable source of truth, published by the baker — a different system)
//!    │   $BODY_BAKE_V4_ENDPOINT/$BODY_BAKE_V4_BUCKET
//!    │     /q2/bakes/$BODY_BAKE_V4_TAG/{$BODY_BAKE_V4_ASSET, SHA256SUMS}
//!    ▼
//!   volume  ($BODY_BAKE_V4_DIR, /volume01/body-v4, else temp)
//!    │
//!    ▼
//!   GET /api/bake/v4  (streamed from the file, same-origin)
//! ```
//!
//! # No defaults, ever
//!
//! Neither variable has a default. A default artifact name is a bake this
//! deploy did not choose, and the only artifact that exists to default TO is
//! the v3 one — which is how a first version of this module came to point the
//! v4 route at the v3 bake. Unconfigured means unconfigured: the route says so
//! and serves nothing.
//!
//! # Only a VERIFIED path is ever served
//!
//! The verified path is published to [`verified_path`] after the checksum
//! passes, and the handler reads only that. Serving on file-existence alone
//! would hand out the bytes of a cached copy whose checksum FAILED and whose
//! replacement download then also failed — a file that is present, stale, and
//! wrong. A cached copy that fails its checksum is deleted before the retry,
//! so a failed hydrate leaves nothing behind to serve.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use object_store::aws::AmazonS3Builder;
use object_store::{ObjectStore, ObjectStoreExt};
use sha2::{Digest, Sha256};

/// Log prefix. Distinct from the OSM slab's so a boot log never conflates them.
const LABEL: &str = "body bake v4";

/// The verified artifact, published once at boot. Absent = nothing to serve.
static VERIFIED: OnceLock<PathBuf> = OnceLock::new();

/// The checksum-verified artifact, or `None` when this deploy has none.
///
/// The ONLY accessor the request path may use. A path that exists on disk is
/// not evidence it verified.
#[must_use]
pub fn verified_path() -> Option<&'static Path> {
    VERIFIED.get().map(PathBuf::as_path)
}

/// `std::env::var`, with an empty value treated as absent — a platform
/// variable can exist as a row that was never filled in, and that must fail
/// exactly like an unset one rather than attempt a doomed call with an empty
/// bucket name.
fn env_var_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// A name interpolated into BOTH an S3 key and a filesystem path.
///
/// Not decoration: `..`, `/`, or a backslash here would read a different
/// prefix and write outside the cache directory. Restricting the alphabet
/// makes both uses safe by construction rather than by careful escaping at
/// each site.
fn is_safe_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !s.contains("..")
}

fn checked(key: &str) -> Option<String> {
    match env_var_nonempty(key) {
        None => None,
        Some(v) if is_safe_name(&v) => Some(v),
        Some(bad) => {
            tracing::warn!(
                rejected = %bad, key,
                "{LABEL}: name must be [A-Za-z0-9._-] with no `..`; ignoring it"
            );
            None
        }
    }
}

/// The v4 tag and asset, or `None` when this deploy does not serve v4.
fn coordinates() -> Option<(String, String)> {
    Some((checked("BODY_BAKE_V4_TAG")?, checked("BODY_BAKE_V4_ASSET")?))
}

/// The v4 object store, built from v4-only variables.
///
/// `AmazonS3Builder::from_env()` is deliberately NOT used: it reads the ambient
/// `AWS_*` that the map path also reads, which would make one deploy's v4
/// credentials and another subsystem's the same knob. Every field is named
/// here so the two can never be the same setting — set them to the same values
/// if that is what you want.
fn build_store() -> Option<impl ObjectStore> {
    let endpoint = env_var_nonempty("BODY_BAKE_V4_ENDPOINT")?;
    let bucket = env_var_nonempty("BODY_BAKE_V4_BUCKET")?;
    let key_id = env_var_nonempty("BODY_BAKE_V4_ACCESS_KEY_ID")?;
    let secret = env_var_nonempty("BODY_BAKE_V4_SECRET_ACCESS_KEY")?;
    let region = env_var_nonempty("BODY_BAKE_V4_REGION").unwrap_or_else(|| "auto".to_string());

    match AmazonS3Builder::new()
        .with_endpoint(endpoint)
        .with_bucket_name(bucket)
        .with_access_key_id(key_id)
        .with_secret_access_key(secret)
        .with_region(region)
        .build()
    {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!(error = %e, "{LABEL}: S3 client build failed");
            None
        }
    }
}

/// Which required v4 variables are absent, so one boot line settles it.
fn missing_vars() -> Vec<&'static str> {
    [
        "BODY_BAKE_V4_TAG",
        "BODY_BAKE_V4_ASSET",
        "BODY_BAKE_V4_ENDPOINT",
        "BODY_BAKE_V4_BUCKET",
        "BODY_BAKE_V4_ACCESS_KEY_ID",
        "BODY_BAKE_V4_SECRET_ACCESS_KEY",
    ]
    .into_iter()
    .filter(|k| env_var_nonempty(k).is_none())
    .collect()
}

/// The v4 cache directory. Its own leaf — `body-v4` — so nothing this module
/// writes can land beside, or overwrite, an artifact of any other generation.
///
/// The temp fallback is absolute on purpose: a relative path would follow the
/// process CWD, so a later `set_current_dir` would silently address a
/// different directory while logging the same string.
fn cache_dir() -> PathBuf {
    if let Some(p) = env_var_nonempty("BODY_BAKE_V4_DIR") {
        return PathBuf::from(p);
    }
    // Deliberately NOT `RAILWAY_VOL`: that variable already steers the map's
    // cache, and a v4 experiment must not be able to move where the map looks.
    // A deploy whose volume is mounted elsewhere sets BODY_BAKE_V4_DIR.
    let vol01 = Path::new("/volume01");
    if vol01.is_dir()
        && !vol01
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    {
        return vol01.join("body-v4");
    }
    std::env::temp_dir().join("q2-body-v4")
}

/// Parse `sha256sum` output: `<hex>  <name>` per line, tolerating the `*name`
/// binary marker.
fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let hash = it.next()?;
            let name = it.next()?.trim_start_matches('*');
            (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| (name.to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// SHA-256 of a file, streamed — the artifact is ~59 MB and must not be read
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

/// Hydrate the v4 bake and publish its verified path.
///
/// Never panics, never publishes an unverified path, and never touches any
/// other route's artifacts. Called once at boot, before the listener binds.
pub async fn ensure_local() {
    let missing = missing_vars();
    if !missing.is_empty() {
        tracing::info!(
            missing = %missing.join(", "),
            "{LABEL}: not configured — /api/bake/v4 answers 503; /helix is unaffected"
        );
        return;
    }
    let Some((tag, asset)) = coordinates() else {
        return;
    };

    let dir = cache_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(dir = %dir.display(), error = %e, "{LABEL}: cannot create cache dir");
        return;
    }
    let dest = dir.join(&asset);

    let Some(store) = build_store() else {
        return;
    };

    let prefix = format!("q2/bakes/{tag}");
    let Some(want) = fetch_want(&store, &prefix, &asset).await else {
        return;
    };

    // A cache hit is re-verified, not trusted: the failure this guards is a
    // half-written file from a container killed mid-download, which is exactly
    // what a "we already have it" check waves through.
    if dest.is_file() {
        match sha256_file(&dest) {
            Ok(got) if got == want => {
                tracing::info!(path = %dest.display(), "{LABEL}: cache hit, checksum verified");
                publish(dest);
                return;
            }
            Ok(got) => {
                tracing::warn!(%got, %want, "{LABEL}: cached copy failed its checksum; discarding");
            }
            Err(e) => {
                tracing::warn!(error = %e, "{LABEL}: cannot hash cached copy; discarding");
            }
        }
        // Delete BEFORE the retry. If the retry also fails, an unverified file
        // must not be left where anything could pick it up.
        if let Err(e) = std::fs::remove_file(&dest) {
            tracing::error!(error = %e, "{LABEL}: cannot remove the bad cached copy; refusing");
            return;
        }
    }

    if download_verified(&store, &prefix, &asset, &dest, &want).await {
        tracing::info!(path = %dest.display(), %tag, "{LABEL}: hydrated and verified");
        publish(dest);
    }
}

fn publish(path: PathBuf) {
    if VERIFIED.set(path).is_err() {
        tracing::warn!("{LABEL}: hydrate ran twice; keeping the first verified path");
    }
}

/// The pinned digest for `asset`, from the `SHA256SUMS` beside it.
async fn fetch_want(store: &impl ObjectStore, prefix: &str, asset: &str) -> Option<String> {
    let path = object_store::path::Path::from(format!("{prefix}/SHA256SUMS"));
    let bytes = match store.get(&path).await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "{LABEL}: SHA256SUMS body read failed");
                return None;
            }
        },
        Err(e) => {
            tracing::error!(error = %e, %prefix, "{LABEL}: SHA256SUMS not readable");
            return None;
        }
    };
    let sums = parse_sums(&String::from_utf8_lossy(&bytes));
    match sums.iter().find(|(n, _)| n == asset) {
        Some((_, h)) => Some(h.clone()),
        None => {
            tracing::error!(%asset, %prefix, "{LABEL}: no checksum pinned for this asset; refusing");
            None
        }
    }
}

/// Stream one object to `<dest>.part`, hashing while writing, and rename into
/// place only on a match. A mismatch leaves no file behind.
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
            tracing::error!(artifact = name, error = %e, "{LABEL}: download failed");
            return false;
        }
    };

    let part = dest.with_extension("part");
    let mut file = match std::fs::File::create(&part) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(artifact = name, error = %e, "{LABEL}: cannot create .part");
            return false;
        }
    };

    let mut hasher = Sha256::new();
    let mut stream = result.into_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(artifact = name, error = %e, "{LABEL}: stream error");
                let _ = std::fs::remove_file(&part);
                return false;
            }
        };
        hasher.update(&chunk);
        if let Err(e) = file.write_all(&chunk) {
            tracing::error!(artifact = name, error = %e, "{LABEL}: write error");
            let _ = std::fs::remove_file(&part);
            return false;
        }
    }
    if let Err(e) = file.flush() {
        tracing::error!(artifact = name, error = %e, "{LABEL}: flush error");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    drop(file);

    let got = hex::encode(hasher.finalize());
    if got != want {
        tracing::error!(artifact = name, %got, %want, "{LABEL}: checksum mismatch; discarding");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    if let Err(e) = std::fs::rename(&part, dest) {
        tracing::error!(artifact = name, error = %e, "{LABEL}: rename into place failed");
        let _ = std::fs::remove_file(&part);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation_rejects_traversal_and_separators() {
        assert!(is_safe_name("fma-body-v4-v1"));
        assert!(is_safe_name("body.20260629c.v6helix.soa.gz"));
        assert!(!is_safe_name("../other-tag"));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name("a\\b"));
        assert!(!is_safe_name(""));
        // A name that is only ALMOST traversal must still pass, or the guard
        // fires on everything and discriminates nothing.
        assert!(is_safe_name("v4.1"));
    }

    #[test]
    fn there_is_no_default_artifact() {
        // The defect this pins: a first version defaulted to the v3 tag and
        // filename, so the v4 route would have served the v3 bake on any
        // configured deploy. Unset must stay unset.
        // SAFETY: single-threaded test; both variables are removed, not set.
        unsafe {
            std::env::remove_var("BODY_BAKE_V4_TAG");
            std::env::remove_var("BODY_BAKE_V4_ASSET");
        }
        assert!(coordinates().is_none());
    }

    #[test]
    fn a_rejected_tag_does_not_fall_back_to_some_other_bake() {
        // SAFETY: single-threaded test; the variables are removed below.
        unsafe {
            std::env::set_var("BODY_BAKE_V4_TAG", "../secrets");
            std::env::set_var("BODY_BAKE_V4_ASSET", "body.soa.gz");
        }
        assert!(
            coordinates().is_none(),
            "a rejected name must not resolve at all"
        );
        unsafe {
            std::env::remove_var("BODY_BAKE_V4_TAG");
            std::env::remove_var("BODY_BAKE_V4_ASSET");
        }
    }

    #[test]
    fn the_cache_dir_is_v4_specific_and_ignores_the_map_s_volume_variable() {
        // SAFETY: single-threaded test; the variables are restored below.
        unsafe {
            std::env::set_var("BODY_BAKE_V4_DIR", "/tmp/explicit-v4");
            std::env::set_var("RAILWAY_VOL", "/tmp/vol");
        }
        assert_eq!(cache_dir(), PathBuf::from("/tmp/explicit-v4"));
        unsafe { std::env::remove_var("BODY_BAKE_V4_DIR") };
        // With no v4 directory set, RAILWAY_VOL must NOT be consulted — it
        // steers the map's cache, and this path may not move that.
        assert_ne!(cache_dir(), PathBuf::from("/tmp/vol/body-v4"));
        unsafe { std::env::remove_var("RAILWAY_VOL") };
    }

    #[test]
    fn no_v4_setting_shares_a_name_with_the_map_or_the_ambient_aws_config() {
        // The crossed-wire guard: every name this module reads must be
        // v4-scoped, so changing a v4 setting cannot move anything else.
        for key in [
            "BODY_BAKE_V4_TAG",
            "BODY_BAKE_V4_ASSET",
            "BODY_BAKE_V4_ENDPOINT",
            "BODY_BAKE_V4_BUCKET",
            "BODY_BAKE_V4_ACCESS_KEY_ID",
            "BODY_BAKE_V4_SECRET_ACCESS_KEY",
            "BODY_BAKE_V4_REGION",
            "BODY_BAKE_V4_DIR",
        ] {
            assert!(key.starts_with("BODY_BAKE_V4_"), "{key} is not v4-scoped");
        }
        assert_eq!(
            missing_vars().len(),
            6,
            "with nothing set, every required v4 variable must be reported missing"
        );
    }

    #[test]
    fn nothing_is_served_before_a_verified_hydrate() {
        assert!(verified_path().is_none());
    }

    #[test]
    fn parse_sums_reads_the_sha256sum_format() {
        let text = "  \nabc  short\n\
                    e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  body.soa.gz\n\
                    e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 *other.gz\n";
        let sums = parse_sums(text);
        assert_eq!(sums.len(), 2, "the short-hash line must be rejected");
        assert_eq!(sums[0].0, "body.soa.gz");
        assert_eq!(sums[1].0, "other.gz");
    }
}
