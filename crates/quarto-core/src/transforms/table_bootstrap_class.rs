/*
 * transforms/table_bootstrap_class.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Inject Bootstrap's `table` and `caption-top` classes on every
//! `Block::Table` in the AST.
//!
//! Mirrors Quarto 1's `quarto-bootstrap-table.lua` filter. Quarto's
//! default HTML output ships a Bootstrap stylesheet whose table styling
//! is keyed off the `.table` class; the caption position is keyed off
//! `.caption-top`. Pandoc's HTML writer emits a bare `<table>` by
//! default, so without this enrichment the rendered table inherits no
//! Bootstrap styling.
//!
//! Pipeline placement: **FINALIZATION PHASE**, just before
//! `AttributionRenderTransform`. The classes are HTML-output-specific
//! (this whole pipeline only runs for HTML targets), and running late
//! keeps the AST clean for upstream transforms / user filters that
//! might inspect table classes for their own reasons.
//!
//! Bd-2c8rg. See
//! `claude-notes/plans/2026-05-20-table-default-rendering-parity.md`
//! (D2).

use quarto_pandoc_types::block::Block;
use quarto_pandoc_types::pandoc::Pandoc;

use crate::Result;
use crate::render::RenderContext;
use crate::transform::{AstTransform, TransformPhase};

/// Bootstrap class added to every `<table>` so the Bootstrap stylesheet
/// can theme it. Matches Quarto 1.
const TABLE_CLASS: &str = "table";
/// Class that flips the Pandoc default caption position from `bottom`
/// (browser default for `<table>`) to `top`. Matches Quarto 1.
const CAPTION_TOP_CLASS: &str = "caption-top";

pub struct TableBootstrapClassTransform;

impl TableBootstrapClassTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TableBootstrapClassTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for TableBootstrapClassTransform {
    fn name(&self) -> &str {
        "table-bootstrap-class"
    }

    fn phase(&self) -> TransformPhase {
        TransformPhase::Finalization
    }

    async fn transform(&self, ast: &mut Pandoc, _ctx: &mut RenderContext) -> Result<()> {
        apply_to_blocks(&mut ast.blocks);
        Ok(())
    }
}

/// Apply the class enrichment to every `Block::Table` reachable from
/// `blocks`. Recursive: catches tables nested in divs, blockquotes,
/// figures, lists, and other tables' cells.
fn apply_to_blocks(blocks: &mut Vec<Block>) {
    for block in blocks.iter_mut() {
        apply_to_block(block);
    }
}

fn apply_to_block(block: &mut Block) {
    match block {
        Block::Table(table) => {
            ensure_class(&mut table.attr.1, CAPTION_TOP_CLASS);
            ensure_class(&mut table.attr.1, TABLE_CLASS);
            if let Some(ref mut caption) = table.caption.long {
                apply_to_blocks(caption);
            }
            for body in &mut table.bodies {
                for row in &mut body.body {
                    for cell in &mut row.cells {
                        apply_to_blocks(&mut cell.content);
                    }
                }
                for row in &mut body.head {
                    for cell in &mut row.cells {
                        apply_to_blocks(&mut cell.content);
                    }
                }
            }
            for row in &mut table.head.rows {
                for cell in &mut row.cells {
                    apply_to_blocks(&mut cell.content);
                }
            }
            for row in &mut table.foot.rows {
                for cell in &mut row.cells {
                    apply_to_blocks(&mut cell.content);
                }
            }
        }
        Block::Div(div) => apply_to_blocks(&mut div.content),
        Block::BlockQuote(bq) => apply_to_blocks(&mut bq.content),
        Block::Figure(fig) => {
            apply_to_blocks(&mut fig.content);
            if let Some(ref mut caption) = fig.caption.long {
                apply_to_blocks(caption);
            }
        }
        Block::OrderedList(ol) => {
            for item in &mut ol.content {
                apply_to_blocks(item);
            }
        }
        Block::BulletList(bl) => {
            for item in &mut bl.content {
                apply_to_blocks(item);
            }
        }
        Block::DefinitionList(dl) => {
            for (_term, defs) in &mut dl.content {
                for def in defs {
                    apply_to_blocks(def);
                }
            }
        }
        _ => {}
    }
}

/// Push `class` onto `classes` if it isn't already present. Preserves
/// insertion order, so first-runs are appended after existing user
/// classes (idempotent on second run).
fn ensure_class(classes: &mut Vec<String>, class: &str) {
    if !classes.iter().any(|c| c == class) {
        classes.push(class.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashlink::LinkedHashMap;
    use quarto_pandoc_types::ConfigValue;
    use quarto_pandoc_types::attr::AttrSourceInfo;
    use quarto_pandoc_types::block::{Block, Div, Plain};
    use quarto_pandoc_types::caption::Caption;
    use quarto_pandoc_types::inline::{Inline, Str};
    use quarto_pandoc_types::table::{
        Alignment, Cell, Row, Table, TableBody, TableFoot, TableHead,
    };
    use quarto_source_map::{FileId, Location, Range, SourceInfo};

    fn dummy_source_info() -> SourceInfo {
        SourceInfo::from_range(
            FileId(0),
            Range {
                start: Location {
                    offset: 0,
                    row: 0,
                    column: 0,
                },
                end: Location {
                    offset: 0,
                    row: 0,
                    column: 0,
                },
            },
        )
    }

    fn empty_attr() -> (String, Vec<String>, LinkedHashMap<String, String>) {
        (String::new(), Vec::new(), LinkedHashMap::new())
    }

    fn table_with_classes(classes: Vec<&str>) -> Block {
        let attr = (
            String::new(),
            classes.into_iter().map(|s| s.to_string()).collect(),
            LinkedHashMap::new(),
        );
        Block::Table(Table {
            attr,
            caption: Caption {
                short: None,
                long: None,
                source_info: dummy_source_info(),
            },
            colspec: vec![],
            head: TableHead {
                attr: empty_attr(),
                rows: vec![],
                source_info: dummy_source_info(),
                attr_source: AttrSourceInfo::empty(),
            },
            bodies: vec![TableBody {
                attr: empty_attr(),
                rowhead_columns: 0,
                head: vec![],
                body: vec![Row {
                    attr: empty_attr(),
                    cells: vec![Cell {
                        attr: empty_attr(),
                        alignment: Alignment::Default,
                        row_span: 1,
                        col_span: 1,
                        content: vec![Block::Plain(Plain {
                            content: vec![Inline::Str(Str {
                                text: "x".to_string(),
                                source_info: dummy_source_info(),
                            })],
                            source_info: dummy_source_info(),
                        })],
                        source_info: dummy_source_info(),
                        attr_source: AttrSourceInfo::empty(),
                    }],
                    source_info: dummy_source_info(),
                    attr_source: AttrSourceInfo::empty(),
                }],
                source_info: dummy_source_info(),
                attr_source: AttrSourceInfo::empty(),
            }],
            foot: TableFoot {
                attr: empty_attr(),
                rows: vec![],
                source_info: dummy_source_info(),
                attr_source: AttrSourceInfo::empty(),
            },
            source_info: dummy_source_info(),
            attr_source: AttrSourceInfo::empty(),
        })
    }

    fn ast_with(blocks: Vec<Block>) -> Pandoc {
        Pandoc {
            meta: ConfigValue::default(),
            blocks,
        }
    }

    fn table_classes(block: &Block) -> &[String] {
        match block {
            Block::Table(t) => &t.attr.1,
            _ => panic!("not a table"),
        }
    }

    #[test]
    fn bare_table_gets_both_classes() {
        let mut ast = ast_with(vec![table_with_classes(vec![])]);
        apply_to_blocks(&mut ast.blocks);
        assert_eq!(
            table_classes(&ast.blocks[0]),
            &["caption-top".to_string(), "table".to_string()]
        );
    }

    #[test]
    fn existing_user_class_is_preserved() {
        let mut ast = ast_with(vec![table_with_classes(vec!["my-class"])]);
        apply_to_blocks(&mut ast.blocks);
        assert_eq!(
            table_classes(&ast.blocks[0]),
            &[
                "my-class".to_string(),
                "caption-top".to_string(),
                "table".to_string()
            ]
        );
    }

    #[test]
    fn idempotent_on_second_run() {
        let mut ast = ast_with(vec![table_with_classes(vec![])]);
        apply_to_blocks(&mut ast.blocks);
        let first = table_classes(&ast.blocks[0]).to_vec();
        apply_to_blocks(&mut ast.blocks);
        let second = table_classes(&ast.blocks[0]).to_vec();
        assert_eq!(first, second, "running twice must not duplicate classes");
    }

    #[test]
    fn already_classed_table_unchanged() {
        let mut ast = ast_with(vec![table_with_classes(vec!["caption-top", "table"])]);
        apply_to_blocks(&mut ast.blocks);
        assert_eq!(
            table_classes(&ast.blocks[0]),
            &["caption-top".to_string(), "table".to_string()]
        );
    }

    #[test]
    fn one_of_two_classes_present_only_other_added() {
        let mut ast = ast_with(vec![table_with_classes(vec!["table"])]);
        apply_to_blocks(&mut ast.blocks);
        assert_eq!(
            table_classes(&ast.blocks[0]),
            &["table".to_string(), "caption-top".to_string()]
        );
    }

    #[test]
    fn table_nested_in_div_is_reached() {
        let div = Block::Div(Div {
            attr: empty_attr(),
            content: vec![table_with_classes(vec![])],
            source_info: dummy_source_info(),
            attr_source: AttrSourceInfo::empty(),
        });
        let mut ast = ast_with(vec![div]);
        apply_to_blocks(&mut ast.blocks);
        let Block::Div(d) = &ast.blocks[0] else {
            panic!("not a div");
        };
        assert_eq!(
            table_classes(&d.content[0]),
            &["caption-top".to_string(), "table".to_string()]
        );
    }
}
