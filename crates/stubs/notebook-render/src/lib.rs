// TODO: replace when crate is transcoded
//! d3/vis.js/tables to HTML rendering stubs.

/// Render graph data as an HTML string (vis.js network visualization).
pub fn render_graph(data: &str) -> String {
    // TODO: implement real vis.js/d3 graph rendering
    format!(
        r#"<div class="graph-container"><pre>Graph: {}</pre></div>"#,
        data
    )
}

/// Render a table as an HTML string.
pub fn render_table(headers: &[String], rows: &[Vec<String>]) -> String {
    // TODO: implement real table rendering
    let mut html = String::from("<table>\n<thead><tr>");
    for h in headers {
        html.push_str(&format!("<th>{}</th>", h));
    }
    html.push_str("</tr></thead>\n<tbody>\n");
    for row in rows {
        html.push_str("<tr>");
        for cell in row {
            html.push_str(&format!("<td>{}</td>", cell));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody>\n</table>");
    html
}

/// Render chart data as an HTML string (d3 visualization).
pub fn render_chart(data: &str) -> String {
    // TODO: implement real d3 chart rendering
    format!(
        r#"<div class="chart-container"><pre>Chart: {}</pre></div>"#,
        data
    )
}

/// Render a scalar value as an HTML string.
pub fn render_scalar(value: &str) -> String {
    format!(r#"<span class="scalar-value">{}</span>"#, value)
}
