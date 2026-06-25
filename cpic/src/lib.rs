//! cpic — shared `(part_of : is_a)` GUID toolkit for the CPIC pharmacogenomics POC.
//!
//! The canonical 16-byte `NodeGuid` (`classid·HEEL·HIP·TWIG·family·identity`, little-endian,
//! byte-identical to `lance_graph_contract::canonical_node::NodeGuid`) plus the cascade /
//! basin / mint helpers that `ingest`, `reason`, and `scan` all share.
//!
//! POC over published CPIC rules — NOT clinical decision support.

// ── classid: pharmacogenomics domain 0x0C (cf. anatomy 0x0A used by fma/converge) ──
pub const CID_GENE: u32 = 0x000C_0001;
pub const CID_ALLELE: u32 = 0x000C_0002;
pub const CID_DIPLOTYPE: u32 = 0x000C_0003;
pub const CID_PHENOTYPE: u32 = 0x000C_0004;
pub const CID_DRUG: u32 = 0x000C_0005;
pub const CID_REC: u32 = 0x000C_0006;

// ── FNV-1a (the same prefix-cascade generator the fma converge bin uses) ──
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
pub fn fnv1a(s: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// 3 cascade bytes from a slash distinguished-name: `byte[i]` = low byte of FNV-1a over the
/// cumulative prefix `seg[0]/../seg[i]`. Siblings sharing a leading prefix share leading bytes
/// → prefix-routable. Fewer than 3 segments reuse the deepest prefix.
pub fn cascade3(segs: &[String]) -> [u8; 3] {
    let mut out = [0u8; 3];
    if segs.is_empty() {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let depth = i.min(segs.len() - 1);
        *slot = (fnv1a(&segs[..=depth].join("/")) & 0xFF) as u8;
    }
    out
}

/// Family basin from a key (gene symbol, ATC root, guideline id). `family == 0` is the canon's
/// dormant default basin, so never mint it — bump a zero hash to 1.
pub fn basin(key: &str) -> u32 {
    let f = (fnv1a(key) & 0xFF_FFFF) as u32;
    if f == 0 {
        1
    } else {
        f
    }
}

/// 2^32/φ, odd ⇒ a bijective (collision-free) multiplicative mint over a per-basin counter.
pub const GOLDEN32: u32 = 0x9E37_79B9;

/// `identity(n) = n × GOLDEN32 mod 2^24` — bijective over a per-basin counter `n ∈ [1, 2^24)`.
pub fn mint_identity(counter: u32) -> u32 {
    counter.wrapping_mul(GOLDEN32) & 0xFF_FFFF
}

#[derive(Clone, Copy)]
pub struct NodeGuid {
    pub classid: u32,
    pub heel: u16,
    pub hip: u16,
    pub twig: u16,
    pub family: u32,   // u24
    pub identity: u32, // u24
}
impl NodeGuid {
    /// `part[i]` = part_of cascade byte (HIGH), `isa[i]` = is_a cascade byte (LOW) of tier i.
    pub fn mint(classid: u32, part: [u8; 3], isa: [u8; 3], family: u32, identity: u32) -> Self {
        let tile = |p: u8, i: u8| ((p as u16) << 8) | (i as u16); // (part_of : is_a)
        NodeGuid {
            classid,
            heel: tile(part[0], isa[0]),
            hip: tile(part[1], isa[1]),
            twig: tile(part[2], isa[2]),
            family: family & 0xFF_FFFF,
            identity: identity & 0xFF_FFFF,
        }
    }
    /// Canonical dotted GUID — the OGAR dash-groups (last group = family|identity).
    pub fn hex(&self) -> String {
        format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:06x}{:06x}",
            self.classid, self.heel, self.hip, self.twig, self.family, self.identity
        )
    }
    /// 16-byte little-endian key, byte-identical to `canonical_node::NodeGuid`.
    pub fn key16(&self) -> [u8; 16] {
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&self.classid.to_le_bytes());
        k[4..6].copy_from_slice(&self.heel.to_le_bytes());
        k[6..8].copy_from_slice(&self.hip.to_le_bytes());
        k[8..10].copy_from_slice(&self.twig.to_le_bytes());
        k[10..13].copy_from_slice(&self.family.to_le_bytes()[..3]);
        k[13..16].copy_from_slice(&self.identity.to_le_bytes()[..3]);
        k
    }
}

/// Letters before the first non-alphabetic char (gene-family heuristic for non-CYP/HLA genes).
pub fn alpha_prefix(s: &str) -> String {
    let p: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if p.is_empty() {
        s.to_string()
    } else {
        p
    }
}

/// The gene's `part_of` partonomy: pharmacogenome → family → … → gene. CYP genes cascade
/// family→subfamily→gene (CYP / CYP2 / CYP2C / CYP2C19) so siblings prefix-route deeply.
pub fn gene_part_of(sym: &str) -> Vec<String> {
    let mut p = vec!["pharmacogenome".to_string()];
    if sym.starts_with("CYP") {
        p.push("CYP".into());
        if sym.len() >= 4 {
            p.push(sym[..4].into());
        }
        if sym.len() >= 5 {
            p.push(sym[..5].into());
        }
        p.push(sym.into());
    } else if sym.starts_with("HLA") {
        p.push("HLA".into());
        p.push(sym.into());
    } else {
        p.push(alpha_prefix(sym));
        p.push(sym.into());
    }
    p
}

/// DN-segment normalization: lowercase, non-alnum → '_', trimmed.
pub fn norm(s: &str) -> String {
    let mapped: String = s
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    mapped.trim_matches('_').to_string()
}

/// `allele.clinicalfunctionalstatus` → the is_a functional class.
pub fn func_class(status: Option<&str>) -> String {
    let s = status.unwrap_or("").to_lowercase();
    if s.contains("no function") {
        "no_function"
    } else if s.contains("decreased") {
        "decreased_function"
    } else if s.contains("increased") {
        "increased_function"
    } else if s.contains("normal") {
        "normal_function"
    } else if s.is_empty() || s.contains("uncertain") || s.contains("unknown") {
        "uncertain_function"
    } else {
        "other_function"
    }
    .to_string()
}

// ════════════════════════════════════════════════════════════════════════════════════
//  Reasoning over the real CPIC graph — lives HERE (not just in `bin/reason.rs`) so the
//  CLI and a server endpoint (cockpit `/api/cpic/reason`) share ONE implementation.
//  `{gene, diplotype|phenotype, drug}` → phenotype → recommendation, 2-hop NARS deduction
//  with CPIC-authoritative confidence (`classification`→f, pair `cpiclevel`→c).
// ════════════════════════════════════════════════════════════════════════════════════

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// NARS truth `(f, c)`. `deduction` is the canonical `A→B, B→C ⊢ A→C` (`f=f1·f2, c=f1·f2·c1·c2`).
#[derive(Clone, Copy)]
pub struct Truth {
    pub f: f32,
    pub c: f32,
}
impl Truth {
    pub fn deduction(self, o: Truth) -> Truth {
        Truth { f: self.f * o.f, c: self.f * o.f * self.c * o.c }
    }
    pub fn exp(self) -> f32 {
        self.c * (self.f - 0.5) + 0.5
    }
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
        None
    }
}

/// Activity-score sum → metabolizer class (CYP2D6 activity-score bands; allele-count genes
/// like CYP2C19/TPMT fall out of the same thresholds).
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

/// Routable `(part_of:is_a)` GUID prefix (classid-HEEL-HIP-TWIG-family); identity (the
/// basin-local mint allocated at ingest) is shown as `··`.
fn addr(classid: u32, part: &[String], isa: &[String], basin_key: &str) -> String {
    let g = NodeGuid::mint(classid, cascade3(part), cascade3(isa), basin(basin_key), 0);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:06x}··",
        g.classid, g.heel, g.hip, g.twig, g.family
    )
}

/// The CPIC knowledge base parsed from the six published tables `reason` consults.
pub struct Kb {
    allele_status: HashMap<(String, String), String>, // (gene, allele) -> functional status
    gene_results: HashSet<(String, String)>,          // (gene, phenotype-string) that exist
    recs: Vec<Value>,
    drug_by_name: HashMap<String, (String, String)>, // lc name -> (drugid, display name)
    pair_level: HashMap<(String, String), String>,   // (gene, drugid) -> cpiclevel
    pair_gid: HashMap<(String, String), i64>,         // (gene, drugid) -> guidelineid
    guideline: HashMap<i64, (Option<String>, usize)>, // gid -> (notesonusage, gene-count)
}

impl Kb {
    /// Parse the KB from the six table JSON strings (each a JSON array of objects).
    pub fn from_jsons(
        allele: &str,
        gene_result: &str,
        drug: &str,
        pair: &str,
        guideline: &str,
        recommendation: &str,
    ) -> Self {
        let arr = |s: &str| -> Vec<Value> { serde_json::from_str(s).unwrap_or_default() };
        let mut allele_status = HashMap::new();
        for v in arr(allele) {
            if let (Some(g), Some(n)) = (v["genesymbol"].as_str(), v["name"].as_str()) {
                if let Some(st) = v["clinicalfunctionalstatus"].as_str() {
                    allele_status.insert((g.to_string(), n.to_string()), st.to_string());
                }
            }
        }
        let mut gene_results = HashSet::new();
        for v in arr(gene_result) {
            if let (Some(g), Some(r)) = (v["genesymbol"].as_str(), v["result"].as_str()) {
                gene_results.insert((g.to_string(), r.to_string()));
            }
        }
        let mut drug_by_name = HashMap::new();
        for v in arr(drug) {
            if let (Some(id), Some(n)) = (v["drugid"].as_str(), v["name"].as_str()) {
                drug_by_name.insert(n.to_lowercase(), (id.to_string(), n.to_string()));
            }
        }
        let mut pair_level = HashMap::new();
        let mut pair_gid = HashMap::new();
        for v in arr(pair) {
            if let (Some(g), Some(d)) = (v["genesymbol"].as_str(), v["drugid"].as_str()) {
                if let Some(l) = v["cpiclevel"].as_str() {
                    pair_level.insert((g.to_string(), d.to_string()), l.to_string());
                }
                if let Some(gid) = v["guidelineid"].as_i64() {
                    pair_gid.insert((g.to_string(), d.to_string()), gid);
                }
            }
        }
        let mut gl = HashMap::new();
        for v in arr(guideline) {
            if let Some(id) = v["id"].as_i64() {
                let notes = v["notesonusage"].as_str().map(|s| s.to_string());
                let ngenes = v["genes"].as_array().map(|a| a.len()).unwrap_or(0);
                gl.insert(id, (notes, ngenes));
            }
        }
        Kb {
            allele_status,
            gene_results,
            recs: arr(recommendation),
            drug_by_name,
            pair_level,
            pair_gid,
            guideline: gl,
        }
    }

    /// Load the six tables from a data directory (the CLI path).
    pub fn load(dir: &str) -> Self {
        let r = |f: &str| {
            std::fs::read_to_string(format!("{dir}/{f}"))
                .unwrap_or_else(|e| panic!("read {dir}/{f}: {e}"))
        };
        Self::from_jsons(
            &r("allele.json"),
            &r("gene_result.json"),
            &r("drug.json"),
            &r("pair.json"),
            &r("guideline.json"),
            &r("recommendation.json"),
        )
    }

    /// The six tables baked into the binary — the server / default path, no runtime files.
    pub fn embedded() -> Self {
        Self::from_jsons(
            include_str!("../data/allele.json"),
            include_str!("../data/gene_result.json"),
            include_str!("../data/drug.json"),
            include_str!("../data/pair.json"),
            include_str!("../data/guideline.json"),
            include_str!("../data/recommendation.json"),
        )
    }

    /// Sorted gene symbols + drug names (for the cockpit's pickers).
    pub fn catalog(&self) -> Catalog {
        let mut genes: Vec<String> = self.gene_results.iter().map(|(g, _)| g.clone()).collect();
        genes.sort();
        genes.dedup();
        let mut drugs: Vec<String> = self.drug_by_name.values().map(|(_, n)| n.clone()).collect();
        drugs.sort();
        drugs.dedup();
        Catalog { genes, drugs }
    }
}

/// Pickable genes + drugs for the frontend dropdowns.
#[derive(Clone, Serialize)]
pub struct Catalog {
    pub genes: Vec<String>,
    pub drugs: Vec<String>,
}

/// One node of the reasoned chain, carrying its routable `(part_of:is_a)` GUID prefix.
#[derive(Clone, Serialize)]
pub struct ChainNode {
    pub role: String, // "diplotype" | "phenotype" | "recommendation"
    pub label: String,
    pub guid: String,
}

/// The structured reasoning result — what the CLI prints and the cockpit renders.
#[derive(Clone, Serialize)]
pub struct Outcome {
    pub gene: String,
    pub input: String,
    pub drug: String,
    pub resolved: bool,
    pub phenotype: Option<String>,
    pub how: Option<String>,
    pub chain: Vec<ChainNode>,
    pub classification: Option<String>,
    pub cpic_level: Option<String>,
    pub truth_f: f32,
    pub truth_c: f32,
    pub truth_exp: f32,
    pub recommendation: Option<String>,
    pub flags: Vec<String>,
    pub disclaimer: String,
}

fn new_outcome(gene: &str, input: &str, drug: &str) -> Outcome {
    Outcome {
        gene: gene.into(),
        input: input.into(),
        drug: drug.into(),
        resolved: false,
        phenotype: None,
        how: None,
        chain: vec![],
        classification: None,
        cpic_level: None,
        truth_f: 0.0,
        truth_c: 0.0,
        truth_exp: 0.0,
        recommendation: None,
        flags: vec![],
        disclaimer: "POC over published CPIC rules — NOT clinical decision support.".into(),
    }
}

/// Resolve the 2nd arg to a phenotype + the t1 (input→phenotype) truth. A direct phenotype is
/// near-certain; a diplotype is combined by the transparent simple allele-function rule (lower c).
fn resolve_phenotype(kb: &Kb, gene: &str, input: &str) -> Option<(String, Truth, String)> {
    if kb.gene_results.contains(&(gene.to_string(), input.to_string())) {
        return Some((input.to_string(), Truth { f: 1.0, c: 0.99 }, "direct phenotype".into()));
    }
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
    let cands = [
        format!("{class} Metabolizer"),
        format!("{class} Function"),
        if score == 0.5 { "Decreased Function".to_string() } else { String::new() },
    ];
    let pheno = cands
        .iter()
        .find(|c| !c.is_empty() && kb.gene_results.contains(&(gene.to_string(), (*c).clone())))?;
    let c = if alleles[0].trim() == alleles[1].trim() { 0.85 } else { 0.7 };
    Some((pheno.clone(), Truth { f: 1.0, c }, format!("simple rule (score {score})")))
}

/// Reason a `{gene, diplotype|phenotype, drug}` scenario over the real CPIC graph → `Outcome`.
/// `resolved == false` means CPIC has no simple phenotype→recommendation (the `flags` say why —
/// e.g. a complex / multi-gene guideline), which the POC surfaces instead of fabricating.
pub fn reason(kb: &Kb, gene: &str, input: &str, drug: &str) -> Outcome {
    let mut o = new_outcome(gene, input, drug);

    let Some((drugid, _drugname)) = kb.drug_by_name.get(&drug.to_lowercase()).cloned() else {
        o.flags.push(format!("drug '{drug}' is not in the CPIC drug table"));
        return o;
    };
    let Some((pheno, t1, how)) = resolve_phenotype(kb, gene, input) else {
        o.flags
            .push(format!("could not resolve a phenotype for {gene} {input} — provide the phenotype string directly"));
        return o;
    };
    o.phenotype = Some(pheno.clone());
    o.how = Some(how.clone());

    // diplotype + phenotype chain nodes (emitted once a phenotype resolves)
    if input.contains('/') {
        let mut p = gene_part_of(gene);
        p.push("diplotypes".into());
        p.push(norm(input));
        o.chain.push(ChainNode {
            role: "diplotype".into(),
            label: format!("{gene} {input}"),
            guid: addr(CID_DIPLOTYPE, &p, &["diplotype".into(), "x".into()], gene),
        });
    }
    {
        let mut p = gene_part_of(gene);
        p.push("phenotypes".into());
        p.push(norm(&pheno));
        o.chain.push(ChainNode {
            role: "phenotype".into(),
            label: format!("{gene} {pheno}"),
            guid: addr(CID_PHENOTYPE, &p, &["phenotype".into(), norm(&pheno)], gene),
        });
    }

    // match the CPIC recommendation: same drug, lookupkey[gene] == phenotype
    let rec = kb.recs.iter().find(|r| {
        r["drugid"].as_str() == Some(drugid.as_str())
            && r["lookupkey"]
                .as_object()
                .and_then(|lk| lk.get(gene))
                .and_then(|v| v.as_str())
                == Some(pheno.as_str())
    });

    let Some(rec) = rec else {
        o.flags
            .push(format!("no simple phenotype→recommendation for {gene} {pheno} + {drug}"));
        if let Some(gid) = kb.pair_gid.get(&(gene.to_string(), drugid.clone())) {
            if let Some((Some(notes), _)) = kb.guideline.get(gid) {
                o.flags.push(format!("COMPLEX guideline g{gid} (CPIC): {notes}"));
            }
        }
        return o;
    };

    let gid = rec["guidelineid"].as_i64().unwrap_or(0);
    let class = rec["classification"].as_str().unwrap_or("n/a").to_string();
    let text = rec["drugrecommendation"].as_str().unwrap_or("").to_string();
    let level = kb
        .pair_level
        .get(&(gene.to_string(), drugid.clone()))
        .cloned()
        .unwrap_or_default();

    let class_f = match class.as_str() {
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
    let t = t1.deduction(Truth { f: class_f, c: level_c });

    {
        let p = vec!["recommendations".into(), format!("g{gid}"), norm(&drugid)];
        o.chain.push(ChainNode {
            role: "recommendation".into(),
            label: format!("g{gid} → {class}"),
            guid: addr(CID_REC, &p, &["recommendation".into(), norm(&class)], &format!("rec:g{gid}")),
        });
    }

    o.resolved = true;
    o.classification = Some(class);
    o.cpic_level = Some(level);
    o.truth_f = t.f;
    o.truth_c = t.c;
    o.truth_exp = t.exp();
    o.recommendation = Some(text);

    if let Some((notes, ngenes)) = kb.guideline.get(&gid) {
        if let Some(n) = notes {
            o.flags.push(format!("COMPLEX guideline (CPIC note): {n}"));
        }
        if *ngenes > 1 {
            o.flags
                .push(format!("MULTI-GENE guideline ({ngenes} genes) — single-gene deduction is partial"));
        }
    }
    o
}

#[cfg(test)]
mod reason_tests {
    use super::*;

    fn kb() -> Kb {
        Kb::embedded()
    }

    #[test]
    fn cyp2c19_poor_metabolizer_clopidogrel_strong() {
        let o = reason(&kb(), "CYP2C19", "*2/*2", "clopidogrel");
        assert!(o.resolved, "flags: {:?}", o.flags);
        assert_eq!(o.phenotype.as_deref(), Some("Poor Metabolizer"));
        assert_eq!(o.classification.as_deref(), Some("Strong"));
        assert!(o.truth_c > 0.0 && o.truth_f > 0.0);
        assert_eq!(o.chain.len(), 3); // diplotype, phenotype, recommendation
        assert!(o.recommendation.unwrap().to_lowercase().contains("clopidogrel"));
    }

    #[test]
    fn unknown_drug_is_flagged_not_fabricated() {
        let o = reason(&kb(), "CYP2C19", "*2/*2", "definitely_not_a_drug");
        assert!(!o.resolved);
        assert!(o.flags.iter().any(|f| f.contains("not in the CPIC drug table")));
    }
}
