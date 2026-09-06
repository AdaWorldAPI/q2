//! cpic ingest — mint the canonical `(part_of : is_a)` NodeGuid for every CPIC entity.
//!
//! Reads the CPIC JSON tables (gene, allele, diplotype, phenotype/gene_result, drug,
//! recommendation) and emits `pgx_nodes.tsv` + `pgx_edges.tsv`. The GUID toolkit (classid,
//! cascade, basin, mint) lives in `cpic` (lib.rs); this bin wires the graph.
//!
//! POC over published CPIC rules — NOT clinical decision support.
//!
//! Usage:  ingest [data_dir=cpic/data] [out_dir=cpic/out] [max_diplotype_rows=4000]

use cpic::{
    basin, cascade3, func_class, gene_part_of, mint_identity, norm, NodeGuid, CID_ALLELE,
    CID_DIPLOTYPE, CID_DRUG, CID_GENE, CID_PHENOTYPE, CID_REC,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

/// What to mint: classid, basin key, both distinguished names, and the display kind/label.
struct NodeSpec<'a> {
    classid: u32,
    basin: &'a str,
    part: Vec<String>,
    isa: Vec<String>,
    kind: &'a str,
    label: String,
    record: bool,
}

/// The (part_of:is_a) graph builder: mints + collision-checks keys, accumulates TSV rows.
struct Graph {
    counters: HashMap<(u32, u32), u32>, // (classid, family) -> per-basin counter
    nodes: Vec<String>,
    edges: Vec<String>,
    keys: HashMap<[u8; 16], String>,
    minted: usize,
    collisions: usize,
}
impl Graph {
    fn new() -> Self {
        Graph {
            counters: HashMap::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            keys: HashMap::new(),
            minted: 0,
            collisions: 0,
        }
    }
    /// Mint (always — for the scale + collision proof); push the node row only if `record`.
    fn add(&mut self, n: &NodeSpec) -> NodeGuid {
        let family = basin(n.basin);
        let c = self.counters.entry((n.classid, family)).or_insert(0);
        *c += 1;
        let identity = mint_identity(*c);
        let g = NodeGuid::mint(
            n.classid,
            cascade3(&n.part),
            cascade3(&n.isa),
            family,
            identity,
        );
        self.minted += 1;
        if let Some(prev) = self.keys.insert(g.key16(), n.label.clone()) {
            eprintln!("[COLLISION] {} :: {prev} vs {}", g.hex(), n.label);
            self.collisions += 1;
        }
        if n.record {
            self.nodes.push(format!(
                "{}\t{}\t{}\t{}\t{}",
                g.hex(),
                n.kind,
                n.label,
                n.part.join(" / "),
                n.isa.join(" / ")
            ));
        }
        g
    }
    fn edge(&mut self, src: NodeGuid, rel: &str, dst: NodeGuid) {
        self.edges
            .push(format!("{}\t{rel}\t{}", src.hex(), dst.hex()));
    }
}

fn load(path: &str) -> Vec<Value> {
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// True only for a real ATC code shape (letter + 2 digits + …), so placeholders like "NA"
/// don't get mis-filed under ATC main group "N".
fn is_atc(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1].is_ascii_digit() && b[2].is_ascii_digit()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // default to crate-relative paths when run from cpic/, repo-relative otherwise
    let local = std::path::Path::new("data/gene.json").exists();
    let dir = a
        .get(1)
        .cloned()
        .unwrap_or_else(|| if local { "data" } else { "cpic/data" }.into());
    let out = a
        .get(2)
        .cloned()
        .unwrap_or_else(|| if local { "out" } else { "cpic/out" }.into());
    let max_diplo: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(4000);
    fs::create_dir_all(&out).unwrap();
    let p = |f: &str| format!("{dir}/{f}");

    let mut gr = Graph::new();

    // ── genes (part_of = partonomy; is_a shallow — genes are containers) ──
    let mut gene_g: HashMap<String, NodeGuid> = HashMap::new();
    for v in load(&p("gene.json")) {
        let sym = v["symbol"].as_str().unwrap_or("").to_string();
        if sym.is_empty() {
            continue;
        }
        let g = gr.add(&NodeSpec {
            classid: CID_GENE,
            basin: &sym,
            part: gene_part_of(&sym),
            isa: vec!["entity".into(), "gene".into()],
            kind: "gene",
            label: sym.clone(),
            record: true,
        });
        gene_g.insert(sym, g);
    }

    // ── drugs (is_a = ATC cascade; the second ready-made taxonomy) ──
    let mut drug_g: HashMap<String, NodeGuid> = HashMap::new();
    for v in load(&p("drug.json")) {
        let did = v["drugid"].as_str().unwrap_or("").to_string();
        let name = v["name"].as_str().unwrap_or("").to_string();
        if did.is_empty() {
            continue;
        }
        let atc = v["atcid"]
            .as_array()
            .and_then(|x| x.first())
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let mut isa = vec!["drug".to_string()];
        let valid_atc = is_atc(atc);
        if valid_atc {
            isa.push(atc[..1].into()); // anatomical main group, e.g. N
            isa.push(atc[..3].into()); // therapeutic subgroup, e.g. N06
            if atc.len() >= 4 {
                isa.push(atc[..4].into()); // pharmacological subgroup, e.g. N06A
            }
            if atc.len() >= 5 {
                isa.push(atc[..5].into()); // chemical subgroup, e.g. N06AA
            }
        }
        isa.push(norm(&name));
        let root = if valid_atc {
            atc.chars().next().unwrap_or('_')
        } else {
            '_'
        };
        let g = gr.add(&NodeSpec {
            classid: CID_DRUG,
            basin: &format!("drug:{root}"),
            part: vec!["pharmacogenome".into(), "drugs".into(), norm(&name)],
            isa,
            kind: "drug",
            label: name.clone(),
            record: true,
        });
        drug_g.insert(did, g);
    }

    // ── phenotypes / gene_result (is_a = metabolizer / function ladder) ──
    let mut pheno_by_gr: HashMap<(String, String), NodeGuid> = HashMap::new();
    for v in load(&p("gene_result.json")) {
        let sym = v["genesymbol"].as_str().unwrap_or("").to_string();
        let result = v["result"].as_str().unwrap_or("").to_string();
        if sym.is_empty() || result.is_empty() {
            continue;
        }
        let mut part = gene_part_of(&sym);
        part.push("phenotypes".into());
        part.push(norm(&result));
        let g = gr.add(&NodeSpec {
            classid: CID_PHENOTYPE,
            basin: &sym,
            part,
            isa: vec!["phenotype".into(), norm(&result)],
            kind: "phenotype",
            label: format!("{sym} {result}"),
            record: true,
        });
        pheno_by_gr.insert((sym.clone(), result.clone()), g);
        if let Some(gene) = gene_g.get(&sym) {
            gr.edge(g, "part_of", *gene);
        }
    }

    // ── alleles (is_a = functional status; part_of = under gene) ──
    let mut allele_g: HashMap<(String, String), NodeGuid> = HashMap::new();
    for v in load(&p("allele.json")) {
        let sym = v["genesymbol"].as_str().unwrap_or("").to_string();
        let name = v["name"].as_str().unwrap_or("").to_string();
        if sym.is_empty() || name.is_empty() {
            continue;
        }
        let status = v["clinicalfunctionalstatus"].as_str();
        let mut part = gene_part_of(&sym);
        part.push(norm(&name));
        let g = gr.add(&NodeSpec {
            classid: CID_ALLELE,
            basin: &sym,
            part,
            isa: vec!["allele".into(), func_class(status)],
            kind: "allele",
            label: format!("{sym} {name}"),
            record: true,
        });
        allele_g.insert((sym.clone(), name.clone()), g);
        if let Some(gene) = gene_g.get(&sym) {
            gr.edge(g, "part_of", *gene);
        }
    }

    // ── diplotypes (the combinatorial layer; ~110k rows). Mint ALL for the scale + collision
    //    proof; record only the first `max_diplo` rows to keep the committed TSV small.
    //    NOTE: diplotype→phenotype (`maps_to`) is DEFERRED — the diplotype's `functionphenotypeid`
    //    references a separate `function_phenotype` table (disjoint id space from gene_result.id;
    //    verified 0 overlap) not present in this dump. It is a one-line join once that table lands.
    let mut diplo_minted = 0usize;
    let dipath = p("gene_result_diplotype.json");
    if let Ok(s) = fs::read_to_string(&dipath) {
        let rows: Vec<Value> =
            serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {dipath}: {e}"));
        for v in &rows {
            let dip = v["diplotype"].as_str().unwrap_or("").to_string();
            let sym = v["diplotypekey"]
                .as_object()
                .and_then(|o| o.keys().next())
                .cloned()
                .unwrap_or_default();
            if dip.is_empty() || sym.is_empty() {
                continue;
            }
            let parts: Vec<&str> = dip.split('/').collect();
            let zyg = if parts.len() == 2 && parts[0] == parts[1] {
                "homozygous"
            } else {
                "heterozygous"
            };
            let mut part = gene_part_of(&sym);
            part.push("diplotypes".into());
            part.push(norm(&dip));
            let record = diplo_minted < max_diplo;
            let g = gr.add(&NodeSpec {
                classid: CID_DIPLOTYPE,
                basin: &sym,
                part,
                isa: vec!["diplotype".into(), zyg.into()],
                kind: "diplotype",
                label: format!("{sym} {dip}"),
                record,
            });
            if record {
                if let Some(gene) = gene_g.get(&sym) {
                    gr.edge(g, "part_of", *gene); // diplotype → gene (recorded rows only)
                }
            }
            diplo_minted += 1;
        }
    } else {
        eprintln!("[ingest] {dipath} absent — skipping diplotype layer (fetched separately).");
    }

    // ── recommendations (is_a = classification; recommends from lookupkey, targets drug) ──
    let mut rec_n = 0usize;
    for v in load(&p("recommendation.json")) {
        let gid = v["guidelineid"].as_i64().unwrap_or(0);
        let did = v["drugid"].as_str().unwrap_or("").to_string();
        let class = v["classification"].as_str().unwrap_or("n/a").to_string();
        let lookup = v["lookupkey"].as_object();
        let lk = lookup
            .map(|o| {
                o.iter()
                    .map(|(k, val)| format!("{k}:{}", val.as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let rg = gr.add(&NodeSpec {
            classid: CID_REC,
            basin: &format!("rec:g{gid}"),
            part: vec!["recommendations".into(), format!("g{gid}"), norm(&did)],
            isa: vec!["recommendation".into(), norm(&class)],
            kind: "recommendation",
            label: format!("g{gid} {did} [{lk}] -> {class}"),
            record: true,
        });
        rec_n += 1;
        if let Some(dg) = drug_g.get(&did) {
            gr.edge(rg, "targets", *dg);
        }
        if let Some(o) = lookup {
            for (gene, val) in o {
                let key = val.as_str().unwrap_or("");
                if let Some(pg) = pheno_by_gr.get(&(gene.clone(), key.to_string())) {
                    gr.edge(*pg, "recommends", rg); // phenotype/allele-status → recommendation
                }
            }
        }
    }

    // ── gene ↔ drug pairs (the connected_to / out-of-family edge) ──
    for v in load(&p("pair.json")) {
        if v["removed"].as_bool().unwrap_or(false) {
            continue;
        }
        let sym = v["genesymbol"].as_str().unwrap_or("").to_string();
        let did = v["drugid"].as_str().unwrap_or("").to_string();
        if let (Some(gg), Some(dg)) = (gene_g.get(&sym), drug_g.get(&did)) {
            gr.edge(*gg, "pair", *dg);
        }
    }

    // ── write ──
    let mut nb = String::from("guid\tkind\tlabel\tpart_of\tis_a\n");
    for r in &gr.nodes {
        nb.push_str(r);
        nb.push('\n');
    }
    fs::write(format!("{out}/pgx_nodes.tsv"), nb).unwrap();
    let mut eb = String::from("src\trel\tdst\n");
    for r in &gr.edges {
        eb.push_str(r);
        eb.push('\n');
    }
    fs::write(format!("{out}/pgx_edges.tsv"), eb).unwrap();

    // ── summary + the "both axes route" demonstration ──
    eprintln!(
        "[ingest] minted {} GUIDs, {} collisions",
        gr.minted, gr.collisions
    );
    eprintln!(
        "[ingest] nodes recorded {} (diplotypes minted {}, recorded ≤{}), edges {}",
        gr.nodes.len(),
        diplo_minted,
        max_diplo,
        gr.edges.len()
    );
    eprintln!("[ingest] recommendations {rec_n}");

    // Both axes route: two no-function alleles in the CYP2 subfamily land on the SAME HHTL
    // path and are distinguished only by the family basin (the gene); a decreased-function
    // sibling diverges on the is_a low byte. The cascade is doing its job on both axes.
    let demo = [
        ("CYP2C19", "*2"),
        ("CYP2C9", "*2"),
        ("CYP2C19", "*1"),
        ("CYP2D6", "*4"),
    ];
    eprintln!("\n[ingest] (part_of:is_a) cascade demo — HEEL/HIP/TWIG = (part·is_a):");
    eprintln!(
        "  {:<14} {:<46} part_of                              is_a",
        "allele", "guid"
    );
    for (sym, name) in demo {
        if let Some(g) = allele_g.get(&(sym.to_string(), name.to_string())) {
            eprintln!(
                "  {:<14} {:<46} HEEL={:04x} HIP={:04x} TWIG={:04x}  fam={:06x}",
                format!("{sym} {name}"),
                g.hex(),
                g.heel,
                g.hip,
                g.twig,
                g.family
            );
        }
    }
    eprintln!("  ↑ part_of HIGH bytes c3/ec/f7 = pharmacogenome→CYP→CYP2, shared by all four (siblings route together).");
    eprintln!(
        "    is_a  LOW bytes  58=allele, then function: no_function=6f, decreased=d8, normal=07."
    );
    eprintln!("    CYP2C19*2 & CYP2D6*4 (both no-function, both CYP2) share the FULL path c358-ec6f-f76f — only family (gene) differs.");
}
