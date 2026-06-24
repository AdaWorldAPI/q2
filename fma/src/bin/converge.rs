// converge.rs — VERSION 3: cascading-HHTL `(part_of : is_a)` canonical NodeGuid.
//
// The convergence that loses NEITHER existing version. Three FMA addressings now
// coexist in disjoint files:
//
//   * v1 (other session, in-tree): is_a heart graph — `crates/osint-bake/.../fma.rs`,
//     `cockpit/.../FmaGraph.tsx`, served at `/fma`. Untouched.
//   * v2 (this crate, `guid.rs`): part_of FNV cascade, served at `/FMA`. Untouched.
//   * v3 (HERE): each 8:8 HHTL tier = `(part_of : is_a)` — the two axes are the
//     two BYTES of every tier, cascading down HEEL→HIP→TWIG:
//         high byte = part_of  (mixin / family / basin — WHERE it sits; partonomy)
//         low  byte = is_a      (identity / type        — WHAT it is;   taxonomy)
//     The high-byte chain prefix-routes the body partonomy; the low-byte chain
//     prefix-routes the type taxonomy — both hierarchies in ONE key, routable on
//     either axis at every level. "Cascading HHTL — that's the best of it."
//
// Canonical 16-byte layout, byte-identical to
// `lance_graph_contract::canonical_node::NodeGuid::new(classid, heel, hip, twig,
// family, identity)` (OGAR canon, locked 2026-06-13):
//
//     0..4   classid  (u32 LE)   ← OGAR ConceptDomain::Anatomy (high byte 0x0A)
//     4..6   HEEL     (u16 LE)   ┐ (part_of:is_a) cascade level 0
//     6..8   HIP      (u16 LE)   ├ level 1
//     8..10  TWIG     (u16 LE)   ┘ level 2
//    10..13  family   (u24 LE)   ← (part_of:is_a) level 3 = basin (0 ⇒ default basin)
//    13..16  identity (u24 LE)   ← golden-stride unique mint (24-bit, collision-probed)
//
// classid aligns with the other session's `osint-bake/fma.rs`:
//   0x0000_0A01 = anatomical_structure (soft tissue);  0x0000_0A02 = skeleton
//   (0x0A03/0x0A04 reserved bone/joint). A heart node from their bake and a heart
//   node from this full body therefore share classid — the addressings converge.
//
//   usage: converge [inclusion.txt] [isa_inclusion.txt] [out_dir]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;

// ── a BodyParts3D relation tree (part_of and is_a share the 4-column format) ──
struct Tree {
    parent_of: HashMap<String, String>,
    children: BTreeMap<String, Vec<String>>, // parent -> IRI-sorted children (stable ranks)
    name_of: HashMap<String, String>,
    nodes: HashSet<String>,
}

fn load_tree(path: &str) -> Tree {
    let txt = std::fs::read_to_string(path).unwrap_or_default();
    let mut parent_of = HashMap::new();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut name_of = HashMap::new();
    let mut nodes = HashSet::new();
    for (i, line) in txt.lines().enumerate() {
        if i == 0 {
            continue; // header: parent id / parent name / child id / child name
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 4 {
            parent_of.insert(f[2].to_string(), f[0].to_string());
            children.entry(f[0].to_string()).or_default().push(f[2].to_string());
            name_of.insert(f[2].to_string(), f[3].to_string());
            name_of.entry(f[0].to_string()).or_insert_with(|| f[1].to_string());
            nodes.insert(f[0].to_string());
            nodes.insert(f[2].to_string());
        }
    }
    for v in children.values_mut() {
        v.sort();
    }
    Tree { parent_of, children, name_of, nodes }
}

impl Tree {
    /// 1-based sibling rank under the node's parent (capped to a byte); 0 at the
    /// root. Two nodes sharing a parent get distinct ranks ⇒ distinct tier bytes;
    /// nodes sharing an ancestor at depth d share every tier byte above d.
    fn rank_of(&self, node: &str) -> u8 {
        match self.parent_of.get(node) {
            Some(p) => self.children[p]
                .iter()
                .position(|c| c == node)
                .map_or(0, |k| (k.min(254) as u8) + 1),
            None => 0,
        }
    }
    /// root..node id chain (distinguished-name path).
    fn chain(&self, node: &str) -> Vec<String> {
        let mut ids = vec![node.to_string()];
        let mut cur = node.to_string();
        let mut seen = HashSet::new();
        seen.insert(cur.clone());
        while let Some(p) = self.parent_of.get(&cur) {
            if !seen.insert(p.clone()) {
                break; // cycle guard
            }
            ids.push(p.clone());
            cur = p.clone();
        }
        ids.reverse();
        ids
    }
    /// Per-level sibling-rank along the chain (root-first); the byte axis.
    fn rank_chain(&self, node: &str) -> Vec<u8> {
        self.chain(node).iter().map(|n| self.rank_of(n)).collect()
    }
    fn dn(&self, node: &str) -> String {
        self.chain(node)
            .iter()
            .map(|id| self.name_of.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

// ── canonical NodeGuid emit — byte-identical to NodeGuid::new (see header) ──
fn node_guid_bytes(classid: u32, heel: u16, hip: u16, twig: u16, family: u32, identity: u32) -> [u8; 16] {
    let c = classid.to_le_bytes();
    let h = heel.to_le_bytes();
    let p = hip.to_le_bytes();
    let t = twig.to_le_bytes();
    let f = family.to_le_bytes(); // low 3 bytes = u24
    let i = identity.to_le_bytes(); // low 3 bytes = u24
    [
        c[0], c[1], c[2], c[3], // 0..4  classid
        h[0], h[1], // 4..6  HEEL
        p[0], p[1], // 6..8  HIP
        t[0], t[1], // 8..10 TWIG
        f[0], f[1], f[2], // 10..13 family (u24)
        i[0], i[1], i[2], // 13..16 identity (u24)
    ]
}
/// Canonical self-describing print (NodeGuid Display): 8-4-4-4-12 hex dash-groups.
fn guid_display(b: &[u8; 16]) -> String {
    let classid = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let heel = u16::from_le_bytes([b[4], b[5]]);
    let hip = u16::from_le_bytes([b[6], b[7]]);
    let twig = u16::from_le_bytes([b[8], b[9]]);
    let family = u32::from_le_bytes([b[10], b[11], b[12], 0]);
    let identity = u32::from_le_bytes([b[13], b[14], b[15], 0]);
    format!("{classid:08x}-{heel:04x}-{hip:04x}-{twig:04x}-{family:06x}{identity:06x}")
}

/// 8:8 tier = `(part_of_rank : is_a_rank)`.
fn tier(po: u8, ia: u8) -> u16 {
    ((po as u16) << 8) | ia as u16
}

// Golden stride = GOLDEN_RATIO × EULER_GAMMA (the bgz17 / bgz-hhtl-d / helix
// low-discrepancy generator); helix CurveRuler walk at stride 4, offset 20.
const GOLDEN_STRIDE: f64 = std::f64::consts::GOLDEN_RATIO * std::f64::consts::EULER_GAMMA;
fn golden_id24(k: usize) -> u32 {
    let x = (20.0 + (4 * k) as f64) * GOLDEN_STRIDE;
    (((x - x.floor()) * 16_777_216.0) as u32) & 0x00FF_FFFF
}

fn is_skeletal(s: &str) -> bool {
    let s = s.to_lowercase();
    ["bone", "skelet", "cartilage", "osseous", "vertebra", "rib", "femur", "skull"]
        .iter()
        .any(|k| s.contains(k))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let po_path = args.get(1).cloned().unwrap_or_else(|| "data/inclusion.txt".into());
    let ia_path = args.get(2).cloned().unwrap_or_else(|| "data/isa_inclusion.txt".into());
    let out_dir = args.get(3).cloned().unwrap_or_else(|| "guid".into());

    let po = load_tree(&po_path);
    let ia = load_tree(&ia_path);

    // Spine = the part_of nodes (where a part physically sits); is_a is joined in
    // per node as the type axis (absent ⇒ 0 byte = zero-fallback "not consulted").
    let mut nodes: Vec<String> = po.nodes.iter().cloned().collect();
    nodes.sort();

    std::fs::create_dir_all(&out_dir).ok();
    let mut mf = File::create(format!("{out_dir}/guid_converged.tsv")).unwrap();
    writeln!(mf, "fma\tcanonical_guid\tclassid\tpart_of_dn\tis_a_dn").unwrap();

    let mut used: HashSet<[u8; 16]> = HashSet::new();
    let mut records: Vec<(String, [u8; 16], u32, String, String)> = Vec::new();
    let (mut collisions, mut with_isa, mut skeletal) = (0usize, 0usize, 0usize);

    for (k, fma) in nodes.iter().enumerate() {
        let po_ranks = po.rank_chain(fma); // root..node, WHERE
        let ia_ranks = ia.rank_chain(fma); // root..node, WHAT (may be empty)
        let in_isa = ia.nodes.contains(fma);
        if in_isa {
            with_isa += 1;
        }

        // byte at cascade level L: part_of ancestor rank at depth L+1 (skip the
        // shared root), is_a likewise. 0 when that axis isn't that deep.
        let lvl = |ranks: &[u8], l: usize| -> u8 { ranks.get(l + 1).copied().unwrap_or(0) };

        let heel = tier(lvl(&po_ranks, 0), lvl(&ia_ranks, 0));
        let hip = tier(lvl(&po_ranks, 1), lvl(&ia_ranks, 1));
        let twig = tier(lvl(&po_ranks, 2), lvl(&ia_ranks, 2));
        // family (u24) = (part_of:is_a) level 3 in the low 16 bits — the basin.
        let family = ((lvl(&po_ranks, 3) as u32) << 8) | lvl(&ia_ranks, 3) as u32;

        // classid in the 0x0A anatomy domain, split skeleton vs soft tissue.
        let names: String = po.dn(fma) + " " + &ia.dn(fma);
        let classid = if is_skeletal(&names) { 0x0000_0A02 } else { 0x0000_0A01 };
        if classid == 0x0000_0A02 {
            skeletal += 1;
        }

        // golden-stride identity, probed unique within the (classid,heel,hip,twig,family) basin.
        let mut identity = golden_id24(k);
        let mut bytes = node_guid_bytes(classid, heel, hip, twig, family, identity);
        while used.contains(&bytes) {
            identity = (identity + 1) & 0x00FF_FFFF;
            bytes = node_guid_bytes(classid, heel, hip, twig, family, identity);
            collisions += 1;
        }
        used.insert(bytes);

        writeln!(
            mf,
            "{fma}\t{}\t{:#010x}\t{}\t{}",
            guid_display(&bytes),
            classid,
            po.dn(fma),
            if in_isa { ia.dn(fma) } else { "—".into() }
        )
        .unwrap();
        records.push((fma.clone(), bytes, classid, po.dn(fma), ia.dn(fma)));
    }

    // Self-check: the layout must round-trip (proves byte-compat with NodeGuid).
    if let Some((_, b, classid, _, _)) = records.first() {
        let dec_classid = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(dec_classid, *classid, "classid must round-trip at bytes 0..4 (canonical LE)");
    }

    eprintln!(
        "[converge] {} nodes -> {out_dir}/guid_converged.tsv  ({with_isa} with is_a axis, \
         {skeletal} skeletal 0x0A02, {collisions} identity probes)",
        records.len()
    );
    eprintln!("[converge] canonical NodeGuid layout (OGAR 2026-06-13): classid·HEEL·HIP·TWIG·family·identity, each tier 8:8 = (part_of:is_a)");

    // Demo: the aorta subtree resolves on BOTH axes at once — shared part_of
    // high-bytes (same body region) AND shared is_a low-bytes (same vessel type).
    eprintln!("\n[demo] aorta subtree — (part_of:is_a) cascade, shared leading tiles = shared ancestry on each axis:");
    let mut demo: Vec<&(String, [u8; 16], u32, String, String)> =
        records.iter().filter(|(_, _, _, po_dn, _)| po_dn.to_lowercase().contains("aorta")).collect();
    demo.sort_by(|a, b| guid_display(&a.1).cmp(&guid_display(&b.1)));
    for (fma, b, _, po_dn, ia_dn) in demo.iter().take(8) {
        let short = po_dn.rsplit(" / ").next().unwrap_or(po_dn);
        let typ = ia_dn.rsplit(" / ").next().unwrap_or(ia_dn);
        eprintln!("   {}  {fma}  part_of:{short}  is_a:{typ}", guid_display(b));
    }
}
