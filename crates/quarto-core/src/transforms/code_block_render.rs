/*
 * transforms/code_block_render.rs
 * Copyright (c) 2026 Posit, PBC
 */

//! Code-block decoration *Render* transform.
//!
//! Format-specific half of the code-block decoration pipeline,
//! consuming the typed
//! [`CodeBlockDecoration`](super::code_block_generate::CodeBlockDecoration)
//! produced by
//! [`CodeBlockGenerateTransform`](super::code_block_generate::CodeBlockGenerateTransform).
//!
//! Phase 1 (this commit): filename header.
//! When a code block carries a `filename` decoration, the transform
//! replaces the `Block::CodeBlock` with a `Block::Div { class: "code-with-filename" }`
//! that contains:
//!
//! - A filename-header `RawBlock("html", …)` carrying the exact markup
//!   Quarto 1 produced — `<div class="code-with-filename-file"><pre><strong>filename</strong></pre></div>`
//!   — so the ported SCSS (`_quarto-rules-code-filename.scss`) matches
//!   selectors byte-for-byte.
//! - The original `CodeBlock` unchanged. The HTML writer's
//!   `<div class="sourceCode">` wrapper (emitted whenever
//!   `data-hl-spans` is present, per the change in c81b6001) then
//!   nests *inside* the filename wrapper, exactly matching Q1's
//!   composition.
//!
//! Phase 2 / 3 will extend the wrapper logic: copy button outside the
//! filename header, `<details>` fold outermost.
//!
//! Pipeline placement: **Finalization Phase**, alongside
//! [`CrossrefRenderTransform`](super::CrossrefRenderTransform) and
//! before [`AttributionRenderTransform`](super::AttributionRenderTransform).

use quarto_pandoc_types::attr::AttrSourceInfo;
use quarto_pandoc_types::block::{Block, Blocks, Div, RawBlock};
use quarto_pandoc_types::pandoc::Pandoc;

use crate::Result;
use crate::render::RenderContext;
use crate::transform::AstTransform;
use crate::transforms::code_block_generate::{
    CodeBlockDecoration, CodeBlockDecorationKey, decoration_has_any_field,
};

/// See module docs.
pub struct CodeBlockRenderTransform;

impl CodeBlockRenderTransform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodeBlockRenderTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl AstTransform for CodeBlockRenderTransform {
    fn name(&self) -> &str {
        "code-block-render"
    }

    async fn transform(&self, ast: &mut Pandoc, ctx: &mut RenderContext) -> Result<()> {
        // Short-circuit: nothing to do if Generate produced no
        // decorations. The HashMap lookup overhead would be harmless
        // but the AST walk isn't free.
        if ctx.code_block_decorations.is_empty() {
            return Ok(());
        }

        wrap_decorated_blocks(&mut ast.blocks, &ctx.code_block_decorations);
        Ok(())
    }
}

/// Walk `blocks`, replacing each decorated `CodeBlock` with the
/// appropriate wrapper structure. Descends into containers so
/// decorations attach to nested code blocks too.
///
/// Walks the same container variants as
/// [`super::code_block_generate::CodeBlockGenerateTransform`] — the
/// two must stay in sync.
fn wrap_decorated_blocks(
    blocks: &mut Blocks,
    decorations: &std::collections::HashMap<CodeBlockDecorationKey, CodeBlockDecoration>,
) {
    for block in blocks.iter_mut() {
        match block {
            Block::CodeBlock(cb) => {
                let Some(key) = CodeBlockDecorationKey::from_source_info(&cb.source_info) else {
                    continue;
                };
                let Some(decoration) = decorations.get(&key) else {
                    continue;
                };
                if !decoration_has_any_field(decoration) {
                    continue;
                }
                wrap_in_place(block, decoration);
            }
            Block::BlockQuote(bq) => wrap_decorated_blocks(&mut bq.content, decorations),
            Block::Div(div) => wrap_decorated_blocks(&mut div.content, decorations),
            Block::Figure(fig) => wrap_decorated_blocks(&mut fig.content, decorations),
            Block::OrderedList(list) => {
                for item in list.content.iter_mut() {
                    wrap_decorated_blocks(item, decorations);
                }
            }
            Block::BulletList(list) => {
                for item in list.content.iter_mut() {
                    wrap_decorated_blocks(item, decorations);
                }
            }
            Block::DefinitionList(dl) => {
                for (_term, defs) in dl.content.iter_mut() {
                    for def in defs.iter_mut() {
                        wrap_decorated_blocks(def, decorations);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Replace a `Block::CodeBlock` (already confirmed decorated) in place
/// with the wrapper structure described in the module docs.
///
/// Move semantics: the original `CodeBlock` is moved into the new
/// wrapper Div, so its content (including `data-hl-spans` annotations
/// from `CodeHighlightStage`) is preserved verbatim.
///
/// Phase 2 (Commit 1) note: Generate now emits a sideband entry for
/// *every* code block under the document-level copy default — even
/// blocks with no per-block decoration. Render must therefore filter
/// to the wrapping layers it actually understands today, which is
/// just the filename header. When `filename` is `None`, leave the
/// block alone; Phase 2 Commit 2 will refactor this into a single-
/// pass cumulative wrap and add the copy-scaffold layer.
fn wrap_in_place(block: &mut Block, decoration: &CodeBlockDecoration) {
    let Some(filename) = decoration.filename.as_ref() else {
        return;
    };

    // `Block` has no `Default`, so we need a real placeholder to swap
    // out the original. A RawBlock("html", "") with the same source
    // info has zero rendered output and zero cost; it's the cheapest
    // sentinel we can construct.
    let source_info = block.source_info().clone();
    let placeholder = Block::RawBlock(RawBlock {
        format: "html".to_string(),
        text: String::new(),
        source_info: source_info.clone(),
    });
    let original = std::mem::replace(block, placeholder);

    let wrapper_children: Vec<Block> = vec![
        make_filename_header(filename, source_info.clone()),
        original,
    ];

    *block = Block::Div(Div {
        attr: (
            String::new(),
            vec!["code-with-filename".to_string()],
            hashlink::LinkedHashMap::new(),
        ),
        content: wrapper_children,
        source_info,
        attr_source: AttrSourceInfo::empty(),
    });
}

/// Build the filename-header sub-block. Emitted as a `RawBlock("html", …)`
/// so the HTML output matches Q1's
/// `<div class="code-with-filename-file"><pre><strong>filename</strong></pre></div>`
/// byte-for-byte — the ported SCSS keys off that exact structure.
fn make_filename_header(filename: &str, source_info: quarto_source_map::SourceInfo) -> Block {
    // Filename is user-controlled, so escape it for HTML.
    let escaped = html_escape(filename);
    let text = format!(
        "<div class=\"code-with-filename-file\"><pre><strong>{}</strong></pre></div>",
        escaped
    );
    Block::RawBlock(RawBlock {
        format: "html".to_string(),
        text,
        source_info,
    })
}

/// Minimal HTML escape — enough for an attribute / element text
/// value. We deliberately don't pull in a heavyweight HTML library
/// for a string that will never contain anything more exotic than a
/// filename.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::project::{DocumentInfo, ProjectConfig, ProjectContext};
    use crate::render::BinaryDependencies;
    use quarto_pandoc_types::block::CodeBlock;
    use quarto_pandoc_types::{ConfigValue, attr::AttrSourceInfo};
    use quarto_source_map::SourceInfo;
    use quarto_source_map::types::FileId;

    fn source_info_at(file: usize, start: usize, end: usize) -> SourceInfo {
        SourceInfo::Original {
            file_id: FileId(file),
            start_offset: start,
            end_offset: end,
        }
    }

    fn make_codeblock(text: &str, kvs: Vec<(&str, &str)>) -> Block {
        let mut kv_map = hashlink::LinkedHashMap::new();
        for (k, v) in kvs {
            kv_map.insert(k.to_string(), v.to_string());
        }
        Block::CodeBlock(CodeBlock {
            attr: (String::new(), vec!["python".to_string()], kv_map),
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

    /// End-to-end shape test: run Generate then Render on a single
    /// code block with a filename and confirm the resulting AST is
    /// the wrapper Div containing a filename `RawBlock` followed by
    /// the original `CodeBlock`.
    #[tokio::test]
    async fn render_wraps_codeblock_with_filename_header() {
        use crate::transforms::CodeBlockGenerateTransform;

        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![make_codeblock(
                "print('hi')",
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
        CodeBlockRenderTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        // Top-level block is now the wrapper Div.
        assert_eq!(ast.blocks.len(), 1);
        let Block::Div(wrapper) = &ast.blocks[0] else {
            panic!("expected Block::Div wrapper; got {:?}", ast.blocks[0]);
        };
        assert!(
            wrapper.attr.1.contains(&"code-with-filename".to_string()),
            "wrapper must carry the code-with-filename class; got attrs {:?}",
            wrapper.attr
        );

        // Wrapper has two children: filename header + original code block.
        assert_eq!(wrapper.content.len(), 2);

        // First child is the filename header — a RawBlock with the
        // exact Q1 markup the ported SCSS expects.
        let Block::RawBlock(header) = &wrapper.content[0] else {
            panic!(
                "expected filename header as RawBlock; got {:?}",
                wrapper.content[0]
            );
        };
        assert_eq!(header.format, "html");
        assert_eq!(
            header.text,
            "<div class=\"code-with-filename-file\"><pre><strong>hello.py</strong></pre></div>"
        );

        // Second child is the original CodeBlock untouched (text and
        // attrs preserved).
        let Block::CodeBlock(cb) = &wrapper.content[1] else {
            panic!(
                "expected original CodeBlock as second child; got {:?}",
                wrapper.content[1]
            );
        };
        assert_eq!(cb.text, "print('hi')");
    }

    /// Code blocks without a filename decoration must NOT be wrapped.
    #[tokio::test]
    async fn render_leaves_undecorated_codeblocks_alone() {
        use crate::transforms::CodeBlockGenerateTransform;

        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![make_codeblock("print('hi')", vec![])],
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
        CodeBlockRenderTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        // Still a bare CodeBlock at the top level.
        assert_eq!(ast.blocks.len(), 1);
        assert!(
            matches!(ast.blocks[0], Block::CodeBlock(_)),
            "undecorated code block must not be wrapped; got {:?}",
            ast.blocks[0],
        );
    }

    /// Filename text must be HTML-escaped so user-controlled values
    /// can't inject markup. Defense in depth — `filename` comes from
    /// the user via a kv attribute on the CodeBlock, and the produced
    /// RawBlock passes through to the writer verbatim.
    #[tokio::test]
    async fn render_html_escapes_filename() {
        use crate::transforms::CodeBlockGenerateTransform;

        let mut ast = Pandoc {
            meta: ConfigValue::default(),
            blocks: vec![make_codeblock(
                "x",
                vec![("filename", "<script>alert(1)</script>")],
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
        CodeBlockRenderTransform::new()
            .transform(&mut ast, &mut ctx)
            .await
            .unwrap();

        let Block::Div(wrapper) = &ast.blocks[0] else {
            panic!("expected wrapper Div");
        };
        let Block::RawBlock(header) = &wrapper.content[0] else {
            panic!("expected RawBlock header");
        };
        assert!(
            !header.text.contains("<script>"),
            "raw <script> must be escaped; got:\n{}",
            header.text,
        );
        assert!(
            header.text.contains("&lt;script&gt;"),
            "expected escaped form; got:\n{}",
            header.text,
        );
    }
}
