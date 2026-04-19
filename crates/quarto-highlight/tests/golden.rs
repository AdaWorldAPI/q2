//! Golden snapshot tests covering every built-in language plus one
//! fixture user grammar. The snapshots live under
//! `tests/snapshots/` and are updated via `cargo insta review` when
//! grammar / query output intentionally changes. Unreviewed changes
//! will fail CI, catching accidental drift in upstream grammar crates
//! or in our own encoding logic.
//!
//! The fixture inputs are deliberately tiny — enough shape to exercise
//! a handful of captures per grammar, not a full correctness test.

#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use quarto_highlight::{encoding, highlight, highlight_with_user};

/// Helper: one golden case.
struct Case {
    /// Snapshot name — drives the file under `tests/snapshots/`.
    name: &'static str,
    /// The language class as written after ```` ``` ````.
    class: &'static str,
    /// The source text to highlight.
    source: &'static str,
}

fn format_spans(spans: &[quarto_highlight::HighlightSpan]) -> String {
    // Pretty, review-friendly shape: one span per line, ordered by the
    // encoder. JSON keeps the form close to what's written to
    // `data-hl-spans` on the AST.
    serde_json::to_string_pretty(spans).expect("spans serialize")
}

fn check_builtin(case: &Case) {
    let encoded = highlight(case.class, case.source)
        .unwrap_or_else(|e| panic!("`{}` highlight errored: {e}", case.class))
        .unwrap_or_else(|| panic!("`{}` is not a registered class", case.class));
    let spans = encoding::decode(&encoded).expect("valid JSON");
    insta::with_settings!({ description => case.source, omit_expression => true }, {
        insta::assert_snapshot!(case.name, format_spans(&spans));
    });
}

#[test]
fn golden_python() {
    check_builtin(&Case {
        name: "python",
        class: "python",
        source: "def greet(name):\n    print(f\"hi, {name}\")\n",
    });
}

#[test]
fn golden_r() {
    check_builtin(&Case {
        name: "r",
        class: "r",
        source: "x <- c(1, 2, 3)\nmean(x)\n",
    });
}

#[test]
fn golden_javascript() {
    check_builtin(&Case {
        name: "javascript",
        class: "javascript",
        source: "const add = (a, b) => a + b;\n",
    });
}

#[test]
fn golden_jsx_aliases_javascript() {
    // The `jsx` class shares a grammar + config with `javascript`, so
    // the output should be identical to the plain-JS case — we lock
    // that in via the same snapshot name.
    check_builtin(&Case {
        name: "javascript_jsx_alias",
        class: "jsx",
        source: "const el = <div title=\"hi\">{name}</div>;\n",
    });
}

#[test]
fn golden_typescript() {
    check_builtin(&Case {
        name: "typescript",
        class: "typescript",
        source: "const add = (a: number, b: number): number => a + b;\n",
    });
}

#[test]
fn golden_tsx() {
    check_builtin(&Case {
        name: "tsx",
        class: "tsx",
        source: "const el: JSX.Element = <div>{x}</div>;\n",
    });
}

#[test]
fn golden_bash() {
    check_builtin(&Case {
        name: "bash",
        class: "bash",
        source: "greet() {\n  echo \"hello, $1\"\n}\n",
    });
}

#[test]
fn golden_sql() {
    check_builtin(&Case {
        name: "sql",
        class: "sql",
        source: "SELECT name FROM users WHERE id = 1;\n",
    });
}

#[test]
fn golden_html() {
    check_builtin(&Case {
        name: "html",
        class: "html",
        source: "<a href=\"/\">home</a>\n",
    });
}

#[test]
fn golden_css() {
    check_builtin(&Case {
        name: "css",
        class: "css",
        source: "p.intro { color: red; font-weight: bold; }\n",
    });
}

#[test]
fn golden_json() {
    check_builtin(&Case {
        name: "json",
        class: "json",
        source: "{\"n\": 1, \"s\": \"x\"}\n",
    });
}

#[test]
fn golden_yaml() {
    check_builtin(&Case {
        name: "yaml",
        class: "yaml",
        source: "title: \"hello\"\ncount: 1\n",
    });
}

#[test]
fn golden_julia() {
    check_builtin(&Case {
        name: "julia",
        class: "julia",
        source: "function greet(name)\n  println(\"hi, $name\")\nend\n",
    });
}

#[test]
fn golden_lua() {
    check_builtin(&Case {
        name: "lua",
        class: "lua",
        source: "local function greet(name)\n  print(\"hi, \" .. name)\nend\n",
    });
}

#[test]
fn golden_user_grammar_toml() {
    // A user-supplied grammar loaded dynamically from the
    // `user-grammar-toml` fixture. Snapshot tracks that dynamic loading
    // produces the same shape of output as static built-ins.
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/user-grammar-toml");
    let mut user = quarto_highlight::UserGrammars::new();
    user.load_from_directory(&fixture_dir)
        .expect("toml fixture should load");

    let source = "name = \"value\"\ncount = 42\n";
    let encoded = highlight_with_user("toml", source, Some(&mut user))
        .expect("toml highlight succeeds")
        .expect("toml resolves via user grammars");
    let spans = encoding::decode(&encoded).expect("valid JSON");

    insta::with_settings!({ description => source, omit_expression => true }, {
        insta::assert_snapshot!("user_grammar_toml", format_spans(&spans));
    });
}
