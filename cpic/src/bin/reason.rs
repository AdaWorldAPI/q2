//! cpic reason — NARS reasoning over the REAL CPIC graph (thin CLI over the `cpic` lib).
//!
//! A scenario `{gene, diplotype|phenotype, drug}` is resolved to a phenotype, matched against
//! the CPIC `recommendation` (via `lookupkey`), and chained by 2-hop NARS deduction
//! `diplotype → phenotype → recommendation` with CPIC-**authoritative** confidence
//! (`classification` + pair `cpiclevel` → c). The reasoning itself lives in `cpic::reason`
//! so this CLI and the cockpit `/api/cpic/reason` endpoint share ONE implementation; this
//! binary only loads the tables and renders the structured `Outcome`.
//!
//! Complex guidelines that CPIC flags as NOT a simple diplotype→phenotype→rec (warfarin; any
//! multi-gene lookupkey) come back `resolved == false` with `flags` explaining why — surfaced,
//! not silently auto-deduced.
//!
//! POC over published CPIC rules — NOT clinical decision support.
//!
//! Usage:  reason <gene> <diplotype|phenotype> <drug-name>     (no args → built-in demos)

use cpic::{reason, Kb, Outcome};

/// Render a structured `Outcome` to the console — the same chain the cockpit draws visually.
fn print_outcome(o: &Outcome) {
    println!(
        "\n══ {}  {}  +  {} ═════════════════════════════════════",
        o.gene, o.input, o.drug
    );
    if !o.resolved {
        // unresolved: the flags say WHY (unknown drug, unresolvable phenotype, complex /
        // multi-gene guideline). The POC surfaces the reason; it never fabricates a rec.
        for f in &o.flags {
            println!("  ⚠ {f}");
        }
        return;
    }
    // the reasoned chain — each node carries its routable (part_of:is_a) GUID prefix
    for n in &o.chain {
        println!("  {:<14} {:<26}  {}", n.role, n.label, n.guid);
    }
    if let (Some(class), Some(level)) = (&o.classification, &o.cpic_level) {
        println!("       │  recommends  (class={class}, cpic level {level})");
    }
    println!(
        "\n  ⊢ NARS deduction  {} {} → recommendation",
        o.gene, o.input
    );
    println!(
        "    truth f={:.3} c={:.3}  (expectation {:.3})",
        o.truth_f, o.truth_c, o.truth_exp
    );
    if let Some(text) = &o.recommendation {
        println!("    CPIC says: {text}");
    }
    for f in &o.flags {
        println!("  ⚠ {f}");
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // optional data dir via env; default `data` when run from cpic/, else `cpic/data`.
    let dir = std::env::var("CPIC_DATA").unwrap_or_else(|_| {
        if std::path::Path::new("data/gene.json").exists() {
            "data".into()
        } else {
            "cpic/data".into()
        }
    });
    let kb = Kb::load(&dir);

    println!("cpic reason — NARS over real CPIC edges (classification + cpiclevel → confidence).");
    println!("POC over published CPIC rules — NOT clinical decision support.");

    if a.len() >= 4 {
        print_outcome(&reason(&kb, &a[1], &a[2], &a[3]));
        return;
    }
    // built-in demos: clean 2-hop, direct 1-hop, multi-gene flag, complex-guideline flag
    for (g, i, d) in [
        ("CYP2C19", "*2/*2", "clopidogrel"),
        ("HLA-B", "*57:01 positive", "abacavir"),
        ("TPMT", "*3A/*3A", "azathioprine"),
        ("CYP2C9", "*1/*1", "warfarin"),
    ] {
        print_outcome(&reason(&kb, g, i, d));
    }
}
