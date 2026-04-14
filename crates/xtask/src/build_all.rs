//! Build-all command - fresh-clone build orchestration.
//!
//! Runs the full fresh-build sequence in dependency order, serving as the
//! source of truth for what CI (and a developer on a fresh clone) needs to do
//! to produce a working build:
//!
//! 1. `npm install` at the repo root (npm workspaces)
//! 2. Build hub-client (includes WASM via `npm run build:all`)
//! 3. Build the Rust workspace (`cargo build --workspace`)
//!
//! Phase 4.3 extends this to also build `trace-viewer/` before the final
//! Rust build, since the `quarto-trace-server` crate embeds its `dist/` via
//! `include_dir!`.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Configuration for the build-all command.
pub struct BuildAllConfig {
    /// Skip `npm install`. Useful when running in a loop where dependencies
    /// haven't changed.
    pub skip_npm_install: bool,
    /// Skip the hub-client build step.
    pub skip_hub_build: bool,
    /// Skip the trace-viewer build step. No-op until Phase 4.3 lands.
    pub skip_trace_viewer_build: bool,
    /// Skip the final `cargo build --workspace` step.
    pub skip_rust_build: bool,
}

impl Default for BuildAllConfig {
    fn default() -> Self {
        Self {
            skip_npm_install: false,
            skip_hub_build: false,
            skip_trace_viewer_build: false,
            skip_rust_build: false,
        }
    }
}

/// Run the build-all command.
pub fn run(config: &BuildAllConfig) -> Result<()> {
    let project_root = find_project_root()?;

    let steps: Vec<(&str, bool)> = vec![
        ("npm install (root workspaces)", !config.skip_npm_install),
        ("hub-client build (WASM + TS)", !config.skip_hub_build),
        (
            "trace-viewer build",
            !config.skip_trace_viewer_build && trace_viewer_exists(&project_root),
        ),
        ("Rust workspace build", !config.skip_rust_build),
    ];

    let enabled_count = steps.iter().filter(|(_, enabled)| *enabled).count();
    let total = enabled_count as u32;
    let mut step_idx: u32 = 0;

    // Step: npm install
    if !config.skip_npm_install {
        step_idx += 1;
        banner(step_idx, total, "Installing npm workspace dependencies");
        run_command(
            "npm",
            &["install"],
            &project_root,
            None,
            "npm install failed",
        )?;
        println!("✓ npm install complete");
    }

    // Step: hub-client build (WASM + TS)
    if !config.skip_hub_build {
        step_idx += 1;
        banner(step_idx, total, "Building hub-client (WASM + TS)");
        let hub_client_dir = project_root.join("hub-client");
        run_command(
            "npm",
            &["run", "build:all"],
            &hub_client_dir,
            None,
            "hub-client build failed",
        )?;
        println!("✓ hub-client build complete");
    }

    // Step: trace-viewer build (Phase 4.3+)
    if !config.skip_trace_viewer_build && trace_viewer_exists(&project_root) {
        step_idx += 1;
        banner(step_idx, total, "Building trace-viewer");
        let trace_viewer_dir = project_root.join("trace-viewer");
        run_command(
            "npm",
            &["run", "build"],
            &trace_viewer_dir,
            None,
            "trace-viewer build failed",
        )?;
        println!("✓ trace-viewer build complete");
    }

    // Step: Rust workspace build
    if !config.skip_rust_build {
        step_idx += 1;
        banner(step_idx, total, "Building Rust workspace");
        run_command(
            "cargo",
            &["build", "--workspace"],
            &project_root,
            None,
            "Rust build failed",
        )?;
        println!("✓ Rust build complete");
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✓ Fresh build complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    Ok(())
}

fn banner(step: u32, total: u32, label: &str) {
    println!("\n━━━ Step {}/{}: {} ━━━\n", step, total, label);
}

fn trace_viewer_exists(project_root: &Path) -> bool {
    project_root
        .join("trace-viewer")
        .join("package.json")
        .is_file()
}

/// Find the project root directory (where Cargo.toml with [workspace] lives).
fn find_project_root() -> Result<std::path::PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current directory")?;

    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content =
                std::fs::read_to_string(&cargo_toml).context("Failed to read Cargo.toml")?;
            if content.contains("[workspace]") {
                return Ok(dir);
            }
        }

        if !dir.pop() {
            bail!("Could not find workspace root (Cargo.toml with [workspace])");
        }
    }
}

fn run_command(
    program: &str,
    args: &[&str],
    dir: &std::path::Path,
    rustflags: Option<&str>,
    error_msg: &str,
) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(dir);

    if let Some(flags) = rustflags {
        cmd.env("RUSTFLAGS", flags);
    }

    let status = cmd
        .status()
        .with_context(|| format!("Failed to run {} {:?}", program, args))?;

    if !status.success() {
        bail!("{}", error_msg);
    }

    Ok(())
}
