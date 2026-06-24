// converge.rs — VERSION 3: cascading-HHTL `(place : tissue)` canonical NodeGuid,
// now converging BOTH the baked address AND the rendering substrate.
//
// Three FMA addressings coexist in disjoint files (lose none):
//   * v1 (other session): is_a heart graph, /fma, canonical NodeGuid (osint-bake).
//   * v2 (this crate, guid.rs): full-body part_of FNV cascade, /FMA.
//   * v3 (HERE): each 8:8 HHTL tier = `(place : tissue)` cascading HEEL→HIP→TWIG.
//
// The two bytes of every tier:
//   high = PLACE  — WHERE it sits. classid-dispatched (OGAR HhtlMode):
//            · skeleton (classid 0x0A02, Located): Morton spatial cell of the bone
//              centroid — the exact anchor IS the key (my anchor.rs hybrid).
//            · soft tissue (0x0A01, Cascade): part_of sibling-rank — ontological place,
//              inheriting position from its part_of basin's skeleton anchor.
//   low  = TISSUE — WHAT it is. is_a (taxonomy) sibling-rank, both modes.
// The high-byte chain prefix-routes the body (spatial for bone, partonomy for soft);
// the low-byte chain prefix-routes the type taxonomy. Both hierarchies, one key.
//
// connected_to (the EdgeBlock, 12 in-family + 4 out-of-family): part_of siblings are
// the in-family adjacency (the segments/sub-parts that physically connect — e.g. the
// aortic segments, the heart chambers); the is_a parent + nearest cross-basin neighbor
// are the out-of-family links. This is the "nodes connecting via relationships" graph.
//
// Canonical 16-byte layout, byte-identical to
// `lance_graph_contract::canonical_node::NodeGuid::new(classid, heel, hip, twig,
// family, identity)` (OGAR canon, 2026-06-13). classid in ConceptDomain::Anatomy
// (0x0A): 0x0A01 soft tissue, 0x0A02 skeleton — same space as the other session's bake.
//
//   usage: converge [inclusion.txt] [isa_inclusion.txt] [out_dir] [parts_dir] [element_parts]
//   (parts_dir optional — present ⇒ Located skeleton + spatial edges + render-ready
//    nodes.tsv/edges.tsv; absent ⇒ Cascade everywhere + structural edges only.)

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
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 4 {
            parent_of.insert(f[2].to_string(), f[0].to_string());
            children
                .entry(f[0].to_string())
                .or_default()
                .push(f[2].to_string());
            name_of.insert(f[2].to_string(), f[3].to_string());
            name_of
                .entry(f[0].to_string())
                .or_insert_with(|| f[1].to_string());
            nodes.insert(f[0].to_string());
            nodes.insert(f[2].to_string());
        }
    }
    for v in children.values_mut() {
        v.sort();
    }
    Tree {
        parent_of,
        children,
        name_of,
        nodes,
    }
}

impl Tree {
    fn rank_of(&self, node: &str) -> u8 {
        match self.parent_of.get(node) {
            Some(p) => self.children[p]
                .iter()
                .position(|c| c == node)
                .map_or(0, |k| (k.min(254) as u8) + 1),
            None => 0,
        }
    }
    fn chain(&self, node: &str) -> Vec<String> {
        let mut ids = vec![node.to_string()];
        let mut cur = node.to_string();
        let mut seen = HashSet::new();
        seen.insert(cur.clone());
        while let Some(p) = self.parent_of.get(&cur) {
            if !seen.insert(p.clone()) {
                break;
            }
            ids.push(p.clone());
            cur = p.clone();
        }
        ids.reverse();
        ids
    }
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
    /// in-family adjacency = part_of siblings (the co-parts that physically connect).
    fn siblings(&self, node: &str) -> Vec<String> {
        match self.parent_of.get(node) {
            Some(p) => self.children[p]
                .iter()
                .filter(|c| c.as_str() != node)
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }
}

// ── tissue classification (name + is_a + part_of → label/color), shared with the renderer ──
// Priority order (specific → generic): the first tissue whose keyword hits the structure's own
// name or any ancestor wins. "organ" is NOT a bare keyword (it would match "organ zone/region/
// part"); organs are caught by viscera names + "viscus"/"gland". Matching is word-bounded for
// single tokens so "scapula" (bone) does not hit "subscapularis" (muscle).
const TISSUE: &[(&str, [u8; 3], &[&str])] = &[
    (
        "nerve",
        [235, 209, 82],
        &["nerve", "neural", "ganglion", "plexus", "nervous"],
    ),
    (
        "vessel",
        [204, 56, 56],
        &[
            "artery", "arteries", "arterial", "arteriole", "vein", "veins", "venous", "vena",
            "venule", "vascular", "aorta", "aortic", "capillary", "sinus",
        ],
    ),
    ("cartilage", [159, 184, 217], &["cartilage", "chondral", "meniscus"]),
    ("ligament", [230, 230, 219], &["ligament"]),
    ("tendon", [224, 219, 204], &["tendon", "aponeurosis"]),
    ("fascia", [210, 205, 196], &["fascia", "iliotibial", "retinaculum"]),
    (
        "muscle",
        [189, 92, 87],
        &["muscle", "musculature", "musculus", "myocardium"],
    ),
    (
        "bone",
        [235, 224, 199],
        &[
            "bone", "skeletal", "osseous", "vertebra", "vertebral", "sesamoid", "ossicle",
            "scapula", "clavicle", "sternum", "manubrium", "rib", "ribs", "ilium", "ischium",
            "pubis", "hip bone", "pelvis", "sacrum", "coccyx", "femur", "patella", "tibia",
            "fibula", "humerus", "radius", "ulna", "carpal", "metacarpal", "tarsal",
            "metatarsal", "phalanx", "phalange", "skull", "cranium", "cranial", "mandible",
            "maxilla", "hyoid", "calvaria", "zygomatic", "ethmoid", "sphenoid", "vomer",
            "palatine",
        ],
    ),
    (
        "organ",
        [204, 148, 132],
        &[
            "viscus", "gland", "heart", "ventricle", "atrium", "lung", "liver", "hepatic lobe",
            "kidney", "spleen", "stomach", "pancreas", "gallbladder", "intestine", "duodenum",
            "jejunum", "ileum", "cecum", "caecum", "colon", "rectum", "appendix", "esophagus",
            "oesophagus", "trachea", "bronchus", "bronchi", "larynx", "pharynx", "bladder",
            "ureter", "urethra", "brain", "cerebrum", "cerebral", "cerebellum", "cerebellar",
            "brainstem", "diencephalon", "thalamus", "thyroid", "thymus", "adrenal",
            "suprarenal", "hypophysis", "pituitary", "uterus", "ovary", "testis", "testicle",
            "prostate", "epididymis", "eyeball", "cochlea", "tongue", "tonsil", "scrotum",
            "penis", "vagina", "cervix", "mammary", "breast", "parotid", "submandibular",
            "sublingual",
        ],
    ),
    ("skin", [219, 168, 138], &["skin", "integument", "epidermis", "dermis"]),
];
/// word-bounded for single tokens (so "scapula" ≠ "subscapularis"); substring for multi-word keys.
fn hit(name: &str, kw: &str) -> bool {
    if kw.contains(' ') {
        name.contains(kw)
    } else {
        name.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == kw)
    }
}
fn tissue_of(po: &Tree, ia: &Tree, fma: &str) -> (&'static str, [u8; 3]) {
    // candidate names, leaf-first: own name + is_a ancestry, then part_of ancestry. is_a first
    // carries "muscle organ"/"bone organ" disambiguators; part_of recovers the ~500 structures
    // absent from the is_a tree (heart, nerves, pelvis, vessels) via their own descriptive name.
    let mut names: Vec<String> = Vec::new();
    for tree in [ia, po] {
        for id in tree.chain(fma).iter().rev() {
            if let Some(nm) = tree.name_of.get(id) {
                names.push(nm.to_lowercase());
            }
        }
    }
    for nm in &names {
        for (lab, col, kws) in TISSUE {
            if kws.iter().any(|k| hit(nm, k)) {
                return (lab, *col);
            }
        }
    }
    ("other", [150, 150, 160])
}

#[cfg(test)]
mod tests {
    use super::*;
    /// classify a single lowercase name through the priority table (the inner loop of tissue_of).
    fn classify(name: &str) -> &'static str {
        let l = name.to_lowercase();
        TISSUE
            .iter()
            .find(|(_, _, kws)| kws.iter().any(|k| hit(&l, k)))
            .map_or("other", |(lab, _, _)| lab)
    }
    #[test]
    fn hit_is_word_bounded() {
        // single tokens are word-bounded: "scapula" the bone must not leak into its neighbours
        assert!(hit("left scapula", "scapula"));
        assert!(!hit("levator scapulae", "scapula")); // a muscle
        assert!(!hit("left subscapular artery", "scapula")); // a vessel
        assert!(!hit("nervure", "nerve")); // no stray substring match
        assert!(hit("left hip bone", "hip bone")); // multi-word keys match as substrings
    }
    #[test]
    fn priority_and_organ_restriction() {
        assert_eq!(classify("heart"), "organ");
        assert_eq!(classify("right scapula"), "bone");
        assert_eq!(classify("muscle of thigh"), "muscle");
        // word-bounding: "scapulae" must NOT be read as the bone "scapula" — by name alone this
        // is unclassified (the real tissue_of upgrades it to muscle via its is_a ancestry).
        assert_eq!(classify("levator scapulae"), "other");
        assert_eq!(classify("vertebral artery"), "vessel"); // vessel beats bone
        assert_eq!(classify("right iliotibial tract"), "fascia"); // NOT organ
        assert_eq!(classify("median nerve"), "nerve");
        assert_eq!(classify("organ zone"), "other"); // bare "organ" no longer fires
        assert_eq!(classify("cardinal organ part"), "other");
    }
}

// ── geometry: per-FMA centroid from the meshes (slices 1 + 3) ──
fn obj_centroid(text: &str) -> Option<([f64; 3], usize)> {
    let (mut s, mut n) = ([0.0f64; 3], 0usize);
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some("v") {
            let c: Vec<f64> = it.take(3).filter_map(|t| t.parse().ok()).collect();
            if c.len() == 3 {
                for k in 0..3 {
                    s[k] += c[k];
                }
                n += 1;
            }
        }
    }
    (n > 0).then(|| ([s[0] / n as f64, s[1] / n as f64, s[2] / n as f64], n))
}

/// FMA → centroid (vertex-count-weighted mean over its FJ meshes), if a parts_dir is given.
fn load_centroids(parts_dir: &str, elem_path: &str) -> HashMap<String, [f32; 3]> {
    let mut fj_fma: HashMap<String, Vec<String>> = HashMap::new();
    for (i, line) in std::fs::read_to_string(elem_path)
        .unwrap_or_default()
        .lines()
        .enumerate()
    {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 3 {
            fj_fma.entry(f[2].into()).or_default().push(f[0].into());
        }
    }
    let mut acc: HashMap<String, ([f64; 3], usize)> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(parts_dir) {
        for path in rd.filter_map(|e| e.ok()).map(|e| e.path()) {
            if path.extension().and_then(|s| s.to_str()) != Some("obj") {
                continue;
            }
            let fj = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let Some(fmas) = fj_fma.get(&fj) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some((c, w)) = obj_centroid(&text) else {
                continue;
            };
            for fma in fmas {
                let e = acc.entry(fma.clone()).or_insert(([0.0; 3], 0));
                for (slot, &ci) in e.0.iter_mut().zip(c.iter()) {
                    *slot += ci * w as f64;
                }
                e.1 += w;
            }
        }
    }
    acc.into_iter()
        .filter(|(_, (_, w))| *w > 0)
        .map(|(fma, (s, w))| {
            (
                fma,
                [
                    (s[0] / w as f64) as f32,
                    (s[1] / w as f64) as f32,
                    (s[2] / w as f64) as f32,
                ],
            )
        })
        .collect()
}

// ── Morton (slice 1): interleave 3×8 bits → 24-bit Z-order spatial code ──
fn part1by2(mut n: u32) -> u32 {
    n &= 0xff;
    n = (n | (n << 16)) & 0x0300_00FF;
    n = (n | (n << 8)) & 0x0300_F00F;
    n = (n | (n << 4)) & 0x030C_30C3;
    n = (n | (n << 2)) & 0x0924_9249;
    n
}
fn morton3(x: u8, y: u8, z: u8) -> u32 {
    part1by2(x as u32) | (part1by2(y as u32) << 1) | (part1by2(z as u32) << 2)
}

// ── canonical NodeGuid emit — byte-identical to NodeGuid::new (see header) ──
fn node_guid_bytes(
    classid: u32,
    heel: u16,
    hip: u16,
    twig: u16,
    family: u32,
    identity: u32,
) -> [u8; 16] {
    let c = classid.to_le_bytes();
    let h = heel.to_le_bytes();
    let p = hip.to_le_bytes();
    let t = twig.to_le_bytes();
    let f = family.to_le_bytes();
    let i = identity.to_le_bytes();
    [
        c[0], c[1], c[2], c[3], h[0], h[1], p[0], p[1], t[0], t[1], f[0], f[1], f[2], i[0], i[1],
        i[2],
    ]
}
fn guid_display(b: &[u8; 16]) -> String {
    let classid = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let heel = u16::from_le_bytes([b[4], b[5]]);
    let hip = u16::from_le_bytes([b[6], b[7]]);
    let twig = u16::from_le_bytes([b[8], b[9]]);
    let family = u32::from_le_bytes([b[10], b[11], b[12], 0]);
    let identity = u32::from_le_bytes([b[13], b[14], b[15], 0]);
    format!("{classid:08x}-{heel:04x}-{hip:04x}-{twig:04x}-{family:06x}{identity:06x}")
}
fn tier(hi: u8, lo: u8) -> u16 {
    ((hi as u16) << 8) | lo as u16
}
const GOLDEN_STRIDE: f64 = std::f64::consts::GOLDEN_RATIO * std::f64::consts::EULER_GAMMA;
fn golden_id24(k: usize) -> u32 {
    let x = (20.0 + (4 * k) as f64) * GOLDEN_STRIDE;
    (((x - x.floor()) * 16_777_216.0) as u32) & 0x00FF_FFFF
}
fn is_skeletal(s: &str) -> bool {
    let s = s.to_lowercase();
    [
        "bone",
        "skelet",
        "cartilage",
        "osseous",
        "vertebra",
        "rib",
        "femur",
        "skull",
    ]
    .iter()
    .any(|k| s.contains(k))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let po_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "data/inclusion.txt".into());
    let ia_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "data/isa_inclusion.txt".into());
    let out_dir = args.get(3).cloned().unwrap_or_else(|| "guid".into());
    let parts_dir = args.get(4).cloned();
    let elem_path = args
        .get(5)
        .cloned()
        .unwrap_or_else(|| "data/combined_element_parts.txt".into());

    let po = load_tree(&po_path);
    let ia = load_tree(&ia_path);

    // Optional centroids → Located skeleton (slice 1) + spatial edges + render input.
    let centroids = parts_dir
        .as_ref()
        .map(|d| load_centroids(d, &elem_path))
        .unwrap_or_default();
    // Global centroid bbox for Morton normalization.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for c in centroids.values() {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let morton_of = |c: &[f32; 3]| -> u32 {
        let q = |v: f32, k: usize| -> u8 {
            let span = (hi[k] - lo[k]).max(1e-6);
            (((v - lo[k]) / span).clamp(0.0, 1.0) * 255.0) as u8
        };
        morton3(q(c[0], 0), q(c[1], 1), q(c[2], 2))
    };

    let mut nodes: Vec<String> = po.nodes.iter().cloned().collect();
    nodes.sort();

    std::fs::create_dir_all(&out_dir).ok();
    let mut mf = File::create(format!("{out_dir}/guid_converged.tsv")).unwrap();
    writeln!(
        mf,
        "fma\tcanonical_guid\tclassid\tplace_mode\ttissue\tconnected_to\tpart_of_dn\tis_a_dn"
    )
    .unwrap();
    let mut nf = File::create(format!("{out_dir}/nodes.tsv")).unwrap();
    writeln!(nf, "fma\tx\ty\tz\tclassid\ttissue\tr\tg\tb\tguid").unwrap();
    let mut ef = File::create(format!("{out_dir}/edges.tsv")).unwrap();
    writeln!(ef, "src_fma\tdst_fma\trel").unwrap();

    let mut used: HashSet<[u8; 16]> = HashSet::new();
    let (mut located, mut with_isa, mut skeletal, mut edge_n, mut placed) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut demo: Vec<(String, [u8; 16], String, String, &'static str)> = Vec::new();

    for (k, fma) in nodes.iter().enumerate() {
        let po_ranks = po.rank_chain(fma);
        let ia_ranks = ia.rank_chain(fma);
        if ia.nodes.contains(fma) {
            with_isa += 1;
        }
        let lvl = |ranks: &[u8], l: usize| -> u8 { ranks.get(l + 1).copied().unwrap_or(0) };

        let names = po.dn(fma) + " " + &ia.dn(fma);
        let skeletal_node = is_skeletal(&names);
        let classid = if skeletal_node {
            0x0000_0A02
        } else {
            0x0000_0A01
        };
        if skeletal_node {
            skeletal += 1;
        }
        let (tissue, rgb) = tissue_of(&po, &ia, fma);

        // PLACE (high bytes): Located Morton for a skeletal node WITH a centroid,
        // else Cascade part_of rank. TISSUE (low bytes): is_a rank, both modes.
        let (heel, hip, twig, mode) = match (skeletal_node, centroids.get(fma)) {
            (true, Some(c)) => {
                let m = morton_of(c);
                located += 1;
                (
                    tier(((m >> 16) & 0xFF) as u8, lvl(&ia_ranks, 0)),
                    tier(((m >> 8) & 0xFF) as u8, lvl(&ia_ranks, 1)),
                    tier((m & 0xFF) as u8, lvl(&ia_ranks, 2)),
                    "Located",
                )
            }
            _ => (
                tier(lvl(&po_ranks, 0), lvl(&ia_ranks, 0)),
                tier(lvl(&po_ranks, 1), lvl(&ia_ranks, 1)),
                tier(lvl(&po_ranks, 2), lvl(&ia_ranks, 2)),
                "Cascade",
            ),
        };
        // family (u24) = ontological basin: (part_of:is_a) level 3, both modes.
        let family = ((lvl(&po_ranks, 3) as u32) << 8) | lvl(&ia_ranks, 3) as u32;

        let mut identity = golden_id24(k);
        let mut bytes = node_guid_bytes(classid, heel, hip, twig, family, identity);
        while used.contains(&bytes) {
            identity = (identity + 1) & 0x00FF_FFFF;
            bytes = node_guid_bytes(classid, heel, hip, twig, family, identity);
        }
        used.insert(bytes);

        // connected_to: in-family = part_of siblings (≤12); out-of-family = is_a parent (≤4).
        let sibs = po.siblings(fma);
        let in_family: Vec<String> = sibs.iter().take(12).cloned().collect();
        for s in &in_family {
            writeln!(ef, "{fma}\t{s}\tin_family:part_of_sibling").unwrap();
            edge_n += 1;
        }
        if let Some(p) = ia.parent_of.get(fma) {
            writeln!(ef, "{fma}\t{p}\tout_family:is_a_parent").unwrap();
            edge_n += 1;
        }
        let conn = if in_family.is_empty() {
            "—".to_string()
        } else {
            in_family.join(",")
        };

        writeln!(
            mf,
            "{fma}\t{}\t{:#010x}\t{mode}\t{tissue}\t{conn}\t{}\t{}",
            guid_display(&bytes),
            classid,
            po.dn(fma),
            if ia.nodes.contains(fma) {
                ia.dn(fma)
            } else {
                "—".into()
            }
        )
        .unwrap();

        if let Some(c) = centroids.get(fma) {
            writeln!(
                nf,
                "{fma}\t{:.4}\t{:.4}\t{:.4}\t{:#010x}\t{tissue}\t{}\t{}\t{}\t{}",
                c[0],
                c[1],
                c[2],
                classid,
                rgb[0],
                rgb[1],
                rgb[2],
                guid_display(&bytes)
            )
            .unwrap();
            placed += 1;
        }
        if po.dn(fma).to_lowercase().contains("aorta") {
            demo.push((fma.clone(), bytes, po.dn(fma), conn.clone(), tissue));
        }
    }

    eprintln!(
        "[converge] {} nodes -> {out_dir}/  ({with_isa} with is_a, {skeletal} skeletal, {located} Located via centroid, {placed} placed, {edge_n} edges)",
        nodes.len()
    );
    eprintln!("[converge] tier = (place:tissue): place = Morton(bone)/part_of(soft) [classid-dispatched], tissue = is_a; canonical NodeGuid (OGAR 2026-06-13)");
    if centroids.is_empty() {
        eprintln!("[converge] no parts_dir → Cascade everywhere + structural edges only (pass a parts_dir for Located skeleton + render-ready nodes.tsv/edges.tsv)");
    }

    eprintln!("\n[demo] aorta subtree — (place:tissue) + connected_to (part_of siblings = the segments that connect):");
    demo.sort_by_key(|d| guid_display(&d.1));
    for (fma, b, po_dn, conn, tissue) in demo.iter().take(8) {
        let short = po_dn.rsplit(" / ").next().unwrap_or(po_dn);
        eprintln!(
            "   {}  {fma}  {short} [{tissue}]  ↔ {conn}",
            guid_display(b)
        );
    }
}
