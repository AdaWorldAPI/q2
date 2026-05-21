/*
 * project_resources.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * User- and engine-declared project resources (bd-o8pr).
 */

//! User-declared additional files for a project render (`bd-o8pr`).
//!
//! Three declaration channels (see
//! `claude-notes/plans/2026-05-03-project-resources.md`):
//!
//! 1. **Project metadata** — `project.resources:` in `_quarto.yml`,
//!    parsed into [`crate::project::ProjectConfig::resources`].
//! 2. **Document metadata** — `resources:` in document YAML
//!    frontmatter, captured into
//!    [`crate::document_profile::DocumentProfile::resources`] at
//!    profile freeze time. Frozen.
//! 3. **Engine and Lua filter** (Phase 2 / Phase 3) — accumulated
//!    into a `DocumentResourceReport` by a late-pipeline collector
//!    stage.
//!
//! This module owns the type definitions and the glob/path helpers.
//! The wiring into the render pipeline lives in
//! [`crate::project::orchestrator`] and the per-project-type
//! `post_render` hooks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────

/// Where a resource declaration originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResourceOrigin {
    /// `project.resources:` in `_quarto.yml`.
    ProjectMetadata,
    /// `resources:` in a document's YAML frontmatter.
    DocumentMetadata { source: PathBuf },
    /// Returned by an engine via `ExecuteResult.supporting_files`.
    Engine { engine: String, source: PathBuf },
    /// Added by a Lua filter via `quarto.doc.add_resource(path)`.
    LuaFilter { source: PathBuf },
    // Reserved for future built-in walkers (Image src, OJS
    // FileAttachment, includes). See plan §"Internal use".
    // AutoDiscovery { kind: AutoDiscoveryKind, source: PathBuf },
}

/// Where the resource's output path is anchored.
///
/// - `Project`: anchored at `output_dir` root. Author declarations in
///   `_quarto.yml` use this scope.
/// - `Page { source }`: anchored at the document's output dir. Doc
///   YAML, engine, and Lua-filter declarations use this scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub enum ResourceScope {
    Project,
    Page { source: PathBuf },
}

/// One resource entry resolved to an absolute on-disk source path
/// and a project-relative output path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedResource {
    /// Absolute path of the source file on disk.
    pub source: PathBuf,
    /// Output path relative to `output_dir`, forward-slash separated.
    pub output_relative: String,
    /// Where this declaration came from.
    pub origin: ResourceOrigin,
    /// Where the output path is anchored.
    pub scope: ResourceScope,
}

// ─────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ResourceError {
    #[error(
        "resource path '{pattern}' resolves outside the project root '{project_root}'. \
         Project resources must live within the project directory."
    )]
    OutOfProject {
        pattern: String,
        project_root: PathBuf,
    },

    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidGlob {
        pattern: String,
        #[source]
        source: glob::PatternError,
    },

    #[error("error walking glob matches for '{pattern}': {source}")]
    GlobWalk {
        pattern: String,
        #[source]
        source: glob::GlobError,
    },
}

// ─────────────────────────────────────────────────────────────────────
// YAML extraction
// ─────────────────────────────────────────────────────────────────────

/// Read a `resources:` field from a `ConfigValue`, accepting either a
/// list of strings or a single scalar (Q1 parity). Returns the raw
/// patterns; expansion happens later via [`expand_patterns`].
pub fn extract_resource_patterns(
    meta: &quarto_pandoc_types::ConfigValue,
    key_path: &[&str],
) -> Vec<String> {
    let mut cur = meta;
    for key in key_path {
        match cur.get(key) {
            Some(v) => cur = v,
            None => return Vec::new(),
        }
    }
    if let Some(arr) = cur.as_array() {
        arr.iter().filter_map(|v| v.as_plain_text()).collect()
    } else if let Some(s) = cur.as_plain_text() {
        vec![s]
    } else {
        Vec::new()
    }
}

// ─────────────────────────────────────────────────────────────────────
// Glob expansion + path validation
// ─────────────────────────────────────────────────────────────────────

const GLOB_CHARS: &[char] = &['*', '?', '['];

fn looks_like_glob(s: &str) -> bool {
    s.contains(|c| GLOB_CHARS.contains(&c))
}

/// Expand a list of patterns into resolved resources.
///
/// - `project_root`: canonical project root; every resolved source
///   must be inside this directory or [`ResourceError::OutOfProject`]
///   is returned.
/// - `anchor`: directory the patterns are relative to. For project-
///   level patterns this equals `project_root`. For doc-level
///   patterns this is the document's parent directory.
/// - `patterns`: raw patterns from YAML.
/// - `make_origin`: builds the `ResourceOrigin` for each entry given
///   the pattern that produced it. Same origin for every match of
///   a single pattern.
/// - `scope`: reused across every entry produced.
///
/// **Leading-`/` semantics (Quarto YAML convention, TS Quarto parity).**
/// A pattern beginning with `/` is project-root-relative — e.g.
/// `"/docs/foo.json"` means `<project_root>/docs/foo.json`, not the
/// filesystem path `/docs/foo.json`. This applies to YAML
/// `resources:` declarations in both `_quarto.yml` and document
/// headers. It does NOT apply to engine/Lua-filter contributions,
/// which arrive through [`resolve_reported_resources`] and use
/// real filesystem semantics for absolute paths.
pub fn expand_patterns(
    project_root: &Path,
    anchor: &Path,
    patterns: &[String],
    mut make_origin: impl FnMut() -> ResourceOrigin,
    scope: ResourceScope,
) -> Result<Vec<ResolvedResource>, ResourceError> {
    let mut out = Vec::new();
    for pattern in patterns {
        let matched = expand_one(project_root, anchor, pattern)?;
        for source in matched {
            let rel = source
                .strip_prefix(project_root)
                .expect("expand_one verified the path is within project_root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(ResolvedResource {
                source,
                output_relative: rel,
                origin: make_origin(),
                scope: scope.clone(),
            });
        }
    }
    Ok(out)
}

fn expand_one(
    project_root: &Path,
    anchor: &Path,
    pattern: &str,
) -> Result<Vec<PathBuf>, ResourceError> {
    // YAML convention (TS Quarto parity, bd-wlza2): a leading `/`
    // anchors the pattern at the project root, NOT the filesystem
    // root. Strip exactly one `/` and rebase from `project_root` so
    // the `join` below treats the remainder as relative. The
    // original `pattern` string is preserved for use in any error
    // message so the user sees what they wrote.
    //
    // Engine/Lua-filter channels do NOT go through `expand_one`;
    // they enter via `resolve_reported_resources` and keep
    // absolute-path semantics intact (engines really do return
    // filesystem-absolute paths to on-disk supporting files).
    let (base, pat) = match pattern.strip_prefix('/') {
        Some(rest) => (project_root, rest),
        None => (anchor, pattern),
    };
    if looks_like_glob(pat) {
        let combined = base.join(pat);
        let combined_str = combined.to_string_lossy().to_string();
        let entries = glob::glob(&combined_str).map_err(|e| ResourceError::InvalidGlob {
            pattern: pattern.to_string(),
            source: e,
        })?;
        let mut matched = Vec::new();
        for entry in entries {
            let path = entry.map_err(|e| ResourceError::GlobWalk {
                pattern: pattern.to_string(),
                source: e,
            })?;
            // Skip directories — only files become published
            // resources. (A directory match would be ambiguous: do
            // we copy the dir contents recursively, or not at all?
            // Q1 requires explicit `dir/**/*` for recursive copy.)
            if path.is_dir() {
                continue;
            }
            let canonical = canonicalize_within_project(project_root, &path, pattern)?;
            matched.push(canonical);
        }
        Ok(matched)
    } else {
        let absolute = base.join(pat);
        let canonical = canonicalize_within_project(project_root, &absolute, pattern)?;
        Ok(vec![canonical])
    }
}

fn canonicalize_within_project(
    project_root: &Path,
    path: &Path,
    pattern: &str,
) -> Result<PathBuf, ResourceError> {
    // Best-effort canonicalization: if the file doesn't exist yet
    // (literal-path case for a file the user just declared but hasn't
    // created), fall back to lexical normalization. Either way the
    // out-of-project check uses the same prefix comparison.
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| lexical_normalize(path));
    if !canonical.starts_with(project_root) {
        return Err(ResourceError::OutOfProject {
            pattern: pattern.to_string(),
            project_root: project_root.to_path_buf(),
        });
    }
    Ok(canonical)
}

/// Lexical (no-I/O) normalization that resolves `.` and `..`
/// components without consulting the filesystem. Used as a fallback
/// for paths that don't yet exist on disk.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(seg) => out.push(seg),
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// Per-document resource report (engine + Lua-filter channels)
// ─────────────────────────────────────────────────────────────────────

/// One entry contributed to a document's [`DocumentResourceReport`]
/// by an engine or a Lua filter.
///
/// Stays raw (not yet resolved against the project root) so that the
/// resolution + out-of-project check happen in one place
/// ([`resolve_reported_resources`]). The `origin` already records who
/// added the entry, which is preserved in the resolved
/// [`ResolvedResource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedResource {
    /// Path the engine or filter handed us. Absolute or relative —
    /// the resolver anchors relatives at the document's parent
    /// directory.
    pub raw_path: PathBuf,
    /// Where this entry came from. Carries the document source path
    /// so engine/filter contributions are still attributable after
    /// being merged into the project-wide list.
    pub origin: ResourceOrigin,
}

/// Per-document accumulator drained by the orchestrator after each
/// Pass-2 render.
///
/// Engines push to this from [`crate::stage::stages::EngineExecutionStage`]
/// after [`crate::engine::ExecuteResult::supporting_files`] is
/// returned. Lua filters (Phase 3) push from
/// `quarto.doc.add_resource(path)` via the standard sidecar drain.
///
/// The orchestrator resolves entries against the project root and
/// the document's parent directory, then merges with the static-
/// channel list before the copy step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentResourceReport {
    pub entries: Vec<ReportedResource>,
}

impl DocumentResourceReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append every supporting-file path produced by an engine,
    /// tagged with that engine's name.
    pub fn add_engine_files(
        &mut self,
        engine_name: &str,
        doc_source: &Path,
        files: impl IntoIterator<Item = PathBuf>,
    ) {
        for file in files {
            self.entries.push(ReportedResource {
                raw_path: file,
                origin: ResourceOrigin::Engine {
                    engine: engine_name.to_string(),
                    source: doc_source.to_path_buf(),
                },
            });
        }
    }

    /// Append every path supplied by a Lua filter (Phase 3).
    pub fn add_lua_filter_files(
        &mut self,
        doc_source: &Path,
        files: impl IntoIterator<Item = PathBuf>,
    ) {
        for file in files {
            self.entries.push(ReportedResource {
                raw_path: file,
                origin: ResourceOrigin::LuaFilter {
                    source: doc_source.to_path_buf(),
                },
            });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Resolve a [`DocumentResourceReport`] against the project root.
/// Each entry becomes a [`ResolvedResource`] anchored at the
/// document's parent dir (for relative paths) or used as-is (for
/// absolute paths), validated for project-root containment.
///
/// The doc source is pulled from each entry's `origin`, so a single
/// call can resolve a report containing entries from multiple
/// originating documents (although in practice each report is per-doc).
pub fn resolve_reported_resources(
    project_root: &Path,
    report: &DocumentResourceReport,
) -> Result<Vec<ResolvedResource>, ResourceError> {
    let mut out = Vec::with_capacity(report.entries.len());
    for entry in &report.entries {
        let doc_source = match &entry.origin {
            ResourceOrigin::Engine { source, .. }
            | ResourceOrigin::LuaFilter { source }
            | ResourceOrigin::DocumentMetadata { source } => source.clone(),
            ResourceOrigin::ProjectMetadata => project_root.to_path_buf(),
        };
        let doc_dir = doc_source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project_root.to_path_buf());

        let raw_str = entry.raw_path.to_string_lossy();
        let absolute = if entry.raw_path.is_absolute() {
            entry.raw_path.clone()
        } else {
            doc_dir.join(&entry.raw_path)
        };
        let canonical = canonicalize_within_project(project_root, &absolute, &raw_str)?;
        let rel = canonical
            .strip_prefix(project_root)
            .expect("canonicalize_within_project verified containment")
            .to_string_lossy()
            .replace('\\', "/");
        out.push(ResolvedResource {
            source: canonical,
            output_relative: rel,
            origin: entry.origin.clone(),
            scope: ResourceScope::Page { source: doc_source },
        });
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────
// High-level collection (static channel)
// ─────────────────────────────────────────────────────────────────────

/// Collect every static-channel resource declared by the project and
/// its documents (`bd-o8pr`, Phase 1).
///
/// "Static channel" means: declarations frozen by the time the
/// pipeline reaches the collector — i.e. project YAML
/// (`project.resources:`) and document YAML (`resources:`).
/// Engine and Lua-filter channels (Phases 2 and 3) merge their
/// contributions into the same vector via the
/// `DocumentResourceReport` mechanism.
///
/// Errors out if any pattern resolves outside the project root —
/// that's a design choice for v1; see plan §"Out-of-project
/// resources".
pub fn collect_static_resources(
    project: &crate::project::ProjectContext,
    index: &crate::project::index::ProjectIndex,
) -> Result<Vec<ResolvedResource>, ResourceError> {
    let project_root = &project.dir;
    let mut out = Vec::new();

    // Project-level: anchor = project root, scope = Project.
    out.extend(expand_patterns(
        project_root,
        project_root,
        &project.config.resources,
        || ResourceOrigin::ProjectMetadata,
        ResourceScope::Project,
    )?);

    // Document-level: anchor = doc's parent dir, scope = Page.
    for profile in index.profiles() {
        if profile.resources.is_empty() {
            continue;
        }
        let doc_source_abs = if profile.source_path.is_absolute() {
            profile.source_path.clone()
        } else {
            project_root.join(&profile.source_path)
        };
        let doc_dir = doc_source_abs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project_root.clone());

        out.extend(expand_patterns(
            project_root,
            &doc_dir,
            &profile.resources,
            || ResourceOrigin::DocumentMetadata {
                source: doc_source_abs.clone(),
            },
            ResourceScope::Page {
                source: doc_source_abs.clone(),
            },
        )?);
    }

    Ok(out)
}

/// Copy resolved resources into `output_dir`, preserving each entry's
/// project-relative output path. Creates parent directories as
/// needed. Skips entries whose source equals their destination
/// (degenerate case for `output_dir` inside the project).
#[cfg(not(target_arch = "wasm32"))]
pub fn copy_resources_to_output_dir(
    resources: &[ResolvedResource],
    output_dir: &Path,
    runtime: &dyn quarto_system_runtime::SystemRuntime,
) -> crate::Result<()> {
    for entry in resources {
        let dst = output_dir.join(&entry.output_relative);
        // Source missing? Convert to a clear error before file_copy
        // produces a less-friendly one.
        let exists = runtime.path_exists(&entry.source, None).map_err(|e| {
            crate::error::QuartoError::other(format!(
                "Failed to probe resource '{}': {}",
                entry.source.display(),
                e
            ))
        })?;
        if !exists {
            return Err(crate::error::QuartoError::other(format!(
                "Declared resource '{}' does not exist on disk",
                entry.source.display()
            )));
        }
        if same_canonical_path(&entry.source, &dst) {
            continue;
        }
        if let Some(parent) = dst.parent() {
            runtime.dir_create(parent, true).map_err(|e| {
                crate::error::QuartoError::other(format!(
                    "Failed to create resource output directory '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        runtime.file_copy(&entry.source, &dst).map_err(|e| {
            crate::error::QuartoError::other(format!(
                "Failed to copy resource {} → {}: {}",
                entry.source.display(),
                dst.display(),
                e
            ))
        })?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn same_canonical_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Render manifest (Phase 4)
// ─────────────────────────────────────────────────────────────────────

/// One entry in the manifest's `resources` array. Mirrors
/// [`ResolvedResource`] in serializable form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResource {
    /// Project-relative source path, forward-slash separated.
    pub source: String,
    /// Project-relative output path inside `output_dir`,
    /// forward-slash separated.
    pub output: String,
    /// Where this entry came from. Same enum as
    /// [`ResourceOrigin`], preserved for diagnostics.
    pub origin: ResourceOrigin,
}

/// The shape written to `.quarto/render-manifest.json` after every
/// project render. The canonical input to `quarto publish`.
///
/// Schema is intentionally permissive: extra fields are ignored by
/// consumers, so we can add fields without breaking older `quarto
/// publish` versions. `version` is the schema version, bumped only
/// for breaking changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderManifest {
    pub version: u32,
    /// Project-relative paths of every primary rendered output
    /// (e.g. `index.html`, `posts/foo.html`).
    pub rendered_files: Vec<String>,
    /// Project-relative paths of every resource published with the
    /// site, plus the origin metadata.
    pub resources: Vec<ManifestResource>,
}

impl RenderManifest {
    pub const VERSION: u32 = 1;
    pub const FILENAME: &'static str = ".quarto/render-manifest.json";

    pub fn new(
        project_root: &Path,
        rendered_files: Vec<String>,
        resources: &[ResolvedResource],
    ) -> Self {
        let resources = resources
            .iter()
            .map(|r| ManifestResource {
                source: project_relative_str(&r.source, Some(project_root))
                    .unwrap_or_else(|| r.source.to_string_lossy().replace('\\', "/")),
                output: r.output_relative.clone(),
                origin: r.origin.clone(),
            })
            .collect();
        Self {
            version: Self::VERSION,
            rendered_files,
            resources,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Compute a project-relative path string. If the path can be made
/// relative to `project_root` (and a root was supplied), return that.
/// Otherwise return the absolute string for diagnostics. Always
/// forward-slash separated.
fn project_relative_str(path: &Path, project_root: Option<&Path>) -> Option<String> {
    project_root
        .and_then(|root| path.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// Write the manifest to `<project_dir>/.quarto/render-manifest.json`.
/// Native-only (the in-browser renderer doesn't have a project dir).
#[cfg(not(target_arch = "wasm32"))]
pub fn write_render_manifest(
    project_dir: &Path,
    manifest: &RenderManifest,
    runtime: &dyn quarto_system_runtime::SystemRuntime,
) -> crate::Result<()> {
    let path = project_dir.join(RenderManifest::FILENAME);
    if let Some(parent) = path.parent() {
        runtime.dir_create(parent, true).map_err(|e| {
            crate::error::QuartoError::other(format!(
                "Failed to create .quarto directory '{}': {}",
                parent.display(),
                e
            ))
        })?;
    }
    let json = manifest.to_json_pretty().map_err(|e| {
        crate::error::QuartoError::other(format!("Failed to serialize render manifest: {}", e))
    })?;
    runtime.file_write(&path, json.as_bytes()).map_err(|e| {
        crate::error::QuartoError::other(format!(
            "Failed to write render manifest '{}': {}",
            path.display(),
            e
        ))
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(path: &Path) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn looks_like_glob_basic() {
        assert!(looks_like_glob("data/*.csv"));
        assert!(looks_like_glob("img/?.png"));
        assert!(looks_like_glob("[ab].txt"));
        assert!(!looks_like_glob("plain.txt"));
        assert!(!looks_like_glob("data/file.csv"));
    }

    #[test]
    fn expand_literal_path() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        touch(&root.join("a.txt"));

        let resolved = expand_patterns(
            &root,
            &root,
            &["a.txt".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "a.txt");
        assert_eq!(
            resolved[0].source,
            root.join("a.txt").canonicalize().unwrap()
        );
    }

    #[test]
    fn expand_glob() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        touch(&root.join("data/a.csv"));
        touch(&root.join("data/b.csv"));
        touch(&root.join("data/skip.txt"));

        let mut resolved = expand_patterns(
            &root,
            &root,
            &["data/*.csv".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap();
        resolved.sort_by(|a, b| a.output_relative.cmp(&b.output_relative));
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].output_relative, "data/a.csv");
        assert_eq!(resolved[1].output_relative, "data/b.csv");
    }

    #[test]
    fn glob_skips_directories() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("data/sub")).unwrap();
        touch(&root.join("data/file.txt"));

        let resolved = expand_patterns(
            &root,
            &root,
            &["data/*".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "data/file.txt");
    }

    #[test]
    fn out_of_project_literal_is_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();

        let err = expand_patterns(
            &root,
            &root,
            &["../outside.csv".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap_err();
        assert!(matches!(err, ResourceError::OutOfProject { .. }));
    }

    // === Leading-slash patterns are project-root-relative ===
    //
    // Quarto YAML convention (matching TS Quarto): a `resources:`
    // entry beginning with `/` is anchored at the project root,
    // *not* the filesystem root. See bd-wlza2.

    #[test]
    fn expand_leading_slash_literal_is_project_relative() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        touch(&root.join("data/a.txt"));

        let resolved = expand_patterns(
            &root,
            &root,
            &["/data/a.txt".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "data/a.txt");
        assert_eq!(
            resolved[0].source,
            root.join("data/a.txt").canonicalize().unwrap()
        );
    }

    #[test]
    fn expand_leading_slash_glob_is_project_relative() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        touch(&root.join("data/a.csv"));
        touch(&root.join("data/b.csv"));

        let mut resolved = expand_patterns(
            &root,
            &root,
            &["/data/*.csv".to_string()],
            || ResourceOrigin::ProjectMetadata,
            ResourceScope::Project,
        )
        .unwrap();
        resolved.sort_by(|a, b| a.output_relative.cmp(&b.output_relative));
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].output_relative, "data/a.csv");
        assert_eq!(resolved[1].output_relative, "data/b.csv");
    }

    #[test]
    fn expand_leading_slash_doc_pattern_anchors_to_project_root_not_doc_dir() {
        // A doc under <root>/posts/ declares "/shared.js" — that's the
        // project-root-relative `<root>/shared.js`, not `<root>/posts/shared.js`.
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc_dir = root.join("posts");
        std::fs::create_dir_all(&doc_dir).unwrap();
        touch(&root.join("shared.js"));

        let doc_source = doc_dir.join("foo.qmd");
        let resolved = expand_patterns(
            &root,
            &doc_dir,
            &["/shared.js".to_string()],
            || ResourceOrigin::DocumentMetadata {
                source: doc_source.clone(),
            },
            ResourceScope::Page {
                source: doc_source.clone(),
            },
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "shared.js");
        assert_eq!(
            resolved[0].source,
            root.join("shared.js").canonicalize().unwrap()
        );
    }

    #[test]
    fn engine_report_absolute_path_keeps_filesystem_absolute_semantics() {
        // Regression guard: the leading-`/` normalization is YAML-only.
        // Engine and Lua-filter channels still pass real on-disk
        // absolute paths and must NOT be stripped/reinterpreted.
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc = root.join("posts/foo.qmd");
        std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
        let supporting = root.join("posts/foo_files/data.png");
        touch(&supporting);

        // `supporting` is filesystem-absolute (e.g. /tmp/.../posts/foo_files/data.png)
        // — the engine channel uses it as-is.
        let mut report = DocumentResourceReport::new();
        report.add_engine_files("stub", &doc, [supporting.clone()]);

        let resolved = resolve_reported_resources(&root, &report).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].source, supporting);
        assert_eq!(resolved[0].output_relative, "posts/foo_files/data.png");
    }

    #[test]
    fn doc_anchor_resolves_relative_to_doc_dir() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc_dir = root.join("posts");
        std::fs::create_dir_all(&doc_dir).unwrap();
        touch(&doc_dir.join("data/extra.html"));

        let doc_source = doc_dir.join("foo.qmd");
        let resolved = expand_patterns(
            &root,
            &doc_dir,
            &["data/extra.html".to_string()],
            || ResourceOrigin::DocumentMetadata {
                source: doc_source.clone(),
            },
            ResourceScope::Page {
                source: doc_source.clone(),
            },
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "posts/data/extra.html");
    }

    #[test]
    fn extract_patterns_scalar() {
        use quarto_pandoc_types::ConfigValue;
        let scalar = ConfigValue::from_path(&["resources"], "x.txt");
        assert_eq!(
            extract_resource_patterns(&scalar, &["resources"]),
            vec!["x.txt".to_string()]
        );
    }

    #[test]
    fn extract_patterns_nested_path() {
        use quarto_pandoc_types::ConfigValue;
        let cv = ConfigValue::from_path(&["project", "resources"], "x.txt");
        assert_eq!(
            extract_resource_patterns(&cv, &["project", "resources"]),
            vec!["x.txt".to_string()]
        );
        // missing
        assert!(extract_resource_patterns(&cv, &["project", "missing"]).is_empty());
    }

    // === DocumentResourceReport / resolve_reported_resources ===

    #[test]
    fn resolve_engine_report_absolute_paths() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc = root.join("posts/foo.qmd");
        std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
        let supporting = root.join("posts/foo_files/figure-html/cell-1.png");
        touch(&supporting);

        let mut report = DocumentResourceReport::new();
        report.add_engine_files("knitr", &doc, [supporting.clone()]);

        let resolved = resolve_reported_resources(&root, &report).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].source, supporting);
        assert_eq!(
            resolved[0].output_relative,
            "posts/foo_files/figure-html/cell-1.png"
        );
        assert!(matches!(
            resolved[0].origin,
            ResourceOrigin::Engine { ref engine, .. } if engine == "knitr"
        ));
    }

    #[test]
    fn resolve_engine_report_relative_paths_anchored_at_doc_dir() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc = root.join("posts/foo.qmd");
        std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
        let supporting = root.join("posts/extras/data.csv");
        touch(&supporting);

        let mut report = DocumentResourceReport::new();
        report.add_engine_files(
            "stub",
            &doc,
            [PathBuf::from("extras/data.csv")], // relative
        );

        let resolved = resolve_reported_resources(&root, &report).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].output_relative, "posts/extras/data.csv");
    }

    #[test]
    fn resolve_lua_filter_report_carries_origin() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc = root.join("a.qmd");
        let supporting = root.join("from-filter.txt");
        touch(&supporting);

        let mut report = DocumentResourceReport::new();
        report.add_lua_filter_files(&doc, [supporting.clone()]);

        let resolved = resolve_reported_resources(&root, &report).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(matches!(
            resolved[0].origin,
            ResourceOrigin::LuaFilter { ref source } if source == &doc
        ));
    }

    #[test]
    fn resolve_engine_report_out_of_project_errors() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let doc = root.join("a.qmd");

        let mut report = DocumentResourceReport::new();
        report.add_engine_files("stub", &doc, [PathBuf::from("../escape.csv")]);

        let err = resolve_reported_resources(&root, &report).unwrap_err();
        assert!(matches!(err, ResourceError::OutOfProject { .. }));
    }
}
