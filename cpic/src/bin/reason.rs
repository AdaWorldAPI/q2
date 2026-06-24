//! cpic reason — NARS reasoning over the REAL CPIC graph.
//!
//! A scenario `{gene, diplotype|phenotype, drug}` is resolved to a phenotype, matched against
//! the CPIC `recommendation` (via `lookupkey`), and chained by 2-hop NARS deduction
//! `diplotype → phenotype → recommendation` with CPIC-**authoritative** confidence
//! (`classification` + pair `cpiclevel` → c) — the same algebra as the cockpit `clinical.rs`
//! demo, but every edge is a published CPIC fact. The routable `(part_of:is_a)` GUID prefix of
//! each chain node is printed.
//!
//! Complex guidelines that CPIC flags as NOT simple diplotype→phenotype→rec (warfarin; any
//! multi-gene lookupkey) are surfaced, not silently auto-deduced.
//!
//! POC over published CPIC rules — NOT clinical decision support.
//!
//! Usage:  reason <gene> <diplotype|phenotype> <drug-name>     (no args → built-in demos)

use cpic::{
    basin, cascade3, gene_part_of, norm, NodeGuid, CID_DIPLOTYPE, CID_PHENOTYPE, CID_REC,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;

#[derive(Clone, Copy)]
struct Truth {
    f: f32,
    c: f32,
}
impl Truth {
    /// Canonical NARS deduction `A→B, B→C ⊢ A→C`: f = f1·f2, c = f1·f2·c1·c2.
    fn deduction(self, o: Truth) -> Truth {
        Truth {
            f: self.f * o.f,
            c: self.f * o.f * self.c * o.c,
        }
    }
    fn exp(self) -> f32 {
        self.c * (self.f - 0.5) + 0.5
    }
}

fn load(path: &str) -> Vec<Value> {
    let s = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// Routable `(part_of:is_a)` GUID prefix (classid-HEEL-HIP-TWIG-family); identity is the
/// basin-local mint allocated at ingest, shown as `··`.
fn addr(classid: u32, part: &[String], isa: &[String], basin_key: &str) -> String {
    let g = NodeGuid::mint(classid, cascade3(part), cascade3(isa), basin(basin_key), 0);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:06x}··",
        g.classid, g.heel, g.hip, g.twig, g.family
    )
}

/// Functional-status → activity rank (the transparent simple combination rule).
fn rank(status: &str) -> Option<f32> {
    let s = status.to_lowercase();
    if s.contains("no function") {
        Some(0.0)
    } else if s.contains("decreased") {
        Some(0.5)
    } else if s.contains("increased") {
        Some(2.0)
    } else if s.contains("normal") {
        Some(1.0)
    } else {
        None // uncertain / unknown — not resolvable by the simple rule
    }
}

/// Activity-score sum → metabolizer class (approximates CPIC's CYP2D6 activity-score bands;
/// allele-count genes like CYP2C19/TPMT fall out of the same thresholds).
fn class_from_score(score: f32) -> &'static str {
    if score <= 0.0 {
        "Poor"
    } else if score <= 1.0 {
        "Intermediate"
    } else if score <= 2.25 {
        "Normal"
    } else {
        "Ultrarapid"
    }
}

struct Kb {
    allele_status: HashMap<(String, String), String>, // (gene, allele) -> functional status
    gene_results: HashSet<(String, String)>,          // (gene, phenotype-string) that exist
    recs: Vec<Value>,
    drug_by_name: HashMap<String, (String, String)>, // lc name -> (drugid, display name)
    pair_level: HashMap<(String, String), String>,   // (gene, drugid) -> cpiclevel
    pair_gid: HashMap<(String, String), i64>,         // (gene, drugid) -> guidelineid
    guideline: HashMap<i64, (Option<String>, usize)>, // gid -> (notesonusage, gene-count)
}

fn load_kb(dir: &str) -> Kb {
    let p = |f: &str| format!("{dir}/{f}");
    let mut allele_status = HashMap::new();
    for v in load(&p("allele.json")) {
        if let (Some(g), Some(n)) = (v["genesymbol"].as_str(), v["name"].as_str()) {
            if let Some(s) = v["clinicalfunctionalstatus"].as_str() {
                allele_status.insert((g.to_string(), n.to_string()), s.to_string());
            }
        }
    }
    let mut gene_results = HashSet::new();
    for v in load(&p("gene_result.json")) {
        if let (Some(g), Some(r)) = (v["genesymbol"].as_str(), v["result"].as_str()) {
            gene_results.insert((g.to_string(), r.to_string()));
        }
    }
    let mut drug_by_name = HashMap::new();
    for v in load(&p("drug.json")) {
        if let (Some(id), Some(n)) = (v["drugid"].as_str(), v["name"].as_str()) {
            drug_by_name.insert(n.to_lowercase(), (id.to_string(), n.to_string()));
        }
    }
    let mut pair_level = HashMap::new();
    let mut pair_gid = HashMap::new();
    for v in load(&p("pair.json")) {
        if let (Some(g), Some(d)) = (v["genesymbol"].as_str(), v["drugid"].as_str()) {
            if let Some(l) = v["cpiclevel"].as_str() {
                pair_level.insert((g.to_string(), d.to_string()), l.to_string());
            }
            if let Some(gid) = v["guidelineid"].as_i64() {
                pair_gid.insert((g.to_string(), d.to_string()), gid);
            }
        }
    }
    let mut guideline = HashMap::new();
    for v in load(&p("guideline.json")) {
        if let Some(id) = v["id"].as_i64() {
            let notes = v["notesonusage"].as_str().map(|s| s.to_string());
            let ngenes = v["genes"].as_array().map(|a| a.len()).unwrap_or(0);
            guideline.insert(id, (notes, ngenes));
        }
    }
    Kb {
        allele_status,
        gene_results,
        recs: load(&p("recommendation.json")),
        drug_by_name,
        pair_level,
        pair_gid,
        guideline,
    }
}

/// Resolve the scenario's 2nd arg to a phenotype string + the t1 (input→phenotype) truth.
/// Direct phenotype/allele-status input → near-certain; diplotype → the simple rule (lower c).
fn resolve_phenotype(kb: &Kb, gene: &str, input: &str) -> Option<(String, Truth, String)> {
    // direct: the input already names a phenotype/allele-status CPIC knows for this gene
    if kb.gene_results.contains(&(gene.to_string(), input.to_string())) {
        return Some((input.to_string(), Truth { f: 1.0, c: 0.99 }, "direct phenotype".into()));
    }
    // diplotype: split into two alleles, combine functional ranks
    let alleles: Vec<&str> = input.split('/').collect();
    if alleles.len() != 2 {
        return None;
    }
    let mut score = 0.0;
    for al in &alleles {
        let st = kb.allele_status.get(&(gene.to_string(), al.trim().to_string()))?;
        score += rank(st)?;
    }
    let class = class_from_score(score);
    // match the resolved class to CPIC's vocabulary for this gene (Metabolizer vs Function)
    let cands = [
        format!("{class} Metabolizer"),
        format!("{class} Function"),
        if score == 0.5 { "Decreased Function".to_string() } else { String::new() },
    ];
    let pheno = cands
        .iter()
        .find(|c| !c.is_empty() && kb.gene_results.contains(&(gene.to_string(), (*c).clone())))?;
    // confidence: homozygous clean call > heterozygous/mixed
    let c = if alleles[0].trim() == alleles[1].trim() { 0.85 } else { 0.7 };
    Some((pheno.clone(), Truth { f: 1.0, c }, format!("simple rule (score {score})")))
}

fn reason(kb: &Kb, gene: &str, input: &str, drug: &str) {
    println!("\n══ {gene}  {input}  +  {drug} ═════════════════════════════════════");
    let Some((drugid, drugname)) = kb.drug_by_name.get(&drug.to_lowercase()).cloned() else {
        println!("  drug '{drug}' not in CPIC drug table.");
        return;
    };
    let Some((pheno, t1, how)) = resolve_phenotype(kb, gene, input) else {
        println!("  could not resolve a phenotype for {gene} {input}");
        println!("  (gene may use a non-standard scheme — provide the phenotype string directly).");
        return;
    };

    // match the CPIC recommendation: same drug, lookupkey[gene] == phenotype
    let mut hit: Option<&Value> = None;
    for r in &kb.recs {
        if r["drugid"].as_str() != Some(drugid.as_str()) {
            continue;
        }
        if let Some(lk) = r["lookupkey"].as_object() {
            if lk.get(gene).and_then(|v| v.as_str()) == Some(pheno.as_str()) {
                hit = Some(r);
                break;
            }
        }
    }
    let Some(rec) = hit else {
        println!("  phenotype {gene} {pheno}: no simple phenotype→rec for {drugname}.");
        // surface WHY: warfarin et al. use a dosing algorithm, flagged in the guideline notes
        if let Some(gid) = kb.pair_gid.get(&(gene.to_string(), drugid.clone())) {
            if let Some((Some(notes), _)) = kb.guideline.get(gid) {
                println!("  ⚠ COMPLEX guideline g{gid} (CPIC note): {notes}");
            }
        }
        return;
    };

    let gid = rec["guidelineid"].as_i64().unwrap_or(0);
    let class = rec["classification"].as_str().unwrap_or("n/a");
    let text = rec["drugrecommendation"].as_str().unwrap_or("");
    let level = kb
        .pair_level
        .get(&(gene.to_string(), drugid.clone()))
        .cloned()
        .unwrap_or_default();

    // t2 (phenotype → recommendation): f from classification, c from pair cpic level
    let class_f = match class {
        "Strong" => 0.95,
        "Moderate" => 0.8,
        "Optional" => 0.6,
        _ => 0.65,
    };
    let level_c = match level.as_str() {
        "A" => 0.95,
        "B" => 0.85,
        "C" => 0.65,
        "D" => 0.45,
        _ => 0.7,
    };
    let t2 = Truth { f: class_f, c: level_c };
    let t = t1.deduction(t2);

    // the (part_of:is_a) addresses of the chain
    let dip_is_diplo = input.contains('/');
    let p_dip = {
        let mut p = gene_part_of(gene);
        p.push("diplotypes".into());
        p.push(norm(input));
        p
    };
    let p_pheno = {
        let mut p = gene_part_of(gene);
        p.push("phenotypes".into());
        p.push(norm(&pheno));
        p
    };
    let p_rec = vec!["recommendations".into(), format!("g{gid}"), norm(&drugid)];

    if dip_is_diplo {
        println!(
            "  diplotype  {gene} {input:<10}  {}",
            addr(CID_DIPLOTYPE, &p_dip, &["diplotype".into(), "x".into()], gene)
        );
        println!("       │  maps_to  (t={:.2}/{:.2}, {how})", t1.f, t1.c);
    } else {
        println!("  input      {gene} {input}   ({how})");
    }
    println!(
        "  phenotype  {gene} {pheno:<22}  {}",
        addr(CID_PHENOTYPE, &p_pheno, &["phenotype".into(), norm(&pheno)], gene)
    );
    println!("       │  recommends  (class={class}, cpic level {level} → t={class_f:.2}/{level_c:.2})");
    println!(
        "  rec        g{gid} → {class:<8}            {}",
        addr(CID_REC, &p_rec, &["recommendation".into(), norm(class)], &format!("rec:g{gid}"))
    );
    println!("\n  ⊢ NARS deduction  {gene} {input} → recommendation");
    println!("    truth f={:.3} c={:.3}  (expectation {:.3})", t.f, t.c, t.exp());
    println!("    CPIC says: {text}");

    // surface the complexity flags CPIC itself raises
    if let Some((notes, ngenes)) = kb.guideline.get(&gid) {
        if let Some(n) = notes {
            println!("  ⚠ COMPLEX guideline (CPIC note): {n}");
        }
        if *ngenes > 1 {
            println!("  ⚠ MULTI-GENE guideline ({ngenes} genes) — single-gene deduction is partial; consult the full guideline.");
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // allow an optional leading data dir via env; default cpic/data, or `data` if run from cpic/
    let dir = std::env::var("CPIC_DATA").unwrap_or_else(|_| {
        if std::path::Path::new("data/gene.json").exists() {
            "data".into()
        } else {
            "cpic/data".into()
        }
    });
    let kb = load_kb(&dir);

    println!("cpic reason — NARS over real CPIC edges (classification + cpiclevel → confidence).");
    println!("POC over published CPIC rules — NOT clinical decision support.");

    if a.len() >= 4 {
        reason(&kb, &a[1], &a[2], &a[3]);
        return;
    }
    // built-in demos: clean 2-hop, direct 1-hop, multi-gene flag, complex-guideline flag
    reason(&kb, "CYP2C19", "*2/*2", "clopidogrel");
    reason(&kb, "HLA-B", "*57:01 positive", "abacavir");
    reason(&kb, "TPMT", "*3A/*3A", "azathioprine");
    reason(&kb, "CYP2C9", "*1/*1", "warfarin");
}
