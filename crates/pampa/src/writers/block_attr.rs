/*
 * block_attr.rs
 * Copyright (c) 2026 Posit, PBC
 *
 * Shared collection of trailing standalone `Inline::Attr` nodes into a single
 * block-level `Attr`, so writers can attach attributes to Pandoc blocks that
 * have no native `Attr` field (`Paragraph`, `Plain`, list items). See
 * bd-itqcfxc3 and claude-notes/plans/2026-06-17-block-level-attrs-inline-attr.md.
 */

use hashlink::LinkedHashMap;
use quarto_pandoc_types::attr::{Attr, empty_attr};
use quarto_pandoc_types::inline::Inline;

/// Split a block's inline content into `(retained_content, merged_attr)` by
/// collecting the **trailing run** of standalone [`Inline::Attr`] nodes — plus
/// any whitespace (`Space`/`SoftBreak`) interleaved with, or immediately
/// preceding, that run — and merging the attrs into one [`Attr`].
///
/// The retained content is always a prefix of `inlines` (the trailing run sits
/// at the end), so the return is a borrowed slice — no clone.
///
/// If the block carries no trailing `Inline::Attr`, `inlines` is returned
/// unchanged with an empty attr (and no whitespace is stripped). Callers should
/// treat an empty attr (`is_empty_attr`) as "no block attribute".
///
/// Merge rule (mirrors the `Header` fold in `postprocess.rs`): processing the
/// collected attrs in document order, `id` = last non-empty wins, `classes`
/// accumulate in order (deduplicated), key-values = last wins. Empty attrs in
/// the run contribute nothing.
///
/// Only a *trailing* run is collected (matching the heading/table precedent); a
/// standalone `Inline::Attr` in the middle of content is left in place for the
/// caller to handle (the inline writers already drop it from output).
pub(crate) fn split_trailing_block_attr(inlines: &[Inline]) -> (&[Inline], Attr) {
    // Walk back from the end over a maximal run of (Attr | whitespace),
    // remembering where the run starts and whether it contains any Attr.
    let mut run_start = inlines.len();
    let mut saw_attr = false;
    for (idx, inline) in inlines.iter().enumerate().rev() {
        match inline {
            Inline::Attr(_) => {
                saw_attr = true;
                run_start = idx;
            }
            Inline::Space(_) | Inline::SoftBreak(_) => {
                run_start = idx;
            }
            _ => break,
        }
    }

    if !saw_attr {
        // No trailing attr — leave content (and any trailing whitespace) intact.
        return (inlines, empty_attr());
    }

    let mut id = String::new();
    let mut classes: Vec<String> = Vec::new();
    let mut kvs: LinkedHashMap<String, String> = LinkedHashMap::new();
    for inline in &inlines[run_start..] {
        if let Inline::Attr(a) = inline {
            let (a_id, a_classes, a_kvs) = &a.attr;
            if !a_id.is_empty() {
                id = a_id.clone();
            }
            for c in a_classes {
                if !classes.contains(c) {
                    classes.push(c.clone());
                }
            }
            for (k, v) in a_kvs {
                kvs.insert(k.clone(), v.clone());
            }
        }
    }

    (&inlines[..run_start], (id, classes, kvs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quarto_pandoc_types::attr::{AttrSourceInfo, is_empty_attr};
    use quarto_pandoc_types::inline::{InlineAttr, Space, Str};
    use quarto_source_map::SourceInfo;

    fn si() -> SourceInfo {
        SourceInfo::for_test()
    }

    fn str_(text: &str) -> Inline {
        Inline::Str(Str {
            text: text.to_string(),
            source_info: si(),
        })
    }

    fn space() -> Inline {
        Inline::Space(Space { source_info: si() })
    }

    fn attr_node(id: &str, classes: &[&str], kvs: &[(&str, &str)]) -> Inline {
        let mut m = LinkedHashMap::new();
        for (k, v) in kvs {
            m.insert(k.to_string(), v.to_string());
        }
        let attr: Attr = (
            id.to_string(),
            classes.iter().map(|s| s.to_string()).collect(),
            m,
        );
        Inline::Attr(InlineAttr::new(attr, AttrSourceInfo::empty(), si()))
    }

    fn texts(inlines: &[Inline]) -> Vec<String> {
        inlines
            .iter()
            .map(|i| match i {
                Inline::Str(s) => format!("Str({})", s.text),
                Inline::Space(_) => "Space".to_string(),
                Inline::Attr(_) => "Attr".to_string(),
                _ => "?".to_string(),
            })
            .collect()
    }

    #[test]
    fn no_trailing_attr_is_unchanged() {
        let inlines = vec![str_("hello"), space()];
        let (content, attr) = split_trailing_block_attr(&inlines);
        assert_eq!(texts(content), vec!["Str(hello)", "Space"]);
        assert!(is_empty_attr(&attr));
    }

    #[test]
    fn trailing_attr_with_preceding_space_is_stripped() {
        let inlines = vec![str_("caption"), space(), attr_node("", &["caption"], &[])];
        let (content, attr) = split_trailing_block_attr(&inlines);
        assert_eq!(texts(content), vec!["Str(caption)"]);
        assert_eq!(attr.1, vec!["caption".to_string()]);
        assert!(attr.0.is_empty());
    }

    #[test]
    fn trailing_whitespace_after_attr_is_stripped() {
        let inlines = vec![str_("x"), space(), attr_node("", &["c"], &[]), space()];
        let (content, _) = split_trailing_block_attr(&inlines);
        assert_eq!(texts(content), vec!["Str(x)"]);
    }

    #[test]
    fn multiple_attrs_merge_in_document_order() {
        // id: last non-empty wins; classes accumulate (dedup); kv: last wins.
        let inlines = vec![
            str_("x"),
            attr_node("first", &["a", "b"], &[("k", "1")]),
            attr_node("second", &["b", "c"], &[("k", "2")]),
        ];
        let (content, attr) = split_trailing_block_attr(&inlines);
        assert_eq!(texts(content), vec!["Str(x)"]);
        assert_eq!(attr.0, "second");
        assert_eq!(
            attr.1,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(attr.2.get("k"), Some(&"2".to_string()));
    }

    #[test]
    fn empty_attr_node_collects_to_nothing() {
        let inlines = vec![str_("x"), attr_node("", &[], &[])];
        let (content, attr) = split_trailing_block_attr(&inlines);
        // The (empty) attr node is still stripped from content...
        assert_eq!(texts(content), vec!["Str(x)"]);
        // ...but contributes no id/classes/kvs.
        assert!(is_empty_attr(&attr));
    }

    #[test]
    fn mid_content_attr_is_left_in_place() {
        let inlines = vec![attr_node("", &["mid"], &[]), str_("x")];
        let (content, attr) = split_trailing_block_attr(&inlines);
        assert_eq!(texts(content), vec!["Attr", "Str(x)"]);
        assert!(is_empty_attr(&attr));
    }
}
