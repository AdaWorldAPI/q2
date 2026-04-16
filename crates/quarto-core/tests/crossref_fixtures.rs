//! End-to-end fixture tests for the crossref pipeline.
//!
//! Each test parses a small qmd fragment, runs the normalization +
//! crossref phases of the transform pipeline, and asserts over the
//! resulting `CrossrefIndex` as structured data. Per plan success
//! criterion #4, we validate over the index rather than rendered HTML so
//! tests stay insensitive to HTML formatting churn.
//!
//! These fixtures cover the full Phase 1 surface: all four authoring
//! shapes (Div, Figure, Div>Figure, Div>Table), code-block shorthand,
//! duplicate ids, unresolved refs, `@`-disambiguation between crossref
//! and citation prefixes, and custom ref-types via `crossref.custom`.

use quarto_core::crossref::{CrossrefEntry, CrossrefIndex, RefTypeRegistry, metadata};
use quarto_core::transform::AstTransform;
use quarto_core::transforms::{
    CalloutTransform, CrossrefIndexTransform, CrossrefResolveTransform,
    FloatRefTargetSugarTransform, ProofSugarTransform, TheoremSugarTransform,
};
use quarto_pandoc_types::pandoc::Pandoc;

/// Parse a qmd snippet and run the crossref-relevant part of the
/// transform pipeline on it. Returns (ast, index, diagnostics).
///
/// The pre-engine stage's *logic* (metadata extraction + code-block
/// shorthand desugar) is applied inline here so tests don't need to
/// spin up a full StageContext.
async fn run_crossref(
    qmd: &str,
) -> (
    Pandoc,
    CrossrefIndex,
    Vec<quarto_error_reporting::DiagnosticMessage>,
) {
    // Parse qmd -> AST.
    let (mut ast, _ast_ctx, _warnings) = pampa::readers::qmd::read(
        qmd.as_bytes(),
        false,
        "<fixture>",
        &mut std::io::sink(),
        true,
        None,
    )
    .expect("qmd parse");

    // Step 1: build registry from metadata.
    let mut registry = RefTypeRegistry::builtin();
    let extracted = metadata::read(&ast.meta, &mut registry);
    registry.extend_from_promised(&extracted.promised_ids);
    // metadata extraction errors turn into diagnostics downstream — we
    // don't surface them here because the fixtures under test have valid
    // metadata.

    // Step 2: code-block shorthand desugar.
    quarto_core::crossref::codeblock_shorthand::desugar_blocks(&mut ast.blocks, &registry);

    // Step 3: front-end transforms. We build a minimal RenderContext for
    // the async transform API.
    use quarto_core::format::Format;
    use quarto_core::project::{DocumentInfo, ProjectConfig, ProjectContext};
    use quarto_core::render::{BinaryDependencies, RenderContext};
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
    ctx.ref_type_registry = Some(registry);
    ctx.crossref_index = Some({
        let mut idx = CrossrefIndex::new(quarto_source_map::FileId(0));
        idx.promised_ids = extracted.promised_ids;
        idx
    });

    // Normalization phase: callout → theorem → proof → float.
    // Mirrors the pipeline order in build_transform_pipeline.
    CalloutTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("callout");
    TheoremSugarTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("theorem");
    ProofSugarTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("proof");
    FloatRefTargetSugarTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("float sugar");
    CrossrefIndexTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("index");
    CrossrefResolveTransform::new()
        .transform(&mut ast, &mut ctx)
        .await
        .expect("resolve");

    (ast, ctx.crossref_index.unwrap(), ctx.diagnostics)
}

fn entry_summary(e: &CrossrefEntry) -> (String, String, Vec<u32>, u32) {
    (
        e.identifier.clone(),
        e.ref_type.clone(),
        e.order.section.clone(),
        e.order.order,
    )
}

#[tokio::test]
async fn fixture_div_figure_target() {
    let qmd = r#"---
title: t
---

# Intro

::: {#fig-alpha}
![hi](x.png)

A caption.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {diags:?}");
    assert_eq!(idx.entries.len(), 1);
    let summary = entry_summary(idx.get("fig-alpha").unwrap());
    assert_eq!(summary, ("fig-alpha".into(), "fig".into(), vec![1], 1));
}

#[tokio::test]
async fn fixture_figure_markdown_target() {
    // `![caption](img){#fig-..}` — Pandoc native Figure with an id.
    let qmd = r#"---
title: t
---

![A plot](x.png){#fig-mplot}
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {diags:?}");
    let summary = entry_summary(idx.get("fig-mplot").unwrap());
    assert_eq!(summary, ("fig-mplot".into(), "fig".into(), vec![], 1));
}

#[tokio::test]
async fn fixture_table_target() {
    let qmd = r#"---
title: t
---

::: {#tbl-stats}
| a | b |
|---|---|
| 1 | 2 |

Table caption.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {diags:?}");
    let summary = entry_summary(idx.get("tbl-stats").unwrap());
    assert_eq!(summary, ("tbl-stats".into(), "tbl".into(), vec![], 1));
}

#[tokio::test]
async fn fixture_counts_per_ref_type() {
    let qmd = r#"---
title: t
---

::: {#fig-a}
![](1.png)

A.
:::

::: {#fig-b}
![](2.png)

B.
:::

::: {#tbl-c}
|x|
|-|
|1|

C.
:::
"#;
    let (_, idx, _) = run_crossref(qmd).await;
    assert_eq!(idx.get("fig-a").unwrap().order.order, 1);
    assert_eq!(idx.get("fig-b").unwrap().order.order, 2);
    assert_eq!(idx.get("tbl-c").unwrap().order.order, 1);
}

#[tokio::test]
async fn fixture_section_path_included() {
    let qmd = r#"---
title: t
---

# Chapter

## Subsection

::: {#fig-deep}
![](x.png)

Deep.
:::
"#;
    let (_, idx, _) = run_crossref(qmd).await;
    let e = idx.get("fig-deep").unwrap();
    assert_eq!(e.order.section, vec![1, 1]);
}

#[tokio::test]
async fn fixture_non_crossref_div_left_alone() {
    let qmd = r#"---
title: t
---

::: {#just-a-section}
Some content.
:::

::: {#fig-real}
![](x.png)

Real fig.
:::
"#;
    let (_, idx, _) = run_crossref(qmd).await;
    // `just-a-section` isn't a crossref — not indexed.
    assert!(idx.get("just-a-section").is_none());
    assert_eq!(idx.entries.len(), 1);
}

#[tokio::test]
async fn fixture_duplicate_id_diagnostic() {
    let qmd = r#"---
title: t
---

::: {#fig-dup}
![](1.png)

first.
:::

::: {#fig-dup}
![](2.png)

second.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert_eq!(idx.entries.len(), 1);
    assert_eq!(diags.len(), 1);
    let msg = format!("{:?}", diags[0]);
    assert!(msg.contains("fig-dup"), "msg: {msg}");
}

#[tokio::test]
async fn fixture_unresolved_ref_diagnostic() {
    let qmd = r#"---
title: t
---

See @fig-missing.
"#;
    let (_, _idx, diags) = run_crossref(qmd).await;
    assert_eq!(diags.len(), 1);
    let msg = format!("{:?}", diags[0]);
    assert!(msg.contains("fig-missing"), "msg: {msg}");
}

#[tokio::test]
async fn fixture_disambiguates_crossref_from_citation() {
    // `@fig-foo` is a crossref, `@smith2020` is a citation; neither
    // `@mycustomfoo2020` nor `@smith-2020` is a crossref because their
    // prefixes aren't registered. We expect one diagnostic: the
    // unresolved `fig-foo` ref (we don't define `fig-foo` in the doc).
    let qmd = r#"---
title: t
---

See @fig-foo and read @smith2020, also @mycustomfoo2020 and @smith-2020.
"#;
    let (_, _idx, diags) = run_crossref(qmd).await;
    // Exactly one diagnostic — the unresolved fig-foo crossref.
    assert_eq!(
        diags.len(),
        1,
        "expected 1 diagnostic for unresolved fig-foo, got {:?}",
        diags
    );
    let msg = format!("{:?}", diags[0]);
    assert!(msg.contains("fig-foo"));
}

#[tokio::test]
async fn fixture_custom_ref_type_via_metadata() {
    let qmd = r#"---
title: t
crossref:
  custom:
    - key: dia
      reference-prefix: Diagram
---

::: {#dia-one}
![](x.png)

A diagram.
:::

See @dia-one.
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(
        diags.is_empty(),
        "unexpected diagnostics for custom type: {:?}",
        diags
    );
    let summary = entry_summary(idx.get("dia-one").unwrap());
    assert_eq!(summary, ("dia-one".into(), "dia".into(), vec![], 1));
}

#[tokio::test]
async fn fixture_code_block_shorthand_end_to_end() {
    let qmd = r#"---
title: t
---

See @fig-plot.

```{python}
#| label: fig-plot
#| fig-cap: A plot.
print("x")
```
"#;
    let (ast, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);

    // Index has the fig-plot entry.
    let entry = idx.get("fig-plot").expect("fig-plot indexed");
    assert_eq!(entry.order.order, 1);

    // The AST should have a FloatRefTarget custom node; look for it.
    let target = find_first_float_ref_target(&ast.blocks);
    assert!(target.is_some(), "FloatRefTarget not present in AST");
}

fn find_first_float_ref_target(
    blocks: &[quarto_pandoc_types::block::Block],
) -> Option<&quarto_pandoc_types::custom::CustomNode> {
    use quarto_pandoc_types::block::Block;
    for b in blocks {
        if let Block::Custom(node) = b {
            if node.type_name == quarto_core::crossref::FLOAT_REF_TARGET {
                return Some(node);
            }
        }
    }
    None
}

// === Phase 2 fixtures: theorems, proofs ===

#[tokio::test]
async fn fixture_theorem_indexed_and_resolved() {
    let qmd = r#"---
title: t
---

See @thm-foo.

::: {#thm-foo .theorem name="Test"}
A theorem body.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    let entry = idx.get("thm-foo").expect("thm-foo indexed");
    assert_eq!(entry.ref_type, "thm");
    assert_eq!(entry.order.order, 1);
}

#[tokio::test]
async fn fixture_theorem_and_lemma_counted_separately() {
    let qmd = r#"---
title: t
---

::: {#thm-a .theorem}
A.
:::

::: {#thm-b .theorem}
B.
:::

::: {#lem-c .lemma}
C.
:::
"#;
    let (_, idx, _) = run_crossref(qmd).await;
    assert_eq!(idx.get("thm-a").unwrap().order.order, 1);
    assert_eq!(idx.get("thm-b").unwrap().order.order, 2);
    assert_eq!(idx.get("lem-c").unwrap().order.order, 1);
}

#[tokio::test]
async fn fixture_theorem_section_path() {
    let qmd = r#"---
title: t
---

# Results

::: {#thm-deep .theorem}
Nested.
:::
"#;
    let (_, idx, _) = run_crossref(qmd).await;
    assert_eq!(idx.get("thm-deep").unwrap().order.section, vec![1]);
}

#[tokio::test]
async fn fixture_proof_not_indexed() {
    let qmd = r#"---
title: t
---

::: {.proof}
QED.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty());
    assert!(idx.entries.is_empty());
}

#[tokio::test]
async fn fixture_theorem_and_figure_coexist() {
    let qmd = r#"---
title: t
---

::: {#thm-one .theorem}
A theorem.
:::

::: {#fig-one}
![](x.png)

A figure.
:::

See @thm-one and @fig-one.
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    assert_eq!(idx.get("thm-one").unwrap().ref_type, "thm");
    assert_eq!(idx.get("fig-one").unwrap().ref_type, "fig");
    // Both numbered independently: Theorem 1, Figure 1.
    assert_eq!(idx.get("thm-one").unwrap().order.order, 1);
    assert_eq!(idx.get("fig-one").unwrap().order.order, 1);
}

// === Phase 2.2 fixtures: callout crossref indexing ===

#[tokio::test]
async fn fixture_callout_with_crossref_id_indexed() {
    let qmd = r#"---
title: t
---

See @nte-important.

::: {#nte-important .callout-note}
## Pay attention

This is a very important note.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    let entry = idx.get("nte-important").expect("nte-important indexed");
    assert_eq!(entry.ref_type, "nte");
    assert_eq!(entry.order.order, 1);
}

#[tokio::test]
async fn fixture_callout_without_crossref_id_not_indexed() {
    let qmd = r#"---
title: t
---

::: {.callout-warning}
Watch out!
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty());
    assert!(idx.entries.is_empty());
}

#[tokio::test]
async fn fixture_callout_with_non_crossref_id_not_indexed() {
    let qmd = r#"---
title: t
---

::: {#my-callout .callout-tip}
A tip.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty());
    // "my" is not a registered ref-type prefix, so not indexed.
    assert!(idx.entries.is_empty());
}

#[tokio::test]
async fn fixture_multiple_callout_types_numbered_separately() {
    let qmd = r#"---
title: t
---

::: {#nte-a .callout-note}
Note A.
:::

::: {#nte-b .callout-note}
Note B.
:::

::: {#wrn-a .callout-warning}
Warning A.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    assert_eq!(idx.get("nte-a").unwrap().order.order, 1);
    assert_eq!(idx.get("nte-b").unwrap().order.order, 2);
    assert_eq!(idx.get("wrn-a").unwrap().order.order, 1);
}

#[tokio::test]
async fn fixture_callout_ref_resolves_to_link() {
    let qmd = r#"---
title: t
---

See @nte-foo.

::: {#nte-foo .callout-note}
A note.
:::
"#;
    let (_, idx, diags) = run_crossref(qmd).await;
    assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    let entry = idx.get("nte-foo").expect("nte-foo indexed");
    assert_eq!(entry.ref_type, "nte");
}
