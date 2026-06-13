//! Launcher argument classification and the self-documenting outputs
//! (bd-9a8yu2gw). `q2 mcp` is a verbatim passthrough to the embedded
//! TS server; these tests pin which args the *launcher* intercepts
//! before delegation (`--launcher-info`, `--print-config`, and the
//! `--help` preamble) versus what flows through untouched.

use quarto_mcp_launcher::{LauncherAction, classify_args, config_snippet, help_preamble};

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn launcher_info_only_as_sole_arg() {
    assert_eq!(
        classify_args(&argv(&["--launcher-info"])),
        LauncherAction::LauncherInfo
    );
    // Combined with anything else, it is no longer a launcher query —
    // it flows through (the server will reject the unknown flag).
    assert_eq!(
        classify_args(&argv(&["--launcher-info", "--server", "wss://x"])),
        LauncherAction::Delegate
    );
}

#[test]
fn print_config_only_as_sole_arg() {
    assert_eq!(
        classify_args(&argv(&["--print-config"])),
        LauncherAction::PrintConfig
    );
    assert_eq!(
        classify_args(&argv(&["--print-config", "--read-only"])),
        LauncherAction::Delegate
    );
}

#[test]
fn help_intercepted_anywhere() {
    for args in [
        argv(&["--help"]),
        argv(&["-h"]),
        argv(&["--server", "wss://x", "--help"]),
        argv(&["--read-only", "-h"]),
    ] {
        assert_eq!(
            classify_args(&args),
            LauncherAction::HelpPreambleThenDelegate,
            "expected help interception for {args:?}"
        );
    }
}

#[test]
fn ordinary_args_delegate() {
    assert_eq!(classify_args(&argv(&[])), LauncherAction::Delegate);
    assert_eq!(
        classify_args(&argv(&["--server", "wss://quarto-hub.com/ws"])),
        LauncherAction::Delegate
    );
    assert_eq!(
        classify_args(&argv(&["--read-only"])),
        LauncherAction::Delegate
    );
}

#[test]
fn help_wins_over_launcher_info() {
    // Both present: --launcher-info's sole-arg rule isn't met, and
    // --help is intercepted anywhere, so help wins.
    assert_eq!(
        classify_args(&argv(&["--launcher-info", "--help"])),
        LauncherAction::HelpPreambleThenDelegate
    );
}

#[test]
fn config_snippet_is_valid_launcher_json() {
    let snippet = config_snippet();
    let v: serde_json::Value =
        serde_json::from_str(&snippet).expect("config snippet must be valid JSON");
    let entry = &v["mcpServers"]["quarto-hub"];
    assert_eq!(entry["command"], "q2", "command must drive the q2 launcher");
    assert_eq!(
        entry["args"],
        serde_json::json!(["mcp"]),
        "args must be [\"mcp\"]"
    );
}

#[test]
fn help_preamble_names_launcher_controls() {
    let p = help_preamble();
    for needle in [
        "--launcher-info",
        "--print-config",
        "QUARTO_NODE",
        "QUARTO_MCP_CACHE_DIR",
    ] {
        assert!(
            p.contains(needle),
            "help preamble must mention {needle}: {p}"
        );
    }
}
