// Distinguished-name parent resolution -> deterministic canonical GUID.
//
//   container:identity  (classid)::(8:8)::(8:8)::(8:8)::(8:8)::(8:8):(8:8)
//      classid (u32) = body-system class  |  6 tiers (u16 = two byte-axes each):
//      HEEL HIP TWIG  +  two family tiers  +  IDENTITY
//
//   usage: guid <inclusion.txt> <element_parts.txt> <out_dir>
//
// Each FMA node's distinguished name is its part_of ancestry (root..leaf),
// resolved by walking parent_of. The cascade tiers are FNV-1a hashes of the
// CUMULATIVE ancestor prefix (id path), so two nodes sharing an ancestor share
// every leading tier — prefix-routable by construction. classid = system hash.
// Deterministic (FNV-1a, no time/random); identity collisions resolved by an
// in-basin probe over sorted nodes.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;

fn fnv1a64(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}
fn fnv32(s: &str) -> u32 {
    let h = fnv1a64(s);
    (h as u32) ^ ((h >> 32) as u32)
}
fn fnv16(s: &str) -> u16 {
    let h = fnv1a64(s);
    (h as u16) ^ ((h >> 16) as u16) ^ ((h >> 32) as u16) ^ ((h >> 48) as u16)
}
fn bb(x: u16) -> String {
    format!("{:02x}:{:02x}", (x >> 8) as u8, x as u8)
}

// Golden stride = GOLDEN_RATIO × EULER_GAMMA (canon γ+φ; the bgz17 / bgz-hhtl-d /
// helix low-discrepancy generator). Identity mint walks it at stride 4, offset 20
// — the helix CurveRuler — giving maximally-spread, deterministic identity codes.
const GOLDEN_STRIDE: f64 = std::f64::consts::GOLDEN_RATIO * std::f64::consts::EULER_GAMMA;
fn golden_id(k: usize) -> u16 {
    let x = (20.0 + (4 * k) as f64) * GOLDEN_STRIDE;
    ((x - x.floor()) * 65536.0) as u16
}
fn fmt_guid(classid: u32, t: &[u16; 6]) -> String {
    format!("{:08x}::{}::{}::{}::{}::{}:{}", classid, bb(t[0]), bb(t[1]), bb(t[2]), bb(t[3]), bb(t[4]), bb(t[5]))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inc_path = args.get(1).cloned().unwrap_or_else(|| "data/inclusion.txt".into());
    let elem_path = args.get(2).cloned().unwrap_or_else(|| "data/combined_element_parts.txt".into());
    let out_dir = args.get(3).cloned().unwrap_or_else(|| "guid".into());

    // part_of tree: child -> parent, id -> name.
    let inc = std::fs::read_to_string(&inc_path).expect("inclusion");
    let mut parent_of: HashMap<String, String> = HashMap::new();
    let mut name_of: HashMap<String, String> = HashMap::new();
    let mut all: HashSet<String> = HashSet::new();
    for (i, line) in inc.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 4 {
            parent_of.insert(f[2].into(), f[0].into());
            name_of.insert(f[2].into(), f[3].into());
            name_of.entry(f[0].into()).or_insert_with(|| f[1].into());
            all.insert(f[0].into());
            all.insert(f[2].into());
        }
    }

    // Distinguished name = parent resolution (root..node).
    let path_of = |node: &str| -> Vec<String> {
        let mut ids = vec![node.to_string()];
        let mut cur = node.to_string();
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(cur.clone());
        while let Some(p) = parent_of.get(&cur) {
            if !seen.insert(p.clone()) {
                break; // cycle guard
            }
            ids.push(p.clone());
            cur = p.clone();
        }
        ids.reverse(); // root .. node
        ids
    };
    let dn_string = |path: &[String]| -> String {
        path.iter().map(|id| name_of.get(id).cloned().unwrap_or_else(|| id.clone())).collect::<Vec<_>>().join(" / ")
    };

    // Deterministic GUID per node, identity-probed within (classid, tiers[0..5]) basin.
    let mut sorted: Vec<String> = all.iter().cloned().collect();
    sorted.sort();
    let mut used: HashSet<(u32, [u16; 6])> = HashSet::new();
    let mut guid_of: HashMap<String, String> = HashMap::new();
    let mut record: Vec<(String, String, String, String)> = Vec::new(); // system, fma, guid, dn
    let mut depth_hist: HashMap<usize, usize> = HashMap::new();
    let (mut collisions, mut max_depth) = (0usize, 0usize);

    for (k, fma) in sorted.iter().enumerate() {
        let path = path_of(fma);
        let depth = path.len();
        max_depth = max_depth.max(depth);
        *depth_hist.entry(depth).or_insert(0) += 1;

        // classid = hash(root/system); HEEL..W5 = cumulative ancestor prefixes.
        let classid = if depth >= 2 { fnv32(&path[..2].join("/")) } else { 0 };
        let mut t = [0u16; 6];
        for (i, ti) in t.iter_mut().take(5).enumerate() {
            let lvl = 3 + i; // prefix length root+system+(i+1) deep
            *ti = if depth >= lvl { fnv16(&path[..lvl].join("/")) } else { 0 };
        }
        t[5] = golden_id(k); // IDENTITY = golden-stride mint (helix CurveRuler, not arbitrary hash)
        while used.contains(&(classid, t)) {
            t[5] = t[5].wrapping_add(1);
            collisions += 1;
        }
        used.insert((classid, t));

        let system = if depth >= 2 { name_of.get(&path[1]).cloned().unwrap_or_default() } else { "—".into() };
        let guid = fmt_guid(classid, &t);
        guid_of.insert(fma.clone(), guid.clone());
        record.push((system, fma.clone(), guid, dn_string(&path)));
    }

    std::fs::create_dir_all(&out_dir).ok();
    let mut mf = File::create(format!("{out_dir}/guid_manifest.tsv")).unwrap();
    writeln!(mf, "container(system)\tidentity(fma)\tguid\tdistinguished_name").unwrap();
    for (sys, fma, guid, dn) in &record {
        writeln!(mf, "{sys}\t{fma}\t{guid}\t{dn}").unwrap();
    }

    // Mesh FJ -> GUID: primary FMA = the deepest (most specific) concept the FJ is part of.
    let elem = std::fs::read_to_string(&elem_path).unwrap_or_default();
    let mut fj_fma: HashMap<String, Vec<String>> = HashMap::new();
    for (i, line) in elem.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 3 {
            fj_fma.entry(f[2].into()).or_default().push(f[0].into());
        }
    }
    let mut ff = File::create(format!("{out_dir}/fj_guid.tsv")).unwrap();
    writeln!(ff, "fj\tprimary_fma\tguid\tdistinguished_name").unwrap();
    let mut fj_keys: Vec<&String> = fj_fma.keys().collect();
    fj_keys.sort();
    let mut fj_mapped = 0usize;
    for fj in fj_keys {
        let best = fj_fma[fj].iter().filter(|c| all.contains(*c)).max_by_key(|c| path_of(c).len());
        if let Some(c) = best {
            writeln!(ff, "{fj}\t{c}\t{}\t{}", guid_of[c], dn_string(&path_of(c))).unwrap();
            fj_mapped += 1;
        }
    }

    eprintln!("[guid] {} FMA nodes, max DN depth {max_depth}, {collisions} identity collisions probed", record.len());
    eprintln!("[guid] {fj_mapped} meshes mapped to GUIDs -> {out_dir}/fj_guid.tsv");
    let mut dks: Vec<usize> = depth_hist.keys().cloned().collect();
    dks.sort();
    eprint!("[guid] DN depth histogram: ");
    for d in &dks {
        eprint!("{d}:{} ", depth_hist[d]);
    }
    eprintln!("\n[guid] format  container:identity  (classid)::HEEL(8:8)::HIP::TWIG::F4::F5:IDENTITY\n");

    // Prefix-routability demo: aorta subtree — shared classid/HEEL/HIP, diverging TWIG.
    eprintln!("[demo] aorta subtree (note shared leading groups = shared ancestry):");
    let mut demo: Vec<&(String, String, String, String)> =
        record.iter().filter(|(_, _, _, dn)| dn.to_lowercase().contains("aorta")).collect();
    demo.sort_by(|a, b| a.2.cmp(&b.2));
    for (sys, fma, guid, dn) in demo.iter().take(10) {
        let short = dn.rsplit(" / ").next().unwrap_or(dn);
        eprintln!("   {sys}:{fma}  {guid}   {short}");
    }
}
