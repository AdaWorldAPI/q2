/*
 * transforms/crossref_render.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Finalization-phase rendering of crossref custom nodes.
 */

//! Finalization-phase transform for crossref custom nodes.
//!
//! Converts the two front-end crossref custom node types into shapes the
//! writer knows how to emit:
//!
//! - [`CustomNode("FloatRefTarget")`](crate::crossref::FLOAT_REF_TARGET)
//!   → Pandoc's native `Figure` for figure-kind targets (so the HTML
//!   writer emits `<figure><figcaption>...</figcaption></figure>`), or a
//!   `Div` wrapping the content with the caption as a trailing paragraph
//!   for table- and listing-kind targets (where Pandoc's native `Figure`
//!   isn't the right enclosing element).
//! - [`CustomNode("CrossrefResolvedRef")`](crate::crossref::CROSSREF_RESOLVED_REF)
//!   → `Link` inline pointing at `#<identifier>` with text like
//!   `"Figure 1"` (rendered from `kind` + `order.order`).
//!
//! ## Caption numbering
//!
//! A caption like "An overview of the pipeline" becomes "Figure 1: An
//! overview of the pipeline" — the `kind` + `order` prefix is prepended.
//! Unnumbered targets (no `order` in plain_data) simply keep the caption
//! as-is. The separator, sequence format, and localization live in a
//! later task (Q1 supports `crossref.fig-prefix`, `title-delim`, etc.);
//! Phase 1 hard-codes the English defaults: `"<Kind> <N>: "`.
//!
//! ## Format scope
//!
//! For Phase 1 we only target HTML via Pandoc's native Figure shape,
//! which is the right structure for all HTML-family formats. LaTeX /
//! Typst back-ends will need their own rendering transforms that emit
//! `\ref` / `@label` into raw blocks; those land later and are wired in
//! a format-specific pipeline.

use quarto_pandoc_types::attr::{Attr, AttrSourceInfo, TargetSourceInfo};
use quarto_pandoc_types::block::{Block, Blocks, Div, Figure};
use quarto_pandoc_types::caption::Caption;
use quarto_pandoc_types::custom::{CustomNode, Slot};
use quarto_pandoc_types::inline::{Inline, Inlines, Link, Str};
use quarto_pandoc_types::pandoc::Pandoc;
use quarto_source_map::SourceInfo;

use crate::Result;
use crate::crossref::{CROSSREF_RESOLVED_REF, FLOAT_REF_TARGET, PROOF, THEOREM};
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// Transform that converts FloatRefTarget / CrossrefResolvedRef custom
/// nodes into writer-visible shapes.
pub struct CrossrefRenderTransform;

impl CrossrefRenderTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CrossrefRenderTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for CrossrefRenderTransform {
    fn name(&self) -> &str {
        "crossref-render"
    }

    async fn transform(&self, ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        render_blocks(&mut ast.blocks);
        Ok(())
    }
}

fn render_blocks(blocks: &mut Blocks) {
    for block in blocks.iter_mut() {
        render_block(block);
    }
}

fn render_block(block: &mut Block) {
    // Recurse into children.
    match block {
        Block::BlockQuote(bq) => render_blocks(&mut bq.content),
        Block::OrderedList(ol) => {
            for item in &mut ol.content {
                render_blocks(item);
            }
        }
        Block::BulletList(bl) => {
            for item in &mut bl.content {
                render_blocks(item);
            }
        }
        Block::DefinitionList(dl) => {
            for (term, defs) in &mut dl.content {
                render_inlines(term);
                for def in defs {
                    render_blocks(def);
                }
            }
        }
        Block::Figure(fig) => {
            render_blocks(&mut fig.content);
            if let Some(long) = fig.caption.long.as_mut() {
                render_blocks(long);
            }
            if let Some(short) = fig.caption.short.as_mut() {
                render_inlines(short);
            }
        }
        Block::Div(div) => render_blocks(&mut div.content),
        Block::Paragraph(p) => render_inlines(&mut p.content),
        Block::Plain(p) => render_inlines(&mut p.content),
        Block::LineBlock(lb) => {
            for line in &mut lb.content {
                render_inlines(line);
            }
        }
        Block::Header(h) => render_inlines(&mut h.content),
        Block::Custom(node) => {
            // Recurse into slots first so nested resolved refs are rendered.
            for (_k, slot) in node.slots.iter_mut() {
                match slot {
                    Slot::Block(b) => render_block(b),
                    Slot::Blocks(bs) => render_blocks(bs),
                    Slot::Inline(i) => render_inline(i),
                    Slot::Inlines(is) => render_inlines(is),
                }
            }
        }
        _ => {}
    }

    // Convert this node if it's a recognized crossref block custom type.
    if let Block::Custom(node) = block {
        if node.type_name == FLOAT_REF_TARGET {
            let replacement = render_float_ref_target(take_custom_node(node));
            *block = replacement;
        } else if node.type_name == THEOREM {
            let replacement = render_theorem(take_custom_node(node));
            *block = replacement;
        } else if node.type_name == PROOF {
            let replacement = render_proof(take_custom_node(node));
            *block = replacement;
        }
    }
}

/// Swap out a `CustomNode` in place with a placeholder, returning the
/// original so rendering can take ownership without cloning the whole
/// subtree. The placeholder is immediately replaced by the caller; it
/// never reaches downstream code.
fn take_custom_node(node: &mut CustomNode) -> CustomNode {
    std::mem::replace(
        node,
        CustomNode::new(
            "_placeholder",
            (String::new(), Vec::new(), hashlink::LinkedHashMap::new()),
            node.source_info.clone(),
        ),
    )
}

fn render_inlines(inlines: &mut Inlines) {
    for inline in inlines.iter_mut() {
        render_inline(inline);
    }
}

fn render_inline(inline: &mut Inline) {
    match inline {
        Inline::Emph(e) => render_inlines(&mut e.content),
        Inline::Underline(u) => render_inlines(&mut u.content),
        Inline::Strong(s) => render_inlines(&mut s.content),
        Inline::Strikeout(s) => render_inlines(&mut s.content),
        Inline::Superscript(s) => render_inlines(&mut s.content),
        Inline::Subscript(s) => render_inlines(&mut s.content),
        Inline::SmallCaps(s) => render_inlines(&mut s.content),
        Inline::Quoted(q) => render_inlines(&mut q.content),
        Inline::Link(l) => render_inlines(&mut l.content),
        Inline::Image(i) => render_inlines(&mut i.content),
        Inline::Note(n) => render_blocks(&mut n.content),
        Inline::Span(s) => render_inlines(&mut s.content),
        Inline::Insert(i) => render_inlines(&mut i.content),
        Inline::Delete(d) => render_inlines(&mut d.content),
        Inline::Highlight(h) => render_inlines(&mut h.content),
        Inline::Custom(node) => {
            for (_k, slot) in node.slots.iter_mut() {
                match slot {
                    Slot::Block(b) => render_block(b),
                    Slot::Blocks(bs) => render_blocks(bs),
                    Slot::Inline(i) => render_inline(i),
                    Slot::Inlines(is) => render_inlines(is),
                }
            }
        }
        _ => {}
    }

    if let Inline::Custom(node) = inline {
        if node.type_name == CROSSREF_RESOLVED_REF {
            *inline = render_resolved_ref(take_custom_node(node));
        }
    }
}

/// Convert a FloatRefTarget custom node into the writer-visible shape.
///
/// Figures map to Pandoc's native `Figure` node. Tables and listings map
/// to a `Div` wrapping the original content with the (numbered) caption
/// as a trailing Paragraph — a pragmatic choice that avoids needing a
/// CSS class taxonomy right away. A later pass can replace this with
/// richer per-category structure if needed.
fn render_float_ref_target(node: CustomNode) -> Block {
    let identifier = node.attr.0.clone();
    let ref_type = node
        .plain_data
        .get("ref_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = node
        .plain_data
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let number = node
        .plain_data
        .get("order")
        .and_then(|v| v.get("order"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let source_info = node.source_info.clone();

    // Extract slots
    let mut slots = node.slots;
    let content: Blocks = match slots.remove("content") {
        Some(Slot::Blocks(bs)) => bs,
        _ => Vec::new(),
    };
    let caption_long: Blocks = match slots.remove("caption_long") {
        Some(Slot::Blocks(bs)) => bs,
        _ => Vec::new(),
    };
    let caption_short: Option<Inlines> = match slots.remove("caption_short") {
        Some(Slot::Inlines(is)) => Some(is),
        _ => None,
    };

    let numbered_caption = prefix_caption(caption_long.clone(), &kind, number);

    if ref_type == "fig" {
        // Prefer Pandoc's native Figure so the HTML writer emits
        // `<figure><figcaption>...</figcaption></figure>` with the id.
        Block::Figure(Figure {
            attr: node.attr,
            caption: Caption {
                short: caption_short,
                long: Some(numbered_caption),
                source_info: source_info.clone(),
            },
            content,
            source_info,
            attr_source: AttrSourceInfo::empty(),
        })
    } else {
        // Div wrapper: content + numbered-caption paragraph.
        let mut body = content;
        if !numbered_caption.is_empty() {
            body.extend(numbered_caption);
        }
        let _ = identifier; // id is on node.attr already
        Block::Div(Div {
            attr: node.attr,
            content: body,
            source_info,
            attr_source: AttrSourceInfo::empty(),
        })
    }
}

/// Convert a Theorem custom node into a `Div` structure the HTML writer
/// can serialize.
///
/// Output shape:
///
/// ```html
/// <div id="thm-x" class="theorem">
///   <p><strong>Theorem 1 (Optional Title).</strong> First body text...</p>
///   <p>Second body paragraph...</p>
/// </div>
/// ```
///
/// The numbered title is prepended inline to the first paragraph of the
/// body. If the body starts with a non-Paragraph block (e.g. a display
/// math) the label lives in its own Paragraph so the prefix isn't lost.
/// Unnumbered theorems (no `order` in plain_data) get the title without
/// a number: `"Theorem (Title)."`.
fn render_theorem(node: CustomNode) -> Block {
    let kind = node
        .plain_data
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ref_type = node
        .plain_data
        .get("ref_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let number = node
        .plain_data
        .get("order")
        .and_then(|v| v.get("order"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let source_info = node.source_info.clone();
    let mut attr = node.attr;
    // Surface the theorem kind as a class so CSS / later transforms can
    // style it. Idempotent — don't add if already present.
    if !ref_type.is_empty() && !attr.1.iter().any(|c| *c == ref_type) {
        attr.1.push(ref_type.clone());
    }
    let theorem_class = theorem_class_name(&ref_type);
    if !theorem_class.is_empty() && !attr.1.iter().any(|c| c == theorem_class) {
        attr.1.push(theorem_class.to_string());
    }

    let mut slots = node.slots;
    let content: Blocks = match slots.remove("content") {
        Some(Slot::Blocks(bs)) => bs,
        _ => Vec::new(),
    };
    let title: Option<Inlines> = match slots.remove("title") {
        Some(Slot::Inlines(is)) if !is.is_empty() => Some(is),
        _ => None,
    };

    let label = theorem_label_inlines(&kind, number, title.as_deref(), source_info.clone());

    // Prepend the label to the first Paragraph, or insert a new Paragraph
    // if the body doesn't start with one.
    let body = prepend_theorem_label(content, label, source_info.clone());

    Block::Div(Div {
        attr,
        content: body,
        source_info,
        attr_source: AttrSourceInfo::empty(),
    })
}

/// Map a ref_type prefix to the class name that should appear on the
/// rendered Div. `"thm"` → `"theorem"`, `"lem"` → `"lemma"`, etc.
/// Empty string for unknown prefixes.
fn theorem_class_name(ref_type: &str) -> &'static str {
    match ref_type {
        "thm" => "theorem",
        "lem" => "lemma",
        "cor" => "corollary",
        "prp" => "proposition",
        "cnj" => "conjecture",
        "def" => "definition",
        "exm" => "example",
        "exr" => "exercise",
        _ => "",
    }
}

/// Build the label inlines: `"<Kind> <N> (<Title>). "` — in bold.
///
/// Components are omitted individually: no number if `number` is None,
/// no parenthesized title if `title` is None. Always trailing-spaced so
/// it can be prepended to the first content paragraph inline.
fn theorem_label_inlines(
    kind: &str,
    number: Option<u32>,
    title: Option<&[Inline]>,
    source_info: SourceInfo,
) -> Inlines {
    let mut label_text = String::new();
    if !kind.is_empty() {
        label_text.push_str(kind);
    }
    if let Some(n) = number {
        if !label_text.is_empty() {
            label_text.push(' ');
        }
        label_text.push_str(&n.to_string());
    }

    // Build a Strong inline with label_text plus (optional " (Title)")
    let mut strong_content: Inlines = vec![Inline::Str(Str {
        text: label_text,
        source_info: source_info.clone(),
    })];
    if let Some(title_inlines) = title {
        // Space between label and "(" so it renders as "Theorem 1 (Title)".
        strong_content.push(Inline::Str(Str {
            text: " (".to_string(),
            source_info: source_info.clone(),
        }));
        strong_content.extend(title_inlines.iter().cloned());
        strong_content.push(Inline::Str(Str {
            text: ")".to_string(),
            source_info: source_info.clone(),
        }));
    }
    // Period + trailing space to separate label from body text.
    strong_content.push(Inline::Str(Str {
        text: ".".to_string(),
        source_info: source_info.clone(),
    }));

    vec![
        Inline::Strong(quarto_pandoc_types::inline::Strong {
            content: strong_content,
            source_info: source_info.clone(),
        }),
        Inline::Str(Str {
            text: " ".to_string(),
            source_info,
        }),
    ]
}

/// Prepend `label` inlines to the first Paragraph of `content`. If
/// `content` is empty or its first block isn't a Paragraph, insert a
/// new label-only Paragraph at the front.
fn prepend_theorem_label(mut content: Blocks, label: Inlines, source_info: SourceInfo) -> Blocks {
    if let Some(Block::Paragraph(first)) = content.first_mut() {
        let mut new_content = label;
        new_content.extend(std::mem::take(&mut first.content));
        first.content = new_content;
        content
    } else {
        let label_para = Block::Paragraph(quarto_pandoc_types::block::Paragraph {
            content: label,
            source_info,
        });
        let mut out = Vec::with_capacity(content.len() + 1);
        out.push(label_para);
        out.extend(content);
        out
    }
}

/// Convert a Proof custom node into a `Div` with an italicized
/// "Proof." prefix (or the user's title).
///
/// Shape:
///
/// ```html
/// <div class="proof">
///   <p><em>Proof.</em> First body...</p>
///   <p>Second body...</p>
/// </div>
/// ```
///
/// Proofs never carry a number. The id (if any) flows through on the
/// Div's `id` attribute so anchor links still work.
fn render_proof(node: CustomNode) -> Block {
    let source_info = node.source_info.clone();
    let mut attr = node.attr;
    if !attr.1.iter().any(|c| c == "proof") {
        attr.1.push("proof".to_string());
    }

    let mut slots = node.slots;
    let content: Blocks = match slots.remove("content") {
        Some(Slot::Blocks(bs)) => bs,
        _ => Vec::new(),
    };
    let title: Option<Inlines> = match slots.remove("title") {
        Some(Slot::Inlines(is)) if !is.is_empty() => Some(is),
        _ => None,
    };

    // Build the italic label: "*Proof.* " or "*Custom title.* ".
    let mut em_content: Inlines = match title {
        Some(t) => {
            let mut inlines = t;
            inlines.push(Inline::Str(Str {
                text: ".".to_string(),
                source_info: source_info.clone(),
            }));
            inlines
        }
        None => vec![Inline::Str(Str {
            text: "Proof.".to_string(),
            source_info: source_info.clone(),
        })],
    };
    // Make the label italic via Emph.
    let label: Inlines = vec![
        Inline::Emph(quarto_pandoc_types::inline::Emph {
            content: std::mem::take(&mut em_content),
            source_info: source_info.clone(),
        }),
        Inline::Str(Str {
            text: " ".to_string(),
            source_info: source_info.clone(),
        }),
    ];

    let body = prepend_theorem_label(content, label, source_info.clone());

    Block::Div(Div {
        attr,
        content: body,
        source_info,
        attr_source: AttrSourceInfo::empty(),
    })
}

/// Convert a CrossrefResolvedRef custom node into a `Link` inline.
///
/// Link text is `"<Kind> <N>"` when the ref is resolved, or the literal
/// `"?id?"` (wrapped visibly) for unresolved refs so the failure is
/// obvious in the rendered document.
fn render_resolved_ref(node: CustomNode) -> Inline {
    let identifier = node
        .plain_data
        .get("identifier")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = node
        .plain_data
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let resolved = node
        .plain_data
        .get("resolved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let number = node
        .plain_data
        .get("order")
        .and_then(|v| v.get("order"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let source_info = node.source_info.clone();

    let text = if resolved {
        match number {
            Some(n) => format!("{kind} {n}"),
            None => kind.clone(),
        }
    } else {
        format!("?{identifier}?")
    };

    let content: Inlines = vec![Inline::Str(Str {
        text,
        source_info: source_info.clone(),
    })];
    let target = (format!("#{identifier}"), String::new());

    Inline::Link(Link {
        attr: (
            String::new(),
            vec!["quarto-xref".to_string()],
            hashlink::LinkedHashMap::new(),
        ),
        content,
        target,
        source_info,
        attr_source: AttrSourceInfo::empty(),
        target_source: TargetSourceInfo::empty(),
    })
}

/// Prepend a numbered prefix onto the first Paragraph of a caption block
/// list, returning a fresh Blocks. No-op if the kind is empty or the
/// caption is empty.
fn prefix_caption(caption: Blocks, kind: &str, number: Option<u32>) -> Blocks {
    if kind.is_empty() || caption.is_empty() {
        return caption;
    }
    let prefix_text = match number {
        Some(n) => format!("{kind} {n}: "),
        None => format!("{kind}: "),
    };
    let mut out = caption;
    if let Some(Block::Paragraph(first)) = out.first_mut() {
        // Prepend Str + Space-like (we use a single Str containing the
        // trailing space so we don't have to synthesize Space inlines).
        let src = first.source_info.clone();
        let mut new_content: Inlines = vec![Inline::Str(Str {
            text: prefix_text,
            source_info: src,
        })];
        new_content.extend(std::mem::take(&mut first.content));
        first.content = new_content;
    }
    out
}

/// Placeholder to silence potentially-unused helper warnings in narrow
/// test builds.
#[allow(dead_code)]
fn _dummy(a: Attr) -> Attr {
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossref::RefTypeRegistry;
    use crate::transforms::{
        CrossrefIndexTransform, CrossrefResolveTransform, FloatRefTargetSugarTransform,
        ProofSugarTransform, TheoremSugarTransform,
    };
    use hashlink::LinkedHashMap;
    use quarto_pandoc_types::block::{Block, CodeBlock, Div, Paragraph};
    use quarto_pandoc_types::inline::{Citation, CitationMode, Cite};
    use quarto_source_map::{FileId, SourceInfo};

    fn si() -> SourceInfo {
        SourceInfo::original(FileId(0), 0, 0)
    }

    fn attr_id(id: &str) -> Attr {
        (id.to_string(), Vec::new(), LinkedHashMap::new())
    }

    fn str_inline(s: &str) -> Inline {
        Inline::Str(Str {
            text: s.to_string(),
            source_info: si(),
        })
    }

    fn para(text: &str) -> Block {
        Block::Paragraph(Paragraph {
            content: vec![str_inline(text)],
            source_info: si(),
        })
    }

    fn fig_div(id: &str, cap: &str) -> Block {
        Block::Div(Div {
            attr: attr_id(id),
            content: vec![
                Block::CodeBlock(CodeBlock {
                    attr: (String::new(), vec!["python".into()], LinkedHashMap::new()),
                    text: "x=1".into(),
                    source_info: si(),
                    attr_source: AttrSourceInfo::empty(),
                }),
                para(cap),
            ],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    async fn run_full(blocks: Vec<Block>) -> Pandoc {
        use crate::format::Format;
        use crate::project::{DocumentInfo, ProjectConfig, ProjectContext};
        use crate::render::{BinaryDependencies, RenderContext};
        use std::path::PathBuf;
        let project = ProjectContext {
            dir: PathBuf::from("/p"),
            config: ProjectConfig::default(),
            is_single_file: true,
            files: vec![],
            output_dir: PathBuf::from("/p"),
        };
        let doc = DocumentInfo::from_path("/p/t.qmd");
        let format = Format::html();
        let binaries = BinaryDependencies::new();
        let mut ctx = RenderContext::new(&project, &doc, &format, &binaries);
        ctx.ref_type_registry = Some(RefTypeRegistry::builtin());

        let mut ast = Pandoc {
            meta: quarto_pandoc_types::ConfigValue::default(),
            blocks,
        };
        TheoremSugarTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        ProofSugarTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        FloatRefTargetSugarTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        CrossrefIndexTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        CrossrefResolveTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        CrossrefRenderTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();
        ast
    }

    fn cite(id: &str) -> Inline {
        Inline::Cite(Cite {
            citations: vec![Citation {
                id: id.to_string(),
                prefix: vec![],
                suffix: vec![],
                mode: CitationMode::NormalCitation,
                note_num: 0,
                hash: 0,
                id_source: None,
            }],
            content: vec![str_inline(&format!("@{}", id))],
            source_info: si(),
        })
    }

    #[tokio::test]
    async fn figure_target_renders_to_pandoc_figure() {
        let ast = run_full(vec![fig_div("fig-1", "Caption A")]).await;
        let block = &ast.blocks[0];
        let Block::Figure(f) = block else {
            panic!("expected Figure, got {:?}", block);
        };
        assert_eq!(f.attr.0, "fig-1");
        let long = f.caption.long.as_ref().unwrap();
        let Block::Paragraph(p) = &long[0] else {
            panic!();
        };
        // First inline should be the "Figure 1: " prefix.
        let Inline::Str(s) = &p.content[0] else {
            panic!();
        };
        assert_eq!(s.text, "Figure 1: ");
        // Followed by the original caption inline.
        let Inline::Str(s) = &p.content[1] else {
            panic!();
        };
        assert_eq!(s.text, "Caption A");
    }

    #[tokio::test]
    async fn table_target_renders_to_div_with_prefixed_caption() {
        use quarto_pandoc_types::table::{Table, TableBody, TableFoot, TableHead};
        let table = Block::Table(Table {
            attr: (String::new(), Vec::new(), LinkedHashMap::new()),
            caption: Caption {
                short: None,
                long: None,
                source_info: si(),
            },
            colspec: vec![],
            head: TableHead {
                attr: (String::new(), Vec::new(), LinkedHashMap::new()),
                rows: vec![],
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            },
            bodies: vec![TableBody {
                attr: (String::new(), Vec::new(), LinkedHashMap::new()),
                rowhead_columns: 0,
                head: vec![],
                body: vec![],
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            }],
            foot: TableFoot {
                attr: (String::new(), Vec::new(), LinkedHashMap::new()),
                rows: vec![],
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            },
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let blocks = vec![Block::Div(Div {
            attr: attr_id("tbl-one"),
            content: vec![table, para("Table caption")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })];
        let ast = run_full(blocks).await;
        let Block::Div(d) = &ast.blocks[0] else {
            panic!();
        };
        assert_eq!(d.attr.0, "tbl-one");
        // Div should contain the Table and a trailing caption paragraph
        // with "Table 1: " prefix.
        assert!(matches!(d.content[0], Block::Table(_)));
        let last = d.content.last().unwrap();
        let Block::Paragraph(p) = last else { panic!() };
        let Inline::Str(s) = &p.content[0] else {
            panic!()
        };
        assert_eq!(s.text, "Table 1: ");
    }

    #[tokio::test]
    async fn resolved_ref_renders_to_link() {
        let blocks = vec![
            fig_div("fig-a", "Cap"),
            Block::Paragraph(Paragraph {
                content: vec![str_inline("see "), cite("fig-a")],
                source_info: si(),
            }),
        ];
        let ast = run_full(blocks).await;
        // First block is the rendered figure, second is the paragraph.
        let Block::Paragraph(p) = &ast.blocks[1] else {
            panic!();
        };
        let Inline::Link(link) = &p.content[1] else {
            panic!("expected Link, got {:?}", p.content[1]);
        };
        assert_eq!(link.target.0, "#fig-a");
        let Inline::Str(s) = &link.content[0] else {
            panic!();
        };
        assert_eq!(s.text, "Figure 1");
        assert!(link.attr.1.contains(&"quarto-xref".to_string()));
    }

    #[tokio::test]
    async fn unresolved_ref_renders_with_question_marks() {
        let blocks = vec![Block::Paragraph(Paragraph {
            content: vec![cite("fig-nope")],
            source_info: si(),
        })];
        let ast = run_full(blocks).await;
        let Block::Paragraph(p) = &ast.blocks[0] else {
            panic!();
        };
        let Inline::Link(link) = &p.content[0] else {
            panic!();
        };
        let Inline::Str(s) = &link.content[0] else {
            panic!();
        };
        assert_eq!(s.text, "?fig-nope?");
    }

    #[tokio::test]
    async fn float_ref_target_with_no_caption_renders_figure_with_empty_caption() {
        let blocks = vec![Block::Div(Div {
            attr: attr_id("fig-bare"),
            content: vec![Block::CodeBlock(CodeBlock {
                attr: (String::new(), vec!["python".into()], LinkedHashMap::new()),
                text: "x=1".into(),
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            })],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })];
        let ast = run_full(blocks).await;
        let Block::Figure(f) = &ast.blocks[0] else {
            panic!();
        };
        assert_eq!(f.attr.0, "fig-bare");
        // Caption.long stays empty — no "Figure 1: " prefix without a caption.
        assert!(f.caption.long.as_ref().map_or(true, |v| v.is_empty()));
    }

    #[test]
    fn prefix_caption_prepends_kind_and_number() {
        let cap = vec![Block::Paragraph(Paragraph {
            content: vec![str_inline("Hello")],
            source_info: si(),
        })];
        let out = prefix_caption(cap, "Figure", Some(3));
        let Block::Paragraph(p) = &out[0] else {
            panic!();
        };
        let Inline::Str(s) = &p.content[0] else {
            panic!();
        };
        assert_eq!(s.text, "Figure 3: ");
    }

    #[test]
    fn prefix_caption_no_number_still_adds_prefix() {
        let cap = vec![Block::Paragraph(Paragraph {
            content: vec![str_inline("Hello")],
            source_info: si(),
        })];
        let out = prefix_caption(cap, "Figure", None);
        let Block::Paragraph(p) = &out[0] else {
            panic!();
        };
        let Inline::Str(s) = &p.content[0] else {
            panic!();
        };
        assert_eq!(s.text, "Figure: ");
    }

    #[test]
    fn render_preserves_non_crossref_custom_nodes() {
        // A plain Callout-like custom node survives the render pass
        // untouched (it's not one of our two types).
        let mut callout = CustomNode::new(
            "Callout",
            (String::new(), Vec::new(), LinkedHashMap::new()),
            si(),
        );
        callout
            .slots
            .insert("content".into(), Slot::Blocks(vec![para("inside")]));
        let mut block = Block::Custom(callout);
        render_block(&mut block);
        match block {
            Block::Custom(n) => assert_eq!(n.type_name, "Callout"),
            _ => panic!("callout was mutated"),
        }
    }

    /// Helper to build a Div(.theorem) input block.
    fn theorem_div(id: &str, title: Option<&str>, body: &str) -> Block {
        let mut kvs = LinkedHashMap::new();
        if let Some(t) = title {
            kvs.insert("name".into(), t.to_string());
        }
        Block::Div(Div {
            attr: (id.into(), vec!["theorem".into()], kvs),
            content: vec![para(body)],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    #[tokio::test]
    async fn theorem_renders_to_div_with_numbered_label() {
        let ast = run_full(vec![theorem_div("thm-pyth", None, "body text")]).await;
        let Block::Div(div) = &ast.blocks[0] else {
            panic!("expected Div, got {:?}", ast.blocks[0]);
        };
        assert_eq!(div.attr.0, "thm-pyth");
        assert!(div.attr.1.iter().any(|c| c == "theorem"));
        // First child is a Paragraph beginning with "**Theorem 1.** body text".
        let Block::Paragraph(p) = &div.content[0] else {
            panic!();
        };
        // Strong inline with "Theorem 1." text.
        match &p.content[0] {
            Inline::Strong(s) => {
                let Inline::Str(label) = &s.content[0] else {
                    panic!("first strong inline should be Str");
                };
                assert_eq!(label.text, "Theorem 1");
                // Last strong piece is the period.
                let Inline::Str(dot) = s.content.last().unwrap() else {
                    panic!()
                };
                assert_eq!(dot.text, ".");
            }
            other => panic!("expected Strong label, got {:?}", other),
        }
        // Space inline after Strong, then the body text.
        let Inline::Str(sp) = &p.content[1] else {
            panic!()
        };
        assert_eq!(sp.text, " ");
        let Inline::Str(body) = &p.content[2] else {
            panic!()
        };
        assert_eq!(body.text, "body text");
    }

    #[tokio::test]
    async fn theorem_with_title_renders_parenthesized() {
        let ast = run_full(vec![theorem_div(
            "thm-pyth",
            Some("Pythagoras"),
            "a^2+b^2=c^2.",
        )])
        .await;
        let Block::Div(div) = &ast.blocks[0] else {
            panic!()
        };
        let Block::Paragraph(p) = &div.content[0] else {
            panic!()
        };
        let Inline::Strong(s) = &p.content[0] else {
            panic!()
        };
        // Expected: ["Theorem 1", " (", "Pythagoras", ")", "."]
        let parts: Vec<String> = s
            .content
            .iter()
            .map(|i| match i {
                Inline::Str(st) => st.text.clone(),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(parts, vec!["Theorem 1", " (", "Pythagoras", ")", "."]);
    }

    #[tokio::test]
    async fn theorem_counters_independent_from_lemmas() {
        let t1 = theorem_div("thm-a", None, "a");
        let t2 = theorem_div("thm-b", None, "b");
        let mut lemma_div = Block::Div(Div {
            attr: ("lem-c".into(), vec!["lemma".into()], LinkedHashMap::new()),
            content: vec![para("c")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        if let Block::Div(ref mut d) = lemma_div {
            let _ = d; // keep lemma_div mutable accessor consistent
        }
        let ast = run_full(vec![t1, t2, lemma_div]).await;
        // Extract the three labels.
        let strong_text = |block: &Block| -> String {
            let Block::Div(d) = block else {
                return "".into();
            };
            let Block::Paragraph(p) = &d.content[0] else {
                return "".into();
            };
            let Inline::Strong(s) = &p.content[0] else {
                return "".into();
            };
            let Inline::Str(st) = &s.content[0] else {
                return "".into();
            };
            st.text.clone()
        };
        assert_eq!(strong_text(&ast.blocks[0]), "Theorem 1");
        assert_eq!(strong_text(&ast.blocks[1]), "Theorem 2");
        assert_eq!(strong_text(&ast.blocks[2]), "Lemma 1");
    }

    #[tokio::test]
    async fn unnumbered_theorem_renders_without_number() {
        let div = Block::Div(Div {
            attr: (String::new(), vec!["theorem".into()], LinkedHashMap::new()),
            content: vec![para("unnumbered")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let ast = run_full(vec![div]).await;
        let Block::Div(div) = &ast.blocks[0] else {
            panic!()
        };
        let Block::Paragraph(p) = &div.content[0] else {
            panic!()
        };
        let Inline::Strong(s) = &p.content[0] else {
            panic!()
        };
        let Inline::Str(label) = &s.content[0] else {
            panic!()
        };
        // No number because there's no id -> no index entry -> no order.
        assert_eq!(label.text, "Theorem");
    }

    #[tokio::test]
    async fn theorem_ref_resolves_to_link() {
        let body = vec![
            theorem_div("thm-x", None, "body"),
            Block::Paragraph(Paragraph {
                content: vec![str_inline("see "), cite("thm-x")],
                source_info: si(),
            }),
        ];
        let ast = run_full(body).await;
        let Block::Paragraph(p) = &ast.blocks[1] else {
            panic!()
        };
        let Inline::Link(link) = &p.content[1] else {
            panic!("expected Link for thm-x ref")
        };
        assert_eq!(link.target.0, "#thm-x");
        let Inline::Str(s) = &link.content[0] else {
            panic!()
        };
        assert_eq!(s.text, "Theorem 1");
    }

    fn proof_div(title: Option<&str>, body: &str) -> Block {
        let mut kvs = LinkedHashMap::new();
        if let Some(t) = title {
            kvs.insert("name".into(), t.to_string());
        }
        Block::Div(Div {
            attr: ("".into(), vec!["proof".into()], kvs),
            content: vec![para(body)],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    #[tokio::test]
    async fn proof_renders_with_italic_label() {
        let ast = run_full(vec![proof_div(None, "body text")]).await;
        let Block::Div(div) = &ast.blocks[0] else {
            panic!("expected Div, got {:?}", ast.blocks[0]);
        };
        assert!(div.attr.1.iter().any(|c| c == "proof"));
        let Block::Paragraph(p) = &div.content[0] else {
            panic!()
        };
        // First inline should be Emph("Proof."), then Space " ", then body.
        let Inline::Emph(em) = &p.content[0] else {
            panic!("expected Emph, got {:?}", p.content[0])
        };
        let Inline::Str(s) = &em.content[0] else {
            panic!()
        };
        assert_eq!(s.text, "Proof.");
    }

    #[tokio::test]
    async fn proof_with_custom_title_renders_italic() {
        let ast = run_full(vec![proof_div(Some("of Theorem 1"), "...")]).await;
        let Block::Div(div) = &ast.blocks[0] else {
            panic!()
        };
        let Block::Paragraph(p) = &div.content[0] else {
            panic!()
        };
        let Inline::Emph(em) = &p.content[0] else {
            panic!()
        };
        // Should be "of Theorem 1" + ".".
        let parts: Vec<String> = em
            .content
            .iter()
            .map(|i| match i {
                Inline::Str(st) => st.text.clone(),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(parts, vec!["of Theorem 1", "."]);
    }
}
