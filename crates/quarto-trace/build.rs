//! Build script that exposes `QUARTO_GIT_HASH` as a compile-time env var.
//!
//! The hash format is `<short-hash>` or `<short-hash>-dirty` if the working
//! tree has uncommitted changes. When `git` is not available or `.git` is
//! missing (e.g. tarball builds via `cargo package`), the hash is `unknown`.

use std::process::Command;

fn main() {
    let hash = git_short_hash().unwrap_or_else(|| "unknown".to_string());
    let dirty = git_is_dirty().unwrap_or(false);
    let value = if hash == "unknown" {
        hash
    } else if dirty {
        format!("{}-dirty", hash)
    } else {
        hash
    };
    println!("cargo:rustc-env=QUARTO_GIT_HASH={}", value);

    // Re-run when HEAD or the index changes so the hash stays fresh.
    // These paths are relative to the crate root; missing .git is handled gracefully.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}

fn git_short_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn git_is_dirty() -> Option<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(!output.stdout.is_empty())
}
