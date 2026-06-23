//! Standalone OSINT/Gotham **SoA bake** — the pure (`aiwar-ingest` +
//! `lance-graph-contract`) half of `cockpit-server`'s `osint_gotham.rs`, lifted
//! into a light tool crate so the enriched `osint_scene.soa` asset can be
//! regenerated WITHOUT compiling cockpit-server's heavy closure (lance /
//! datafusion / arrow / deno_core). This is now the **canonical** bake: it began
//! as a verbatim mirror of `osint_gotham.rs`'s pure bake and now *leads* it — the
//! additive **tenant section** (`value[1..=6]` per node) lands here first. The
//! legacy copy in `osint_gotham.rs` (run only by its `#[ignore]`d bake test) is
//! pending the delegation de-dup. The bin (`src/main.rs`) is the regenerate
//! entrypoint.
//!
//! Why this exists: at runtime cockpit-server serves the *pre-baked* asset via
//! `include_bytes!`; `osint_soa_bytes` is a bake-time tool. Pulling it into its
//! own crate lets the asset be rebuilt on a disk-constrained host. Two layers
//! ship for the dual-use dimensions: the facet EDGES (rel 10..15, PR #44 — the
//! materialized "reference" categories) and the **tenant section** (the dynamic,
//! per-node attribute the cockpit groups across all nodes — the "residual").

// MIRROR SUPPRESSIONS. The bulk of this crate is byte-identical to the pure bake
// in cockpit-server's `osint_gotham.rs` (the single source of truth until that
// crate is wired to delegate here; `osint_soa_bytes` now additionally emits the
// tenant tail). These clippy/rustc nits are inherited from that mirror and are
// latent there too; suppressing rather than rewriting them avoids gratuitous
// drift until the de-dup lands, at which point this block — and this crate's
// copy of the shared functions — go away.
#![allow(
    clippy::collapsible_if,
    clippy::implicit_clone,
    clippy::map_unwrap_or,
    unused_mut
)]

use std::collections::HashMap;

use serde_json::Value;

use aiwar_ingest::AiWarGraph;
use aiwar_ingest::encounter_round::EncounterRound;
use lance_graph_contract::canonical_node::{EdgeBlock, NodeGuid, NodeRow};

/// The OSINT class's **order-based function/label row** — its schema, stored
/// inside the one class (not sprinkled across classids). The function defined at
/// order *i* carries label *i*; an instance inherits its label by the order it
/// carries in its value tenant. This is the consumer-side home of the schema for
/// the single OSINT class `0x0700`; the AR-native home is `ogar-vocab`'s `0x0700`
/// `ObjectView` (an upstream OGAR change, out of this repo's push scope).
/// The complete label↔order row: every label an item can carry has a slot, so
/// no item falls through to "Other" (an item with a label but no order would be
/// missing its label↔identity pairing). Orders 0-4 are the entity types; 5-6 are
/// the ontology layer the enrichment surfaces (`SchemaValue`/`SchemaAxis` +
/// `VALID_FOR`).
const OSINT_SCHEMA: &[&str] = &[
    "System",           // order 0  (N_Systems)
    "Stakeholder",      // order 1  (N_Stakeholders)
    "Person",           // order 2  (N_People)
    "CivicSystem",      // order 3  (N_Civic)
    "HistoricalSystem", // order 4  (N_Historical)
    "SchemaValue",      // order 5  (ontology: a legal value on an axis)
    "SchemaAxis",       // order 6  (ontology: a schema axis / dimension)
];

/// The value-slab byte that carries a node's class-label **order** (the per-
/// instance function/label tenant). The rest of the slab stays zero — one fixed
/// tenant byte, not serialized properties.
const CLASS_ORDER_TENANT: usize = 0;

/// Order index of a label in [`OSINT_SCHEMA`]; unknown types fall past the end
/// (resolved to "Other"), never collide with a defined order.
fn label_order(node_type: &str) -> u8 {
    OSINT_SCHEMA
        .iter()
        .position(|&s| s == node_type)
        .map(|i| i as u8)
        .unwrap_or(OSINT_SCHEMA.len() as u8)
}

/// The label inherited at an order (the class schema resolved by order).
pub fn label_of_order(order: u8) -> &'static str {
    OSINT_SCHEMA.get(order as usize).copied().unwrap_or("Other")
}

// ── Dual-use facet tenant (value-slab bytes 1..=6) ──────────────────────────
//
// The aiwar dual-use taxonomy (the AIRO/VAIR ontology: militaryUse ↔ civicUse,
// airo:type, MLType, purpose, capacity) packed as fixed-width codes into the
// SAME `[0u8; 480]` value slab that byte 0 ([`CLASS_ORDER_TENANT`]) uses for the
// class-label order. These are NOT separate `SchemaAxis`/`SchemaValue` nodes and
// NOT a per-axis family class — they are a **value tenant on the one OSINT class
// (0x0700)**, read by the same ClassView. That makes dual-use *hot*: a scan over
// the value column can filter/group by facet without touching any cold blob.
//
// Each axis is a closed codebook (the schema-as-data `SchemaValue` value-set,
// stabilised here as a sorted `&[&str]`); a facet code is `1 + index`, and `0`
// means absent/unknown (graceful — an enrichment node with no aiwar facet, or a
// value the harvest added past this codebook, simply reads 0). `airo:type` is a
// BITSET because it is compound: a node can be AIDeployer AND AISubject — the
// techno-imperial boomerang (tech fielded *and* turned on the fielder) in one
// node. Systems fill military/civic/ML/purpose/capacity; stakeholders & people
// fill the AIRO role; schema/ontology nodes fill none.
const FACET_MILITARY: usize = 1; // militaryUse — primary token, u8 code
const FACET_CIVIC: usize = 2; // civicUse    — primary token, u8 code
const FACET_AIRO_ROLE: usize = 3; // airo:type   — u8 bitset (compound)
const FACET_MLTYPE: usize = 4; // MLTask/MLTasks primary token, u8 code
const FACET_PURPOSE: usize = 5; // purpose / purpose:vair — u8 code
const FACET_CAPACITY: usize = 6; // capacity / capacity:airo — u8 code

/// `airo:type` actor roles in bit order. The four canonical AIRO players plus
/// the two rare variants the harvest carries; the game-theory structure is
/// Developer *builds* → Deployer *fields* → Subject *is targeted*, Provider
/// *supplies*. Index = bit position in [`FACET_AIRO_ROLE`].
const AIRO_ROLE: &[&str] = &[
    "AISubject",   // bit 0 — the targeted (where harm lands)
    "AIDeployer",  // bit 1 — the fielder
    "AIDeveloper", // bit 2 — the builder
    "AIProvider",  // bit 3 — the supplier
    "AIOperator",  // bit 4
    "AISupplier",  // bit 5
];

/// `militaryUse` value-set (schema legend ∪ system instances).
const MILITARY_USE: &[&str] = &[
    "Command",
    "Communications",
    "Intel",
    "Intelligence",
    "Logistics",
    "Mapping",
    "Operations",
    "Personnel",
    "Planning",
    "Prediction",
    "Robot",
];

/// `civicUse` value-set (the civilian face of the dual-use pair).
const CIVIC_USE: &[&str] = &[
    "AIAssistants",
    "AR",
    "AccessGranting",
    "Advertising",
    "AppUnlocking",
    "BehaviorEvaluation",
    "BorderPatrol",
    "Chatbots",
    "CloudComputing",
    "ConsumerTracking",
    "CrowdControl",
    "Dashboard",
    "DataBrokers",
    "DataDistribution",
    "Delivery",
    "EdgeComputing",
    "Games",
    "IdentityVerification",
    "InternetAccess",
    "LocationAnalytics",
    "Logistics",
    "Marketing",
    "NaturalLanguageProcessing",
    "Policing",
    "PrivateSecurity",
    "ProjectManagement",
    "RecommenderSystem",
    "RecommenderSystems",
    "Retired",
    "Robot",
    "ScientificResearch",
    "SecureTransmission",
    "Security",
    "SmartHome",
    "SocialWelfareSystems",
    "SupplyChainManagement",
    "Surveillance",
    "TranslationApps",
    "Unknown",
    "VR",
    "VehicleTasking",
];

/// `MLTask` / `MLType` value-set (the technical capability).
const ML_TYPE: &[&str] = &[
    "Assign",
    "Automate",
    "Capture",
    "Classification",
    "Clustering",
    "CommonSenseReasoning",
    "ComputerVision",
    "DecisionTree",
    "FaceRecognition",
    "Generate",
    "ImageGeneration",
    "InformationRetrieval",
    "KnowledgeBase",
    "ObjectDetection",
    "ObjectRecognition",
    "PatternRecognition",
    "Photogrammetry",
    "PolicyReasoning",
    "PoseEstimation",
    "Predict",
    "Ranking",
    "Recognize",
    "Recommendation",
    "SentimentAnalysis",
    "SignalAnalysis",
    "SignalTracking",
    "Sort",
    "SortingAlgorithm",
    "SpatialReasoning",
    "SpeechAnalysis",
    "SpeechSynthesis",
    "Store",
    "TemporalReasoning",
    "TextAnalysis",
    "TextGeneration",
    "TransformerModel",
    "VoiceRecognition",
];

/// `purpose:vair` value-set (the AIRO/VAIR declared purpose).
const PURPOSE_VAIR: &[&str] = &[
    "AssessingPeopleRelatedRisk",
    "AssessingRiskOfOffending",
    "DetectingCriminalOffences",
    "DetectingIndividuals",
    "DetectingLies",
    "EvaluatingEmployeePerformance",
    "EvaluatingJobCandidates",
    "IdentifyingIndividuals",
    "Monitoring",
    "PerformingBackgroundChecks",
    "PredictiveMapping",
    "ProducingRecommendation",
    "RecognizingIndividuals",
    "RemoteIdentification",
];

/// `capacity:airo` value-set (the AIRO capability the system exercises).
const CAPACITY_AIRO: &[&str] = &[
    "AudioProcessing",
    "BehaviourAnalysis",
    "BiometricCategorisation",
    "BiometricIdentification",
    "BiometricsBasedEmotionRecognition",
    "Classification",
    "ComputerVision",
    "DialectRecognition",
    "EmotionRecognition",
    "FaceRecognition",
    "Geolocation",
    "GestureRecognition",
    "ImageGeneration",
    "InformationRetrieval",
    "LieDetection",
    "NamedEntityRecognition",
    "ObjectDetection",
    "ObjectRecognition",
    "PoseEstimation",
    "Profiling",
    "RelationshipExtraction",
    "SensitiveAttributeInference",
    "SentimentAnalysis",
    "SignalTracking",
];

/// Code a single facet value (`1 + index` in its codebook; `0` = absent /
/// unknown). Compound axes (comma-joined) code their **primary** token; the
/// match is case-insensitive so harvest casing drift never silently drops.
fn facet_code(book: &[&str], value: &str) -> u8 {
    let primary = value.split(',').next().unwrap_or("").trim();
    if primary.is_empty() {
        return 0;
    }
    book.iter()
        .position(|&v| v.eq_ignore_ascii_case(primary))
        .map(|i| i as u8 + 1)
        .unwrap_or(0)
}

/// Code the (compound) `airo:type` as a bitset over [`AIRO_ROLE`]. A node that
/// is both AIDeployer and AISubject sets both bits — the boomerang made legible.
fn airo_role_bits(value: &str) -> u8 {
    let mut bits = 0u8;
    for tok in value.split(',') {
        if let Some(i) = AIRO_ROLE
            .iter()
            .position(|&r| r.eq_ignore_ascii_case(tok.trim()))
        {
            bits |= 1u8 << i;
        }
    }
    bits
}

/// Pack the dual-use facet tenant (`value[1..=6]`) from a node's source
/// properties. Absent props leave their byte at 0.
fn write_facet_tenant(value: &mut [u8; 480], props: &HashMap<String, Value>) {
    let s = |k: &str| props.get(k).and_then(|v| v.as_str());
    if let Some(v) = s("militaryUse") {
        value[FACET_MILITARY] = facet_code(MILITARY_USE, v);
    }
    if let Some(v) = s("civicUse") {
        value[FACET_CIVIC] = facet_code(CIVIC_USE, v);
    }
    if let Some(v) = s("airo:type") {
        value[FACET_AIRO_ROLE] = airo_role_bits(v);
    }
    if let Some(v) = s("MLTask").or_else(|| s("MLTasks")) {
        value[FACET_MLTYPE] = facet_code(ML_TYPE, v);
    }
    if let Some(v) = s("purpose").or_else(|| s("purpose:vair")) {
        value[FACET_PURPOSE] = facet_code(PURPOSE_VAIR, v);
    }
    if let Some(v) = s("capacity").or_else(|| s("capacity:airo")) {
        value[FACET_CAPACITY] = facet_code(CAPACITY_AIRO, v);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Basin taxonomy — two-tier (round theme → anchor), deterministic, no clustering
// ─────────────────────────────────────────────────────────────────────────────

/// Normalize an enrichment `source_file` to its theme stem: strip the
/// `aiwar[_enrichment]_` prefix, a leading `vNN_`, and trailing `_vNN` / `_patch`
/// version markers. `aiwar_enriched` / `aiwar_full` collapse to `core`.
fn theme_stem(source_file: &str) -> String {
    let s = source_file.trim_end_matches(".cypher");
    let s = s
        .strip_prefix("aiwar_enrichment_")
        .or_else(|| s.strip_prefix("aiwar_"))
        .unwrap_or(s);
    // strip a leading version segment: `vNN_...`
    let s = match s.split_once('_') {
        Some((head, tail)) if is_version_token(head) => tail,
        _ => s,
    };
    // strip trailing `_patch` then `_vNN`
    let mut s = s.to_string();
    loop {
        if let Some(t) = s.strip_suffix("_patch") {
            s = t.to_string();
            continue;
        }
        if let Some((head, tail)) = s.rsplit_once('_') {
            if is_version_token(tail) {
                s = head.to_string();
                continue;
            }
        }
        break;
    }
    if s.is_empty() || s == "enriched" || s == "full" {
        "core".to_string()
    } else {
        s
    }
}

/// `vNN` (a `v` followed by 1+ digits) — the version-segment marker.
fn is_version_token(tok: &str) -> bool {
    tok.strip_prefix('v')
        .map(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false)
}

/// The deterministic basin plan: every node → an 8-bit basin byte
/// `(theme << 4) | anchor`, plus the inverse map basin → anchor entity id (for
/// hub labels).
pub struct BasinPlan {
    /// node id → basin byte.
    pub node_basin: HashMap<String, u8>,
    /// node id → theme stem (for hub labels / props). Consumed by
    /// cockpit-server's live projection, not the SoA bake.
    pub node_theme: HashMap<String, String>,
    /// basin byte → the anchor entity id that names it (if any).
    pub anchor_of_basin: HashMap<u8, String>,
    /// theme stem → high-nibble index. Consumed by cockpit-server's live
    /// projection, not the SoA bake.
    pub theme_index: HashMap<String, u8>,
}

/// Assign every node a two-tier basin. Rounds are first-touch (lowest version
/// introduces the theme); anchors are the top-degree nodes of a theme, and each
/// member takes the anchor it is most strongly tied to. Fully deterministic.
fn plan_basins(graph: &AiWarGraph, rounds: &[EncounterRound]) -> BasinPlan {
    // 1. node → theme (first-touch round stem; base-graph nodes → "core").
    //    `load_encounter_rounds` returns rounds sorted by version ascending, so
    //    the first `or_insert` for an id is its earliest (introducing) round.
    let mut node_theme: HashMap<String, String> = HashMap::new();
    for r in rounds {
        let stem = theme_stem(&r.source_file);
        for cn in &r.nodes {
            node_theme
                .entry(cn.id.clone())
                .or_insert_with(|| stem.clone());
        }
    }
    for n in &graph.nodes {
        node_theme
            .entry(n.id.clone())
            .or_insert_with(|| "core".to_string());
    }

    // 2. theme → stable high-nibble index (core = 0, rest sorted 1..=15).
    let mut themes: Vec<String> = node_theme.values().cloned().collect();
    themes.sort();
    themes.dedup();
    let mut theme_index: HashMap<String, u8> = HashMap::new();
    theme_index.insert("core".to_string(), 0);
    let mut next: u8 = 1;
    for t in themes {
        if t == "core" {
            continue;
        }
        theme_index.entry(t).or_insert_with(|| {
            let v = next.min(15);
            next = next.saturating_add(1);
            v
        });
    }

    // 3. undirected degree + adjacency over the merged graph.
    let mut degree: HashMap<String, u32> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for e in &graph.edges {
        *degree.entry(e.source.clone()).or_insert(0) += 1;
        *degree.entry(e.target.clone()).or_insert(0) += 1;
        adj.entry(e.source.clone())
            .or_default()
            .push(e.target.clone());
        adj.entry(e.target.clone())
            .or_default()
            .push(e.source.clone());
    }
    let deg = |id: &str| degree.get(id).copied().unwrap_or(0);

    // 4. per-theme anchors = top-15 by (degree desc, id asc) → low nibble 1..=15.
    let mut theme_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for n in &graph.nodes {
        let t = node_theme
            .get(&n.id)
            .cloned()
            .unwrap_or_else(|| "core".into());
        theme_nodes.entry(t).or_default().push(n.id.clone());
    }
    let mut anchor_nibble: HashMap<String, u8> = HashMap::new(); // node → its anchor nibble
    let mut anchor_of_basin: HashMap<u8, String> = HashMap::new();
    // Iterate themes in a STABLE (sorted) order. When >15 themes exist,
    // `theme_index` clamps the overflow to ti=15, so their `(15<<4)|nib` basins
    // collide in `anchor_of_basin`; a raw HashMap iteration here would let the
    // surviving hub label depend on the hash seed — a non-deterministic bake
    // (the asset SHA changed run-to-run only in the basin-hub label tail).
    // `anchor_nibble` is per-id collision-free, so this does NOT affect node
    // family/identity/GUIDs — only which entity names a colliding ti=15 hub.
    // (This is a determinism fix that osint_gotham.rs's legacy copy also needs.)
    let mut theme_keys: Vec<&String> = theme_nodes.keys().collect();
    theme_keys.sort();
    for theme in theme_keys {
        let ids = &theme_nodes[theme];
        let ti = *theme_index.get(theme).unwrap_or(&0);
        let mut sorted = ids.clone();
        sorted.sort_by(|a, b| deg(b).cmp(&deg(a)).then_with(|| a.cmp(b)));
        for (i, id) in sorted.iter().take(15).enumerate() {
            let nib = (i as u8) + 1;
            anchor_nibble.insert(id.clone(), nib);
            anchor_of_basin.insert((ti << 4) | nib, id.clone());
        }
    }

    // 5. non-anchor members take the anchor (same theme) they are most tied to.
    let mut node_basin: HashMap<String, u8> = HashMap::new();
    for n in &graph.nodes {
        let theme = node_theme
            .get(&n.id)
            .cloned()
            .unwrap_or_else(|| "core".into());
        let ti = *theme_index.get(&theme).unwrap_or(&0);
        let nib = if let Some(nib) = anchor_nibble.get(&n.id) {
            *nib
        } else {
            // highest-degree anchor-neighbor in the same theme; else 0 (theme default).
            let mut best: Option<(u32, u8)> = None;
            if let Some(neighbors) = adj.get(&n.id) {
                for ne in neighbors {
                    if node_theme.get(ne) != Some(&theme) {
                        continue;
                    }
                    if let Some(&nib) = anchor_nibble.get(ne) {
                        let d = deg(ne);
                        if best.map(|(bd, _)| d > bd).unwrap_or(true) {
                            best = Some((d, nib));
                        }
                    }
                }
            }
            best.map(|(_, nib)| nib).unwrap_or(0)
        };
        node_basin.insert(n.id.clone(), (ti << 4) | (nib & 0x0F));
    }

    BasinPlan {
        node_basin,
        node_theme,
        anchor_of_basin,
        theme_index,
    }
}

/// The CEILING global-category pole. `HEEL = HIP = 0xFFFF` marks a node as a
/// **cross-cutting global category** (not basin-local); the first non-sentinel
/// tier below — here TWIG — sets its grain (run the sentinel deeper, through
/// TWIG, to reach a leaf-limited category). Mirrors the existing `0xFF` hub
/// convention lifted from the basin byte to the HHTL tiers; `0x0000` is the
/// opposite **floor** pole (default / fall-through).
const GLOBAL_CEILING: u16 = 0xFFFF;

/// Build the classid-`0x0700` OSINT node rows (one per entity, in `graph.nodes`
/// order so `identity == index`). The GUID-v2 tail is `leaf=0 | family=basin |
/// identity=index`; HEEL/HIP carry the theme/anchor for deterministic HHTL
/// routing; the [`EdgeBlock`] is a FLAT 16×8bit array of mixin-node adapters —
/// the ≤16 distinct relay basins this node connects through (the old 12+4
/// in/out split is waived). The value slab carries a single tenant byte — the
/// class-label order ([`OSINT_SCHEMA`]) the instance inherits its label by; the
/// rest of the slab stays zero.
pub fn osint_node_rows(graph: &AiWarGraph, plan: &BasinPlan) -> Vec<NodeRow> {
    let basin_of = |id: &str| plan.node_basin.get(id).copied().unwrap_or(0);
    // node id → its outgoing target ids (for the adapter mask).
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &graph.edges {
        out.entry(e.source.as_str())
            .or_default()
            .push(e.target.as_str());
    }

    // SchemaAxis nodes (the dual-use dimensions) are promoted to the CEILING
    // global-category pole at TWIG grain: each gets a stable TWIG (its order
    // among the SchemaAxis nodes, in graph.nodes order — deterministic). They
    // become cross-cutting categories addressable across every basin, instead
    // of being stranded basin-local (which is what islanded them).
    let axis_twig: HashMap<&str, u16> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "SchemaAxis")
        .enumerate()
        .map(|(k, n)| (n.id.as_str(), k as u16))
        .collect();

    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let basin = basin_of(&n.id);
            // ceiling pole (HEEL=HIP=0xFFFF, TWIG=axis grain) for the dimensions;
            // theme/anchor (TWIG=0) for everything basin-local.
            let (heel, hip, twig) = match axis_twig.get(n.id.as_str()) {
                Some(&t) => (GLOBAL_CEILING, GLOBAL_CEILING, t),
                None => (u16::from(basin >> 4), u16::from(basin & 0x0F), 0),
            };

            // 16×8bit mixin-node adapters (FLAT — the old 12+4 in/out split is
            // waived). The distinct nonzero *other* basins this node connects
            // through are its mixin/relay nodes; basin 0x00 is the CANON
            // default/dormant compartment (and the empty-slot sentinel), so it
            // is never an addressable mixin.
            let mut mixins: Vec<u8> = Vec::new();
            if let Some(targets) = out.get(n.id.as_str()) {
                for t in targets {
                    let tb = basin_of(t);
                    if tb != 0 && tb != basin && !mixins.contains(&tb) {
                        mixins.push(tb);
                    }
                }
            }
            mixins.sort_unstable();
            // Fill the 16-byte EdgeBlock as one flat array of mixin adapters.
            let mut edges = EdgeBlock::default();
            for (k, &b) in mixins.iter().take(16).enumerate() {
                if k < edges.in_family.len() {
                    edges.in_family[k] = b;
                } else {
                    edges.out_family[k - edges.in_family.len()] = b;
                }
            }

            // value tenant: byte 0 carries the class-label ORDER (an instance
            // inherits its label by the order it carries here, from the one
            // class's schema, OSINT_SCHEMA); bytes 1..=6 carry the dual-use
            // facet tenant (militaryUse ↔ civicUse, AIRO role, MLType, purpose,
            // capacity) packed from the source props; the rest stays zero.
            let mut value = [0u8; 480];
            value[CLASS_ORDER_TENANT] = label_order(&n.node_type);
            write_facet_tenant(&mut value, &n.properties);

            NodeRow {
                key: NodeGuid::new_v2(
                    NodeGuid::CLASSID_OSINT, // classid 0x0700 — the ONE OSINT class
                    heel,                    // HEEL — theme, or 0xFFFF ceiling for a dimension
                    hip,                     // HIP  — anchor, or 0xFFFF ceiling for a dimension
                    twig,                    // TWIG — axis grain for a ceiling-pole dimension
                    0,                       // LEAF (4th HHTL tier)
                    u16::from(basin),        // family = basin relay (mixin)
                    i as u16,                // identity = node index
                ),
                edges,
                value,
            }
        })
        .collect()
}

// ── SoA binary wire format (no JSON; the bits feed the render) ──────────────
//
// Little-endian: `magic b"OSO1"(4) | node_count u32 | edge_count u32`, then
//   node_count × [ guid: 16 B | class: u8 ]        (17 B/node)
//   edge_count × [ src: u16 | tgt: u16 | rel: u8 ] (5 B/edge)
//   node_count × [ len: u8 | utf8 name ]           (label tail, node order)
// The client decodes each 16-byte GUID → xyz (the `position()` logic ported to
// JS); `class` drives colour; edges are u16 indices into the node array; the
// label tail names each node (members in graph.nodes order, then basin hubs).

/// Magic header for the OSINT SoA wire buffer.
pub const OSINT_SOA_MAGIC: [u8; 4] = *b"OSO1";
/// Class sentinel marking a basin / family hub node.
const SOA_HUB_CLASS: u8 = 0xFF;

/// Edge-type → 1-byte code (the client colours by this).
fn rel_code(label: &str) -> u8 {
    match label {
        "member-of" => 0,
        "interfaces" => 1,
        "CONNECTED_TO" => 2,
        "DEVELOPED_BY" => 3,
        "DEPLOYED_BY" => 4,
        "PERSON_LINK" => 5,
        "USED_IN" => 6,
        "HIERARCHICAL" => 7,
        "VALID_FOR" => 8,
        _ => 9,
    }
}

fn push_edge(buf: &mut Vec<u8>, s: usize, t: usize, rel: u8) {
    buf.extend_from_slice(&(s as u16).to_le_bytes());
    buf.extend_from_slice(&(t as u16).to_le_bytes());
    buf.push(rel);
}

// Facet edge-type codes (entity → SchemaValue, one per dual-use axis). These put
// the dimensions IN the schema — traversable — beyond the orphaned
// `SchemaValue —VALID_FOR→ SchemaAxis` legend the harvest emits. They are the
// graph-structure twin of the value tenant (value[1..=6]): the tenant is the hot
// scan, these edges are the schema. rel ≥ 10 keeps them a distinct, toggleable
// layer the client can hide ("family concepts" off).
const REL_FACET_MILITARY: u8 = 10;
const REL_FACET_CIVIC: u8 = 11;
const REL_FACET_AIRO: u8 = 12;
const REL_FACET_MLTYPE: u8 = 13;
const REL_FACET_PURPOSE: u8 = 14;
const REL_FACET_CAPACITY: u8 = 15;

/// (property-key candidates, facet rel) per dual-use axis. First matching key
/// wins (e.g. `MLTask` before `MLTasks`). Mirrors the facet tenant axes.
const FACET_AXES: &[(&[&str], u8)] = &[
    (&["militaryUse"], REL_FACET_MILITARY),
    (&["civicUse"], REL_FACET_CIVIC),
    (&["airo:type"], REL_FACET_AIRO),
    (&["MLTask", "MLTasks"], REL_FACET_MLTYPE),
    (&["purpose", "purpose:vair"], REL_FACET_PURPOSE),
    (&["capacity", "capacity:airo"], REL_FACET_CAPACITY),
];

/// Entity → `SchemaValue` facet edges: for each node carrying a dual-use facet
/// property, an edge to the `SchemaValue` node for each comma-split value.
/// `SchemaValue` nodes are keyed by their value string (the ingest's id), so the
/// match is exact. This is the harvest's faceted graph (model.rs pattern #1 —
/// "nodes belong to multiple overlapping taxonomies") that its cypher never
/// actually emitted: without it the schema is a disconnected legend.
fn entity_facet_edges(graph: &AiWarGraph) -> Vec<(usize, usize, u8)> {
    let value_idx: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.node_type == "SchemaValue")
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut out = Vec::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        for (keys, rel) in FACET_AXES {
            for key in *keys {
                let Some(s) = n.properties.get(*key).and_then(|v| v.as_str()) else {
                    continue;
                };
                for tok in s.split(',') {
                    if let Some(&vi) = value_idx.get(tok.trim()) {
                        out.push((i, vi, *rel));
                    }
                }
                break; // first matching key for this axis wins
            }
        }
    }
    out
}

/// Serialize the enriched CAM SoA scene to the binary wire format above: node
/// GUIDs (decoded to xyz on the client) + class byte, then the typed link chart
/// and the member-of / interface structure as u16 index pairs. Members occupy
/// indices `0..N`; one basin/family hub follows per distinct basin.
pub fn osint_soa_bytes(graph: &AiWarGraph, rounds: &[EncounterRound]) -> Vec<u8> {
    let plan = plan_basins(graph, rounds);
    let rows = osint_node_rows(graph, &plan);
    let n_members = rows.len();

    let mut basins: Vec<u8> = rows
        .iter()
        .map(|r| (r.key.family_v2() & 0xFF) as u8)
        .collect::<std::collections::BTreeSet<u8>>()
        .into_iter()
        .collect();
    basins.sort_unstable();
    let hub_index: HashMap<u8, usize> = basins
        .iter()
        .enumerate()
        .map(|(k, &b)| (b, n_members + k))
        .collect();
    let node_count = n_members + basins.len();

    let mut nodes: Vec<u8> = Vec::with_capacity(node_count * 17);
    for r in &rows {
        nodes.extend_from_slice(r.key.as_bytes());
        nodes.push(r.value[CLASS_ORDER_TENANT]);
    }
    for &b in &basins {
        let hub = NodeGuid::new_v2(
            NodeGuid::CLASSID_OSINT,
            u16::from(b >> 4),
            u16::from(b & 0x0F),
            0,
            0,
            u16::from(b),
            0,
        );
        nodes.extend_from_slice(hub.as_bytes());
        nodes.push(SOA_HUB_CLASS);
    }

    let idx_of: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut edges: Vec<u8> = Vec::new();
    for e in &graph.edges {
        if let (Some(&s), Some(&t)) = (idx_of.get(e.source.as_str()), idx_of.get(e.target.as_str()))
        {
            push_edge(&mut edges, s, t, rel_code(&e.rel_type));
        }
    }
    // facet edges: wire each entity to the SchemaValue node for each dual-use
    // facet — the dimensions IN the schema (rel ≥ 10, a toggleable layer).
    for (s, t, rel) in entity_facet_edges(graph) {
        push_edge(&mut edges, s, t, rel);
    }
    for (i, r) in rows.iter().enumerate() {
        let basin = (r.key.family_v2() & 0xFF) as u8;
        if let Some(&h) = hub_index.get(&basin) {
            push_edge(&mut edges, i, h, rel_code("member-of"));
        }
        let ifaces: std::collections::BTreeSet<u8> = r
            .edges
            .in_family
            .iter()
            .chain(r.edges.out_family.iter())
            .copied()
            .filter(|&b| b != 0 && b != basin)
            .collect();
        for b in ifaces {
            if let Some(&h) = hub_index.get(&b) {
                push_edge(&mut edges, i, h, rel_code("interfaces"));
            }
        }
    }
    let edge_count = edges.len() / 5;

    // ── label section (OSO1, additive tail): node_count × [len u8 | utf8] in
    //    node order — members in graph.nodes order, then basin hubs. This is the
    //    inverse-AST-parser materialising the entity NAMES into the wire so the
    //    foveal view can show "Fortify", not just a class. Old readers stop after
    //    `edges`; new readers consume this tail.
    let mut labels: Vec<u8> = Vec::new();
    let mut push_label = |buf: &mut Vec<u8>, s: &str| {
        let b = s.as_bytes();
        let l = b.len().min(255);
        buf.push(l as u8);
        buf.extend_from_slice(&b[..l]);
    };
    for n in &graph.nodes {
        let nm = if n.label.is_empty() {
            n.id.as_str()
        } else {
            n.label.as_str()
        };
        push_label(&mut labels, nm);
    }
    for &b in &basins {
        let nm = plan
            .anchor_of_basin
            .get(&b)
            .cloned()
            .unwrap_or_else(|| format!("family {b:02x}"));
        push_label(&mut labels, &nm);
    }

    // ── tenant section (OSO1, additive tail): node_count × 6 facet bytes
    //    (`value[1..=6]`: militaryUse, civicUse, airo:type, MLType, purpose,
    //    capacity) in node order — members in graph.nodes order, then basin hubs
    //    (zeroed; a hub carries no facet). This is the DYNAMIC, per-node
    //    attribute the client groups by across all nodes (the residual layer);
    //    the facet EDGES (rel 10..15) are its materialized twin (the reference
    //    layer). Old readers stop after the label tail; new readers consume this.
    let mut tenants: Vec<u8> = Vec::with_capacity(node_count * 6);
    for r in &rows {
        tenants.extend_from_slice(&r.value[FACET_MILITARY..=FACET_CAPACITY]);
    }
    for _ in &basins {
        tenants.extend_from_slice(&[0u8; 6]);
    }

    let mut out =
        Vec::with_capacity(12 + nodes.len() + edges.len() + labels.len() + tenants.len());
    out.extend_from_slice(&OSINT_SOA_MAGIC);
    out.extend_from_slice(&(node_count as u32).to_le_bytes());
    out.extend_from_slice(&(edge_count as u32).to_le_bytes());
    out.extend_from_slice(&nodes);
    out.extend_from_slice(&edges);
    out.extend_from_slice(&labels);
    out.extend_from_slice(&tenants);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "N_Stakeholders": [
            {"id": "Palantir", "name": "Palantir Technologies", "type": "company"},
            {"id": "Epstein", "name": "Jeffrey Epstein", "type": "person"},
            {"id": "Thiel", "name": "Peter Thiel", "type": "person"}
        ],
        "N_People": [
            {"id": "Karp", "name": "Alex Karp"}
        ],
        "E_connection": [
            {"source": "Thiel", "target": "Palantir", "weight": 3},
            {"source": "Karp", "target": "Palantir"},
            {"source": "Epstein", "target": "Thiel"}
        ]
    }"#;

    fn sample() -> AiWarGraph {
        aiwar_ingest::load_from_str(SAMPLE).expect("valid sample")
    }

    #[test]
    fn theme_stem_normalizes_version_markers() {
        assert_eq!(
            theme_stem("aiwar_enrichment_epstein_v31_patch.cypher"),
            "epstein"
        );
        assert_eq!(
            theme_stem("aiwar_enrichment_thiel_infrastructure.cypher"),
            "thiel_infrastructure"
        );
        assert_eq!(
            theme_stem("aiwar_enrichment_v40_surveillance_ecosystem.cypher"),
            "surveillance_ecosystem"
        );
        assert_eq!(theme_stem("aiwar_enriched.cypher"), "core");
        assert_eq!(theme_stem("aiwar_full.cypher"), "core");
    }

    #[test]
    fn rows_fill_the_osint_domain_with_v2_basin_tail() {
        let g = sample();
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);
        assert_eq!(rows.len(), g.node_count(), "one OSINT row per entity");
        for (i, row) in rows.iter().enumerate() {
            // classid 0x0700 (the OSINT domain byte is 0x07)
            assert_eq!(row.key.classid(), NodeGuid::CLASSID_OSINT);
            assert_eq!(row.key.classid() >> 8, 0x07);
            // GUID-v2 tail: identity == index, family == basin byte (high byte 0)
            assert_eq!(row.key.identity_v2(), i as u16);
            assert_eq!(row.key.family_v2() >> 8, 0, "basin is an 8-bit byte");
            // value tenant: byte 0 = class-label order, bytes 1..=6 = the
            // dual-use facet tenant (zero in this fixture — no aiwar facet
            // props), the rest of the slab stays zero.
            let order = row.value[CLASS_ORDER_TENANT];
            assert!((order as usize) <= OSINT_SCHEMA.len(), "order in range");
            assert!(
                row.value[FACET_CAPACITY + 1..].iter().all(|&b| b == 0),
                "nothing past the facet tenant is set"
            );
        }
    }

    #[test]
    fn dual_use_facets_pack_into_the_value_tenant() {
        // A System carries the dual-use pair + ML/purpose/capacity; an actor
        // carries the AIRO role. The boomerang stakeholder is BOTH AIDeployer
        // and AISubject — both bits set in one node (tech fielded AND turned on
        // the fielder).
        const DU: &str = r#"{
            "N_Systems": [
                {"id": "Lavender", "name": "Lavender",
                 "militaryUse": "Intelligence",
                 "civicUse": "RecommenderSystem, BehaviorEvaluation",
                 "MLTask": "Predict",
                 "purpose": "AssessingRiskOfOffending",
                 "capacity": "Profiling"}
            ],
            "N_Stakeholders": [
                {"id": "Boomerang", "name": "Boomerang Nation", "type": "Nation",
                 "airo:type": "AIDeployer, AISubject"}
            ]
        }"#;
        let g = aiwar_ingest::load_from_str(DU).expect("valid dual-use fixture");
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);

        let lav = g.nodes.iter().position(|n| n.id == "Lavender").unwrap();
        let lv = &rows[lav].value;
        // militaryUse "Intelligence" is index 3 in MILITARY_USE → code 4.
        assert_eq!(lv[FACET_MILITARY], 4, "militaryUse coded by codebook index");
        // civicUse primary token "RecommenderSystem" coded; the compound second
        // token is dropped at v1.
        assert_ne!(lv[FACET_CIVIC], 0, "civicUse primary token coded");
        assert_ne!(lv[FACET_MLTYPE], 0, "MLTask coded");
        assert_ne!(lv[FACET_PURPOSE], 0, "purpose coded");
        assert_ne!(lv[FACET_CAPACITY], 0, "capacity coded");
        assert_eq!(
            lv[FACET_AIRO_ROLE], 0,
            "a System carries no AIRO actor role"
        );

        let boom = g.nodes.iter().position(|n| n.id == "Boomerang").unwrap();
        let bv = &rows[boom].value;
        // AISubject = bit 0, AIDeployer = bit 1 → 0b011. The boomerang in one node.
        assert_eq!(
            bv[FACET_AIRO_ROLE], 0b011,
            "deployer AND subject — the boomerang"
        );
        assert_eq!(
            bv[FACET_MILITARY], 0,
            "a stakeholder carries no system facet"
        );

        // the tenant stays within bytes 0..=6; the rest of the slab is zero.
        assert!(lv[FACET_CAPACITY + 1..].iter().all(|&b| b == 0));
        assert!(bv[FACET_CAPACITY + 1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn label_inherits_by_order_from_the_one_class_schema() {
        // ONE class (0x0700); the label rides at the order the function is
        // defined in the schema — no per-class sprinkling.
        assert_eq!(label_order("System"), 0);
        assert_eq!(label_order("Stakeholder"), 1);
        assert_eq!(label_order("Person"), 2);
        assert_eq!(label_of_order(0), "System");
        assert_eq!(label_of_order(2), "Person");
        // unknown types fall past the defined orders (never collide).
        assert_eq!(label_order("Nation"), OSINT_SCHEMA.len() as u8);
        assert_eq!(label_of_order(OSINT_SCHEMA.len() as u8), "Other");

        // every row's value-tenant order resolves back to its source category.
        let g = sample();
        let plan = plan_basins(&g, &[]);
        for (i, row) in osint_node_rows(&g, &plan).iter().enumerate() {
            let order = row.value[CLASS_ORDER_TENANT];
            assert_eq!(label_of_order(order), g.nodes[i].node_type);
        }
    }

    #[test]
    fn basins_are_two_tier_and_adapters_point_at_basins() {
        let g = sample();
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);

        // Every adapter byte resolves to a real basin (an interface to a basin).
        let known: std::collections::HashSet<u8> = plan.node_basin.values().copied().collect();
        for row in &rows {
            for &b in row
                .edges
                .in_family
                .iter()
                .chain(row.edges.out_family.iter())
            {
                if b != 0 {
                    assert!(known.contains(&b), "adapter {b:02x} must be a real basin");
                }
            }
        }
        // ≤16 adapters per node, ≤256 basins overall.
        assert!(known.len() <= 256);
    }

    #[test]
    fn facet_edges_wire_entities_to_schema_values() {
        // A tiny faceted graph: a System + a Stakeholder, plus the SchemaValue
        // nodes their facets point at. entity_facet_edges must wire each entity
        // to the value(s) it carries — the dimensions IN the schema, not a legend.
        let gnode = |id: &str, ty: &str, props: &[(&str, &str)]| aiwar_ingest::GraphNode {
            id: id.to_string(),
            label: id.to_string(),
            node_type: ty.to_string(),
            properties: props
                .iter()
                .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
                .collect(),
        };
        let g = AiWarGraph {
            nodes: vec![
                gnode(
                    "Lattice",
                    "System",
                    &[
                        ("militaryUse", "Intelligence"),
                        ("civicUse", "Policing, CrowdControl"),
                    ],
                ),
                gnode("Intelligence", "SchemaValue", &[]),
                gnode("Policing", "SchemaValue", &[]),
                gnode("CrowdControl", "SchemaValue", &[]),
                gnode(
                    "Israel",
                    "Stakeholder",
                    &[("airo:type", "AIDeployer, AISubject")],
                ),
                gnode("AIDeployer", "SchemaValue", &[]),
                gnode("AISubject", "SchemaValue", &[]),
            ],
            edges: vec![],
        };
        let fe = entity_facet_edges(&g);
        // System → its militaryUse value, and BOTH civicUse values (compound split).
        assert!(fe.contains(&(0, 1, REL_FACET_MILITARY)), "militaryUse edge");
        assert!(fe.contains(&(0, 2, REL_FACET_CIVIC)), "civicUse Policing");
        assert!(
            fe.contains(&(0, 3, REL_FACET_CIVIC)),
            "civicUse CrowdControl"
        );
        // Stakeholder → BOTH airo roles (the boomerang, as two edges).
        assert!(fe.contains(&(4, 5, REL_FACET_AIRO)), "airo AIDeployer");
        assert!(fe.contains(&(4, 6, REL_FACET_AIRO)), "airo AISubject");
        // SchemaValue nodes never source a facet edge; exactly five edges, no extras.
        assert!(!fe.iter().any(|&(s, _, _)| (1..=3).contains(&s) || s >= 5));
        assert_eq!(fe.len(), 5);
    }

    #[test]
    fn soa_bytes_have_a_parseable_header() {
        let g = sample();
        let bytes = osint_soa_bytes(&g, &[]);
        assert_eq!(&bytes[0..4], &OSINT_SOA_MAGIC);
        let nodes = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let edges = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        // members + at least one basin hub; the fixed node+edge records are a
        // lower bound — the OSO1 label tail (additive, node_count × [len|utf8])
        // follows, so total bytes exceed the fixed records. (The original
        // assertion in osint_gotham.rs hard-equated to the fixed size; it
        // predates the label tail and is stale there — see deferred cleanup.)
        assert!(nodes > g.node_count());
        let fixed = 12 + nodes * 17 + edges * 5;
        assert!(bytes.len() >= fixed, "fixed records are a lower bound");
        assert!(bytes.len() > fixed, "additive label tail present");
    }

    #[test]
    fn tenant_section_carries_the_facet_codes() {
        // The dynamic per-node attribute layer: `value[1..=6]` shipped as the
        // trailing tenant section (6 bytes/node) — the residual twin of the facet
        // edges. Baked from the same rows, so the tail must mirror them exactly.
        const DU: &str = r#"{
            "N_Systems": [
                {"id": "Lavender", "name": "Lavender",
                 "militaryUse": "Intelligence",
                 "civicUse": "Surveillance",
                 "MLTask": "Predict",
                 "purpose": "Monitoring",
                 "capacity": "Profiling"}
            ]
        }"#;
        let g = aiwar_ingest::load_from_str(DU).expect("valid dual-use fixture");
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);
        let bytes = osint_soa_bytes(&g, &[]);

        let node_count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        // the tenant section is the final node_count × 6 bytes (the last tail).
        let tail = &bytes[bytes.len() - node_count * 6..];

        // member tenants come first, in graph.nodes order → index == row index.
        let lav = g.nodes.iter().position(|n| n.id == "Lavender").unwrap();
        assert_eq!(
            &tail[lav * 6..lav * 6 + 6],
            &rows[lav].value[FACET_MILITARY..=FACET_CAPACITY],
            "tenant tail mirrors value[1..=6]"
        );
        assert!(
            tail[lav * 6..lav * 6 + 6].iter().any(|&b| b != 0),
            "Lavender carries facet codes in its tenant"
        );
        // basin-hub tenants (after the members) are zero — a hub has no facet.
        assert!(
            tail[rows.len() * 6..].iter().all(|&b| b == 0),
            "basin hubs carry a zero tenant"
        );
    }

    #[test]
    fn schema_axes_are_ceiling_pole_global_categories() {
        // The dual-use dimensions are promoted to the 0xFFFF CEILING pole at
        // TWIG grain: cross-cutting global categories (HEEL=HIP=sentinel), the
        // axis index in TWIG. Entities stay basin-local (theme/anchor in
        // HEEL/HIP, TWIG unused). The address says "global" without an edge.
        let node = |id: &str, ty: &str| aiwar_ingest::GraphNode {
            id: id.to_string(),
            label: id.to_string(),
            node_type: ty.to_string(),
            properties: HashMap::new(),
        };
        let g = AiWarGraph {
            nodes: vec![
                node("Lattice", "System"),
                node("militaryUse", "SchemaAxis"),
                node("civicUse", "SchemaAxis"),
            ],
            edges: vec![],
        };
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);

        let mu = g.nodes.iter().position(|n| n.id == "militaryUse").unwrap();
        let cu = g.nodes.iter().position(|n| n.id == "civicUse").unwrap();
        // both axes sit at the ceiling pole, with DISTINCT twig grains.
        assert_eq!(rows[mu].key.heel(), 0xFFFF, "axis HEEL = ceiling sentinel");
        assert_eq!(rows[mu].key.hip(), 0xFFFF, "axis HIP = ceiling sentinel");
        assert_eq!(rows[cu].key.heel(), 0xFFFF);
        assert_eq!(rows[cu].key.hip(), 0xFFFF);
        assert_ne!(
            rows[mu].key.twig(),
            rows[cu].key.twig(),
            "each axis is a distinct twig-grain category"
        );
        // identity/family are still the fine address (unchanged).
        assert_eq!(rows[mu].key.identity_v2(), mu as u16);

        // a normal entity stays basin-local — NOT the ceiling sentinel.
        let sys = g.nodes.iter().position(|n| n.id == "Lattice").unwrap();
        assert_ne!(rows[sys].key.heel(), 0xFFFF, "entity is basin-local");
        assert_eq!(rows[sys].key.twig(), 0, "entity TWIG unused");
    }
}
