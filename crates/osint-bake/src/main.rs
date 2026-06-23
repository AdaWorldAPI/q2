//! Regenerate `crates/cockpit-server/assets/osint_scene.soa` from the on-disk
//! aiwar harvest, through the SAME pure bake cockpit-server uses
//! ([`osint_bake::osint_soa_bytes`]). This is the runnable twin of the
//! `#[ignore]`d `bake_osint_soa` test in `osint_gotham.rs`, but with a light
//! dependency closure (`aiwar-ingest` + zero-dep `lance-graph-contract`) so it
//! builds on a disk-constrained host where cockpit-server itself (lance /
//! datafusion / arrow / deno_core) cannot.
//!
//! Run from the workspace root:  `cargo run -p osint-bake`

use std::path::{Path, PathBuf};

fn main() {
    // Locate the harvest (base graph JSON; sibling `cypher/` enrichment is
    // applied when present — mirrors `bake_osint_soa`'s candidate list).
    let base_candidates = [
        "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
        "cockpit/public/aiwar_graph.json",
        "../../cockpit/public/aiwar_graph.json",
    ];
    let Some(path) = base_candidates.iter().find(|p| Path::new(p).exists()) else {
        eprintln!("harvest not found in {base_candidates:?}; cannot bake");
        std::process::exit(1);
    };
    let cypher_dir = Path::new(path)
        .parent()
        .and_then(|p| p.parent())
        .map(|r| r.join("cypher"));

    let graph = match &cypher_dir {
        Some(d) if d.is_dir() => aiwar_ingest::load_with_enrichment(path, d).expect("load+enrich"),
        _ => aiwar_ingest::load_from_file(path).expect("load base"),
    };
    let rounds = cypher_dir
        .as_ref()
        .and_then(|d| aiwar_ingest::encounter_round::load_encounter_rounds(d).ok())
        .unwrap_or_default();

    let bytes = osint_bake::osint_soa_bytes(&graph, &rounds);

    // The asset lives next to cockpit-server; resolve whether we're at the
    // workspace root or inside the crate.
    let out_candidates = [
        "crates/cockpit-server/assets/osint_scene.soa",
        "assets/osint_scene.soa",
    ];
    let out: PathBuf = out_candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.parent().is_some_and(Path::exists))
        .unwrap_or_else(|| PathBuf::from(out_candidates[0]));
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("mkdir assets");
    }
    std::fs::write(&out, &bytes).expect("write osint_scene.soa");

    let nodes = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let edges = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    eprintln!(
        "baked {} → {nodes} nodes, {edges} edges, {} bytes (harvest: {path})",
        out.display(),
        bytes.len()
    );
}
