//! Clinical NARS reasoning — the FMA `/fma-body` viewer's "reason about this organ"
//! panel, backed by the REAL q2 NARS (`lance_graph_planner::nars::truth::TruthValue::
//! deduction`, the same canonical algebra `graph_engine::nars_deduction` uses).
//!
//! A scenario {organ, condition, medication, labs} is compiled into a small clinical
//! graph of `(subject --rel--> object, NarsTruth)` statements drawn from a hand-authored
//! rule KB (condition→effects, drug→properties, (effect×property)→risk, lab→effect). NARS
//! 2-hop deduction `A→B, B→C ⊢ A→C` then chains e.g. `acetaminophen → hepatically_metabolized
//! → drug_accumulation_toxicity ⊢ acetaminophen → drug_accumulation_toxicity` with a derived
//! truth value. Returns the inferences (frequency/confidence) + a plain-language summary.
//!
//! DEMO ONLY — a NARS reasoning illustration over a tiny rule set, NOT medical advice. The
//! frontend shows that disclaimer in-view.

use lance_graph_contract::exploration::NarsTruth;
use lance_graph_planner::nars::truth::TruthValue;
use serde::Serialize;

/// One clinical statement: `subject --relation--> object` carrying a NARS truth value.
#[derive(Clone)]
struct Stmt {
    s: String,
    rel: String,
    o: String,
    truth: NarsTruth,
}

/// A derived (or asserted) clinical inference, wire-serialized with `truth_f`/`truth_c`
/// to match the cockpit's existing NARS JSON convention (see graph_engine).
#[derive(Clone)]
pub struct CliInference {
    s: String,
    rel: String,
    o: String,
    kind: &'static str, // "Asserted" | "Deduction"
    truth: NarsTruth,
    via: Vec<String>,
}

impl Serialize for CliInference {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = ser.serialize_struct("CliInference", 7)?;
        st.serialize_field("source", &self.s)?;
        st.serialize_field("relation", &self.rel)?;
        st.serialize_field("target", &self.o)?;
        st.serialize_field("inference_type", &self.kind)?;
        st.serialize_field("truth_f", &self.truth.frequency)?;
        st.serialize_field("truth_c", &self.truth.confidence)?;
        st.serialize_field("via", &self.via)?;
        st.end()
    }
}

/// Canonical NARS deduction `A→B, B→C ⊢ A→C` via the planner's `TruthValue::deduction`
/// (`f = f1·f2, c = f1·f2·c1·c2`) — identical bridge to `graph_engine::nars_deduction`.
fn nars_deduction(ab: &NarsTruth, bc: &NarsTruth) -> NarsTruth {
    let r = TruthValue::new(ab.frequency, ab.confidence)
        .deduction(&TruthValue::new(bc.frequency, bc.confidence));
    NarsTruth::new(r.frequency, r.confidence)
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase().replace([' ', '-'], "_")
}

// ── the rule KB (compact clinical demo) ───────────────────────────────────────────────
// condition → physiological effects it induces.
fn condition_effects(cond: &str) -> &'static [(&'static str, f32, f32)] {
    match cond {
        "cirrhosis" => &[
            ("impaired_hepatic_clearance", 0.9, 0.85),
            ("coagulopathy", 0.75, 0.8),
            ("portal_hypertension", 0.7, 0.75),
        ],
        "hepatitis" => &[
            ("hepatic_inflammation", 0.85, 0.8),
            ("impaired_hepatic_clearance", 0.6, 0.7),
        ],
        "ckd" | "renal_failure" | "chronic_kidney_disease" => {
            &[("impaired_renal_clearance", 0.9, 0.85)]
        }
        "heart_failure" => &[
            ("renal_hypoperfusion", 0.7, 0.75),
            ("fluid_overload", 0.8, 0.8),
        ],
        _ => &[],
    }
}
// medication → pharmacologic properties.
fn drug_properties(drug: &str) -> &'static [(&'static str, f32, f32)] {
    match drug {
        "acetaminophen" | "paracetamol" => &[
            ("hepatically_metabolized", 0.95, 0.9),
            ("hepatotoxic_in_overdose", 0.7, 0.8),
        ],
        "ibuprofen" | "naproxen" | "nsaid" => &[
            ("renally_cleared", 0.8, 0.85),
            ("gi_bleed_propensity", 0.65, 0.75),
            ("nephrotoxic", 0.6, 0.7),
        ],
        "warfarin" => &[
            ("hepatically_metabolized", 0.9, 0.85),
            ("anticoagulant", 0.95, 0.9),
        ],
        "metformin" => &[
            ("renally_cleared", 0.9, 0.9),
            ("lactic_acidosis_if_accumulated", 0.6, 0.75),
        ],
        _ => &[],
    }
}
// (active effect × drug property) → the risk it produces. property→risk is the edge NARS
// chains through (drug → property → risk).
fn risk_rule(effect: &str, property: &str) -> Option<(&'static str, f32, f32)> {
    match (effect, property) {
        ("impaired_hepatic_clearance", "hepatically_metabolized") => {
            Some(("drug_accumulation_toxicity", 0.85, 0.8))
        }
        ("impaired_renal_clearance", "renally_cleared") => {
            Some(("drug_accumulation_toxicity", 0.85, 0.8))
        }
        ("coagulopathy", "anticoagulant") => Some(("major_bleeding_risk", 0.9, 0.85)),
        ("coagulopathy", "gi_bleed_propensity") => Some(("gi_hemorrhage_risk", 0.85, 0.8)),
        ("portal_hypertension", "gi_bleed_propensity") => Some(("variceal_bleed_risk", 0.8, 0.8)),
        ("renal_hypoperfusion", "nephrotoxic") => Some(("acute_kidney_injury_risk", 0.85, 0.8)),
        ("impaired_renal_clearance", "lactic_acidosis_if_accumulated") => {
            Some(("lactic_acidosis_risk", 0.8, 0.8))
        }
        _ => None,
    }
}
// lab (name, abnormal flag) → the effect it asserts/reinforces.
fn lab_effect(name: &str, flag: &str) -> Option<(&'static str, f32, f32)> {
    match (name, flag) {
        ("inr", "high") => Some(("coagulopathy", 0.85, 0.85)),
        ("bilirubin", "high") => Some(("impaired_hepatic_clearance", 0.8, 0.8)),
        ("albumin", "low") => Some(("hepatic_synthetic_dysfunction", 0.7, 0.75)),
        ("creatinine", "high") => Some(("impaired_renal_clearance", 0.88, 0.85)),
        ("egfr", "low") => Some(("impaired_renal_clearance", 0.88, 0.85)),
        ("platelets", "low") => Some(("coagulopathy", 0.6, 0.7)),
        _ => None,
    }
}

#[derive(serde::Deserialize)]
pub struct LabValue {
    pub name: String,
    #[serde(default)]
    pub flag: String, // "high" | "low" | "normal"
}

#[derive(serde::Deserialize)]
pub struct ClinicalScenario {
    #[serde(default)]
    pub organ: String,
    #[serde(default)]
    pub condition: String,
    #[serde(default)]
    pub medication: String,
    #[serde(default)]
    pub labs: Vec<LabValue>,
}

/// Compile the scenario → clinical statements, run 2-hop NARS deduction, return inferences.
fn reason(sc: &ClinicalScenario) -> (Vec<CliInference>, Vec<String>) {
    let (organ, cond, drug) = (norm(&sc.organ), norm(&sc.condition), norm(&sc.medication));
    let mut stmts: Vec<Stmt> = Vec::new();
    let mut effects: Vec<String> = Vec::new();
    let push = |v: &mut Vec<Stmt>, s: &str, rel: &str, o: &str, f: f32, c: f32| {
        v.push(Stmt {
            s: s.into(),
            rel: rel.into(),
            o: o.into(),
            truth: NarsTruth::new(f, c),
        });
    };

    // organ --has--> condition
    if !organ.is_empty() && !cond.is_empty() {
        push(&mut stmts, &organ, "has", &cond, 1.0, 0.9);
    }
    // condition --induces--> effect
    for &(e, f, c) in condition_effects(&cond) {
        push(&mut stmts, &cond, "induces", e, f, c);
        effects.push(e.to_string());
    }
    // lab --indicates--> effect (reinforces / asserts)
    for lab in &sc.labs {
        let (ln, lf) = (norm(&lab.name), norm(&lab.flag));
        if let Some((e, f, c)) = lab_effect(&ln, &lf) {
            push(&mut stmts, &format!("{ln}_{lf}"), "indicates", e, f, c);
            if !effects.contains(&e.to_string()) {
                effects.push(e.to_string());
            }
        }
    }
    // medication --is--> property ; property --confers--> risk (only for ACTIVE effects)
    for &(p, f, c) in drug_properties(&drug) {
        if !drug.is_empty() {
            push(&mut stmts, &drug, "is", p, f, c);
        }
        for e in &effects {
            if let Some((risk, rf, rc)) = risk_rule(e, p) {
                push(&mut stmts, p, "confers", risk, rf, rc);
            }
        }
    }

    // assert + deduce
    let mut out: Vec<CliInference> = stmts
        .iter()
        .map(|s| CliInference {
            s: s.s.clone(),
            rel: s.rel.clone(),
            o: s.o.clone(),
            kind: "Asserted",
            truth: s.truth,
            via: vec![],
        })
        .collect();

    // 2-hop deduction A→B, B→C ⊢ A→C
    let existing: std::collections::HashSet<(String, String)> =
        stmts.iter().map(|s| (s.s.clone(), s.o.clone())).collect();
    for ab in &stmts {
        for bc in &stmts {
            if ab.o == bc.s && ab.s != bc.o && !existing.contains(&(ab.s.clone(), bc.o.clone())) {
                let t = nars_deduction(&ab.truth, &bc.truth);
                if t.confidence >= 0.1 {
                    out.push(CliInference {
                        s: ab.s.clone(),
                        rel: bc.rel.clone(),
                        o: bc.o.clone(),
                        kind: "Deduction",
                        truth: t,
                        via: vec![ab.o.clone()],
                    });
                }
            }
        }
    }

    // plain-language summary: strongest derived risks (expectation = c·(f−0.5)+0.5).
    let mut derived: Vec<&CliInference> = out
        .iter()
        .filter(|i| i.kind == "Deduction" && (i.o.contains("risk") || i.o.contains("toxicity")))
        .collect();
    derived.sort_by(|a, b| {
        let ex = |i: &CliInference| i.truth.confidence * (i.truth.frequency - 0.5) + 0.5;
        ex(b)
            .partial_cmp(&ex(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut summary: Vec<String> = derived
        .iter()
        .take(4)
        .map(|i| {
            let pretty = |s: &str| s.replace('_', " ");
            format!(
                "{} → {} (f={:.2}, c={:.2}, via {})",
                pretty(&i.s),
                pretty(&i.o),
                i.truth.frequency,
                i.truth.confidence,
                i.via.join(", ")
            )
        })
        .collect();
    if summary.is_empty() {
        summary.push("No risk chain derived from the rule KB for this scenario.".into());
    }
    (out, summary)
}

/// `POST /api/clinical/reason` — body `{organ, condition, medication, labs:[{name,flag}]}`.
pub async fn clinical_reason_handler(
    axum::Json(sc): axum::Json<ClinicalScenario>,
) -> axum::Json<serde_json::Value> {
    let (inferences, summary) = reason(&sc);
    let asserted = inferences.iter().filter(|i| i.kind == "Asserted").count();
    let derived = inferences.len() - asserted;
    axum::Json(serde_json::json!({
        "organ": sc.organ,
        "condition": sc.condition,
        "medication": sc.medication,
        "asserted": asserted,
        "derived": derived,
        "inferences": inferences,
        "summary": summary,
        "engine": "lance_graph_planner::nars::truth::TruthValue::deduction (f=f1·f2, c=f1·f2·c1·c2)",
        "disclaimer": "NARS reasoning DEMO over a small rule KB — not medical advice.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cirrhosis_acetaminophen_chains_to_accumulation() {
        let sc = ClinicalScenario {
            organ: "liver".into(),
            condition: "cirrhosis".into(),
            medication: "acetaminophen".into(),
            labs: vec![LabValue {
                name: "inr".into(),
                flag: "high".into(),
            }],
        };
        let (inf, summary) = reason(&sc);
        // deduction must surface acetaminophen → drug_accumulation_toxicity via hepatically_metabolized
        assert!(
            inf.iter().any(|i| i.s == "acetaminophen"
                && i.o == "drug_accumulation_toxicity"
                && i.kind == "Deduction"),
            "expected acetaminophen→drug_accumulation_toxicity; got {:?}",
            inf.iter()
                .map(|i| format!("{}→{}", i.s, i.o))
                .collect::<Vec<_>>()
        );
        assert!(!summary.is_empty());
    }
}
