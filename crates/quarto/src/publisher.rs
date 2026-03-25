//! Notebook publisher — renders notebooks to HTML and PDF
//!
//! This is the "Rust Quarto" publisher. For now it implements the minimum:
//! - HTML: single-file with embedded results, dark cockpit theme
//! - PDF: shell out to pandoc + wkhtmltopdf
//!
//! Pipeline: cells → HTML fragments → assembled document

use anyhow::Result;

use notebook_query::detect_language;
use crate::notebook_types::{render_table, CellOutput, Notebook};

use crate::commands::notebook::NotebookRenderArgs;

/// CSS for the cockpit dark theme
const COCKPIT_CSS: &str = r#"
:root {
    --bg-base: #0a0e17;
    --bg-surface: #0f1420;
    --bg-panel: #131927;
    --accent: #00bcd4;
    --accent-light: #4dd0e1;
    --text: #e8eaf6;
    --text-muted: #8892b0;
    --border: #1e2a3a;
    --green: #4caf50;
    --amber: #ffc107;
    --red: #ef5350;
    --mono: 'JetBrains Mono', 'Fira Code', Consolas, monospace;
    --sans: 'Inter', -apple-system, sans-serif;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    font-family: var(--sans);
    background: var(--bg-base);
    color: var(--text);
    line-height: 1.6;
    padding: 32px;
    max-width: 1200px;
    margin: 0 auto;
}
h1, h2, h3 { color: var(--text); margin: 1em 0 0.5em; }
h1 { font-size: 24px; border-bottom: 2px solid var(--accent); padding-bottom: 8px; }
.cell {
    border: 1px solid var(--border);
    border-radius: 8px;
    margin: 16px 0;
    overflow: hidden;
    background: var(--bg-surface);
}
.cell-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-muted);
}
.cell-lang {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--accent);
    background: rgba(0,188,212,0.12);
}
.cell-source {
    padding: 12px;
}
.cell-source pre {
    font-family: var(--mono);
    font-size: 13px;
    line-height: 1.5;
    color: var(--text);
    white-space: pre-wrap;
    overflow-x: auto;
}
.cell-output {
    border-top: 1px solid var(--border);
    padding: 12px;
    background: var(--bg-panel);
}
.cell-output-label {
    font-size: 10px;
    color: var(--text-muted);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
.markdown-cell {
    padding: 16px 20px;
    background: var(--bg-surface);
    border-left: 3px solid var(--accent);
    margin: 16px 0;
    border-radius: 4px;
}
.markdown-cell p { margin: 0.5em 0; }
table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
    font-family: var(--mono);
}
th {
    background: var(--bg-panel);
    padding: 8px;
    text-align: left;
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-bottom: 1px solid var(--border);
}
td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
}
.error { color: var(--red); }
.footer {
    margin-top: 40px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-muted);
    text-align: center;
}
details summary {
    cursor: pointer;
    color: var(--accent);
    font-size: 11px;
}
@media print {
    body { background: white; color: #1a1a1a; }
    .cell { border-color: #ddd; }
    .cell-header { background: #f5f5f5; }
    .cell-source pre { color: #1a1a1a; }
    .cell-output { background: #fafafa; }
}
"#;

/// Render a notebook to a self-contained HTML string.
pub fn render_notebook_to_html(notebook: &Notebook) -> String {
    let title = notebook
        .metadata
        .title
        .as_deref()
        .unwrap_or("q2 Notebook");

    let mut body = String::new();

    body.push_str(&format!("<h1>{}</h1>\n", html_escape(title)));

    if !notebook.metadata.authors.is_empty() {
        body.push_str("<p style=\"color:var(--text-muted);font-size:13px;\">");
        body.push_str(&notebook.metadata.authors.join(", "));
        body.push_str("</p>\n");
    }

    for (i, cell) in notebook.cells.iter().enumerate() {
        let lang = cell
            .language
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let detected = detect_language(&cell.source);
                format!("{:?}", detected).to_lowercase()
            });

        if lang == "markdown" {
            // Render as markdown prose
            body.push_str("<div class=\"markdown-cell\">\n");
            body.push_str(&simple_markdown_to_html(&cell.source));
            body.push_str("</div>\n");
        } else {
            // Render as code cell
            body.push_str("<div class=\"cell\">\n");
            body.push_str("  <div class=\"cell-header\">\n");
            body.push_str(&format!(
                "    <span class=\"cell-lang\">{}</span>\n",
                html_escape(&lang)
            ));
            body.push_str(&format!(
                "    <span>Cell [{}]</span>\n",
                i + 1
            ));
            body.push_str("  </div>\n");

            // Source code (collapsible)
            body.push_str("  <div class=\"cell-source\">\n");
            body.push_str("    <details open>\n");
            body.push_str("      <summary>source</summary>\n");
            body.push_str(&format!(
                "      <pre><code>{}</code></pre>\n",
                html_escape(&cell.source)
            ));
            body.push_str("    </details>\n");
            body.push_str("  </div>\n");

            // Outputs
            if !cell.outputs.is_empty() {
                body.push_str("  <div class=\"cell-output\">\n");
                body.push_str("    <div class=\"cell-output-label\">output</div>\n");

                for output in &cell.outputs {
                    match output {
                        CellOutput::Html(h) => {
                            body.push_str("    ");
                            body.push_str(h);
                            body.push('\n');
                        }
                        CellOutput::Text(t) => {
                            body.push_str(&format!(
                                "    <pre>{}</pre>\n",
                                html_escape(t)
                            ));
                        }
                        CellOutput::Error(e) => {
                            body.push_str(&format!(
                                "    <pre class=\"error\">{}</pre>\n",
                                html_escape(e)
                            ));
                        }
                        CellOutput::Table { headers, rows } => {
                            body.push_str("    ");
                            body.push_str(&render_table(headers, rows));
                            body.push('\n');
                        }
                        CellOutput::Graph { html } => {
                            body.push_str("    ");
                            body.push_str(html);
                            body.push('\n');
                        }
                    }
                }

                body.push_str("  </div>\n");
            }

            body.push_str("</div>\n");
        }
    }

    body.push_str("<div class=\"footer\">Generated by q2 graph notebook</div>\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>{css}</style>
</head>
<body>
{body}
</body>
</html>"#,
        title = html_escape(title),
        css = COCKPIT_CSS,
        body = body,
    )
}

/// CLI entry point: `q2 notebook render <input> --format <html|pdf>`
pub fn render(args: NotebookRenderArgs) -> Result<()> {
    let input = &args.input;
    let content = std::fs::read_to_string(input)?;

    // Parse a simple notebook format (JSON with cells array)
    let doc: serde_json::Value = serde_json::from_str(&content).map_err(|_| {
        // If not JSON, treat the whole file as a single markdown cell
        anyhow::anyhow!("Input must be a JSON notebook file (.nb) or a .qmd file")
    })?;

    let mut notebook = Notebook::default();

    if let Some(title) = doc.get("title").and_then(|v| v.as_str()) {
        notebook.metadata.title = Some(title.to_string());
    }

    if let Some(cells) = doc.get("cells").and_then(|v| v.as_array()) {
        for cell_val in cells {
            let cell = crate::notebook_types::Cell {
                id: cell_val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                source: cell_val
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                language: cell_val.get("language").and_then(|v| v.as_str()).map(|s| s.to_string()),
                outputs: vec![],
                execution_state: crate::notebook_types::ExecutionState::Idle,
            };
            notebook.cells.push(cell);
        }
    }

    let html = render_notebook_to_html(&notebook);

    match args.format.as_str() {
        "html" => {
            let output = args
                .output
                .unwrap_or_else(|| input.replace(".nb", ".html").replace(".qmd", ".html"));
            std::fs::write(&output, &html)?;
            println!("Rendered HTML: {}", output);
        }
        "pdf" => {
            let output = args
                .output
                .unwrap_or_else(|| input.replace(".nb", ".pdf").replace(".qmd", ".pdf"));
            let tmp_html = format!("{}.tmp.html", output);
            std::fs::write(&tmp_html, &html)?;

            let result = std::process::Command::new("pandoc")
                .args([&tmp_html, "-o", &output, "--pdf-engine=wkhtmltopdf"])
                .status();

            let _ = std::fs::remove_file(&tmp_html);

            match result {
                Ok(status) if status.success() => {
                    println!("Rendered PDF: {}", output);
                }
                Ok(status) => {
                    anyhow::bail!("pandoc exited with status {}", status);
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to run pandoc: {}. Install pandoc and wkhtmltopdf for PDF export.",
                        e
                    );
                }
            }
        }
        other => {
            anyhow::bail!("Unsupported format: {}. Use 'html' or 'pdf'.", other);
        }
    }

    Ok(())
}

// ---- Utility functions ----

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Very simple markdown to HTML (headers, paragraphs, bold, italic, code).
/// For real use, this would be replaced by a proper markdown parser.
fn simple_markdown_to_html(md: &str) -> String {
    let mut html = String::new();
    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            html.push_str("<br>\n");
        } else if let Some(rest) = trimmed.strip_prefix("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", html_escape(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            html.push_str(&format!("<h2>{}</h2>\n", html_escape(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            html.push_str(&format!("<h1>{}</h1>\n", html_escape(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            html.push_str(&format!("<li>{}</li>\n", html_escape(rest)));
        } else {
            html.push_str(&format!("<p>{}</p>\n", html_escape(trimmed)));
        }
    }
    html
}
