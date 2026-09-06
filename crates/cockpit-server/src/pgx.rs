//! CPIC pharmacogenomics — the `/cpic` cockpit's gene-first panel, backed by the standalone
//! `cpic` crate's `reason()` over the REAL published CPIC tables (allele, gene_result, drug,
//! pair, guideline, recommendation), baked into the binary via `cpic::Kb::embedded()` so the
//! endpoint needs no runtime data files (Railway-safe).
//!
//! A scenario `{gene, input, drug}` resolves to a phenotype and chains
//! `diplotype → phenotype → recommendation` by 2-hop NARS deduction with CPIC-**authoritative**
//! confidence (`classification`→f, pair `cpiclevel`→c). Each chain node carries its routable
//! `(part_of:is_a)` GUID prefix. This is the SAME `cpic::reason` the `reason` CLI calls —
//! one engine, two surfaces (the CLAUDE module-named `cpic` crate is reached unambiguously
//! here; this module is `pgx` precisely so it does not shadow that dependency).
//!
//! POC over published CPIC rules — NOT clinical decision support. The frontend shows that
//! disclaimer in-view.

use std::sync::LazyLock;

use cpic::{Catalog, Kb, Outcome, reason};

/// The CPIC knowledge base, parsed once from the tables baked into the binary
/// (`include_str!` of `cpic/data/*.json`). No runtime files; safe on a fresh container.
static KB: LazyLock<Kb> = LazyLock::new(Kb::embedded);

/// `POST /api/cpic/reason` request — `{gene, input, drug}`. `input` is a diplotype like
/// `*2/*2`, or a phenotype / allele-status string CPIC already knows for the gene.
#[derive(serde::Deserialize)]
pub struct CpicScenario {
    #[serde(default)]
    pub gene: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub drug: String,
}

/// `POST /api/cpic/reason` → the structured `Outcome` (`resolved`, `phenotype`, `chain[]` with
/// routable GUIDs, `classification`, `cpic_level`, `truth_{f,c,exp}`, `recommendation`, `flags`,
/// `disclaimer`). When CPIC has no simple phenotype→rec, `resolved == false` and `flags` say why
/// (unknown drug, unresolvable phenotype, complex / multi-gene guideline) — never fabricated.
pub async fn cpic_reason_handler(axum::Json(sc): axum::Json<CpicScenario>) -> axum::Json<Outcome> {
    axum::Json(reason(&KB, &sc.gene, &sc.input, &sc.drug))
}

/// `GET /api/cpic/catalog` → sorted gene + drug pick-lists for the cockpit's dropdowns.
pub async fn cpic_catalog_handler() -> axum::Json<Catalog> {
    axum::Json(KB.catalog())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guards the integration wiring: the baked KB (`include_str!` paths resolving against the
    // cpic crate) loads in the cockpit-server build, and a canonical scenario reasons through.
    #[test]
    fn embedded_kb_reasons_and_has_a_catalog() {
        let o = reason(&KB, "CYP2C19", "*2/*2", "clopidogrel");
        assert!(o.resolved, "flags: {:?}", o.flags);
        assert_eq!(o.phenotype.as_deref(), Some("Poor Metabolizer"));
        let cat = KB.catalog();
        assert!(!cat.genes.is_empty() && !cat.drugs.is_empty());
    }
}
