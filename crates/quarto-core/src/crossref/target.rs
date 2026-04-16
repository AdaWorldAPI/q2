/*
 * crossref/target.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Uniform inspection API for crossref-capable blocks.
 */

//! Uniform inspection API for crossref-capable blocks.
//!
//! Float-ref targets post-sugaring are `CustomNode("FloatRefTarget", ..)`.
//! Future block-level crossref categories (theorems, callouts with ids, ...)
//! live in their own [`CustomNode`] types and carry their own slot structure,
//! but the *index builder* and *reference resolver* both want to treat any
//! crossref-capable block uniformly.
//!
//! This module exposes that shared view. Adding a new crossref-capable custom
//! node type in the future is a matter of extending
//! [`crossref_target_view`] to recognize it — all call sites gain support
//! automatically.
//!
//! Design reference: plan D1b ("shared inspection API") in
//! `claude-notes/plans/2026-04-15-crossref-design.md`.

use quarto_pandoc_types::block::Block;
use quarto_pandoc_types::custom::CustomNode;
use quarto_source_map::SourceInfo;

use super::FLOAT_REF_TARGET;

/// A uniform read-only view over the crossref-relevant fields of a block.
///
/// Borrowed from the underlying node — cheap to construct, cheap to pass
/// around. Callers that need ownership should `.to_owned()` individual fields.
#[derive(Debug, Clone, Copy)]
pub struct CrossrefTargetView<'a> {
    /// Full identifier, e.g. `"fig-myplot"`. Canonically taken from the
    /// node's `attr.identifier` (the block-level id); the CustomNode's
    /// `plain_data.identifier` is redundant and kept only for JSON
    /// readability.
    pub identifier: &'a str,

    /// Id prefix, e.g. `"fig"` — matches [`crate::crossref::RefTypeRegistry`]
    /// keys. Read from `plain_data.ref_type`.
    pub ref_type: &'a str,

    /// Display / category name, e.g. `"Figure"`. Read from `plain_data.kind`.
    pub kind: &'a str,

    /// Source location of the target in the authored document. Used for
    /// diagnostics (duplicate ids, unresolved refs).
    pub source_info: &'a SourceInfo,
}

/// Return a view over the block if it is a crossref-capable target.
///
/// Currently recognizes:
///
/// - `Block::Custom(CustomNode { type_name: "FloatRefTarget", .. })`.
///
/// Future work (per plan D1b) extends this to theorem-like and callout-with-id
/// nodes. Call sites do not need to change when that happens.
pub fn crossref_target_view(block: &Block) -> Option<CrossrefTargetView<'_>> {
    match block {
        Block::Custom(node) if node.type_name == FLOAT_REF_TARGET => float_ref_view(node),
        _ => None,
    }
}

/// Return the ref-type prefix if the block is a crossref target.
///
/// Convenience wrapper over [`crossref_target_view`] for callers that only
/// care about the prefix (e.g. bucketing targets by category before numbering).
pub fn ref_type_of(block: &Block) -> Option<&str> {
    crossref_target_view(block).map(|v| v.ref_type)
}

/// Return the identifier if the block is a crossref target.
pub fn identifier_of(block: &Block) -> Option<&str> {
    crossref_target_view(block).map(|v| v.identifier)
}

fn float_ref_view(node: &CustomNode) -> Option<CrossrefTargetView<'_>> {
    let identifier = node.attr.0.as_str();
    if identifier.is_empty() {
        // A FloatRefTarget without an identifier is a degenerate case that
        // shouldn't exist post-sugaring — return None rather than silently
        // inventing behavior.
        return None;
    }
    let ref_type = node.plain_data.get("ref_type")?.as_str()?;
    let kind = node.plain_data.get("kind")?.as_str()?;
    Some(CrossrefTargetView {
        identifier,
        ref_type,
        kind,
        source_info: &node.source_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashlink::LinkedHashMap;
    use quarto_pandoc_types::attr::empty_attr;
    use quarto_pandoc_types::block::{Block, Paragraph};
    use quarto_pandoc_types::custom::{CustomNode, Slot};
    use quarto_source_map::{FileId, SourceInfo};
    use serde_json::json;

    fn si() -> SourceInfo {
        SourceInfo::original(FileId(0), 0, 0)
    }

    fn make_float_ref_target(ident: &str, ref_type: &str, kind: &str) -> Block {
        let attr = (ident.to_string(), vec![], LinkedHashMap::new());
        let mut node = CustomNode::new(FLOAT_REF_TARGET, attr, si());
        node.plain_data = json!({
            "ref_type": ref_type,
            "kind": kind,
            "identifier": ident,
        });
        node.slots.insert("content".into(), Slot::Blocks(vec![]));
        Block::Custom(node)
    }

    #[test]
    fn view_for_float_ref_target() {
        let block = make_float_ref_target("fig-one", "fig", "Figure");
        let view = crossref_target_view(&block).expect("should be a crossref target");
        assert_eq!(view.identifier, "fig-one");
        assert_eq!(view.ref_type, "fig");
        assert_eq!(view.kind, "Figure");
    }

    #[test]
    fn view_none_for_plain_paragraph() {
        let block = Block::Paragraph(Paragraph {
            content: vec![],
            source_info: si(),
        });
        assert!(crossref_target_view(&block).is_none());
    }

    #[test]
    fn view_none_for_unrelated_custom_node() {
        let node = CustomNode::new("Callout", empty_attr(), si());
        let block = Block::Custom(node);
        assert!(crossref_target_view(&block).is_none());
    }

    #[test]
    fn view_none_for_float_ref_target_without_identifier() {
        let attr = (String::new(), vec![], LinkedHashMap::new());
        let mut node = CustomNode::new(FLOAT_REF_TARGET, attr, si());
        node.plain_data = json!({"ref_type": "fig", "kind": "Figure"});
        let block = Block::Custom(node);
        assert!(crossref_target_view(&block).is_none());
    }

    #[test]
    fn view_none_when_plain_data_missing_ref_type() {
        let attr = ("fig-one".to_string(), vec![], LinkedHashMap::new());
        let mut node = CustomNode::new(FLOAT_REF_TARGET, attr, si());
        node.plain_data = json!({"kind": "Figure"});
        let block = Block::Custom(node);
        assert!(crossref_target_view(&block).is_none());
    }

    #[test]
    fn convenience_helpers_agree_with_view() {
        let block = make_float_ref_target("tbl-x", "tbl", "Table");
        assert_eq!(ref_type_of(&block), Some("tbl"));
        assert_eq!(identifier_of(&block), Some("tbl-x"));
    }
}
