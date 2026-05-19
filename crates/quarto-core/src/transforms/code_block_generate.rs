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

    async fn transform(&self, ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        // Phase 0: walk the AST so the traversal cost is paid here
        // rather than added later. The walker is shaped to mutate
        // blocks (Phases 1+ will need to read attrs and may rewrite
        // them or attach sideband state), so we use `iter_mut`.
        walk_blocks_mut(&mut ast.blocks, &mut |_block| {
            // No-op for Phase 0. Future phases will:
            //   1. Read CodeBlock.attr.2 (kv map) for `filename`,
            //      `code-fold`, `code-summary`, `code-copy`, etc.
            //   2. Combine with doc-level defaults from `ast.meta`.
            //   3. Construct a CodeBlockDecoration and store it
            //      somewhere (CustomNode wrapper vs sideband map —
            //      decision deferred to Phase 1 where there's an
            //      actual consumer).
        });
        Ok(())
    }
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
    use std::sync::Arc;

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
