/*
 * transforms/code_block_generate.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Code-block decoration *Generate* transform (Phase 0 skeleton).
//!
//! Format-agnostic half of the code-block decoration pipeline. Walks
//! every [`CodeBlock`](quarto_pandoc_types::block::CodeBlock) in the
//! document, parses its attributes plus the relevant document-level
//! defaults from `ast.meta`, and produces a typed
//! [`CodeBlockDecoration`] payload that [`CodeBlockRenderTransform`]
//! (see [`super::code_block_render`]) consumes.
//!
//! Today the payload is empty — this transform exists to lock in the
//! pipeline placement and the Generate/Render shape before Phases 1–3
//! (filename / copy / fold) add real fields. The walk runs in
//! `O(blocks)` and produces no AST mutation; for Phase 0 it is a
//! deliberate no-op end-to-end.
//!
//! Pipeline placement: **Normalization Phase**, after
//! [`MetadataNormalizeTransform`](super::MetadataNormalizeTransform)
//! so document-level defaults (e.g. `code-copy: true`) are visible
//! when computing per-block decorations.
//!
//! See `claude-notes/plans/2026-05-19-code-block-features.md` for the
//! full epic plan and the Phase 0 acceptance criteria.

use quarto_pandoc_types::block::{Block, Blocks};
use quarto_pandoc_types::pandoc::Pandoc;
use quarto_source_map::SourceInfo;
use quarto_source_map::types::FileId;

use crate::Result;
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// Typed payload carrying everything format renderers need to wrap a
/// `CodeBlock` in the right outer structure (filename header, copy
/// button, fold details, etc.).
///
/// Storage shape: a sideband
/// [`HashMap<CodeBlockDecorationKey, CodeBlockDecoration>`](CodeBlockDecorationKey)
/// on [`RenderContext::code_block_decorations`](crate::render::RenderContext)
/// (decision pinned in
/// `claude-notes/plans/2026-05-19-code-block-features.md`). Generate
/// populates the map; Render reads it. Both transforms run inside
/// `AstTransformsStage` so they share the same `RenderContext` —
/// no `StageContext` bridge needed.
///
/// Per-feature fields land in Phases 1 – 3:
/// - Phase 1 (filename): see `filename` below.
/// - Phase 2 (copy): `pub copy: CopyMode`.
/// - Phase 3 (fold): `pub fold: FoldMode`, `pub summary: Option<String>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeBlockDecoration {
    /// Phase 1: the value of the `filename` attribute (or the
    /// `#| filename:` chunk option). `None` when no filename was
    /// declared; in that case Render emits no filename header.
    pub filename: Option<String>,
    // Phase 2: pub copy: CopyMode
    // Phase 3: pub fold: FoldMode, pub summary: Option<String>
}

/// Stable identity used to key
/// [`RenderContext::code_block_decorations`](crate::render::RenderContext)
/// across the Generate → Render boundary.
///
/// Derived from a `CodeBlock`'s [`SourceInfo`], using the underlying
/// `Original` variant's `(file_id, start_offset, end_offset)` triple.
/// The triple is stable through every subsequent transform that
/// leaves the source-tracking intact, which is every transform in
/// the current pipeline — none of them rewrite a code block's
/// `source_info`.
///
/// **Non-`Original` variants are skipped at decoration time.**
/// `Substring` and `Concat` are inline-text artefacts that effectively
/// never occur on `Block::CodeBlock`. `FilterProvenance` applies to
/// elements created from inside a Lua filter; the user-filter slot
/// brackets the entire `AstTransformsStage`, so any filter-created
/// `CodeBlock` enters the pipeline either before Generate or after
/// Render — never between them, where the key would matter.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct CodeBlockDecorationKey {
    pub file_id: FileId,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl CodeBlockDecorationKey {
    /// Build a key from a `SourceInfo`. Returns `None` when the
    /// source info isn't an `Original` (see struct docs).
    pub fn from_source_info(si: &SourceInfo) -> Option<Self> {
        match si {
            SourceInfo::Original {
                file_id,
                start_offset,
                end_offset,
            } => Some(Self {
                file_id: *file_id,
                start_offset: *start_offset,
                end_offset: *end_offset,
            }),
            _ => None,
        }
    }
}

/// See module docs.
pub struct CodeBlockGenerateTransform;

impl CodeBlockGenerateTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodeBlockGenerateTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for CodeBlockGenerateTransform {
    fn name(&self) -> &str {
        "code-block-generate"
    }

    async fn transform(&self, ast: &mut Pandoc, ctx: &mut RenderContext) -> Result<()> {
        walk_blocks_mut(&mut ast.blocks, &mut |block| {
            let Block::CodeBlock(cb) = block else {
                return;
            };
            // Build a decoration from the per-block attributes.
            // Phase 1 reads only `filename`. Phases 2 / 3 will extend
            // this match to read `code-copy`, `code-fold`,
            // `code-summary`, etc., and to combine with doc-level
            // defaults from `ast.meta` (read via a closed-over
            // reference if we end up needing it — `walk_blocks_mut`
            // already supports that).
            let filename = cb.attr.2.get("filename").map(|s| s.clone());

            let decoration = CodeBlockDecoration { filename };

            // Skip blocks that produced no actual decoration — keeps
            // the sideband map sparse so Render can short-circuit on
            // empty.
            if !decoration_has_any_field(&decoration) {
                return;
            }

            // Source-info → key. Non-`Original` variants (Substring,
            // Concat, FilterProvenance) currently can't happen on
            // block-level `CodeBlock`; if they ever do, we skip
            // decoration for those blocks rather than panicking.
            let Some(key) = CodeBlockDecorationKey::from_source_info(&cb.source_info) else {
                return;
            };

            ctx.code_block_decorations.insert(key, decoration);
        });
        Ok(())
    }
}

/// Returns `true` when at least one decoration-triggering field is
/// populated. Phase 1: only `filename` exists, so this is just an
/// `is_some` check. Phases 2 / 3 will extend the disjunction.
pub(crate) fn decoration_has_any_field(d: &CodeBlockDecoration) -> bool {
    d.filename.is_some()
}

/// Walk every `CodeBlock` in the document, descending into containers
/// (BlockQuote, Div, list items, table cells, figure body, …) so
/// decorations attach to nested code blocks too.
///
/// Visits every `Block::CodeBlock` reachable from `blocks` exactly
/// once. Containers we walk into are kept in sync with the structural
/// shape of `Block` — additions to that enum that can contain code
/// blocks must extend the match arms here.
fn walk_blocks_mut(blocks: &mut Blocks, f: &mut impl FnMut(&mut Block)) {
    for block in blocks.iter_mut() {
        match block {
            Block::CodeBlock(_) => f(block),
            Block::BlockQuote(bq) => walk_blocks_mut(&mut bq.content, f),
            Block::Div(div) => walk_blocks_mut(&mut div.content, f),
            Block::Figure(fig) => walk_blocks_mut(&mut fig.content, f),
            Block::OrderedList(list) => {
                for item in list.content.iter_mut() {
                    walk_blocks_mut(item, f);
                }
            }
            Block::BulletList(list) => {
                for item in list.content.iter_mut() {
                    walk_blocks_mut(item, f);
                }
            }
            Block::DefinitionList(dl) => {
                for (_term, defs) in dl.content.iter_mut() {
                    for def in defs.iter_mut() {
                        walk_blocks_mut(def, f);
                    }
                }
            }
            // Other block variants cannot contain a CodeBlock as a
            // direct child. Header / Paragraph / Plain / LineBlock /
            // RawBlock / HorizontalRule / Table / MetaBlock / Note
            // variants / CaptionBlock / Custom are all leaf-ish from
            // the perspective of block-level code-block discovery.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::project::{DocumentInfo, ProjectConfig, ProjectContext};
    use crate::render::BinaryDependencies;
    use quarto_pandoc_types::block::CodeBlock;
    use quarto_pandoc_types::{ConfigValue, attr::AttrSourceInfo};
    use std::sync::Arc;

    fn source_info_at(file: usize, start: usize, end: usize) -> SourceInfo {
        SourceInfo::Original {
            file_id: FileId(file),
            start_offset: start,
            end_offset: end,
        }
    }

    fn make_codeblock(text: &str, classes: Vec<&str>, kvs: Vec<(&str, &str)>) -> Block {
        let mut kv_map = hashlink::LinkedHashMap::new();
        for (k, v) in kvs {
            kv_map.insert(k.to_string(), v.to_string());
        }
        Block::CodeBlock(CodeBlock {
            attr: (
                String::new(),
                classes.into_iter().map(String::from).collect(),
                kv_map,
            ),
            text: text.to_string(),
            source_info: source_info_at(0, 0, text.len()),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    fn make_test_project() -> ProjectContext {
        ProjectContext {
            dir: std::path::PathBuf::from("/project"),
            config: ProjectConfig::default(),
            is_single_file: true,
            files: vec![DocumentInfo::from_path("/project/doc.qmd")],
            output_dir: std::path::PathBuf::from("/project"),
        }
    }

    #[tokio::test]
    async fn generate_populates_filename_decoration() {
        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![make_codeblock(
                "print('hi')",
                vec!["python"],
                vec![("filename", "hello.py")],
            )],
        };

        let project = make_test_project();
        let doc = DocumentInfo::from_path("/project/doc.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);

        CodeBlockGenerateTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        assert_eq!(ctx.code_block_decorations.len(), 1);
        let key = CodeBlockDecorationKey::from_source_info(ast.blocks[0].source_info()).unwrap();
        let deco = ctx.code_block_decorations.get(&key).unwrap();
        assert_eq!(deco.filename.as_deref(), Some("hello.py"));
    }

    #[tokio::test]
    async fn generate_skips_blocks_without_decoration_triggers() {
        // Code block without filename / copy / fold attributes carries
        // no decoration. The sideband stays empty so Render can skip
        // its walk cheaply.
        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![make_codeblock("print('hi')", vec!["python"], vec![])],
        };

        let project = make_test_project();
        let doc = DocumentInfo::from_path("/project/doc.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);

        CodeBlockGenerateTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        assert!(
            ctx.code_block_decorations.is_empty(),
            "no-decoration code block should not enter the sideband; got {:?}",
            ctx.code_block_decorations,
        );
    }

    #[tokio::test]
    async fn generate_visits_nested_codeblocks() {
        use quarto_pandoc_types::block::{BlockQuote, Div};

        // Give the two inner code blocks distinct source ranges so
        // their decoration keys don't collide.
        let mut cb_inside_blockquote =
            make_codeblock("x = 1", vec!["python"], vec![("filename", "inner.py")]);
        if let Block::CodeBlock(cb) = &mut cb_inside_blockquote {
            cb.source_info = source_info_at(0, 10, 50);
        }
        let mut cb_inside_div =
            make_codeblock("y = 2", vec!["python"], vec![("filename", "deeper.py")]);
        if let Block::CodeBlock(cb) = &mut cb_inside_div {
            cb.source_info = source_info_at(0, 150, 180);
        }

        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![Block::BlockQuote(BlockQuote {
                content: vec![
                    cb_inside_blockquote,
                    Block::Div(Div {
                        attr: (String::new(), vec![], hashlink::LinkedHashMap::new()),
                        content: vec![cb_inside_div],
                        source_info: source_info_at(0, 100, 200),
                        attr_source: AttrSourceInfo::empty(),
                    }),
                ],
                source_info: source_info_at(0, 0, 300),
            })],
        };

        let project = make_test_project();
        let doc = DocumentInfo::from_path("/project/doc.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);

        CodeBlockGenerateTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        let filenames: std::collections::HashSet<_> = ctx
            .code_block_decorations
            .values()
            .filter_map(|d| d.filename.clone())
            .collect();
        assert!(filenames.contains("inner.py"), "got {filenames:?}");
        assert!(filenames.contains("deeper.py"), "got {filenames:?}");
    }

    #[test]
    fn decoration_key_from_original_source_info() {
        let si = SourceInfo::Original {
            file_id: FileId(7),
            start_offset: 100,
            end_offset: 200,
        };
        let key = CodeBlockDecorationKey::from_source_info(&si).unwrap();
        assert_eq!(key.file_id, FileId(7));
        assert_eq!(key.start_offset, 100);
        assert_eq!(key.end_offset, 200);
    }

    #[test]
    fn decoration_key_skips_non_original_variants() {
        // Substring / Concat / FilterProvenance return None — see the
        // docs on `CodeBlockDecorationKey` for the timing argument
        // that makes this safe in practice.
        let original = Arc::new(SourceInfo::Original {
            file_id: FileId(1),
            start_offset: 0,
            end_offset: 10,
        });
        let substring = SourceInfo::Substring {
            parent: original.clone(),
            start_offset: 2,
            end_offset: 5,
        };
        assert!(CodeBlockDecorationKey::from_source_info(&substring).is_none());

        let concat = SourceInfo::Concat { pieces: vec![] };
        assert!(CodeBlockDecorationKey::from_source_info(&concat).is_none());

        let filter = SourceInfo::FilterProvenance {
            filter_path: "fixture.lua".into(),
            line: 1,
        };
        assert!(CodeBlockDecorationKey::from_source_info(&filter).is_none());
    }

    #[test]
    fn decoration_key_is_hash_eq_for_same_triple() {
        // The key is the load-bearing identity across the
        // Generate → Render boundary; two source infos with the same
        // `(file_id, start, end)` triple must hash and compare equal.
        let a = CodeBlockDecorationKey::from_source_info(&SourceInfo::Original {
            file_id: FileId(3),
            start_offset: 42,
            end_offset: 99,
        })
        .unwrap();
        let b = CodeBlockDecorationKey::from_source_info(&SourceInfo::Original {
            file_id: FileId(3),
            start_offset: 42,
            end_offset: 99,
        })
        .unwrap();
        assert_eq!(a, b);

        let mut map = std::collections::HashMap::new();
        map.insert(a, CodeBlockDecoration::default());
        assert!(map.contains_key(&b));
    }
}
