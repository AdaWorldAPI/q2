//! Boot-time hydration of the baked FMA body onto durable disk.
//!
//! # The topology — identical to the OSM slab's, one artifact narrower
//!
//! ```text
//!   S3  (durable source of truth)
//!    │   s3://$AWS_S3_BUCKET_NAME/q2/bakes/<tag>/{<asset>, SHA256SUMS}
//!    ▼
//!   volume  ($RAILWAY_VOL/body, /volume01/body, else a temp dir)
//!    │
//!    ▼
//!   /api/bake/<tag>/<asset>  (served from the file, same-origin)
//! ```
//!
//! # Why hydrate instead of proxying each request
//!
//! The first version of this fetched the artifact from S3 **per request** with
//! a hand-rolled SigV4 signer. That was a second object-store client in a crate
//! that already had one ([`crate::osm_slab_hydrate`]'s `object_store`), and it
//! re-downloaded ~59 MB on every cold browser cache. The boot-time shape is
//! what this repo already does for the OSM slab and what `medcare-rs`'s
//! `bake_hydrate` does for the ontology crystal: fetch once, land it on the
//! volume, serve from the file.
//!
//! # The volume is a cache, never truth
//!
//! Deleting it costs a re-download and nothing else. A redeploy that lands on a
//! container WITHOUT the volume mounted falls through the ladder to a temp dir
//! and re-fetches — degraded (it pays the transfer again) but never wrong. That
//! is the whole reason the checksum is re-verified on a cache HIT and not only
//! after a download: the artifact outlives this code, so "we already have it"
//! is not evidence that what we have is complete.
//!
//! # Absent configuration is not an error
//!
//! No bucket, no credentials ⇒ `None`, one WARN naming what is missing, and
//! `/api/bake/*` answers 503 while the embedded `dist/` copy keeps serving
//! `/helix` exactly as before. Nothing here can take the working route down.

use std::path::{Path, PathBuf};

use object_store::aws::AmazonS3Builder;

use crate::osm_slab_hydrate::{
    CacheDecision, download_verified, env_var_nonempty, fetch_sums, resolve_cache_hit,
};

/// Log prefix, so a boot log distinguishes this from the OSM slab's lines.
const LABEL: &str = "body bake";

/// The release tag the artifact lives under. `BODY_BAKE_TAG` selects another.
const DEFAULT_TAG: &str = "fma-body-v3-v1";

/// The artifact within that tag. `BODY_BAKE_ASSET` selects another.
const DEFAULT_ASSET: &str = "body.20260629c.v6helix.soa.gz";

/// Where a hydrated copy lands, in the same order [`crate::osm_lifecycle`]
/// resolves the OSM root — an explicit override, then a declared volume, then
/// the platform default mount, then an absolute temp path.
///
/// The temp fallback is deliberate and last: a relative path would follow the
/// process CWD, which is the failure `medcare-rs::bake_hydrate::absolutize`
/// documents (a handler resolving the store after a `set_current_dir` addresses
/// a *different* directory while logging the same string).
fn cache_dir() -> PathBuf {
    if let Some(p) = env_var_nonempty("BODY_BAKE_DIR") {
        return PathBuf::from(p);
    }
    if let Some(v) = env_var_nonempty("RAILWAY_VOL") {
        return Path::new(&v).join("body");
    }
    let vol01 = Path::new("/volume01");
    if vol01.is_dir()
        && !vol01
            .metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    {
        return vol01.join("body");
    }
    std::env::temp_dir().join("q2-body")
}

/// A name that is interpolated into BOTH an S3 key and a filesystem path.
///
/// The validation is not decoration: `..`, `/`, or a backslash in one of these
/// environment variables would read a different prefix and write outside the
/// cache directory. Restricting the alphabet makes both uses safe by
/// construction instead of by careful escaping at each site.
fn is_safe_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !s.contains("..")
}

fn from_env_or(key: &str, default: &str) -> String {
    match env_var_nonempty(key) {
        None => default.to_string(),
        Some(v) if is_safe_name(&v) => v,
        Some(bad) => {
            tracing::warn!(
                rejected = %bad, using = default, key,
                "{LABEL}: name must be [A-Za-z0-9._-] with no `..`; ignoring it"
            );
            default.to_string()
        }
    }
}

/// The tag and asset this deploy serves.
#[must_use]
pub fn coordinates() -> (String, String) {
    (
        from_env_or("BODY_BAKE_TAG", DEFAULT_TAG),
        from_env_or("BODY_BAKE_ASSET", DEFAULT_ASSET),
    )
}

/// The local path the artifact would occupy, whether or not it is there yet.
#[must_use]
pub fn local_path() -> PathBuf {
    cache_dir().join(coordinates().1)
}

/// Resolve a local, checksum-verified copy of the body bake, hydrating from S3
/// if needed.
///
/// Returns `None` — never panics, never returns an unverified path — when the
/// object store is not configured, the checksum file is absent, or the transfer
/// fails. Every one of those says so at WARN and leaves the embedded `dist/`
/// copy serving.
pub async fn ensure_body_bake_local() -> Option<PathBuf> {
    let (tag, asset) = coordinates();

    let Some(bucket) = env_var_nonempty("AWS_S3_BUCKET_NAME") else {
        tracing::warn!(
            "{LABEL}: AWS_S3_BUCKET_NAME is unset — /api/bake/* will answer 503 and \
             the embedded bake keeps serving /helix"
        );
        return None;
    };

    let dir = cache_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(dir = %dir.display(), error = %e, "{LABEL}: cannot create cache dir");
        return None;
    }
    let dest = dir.join(&asset);

    let store = match AmazonS3Builder::from_env()
        .with_bucket_name(&bucket)
        .build()
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "{LABEL}: S3 client build failed");
            return None;
        }
    };

    let prefix = format!("q2/bakes/{tag}");
    let sums = fetch_sums(LABEL, &store, &prefix).await?;
    let want = sums
        .iter()
        .find(|(n, _)| *n == asset)
        .map(|(_, h)| h.clone());
    let Some(want) = want else {
        tracing::error!(%asset, %prefix, "{LABEL}: no checksum pinned for this asset; refusing");
        return None;
    };

    if dest.is_file() {
        match resolve_cache_hit(LABEL, &dest, &want) {
            CacheDecision::TrustedViaMarker | CacheDecision::Verified => {
                tracing::info!(path = %dest.display(), "{LABEL}: cache hit");
                return Some(dest);
            }
            CacheDecision::Mismatch(got) => {
                tracing::warn!(%got, %want, "{LABEL}: cached copy failed its checksum; re-fetching")
            }
            CacheDecision::Unreadable(e) => {
                tracing::warn!(error = %e, "{LABEL}: cannot hash cached copy; re-fetching")
            }
        }
    }

    if !download_verified(LABEL, &store, &prefix, &asset, &dest, &want).await {
        return None;
    }
    tracing::info!(path = %dest.display(), %tag, "{LABEL}: hydrated and verified");
    Some(dest)
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
        // A name that is only ALMOST traversal still has to survive, or the
        // guard is the kind that fires on everything and discriminates nothing.
        assert!(is_safe_name("v4.1"));
    }

    #[test]
    fn cache_dir_prefers_an_explicit_override_over_the_volume() {
        // SAFETY: single-threaded test, and both variables are restored below.
        unsafe {
            std::env::set_var("BODY_BAKE_DIR", "/tmp/explicit-body");
            std::env::set_var("RAILWAY_VOL", "/tmp/vol");
        }
        assert_eq!(cache_dir(), PathBuf::from("/tmp/explicit-body"));
        unsafe { std::env::remove_var("BODY_BAKE_DIR") };
        assert_eq!(cache_dir(), PathBuf::from("/tmp/vol/body"));
        unsafe { std::env::remove_var("RAILWAY_VOL") };
    }

    #[test]
    fn a_rejected_tag_falls_back_to_the_default_rather_than_reading_it() {
        // SAFETY: single-threaded test; the variable is removed below.
        unsafe { std::env::set_var("BODY_BAKE_TAG", "../secrets") };
        assert_eq!(from_env_or("BODY_BAKE_TAG", DEFAULT_TAG), DEFAULT_TAG);
        unsafe { std::env::remove_var("BODY_BAKE_TAG") };
    }
}
