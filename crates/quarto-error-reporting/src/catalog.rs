//! Pluggable error-code catalog.
//!
//! `quarto-error-reporting` is **catalog-agnostic**: it defines the catalog
//! *shape* ([`ErrorCodeInfo`]) and a [`CatalogProvider`] seam, but ships no
//! catalog *data*. An embedding product installs its own catalog once, early,
//! via [`install_catalog`]; in Quarto this is done by the `quarto-error-catalog`
//! crate (`quarto_error_catalog::install()`), which carries the `Q-*`
//! `error_catalog.json`. With nothing installed, every lookup returns `None`
//! (see [`EmptyCatalog`]).
//!
//! This is the host side of the cross-package error-code discipline; see
//! `claude-notes/designs/cross-package-error-codes.md`.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Metadata for an error code.
///
/// Each catalog entry describes a specific error code, including its
/// subsystem, title, default message, and documentation URL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorCodeInfo {
    /// Subsystem name (e.g., "yaml", "markdown", "engine")
    pub subsystem: String,

    /// Short title for the error
    pub title: String,

    /// Default message template (may include placeholders)
    pub message_template: String,

    /// URL to documentation (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,

    /// When this error was introduced (version)
    pub since_version: String,
}

/// A source of error-code metadata, supplied by the embedding product.
///
/// Implementors return metadata for a given code, or `None` if the code is not
/// in their catalog. The returned reference is tied to `&self`, which lets the
/// installed-global path (see [`install_catalog`]) hand out `&'static`
/// references — the installed provider lives for the rest of the process.
///
/// `Send + Sync` is required so the provider can live in a process-wide
/// [`OnceLock`]; it costs nothing for the data-only providers in practice.
pub trait CatalogProvider: Send + Sync {
    /// Look up an error code's metadata, or `None` if it is not in this catalog.
    fn lookup(&self, code: &str) -> Option<&ErrorCodeInfo>;
}

/// The default provider used when none has been installed: every lookup is
/// `None`. This is what makes the crate usable standalone with zero config — a
/// non-Quarto consumer that installs nothing simply gets code-less, URL-less
/// diagnostics (tier-2 "passthrough" in the discipline's terms).
pub struct EmptyCatalog;

impl CatalogProvider for EmptyCatalog {
    fn lookup(&self, _code: &str) -> Option<&ErrorCodeInfo> {
        None
    }
}

/// The process-wide installed catalog. Written at most once, by the embedder.
static CATALOG: OnceLock<Box<dyn CatalogProvider>> = OnceLock::new();

/// Install the process-wide catalog provider.
///
/// The **first** call wins; later calls are no-ops (so a double install — e.g.
/// a binary's `main` plus a test helper — is harmless). Embedders should call
/// this once, as early as possible, at binary / WASM startup, *before* any
/// diagnostic's docs URL is resolved.
pub fn install_catalog(provider: Box<dyn CatalogProvider>) {
    let _ = CATALOG.set(provider);
}

/// The installed provider, or a shared [`EmptyCatalog`] if none was installed.
fn catalog() -> &'static dyn CatalogProvider {
    static EMPTY: EmptyCatalog = EmptyCatalog;
    match CATALOG.get() {
        Some(provider) => &**provider,
        None => &EMPTY,
    }
}

/// Look up full metadata for an error code via the installed catalog.
///
/// Returns `None` if no catalog is installed, or the code is not in it.
///
/// # Example
///
/// ```
/// use quarto_error_reporting::catalog::get_error_info;
///
/// // With a `CatalogProvider` installed (e.g. via `quarto-error-catalog`),
/// // this resolves to the code's metadata; with none installed it is `None`.
/// let _ = get_error_info("Q-0-1");
/// ```
pub fn get_error_info(code: &str) -> Option<&'static ErrorCodeInfo> {
    catalog().lookup(code)
}

/// Get the documentation URL for an error code, if the installed catalog has one.
///
/// Returns `None` if no catalog is installed, the code is unknown, or the entry
/// has no documentation URL.
///
/// # Example
///
/// ```
/// use quarto_error_reporting::catalog::get_docs_url;
///
/// // `Some(url)` iff a catalog mapping this code (with a docs URL) is installed.
/// let _ = get_docs_url("Q-0-1");
/// ```
pub fn get_docs_url(code: &str) -> Option<&'static str> {
    catalog()
        .lookup(code)
        .and_then(|info| info.docs_url.as_deref())
}

/// Get the subsystem name for an error code, if the installed catalog knows it.
///
/// Returns `None` if no catalog is installed or the code is unknown.
///
/// # Example
///
/// ```
/// use quarto_error_reporting::catalog::get_subsystem;
///
/// // With a catalog installed this returns e.g. `Some("internal")` for "Q-0-1".
/// let _ = get_subsystem("Q-0-1");
/// ```
pub fn get_subsystem(code: &str) -> Option<&'static str> {
    catalog().lookup(code).map(|info| info.subsystem.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info(subsystem: &str, docs_url: Option<&str>) -> ErrorCodeInfo {
        ErrorCodeInfo {
            subsystem: subsystem.to_string(),
            title: "Sample".to_string(),
            message_template: "sample".to_string(),
            docs_url: docs_url.map(str::to_string),
            since_version: "0.0.0".to_string(),
        }
    }

    /// The default provider returns `None` for everything. Tested *directly*
    /// (no global state) so it is robust regardless of test runner — this is
    /// the canonical "no catalog installed → None" behaviour.
    #[test]
    fn empty_catalog_returns_none() {
        let empty = EmptyCatalog;
        assert!(empty.lookup("Q-0-1").is_none());
        assert!(empty.lookup("anything").is_none());
    }

    /// A trivial mock provider implements the trait and is found by lookup.
    struct MockCatalog {
        entry: ErrorCodeInfo,
    }
    impl CatalogProvider for MockCatalog {
        fn lookup(&self, code: &str) -> Option<&ErrorCodeInfo> {
            (code == "Q-0-1").then_some(&self.entry)
        }
    }

    /// The **single** test in this crate that mutates the process-global
    /// catalog: install a mock and assert the free functions delegate to it for
    /// a known code, and return `None` for an unknown one. Keeping this the only
    /// global-mutating test means there is no intra-process install conflict,
    /// even under `cargo test` (threads) rather than nextest (process-per-test).
    #[test]
    fn installed_catalog_is_used_by_lookups() {
        install_catalog(Box::new(MockCatalog {
            entry: sample_info("internal", Some("https://example.test/docs/Q-0-1")),
        }));

        assert_eq!(get_subsystem("Q-0-1"), Some("internal"));
        assert_eq!(
            get_docs_url("Q-0-1"),
            Some("https://example.test/docs/Q-0-1")
        );
        assert!(get_error_info("Q-0-1").is_some());

        // Unknown code, even with a catalog installed, is `None`.
        assert!(get_subsystem("Q-9-9").is_none());
        assert!(get_docs_url("Q-9-9").is_none());
        assert!(get_error_info("Q-9-9").is_none());
    }
}
