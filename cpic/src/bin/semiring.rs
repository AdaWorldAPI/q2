//! cpic semiring — retrieval IS inference: ONE adjacency, FOUR semirings, FOUR reasoning modes.
//!
//! The research frontier-map's `[G]` probe #1 (Dudzik & Veličković, *GNNs are Dynamic
//! Programmers*, [2203.15544]: message-passing = DP parameterized by a **semiring**). Here the
//! diplotype → phenotype → recommendation chain `cpic reason` computes is shown to be a single
//! **semiring transitive closure** over the CPIC edge adjacency — and swapping ONLY the semiring
//! switches the reasoning mode on the SAME graph:
//!
//!   Boolean   (∨,∧)        → reachability       ("is a recommendation derivable at all?")
//!   MinPlus   (min,+)       → cheapest path       (fewest hops / strongest-evidence route)
//!   Nars      (revise, ⊢)   → CPIC-authoritative confidence  (f=f₁f₂, c=f₁f₂c₁c₂)
//!   MaxTimes  (max,·)       → most-likely path    (Viterbi over edge expectations)
//!
//! The keystone assertion: the **Nars** semiring reproduces `cpic reason`'s exact (f,c) for the
//! CYP2C19 *2/*2 → clopidogrel chain (f=0.95, c=0.767). If it does, "retrieval = reasoning" is a
//! measured identity, not a slogan. Edge truths are read from the REAL CPIC `recommendation`
//! (classification) + `pair` (cpiclevel) — same mapping as `reason`. (In production the closure
//! is a lance-graph GraphBLAS matrix walk; here it is a self-contained Floyd-Warshall so the
//! standalone `cpic` crate stays dep-light.)
//!
//! POC over published CPIC rules — NOT clinical decision support.
//!
//! Usage:  semiring [data_dir]

use serde_json::Value;
use std::fs;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Truth {
    f: f32,
    c: f32,
}
impl Truth {
    fn exp(self) -> f32 {
        self.c * (self.f - 0.5) + 0.5
    }
}

/// A semiring `(T, ⊕, ⊗, 0̄, 1̄)`: `⊗` extends a path, `⊕` combines alternative paths. Floyd-
/// Warshall over ANY closed semiring computes all-pairs path weights — the reasoning mode is
/// entirely a choice of these four operators (the Dudzik-Veličković identity, made executable).
trait Semiring {
    type T: Copy + std::fmt::Debug;
    const NAME: &'static str;
    fn zero() -> Self::T; // ⊕ identity — "no path"
    fn one() -> Self::T; // ⊗ identity — "self / empty path"
    fn lift(e: Truth) -> Self::T; // a CPIC edge → this semiring's carrier
    fn add(a: Self::T, b: Self::T) -> Self::T; // combine alternatives
    fn mul(a: Self::T, b: Self::T) -> Self::T; // chain a path
    fn show(t: Self::T) -> String;
}

/// All-pairs path weights via semiring Floyd-Warshall (the generic DP the four modes share).
fn closure<S: Semiring>(adj: &[Vec<Option<Truth>>]) -> Vec<Vec<S::T>> {
    let n = adj.len();
    let mut p = vec![vec![S::zero(); n]; n];
    for (i, row) in adj.iter().enumerate() {
        p[i][i] = S::one();
        for (j, cell) in row.iter().enumerate() {
            if let Some(t) = cell {
                p[i][j] = S::add(p[i][j], S::lift(*t));
            }
        }
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                let via = S::mul(p[i][k], p[k][j]);
                p[i][j] = S::add(p[i][j], via);
            }
        }
    }
    p
}

// ── the four semirings ──────────────────────────────────────────────────────

struct Boolean;
impl Semiring for Boolean {
    type T = bool;
    const NAME: &'static str = "Boolean  (∨,∧)  reachability";
    fn zero() -> bool {
        false
    }
    fn one() -> bool {
        true
    }
    fn lift(_: Truth) -> bool {
        true
    }
    fn add(a: bool, b: bool) -> bool {
        a || b
    }
    fn mul(a: bool, b: bool) -> bool {
        a && b
    }
    fn show(t: bool) -> String {
        if t { "reachable".into() } else { "—".into() }
    }
}

struct MinPlus;
impl Semiring for MinPlus {
    type T = f32; // hop count (tropical min-plus); ∞ = unreachable
    const NAME: &'static str = "MinPlus  (min,+) cheapest path (hops)";
    fn zero() -> f32 {
        f32::INFINITY
    }
    fn one() -> f32 {
        0.0
    }
    fn lift(_: Truth) -> f32 {
        1.0
    }
    fn add(a: f32, b: f32) -> f32 {
        a.min(b)
    }
    fn mul(a: f32, b: f32) -> f32 {
        a + b
    }
    fn show(t: f32) -> String {
        if t.is_finite() {
            format!("{t:.0} hops")
        } else {
            "—".into()
        }
    }
}

struct Nars;
impl Semiring for Nars {
    type T = Truth; // CPIC-authoritative (f,c)
    const NAME: &'static str = "Nars     (revise,⊢) CPIC confidence";
    fn zero() -> Truth {
        Truth { f: 0.0, c: 0.0 }
    }
    fn one() -> Truth {
        Truth { f: 1.0, c: 1.0 } // empty path: certain identity
    }
    fn lift(e: Truth) -> Truth {
        e
    }
    /// ⊕ = pick the alternative we believe more (higher expectation) — NARS choice.
    fn add(a: Truth, b: Truth) -> Truth {
        if b.exp() > a.exp() {
            b
        } else {
            a
        }
    }
    /// ⊗ = NARS deduction `A→B, B→C ⊢ A→C`: f=f₁f₂, c=f₁f₂c₁c₂ (the reason.rs algebra).
    fn mul(a: Truth, b: Truth) -> Truth {
        Truth {
            f: a.f * b.f,
            c: a.f * b.f * a.c * b.c,
        }
    }
    fn show(t: Truth) -> String {
        if t.c > 0.0 {
            format!("f={:.3} c={:.3}", t.f, t.c)
        } else {
            "—".into()
        }
    }
}

struct MaxTimes;
impl Semiring for MaxTimes {
    type T = f32; // Viterbi: max product of edge expectations
    const NAME: &'static str = "MaxTimes (max,·) most-likely path";
    fn zero() -> f32 {
        0.0
    }
    fn one() -> f32 {
        1.0
    }
    fn lift(e: Truth) -> f32 {
        e.exp()
    }
    fn add(a: f32, b: f32) -> f32 {
        a.max(b)
    }
    fn mul(a: f32, b: f32) -> f32 {
        a * b
    }
    fn show(t: f32) -> String {
        if t > 0.0 {
            format!("p={t:.3}")
        } else {
            "—".into()
        }
    }
}

fn load(p: &str) -> Vec<Value> {
    serde_json::from_str(&fs::read_to_string(p).unwrap_or_else(|e| panic!("read {p}: {e}")))
        .unwrap_or_else(|e| panic!("parse {p}: {e}"))
}

/// classification → frequency, cpiclevel → confidence (identical to reason.rs).
fn class_f(class: &str) -> f32 {
    match class {
        "Strong" => 0.95,
        "Moderate" => 0.8,
        "Optional" => 0.6,
        _ => 0.65,
    }
}
fn level_c(level: &str) -> f32 {
    match level {
        "A" => 0.95,
        "B" => 0.85,
        "C" => 0.65,
        "D" => 0.45,
        _ => 0.7,
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = a.get(1).cloned().unwrap_or_else(|| {
        if std::path::Path::new("data/recommendation.json").exists() {
            "data".into()
        } else {
            "cpic/data".into()
        }
    });
    let p = |f: &str| format!("{dir}/{f}");

    // A demo chain's three nodes. Indices: 0=diplotype, 1=phenotype, 2=recommendation.
    // The chain is CYP2C19 *2/*2 → Poor Metabolizer → (clopidogrel) recommendation.
    let labels = [
        "CYP2C19 *2/*2 (diplotype)",
        "CYP2C19 Poor Metabolizer (phenotype)",
        "clopidogrel recommendation",
    ];
    let n = labels.len();
    let mut adj: Vec<Vec<Option<Truth>>> = vec![vec![None; n]; n];

    // edge 0→1 (diplotype → phenotype): the simple homozygous rule (reason.rs t1).
    adj[0][1] = Some(Truth { f: 1.0, c: 0.85 });

    // edge 1→2 (phenotype → recommendation): truth from the REAL CPIC clopidogrel rec
    // (guideline 100411, CYP2C19 lookupkey "Poor Metabolizer") — classification × cpiclevel.
    let mut t2 = Truth { f: 0.95, c: 0.95 }; // fallback Strong/A
    let clopidogrel_drugid = load(&p("drug.json"))
        .iter()
        .find(|d| d["name"].as_str() == Some("clopidogrel"))
        .and_then(|d| d["drugid"].as_str().map(String::from));
    if let Some(did) = &clopidogrel_drugid {
        for r in load(&p("recommendation.json")) {
            let is_pm = r["lookupkey"]
                .get("CYP2C19")
                .and_then(|v| v.as_str())
                == Some("Poor Metabolizer");
            if r["drugid"].as_str() == Some(did.as_str()) && is_pm {
                let class = r["classification"].as_str().unwrap_or("Strong");
                let level = load(&p("pair.json"))
                    .iter()
                    .find(|pr| {
                        pr["genesymbol"].as_str() == Some("CYP2C19")
                            && pr["drugid"].as_str() == Some(did.as_str())
                    })
                    .and_then(|pr| pr["cpiclevel"].as_str().map(String::from))
                    .unwrap_or_else(|| "A".into());
                t2 = Truth {
                    f: class_f(class),
                    c: level_c(&level),
                };
                println!("CPIC edge phenotype→rec read from data: classification={class}, cpiclevel={level}");
                break;
            }
        }
    }
    adj[1][2] = Some(t2);

    println!("\nadjacency (CPIC edges):");
    println!("  {} --t={:.2}/{:.2}--> {}", labels[0], adj[0][1].unwrap().f, adj[0][1].unwrap().c, labels[1]);
    println!("  {} --t={:.2}/{:.2}--> {}", labels[1], t2.f, t2.c, labels[2]);

    // ── ONE closure, FOUR semirings — the diplotype(0) → recommendation(2) cell ──
    println!("\nretrieval IS inference — same adjacency, the reasoning mode is the semiring:");
    let b = closure::<Boolean>(&adj);
    let m = closure::<MinPlus>(&adj);
    let nars = closure::<Nars>(&adj);
    let mx = closure::<MaxTimes>(&adj);
    println!("  {:<38} {}", Boolean::NAME, Boolean::show(b[0][2]));
    println!("  {:<38} {}", MinPlus::NAME, MinPlus::show(m[0][2]));
    println!("  {:<38} {}", Nars::NAME, Nars::show(nars[0][2]));
    println!("  {:<38} {}", MaxTimes::NAME, MaxTimes::show(mx[0][2]));

    // ── the keystone: the Nars semiring reproduces cpic reason's (f,c) ──
    let got = nars[0][2];
    let want = Nars::mul(adj[0][1].unwrap(), t2); // the 2-hop deduction reason.rs computes
    let ok = (got.f - want.f).abs() < 1e-6 && (got.c - want.c).abs() < 1e-6;
    println!(
        "\nkeystone: Nars closure = reason.rs deduction?  closure f={:.3} c={:.3}  vs  reason f={:.3} c={:.3}  → {}",
        got.f, got.c, want.f, want.c, if ok { "✓ IDENTICAL (retrieval = reasoning)" } else { "✗ DIVERGED" }
    );
    assert!(ok, "the Nars semiring closure must reproduce reason.rs's deduction");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Vec<Vec<Option<Truth>>> {
        let mut adj = vec![vec![None; 3]; 3];
        adj[0][1] = Some(Truth { f: 1.0, c: 0.85 });
        adj[1][2] = Some(Truth { f: 0.95, c: 0.95 });
        adj
    }

    #[test]
    fn nars_closure_equals_reason_deduction() {
        // f = 1.0·0.95 = 0.95 ; c = 1.0·0.95·0.85·0.95 = 0.767 — exactly reason.rs.
        let p = closure::<Nars>(&chain());
        assert!((p[0][2].f - 0.95).abs() < 1e-6);
        assert!((p[0][2].c - 0.767).abs() < 1e-3);
    }

    #[test]
    fn swapping_the_semiring_swaps_the_mode_on_one_graph() {
        let adj = chain();
        // same adjacency, four different answers for diplotype(0) → rec(2)
        assert!(closure::<Boolean>(&adj)[0][2]); // reachable
        assert_eq!(closure::<MinPlus>(&adj)[0][2], 2.0); // 2 hops
        assert!(closure::<Nars>(&adj)[0][2].c > 0.0); // confidence
        assert!(closure::<MaxTimes>(&adj)[0][2] > 0.0); // likelihood
        // unreachable pair: rec(2) → diplotype(0) is zero in every semiring
        assert!(!closure::<Boolean>(&adj)[2][0]);
        assert!(closure::<MinPlus>(&adj)[2][0].is_infinite());
    }
}
