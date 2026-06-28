//! Full-body FMA bake — fuses the full-resolution polygon geometry with a
//! `CLASSID_FMA_V3` NodeGuid substrate into `cockpit/public/body.soa`.
//!
//! This is the operator-directed successor to the decimated `torso.mesh` + flat
//! `guid=(container<<16)|identity` torso: ALL points (the 4.2 M-vertex / 6.7 M-
//! triangle BodyParts3D is_a surface, no cell_mm decimation), addressed on the V3
//! **(part_of:is_a) cascade** — the SAME `mint_for(classid_read_mode(c).tail_variant,
//! …)` path `bin/fma.rs` mints the heart slice on, but over the whole body.
//!
//! Two-stage bake (geometry in Python, V3 minting in Rust — the minting MUST live
//! here because NodeGuid is `lance-graph-contract`):
//!   1. `tools/bake_body_v3.py` → `body.spm1` (full-res SPM1 geometry) +
//!      `body.nodes.json` (per-concept is_a cascade sibling-rank chain + tissue/rgb/
//!      geometry range). No decimation: every OBJ vertex survives.
//!   2. THIS bin reads both, mints one V3 NodeGuid per concept (cascade tier
//!      `[mixin-by-depth : sibling-rank]`, identity = row), and emits `body.soa`.
//!
//! body.soa wire (BSO1, little-endian) — geometry block is byte-identical SPM1 so the
//! cockpit reuses its SPM1 decoder; the node table is the V3 substrate node_row → key:
//! ```text
//!   header 18 B: magic "BSO1" | version u16 | node_count u32 | nodes_len u32 | spm1_len u32
//!   node table  nodes_len B:  node_count × [ key 16 | tissue u8 | depth u8 | rgb 3u8
//!                                            | v_start u32 | v_count u32 | label_len u8 | utf8 ]
//!   geometry    spm1_len  B:  the SPM1 block verbatim (vert node_row indexes the table)
//! ```
//!
//! Run from the workspace root (after the Python geometry bake):
//!   `cargo run -p osint-bake --bin body`
//! Default paths: scratch-fma/out/{body.nodes.json,body.spm1} → cockpit/public/body.soa.
//!
//! The output is BIG (~168 MB) and is NOT committed to git. It lives as a GitHub
//! **release asset** (q2 release `fma-body-soa-v3-v1`, both `body.soa` and the
//! gzipped `body.soa.gz`); the cockpit `/body` view fetches the .gz from there and
//! inflates it client-side. `cockpit/public/body.soa*` is gitignored so a local
//! bake never lands the binary in the repo.

use lance_graph_contract::canonical_node::{classid_read_mode, NodeGuid};
use std::path::{Path, PathBuf};

/// FMA V3 cascade key (`0x1000_0A01`) — same constant `bin/fma.rs` uses; the V3
/// generation marker `0x1000` over the canon `anatomical_structure` concept `0x0A01`.
const CLASSID_FMA: u32 = NodeGuid::CLASSID_FMA_V3;

/// One 8:8 HHTL tier: `[container-mixin : identity]` (mirrors `fma.rs::tier`).
const fn tier(container: u8, identity: u8) -> u16 {
    ((container as u16) << 8) | identity as u16
}

/// Kind-mixin for is_a depth `k` (0..4) — the family node each cascade level attaches
/// on. Generic depth mixins (0x01..0x05) play the role `fma.rs`'s Organ/Chamber/Wall/
/// Tissue/Cell mixins do for the heart; the body's is_a tree is not chamber-shaped, so
/// depth indexes the mixin directly. Deeper-than-5 levels fold into `identity`.
const fn mixin_for_depth(k: usize) -> u8 {
    (k as u8) + 1
}

fn arg(n: usize, default: &str) -> String {
    std::env::args().nth(n).unwrap_or_else(|| default.to_string())
}

fn main() {
    let nodes_path = arg(1, "scratch-fma/out/body.nodes.json");
    let spm1_path = arg(2, "scratch-fma/out/body.spm1");
    let out_arg = arg(3, "");

    let nodes_json = std::fs::read_to_string(&nodes_path)
        .unwrap_or_else(|e| panic!("read {nodes_path}: {e}"));
    let doc: serde_json::Value =
        serde_json::from_str(&nodes_json).unwrap_or_else(|e| panic!("parse {nodes_path}: {e}"));
    let nodes = doc["nodes"].as_array().expect("body.nodes.json: .nodes array");

    let spm1 = std::fs::read(&spm1_path).unwrap_or_else(|e| panic!("read {spm1_path}: {e}"));
    assert_eq!(&spm1[..4], b"SPM1", "geometry block is not SPM1");

    let tail = classid_read_mode(CLASSID_FMA).tail_variant;

    // ── node table: one V3 NodeGuid per concept, minted on the is_a cascade ──
    let mut table: Vec<u8> = Vec::with_capacity(nodes.len() * 40);
    let mut deepest = 0u8;
    for n in nodes {
        let row = n["row"].as_u64().unwrap_or(0) as u32;
        let tissue = n["container"].as_u64().unwrap_or(0) as u8;
        let depth = n["depth"].as_u64().unwrap_or(0).min(255) as u8;
        deepest = deepest.max(depth);
        let rgb = n["rgb"].as_array();
        let r = rgb.and_then(|a| a.first()).and_then(|v| v.as_u64()).unwrap_or(180) as u8;
        let g = rgb.and_then(|a| a.get(1)).and_then(|v| v.as_u64()).unwrap_or(180) as u8;
        let b = rgb.and_then(|a| a.get(2)).and_then(|v| v.as_u64()).unwrap_or(180) as u8;
        let v_start = n["v_start"].as_u64().unwrap_or(0) as u32;
        let v_count = n["v_count"].as_u64().unwrap_or(0) as u32;

        // cascade = is_a ancestor sibling ranks root->self (≤5 tier identity bytes).
        let cascade = n["cascade"].as_array().cloned().unwrap_or_default();
        let id_at = |k: usize| -> u8 {
            cascade.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as u8
        };
        let tier_at = |k: usize| -> u16 {
            let id = id_at(k);
            if id == 0 { 0 } else { tier(mixin_for_depth(k), id) }
        };

        // Mint by the classid's registered tail variant (V3), never hardcoding the
        // tail — the contract's `mint_for` litmus (same as fma.rs).
        let key = NodeGuid::mint_for(
            tail,
            CLASSID_FMA,
            tier_at(0),                // HEEL  [depth0-mixin : rank]
            tier_at(1),                // HIP   [depth1-mixin : rank]
            tier_at(2),                // TWIG  [depth2-mixin : rank]
            tier_at(3),                // LEAF  [depth3-mixin : rank]
            u32::from(tier_at(4)),     // family[depth4-mixin : rank]
            row,                       // identity — stable concept row (node_row link)
        );

        table.extend_from_slice(key.as_bytes());
        table.push(tissue);
        table.push(depth);
        table.extend_from_slice(&[r, g, b]);
        table.extend_from_slice(&v_start.to_le_bytes());
        table.extend_from_slice(&v_count.to_le_bytes());
        let label = n["name"].as_str().unwrap_or("");
        let lb = label.as_bytes();
        let ll = lb.len().min(255);
        table.push(ll as u8);
        table.extend_from_slice(&lb[..ll]);
    }

    // ── BSO1 wire: header | V3 node table | SPM1 block ──
    let mut out: Vec<u8> = Vec::with_capacity(18 + table.len() + spm1.len());
    out.extend_from_slice(b"BSO1");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    out.extend_from_slice(&(table.len() as u32).to_le_bytes());
    out.extend_from_slice(&(spm1.len() as u32).to_le_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&spm1);

    // geometry counts (from the SPM1 header) for the receipt
    let vc = u32::from_le_bytes(spm1[4..8].try_into().unwrap());
    let tc = u32::from_le_bytes(spm1[8..12].try_into().unwrap());

    let out_path: PathBuf = if !out_arg.is_empty() {
        PathBuf::from(out_arg)
    } else {
        ["cockpit/public/body.soa", "../../cockpit/public/body.soa"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.parent().is_some_and(Path::exists))
            .unwrap_or_else(|| PathBuf::from("cockpit/public/body.soa"))
    };
    std::fs::write(&out_path, &out).unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));

    println!("── body.soa (BSO1) ──");
    println!("  V3 substrate : {} concepts minted on CLASSID_FMA_V3 cascade (max depth {deepest})", nodes.len());
    println!("  geometry     : {vc} verts · {tc} tris (ALL points, full-res SPM1)");
    println!("  node table   : {} B   geometry block: {} B", table.len(), spm1.len());
    println!("  baked {} ({} B)", out_path.display(), out.len());
}
