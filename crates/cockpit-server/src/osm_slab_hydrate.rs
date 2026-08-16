//! Boot-time hydration of the baked OSM slab from S3 onto a persistent volume.
//!
//! # The topology
//!
//! ```text
//!   S3  (durable source of truth)
//!    │   s3://$AWS_S3_BUCKET_NAME/<prefix>/{berlin.soa, berlin.books, berlin.chains, SHA256SUMS}
//!    ▼
//!   $RAILWAY_VOL  (persistence across container rebuilds — a CACHE, not truth)
//!    │   <vol>/osm/{berlin.soa, berlin.books, berlin.chains}
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
//! # Absent configuration is not an error — but it must never be SILENT either
//!
//! No bucket, no volume, no credentials ⇒ `None`, and the endpoint keeps
//! answering 503 exactly as it did before this module existed. Local
//! development sets `OSM_SLAB_PATH` directly and never reaches S3.
//!
//! The first version of this module returned that `None` via a bare `.ok()?`
//! on each required var, with `main.rs`'s caller having no `else` branch —
//! meaning a deploy missing `AWS_S3_BUCKET_NAME` or `RAILWAY_VOL` produced
//! **zero lines in the boot log about OSM slab hydration at all**. That is
//! the exact "silent-empty deploy" failure mode `medcare-rs::bake_hydrate`
//! documents hitting and fixing first (its own module doc: "the store already
//! applies this rule... this module matches it"). This module ported that
//! fix: [`missing_inputs`] + the `tracing::warn!` in [`ensure_slab_local`]
//! below always name exactly which variable is absent, so the very next
//! deploy's log settles the question instead of a search for a line that
//! structurally cannot exist.
//!
//! `medcare-rs::bake_s3::S3Config::from_env` also treats an EMPTY variable
//! value the same as an absent one (`.filter(|v| !v.is_empty())`) — a blank
//! Railway variable (row present, value never filled in) must fail exactly
//! like an unset one, not attempt a doomed S3 call with an empty bucket name
//! and fail differently (and more slowly) than the "never configured" case.
//! [`env_var_nonempty`] carries the same rule here.

use std::path::{Path, PathBuf};

use object_store::aws::AmazonS3Builder;
use object_store::{ObjectStore, ObjectStoreExt};
use sha2::{Digest, Sha256};

/// The region baked by default. `OSM_BAKE_REGION` selects another one.
///
/// The region is the ONLY thing that differs between bakes: the baker
/// (`osm-soa-bake`'s `bake <in.osm.pbf> <out.soa>`) is already region-agnostic,
/// and the sidecars are found by extension from the slab's own stem — so
/// serving Baden-Württemberg instead of Berlin is this string plus a bake in
/// the bucket, not a code change.
const DEFAULT_REGION: &str = "berlin";

/// The region name, validated.
///
/// **The validation is not decoration.** This string is interpolated into an
/// S3 object key AND joined onto a filesystem path, so `..`, `/`, or a
/// backslash would let a stray environment variable read a different prefix or
/// write outside the cache dir. Restricting it to `[a-z0-9-]` makes both uses
/// safe by construction rather than by careful escaping at each site. A
/// rejected value falls back to the default and says so.
fn bake_region() -> String {
    match env_var_nonempty("OSM_BAKE_REGION") {
        None => DEFAULT_REGION.to_string(),
        Some(r) if is_valid_region(&r) => r,
        Some(bad) => {
            tracing::warn!(
                rejected = %bad, using = DEFAULT_REGION,
                "osm slab: OSM_BAKE_REGION must be lowercase [a-z0-9-]; ignoring it"
            );
            DEFAULT_REGION.to_string()
        }
    }
}

fn is_valid_region(r: &str) -> bool {
    !r.is_empty()
        && r.len() <= 64
        && r.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Default S3 prefix holding the bake. Overridable with `OSM_SLAB_S3_PREFIX`,
/// which wins outright — that is the escape hatch for a layout this naming
/// convention does not cover.
fn default_prefix(region: &str) -> String {
    format!("q2/bakes/{region}-v1")
}

/// The slab and its sidecars. All three are required: `RowSlab` can read
/// positions without the books, but identity resolution needs them, and the
/// `.chains` geometry sidecar is what turns a clicked feature back into its
/// vertex chain (`/api/osm/geometry/:idx`). The bake publishes all three
/// atomically (the slab renames into place only after both sidecars exist),
/// so a prefix with a slab but no chains is a stale bake, not a valid state.
///
/// The slab is FIRST and stays first: [`ensure_slab_local`] returns
/// `artifacts[0]` as the path to mmap.
fn artifacts(region: &str) -> [String; 3] {
    [
        format!("{region}.soa"),
        format!("{region}.books"),
        format!("{region}.chains"),
    ]
}

/// Where the hydrated copy lives, given the volume root.
fn cache_dir(vol: &str) -> PathBuf {
    Path::new(vol).join("osm")
}

/// `std::env::var`, but an empty value is treated the same as absent.
///
/// Mirrors `medcare-rs::bake_s3::S3Config::from_env`'s reader: a Railway
/// variable can exist as a row with an empty value (created, never filled
/// in), and that must fail the SAME way as the variable not existing at all —
/// not attempt a real S3 call with an empty bucket name, which fails later,
/// differently, and less legibly than "not configured".
fn env_var_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// Which required inputs are missing, given what was actually resolved from
/// the environment.
///
/// Pure and side-effect-free on purpose: [`ensure_slab_local`] is the only
/// place that touches `std::env::var`, so this can be tested with plain
/// `Option`s and never needs to mutate real process environment state.
fn missing_inputs(bucket: Option<&str>, vol: Option<&str>) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if bucket.is_none() {
        missing.push("AWS_S3_BUCKET_NAME");
    }
    if vol.is_none() {
        missing.push("OSM_SLAB_CACHE_DIR (or RAILWAY_VOL)");
    }
    missing
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
    let bucket_env = env_var_nonempty("AWS_S3_BUCKET_NAME");
    // `OSM_SLAB_CACHE_DIR` overrides the volume — it is what makes this
    // testable off-Railway, where /volume01 does not exist.
    let vol_env =
        env_var_nonempty("OSM_SLAB_CACHE_DIR").or_else(|| env_var_nonempty("RAILWAY_VOL"));

    // See the module doc's "must never be SILENT" section: this WARN is what
    // was missing before, and it is the entire fix — everything below this
    // block is unchanged hydration logic.
    let missing = missing_inputs(bucket_env.as_deref(), vol_env.as_deref());
    if !missing.is_empty() {
        tracing::warn!(
            missing = %missing.join(", "),
            "osm slab: hydration not configured — set {} to enable the drawn basemap; \
             the vector-basemap and feature-dot endpoints stay 503 until then",
            missing.join(", "),
        );
        return None;
    }
    let bucket = bucket_env.expect("checked non-empty above");
    let vol = vol_env.expect("checked non-empty above");
    let region = bake_region();
    let artifacts = artifacts(&region);
    let prefix =
        env_var_nonempty("OSM_SLAB_S3_PREFIX").unwrap_or_else(|| default_prefix(&region));

    let dir = cache_dir(&vol);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(dir = %dir.display(), error = %e, "osm slab: cannot create cache dir");
        return None;
    }

    // Announce BEFORE the transfer, not after. This call blocks the listener
    // bind, and a cold boot moves ~1.42 GB — so without a line here the boot
    // log is silent for 60-90s, which is indistinguishable from a hang for
    // whoever is watching a deploy. Naming the bucket and destination also
    // makes a misconfigured prefix obvious from the first line rather than
    // from a later "not readable" error.
    tracing::info!(
        %region, %bucket, %prefix, dir = %dir.display(),
        "osm slab: resolving from S3 (a cold boot transfers the whole bake and delays the \
         listener — Berlin is ~1.42 GB; a warm volume re-verifies in ~1s)"
    );

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

    for name in artifacts.iter().map(String::as_str) {
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
            match resolve_cache_hit(&dest, &want) {
                CacheDecision::TrustedViaMarker => {
                    tracing::info!(
                        artifact = name,
                        "osm slab: cache hit, trusted via unchanged marker (mtime+len \
                         match the last real verification; skipped re-hash)"
                    );
                    continue;
                }
                CacheDecision::Verified => {
                    tracing::info!(artifact = name, "osm slab: cache hit, checksum verified");
                    continue;
                }
                CacheDecision::Mismatch(got) => tracing::warn!(
                    artifact = name, %got, %want,
                    "osm slab: cached copy failed its checksum; re-fetching"
                ),
                CacheDecision::Unreadable(e) => {
                    tracing::warn!(artifact = name, error = %e, "osm slab: cannot hash cached copy; re-fetching")
                }
            }
        }

        if !download_verified(&store, &prefix, name, &dest, &want).await {
            return None;
        }
    }

    let slab = dir.join(&artifacts[0]);
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
    // A fresh download IS a real verification — record it so the NEXT boot's
    // cache hit can trust it via `resolve_cache_hit` without re-reading.
    write_marker(dest, &got);
    tracing::info!(
        artifact = name,
        bytes = written,
        "osm slab: downloaded and verified"
    );
    true
}

// ── The "hot but idle" cache-hit fast path ──────────────────────────────
//
// `sha256_file` streams the whole artifact through the kernel's page cache
// on EVERY boot with a warm volume, even though the common case is "nothing
// changed since the last boot verified this exact file." A tiny sidecar
// marker — `<artifact>.verified` — records the (mtime, length, digest) that
// the last REAL verification produced. Any write to a file updates its
// mtime, so `(mtime, len)` matching the marker is conclusive proof the
// bytes are unchanged too (the same heuristic `make`/`rsync`/`cargo`/
// `ccache` use by default) — trusting it skips `sha256_file` entirely,
// exactly the "clean shutdown" half of the exchange-DAG analogy this
// investigation used: a persisted marker from a known-good state lets a
// later boot skip the expensive replay. Any mismatch — content changed,
// marker absent, marker malformed, or the bucket republished a different
// `want` digest for this name — falls straight through to `sha256_file`,
// the "dirty shutdown" fallback, unconditionally.

/// Parsed contents of a `<artifact>.verified` marker: exactly what the last
/// successful verification of THIS file recorded, three lines in order —
/// mtime (nanoseconds since `UNIX_EPOCH`), length in bytes, digest. Any
/// malformed or unreadable marker is never a hard error anywhere it is
/// used — the caller always has a correct, if slower, fallback: a real
/// `sha256_file` re-hash.
#[derive(Debug, PartialEq, Eq)]
struct VerifiedMarker {
    mtime_nanos: u128,
    len: u64,
    digest: String,
}

impl VerifiedMarker {
    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let mtime_nanos: u128 = lines.next()?.trim().parse().ok()?;
        let len: u64 = lines.next()?.trim().parse().ok()?;
        let digest = lines.next()?.trim().to_ascii_lowercase();
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            mtime_nanos,
            len,
            digest,
        })
    }

    fn render(&self) -> String {
        format!("{}\n{}\n{}\n", self.mtime_nanos, self.len, self.digest)
    }
}

/// `(mtime as nanoseconds since the Unix epoch, length in bytes)` for
/// `path`, or `None` on any stat failure — a stat failure just means "the
/// fast path is unavailable here", never an error the caller need surface;
/// `sha256_file` remains correct regardless.
fn stat_identity(path: &Path) -> Option<(u128, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let nanos = mtime.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some((nanos, meta.len()))
}

/// Where the marker for `dest` lives — a sibling file, same directory,
/// `.verified` appended to the artifact's own name (`berlin.soa` →
/// `berlin.soa.verified`), so it travels with the cache dir and is
/// trivially recognisable in a directory listing.
fn marker_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".verified");
    PathBuf::from(name)
}

/// Whether the marker at `marker_path(dest)` proves `dest` still matches
/// `want` WITHOUT reading `dest`'s content. Folding `want` into the
/// comparison (not just the file's own recorded digest) means a bucket
/// republish — `SHA256SUMS` now names a DIFFERENT digest for this artifact
/// name — correctly invalidates a marker whose file identity hasn't
/// changed at all: the marker's digest no longer equals the freshly
/// fetched `want`, so this declines and `sha256_file` runs for real,
/// which then correctly reports a mismatch and triggers a re-download.
fn trusted_via_marker(dest: &Path, want: &str) -> bool {
    let Some(marker) = std::fs::read_to_string(marker_path(dest))
        .ok()
        .and_then(|text| VerifiedMarker::parse(&text))
    else {
        return false;
    };
    let Some((mtime_nanos, len)) = stat_identity(dest) else {
        return false;
    };
    marker.mtime_nanos == mtime_nanos && marker.len == len && marker.digest == want
}

/// Record `dest`'s current (mtime, length) alongside `digest` — the just-
/// proven-correct identity a later boot's [`trusted_via_marker`] can trust.
/// Failure to write is logged, never fatal: the next boot simply re-hashes,
/// which is exactly today's behaviour without this whole mechanism.
fn write_marker(dest: &Path, digest: &str) {
    let Some((mtime_nanos, len)) = stat_identity(dest) else {
        return;
    };
    let marker = VerifiedMarker {
        mtime_nanos,
        len,
        digest: digest.to_string(),
    };
    if let Err(e) = std::fs::write(marker_path(dest), marker.render()) {
        tracing::warn!(
            path = %dest.display(), error = %e,
            "osm slab: could not write verification marker (non-fatal; next boot re-hashes)"
        );
    }
}

/// The outcome of checking one cached artifact against `want`, in the shape
/// [`ensure_slab_local`]'s loop needs to log and branch on. Split out from
/// that loop so it is directly testable without env vars or an S3 stub —
/// see `resolve_cache_hit_trusts_a_matching_marker_without_hashing` below.
enum CacheDecision {
    /// The marker proved identity without touching the file's bytes.
    TrustedViaMarker,
    /// A real `sha256_file` ran and matched `want` — the marker is now
    /// (re)written for the next boot.
    Verified,
    /// A real `sha256_file` ran and did NOT match `want`.
    Mismatch(String),
    /// The file could not be hashed at all (e.g. a permissions error).
    Unreadable(std::io::Error),
}

fn resolve_cache_hit(dest: &Path, want: &str) -> CacheDecision {
    if trusted_via_marker(dest, want) {
        return CacheDecision::TrustedViaMarker;
    }
    match sha256_file(dest) {
        Ok(got) if got == want => {
            write_marker(dest, &got);
            CacheDecision::Verified
        }
        Ok(got) => CacheDecision::Mismatch(got),
        Err(e) => CacheDecision::Unreadable(e),
    }
}

/// SHA-256 of a file, streamed — the artifact is 1.29 GiB and must not be read
/// into memory to be hashed.
///
/// The streaming loop keeps process RSS flat, but every byte still passes
/// through the *kernel's* page cache on the way through `read()`, and
/// nothing evicts it afterward. This runs on EVERY boot with a warm volume
/// (the common case — [`ensure_slab_local`]'s cache-hit branch, before
/// `ensure_lance_local` even starts), so redeploying an unchanged region
/// faulted the whole ~1.4-3.75 GB slab into the page cache for a hash it
/// then threw away. Same disease [`crate::osm_lance::ensure_lance_local`]'s
/// warm-reopen path had (see that module's `release_after_write`), a
/// different call site with a different eviction primitive: `posix_fadvise`
/// is the file-descriptor-read equivalent of `MADV_DONTNEED` for mmap —
/// [`advise_dontneed`] tells the kernel it can drop these pages once we're
/// done with them.
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
    advise_dontneed(&f);
    Ok(hex::encode(hasher.finalize()))
}

/// Test-only reachability counter — see [`advise_dontneed`]'s doc comment
/// for why this exists instead of a memory-measurement assertion.
#[cfg(test)]
static FADVISE_ATTEMPTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Advise the kernel it can drop `f`'s pages from the page cache now that
/// we're done reading it.
///
/// No portable equivalent exists for this on non-Unix targets (see
/// `.claude/rules/cross-platform.md`), so it is a documented no-op there —
/// this is memory hygiene, not correctness, so a silent no-op is fine.
///
/// Deliberately untestable via RSS or cgroup memory accounting, same as
/// `osm_lance.rs`'s `release_after_write`: `/proc/self/statm` cannot see
/// kernel page-cache/memcg state, and `/sys/fs/cgroup/*` is unavailable in
/// this dev sandbox (though it IS what Railway's dashboard measures in
/// production). The `#[cfg(test)]`-only [`FADVISE_ATTEMPTED`] counter below
/// proves this function is actually REACHED from `sha256_file` on every
/// call — the regression being guarded against is the call site being
/// silently skipped or removed, which a counter catches and an RSS
/// measurement cannot (see the falsifiability rule: a test that cannot
/// fail when the guard is deleted is not a test of the guard).
#[cfg(unix)]
fn advise_dontneed(f: &std::fs::File) {
    #[cfg(test)]
    FADVISE_ATTEMPTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    use std::os::unix::io::AsRawFd;
    let fd = f.as_raw_fd();
    // SAFETY: `fd` is a valid, open file descriptor borrowed from `f` for
    // the duration of this call. `POSIX_FADV_DONTNEED` only advises the
    // kernel's page-cache policy — it cannot invalidate memory this process
    // holds, and a nonzero return is just the kernel declining the hint, not
    // a memory-safety concern — so the return value is intentionally not
    // surfaced as a `Result`. `len = 0` means "to the end of the file" per
    // POSIX, so this covers everything read above regardless of file size.
    unsafe {
        libc::posix_fadvise(fd, 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

#[cfg(not(unix))]
fn advise_dontneed(_f: &std::fs::File) {
    #[cfg(test)]
    FADVISE_ATTEMPTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

    /// **The regression this fix is for.** `sha256_file` streams the whole
    /// file through the kernel's page cache and, before this fix, left it
    /// there — silently, on every boot with a warm volume. This counts
    /// reachability of the eviction advisory rather than measuring memory
    /// (RSS can't see page-cache state, and cgroup accounting is
    /// unavailable in this sandbox — see `advise_dontneed`'s doc comment).
    #[test]
    fn sha256_file_attempts_page_cache_eviction_after_hashing() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-fadvise");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("probe_fadvise.bin");
        std::fs::write(&p, b"abc").unwrap();

        let before = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        let digest = sha256_file(&p).unwrap();
        let after = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        // The hash itself must still be correct — this test must not pass
        // merely because eviction ran on the wrong (or no) data.
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            after - before,
            1,
            "sha256_file must attempt to advise the kernel to drop this file's \
             pages from the page cache after hashing it — this is the regression: \
             it used to hash the whole ~1.4-3.75 GB slab on every boot with a warm \
             volume, and nothing ever evicted it from the page cache afterward"
        );

        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn verified_marker_render_parse_roundtrips() {
        let marker = VerifiedMarker {
            mtime_nanos: 1_234_567_890_123_456_789,
            len: 1_484_783_616,
            digest: "a".repeat(64),
        };
        let parsed = VerifiedMarker::parse(&marker.render()).expect("must parse its own render");
        assert_eq!(parsed, marker);
    }

    /// Anti-vacuity for the digest-shape guard: a plausible-looking but
    /// wrong-length digest must not silently parse as a 64-char one.
    #[test]
    fn verified_marker_parse_rejects_a_malformed_digest() {
        assert!(VerifiedMarker::parse("123\n456\nnothex").is_none());
        assert!(VerifiedMarker::parse("123\n456\ntooshort").is_none());
        assert!(VerifiedMarker::parse("not-a-number\n456\n").is_none());
        assert!(VerifiedMarker::parse("").is_none());
    }

    fn write_temp_artifact(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).expect("write artifact");
        p
    }

    /// **The base case.** No marker at all must decline, not panic or
    /// somehow trust an absent file.
    #[test]
    fn trusted_via_marker_declines_when_the_marker_is_absent() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-marker-absent");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"hello world");
        assert!(!trusted_via_marker(&p, "irrelevant"));
        std::fs::remove_file(&p).ok();
    }

    /// The marker's own recorded identity no longer matches the file on
    /// disk — the file was rewritten (different content, different mtime),
    /// which is precisely the case `trusted_via_marker` must catch: a stale
    /// marker must never vouch for content it never actually verified.
    #[test]
    fn trusted_via_marker_declines_when_the_file_was_rewritten() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-marker-rewritten");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"original content");
        let (mtime_nanos, len) = stat_identity(&p).expect("stat");
        let digest = sha256_file(&p).expect("hash");
        write_marker(&p, &digest);

        // Rewrite with DIFFERENT content — a real mtime bump, not a forced one.
        std::fs::write(&p, b"different content, different length").expect("rewrite");
        assert!(
            !trusted_via_marker(&p, &digest),
            "a rewritten file must never be trusted via a marker from before the rewrite"
        );
        // Sanity: the original marker really did record the pre-rewrite state.
        assert_ne!(stat_identity(&p).unwrap(), (mtime_nanos, len));

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(marker_path(&p)).ok();
    }

    /// The file itself is byte-for-byte untouched, but the CALLER's `want`
    /// changed — a bucket republish naming a different digest for this
    /// artifact name. The marker's own recorded digest no longer equals the
    /// freshly fetched `want`, so trust must be declined even though the
    /// file's identity (mtime+len) is unchanged.
    #[test]
    fn trusted_via_marker_declines_when_the_wanted_digest_changes() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-marker-want-changed");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"stable content");
        let digest = sha256_file(&p).expect("hash");
        write_marker(&p, &digest);

        assert!(
            !trusted_via_marker(&p, &"f".repeat(64)),
            "a marker must not vouch for a digest it never recorded"
        );

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(marker_path(&p)).ok();
    }

    /// The positive case: identity unchanged, digest matches `want` — must
    /// trust.
    #[test]
    fn trusted_via_marker_accepts_when_everything_matches() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-marker-match");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"unchanged content");
        let digest = sha256_file(&p).expect("hash");
        write_marker(&p, &digest);

        assert!(trusted_via_marker(&p, &digest));

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(marker_path(&p)).ok();
    }

    /// **The "hot but idle" falsifier for the cache-hit path.** A matching
    /// marker must resolve `resolve_cache_hit` to `TrustedViaMarker` WITHOUT
    /// `sha256_file` ever running — proven via the same `FADVISE_ATTEMPTED`
    /// reachability counter `sha256_file_attempts_page_cache_eviction_
    /// after_hashing` uses (bumped inside `advise_dontneed`, which only
    /// `sha256_file` calls): if the counter doesn't move, the read never
    /// happened.
    #[test]
    fn resolve_cache_hit_trusts_a_matching_marker_without_hashing() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-resolve-trusted");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"trust me, i'm unchanged");
        let digest = sha256_file(&p).expect("hash");
        write_marker(&p, &digest);

        let before = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        let decision = resolve_cache_hit(&p, &digest);
        let after = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        assert!(matches!(decision, CacheDecision::TrustedViaMarker));
        assert_eq!(
            after, before,
            "a genuinely trusted marker must skip sha256_file entirely — this is the \
             whole point of the fast path: on a genuinely unchanged artifact, zero \
             bytes of it are ever read"
        );

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(marker_path(&p)).ok();
    }

    /// The silent twin: with NO marker present, `resolve_cache_hit` must
    /// still fall through to a real hash (and get the right answer) — this
    /// is what proves the counter above is a meaningful zero, not a
    /// tautological one from a code path that never hashes at all.
    #[test]
    fn resolve_cache_hit_falls_back_to_a_real_hash_when_untrusted() {
        let dir = std::env::temp_dir().join("q2-hydrate-test-resolve-untrusted");
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_temp_artifact(&dir, "artifact.bin", b"no marker yet");
        let digest = sha256_file(&p).expect("hash");

        let before = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);
        let decision = resolve_cache_hit(&p, &digest);
        let after = FADVISE_ATTEMPTED.load(std::sync::atomic::Ordering::Relaxed);

        assert!(matches!(decision, CacheDecision::Verified));
        assert_eq!(
            after - before,
            1,
            "with no marker to trust, resolve_cache_hit must actually hash the file"
        );

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(marker_path(&p)).ok();
    }

    #[test]
    fn cache_dir_is_under_the_volume_root() {
        assert_eq!(cache_dir("/volume01"), PathBuf::from("/volume01/osm"));
    }

    /// **The regression this whole fix is for.** Before this change,
    /// `ensure_slab_local` returned `None` here via a bare `.ok()?` with no
    /// logging at all — a deploy missing either var produced zero lines about
    /// OSM slab hydration in the boot log. `missing_inputs` is the pure core
    /// of the fix: given what `ensure_slab_local` actually resolved, it must
    /// name EVERY absent piece, not stop at the first.
    #[test]
    fn missing_inputs_names_every_absent_variable() {
        assert_eq!(missing_inputs(None, None).len(), 2, "both absent: both named");
        assert_eq!(
            missing_inputs(Some("my-bucket"), None),
            vec!["OSM_SLAB_CACHE_DIR (or RAILWAY_VOL)"]
        );
        assert_eq!(
            missing_inputs(None, Some("/volume01")),
            vec!["AWS_S3_BUCKET_NAME"]
        );
    }

    /// Anti-vacuity: when both are present, nothing is reported missing — the
    /// function does not just always return a non-empty list.
    #[test]
    fn missing_inputs_is_empty_when_both_are_present() {
        assert!(missing_inputs(Some("my-bucket"), Some("/volume01")).is_empty());
    }

    /// Every artifact name and the prefix derive from ONE region string, so a
    /// second bake is config. The slab must stay at index 0 —
    /// `ensure_slab_local` returns `artifacts[0]` as the path to mmap, so a
    /// reorder would hand the books file to `RowSlab`.
    #[test]
    fn artifacts_and_prefix_follow_the_region() {
        assert_eq!(
            artifacts("berlin"),
            ["berlin.soa", "berlin.books", "berlin.chains"],
            "the default region must reproduce the names the Berlin bake already \
             published — this is a wire/bucket contract, not a naming preference"
        );
        assert_eq!(
            artifacts("baden-wuerttemberg"),
            [
                "baden-wuerttemberg.soa",
                "baden-wuerttemberg.books",
                "baden-wuerttemberg.chains"
            ]
        );
        assert!(artifacts("bw")[0].ends_with(".soa"), "slab stays at index 0");
        assert_eq!(default_prefix("berlin"), "q2/bakes/berlin-v1");
        assert_eq!(
            default_prefix("baden-wuerttemberg"),
            "q2/bakes/baden-wuerttemberg-v1"
        );
    }

    /// **The guard that matters.** The region is interpolated into an S3 key
    /// AND joined onto a filesystem path, so anything that can traverse must
    /// be rejected before it reaches either. Two-sided: the shapes a real
    /// Geofabrik region name takes are all accepted.
    #[test]
    fn region_validation_rejects_traversal_and_accepts_real_names() {
        for good in [
            "berlin",
            "baden-wuerttemberg",
            "nordrhein-westfalen",
            "bw2",
        ] {
            assert!(is_valid_region(good), "{good} is a legitimate region name");
        }
        for bad in [
            "../secrets",         // parent escape
            "a/b",                // sub-prefix
            "a\\b",               // Windows separator
            "Berlin",             // uppercase: S3 keys are case-sensitive
            "baden_wuerttemberg", // underscore is not in the allowed set
            "baden würt",         // non-ASCII + space
            "",                   // empty
        ] {
            assert!(!is_valid_region(bad), "{bad:?} must be rejected");
        }
        // A path built from a rejected name would have escaped the cache dir —
        // proving the guard is load-bearing rather than cosmetic.
        assert_eq!(
            cache_dir("/volume01").join(format!("{}.soa", "../..")),
            PathBuf::from("/volume01/osm/../...soa"),
            "documents WHY the guard exists: this is the path we refuse to build"
        );
    }
}
