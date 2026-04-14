//! Build script that locates the trace-viewer SPA bundle at compile time.
//!
//! The `include_dir!` macro needs a concrete compile-time path. We resolve
//! it here:
//!
//! 1. If `trace-viewer/dist/index.html` exists, embed that directory.
//! 2. Otherwise, write a placeholder `index.html` into the crate's `OUT_DIR`
//!    and embed that, so the build still succeeds. The placeholder tells
//!    the user to run `cargo xtask build-trace-viewer`.
//!
//! The chosen path is exposed to the crate as `QUARTO_TRACE_VIEWER_EMBED_DIR`
//! via `cargo:rustc-env`, and `src/lib.rs` consumes it via
//! `include_dir!("$QUARTO_TRACE_VIEWER_EMBED_DIR")`.

use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.join("..").join("..");
    let real_dist = workspace_root.join("trace-viewer").join("dist");

    let embed_dir = if real_dist.join("index.html").is_file() {
        real_dist.clone()
    } else {
        make_placeholder_dist()
    };

    // include_dir! consumes `$VAR` substitutions at macro-expansion time.
    println!(
        "cargo:rustc-env=QUARTO_TRACE_VIEWER_EMBED_DIR={}",
        embed_dir.display()
    );

    // Re-run if the real dist/ tree changes.
    println!("cargo:rerun-if-changed={}", real_dist.display());
}

fn make_placeholder_dist() -> PathBuf {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dist = out_dir.join("placeholder-dist");
    std::fs::create_dir_all(&dist).expect("create placeholder dist dir");

    let index = dist.join("index.html");
    let html = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8"/>
    <title>Quarto trace viewer — not built</title>
    <style>
      body { font-family: -apple-system, Segoe UI, sans-serif; max-width: 640px; margin: 40px auto; color: #222; }
      code, pre { background: #f4f4f7; padding: 2px 6px; border-radius: 4px; }
      pre { padding: 10px; overflow: auto; }
      h1 { font-size: 18px; }
    </style>
  </head>
  <body>
    <h1>Trace viewer SPA is not built</h1>
    <p>
      The embedded SPA bundle is a placeholder. Build the real UI and rebuild
      the <code>quarto</code> binary:
    </p>
    <pre>cargo xtask build-trace-viewer
cargo build -p quarto</pre>
    <p>
      For iterative UI work you can also run the Vite dev server:
    </p>
    <pre>cd trace-viewer && npm run dev</pre>
  </body>
</html>
"#;
    write_if_changed(&index, html);

    emit_warning(
        "trace-viewer/dist/index.html not found; embedding placeholder. \
         Run `cargo xtask build-trace-viewer` and rebuild to embed the real SPA.",
    );

    dist
}

fn write_if_changed(path: &Path, contents: &str) {
    let existing = std::fs::read_to_string(path).ok();
    if existing.as_deref() != Some(contents) {
        std::fs::write(path, contents).expect("write placeholder html");
    }
}

fn emit_warning(msg: &str) {
    // Cargo picks up `cargo:warning=...` and surfaces it to the user.
    println!("cargo:warning={}", msg);
}
