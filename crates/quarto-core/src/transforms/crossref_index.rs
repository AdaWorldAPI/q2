/*
 * transforms/crossref_index.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Build the per-document crossref index.
 */

//! Build the per-document crossref index.
//!
//! This transform runs in the **crossref phase** (see plan D3), after
//! `FloatRefTargetSugarTransform` has canonicalized every float crossref
//! target into [`CustomNode("FloatRefTarget")`](crate::crossref::FLOAT_REF_TARGET).
//! It walks the AST in document order and:
//!
//! 1. Tracks a section counter stack driven by `Header` blocks. The stack
//!    is the same thing the reader would say is "section 1.2.3".
//! 2. For each crossref target it sees, assigns an [`Order`] with the
//!    current section snapshot and a ref-type-scoped 1-based counter.
//! 3. Stashes the assigned order back into the custom node's
//!    `plain_data.order` so renderers can read it without looking up the
//!    index.
//! 4. Populates [`CrossrefIndex::entries`] with a [`CrossrefEntry`] per
//!    target. Duplicates are recorded as a diagnostic and the first
//!    occurrence wins (this mirrors what a reader will actually see when
//!    resolving `@id` — if two targets share an id, only one can be
//!    linked to).
//! 5. At the end, publishes the index as a trace entry via
//!    [`PipelineObserver::on_auxiliary_data`] under
//!    [`TRACE_KIND_CROSSREF_INDEX`], so the trace viewer and tests can
//!    inspect the structured index independently of the rendered HTML.
//!
//! ## Scope
//!
//! Phase 1 implements single-file, flat numbering (no chapters, no
//! appendix-aware renumbering). Subfloats and appendix behavior are
//! deferred to follow-up tasks (see plan notes).

use quarto_analysis::AnalysisContext;
use quarto_error_reporting::DiagnosticMessage;
use quarto_pandoc_types::block::{Block, Blocks, Header};
use quarto_pandoc_types::custom::{CustomNode, Slot};
use quarto_pandoc_types::pandoc::Pandoc;
use serde_json::json;

use crate::Result;
use crate::crossref::{
    CrossrefEntry, CrossrefIndex, FLOAT_REF_TARGET, Order, TRACE_KIND_CROSSREF_INDEX,
};
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// Transform that builds the per-document crossref index.
pub struct CrossrefIndexTransform;

impl CrossrefIndexTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CrossrefIndexTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for CrossrefIndexTransform {
    fn name(&self) -> &str {
        "crossref-index"
    }

    async fn transform(&self, ast: &mut Pandoc, ctx: &mut RenderContext) -> Result<()> {
        // If no index was seeded by the pre-engine stage, create one here
        // so transforms that bypass the full pipeline still get an index.
        if ctx.crossref_index.is_none() {
            ctx.crossref_index = Some(CrossrefIndex::new(quarto_source_map::FileId(0)));
        }
        let mut index = ctx.crossref_index.take().unwrap();

        let mut walker = Walker {
            index: &mut index,
            diagnostics: Vec::new(),
        };
        walker.visit_blocks(&mut ast.blocks);
        let diagnostics = walker.diagnostics;

        // Push diagnostics to context.
        for diag in diagnostics {
            ctx.add_diagnostic(diag);
        }

        // Publish the index to the trace observer. Errors here are
        // intentionally silent — a serialization failure should not break
        // a render (and our types are guaranteed to serialize).
        if let Ok(payload) = serde_json::to_value(&index) {
            ctx.observer
                .on_auxiliary_data(self.name(), 0, TRACE_KIND_CROSSREF_INDEX, &payload);
        }

        ctx.crossref_index = Some(index);
        Ok(())
    }
}

/// Walker state carried through the AST traversal.
struct Walker<'a> {
    index: &'a mut CrossrefIndex,
    diagnostics: Vec<DiagnosticMessage>,
}

impl<'a> Walker<'a> {
    fn visit_blocks(&mut self, blocks: &mut Blocks) {
        for block in blocks.iter_mut() {
            self.visit_block(block);
        }
    }

    fn visit_block(&mut self, block: &mut Block) {
        match block {
            Block::Header(h) => self.visit_header(h),
            Block::Div(div) => self.visit_blocks(&mut div.content),
            Block::BlockQuote(bq) => self.visit_blocks(&mut bq.content),
            Block::OrderedList(ol) => {
                for item in &mut ol.content {
                    self.visit_blocks(item);
                }
            }
            Block::BulletList(bl) => {
                for item in &mut bl.content {
                    self.visit_blocks(item);
                }
            }
            Block::DefinitionList(dl) => {
                for (_term, defs) in &mut dl.content {
                    for def in defs {
                        self.visit_blocks(def);
                    }
                }
            }
            Block::Figure(fig) => self.visit_blocks(&mut fig.content),
            Block::Custom(node) => self.visit_custom(node),
            _ => {}
        }
    }

    fn visit_header(&mut self, header: &Header) {
        advance_sections(&mut self.index.sections, header.level);
        // Record the heading; needed for cross-file book fixup in future
        // phases, and cheap to collect here.
        self.index.headings.push(crate::crossref::HeadingRecord {
            identifier: if header.attr.0.is_empty() {
                None
            } else {
                Some(header.attr.0.clone())
            },
            level: header.level as u8,
            section: self.index.sections.clone(),
            source_info: header.source_info.clone(),
        });
    }

    fn visit_custom(&mut self, node: &mut CustomNode) {
        // Recurse into child blocks first so nested targets are indexed
        // in document order.
        for (_name, slot) in node.slots.iter_mut() {
            match slot {
                Slot::Block(b) => self.visit_block(b),
                Slot::Blocks(bs) => self.visit_blocks(bs),
                _ => {}
            }
        }
        if node.type_name == FLOAT_REF_TARGET {
            self.index_float_ref_target(node);
        }
    }

    fn index_float_ref_target(&mut self, node: &mut CustomNode) {
        let identifier = node.attr.0.clone();
        if identifier.is_empty() {
            return;
        }

        let ref_type = match node.plain_data.get("ref_type").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return,
        };

        // Duplicate id check: skip numbering the duplicate.
        if self.index.entries.contains_key(&identifier) {
            let existing_src = self.index.entries[&identifier].source_info.clone();
            self.diagnostics.push(duplicate_id_diagnostic(
                &identifier,
                &existing_src,
                &node.source_info,
            ));
            return;
        }

        // Increment the per-ref-type counter.
        let order_num = {
            let counter = self.index.next_order.entry(ref_type.clone()).or_insert(0);
            *counter += 1;
            *counter
        };

        let order = Order {
            section: self.index.sections.clone(),
            order: order_num,
        };

        // Write the order back into the node so renderers don't need to
        // round-trip through the index. `plain_data.order` is an object
        // `{ "section": [...], "order": n }` — same shape as `Order` itself.
        if let Some(obj) = node.plain_data.as_object_mut() {
            obj.insert(
                "order".into(),
                json!({
                    "section": order.section,
                    "order": order.order,
                }),
            );
        }

        // Extract caption inlines for the index entry (for link text).
        // Flatten the caption_long slot's first paragraph, if any.
        let caption = extract_caption_inlines(node);

        let entry = CrossrefEntry {
            identifier: identifier.clone(),
            ref_type,
            parent: None, // subfloats deferred
            order,
            caption,
            in_appendix: false, // deferred
            source_info: node.source_info.clone(),
        };
        self.index.insert(entry);
    }
}

/// Advance the section counter stack when a header of `level` is seen.
fn advance_sections(sections: &mut Vec<u32>, level: usize) {
    if sections.len() > level {
        sections.truncate(level);
    }
    while sections.len() < level {
        sections.push(0);
    }
    if let Some(last) = sections.last_mut() {
        *last += 1;
    }
}

/// Pull caption inlines out of a FloatRefTarget custom node for use as
/// link text when `@id` is resolved.
///
/// Prefers `caption_short` (a short form authored by the user). Falls back
/// to concatenating the inlines of `caption_long`'s first Paragraph.
fn extract_caption_inlines(node: &CustomNode) -> Option<quarto_pandoc_types::inline::Inlines> {
    if let Some(Slot::Inlines(short)) = node.slots.get("caption_short") {
        if !short.is_empty() {
            return Some(short.clone());
        }
    }
    if let Some(Slot::Blocks(long)) = node.slots.get("caption_long") {
        for block in long {
            if let Block::Paragraph(p) = block {
                return Some(p.content.clone());
            }
        }
    }
    None
}

fn duplicate_id_diagnostic(
    id: &str,
    _first: &quarto_source_map::SourceInfo,
    _second: &quarto_source_map::SourceInfo,
) -> DiagnosticMessage {
    // SourceInfo attachment to DiagnosticMessage would let ariadne render
    // both occurrences; plumb that through when the broader source-context
    // threading in transforms is sorted out (same TODO as the metadata
    // diagnostics).
    DiagnosticMessage::error(format!("duplicate crossref identifier `{id}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossref::RefTypeRegistry;
    use crate::transforms::FloatRefTargetSugarTransform;
    use hashlink::LinkedHashMap;
    use quarto_pandoc_types::attr::{Attr, AttrSourceInfo};
    use quarto_pandoc_types::block::{Block, CodeBlock, Div, Header, Paragraph};
    use quarto_pandoc_types::inline::{Inline, Str};
    use quarto_source_map::{FileId, SourceInfo};

    fn si() -> SourceInfo {
        SourceInfo::original(FileId(0), 0, 0)
    }

    fn attr_id(id: &str) -> Attr {
        (id.to_string(), Vec::new(), LinkedHashMap::new())
    }

    fn header(level: usize, id: &str, text: &str) -> Block {
        Block::Header(Header {
            level,
            attr: attr_id(id),
            content: vec![Inline::Str(Str {
                text: text.to_string(),
                source_info: si(),
            })],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    fn fig_div(id: &str, caption: &str) -> Block {
        Block::Div(Div {
            attr: attr_id(id),
            content: vec![
                Block::CodeBlock(CodeBlock {
                    attr: (String::new(), vec!["python".into()], LinkedHashMap::new()),
                    text: "x=1".into(),
                    source_info: si(),
                    attr_source: AttrSourceInfo::empty(),
                }),
                Block::Paragraph(Paragraph {
                    content: vec![Inline::Str(Str {
                        text: caption.into(),
                        source_info: si(),
                    })],
                    source_info: si(),
                }),
            ],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    /// Run sugaring + indexing end-to-end on a block list and return the
    /// resulting (ast, index, diagnostics).
    async fn run(blocks: Vec<Block>) -> (Pandoc, CrossrefIndex, Vec<DiagnosticMessage>) {
        use crate::format::Format;
        use crate::project::{DocumentInfo, ProjectConfig, ProjectContext};
        use crate::render::{BinaryDependencies, RenderContext};
        use std::path::PathBuf;

        let project = ProjectContext {
            dir: PathBuf::from("/project"),
            config: ProjectConfig::default(),
            is_single_file: true,
            files: vec![],
            output_dir: PathBuf::from("/project"),
        };
        let doc = DocumentInfo::from_path("/project/test.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);
        ctx.ref_type_registry = Some(RefTypeRegistry::builtin());
        ctx.crossref_index = Some(CrossrefIndex::new(FileId(0)));

        let mut ast = Pandoc {
            meta: quarto_pandoc_types::ConfigValue::default(),
            blocks,
        };

        FloatRefTargetSugarTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        CrossrefIndexTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        (ast, ctx.crossref_index.unwrap(), ctx.diagnostics)
    }

    #[tokio::test]
    async fn assigns_order_to_single_figure() {
        let (ast, idx, diags) = run(vec![fig_div("fig-one", "Cap 1")]).await;
        assert!(diags.is_empty());
        assert_eq!(idx.entries.len(), 1);
        let entry = idx.get("fig-one").unwrap();
        assert_eq!(entry.ref_type, "fig");
        assert_eq!(entry.order.order, 1);
        assert!(entry.order.section.is_empty());

        // And the order is written into the node's plain_data too.
        let Block::Custom(node) = &ast.blocks[0] else {
            panic!();
        };
        assert_eq!(
            node.plain_data.get("order").unwrap().get("order").unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn counts_per_ref_type() {
        let (_, idx, _) = run(vec![
            fig_div("fig-one", "f1"),
            fig_div("fig-two", "f2"),
            // tbl has its own counter — wrap in a tbl-prefix div.
            Block::Div(Div {
                attr: attr_id("tbl-one"),
                content: vec![Block::Paragraph(Paragraph {
                    content: vec![],
                    source_info: si(),
                })],
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            }),
        ])
        .await;
        assert_eq!(idx.get("fig-one").unwrap().order.order, 1);
        assert_eq!(idx.get("fig-two").unwrap().order.order, 2);
        assert_eq!(idx.get("tbl-one").unwrap().order.order, 1);
    }

    #[tokio::test]
    async fn captures_section_path() {
        let (_, idx, _) = run(vec![
            header(1, "intro", "Intro"),
            fig_div("fig-in-intro", "cap"),
            header(1, "methods", "Methods"),
            header(2, "details", "Details"),
            fig_div("fig-in-details", "cap"),
        ])
        .await;
        assert_eq!(idx.get("fig-in-intro").unwrap().order.section, vec![1]);
        assert_eq!(idx.get("fig-in-details").unwrap().order.section, vec![2, 1]);
    }

    #[tokio::test]
    async fn duplicate_id_emits_diagnostic_and_keeps_first() {
        let (_, idx, diags) = run(vec![
            fig_div("fig-dup", "first caption"),
            fig_div("fig-dup", "second caption"),
        ])
        .await;
        assert_eq!(diags.len(), 1);
        // Only the first entry is in the index.
        assert_eq!(idx.entries.len(), 1);
    }

    #[test]
    fn section_stack_algo_basic() {
        let mut s = Vec::new();
        advance_sections(&mut s, 1);
        assert_eq!(s, vec![1]);
        advance_sections(&mut s, 1);
        assert_eq!(s, vec![2]);
        advance_sections(&mut s, 2);
        assert_eq!(s, vec![2, 1]);
        advance_sections(&mut s, 3);
        assert_eq!(s, vec![2, 1, 1]);
        advance_sections(&mut s, 3);
        assert_eq!(s, vec![2, 1, 2]);
        advance_sections(&mut s, 2);
        assert_eq!(s, vec![2, 2]);
        advance_sections(&mut s, 1);
        assert_eq!(s, vec![3]);
    }

    #[test]
    fn section_stack_skips_levels() {
        // H1 then H3 without an H2: level-2 entry is implicit-zero, which
        // is... a reader convention, not a crossref correctness concern.
        // We mirror the classic behavior: pad with zeros, then increment.
        let mut s = Vec::new();
        advance_sections(&mut s, 1);
        advance_sections(&mut s, 3);
        assert_eq!(s, vec![1, 0, 1]);
    }

    #[tokio::test]
    async fn caption_inlines_recorded_on_entry() {
        let (_, idx, _) = run(vec![fig_div("fig-caption", "Hello caption")]).await;
        let entry = idx.get("fig-caption").unwrap();
        let caption = entry.caption.as_ref().expect("caption inlines recorded");
        assert_eq!(caption.len(), 1);
        match &caption[0] {
            Inline::Str(s) => assert_eq!(s.text, "Hello caption"),
            other => panic!("expected Str, got {:?}", other),
        }
    }
}
