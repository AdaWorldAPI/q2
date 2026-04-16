/*
 * transforms/theorem.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Sugar transform for theorem-like blocks.
 */

//! Sugar transform that canonicalizes theorem-like blocks.
//!
//! This transform runs in the **normalization** phase (plan D3). It
//! detects `Div` blocks whose class list names a theorem-like category
//! (`.theorem`, `.lemma`, `.corollary`, `.proposition`, `.conjecture`,
//! `.definition`, `.example`, `.exercise`) and converts them into
//! `CustomNode("Theorem")` with the same `plain_data` shape that
//! [`FloatRefTarget`](crate::crossref::FLOAT_REF_TARGET) uses:
//!
//! - `ref_type` (prefix): `thm`, `lem`, `cor`, `prp`, `cnj`, `def`,
//!   `exm`, `exr`.
//! - `kind` (display name): `Theorem`, `Lemma`, ...
//! - `identifier`: full id, e.g. `"thm-pythagoras"` — taken from the Div
//!   attr. Empty iff the author omitted the id.
//!
//! Slots:
//! - `"content"` (Blocks) — the body of the theorem (after title
//!   extraction).
//! - `"title"` (Inlines) — optional title. Extracted from (in order):
//!   1. The `name=` key-value on the Div's attr (Q1 convention).
//!   2. The first `Header` child inside the Div, if any.
//!
//! This matches the existing `crossref_target_view` contract, so the
//! indexer and resolver see theorem custom nodes uniformly with
//! `FloatRefTarget`s without any changes — populate `plain_data` the
//! same way and it just works.
//!
//! ## Why one `"Theorem"` type, not one per flavor
//!
//! Per plan D1b, theorem-like structures share their structural shape
//! and only differ in kind/numbering. One custom type with a `kind`
//! field is sufficient and keeps the filter surface small. If a later
//! phase introduces kind-specific structure (e.g., example with
//! "solution" slot), we can split then.

use quarto_pandoc_types::attr::{Attr, AttrSourceInfo};
use quarto_pandoc_types::block::{Block, Blocks, Div, Header};
use quarto_pandoc_types::custom::{CustomNode, Slot};
use quarto_pandoc_types::inline::Inlines;
use quarto_pandoc_types::pandoc::Pandoc;
use serde_json::json;

use crate::Result;
use crate::crossref::THEOREM;
use crate::render::RenderContext;
use crate::transform::AstTransform;

/// Map of theorem-like class name to `(ref_type_prefix, display_kind)`.
///
/// Kept static so the lookup is branchless. Class names are
/// case-sensitive; users write `.theorem` (not `.Theorem`).
const THEOREM_CLASSES: &[(&str, &str, &str)] = &[
    ("theorem", "thm", "Theorem"),
    ("lemma", "lem", "Lemma"),
    ("corollary", "cor", "Corollary"),
    ("proposition", "prp", "Proposition"),
    ("conjecture", "cnj", "Conjecture"),
    ("definition", "def", "Definition"),
    ("example", "exm", "Example"),
    ("exercise", "exr", "Exercise"),
];

/// Sugar transform that converts `Div(.theorem-like)` into
/// `CustomNode("Theorem")`.
pub struct TheoremSugarTransform;

impl TheoremSugarTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TheoremSugarTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for TheoremSugarTransform {
    fn name(&self) -> &str {
        "theorem-sugar"
    }

    async fn transform(&self, ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        transform_blocks(&mut ast.blocks);
        Ok(())
    }
}

fn transform_blocks(blocks: &mut Blocks) {
    for block in blocks.iter_mut() {
        transform_block(block);
    }
}

fn transform_block(block: &mut Block) {
    // Recurse into children first so nested theorems are handled bottom-up.
    match block {
        Block::BlockQuote(bq) => transform_blocks(&mut bq.content),
        Block::OrderedList(ol) => {
            for item in &mut ol.content {
                transform_blocks(item);
            }
        }
        Block::BulletList(bl) => {
            for item in &mut bl.content {
                transform_blocks(item);
            }
        }
        Block::DefinitionList(dl) => {
            for (_term, defs) in &mut dl.content {
                for def in defs {
                    transform_blocks(def);
                }
            }
        }
        Block::Figure(fig) => transform_blocks(&mut fig.content),
        Block::Div(div) => transform_blocks(&mut div.content),
        Block::Custom(node) => {
            for (_name, slot) in node.slots.iter_mut() {
                match slot {
                    Slot::Block(b) => transform_block(b),
                    Slot::Blocks(bs) => transform_blocks(bs),
                    _ => {}
                }
            }
        }
        _ => {}
    }

    // Check this node itself.
    if let Block::Div(div) = block {
        if let Some((ref_type, kind)) = match_theorem_class(&div.attr) {
            let converted = convert_div(
                std::mem::replace(
                    div,
                    Div {
                        attr: empty_attr(),
                        content: Vec::new(),
                        source_info: div.source_info.clone(),
                        attr_source: AttrSourceInfo::empty(),
                    },
                ),
                ref_type,
                kind,
            );
            *block = Block::Custom(converted);
        }
    }
}

/// If this Div's class list names a theorem-like category, return the
/// matching `(ref_type, kind)` pair. The first matching class wins — a
/// Div with both `.theorem` and `.lemma` is unusual; we don't try to be
/// clever about it.
fn match_theorem_class(attr: &Attr) -> Option<(&'static str, &'static str)> {
    for class in &attr.1 {
        for (name, ref_type, kind) in THEOREM_CLASSES {
            if class == name {
                return Some((ref_type, kind));
            }
        }
    }
    None
}

fn empty_attr() -> Attr {
    use hashlink::LinkedHashMap;
    (String::new(), Vec::new(), LinkedHashMap::new())
}

/// Convert a Div we've already matched to the theorem-like set into a
/// `CustomNode("Theorem")`.
///
/// Preserves the original Div's `attr` (so the id flows through intact)
/// but strips the theorem class name itself from the attr's class list —
/// the `plain_data.kind` is the authoritative source from here on. This
/// prevents double-rendering later if a CSS-style transform matches on
/// `.theorem`.
fn convert_div(mut div: Div, ref_type: &str, kind: &str) -> CustomNode {
    // Extract title:
    //   1. `name=` attribute on the Div (Q1 convention).
    //   2. First Header child, if present.
    let title: Option<Inlines> =
        extract_name_attr(&mut div.attr).or_else(|| extract_first_header_title(&mut div.content));

    // Strip the theorem class so downstream transforms don't re-match.
    div.attr
        .1
        .retain(|c| c.as_str() != theorem_class_for(ref_type));

    let identifier = div.attr.0.clone();

    let mut node = CustomNode::new(THEOREM, div.attr, div.source_info);
    node.plain_data = json!({
        "ref_type":   ref_type,
        "kind":       kind,
        "identifier": identifier,
    });
    node.slots
        .insert("content".into(), Slot::Blocks(div.content));
    if let Some(inlines) = title {
        if !inlines.is_empty() {
            node.slots.insert("title".into(), Slot::Inlines(inlines));
        }
    }
    node
}

/// Read and remove the `name` attribute from `attr`, returning its value
/// parsed as plain inlines (just a `Str`, since attrs are strings).
///
/// The user-facing `name="Pythagoras"` becomes
/// `vec![Str("Pythagoras")]`. Inline markup inside the title (bold,
/// italic, etc.) isn't supported today because attribute values are
/// bare strings in Pandoc's data model — matching Q1's behavior.
fn extract_name_attr(attr: &mut Attr) -> Option<Inlines> {
    let (_id, _classes, kvs) = attr;
    let name = kvs.remove("name")?;
    if name.is_empty() {
        return None;
    }
    Some(vec![quarto_pandoc_types::inline::Inline::Str(
        quarto_pandoc_types::inline::Str {
            text: name,
            source_info: quarto_source_map::SourceInfo::default(),
        },
    )])
}

/// Pop the first `Header` from the content blocks and return its inline
/// content as a title. If the first block isn't a Header, leaves
/// `content` unchanged and returns None.
fn extract_first_header_title(content: &mut Blocks) -> Option<Inlines> {
    if let Some(Block::Header(_)) = content.first() {
        let first = content.remove(0);
        if let Block::Header(Header { content: title, .. }) = first {
            return Some(title);
        }
    }
    None
}

fn theorem_class_for(ref_type: &str) -> &'static str {
    for (class, rt, _kind) in THEOREM_CLASSES {
        if *rt == ref_type {
            return class;
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crossref::crossref_target_view;
    use hashlink::LinkedHashMap;
    use quarto_pandoc_types::attr::{Attr, AttrSourceInfo};
    use quarto_pandoc_types::block::{Block, Div, Header, Paragraph};
    use quarto_pandoc_types::inline::{Inline, Str};
    use quarto_source_map::{FileId, SourceInfo};

    fn si() -> SourceInfo {
        SourceInfo::original(FileId(0), 0, 0)
    }

    fn attr_id_classes(id: &str, classes: &[&str]) -> Attr {
        (
            id.to_string(),
            classes.iter().map(|s| s.to_string()).collect(),
            LinkedHashMap::new(),
        )
    }

    fn para(text: &str) -> Block {
        Block::Paragraph(Paragraph {
            content: vec![Inline::Str(Str {
                text: text.into(),
                source_info: si(),
            })],
            source_info: si(),
        })
    }

    fn run(mut blocks: Vec<Block>) -> Vec<Block> {
        transform_blocks(&mut blocks);
        blocks
    }

    #[test]
    fn plain_theorem_div_becomes_theorem_custom_node() {
        let div = Block::Div(Div {
            attr: attr_id_classes("thm-pyth", &["theorem"]),
            content: vec![para("For a right triangle...")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div]);

        let Block::Custom(node) = &out[0] else {
            panic!("expected custom node, got {:?}", out[0]);
        };
        assert_eq!(node.type_name, THEOREM);
        assert_eq!(node.plain_data["ref_type"], "thm");
        assert_eq!(node.plain_data["kind"], "Theorem");
        assert_eq!(node.plain_data["identifier"], "thm-pyth");
        // `.theorem` class stripped from attr so a later "match div.theorem"
        // filter doesn't double-apply.
        assert!(!node.attr.1.iter().any(|c| c == "theorem"));

        // Content preserved in slot.
        let Some(Slot::Blocks(bs)) = node.slots.get("content") else {
            panic!();
        };
        assert_eq!(bs.len(), 1);
        assert!(matches!(bs[0], Block::Paragraph(_)));
    }

    #[test]
    fn lemma_corollary_etc_all_recognized() {
        for (class, ref_type, kind) in [
            ("lemma", "lem", "Lemma"),
            ("corollary", "cor", "Corollary"),
            ("proposition", "prp", "Proposition"),
            ("conjecture", "cnj", "Conjecture"),
            ("definition", "def", "Definition"),
            ("example", "exm", "Example"),
            ("exercise", "exr", "Exercise"),
        ] {
            let div = Block::Div(Div {
                attr: attr_id_classes(&format!("{ref_type}-x"), &[class]),
                content: vec![para("body")],
                source_info: si(),
                attr_source: AttrSourceInfo::empty(),
            });
            let out = run(vec![div]);
            let Block::Custom(node) = &out[0] else {
                panic!("{} did not sugar", class);
            };
            assert_eq!(node.plain_data["ref_type"], ref_type);
            assert_eq!(node.plain_data["kind"], kind);
        }
    }

    #[test]
    fn div_without_theorem_class_untouched() {
        let div = Block::Div(Div {
            attr: attr_id_classes("thm-fake", &["callout-note"]),
            content: vec![para("not a theorem")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div.clone()]);
        assert_eq!(out, vec![div]);
    }

    #[test]
    fn name_attribute_becomes_title_slot() {
        let mut kvs = LinkedHashMap::new();
        kvs.insert("name".into(), "Pythagorean Theorem".into());
        let div = Block::Div(Div {
            attr: ("thm-pyth".into(), vec!["theorem".into()], kvs),
            content: vec![para("body")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div]);
        let Block::Custom(node) = &out[0] else {
            panic!()
        };
        let Some(Slot::Inlines(title)) = node.slots.get("title") else {
            panic!("no title slot");
        };
        assert_eq!(title.len(), 1);
        match &title[0] {
            Inline::Str(s) => assert_eq!(s.text, "Pythagorean Theorem"),
            _ => panic!(),
        }
        // `name` key removed from attr so it doesn't leak into rendered
        // output (would otherwise appear as a data-name="...").
        assert!(!node.attr.2.contains_key("name"));
    }

    #[test]
    fn first_header_becomes_title_if_no_name_attribute() {
        let div = Block::Div(Div {
            attr: attr_id_classes("thm-x", &["theorem"]),
            content: vec![
                Block::Header(Header {
                    level: 3,
                    attr: (String::new(), Vec::new(), LinkedHashMap::new()),
                    content: vec![Inline::Str(Str {
                        text: "Header title".into(),
                        source_info: si(),
                    })],
                    source_info: si(),
                    attr_source: AttrSourceInfo::empty(),
                }),
                para("body"),
            ],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div]);
        let Block::Custom(node) = &out[0] else {
            panic!();
        };
        let Some(Slot::Inlines(title)) = node.slots.get("title") else {
            panic!()
        };
        match &title[0] {
            Inline::Str(s) => assert_eq!(s.text, "Header title"),
            _ => panic!(),
        }
        // Header removed from content, leaving just the body para.
        let Some(Slot::Blocks(bs)) = node.slots.get("content") else {
            panic!()
        };
        assert_eq!(bs.len(), 1);
        assert!(matches!(bs[0], Block::Paragraph(_)));
    }

    #[test]
    fn crossref_target_view_recognizes_sugared_theorem() {
        let div = Block::Div(Div {
            attr: attr_id_classes("thm-x", &["theorem"]),
            content: vec![para("body")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div]);
        let view = crossref_target_view(&out[0]).expect("theorem visible as crossref target");
        assert_eq!(view.identifier, "thm-x");
        assert_eq!(view.ref_type, "thm");
        assert_eq!(view.kind, "Theorem");
    }

    #[test]
    fn theorem_without_id_still_sugars() {
        // Unnumbered theorem: no id. Still becomes a Theorem custom
        // node so renderers can style it consistently.
        let div = Block::Div(Div {
            attr: attr_id_classes("", &["theorem"]),
            content: vec![para("body")],
            source_info: si(),
            attr_source: AttrSourceInfo::empty(),
        });
        let out = run(vec![div]);
        let Block::Custom(node) = &out[0] else {
            panic!()
        };
        assert_eq!(node.plain_data["identifier"], "");
    }
}
