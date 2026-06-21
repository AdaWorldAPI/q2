//! OSINT / Palantir-Gotham domain (classid `0x0700`) — the aiwar harvest as a
//! CANON family-basin graph, rendered through the OGAR Active-Record `ClassView`.
//!
//! The corrected model (operator-locked this session):
//!
//! * **Location is permanent + deterministic — no Louvain.** The HHTL radix trie
//!   gives the address; the basin is *assigned*, not clustered.
//! * **`family(u16)` is a basin = an interface, not a category.** Each node
//!   carries **16 × 8-bit family adapters** (the [`EdgeBlock`]) = an attention
//!   mask picking ≤16 of up to **256 basins** it adapts to. A node "implements"
//!   the basins it points at, exactly like a type implementing interfaces.
//! * **Basin = two-tier `(round << 4) | anchor`** (operator choice). High nibble
//!   = the enrichment-round theme (epstein / thiel_infrastructure /
//!   palantir_surveillance / …); low nibble = the anchor figure/org the node is
//!   most tied to within that theme. ≤16 themes × ≤16 anchors = 256 basins, each
//!   addressable by one 8-bit adapter byte.
//! * **GUID-v2 tail:** `LEAF(u16, 4th HHTL tier) | family(u16, basin) |
//!   identity(u16)`. Built with [`NodeGuid::new_v2`] (feature `guid-v2-tail`).
//! * **ClassView is DISPLAY only**, and it is the OGAR Active-Record value:
//!   `&dyn lance_graph_contract::ClassView = &OgarClassView::new()`. The contract
//!   owns the trait; OGAR owns the concrete AR implementation (the #585 SoC).
//!
//! The contract's `soa_graph::project_snapshot` reads the **v1** `family()` (u24
//! @ bytes 10..13), so it cannot read a v2 row's `family_v2()` (@ 12..14). The
//! Gotham projection here is therefore v2-aware and lives in q2 until the
//! contract grows a `project_snapshot_v2`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde_json::Value;
use tokio::sync::RwLock;

use aiwar_ingest::encounter_round::{load_encounter_rounds, EncounterRound};
use aiwar_ingest::AiWarGraph;
use lance_graph_contract::canonical_node::{EdgeBlock, NodeGuid, NodeRow};
use lance_graph_contract::class_view::{ClassView, FieldMask};
use lance_graph_contract::exploration::NarsTruth;
use lance_graph_ogar::OgarClassView;

use crate::graph_engine::{GraphEdge, GraphHealth, GraphNode, GraphSnapshot};

/// The OSINT/Gotham live snapshot — a parallel buffer to
/// [`graph_engine::live_graph`](crate::graph_engine::live_graph) holding the
/// classid-`0x0700` family-basin projection.
static OSINT_GRAPH: OnceLock<Arc<RwLock<GraphSnapshot>>> = OnceLock::new();

/// Get or initialize the OSINT/Gotham live snapshot buffer.
pub fn osint_graph() -> &'static Arc<RwLock<GraphSnapshot>> {
    OSINT_GRAPH.get_or_init(|| Arc::new(RwLock::new(GraphSnapshot::empty())))
}

/// The ClassView ClassId for the OSINT domain (low u16 of `CLASSID_OSINT`).
fn osint_class_id() -> u16 {
    (NodeGuid::CLASSID_OSINT & 0xFFFF) as u16
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
struct BasinPlan {
    /// node id → basin byte.
    node_basin: HashMap<String, u8>,
    /// node id → theme stem (for hub labels / props).
    node_theme: HashMap<String, String>,
    /// basin byte → the anchor entity id that names it (if any).
    anchor_of_basin: HashMap<u8, String>,
    /// theme stem → high-nibble index.
    theme_index: HashMap<String, u8>,
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
        adj.entry(e.source.clone()).or_default().push(e.target.clone());
        adj.entry(e.target.clone()).or_default().push(e.source.clone());
    }
    let deg = |id: &str| degree.get(id).copied().unwrap_or(0);

    // 4. per-theme anchors = top-15 by (degree desc, id asc) → low nibble 1..=15.
    let mut theme_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for n in &graph.nodes {
        let t = node_theme.get(&n.id).cloned().unwrap_or_else(|| "core".into());
        theme_nodes.entry(t).or_default().push(n.id.clone());
    }
    let mut anchor_nibble: HashMap<String, u8> = HashMap::new(); // node → its anchor nibble
    let mut anchor_of_basin: HashMap<u8, String> = HashMap::new();
    for (theme, ids) in &theme_nodes {
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
        let theme = node_theme.get(&n.id).cloned().unwrap_or_else(|| "core".into());
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

// ─────────────────────────────────────────────────────────────────────────────
// v2 OSINT rows + Gotham projection
// ─────────────────────────────────────────────────────────────────────────────

/// Build the classid-`0x0700` OSINT node rows (one per entity, in `graph.nodes`
/// order so `identity == index`). The GUID-v2 tail is `leaf=0 | family=basin |
/// identity=index`; HEEL/HIP carry the theme/anchor for deterministic HHTL
/// routing; the [`EdgeBlock`] holds the ≤16 distinct basins this node adapts to
/// (same-basin → `in_family`, cross-basin → `out_family`). Head-only.
pub fn osint_node_rows(graph: &AiWarGraph, plan: &BasinPlan) -> Vec<NodeRow> {
    let basin_of = |id: &str| plan.node_basin.get(id).copied().unwrap_or(0);
    // node id → its outgoing target ids (for the adapter mask).
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &graph.edges {
        out.entry(e.source.as_str()).or_default().push(e.target.as_str());
    }

    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let basin = basin_of(&n.id);
            let theme_hi = u16::from(basin >> 4);
            let anchor_lo = u16::from(basin & 0x0F);

            // 16 family adapters: distinct target basins this node interfaces with.
            let mut same: Vec<u8> = Vec::new();
            let mut cross: Vec<u8> = Vec::new();
            if let Some(targets) = out.get(n.id.as_str()) {
                for t in targets {
                    let tb = basin_of(t);
                    if tb == basin {
                        if !same.contains(&tb) {
                            same.push(tb);
                        }
                    } else if !cross.contains(&tb) {
                        cross.push(tb);
                    }
                }
            }
            same.sort_unstable();
            cross.sort_unstable();
            let mut edges = EdgeBlock::default();
            for (k, &b) in same.iter().take(12).enumerate() {
                edges.in_family[k] = b;
            }
            for (k, &b) in cross.iter().take(4).enumerate() {
                edges.out_family[k] = b;
            }

            NodeRow {
                key: NodeGuid::new_v2(
                    NodeGuid::CLASSID_OSINT, // classid 0x0700
                    theme_hi,                // HEEL — coarse HHTL routing by theme
                    anchor_lo,               // HIP  — anchor tier
                    0,                       // TWIG
                    0,                       // LEAF (4th HHTL tier)
                    u16::from(basin),        // family = basin byte
                    i as u16,                // identity = node index
                ),
                edges,
                value: [0u8; 480], // head-only
            }
        })
        .collect()
}

/// Project the aiwar graph into the OSINT/Gotham (neo4j) view on the cockpit's
/// [`GraphSnapshot`] wire shape: basin hubs + member entities + member-of /
/// interface edges. v2-aware (reads `family_v2`). Member display is resolved
/// through the OGAR Active-Record `ClassView` (`&dyn ClassView`).
pub fn build_osint_gotham(graph: &AiWarGraph, rounds: &[EncounterRound]) -> GraphSnapshot {
    let plan = plan_basins(graph, rounds);
    let rows = osint_node_rows(graph, &plan);

    // DISPLAY resolver — the OGAR Active-Record value, named as the contract trait
    // (#585 SoC: contract owns the trait, OGAR owns the value).
    let ogar = OgarClassView::new();
    let class_view: &dyn ClassView = &ogar;
    let osint_class = osint_class_id();
    // render rows for an OSINT instance (empty until OGAR's codebook defines
    // 0x07XX classes — graceful, but the wiring is live and exercised here).
    let osint_fields = class_view.field_count(osint_class);
    let osint_display_rows = class_view.render_rows(osint_class, FieldMask::FULL).len();

    // basin byte → entity ids in it (for hub member counts).
    let mut basin_members: HashMap<u8, usize> = HashMap::new();
    for r in &rows {
        *basin_members.entry((r.key.family_v2() & 0xFF) as u8).or_insert(0) += 1;
    }

    let mut nodes: Vec<GraphNode> = Vec::with_capacity(rows.len() + basin_members.len());

    // 1. basin hub nodes (the compartments / interfaces).
    let mut basins: Vec<u8> = basin_members.keys().copied().collect();
    basins.sort_unstable();
    // theme index → name (inverse of plan.theme_index).
    let theme_name: HashMap<u8, String> = plan
        .theme_index
        .iter()
        .map(|(k, v)| (*v, k.clone()))
        .collect();
    for &b in &basins {
        let theme = theme_name.get(&(b >> 4)).cloned().unwrap_or_else(|| "core".into());
        let anchor = plan.anchor_of_basin.get(&b).cloned();
        let label = match &anchor {
            Some(a) => format!("{theme} · {a}"),
            None => theme.clone(),
        };
        let mut props: HashMap<String, Value> = HashMap::new();
        props.insert("basin".to_string(), Value::String(format!("{b:02x}")));
        props.insert("theme".to_string(), Value::String(theme));
        props.insert(
            "members".to_string(),
            Value::from(*basin_members.get(&b).unwrap_or(&0)),
        );
        if let Some(a) = anchor {
            props.insert("anchor".to_string(), Value::String(a));
        }
        props.insert("hub".to_string(), Value::Bool(true));
        nodes.push(GraphNode {
            id: format!("basin:{b:02x}"),
            label,
            node_type: "Basin".to_string(),
            properties: props,
        });
    }

    // 2. member entity nodes (name + props rehydrated from the source graph;
    //    the projection head is what carries basin/guid — "rich on interaction").
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let src = &graph.nodes[i];
        let basin = (r.key.family_v2() & 0xFF) as u8;
        let mut props: HashMap<String, Value> = src.properties.clone();
        props.insert("guid".to_string(), Value::String(r.key.to_hex_v2()));
        props.insert("classid".to_string(), Value::String("00000700".to_string()));
        props.insert("basin".to_string(), Value::String(format!("{basin:02x}")));
        if let Some(t) = plan.node_theme.get(&src.id) {
            props.insert("theme".to_string(), Value::String(t.clone()));
        }
        // ClassView display wiring (OGAR AR): record whether the OSINT class is
        // known to the codebook yet. Empty today, lights up when 0x07XX lands.
        props.insert(
            "display_class_fields".to_string(),
            Value::from(class_view.field_count(osint_class)),
        );
        nodes.push(GraphNode {
            id: format!("osint:{}", r.key.identity_v2()),
            label: src.label.clone(),
            node_type: src.node_type.clone(),
            properties: props,
        });

        let member_id = format!("osint:{}", r.key.identity_v2());
        // member → own basin hub.
        edges.push(GraphEdge {
            source: member_id.clone(),
            target: format!("basin:{basin:02x}"),
            label: "member-of".to_string(),
            truth: NarsTruth::new(1.0, 1.0),
        });
        // interface edges: member → each adapted basin hub (the 16×8-bit mask).
        let interfaces = r
            .edges
            .in_family
            .iter()
            .chain(r.edges.out_family.iter())
            .copied()
            .filter(|&b| b != 0)
            .collect::<std::collections::BTreeSet<u8>>();
        for b in interfaces {
            if b == basin {
                continue;
            }
            edges.push(GraphEdge {
                source: member_id.clone(),
                target: format!("basin:{b:02x}"),
                label: "interfaces".to_string(),
                truth: NarsTruth::new(1.0, 1.0),
            });
        }
    }

    let node_count = nodes.len();
    let edge_count = edges.len();
    tracing::debug!(
        basins = basins.len(),
        members = rows.len(),
        osint_class_fields = osint_fields,
        osint_display_rows,
        ogar_known_classes = OgarClassView::new().known_class_ids().count(),
        "OSINT/Gotham projection built (ClassView display via OgarClassView)"
    );
    GraphSnapshot {
        nodes,
        edges,
        node_count,
        edge_count,
        scene_version: 0,
        scene_name: "osint_gotham".to_string(),
        health: GraphHealth {
            total_nodes: node_count,
            total_edges: edge_count,
            total_inferences: 0,
            contradiction_count: 0,
            confidence_avg: 1.0,
        },
        nars_inferences: Vec::new(),
    }
}

/// Hydrate the OSINT/Gotham live snapshot from aiwar data (base graph + cypher
/// enrichment rounds when the sibling `cypher/` dir is present next to
/// `data/aiwar_graph.json`).
pub async fn hydrate_osint_gotham(path: &str) -> Result<(), String> {
    let cypher_dir = Path::new(path)
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("cypher"));
    let (graph, rounds) = match &cypher_dir {
        Some(dir) if dir.is_dir() => {
            let g = aiwar_ingest::load_with_enrichment(path, dir).map_err(|e| e.to_string())?;
            let r = load_encounter_rounds(dir).unwrap_or_default();
            (g, r)
        }
        _ => (
            aiwar_ingest::load_from_file(path).map_err(|e| e.to_string())?,
            Vec::new(),
        ),
    };
    let snapshot = build_osint_gotham(&graph, &rounds);
    let node_count = snapshot.node_count;
    let edge_count = snapshot.edge_count;
    {
        let buf = osint_graph();
        let mut state = buf.write().await;
        *state = snapshot;
    }
    tracing::info!(
        "hydrated OSINT/Gotham (classid 0x0700): {} nodes, {} edges",
        node_count,
        edge_count
    );
    Ok(())
}

/// API handler: the OSINT/Gotham (classid `0x0700`) family-basin snapshot.
pub async fn osint_graph_handler() -> axum::Json<GraphSnapshot> {
    let buf = osint_graph();
    let state = buf.read().await;
    axum::Json(state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cross-theme aiwar slice. Two enrichment rounds (epstein, thiel) plus base
    // nodes; cross-basin edges so the adapter mask gets exercised.
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
        assert_eq!(theme_stem("aiwar_enrichment_epstein_v31_patch.cypher"), "epstein");
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
            // head-only: the 480-byte value slab stays zero
            assert_eq!(row.value, [0u8; 480]);
        }
    }

    #[test]
    fn basins_are_two_tier_and_adapters_point_at_basins() {
        let g = sample();
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);

        // Every adapter byte resolves to a real basin (an interface to a basin).
        let known: std::collections::HashSet<u8> =
            plan.node_basin.values().copied().collect();
        for row in &rows {
            for &b in row.edges.in_family.iter().chain(row.edges.out_family.iter()) {
                if b != 0 {
                    assert!(known.contains(&b), "adapter {b:02x} must be a real basin");
                }
            }
        }
        // ≤16 adapters per node, ≤256 basins overall.
        assert!(known.len() <= 256);
    }

    #[test]
    fn classview_display_is_the_ogar_active_record_value() {
        // The #585 SoC split: contract owns the trait, OGAR owns the value.
        let ogar = OgarClassView::new();
        let class_view: &dyn ClassView = &ogar;
        // OGAR AR is really linked: its codebook exposes the promoted classes
        // (project-mgmt / commerce / medcare). If this is 0, OGAR isn't wired.
        assert!(
            class_view_known_count(class_view) > 0 || OgarClassView::new().known_class_ids().count() > 0,
            "OgarClassView must expose its promoted class ids (OGAR AR linked)"
        );
        // OSINT (0x0700) is gracefully empty until OGAR's codebook defines 0x07XX.
        assert_eq!(class_view.field_count(osint_class_id()), 0);
    }

    fn class_view_known_count(_cv: &dyn ClassView) -> usize {
        // `known_class_ids` is on the concrete type, not the trait; this helper
        // just documents that the trait object is the display surface.
        0
    }

    #[test]
    fn gotham_view_has_basin_hubs_and_member_edges() {
        let g = sample();
        let snap = build_osint_gotham(&g, &[]);
        assert_eq!(snap.scene_name, "osint_gotham");

        // basin hub per distinct basin; every entity has a member-of edge to its hub.
        let hubs = snap.nodes.iter().filter(|n| n.node_type == "Basin").count();
        assert!(hubs >= 1, "at least one basin hub");
        let member_of = snap.edges.iter().filter(|e| e.label == "member-of").count();
        assert_eq!(member_of, g.node_count());

        // member entity names are rehydrated (not GUID hex).
        assert!(snap.nodes.iter().any(|n| n.label == "Jeffrey Epstein"));
        assert!(snap.nodes.iter().any(|n| n.label == "Peter Thiel"));

        // every member node carries the OGAR ClassView display-wiring marker.
        assert!(snap
            .nodes
            .iter()
            .filter(|n| n.node_type != "Basin")
            .all(|n| n.properties.contains_key("display_class_fields")));
    }
}
