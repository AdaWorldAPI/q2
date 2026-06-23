//! FMA anatomy slice — the "real test" of the dual-membership lattice.
//!
//! Stands up a small Foundational-Model-of-Anatomy-shaped slice of the **heart**
//! (~120 nodes: organ → chambers → walls → tissues → cells) and proves that one
//! node resolves to BOTH addresses at once:
//!
//! * **part-of position** (basin-local): HEEL=organ, HIP=chamber, TWIG=wall,
//!   LEAF=structure, family=chamber-basin — where the node *is* in the body.
//! * **leaf-limited global type** (the CEILING pole, HEEL=HIP=TWIG=0xFFFF,
//!   LEAF=type): "cardiac muscle tissue", "endothelium" — cross-cutting types
//!   that appear in *every* chamber. The deepest sentinel run (through TWIG)
//!   makes the LEAF the sole discriminator: "limited to the leaf".
//!
//! The same `Cardiac muscle tissue` ceiling node is the `is-a` target for the
//! myocardium tissue in all four chambers — visibly cross-cutting. Emits
//! `cockpit/public/fma.soa` (OSO1, the cockpit's `/fma` view reads it) and
//! prints the dual-membership proof.
//!
//! Run from the workspace root:  `cargo run -p osint-bake --bin fma`

use lance_graph_contract::canonical_node::NodeGuid;
use std::path::{Path, PathBuf};

/// The CEILING global-category pole (HEEL=HIP=0xFFFF; sentinel through TWIG = leaf-grain).
const CEILING: u16 = 0xFFFF;
/// FMA classid (canonical class — distinct from OSINT 0x0700; classids sink in).
const CLASSID_FMA: u32 = 0x00F0_0A00; // "FMA"-ish prefix, app-stamped

// class bytes → cockpit colour/label (see FmaGraph.tsx).
const C_ORGAN: u8 = 0;
const C_CHAMBER: u8 = 1;
const C_WALL: u8 = 2;
const C_TISSUE: u8 = 3;
const C_CELL: u8 = 4;
const C_TYPE: u8 = 5; // the leaf-limited global type categories (ceiling pole)

// rel bytes (rel ≥ 2 renders in the cockpit view; 0/1 reserved like OSINT).
const REL_PART_OF: u8 = 2; // structure → its container (basin-local hierarchy)
const REL_IS_A: u8 = 3; // structure → its cross-cutting global type (ceiling)

struct Node {
    label: String,
    class: u8,
    key: NodeGuid,
}

struct Builder {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize, u8)>,
}

impl Builder {
    fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new() }
    }

    /// A part-of (basin-local) node: HEEL=organ, HIP=chamber, TWIG=wall, LEAF=struct.
    fn part_of_node(
        &mut self,
        label: &str,
        class: u8,
        organ: u16,
        chamber: u16,
        wall: u16,
        leaf: u16,
        basin: u8,
    ) -> usize {
        let i = self.nodes.len();
        let key = NodeGuid::new_v2(
            CLASSID_FMA,
            organ,        // HEEL — organ tier
            chamber,      // HIP  — chamber tier
            wall,         // TWIG — wall/region tier
            leaf,         // LEAF — structure tier
            u16::from(basin),
            i as u16,
        );
        self.nodes.push(Node { label: label.to_string(), class, key });
        i
    }

    /// A leaf-limited global TYPE category: HEEL=HIP=TWIG=0xFFFF, LEAF=type idx.
    fn type_node(&mut self, label: &str, type_idx: u16) -> usize {
        let i = self.nodes.len();
        let key = NodeGuid::new_v2(
            CLASSID_FMA,
            CEILING, // HEEL sentinel
            CEILING, // HIP  sentinel
            CEILING, // TWIG sentinel → leaf-grain ("limited to the leaf")
            type_idx, // LEAF — the sole discriminator
            0,        // family — global, no basin
            i as u16,
        );
        self.nodes.push(Node { label: label.to_string(), class: C_TYPE, key });
        i
    }

    fn edge(&mut self, s: usize, t: usize, rel: u8) {
        self.edges.push((s, t, rel));
    }
}

fn build_heart() -> Builder {
    let mut b = Builder::new();

    // ── cross-cutting global TYPE categories (leaf-limited, ceiling pole) ──
    // Each is the is-a target for the matching tissue in EVERY chamber.
    let types = [
        "Cardiac muscle tissue",
        "Fibrous tissue",
        "Endothelium",
        "Elastic tissue",
        "Mesothelium",
        "Adipose tissue",
    ];
    let type_idx: Vec<usize> = types
        .iter()
        .enumerate()
        .map(|(k, t)| b.type_node(t, k as u16))
        .collect();

    // each wall carries two tissues, each is-a one of the global types above.
    // (wall label, [(tissue label, type index)])
    let walls: [(&str, [(&str, usize); 2]); 3] = [
        ("myocardium", [("muscle layer", 0), ("fibrous skeleton", 1)]),
        ("endocardium", [("endothelial lining", 2), ("elastic layer", 3)]),
        ("epicardium", [("mesothelial layer", 4), ("subepicardial fat", 5)]),
    ];
    // a couple of cell types per tissue (depth + scale; part-of only).
    let cells: [&str; 2] = ["cell A", "cell B"];

    // ── the heart organ (basin-local root) ──
    let heart = b.part_of_node("Heart", C_ORGAN, 1, 0, 0, 0, 0);

    let chambers = ["left atrium", "right atrium", "left ventricle", "right ventricle"];
    for (ci, chamber) in chambers.iter().enumerate() {
        let cnum = (ci as u16) + 1; // HIP 1..4
        let basin = (ci as u8) + 1; // one basin per chamber
        let ch = b.part_of_node(chamber, C_CHAMBER, 1, cnum, 0, 0, basin);
        b.edge(ch, heart, REL_PART_OF);

        for (wi, (wall, tissues)) in walls.iter().enumerate() {
            let wnum = (wi as u16) + 1; // TWIG 1..3
            let w = b.part_of_node(
                &format!("{chamber} {wall}"),
                C_WALL,
                1,
                cnum,
                wnum,
                0,
                basin,
            );
            b.edge(w, ch, REL_PART_OF);

            for (ti, (tissue, gtype)) in tissues.iter().enumerate() {
                let leaf = (ti as u16) + 1;
                let t = b.part_of_node(
                    &format!("{chamber} {tissue}"),
                    C_TISSUE,
                    1,
                    cnum,
                    wnum,
                    leaf,
                    basin,
                );
                b.edge(t, w, REL_PART_OF);
                // THE dual membership: this basin-local tissue is-a the global type.
                b.edge(t, type_idx[*gtype], REL_IS_A);

                for (cells_i, cell) in cells.iter().enumerate() {
                    let cleaf = (ti as u16) * 8 + (cells_i as u16) + 16;
                    let c = b.part_of_node(
                        &format!("{chamber} {tissue} {cell}"),
                        C_CELL,
                        1,
                        cnum,
                        wnum,
                        cleaf,
                        basin,
                    );
                    b.edge(c, t, REL_PART_OF);
                }
            }
        }
    }
    b
}

/// Emit the OSO1 wire buffer the cockpit decodes (magic | counts | nodes
/// [guid|class] | edges [s|t|rel] | label tail). No tenant tail (FMA has no
/// dual-use facets); the decoder treats it as absent.
fn emit_oso1(b: &Builder) -> Vec<u8> {
    let n = b.nodes.len();
    let mut out = Vec::with_capacity(12 + n * 17 + b.edges.len() * 5);
    out.extend_from_slice(b"OSO1");
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(b.edges.len() as u32).to_le_bytes());
    for node in &b.nodes {
        out.extend_from_slice(node.key.as_bytes());
        out.push(node.class);
    }
    for &(s, t, rel) in &b.edges {
        out.extend_from_slice(&(s as u16).to_le_bytes());
        out.extend_from_slice(&(t as u16).to_le_bytes());
        out.push(rel);
    }
    for node in &b.nodes {
        let bytes = node.label.as_bytes();
        let l = bytes.len().min(255);
        out.push(l as u8);
        out.extend_from_slice(&bytes[..l]);
    }
    out
}

fn main() {
    let b = build_heart();
    let bytes = emit_oso1(&b);

    // dual-membership proof: find a basin-local tissue and show BOTH addresses.
    let tissue = b
        .nodes
        .iter()
        .position(|x| x.label == "left ventricle muscle layer")
        .expect("LV muscle layer present");
    let key = &b.nodes[tissue].key;
    println!("── FMA dual-membership proof ──");
    println!("node: {}", b.nodes[tissue].label);
    println!(
        "  part-of address (basin-local): HEEL={} HIP={} TWIG={} LEAF={} family={} identity={}",
        key.heel(),
        key.hip(),
        key.twig(),
        key.leaf(),
        key.family_v2(),
        key.identity_v2()
    );
    // its is-a edge → the leaf-limited global type (ceiling pole)
    let gtype = b
        .edges
        .iter()
        .find(|&&(s, _, rel)| s == tissue && rel == REL_IS_A)
        .map(|&(_, t, _)| t)
        .expect("tissue is-a a global type");
    let gk = &b.nodes[gtype].key;
    println!(
        "  is-a global type (ceiling): {}  HEEL={:#06x} HIP={:#06x} TWIG={:#06x} LEAF={}",
        b.nodes[gtype].label,
        gk.heel(),
        gk.hip(),
        gk.twig(),
        gk.leaf()
    );
    // cross-cutting: how many chambers' tissues share this one global type?
    let members = b.edges.iter().filter(|&&(_, t, rel)| t == gtype && rel == REL_IS_A).count();
    println!(
        "  '{}' is the is-a target of {members} tissues across the chambers (cross-cutting)",
        b.nodes[gtype].label
    );

    let n_types = b.nodes.iter().filter(|x| x.class == C_TYPE).count();
    println!(
        "\nslice: {} nodes ({} part-of + {} ceiling types), {} edges",
        b.nodes.len(),
        b.nodes.len() - n_types,
        n_types,
        b.edges.len()
    );

    // write to where the cockpit serves it (Vite copies public/* into dist/).
    let candidates = ["cockpit/public/fma.soa", "../../cockpit/public/fma.soa"];
    let out: PathBuf = candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.parent().is_some_and(Path::exists))
        .unwrap_or_else(|| PathBuf::from(candidates[0]));
    std::fs::write(&out, &bytes).expect("write fma.soa");
    println!("baked {} ({} bytes)", out.display(), bytes.len());
}
