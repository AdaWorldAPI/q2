//! Political Analyst Savant — creates thinking by running NARS
//! SPO causal inference across the aiwar graph.
//!
//! Loads the graph directly (no MCP), builds TruthEdges from all
//! relationships, runs NARS deduction/abduction/induction, builds
//! causal chains, and projects forward.

use crate::reasoning::{
    infer_edges, nars_deduction, InferenceType, TruthEdge, TruthValue,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisBucket {
    EconomicReview,
    CivilEngineering,
    PoliticalDynamics,
    AiDevelopmentImpact,
    KillChainAnalysis,
    SurveillanceEcosystem,
}

impl AnalysisBucket {
    pub fn all() -> &'static [AnalysisBucket] {
        &[Self::EconomicReview, Self::CivilEngineering, Self::PoliticalDynamics,
          Self::AiDevelopmentImpact, Self::KillChainAnalysis, Self::SurveillanceEcosystem]
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::EconomicReview => "Economic Review",
            Self::CivilEngineering => "Civil Engineering",
            Self::PoliticalDynamics => "Political Dynamics",
            Self::AiDevelopmentImpact => "AI Development Impact",
            Self::KillChainAnalysis => "Kill Chain Analysis",
            Self::SurveillanceEcosystem => "Surveillance Ecosystem",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Self::EconomicReview => "Resource flows, defense contracts, investor networks",
            Self::CivilEngineering => "Dual-use technology transfer, civilian applications",
            Self::PoliticalDynamics => "Power structures, alliances, containment strategies",
            Self::AiDevelopmentImpact => "Autonomy escalation, regulatory capture, thresholds",
            Self::KillChainAnalysis => "Targeting pipelines, civilian harm, oversight gaps",
            Self::SurveillanceEcosystem => "Data flows, privacy erosion, cross-border surveillance",
        }
    }
    fn node_keywords(&self) -> &[&str] {
        match self {
            Self::EconomicReview => &["Thiel", "Luckey", "Musk", "Palantir", "Anduril", "Fund"],
            Self::CivilEngineering => &["Claude", "Roomba", "Pokemon", "Niantic"],
            Self::PoliticalDynamics => &["US", "Israel", "NATO", "China", "Russia", "UK", "DIANA"],
            Self::AiDevelopmentImpact => &["Lattice", "Replicator", "AIP", "Gotham", "LLM"],
            Self::KillChainAnalysis => &["Lavender", "Gospel", "Fire", "Daddy", "Alchemist", "Legion"],
            Self::SurveillanceEcosystem => &["Pegasus", "Clearview", "Gotham", "Foundry", "Palantir", "Fortify"],
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawGraph {
    #[serde(rename = "N_Systems", default)] systems: Vec<RawNode>,
    #[serde(rename = "N_Stakeholders", default)] stakeholders: Vec<RawNode>,
    #[serde(rename = "N_People", default)] people: Vec<RawNode>,
    #[serde(rename = "N_Civic", default)] civic: Vec<RawNode>,
    #[serde(rename = "N_Historical", default)] historical: Vec<RawNode>,
    #[serde(rename = "E_connection", default)] e_connection: Vec<RawEdge>,
    #[serde(rename = "E_isDevelopedBy", default)] e_developed: Vec<RawEdge>,
    #[serde(rename = "E_isDeployedBy", default)] e_deployed: Vec<RawEdge>,
    #[serde(rename = "E_place", default)] e_place: Vec<RawEdge>,
    #[serde(rename = "E_people", default)] e_people: Vec<RawEdge>,
}

#[derive(Debug, Deserialize)]
struct RawNode { id: Option<String>, name: Option<String>, #[serde(rename = "type")] node_type: Option<String> }

#[derive(Debug, Deserialize)]
struct RawEdge { source: Option<String>, target: Option<String>, label: Option<String> }

fn load_graph() -> Option<RawGraph> {
    for path in &[
        std::env::var("AIWAR_DATA_PATH").unwrap_or_default(),
        "/app/data/aiwar_graph.json".into(),
        "cockpit/public/aiwar_graph.json".into(),
        "../aiwar-neo4j-harvest/data/aiwar_graph.json".into(),
        "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json".into(),
    ] {
        if path.is_empty() { continue; }
        if let Ok(content) = std::fs::read_to_string(path) {
            let cleaned = content.replace("NaN", "null");
            if let Ok(g) = serde_json::from_str::<RawGraph>(&cleaned) { return Some(g); }
        }
    }
    None
}

fn build_node_map(g: &RawGraph) -> HashMap<String, (String, String)> {
    let mut m = HashMap::new();
    for (nodes, typ) in [(&g.systems, "System"), (&g.stakeholders, "Stakeholder"),
        (&g.people, "Person"), (&g.civic, "CivicSystem"), (&g.historical, "Historical")] {
        for n in nodes {
            if let Some(id) = &n.id {
                m.insert(id.clone(), (n.name.clone().unwrap_or_default(), typ.to_string()));
            }
        }
    }
    m
}

fn build_truth_edges(g: &RawGraph) -> Vec<TruthEdge> {
    let mut edges = Vec::new();
    for (raw, rel, conf) in [
        (&g.e_connection, "connected_to", 0.75),
        (&g.e_developed, "developed_by", 0.85),
        (&g.e_deployed, "deployed_by", 0.80),
        (&g.e_place, "used_in", 0.80),
        (&g.e_people, "person_link", 0.70),
    ] {
        for e in raw {
            if let (Some(s), Some(t)) = (&e.source, &e.target) {
                edges.push(TruthEdge {
                    source: s.clone(), target: t.clone(),
                    rel_type: e.label.clone().unwrap_or_else(|| rel.to_string()),
                    truth: TruthValue::new(0.85, conf),
                    inferred: false, via: vec![], inference_type: None,
                });
            }
        }
    }
    edges
}

// ── Result Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub bucket: AnalysisBucket,
    pub label: String,
    pub description: String,
    pub thinking_steps: Vec<ThinkingStep>,
    pub causality_chains: Vec<CausalityChain>,
    pub projections: Vec<Projection>,
    pub summary: AnalysisSummary,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingStep {
    pub step: usize,
    pub action: String,
    pub detail: String,
    pub edges_before: usize,
    pub edges_after: usize,
    pub new_inferences: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalityChain {
    pub name: String,
    pub edges: Vec<TruthEdge>,
    pub confidence: f64,
    pub inference_type: String,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projection {
    pub label: String,
    pub confidence: f64,
    pub basis: String,
    pub implication: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub total_nodes: usize,
    pub total_edges_observed: usize,
    pub total_edges_inferred: usize,
    pub avg_confidence: f64,
    pub key_findings: Vec<String>,
    pub blind_spots: Vec<String>,
}

// ── The Analyst ──────────────────────────────────────────────────────────────

pub fn analyze(bucket: AnalysisBucket) -> AnalysisResult {
    let start = std::time::Instant::now();
    let mut steps = Vec::new();

    let graph = match load_graph() {
        Some(g) => g,
        None => return empty_result(bucket, start, "aiwar_graph.json not found"),
    };

    let all_edges = build_truth_edges(&graph);
    let node_map = build_node_map(&graph);

    steps.push(ThinkingStep { step: 1, action: "OBSERVE".into(),
        detail: format!("Loaded {} nodes, {} edges", node_map.len(), all_edges.len()),
        edges_before: 0, edges_after: all_edges.len(), new_inferences: 0 });

    // Filter by bucket relevance
    let kw = bucket.node_keywords();
    let filtered: Vec<TruthEdge> = all_edges.iter().filter(|e| {
        kw.iter().any(|k| e.source.contains(k) || e.target.contains(k))
    }).cloned().collect();

    steps.push(ThinkingStep { step: 2, action: "FILTER".into(),
        detail: format!("Filtered to {} edges for {} (keywords: {})", filtered.len(), bucket.label(), kw.join(", ")),
        edges_before: all_edges.len(), edges_after: filtered.len(), new_inferences: 0 });

    // NARS inference
    let inferred = infer_edges(&filtered, 0.15, 3);
    let n_inf = inferred.len();

    steps.push(ThinkingStep { step: 3, action: "INFER".into(),
        detail: format!("NARS deduction: {} new causal links discovered", n_inf),
        edges_before: filtered.len(), edges_after: filtered.len() + n_inf, new_inferences: n_inf });

    let mut relevant = filtered;
    relevant.extend(inferred);

    // Build chains
    let chains = build_chains(&relevant, &node_map);
    steps.push(ThinkingStep { step: 4, action: "CHAIN".into(),
        detail: format!("{} causality chains built", chains.len()),
        edges_before: relevant.len(), edges_after: relevant.len(), new_inferences: 0 });

    // Project
    let projections = project(&chains, &relevant, bucket);
    steps.push(ThinkingStep { step: 5, action: "PROJECT".into(),
        detail: format!("{} projections generated", projections.len()),
        edges_before: relevant.len(), edges_after: relevant.len(), new_inferences: 0 });

    let obs = relevant.iter().filter(|e| !e.inferred).count();
    let inf = relevant.iter().filter(|e| e.inferred).count();
    let avg_c = if relevant.is_empty() { 0.0 } else { relevant.iter().map(|e| e.truth.confidence).sum::<f64>() / relevant.len() as f64 };

    AnalysisResult {
        bucket, label: bucket.label().into(), description: bucket.description().into(),
        thinking_steps: steps, causality_chains: chains, projections,
        summary: AnalysisSummary {
            total_nodes: node_map.len(), total_edges_observed: obs, total_edges_inferred: inf,
            avg_confidence: avg_c,
            key_findings: findings(&relevant, &node_map, bucket),
            blind_spots: blind_spots(&relevant, bucket),
        },
        elapsed_us: start.elapsed().as_micros() as u64,
    }
}

pub fn full_analysis() -> Vec<AnalysisResult> {
    AnalysisBucket::all().iter().map(|b| analyze(*b)).collect()
}

fn empty_result(bucket: AnalysisBucket, start: std::time::Instant, msg: &str) -> AnalysisResult {
    AnalysisResult {
        bucket, label: bucket.label().into(), description: bucket.description().into(),
        thinking_steps: vec![ThinkingStep { step: 1, action: "ERROR".into(), detail: msg.into(), edges_before: 0, edges_after: 0, new_inferences: 0 }],
        causality_chains: vec![], projections: vec![],
        summary: AnalysisSummary { total_nodes: 0, total_edges_observed: 0, total_edges_inferred: 0, avg_confidence: 0.0, key_findings: vec![msg.into()], blind_spots: vec![] },
        elapsed_us: start.elapsed().as_micros() as u64,
    }
}

fn build_chains(edges: &[TruthEdge], node_map: &HashMap<String, (String, String)>) -> Vec<CausalityChain> {
    let mut adj: HashMap<&str, Vec<&TruthEdge>> = HashMap::new();
    for e in edges { adj.entry(&e.source).or_default().push(e); }
    let targets: std::collections::HashSet<&str> = edges.iter().map(|e| e.target.as_str()).collect();
    let starters: Vec<&str> = adj.keys().filter(|k| !targets.contains(**k)).copied().collect();
    let name = |id: &str| node_map.get(id).map(|(n, _)| n.clone()).unwrap_or_else(|| id.into());

    let mut chains = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for &start in starters.iter().take(20) {
        let mut chain = Vec::new();
        let mut cur = start;
        let mut visited = std::collections::HashSet::new();
        visited.insert(cur);
        for _ in 0..6 {
            if let Some(nexts) = adj.get(cur) {
                if let Some(best) = nexts.iter().filter(|e| !visited.contains(e.target.as_str()))
                    .max_by(|a, b| a.truth.confidence.partial_cmp(&b.truth.confidence).unwrap_or(std::cmp::Ordering::Equal)) {
                    chain.push((*best).clone());
                    visited.insert(&best.target);
                    cur = &best.target;
                } else { break; }
            } else { break; }
        }
        if chain.len() >= 2 {
            let key = format!("{}→{}", start, cur);
            if seen.contains(&key) { continue; }
            seen.insert(key);
            let conf = chain.iter().map(|e| e.truth.confidence).fold(1.0, |a, b| a * b);
            let has_inf = chain.iter().any(|e| e.inferred);
            let narrative = chain.iter().map(|e| format!("{} →[{}]→ {}{}", name(&e.source), e.rel_type, name(&e.target), if e.inferred { " ⟹" } else { "" })).collect::<Vec<_>>().join(" | ");
            chains.push(CausalityChain { name: format!("{} → {}", name(start), name(cur)), edges: chain, confidence: conf, inference_type: if has_inf { "deduction+observed" } else { "observed" }.into(), narrative });
        }
    }
    chains.sort_by(|a, b| b.edges.len().cmp(&a.edges.len()));
    chains.truncate(8);
    chains
}

fn project(chains: &[CausalityChain], edges: &[TruthEdge], bucket: AnalysisBucket) -> Vec<Projection> {
    let mut p = Vec::new();
    for c in chains.iter().take(3) {
        if c.confidence > 0.05 {
            let last = c.edges.last().map(|e| e.target.as_str()).unwrap_or("?");
            p.push(Projection {
                label: format!("{} chain continues", c.name), confidence: c.confidence * 0.7,
                basis: c.narrative.clone(),
                implication: format!("{} remains central to {} dynamics", last, bucket.label()),
            });
        }
    }
    let mut deg: HashMap<&str, usize> = HashMap::new();
    for e in edges { *deg.entry(&e.source).or_default() += 1; *deg.entry(&e.target).or_default() += 1; }
    if let Some((node, d)) = deg.iter().max_by_key(|(_, d)| *d) {
        p.push(Projection { label: format!("Hub: {}", node), confidence: 0.75, basis: format!("{} connections", d), implication: format!("Disruption to {} cascades through the network", node) });
    }
    let inf = edges.iter().filter(|e| e.inferred).count();
    if inf > 0 {
        p.push(Projection { label: "Hidden structure".into(), confidence: 0.5, basis: format!("{} NARS-inferred connections", inf), implication: "Causal links exist that aren't directly documented — discovered through reasoning".into() });
    }
    p
}

fn findings(edges: &[TruthEdge], nm: &HashMap<String, (String, String)>, bucket: AnalysisBucket) -> Vec<String> {
    let name = |id: &str| nm.get(id).map(|(n, _)| n.clone()).unwrap_or_else(|| id.into());
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for e in edges { *freq.entry(&e.source).or_default() += 1; *freq.entry(&e.target).or_default() += 1; }
    let mut sorted: Vec<_> = freq.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    let mut f = Vec::new();
    for (n, c) in sorted.iter().take(3) { f.push(format!("{} ({} connections) — central to {}", name(n), c, bucket.label())); }
    let inf = edges.iter().filter(|e| e.inferred).count();
    if inf > 0 { f.push(format!("NARS discovered {} hidden causal links", inf)); }
    f.push(format!("{} total edges analyzed, {:.0}% avg confidence", edges.len(), edges.iter().map(|e| e.truth.confidence).sum::<f64>() / edges.len().max(1) as f64 * 100.0));
    f
}

fn blind_spots(edges: &[TruthEdge], bucket: AnalysisBucket) -> Vec<String> {
    let mut s = Vec::new();
    let low = edges.iter().filter(|e| e.truth.confidence < 0.3).count();
    if low > 0 { s.push(format!("{} low-confidence edges need verification", low)); }
    match bucket {
        AnalysisBucket::EconomicReview => s.push("Chinese/Russian defense spending underrepresented".into()),
        AnalysisBucket::KillChainAnalysis => s.push("Civilian casualty attribution not in graph".into()),
        AnalysisBucket::SurveillanceEcosystem => s.push("Hikvision/SenseTime export data sparse".into()),
        _ => {}
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_buckets_have_keywords() {
        for b in AnalysisBucket::all() { assert!(!b.node_keywords().is_empty()); }
    }

    #[test]
    fn labels_nonempty() {
        for b in AnalysisBucket::all() { assert!(!b.label().is_empty()); assert!(!b.description().is_empty()); }
    }
}
