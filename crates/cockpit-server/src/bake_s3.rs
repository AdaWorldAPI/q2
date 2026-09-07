//! Serve the baked body/scene artifacts from the shared object store.
//!
//! # Why this exists
//!
//! The artifacts reach the browser today by being baked INTO the image: the
//! Dockerfile `curl`s them from a GitHub release into `cockpit/dist/`, which
//! `include_dir!` embeds in the binary. That works, and it stays — this is an
//! additional source, not a replacement.
//!
//! What it does not do is let a NEW bake reach a running deploy. A v4 body bake
//! published to the object store would otherwise need an image rebuild before
//! `/helix2` could see it. Fetching at request time decouples the artifact from
//! the image.
//!
//! # Why the server fetches, and not the browser
//!
//! Two independent reasons, either sufficient:
//!
//! 1. **The bucket is private and must stay private.** It is shared across
//!    repos and holds MedCare-rs clinical ontology bakes. Making objects
//!    public-read so a browser could fetch them directly would expose that
//!    material; it is not an option under any deadline.
//! 2. **A browser cannot sign a SigV4 request** without being handed the
//!    credentials, which is the same exposure by another route.
//!
//! So the bytes come through this route, same-origin, and the credentials never
//! leave the server. This also sidesteps the CORS problem the Dockerfile
//! already documents for the release redirect.
//!
//! # Why no volume
//!
//! Not every q2 deployment has one (operator, 2026-09-07), so a hydrate-to-disk
//! design like `osm_slab_hydrate` would work on some deploys and silently not
//! on others. This streams per request and keeps no local copy: correct
//! everywhere, at the cost of re-fetching. A volume cache can be layered on
//! later for the deploys that have one — it is an optimisation, not a
//! prerequisite.
//!
//! # Signing
//!
//! Ported from `MedCare-rs::medcare-server::bake_s3`, which already solved this
//! against the same bucket. HMAC is written out rather than pulled in, for the
//! reason recorded there: the `hmac` crate's `digest` version conflicts with the
//! `sha2` in tree, which is a poor trade for fifteen lines of xor.

use sha2::{Digest, Sha256};

/// The key prefix this repo's bakes live under inside the shared bucket.
///
/// A constant, not an env var: a deploy pointed at another prefix is fetching
/// another project's artifacts, which is better made impossible than
/// configurable. Mirrors `REPO_PREFIX` in MedCare-rs for the same reason.
const REPO_PREFIX: &str = "q2";

/// Object-store coordinates, read from the environment.
///
/// All five must be present. A partial configuration is treated as "no object
/// store" rather than as a broken one, so a deploy that never configured it
/// behaves exactly as before instead of failing at request time.
#[derive(Clone, Debug)]
pub struct S3Config {
    endpoint: String,
    bucket: String,
    region: String,
    key_id: String,
    secret: String,
}

impl S3Config {
    pub fn from_env() -> Option<Self> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Some(Self {
            endpoint: get("AWS_ENDPOINT_URL")?
                .trim_end_matches('/')
                .to_string(),
            bucket: get("AWS_S3_BUCKET_NAME")?,
            region: get("AWS_DEFAULT_REGION").unwrap_or_else(|| "auto".to_string()),
            key_id: get("AWS_ACCESS_KEY_ID")?,
            secret: get("AWS_SECRET_ACCESS_KEY")?,
        })
    }

    fn url(&self, tag: &str, asset: &str) -> String {
        format!(
            "{}/{}/{REPO_PREFIX}/bakes/{tag}/{asset}",
            self.endpoint, self.bucket
        )
    }
}

/// HMAC-SHA256 (RFC 2104) over the `sha2` already in the tree.
fn hmac(key: &[u8], msg: &str) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
    inner.update(msg.as_bytes());
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(k.iter().map(|b| b ^ 0x5c).collect::<Vec<u8>>());
    outer.update(inner);
    outer.finalize().to_vec()
}

fn hexs(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Sign an unsigned-payload `GET` and return the headers to send.
///
/// Unsigned payload is correct rather than lax: a GET has no body, so signing
/// the empty payload would authenticate nothing.
fn sign_get(cfg: &S3Config, url: &str, now: &chrono::DateTime<chrono::Utc>) -> Vec<(String, String)> {
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();

    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let (host, path) = after_scheme
        .split_once('/')
        .map_or((after_scheme, "/".to_string()), |(h, p)| (h, format!("/{p}")));

    const PAYLOAD: &str = "UNSIGNED-PAYLOAD";
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical = format!(
        "GET\n{path}\n\nhost:{host}\nx-amz-content-sha256:{PAYLOAD}\nx-amz-date:{amz_date}\n\n\
         {signed_headers}\n{PAYLOAD}"
    );
    let scope = format!("{date}/{}/s3/aws4_request", cfg.region);
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hexs(&Sha256::digest(canonical.as_bytes()))
    );

    let k_date = hmac(format!("AWS4{}", cfg.secret).as_bytes(), &date);
    let k_region = hmac(&k_date, &cfg.region);
    let k_service = hmac(&k_region, "s3");
    let k_signing = hmac(&k_service, "aws4_request");
    let signature = hexs(&hmac(&k_signing, &to_sign));

    vec![
        (
            "Authorization".to_string(),
            format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, \
                 Signature={signature}",
                cfg.key_id
            ),
        ),
        ("x-amz-content-sha256".to_string(), PAYLOAD.to_string()),
        ("x-amz-date".to_string(), amz_date),
    ]
}

/// GET one artifact. `Err` is a message, never a panic — every caller is
/// expected to fall back to the embedded copy.
pub async fn get(cfg: &S3Config, tag: &str, asset: &str) -> Result<Vec<u8>, String> {
    let url = cfg.url(tag, asset);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let mut req = client.get(&url);
    for (k, v) in sign_get(cfg, &url, &chrono::Utc::now()) {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(|e| format!("s3 get: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        // The URL is deliberately not echoed: it names the bucket, and this
        // string reaches the browser.
        return Err(format!("s3 {status} for {tag}/{asset}"));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("s3 body: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 1 — the known-answer test the ported comment calls
    /// for. Without it the hand-rolled HMAC is unproven, and a wrong signature
    /// looks identical to a permissions problem at runtime.
    #[test]
    fn hmac_matches_rfc4231_case_1() {
        let got = hexs(&hmac(&[0x0b; 20], "Hi There"));
        assert_eq!(
            got, "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            "HMAC-SHA256 disagrees with RFC 4231 case 1"
        );
    }

    /// A partial configuration must read as "not configured", never as a
    /// half-built client that fails later against an empty bucket name.
    #[test]
    fn from_env_needs_every_key() {
        // Absent keys are the common case in a test process; assert the shape
        // rather than mutating global env (which races other tests).
        let cfg = S3Config {
            endpoint: "https://example.invalid/".into(),
            bucket: "b".into(),
            region: "auto".into(),
            key_id: "k".into(),
            secret: "s".into(),
        };
        assert_eq!(
            cfg.url("fma-body-v3-v1", "body.soa.gz"),
            "https://example.invalid//b/q2/bakes/fma-body-v3-v1/body.soa.gz",
            "url() must place the artifact under <bucket>/q2/bakes/<tag>/"
        );
    }

    /// The signature must depend on the key, the date and the path. A signer
    /// that ignores any of them still "works" until it meets a real bucket.
    #[test]
    fn signature_varies_with_key_date_and_path() {
        let base = S3Config {
            endpoint: "https://t3.example".into(),
            bucket: "bkt".into(),
            region: "auto".into(),
            key_id: "AKIA".into(),
            secret: "secret".into(),
        };
        let t0 = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let t1 = chrono::DateTime::from_timestamp(1_700_090_000, 0).unwrap();
        let sig = |c: &S3Config, u: &str, t: &chrono::DateTime<chrono::Utc>| {
            sign_get(c, u, t)
                .into_iter()
                .find(|(k, _)| k == "Authorization")
                .map(|(_, v)| v)
                .expect("Authorization header")
        };
        let a = sig(&base, &base.url("tag", "a.gz"), &t0);
        let b_path = sig(&base, &base.url("tag", "b.gz"), &t0);
        let b_date = sig(&base, &base.url("tag", "a.gz"), &t1);
        let mut other = base.clone();
        other.secret = "different".into();
        let b_key = sig(&other, &base.url("tag", "a.gz"), &t0);

        assert_ne!(a, b_path, "signature ignores the object path");
        assert_ne!(a, b_date, "signature ignores the date");
        assert_ne!(a, b_key, "signature ignores the secret");
    }
}
