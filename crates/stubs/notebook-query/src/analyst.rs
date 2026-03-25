//! Political Analyst Savant Agent — generates structured thinking
//! by running NARS causality chains across the aiwar graph through
//! different analytical lenses.
//!
//! The agent doesn't retrieve answers — it CREATES THINKING.
//! Each analysis bucket runs a set of Cypher queries, applies NARS
//! truth propagation, and produces a causality chain with confidence.
//!
//! Buckets:
//! - Economic Review: resource flows, trade leverage, defense contracts
//! - Civil Engineering: dual-use tech transfer, civilian impact
//! - Political Dynamics: power structures, alliances, containment
//! - AI Development Impact: autonomy escalation, regulatory capture
//! - Kill Chain Analysis: targeting pipelines, civilian harm
//! - Surveillance Ecosystem: data flows, privacy erosion

use crate::reasoning::{TruthValue, TruthEdge, nars_deduction, nars_abduction, nars_induction, infer_edges};
use serde::{Deserialize, Serialize};

// ── Analysis Buckets ─────────────────────────────────────────────────────────

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
        &[
            Self::EconomicReview,
            Self::CivilEngineering,
            Self::PoliticalDynamics,
            Self::AiDevelopmentImpact,
            Self::KillChainAnalysis,
            Self::SurveillanceEcosystem,
        ]
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
            Self::EconomicReview => "Resource flows, trade leverage, defense contracts, investor networks",
            Self::CivilEngineering => "Dual-use technology transfer, civilian applications, infrastructure",
            Self::PoliticalDynamics => "Power structures, alliances, containment strategies, sovereignty",
            Self::AiDevelopmentImpact => "Autonomy escalation, regulatory capture, capability thresholds",
            Self::KillChainAnalysis => "Targeting pipelines, civilian harm, proportionality, oversight",
            Self::SurveillanceEcosystem => "Data flows, privacy erosion, cross-border surveillance, NSO/Pegasus",
        }
    }

    /// Cypher queries that seed this bucket's analysis.
    pub fn seed_queries(&self) -> Vec<AnalysisQuery> {
        match self {
            Self::EconomicReview => vec![
                AnalysisQuery {
                    cypher: "MATCH (p:Person)-[r]-(s:Stakeholder) WHERE p.type CONTAINS 'Investor' RETURN p.name, r.label, s.name".into(),
                    intent: "Map investor → company connections".into(),
                    nars_mode: NarsMode::Deduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:Stakeholder)-[r:DEVELOPED_BY]-(sys:System) WHERE s.type = 'DefenseCompany' RETURN s.name, sys.name, sys.year".into(),
                    intent: "Defense contractor → weapons pipeline".into(),
                    nars_mode: NarsMode::Deduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:System)-[r:USED_IN]-(n:Stakeholder) WHERE n.type = 'Nation' RETURN n.name, collect(s.name) AS systems".into(),
                    intent: "Nation → deployed weapons (economic dependency)".into(),
                    nars_mode: NarsMode::Induction,
                },
            ],
            Self::CivilEngineering => vec![
                AnalysisQuery {
                    cypher: "MATCH (s:System) WHERE s.civicUse IS NOT NULL AND s.civicUse <> '' RETURN s.name, s.civicUse, s.militaryUse".into(),
                    intent: "Identify dual-use systems".into(),
                    nars_mode: NarsMode::Abduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (c:CivicSystem)-[r]-(s:System) RETURN c.name, r.label, s.name".into(),
                    intent: "Civic ↔ military technology transfer paths".into(),
                    nars_mode: NarsMode::Deduction,
                },
            ],
            Self::PoliticalDynamics => vec![
                AnalysisQuery {
                    cypher: "MATCH (n1:Stakeholder)-[r]-(n2:Stakeholder) WHERE n1.type = 'Nation' AND n2.type = 'Nation' RETURN n1.name, r.label, n2.name".into(),
                    intent: "Nation → nation relationships".into(),
                    nars_mode: NarsMode::Deduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (n:Stakeholder {type: 'Nation'})-[r:DEPLOYED_BY]-(s:System) RETURN n.name, count(s) AS weapon_count ORDER BY weapon_count DESC".into(),
                    intent: "Military capability ranking by nation".into(),
                    nars_mode: NarsMode::Induction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:System)-[r:USED_IN]-(place:Stakeholder) WHERE place.type = 'Nation' RETURN place.name, s.name, s.year ORDER BY s.year DESC".into(),
                    intent: "Where are weapons being deployed? (conflict zones)".into(),
                    nars_mode: NarsMode::Deduction,
                },
            ],
            Self::AiDevelopmentImpact => vec![
                AnalysisQuery {
                    cypher: "MATCH (s:System) WHERE s.type CONTAINS 'AI' OR s.type CONTAINS 'Generative' OR s.MLTask IS NOT NULL RETURN s.name, s.type, s.year, s.MLTask ORDER BY s.year DESC".into(),
                    intent: "AI capability timeline".into(),
                    nars_mode: NarsMode::Induction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:System) WHERE s.militaryUse CONTAINS 'autonomous' OR s.name CONTAINS 'Lattice' OR s.name CONTAINS 'Replicator' RETURN s.name, s.militaryUse, s.year".into(),
                    intent: "Autonomous weapons escalation".into(),
                    nars_mode: NarsMode::Deduction,
                },
            ],
            Self::KillChainAnalysis => vec![
                AnalysisQuery {
                    cypher: "MATCH (s:System) WHERE s.militaryUse CONTAINS 'kill' OR s.militaryUse CONTAINS 'target' OR s.name IN ['Lavender', 'Gospel', 'Fire Factory', 'Where\\'s Daddy'] RETURN s.name, s.militaryUse, s.year".into(),
                    intent: "Kill chain components".into(),
                    nars_mode: NarsMode::Deduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:System)-[r]-(st:Stakeholder) WHERE s.name IN ['Lavender', 'Gospel'] RETURN s.name, r.label, st.name".into(),
                    intent: "Who develops and deploys targeting systems".into(),
                    nars_mode: NarsMode::Deduction,
                },
            ],
            Self::SurveillanceEcosystem => vec![
                AnalysisQuery {
                    cypher: "MATCH (s:System) WHERE s.militaryUse CONTAINS 'surveillance' OR s.militaryUse CONTAINS 'Intelligence' OR s.type CONTAINS 'Surveillance' RETURN s.name, s.militaryUse, s.year".into(),
                    intent: "Surveillance-focused systems".into(),
                    nars_mode: NarsMode::Deduction,
                },
                AnalysisQuery {
                    cypher: "MATCH (s:System)-[r]-(st:Stakeholder) WHERE s.name IN ['Pegasus', 'Clearview', 'Gotham', 'Foundry', 'Palantir AIP'] RETURN s.name, r.label, st.name".into(),
                    intent: "Surveillance vendor network".into(),
                    nars_mode: NarsMode::Deduction,
                },
            ],
        }
    }

    /// Which thinking style is most appropriate for this bucket.
    #[cfg(feature = "planner")]
    pub fn preferred_style(&self) -> lance_graph_planner::api::ThinkingStyle {
        use lance_graph_planner::api::ThinkingStyle;
        match self {
            Self::EconomicReview => ThinkingStyle::Analytical,
            Self::CivilEngineering => ThinkingStyle::Systematic,
            Self::PoliticalDynamics => ThinkingStyle::Divergent,
            Self::AiDevelopmentImpact => ThinkingStyle::Exploratory,
            Self::KillChainAnalysis => ThinkingStyle::Focused,
            Self::SurveillanceEcosystem => ThinkingStyle::Creative,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisQuery {
    pub cypher: String,
    pub intent: String,
    pub nars_mode: NarsMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarsMode {
    Deduction,
    Abduction,
    Induction,
}

// ── Analysis Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub bucket: AnalysisBucket,
    pub label: String,
    pub description: String,
    pub queries: Vec<QueryResult>,
    pub causality_chains: Vec<CausalityChain>,
    pub summary: AnalysisSummary,
    pub thinking_style: Option<String>,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub cypher: String,
    pub intent: String,
    pub status: QueryStatus,
    pub row_count: usize,
    pub edges_found: Vec<TruthEdge>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    Success,
    Error,
    NoResults,
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
pub struct AnalysisSummary {
    pub total_nodes_involved: usize,
    pub total_edges_found: usize,
    pub total_inferred: usize,
    pub avg_confidence: f64,
    pub key_findings: Vec<String>,
    pub blind_spots: Vec<String>,
}

// ── The Savant Agent ─────────────────────────────────────────────────────────

/// Run a full analysis for one bucket.
///
/// 1. Execute seed Cypher queries against the graph
/// 2. Collect edges with initial truth values
/// 3. Run NARS inference to discover new connections
/// 4. Build causality chains
/// 5. Generate summary with key findings
pub fn analyze(bucket: AnalysisBucket) -> AnalysisResult {
    let start = std::time::Instant::now();
    let seed_queries = bucket.seed_queries();

    let mut all_edges: Vec<TruthEdge> = Vec::new();
    let mut query_results: Vec<QueryResult> = Vec::new();

    // Execute each seed query
    for sq in &seed_queries {
        match crate::execute(&sq.cypher, crate::QueryLanguage::Cypher) {
            Ok(result) => {
                // Extract edges from the query result
                let edges = extract_edges_from_result(&result, &sq.nars_mode);
                let row_count = result.raw_output.lines().count();
                all_edges.extend(edges.clone());
                query_results.push(QueryResult {
                    cypher: sq.cypher.clone(),
                    intent: sq.intent.clone(),
                    status: if row_count > 0 { QueryStatus::Success } else { QueryStatus::NoResults },
                    row_count,
                    edges_found: edges,
                    error: None,
                });
            }
            Err(e) => {
                query_results.push(QueryResult {
                    cypher: sq.cypher.clone(),
                    intent: sq.intent.clone(),
                    status: QueryStatus::Error,
                    row_count: 0,
                    edges_found: vec![],
                    error: Some(e),
                });
            }
        }
    }

    // Run NARS inference on collected edges
    let inferred = infer_edges(&all_edges, 0.3, 3);
    let total_inferred = inferred.len();
    all_edges.extend(inferred);

    // Build causality chains
    let causality_chains = build_causality_chains(&all_edges, bucket);

    // Generate summary
    let unique_nodes: std::collections::HashSet<&str> = all_edges.iter()
        .flat_map(|e| [e.source.as_str(), e.target.as_str()])
        .collect();

    let avg_confidence = if all_edges.is_empty() {
        0.0
    } else {
        all_edges.iter().map(|e| e.truth.confidence).sum::<f64>() / all_edges.len() as f64
    };

    let key_findings = generate_findings(&all_edges, bucket);
    let blind_spots = generate_blind_spots(&all_edges, bucket);

    let summary = AnalysisSummary {
        total_nodes_involved: unique_nodes.len(),
        total_edges_found: all_edges.len() - total_inferred,
        total_inferred,
        avg_confidence,
        key_findings,
        blind_spots,
    };

    AnalysisResult {
        bucket,
        label: bucket.label().to_string(),
        description: bucket.description().to_string(),
        queries: query_results,
        causality_chains,
        summary,
        thinking_style: None, // Set by caller if planner feature is on
        elapsed_us: start.elapsed().as_micros() as u64,
    }
}

/// Run all buckets and return a full analytical report.
pub fn full_analysis() -> Vec<AnalysisResult> {
    AnalysisBucket::all().iter().map(|b| analyze(*b)).collect()
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn extract_edges_from_result(result: &crate::QueryResult, mode: &NarsMode) -> Vec<TruthEdge> {
    // Parse graph JSON if available
    if let Some(ref graph_json) = result.graph_json {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(graph_json) {
            if let Some(edges) = data["edges"].as_array() {
                return edges.iter().filter_map(|e| {
                    let source = e["source"].as_str()?.to_string();
                    let target = e["target"].as_str()?.to_string();
                    let label = e["label"].as_str().unwrap_or("related").to_string();
                    Some(TruthEdge {
                        source,
                        target,
                        rel_type: label,
                        truth: TruthValue { frequency: 0.80, confidence: 0.70 },
                        inferred: false,
                        via: vec![],
                        inference_type: None,
                    })
                }).collect();
            }
        }
    }

    // Fallback: parse raw text output for entity pairs
    let base_confidence = match mode {
        NarsMode::Deduction => 0.80,
        NarsMode::Induction => 0.65,
        NarsMode::Abduction => 0.55,
    };

    result.raw_output.lines().skip(1).filter_map(|line| {
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() >= 2 {
            Some(TruthEdge {
                source: parts[0].to_string(),
                target: parts[1].to_string(),
                rel_type: if parts.len() >= 3 { parts[2].to_string() } else { "related".to_string() },
                truth: TruthValue { frequency: 0.80, confidence: base_confidence },
                inferred: false,
                via: vec![],
                inference_type: None,
            })
        } else {
            None
        }
    }).collect()
}

fn build_causality_chains(edges: &[TruthEdge], bucket: AnalysisBucket) -> Vec<CausalityChain> {
    // Group related edges into chains by following source→target paths
    let mut chains: Vec<CausalityChain> = Vec::new();

    // Find chain starters (nodes that appear as source but not target)
    let targets: std::collections::HashSet<&str> = edges.iter().map(|e| e.target.as_str()).collect();
    let starters: Vec<&TruthEdge> = edges.iter()
        .filter(|e| !targets.contains(e.source.as_str()))
        .collect();

    for start in starters.iter().take(5) {
        let mut chain_edges = vec![(*start).clone()];
        let mut current = &start.target;

        // Follow the chain up to 5 hops
        for _ in 0..5 {
            if let Some(next) = edges.iter().find(|e| e.source == *current && !chain_edges.iter().any(|c| c.target == e.target)) {
                chain_edges.push(next.clone());
                current = &next.target;
            } else {
                break;
            }
        }

        if chain_edges.len() >= 2 {
            let confidence = chain_edges.iter()
                .map(|e| e.truth.confidence)
                .fold(1.0, |acc, c| acc * c);

            let narrative = chain_edges.iter()
                .map(|e| format!("{} →[{}]→ {}", e.source, e.rel_type, e.target))
                .collect::<Vec<_>>()
                .join(" ");

            chains.push(CausalityChain {
                name: format!("{} → {}", chain_edges[0].source, chain_edges.last().unwrap().target),
                edges: chain_edges,
                confidence,
                inference_type: "deduction".to_string(),
                narrative,
            });
        }
    }

    chains.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    chains.truncate(5);
    chains
}

fn generate_findings(edges: &[TruthEdge], bucket: AnalysisBucket) -> Vec<String> {
    let mut findings = Vec::new();

    // Count node frequency
    let mut node_freq: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in edges {
        *node_freq.entry(&e.source).or_default() += 1;
        *node_freq.entry(&e.target).or_default() += 1;
    }
    let mut sorted: Vec<_> = node_freq.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    if let Some((node, count)) = sorted.first() {
        findings.push(format!("Most connected entity: {} ({} connections)", node, count));
    }

    // High confidence inferences
    let high_conf: Vec<_> = edges.iter().filter(|e| e.inferred && e.truth.confidence > 0.6).collect();
    if !high_conf.is_empty() {
        findings.push(format!("{} high-confidence inferences discovered", high_conf.len()));
    }

    let inferred_count = edges.iter().filter(|e| e.inferred).count();
    if inferred_count > 0 {
        findings.push(format!("NARS discovered {} new connections through inference", inferred_count));
    }

    findings.push(format!("{} total edges in {} analysis", edges.len(), bucket.label()));
    findings
}

fn generate_blind_spots(edges: &[TruthEdge], bucket: AnalysisBucket) -> Vec<String> {
    let mut blind_spots = Vec::new();

    let low_conf: Vec<_> = edges.iter().filter(|e| e.truth.confidence < 0.4).collect();
    if !low_conf.is_empty() {
        blind_spots.push(format!("{} low-confidence edges need verification", low_conf.len()));
    }

    match bucket {
        AnalysisBucket::EconomicReview => {
            blind_spots.push("Chinese defense spending data is sparse in this dataset".to_string());
            blind_spots.push("Offshore financial flows not tracked".to_string());
        }
        AnalysisBucket::PoliticalDynamics => {
            blind_spots.push("Russian AI weapons data is minimal".to_string());
            blind_spots.push("Non-state actor networks not mapped".to_string());
        }
        AnalysisBucket::KillChainAnalysis => {
            blind_spots.push("Civilian casualty data not in this graph".to_string());
        }
        AnalysisBucket::SurveillanceEcosystem => {
            blind_spots.push("Chinese surveillance exports (Hikvision, SenseTime) underrepresented".to_string());
        }
        _ => {}
    }

    blind_spots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_buckets_have_queries() {
        for bucket in AnalysisBucket::all() {
            let queries = bucket.seed_queries();
            assert!(!queries.is_empty(), "{:?} has no seed queries", bucket);
        }
    }

    #[test]
    fn bucket_labels_are_nonempty() {
        for bucket in AnalysisBucket::all() {
            assert!(!bucket.label().is_empty());
            assert!(!bucket.description().is_empty());
        }
    }
}
