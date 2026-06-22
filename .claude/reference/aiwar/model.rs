// aiwar-neo4j-harvest/src/model.rs
//
// Graph Schema Harvested from sarahciston/aiwar (GitLab)
// Novel patterns for Neo4j succession:
//
// 1. MULTI-TAXONOMY NODES: Each node carries parallel classification axes
//    (AIRO ontology: type, purpose, capacity, impact) + domain-specific
//    enums (MLTask, militaryUse, civicUse). This is a "faceted graph" —
//    nodes belong to multiple overlapping taxonomies simultaneously.
//
// 2. DUAL-ROLE EDGES: The same entity appears as both source AND target
//    across different edge tables (Stakeholder→develops→System,
//    System→deployed_by→Stakeholder). This creates a bipartite-like
//    structure that collapses into a multigraph.
//
// 3. SCHEMA-AS-DATA: The Schema sheet IS the ontology — it defines
//    valid enum values as rows, not as code. The graph is self-describing.
//
// 4. HIERARCHICAL META-EDGES: E_hierarchical encodes the *schema itself*
//    as edges (E_isDeployedBy → N_Systems), creating a meta-graph where
//    the relationship types are first-class nodes.
//
// 5. ICON-ADDRESSED NODES: Nodes carry `nounKey` and `image` that map
//    to visual nouns — the graph is designed for spatial/visual traversal,
//    not just relational queries. This is the "memory as place" pattern.

use serde::{Deserialize, Serialize};

// ── Node Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System {
    pub id: String,
    pub name: String,
    pub year: Option<i32>,
    pub current_status: Option<String>,     // Development | Deployment | Operation | Retirement
    #[serde(rename = "type")]
    pub system_type: Option<String>,        // CSV of: GenerativeAI, RecommendationSystem, ComputerVision, IoT...
    pub ml_task: Option<String>,            // Generate | Predict | Recognize | Capture | Store | Sort
    pub military_use: Option<String>,       // Intelligence | Command | Robot | Weapon
    pub civic_use: Option<String>,          // RecommenderSystem, AR, Policing, BehaviorEvaluation...
    pub ml_tasks: Option<String>,           // Detailed: Ranking, Recommendation, SignalAnalysis...
    pub purpose: Option<String>,            // AIRO: PredictiveMapping, ProducingRecommendation...
    pub capacity: Option<String>,           // AIRO: Profiling, FaceRecognition, BehaviourAnalysis...
    pub vair_technique: Option<String>,     // DeepLearning, LanguageModels, StatisticalTechnique...
    pub output: Option<String>,             // Action | Content | Decision | Recommendation
    pub vair_risk_sources: Option<String>,  // InaccuratePrediction, BiasedTrainingData...
    pub impact: Option<String>,             // PhysicalInjury, WellbeingImpact, PsychologicalHarm
    pub hover: Option<String>,              // Tooltip text
    pub image: Option<String>,              // Icon path
    pub noun_key: Option<String>,           // Visual noun address
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stakeholder {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub stakeholder_type: String,           // Nation | TechCompany | DefenseCompany | Military | Institution | Investor
    pub airo_type: Option<String>,          // AISubject | AIDeployer | AIDeveloper | AIProvider
    pub hover: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivicSystem {
    pub id: String,
    pub name: String,
    pub year: Option<i32>,
    pub current_status: Option<String>,
    #[serde(rename = "type")]
    pub system_type: Option<String>,
    pub hover: Option<String>,
    pub image: Option<String>,
    pub noun_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalSystem {
    pub id: String,
    pub name: String,
    pub year: Option<i32>,
    pub current_status: Option<String>,
    #[serde(rename = "type")]
    pub system_type: Option<String>,
    pub military_use: Option<String>,
    pub civic_use: Option<String>,
    pub hover: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub person_type: Option<String>,        // Owner, Investor, Founder...
    pub airo_type: Option<String>,
    pub hover: Option<String>,
    pub image: Option<String>,
}

// ── Edge Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub weight: Option<f64>,
    pub hover: Option<String>,
    pub reference: Option<String>,
}

// ── Schema Ontology (the novel part: schema-as-data) ────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRow {
    #[serde(rename = "currentStatus:airo")]
    pub current_status: Option<String>,
    #[serde(rename = "type")]
    pub node_type: Option<String>,
    #[serde(rename = "militaryUse")]
    pub military_use: Option<String>,
    #[serde(rename = "civicUse")]
    pub civic_use: Option<String>,
    #[serde(rename = "MLTask")]
    pub ml_task: Option<String>,
    #[serde(rename = "MLType")]
    pub ml_type: Option<String>,
    #[serde(rename = "purpose:vair")]
    pub purpose: Option<String>,
    #[serde(rename = "capacity:airo")]
    pub capacity: Option<String>,
    #[serde(rename = "output:airo")]
    pub output: Option<String>,
    #[serde(rename = "impact:vair")]
    pub impact: Option<String>,
    pub stakeholder: Option<String>,
    #[serde(rename = "airo:type")]
    pub airo_type: Option<String>,
}

// ── Meta-Edges (hierarchical schema graph) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaEdge {
    pub source: String,
    pub target: String,
}

// ── Full Graph Container ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiWarGraph {
    pub schema: Vec<SchemaRow>,
    pub systems: Vec<System>,
    pub civic: Vec<CivicSystem>,
    pub historical: Vec<HistoricalSystem>,
    pub stakeholders: Vec<Stakeholder>,
    pub people: Vec<Person>,
    pub edges_connection: Vec<Edge>,
    pub edges_developed: Vec<Edge>,
    pub edges_deployed: Vec<Edge>,
    pub edges_place: Vec<Edge>,
    pub edges_people: Vec<Edge>,
    pub meta_edges: Vec<MetaEdge>,
}
