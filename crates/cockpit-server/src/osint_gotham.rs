//! OSINT / Palantir-Gotham domain (classid `0x0700`) — the aiwar harvest as a
//! CANON family-basin graph, rendered through the OGAR Active-Record `ClassView`.
//!
//! The corrected model (operator-locked this session):
//!
//! * **Location is permanent + deterministic — no Louvain.** The HHTL radix trie
//!   gives the address; the basin is *assigned*, not clustered.
//! * **`family(u16)` is a basin = an interface, not a category.** Each node
//!   carries **16 × 8-bit mixin-node adapters** (the [`EdgeBlock`], flat — the
//!   old 12+4 in/out split is waived) = an attention mask picking ≤16 of up to
//!   **256 mixin/relay nodes** it connects through. A node "implements" the
//!   basins it points at, exactly like a type implementing interfaces.
//!   (Separately, a 16×8bit / 32×4bit value tenant is available as a
//!   label/function-by-masking option; here the label rides a single order byte.)
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
//! * **One class, order-based labels.** Every node is the single OSINT class
//!   `0x0700`; the entity labels (System / Stakeholder / Person / …) are an
//!   order-based function/label row ([`OSINT_SCHEMA`]) — the class's schema. An
//!   instance inherits its label by the order it carries in its value tenant.
//!   No per-label classids (no sprinkling).
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
fn label_of_order(order: u8) -> &'static str {
    OSINT_SCHEMA.get(order as usize).copied().unwrap_or("Other")
}

/// Golden angle (radians) — the φ-spiral / Vogel constant.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// Basin cluster centre in 3D: basins spread on a golden-angle spiral so each
/// compartment is its own region of space. This is the coarse tier of the
/// address→coordinate decode (the HHTL `family` byte → a region).
fn basin_center(basin: u8) -> [f32; 3] {
    let b = basin as f32;
    let r = 40.0 * (b + 1.0).sqrt();
    let a = b * GOLDEN_ANGLE;
    [r * a.cos(), 0.0, r * a.sin()]
}

/// **The GUID is a 3D coordinate.** Decode a node address to `[x,y,z]` — no
/// separate layout pass: `family` (basin) picks the region (coarse octree-ish
/// tier), `identity` places the node on a Vogel/φ-spiral disc inside it (the
/// helix sub-position), and `HEEL` (theme) lifts it on the y axis. Nodes are
/// points, edges are 3D lines, family mixins are spatial neighbours — the same
/// decode that would place a CAD primitive or a Gaussian splat.
fn position(key: &NodeGuid) -> [f32; 3] {
    let c = basin_center((key.family_v2() & 0xFF) as u8);
    let id = key.identity_v2() as f32;
    let r = 6.0 * (id + 1.0).sqrt();
    let a = id * GOLDEN_ANGLE;
    let y = key.heel() as f32 * 0.5;
    [c[0] + r * a.cos(), c[1] + y, c[2] + r * a.sin()]
}

/// Insert decoded `x`/`y`/`z` into a node's property bag (the 3D-scene emit).
fn insert_xyz(props: &mut HashMap<String, Value>, p: [f32; 3]) {
    props.insert("x".to_string(), Value::from(p[0] as f64));
    props.insert("y".to_string(), Value::from(p[1] as f64));
    props.insert("z".to_string(), Value::from(p[2] as f64));
}

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

            // value tenant: the class-label ORDER. An instance inherits its
            // label by the order it carries here, from the one class's schema
            // (OSINT_SCHEMA). One fixed byte; the rest of the slab stays zero.
            let mut value = [0u8; 480];
            value[CLASS_ORDER_TENANT] = label_order(&n.node_type);

            NodeRow {
                key: NodeGuid::new_v2(
                    NodeGuid::CLASSID_OSINT, // classid 0x0700 — the ONE OSINT class
                    theme_hi,                // HEEL — coarse HHTL routing by theme
                    anchor_lo,               // HIP  — anchor tier
                    0,                       // TWIG
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
        insert_xyz(&mut props, basin_center(b)); // hub sits at its basin's 3D centre
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
        // class label is inherited by ORDER from the one class's schema: the
        // value tenant carries the order, OSINT_SCHEMA resolves the label.
        let order = r.value[CLASS_ORDER_TENANT];
        let class_label = label_of_order(order);
        let mut props: HashMap<String, Value> = src.properties.clone();
        props.insert("guid".to_string(), Value::String(r.key.to_hex_v2()));
        props.insert("classid".to_string(), Value::String("00000700".to_string()));
        props.insert("class_order".to_string(), Value::from(order));
        props.insert("class".to_string(), Value::String(class_label.to_string()));
        props.insert("basin".to_string(), Value::String(format!("{basin:02x}")));
        if let Some(t) = plan.node_theme.get(&src.id) {
            props.insert("theme".to_string(), Value::String(t.clone()));
        }
        // ClassView display wiring (OGAR AR): the display adapter is OgarClassView
        // held as `&dyn ClassView`. 0x0700 is empty in OGAR's codebook today, so
        // the label inherits by order from OSINT_SCHEMA (the class schema) until
        // the 0x0700 ObjectView is defined upstream in ogar-vocab.
        props.insert(
            "display_class_fields".to_string(),
            Value::from(class_view.field_count(osint_class)),
        );
        // the GUID decodes to a 3D coordinate — node = point in space.
        insert_xyz(&mut props, position(&r.key));
        nodes.push(GraphNode {
            id: format!("osint:{}", r.key.identity_v2()),
            label: src.label.clone(),
            node_type: class_label.to_string(), // inherited by order, not a free string
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

    // 3. typed entity→entity link chart — the actual Gotham/neo4j relationships
    //    (CONNECTED_TO, DEVELOPED_BY, PERSON_LINK, VALID_FOR, …). identity == node
    //    index, so each source edge maps to osint:{idx(src)} → osint:{idx(tgt)};
    //    endpoints that don't resolve to a real node are skipped. This is the
    //    link-analysis substance the basin overlay routes — without it the view
    //    is a clustering diagram, not a link chart.
    let id_to_idx: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    for e in &graph.edges {
        let (Some(&si), Some(&ti)) = (
            id_to_idx.get(e.source.as_str()),
            id_to_idx.get(e.target.as_str()),
        ) else {
            continue;
        };
        let freq = (e.weight as f32).clamp(0.0, 1.0);
        edges.push(GraphEdge {
            source: format!("osint:{si}"),
            target: format!("osint:{ti}"),
            label: e.rel_type.clone(),
            truth: NarsTruth::new(if freq > 0.0 { freq } else { 1.0 }, 0.8),
        });
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

// ── SoA binary wire format (no JSON; the bits feed the render) ──────────────
//
// Little-endian: `magic b"OSO1"(4) | node_count u32 | edge_count u32`, then
//   node_count × [ guid: 16 B | class: u8 ]        (17 B/node)
//   edge_count × [ src: u16 | tgt: u16 | rel: u8 ] (5 B/edge)
// The client decodes each 16-byte GUID → xyz (the `position()` logic ported to
// JS); `class` drives colour; edges are u16 indices into the node array.

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
        if let (Some(&s), Some(&t)) =
            (idx_of.get(e.source.as_str()), idx_of.get(e.target.as_str()))
        {
            push_edge(&mut edges, s, t, rel_code(&e.rel_type));
        }
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

    let mut out = Vec::with_capacity(12 + nodes.len() + edges.len());
    out.extend_from_slice(&OSINT_SOA_MAGIC);
    out.extend_from_slice(&(node_count as u32).to_le_bytes());
    out.extend_from_slice(&(edge_count as u32).to_le_bytes());
    out.extend_from_slice(&nodes);
    out.extend_from_slice(&edges);
    out
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
            // value tenant: one byte carries the class-label order; rest zero.
            let order = row.value[CLASS_ORDER_TENANT];
            assert!((order as usize) <= OSINT_SCHEMA.len(), "order in range");
            assert!(
                row.value[CLASS_ORDER_TENANT + 1..].iter().all(|&b| b == 0),
                "only the class-order tenant byte is set"
            );
        }
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
            ogar.known_class_ids().count() > 0,
            "OgarClassView must expose its promoted class ids (OGAR AR linked)"
        );
        // The trait object IS the display surface; OSINT (0x0700) resolves to
        // zero rows until OGAR's codebook defines 0x07XX classes (graceful).
        assert_eq!(class_view.field_count(osint_class_id()), 0);
        assert_eq!(
            class_view.render_rows(osint_class_id(), FieldMask::FULL).len(),
            0
        );
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

    #[test]
    fn guid_decodes_to_a_3d_position() {
        let g = sample();
        let plan = plan_basins(&g, &[]);
        let rows = osint_node_rows(&g, &plan);

        // deterministic: the address alone fixes the coordinate (no layout pass).
        assert_eq!(position(&rows[0].key), position(&rows[0].key));

        // every address decodes to a DISTINCT point.
        let pts: std::collections::BTreeSet<[u32; 3]> = rows
            .iter()
            .map(|r| {
                let p = position(&r.key);
                [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
            })
            .collect();
        assert_eq!(pts.len(), rows.len(), "distinct addresses → distinct points");

        // distinct basins occupy distinct regions of space.
        assert_ne!(basin_center(1), basin_center(2));

        // the projection emits x/y/z on every node — members AND basin hubs.
        let snap = build_osint_gotham(&g, &[]);
        assert!(snap.nodes.iter().all(|n| {
            n.properties.contains_key("x")
                && n.properties.contains_key("y")
                && n.properties.contains_key("z")
        }));
    }

    /// One-shot evaluation harness over the REAL aiwar harvest (base 221 + 30
    /// enrichment rounds → the ~650 set). Ignored by default; run explicitly:
    /// `cargo test -p cockpit-server -- --ignored --nocapture eval_650`.
    /// Skips cleanly when the harvest data is absent.
    #[test]
    #[ignore = "reads the on-disk aiwar harvest; run with --ignored --nocapture"]
    fn eval_650_full_enrichment_report() {
        use std::collections::BTreeMap;

        let base_candidates = [
            "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
            "cockpit/public/aiwar_graph.json",
            "../../cockpit/public/aiwar_graph.json",
        ];
        let Some(path) = base_candidates.iter().find(|p| Path::new(p).exists()) else {
            eprintln!("aiwar harvest not found; skipping eval");
            return;
        };
        let cypher_dir = Path::new(path)
            .parent()
            .and_then(|p| p.parent())
            .map(|r| r.join("cypher"));

        // ── full enrichment ──
        let g = match &cypher_dir {
            Some(d) if d.is_dir() => {
                aiwar_ingest::load_with_enrichment(path, d).expect("load+enrich")
            }
            _ => aiwar_ingest::load_from_file(path).expect("load base"),
        };
        let rounds = cypher_dir
            .as_ref()
            .and_then(|d| load_encounter_rounds(d).ok())
            .unwrap_or_default();

        eprintln!("\n══════════ OSINT 650-set evaluation ══════════");
        eprintln!("source: {path}");
        eprintln!("enrichment rounds applied: {}", rounds.len());
        eprintln!("\n── enriched property graph ──");
        eprintln!("nodes: {}", g.node_count());
        eprintln!("edges: {}", g.edge_count());

        let mut ntypes: BTreeMap<&str, usize> = BTreeMap::new();
        for n in &g.nodes {
            *ntypes.entry(n.node_type.as_str()).or_default() += 1;
        }
        eprintln!("node types:");
        for (t, c) in &ntypes {
            eprintln!("   {t:<18} {c}");
        }
        // label↔identity coverage: every item's type must have a slot in the
        // class schema (else it resolves to "Other" — a missing label↔identity).
        let missing: BTreeMap<&str, usize> = g
            .nodes
            .iter()
            .filter(|n| label_order(&n.node_type) as usize >= OSINT_SCHEMA.len())
            .fold(BTreeMap::new(), |mut m, n| {
                *m.entry(n.node_type.as_str()).or_default() += 1;
                m
            });
        eprintln!(
            "items missing label↔identity (type ∉ OSINT_SCHEMA): {} {:?}",
            missing.values().sum::<usize>(),
            missing
        );

        // typed link structure = the Gotham richness
        let mut rtypes: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &g.edges {
            *rtypes.entry(e.rel_type.as_str()).or_default() += 1;
        }
        eprintln!("edge rel-types ({} distinct):", rtypes.len());
        for (t, c) in &rtypes {
            eprintln!("   {t:<18} {c}");
        }

        // property richness
        let mut pkeys: BTreeMap<String, usize> = BTreeMap::new();
        for n in &g.nodes {
            for k in n.properties.keys() {
                *pkeys.entry(k.clone()).or_default() += 1;
            }
        }
        let avg_props = g.nodes.iter().map(|n| n.properties.len()).sum::<usize>() as f64
            / g.node_count().max(1) as f64;
        eprintln!(
            "node property keys: {} distinct, avg {avg_props:.1} props/node",
            pkeys.len()
        );
        let mut top_pk: Vec<_> = pkeys.iter().collect();
        top_pk.sort_by(|a, b| b.1.cmp(a.1));
        eprint!("   top keys:");
        for (k, c) in top_pk.iter().take(12) {
            eprint!(" {k}({c})");
        }
        eprintln!();

        // degree distribution (link-analysis signal)
        let mut deg: HashMap<&str, usize> = HashMap::new();
        for e in &g.edges {
            *deg.entry(e.source.as_str()).or_default() += 1;
            *deg.entry(e.target.as_str()).or_default() += 1;
        }
        let mut degv: Vec<_> = deg.iter().collect();
        degv.sort_by(|a, b| b.1.cmp(a.1));
        eprintln!("highest-degree entities (link hubs):");
        for (id, d) in degv.iter().take(10) {
            let label = g
                .nodes
                .iter()
                .find(|n| n.id.as_str() == **id)
                .map_or(**id, |n| n.label.as_str());
            eprintln!("   {d:>3}  {label}");
        }
        let isolated = g
            .nodes
            .iter()
            .filter(|n| !deg.contains_key(n.id.as_str()))
            .count();
        eprintln!("isolated nodes (degree 0): {isolated}");

        // ── OSINT/Gotham projection (current) ──
        let snap = build_osint_gotham(&g, &rounds);
        let plan = plan_basins(&g, &rounds);
        let rows = osint_node_rows(&g, &plan);

        eprintln!("\n── OSINT/Gotham projection (current) ──");
        let hubs = snap.nodes.iter().filter(|n| n.node_type == "Basin").count();
        eprintln!(
            "projection nodes: {} ({hubs} basin hubs + {} members)",
            snap.node_count,
            snap.node_count - hubs
        );
        let mut elabels: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &snap.edges {
            *elabels.entry(e.label.as_str()).or_default() += 1;
        }
        eprintln!("projection edges: {} (by label)", snap.edge_count);
        for (l, c) in &elabels {
            eprintln!("   {l:<14} {c}");
        }

        let mut bsizes: BTreeMap<u8, usize> = BTreeMap::new();
        for r in &rows {
            *bsizes.entry((r.key.family_v2() & 0xFF) as u8).or_default() += 1;
        }
        eprintln!("basins: {} compartments", bsizes.len());
        let mut bv: Vec<_> = bsizes.iter().collect();
        bv.sort_by(|a, b| b.1.cmp(a.1));
        eprint!("   sizes (top): ");
        for (b, c) in bv.iter().take(12) {
            eprint!("{b:02x}:{c} ");
        }
        eprintln!();

        let nonzero = |r: &NodeRow| {
            r.edges
                .in_family
                .iter()
                .chain(r.edges.out_family.iter())
                .filter(|&&b| b != 0)
                .count()
        };
        let total_adapters: usize = rows.iter().map(nonzero).sum();
        let saturated = rows.iter().filter(|r| nonzero(r) == 16).count();
        eprintln!(
            "mixin adapters: {:.1} avg/node, {saturated} nodes saturated (16/16)",
            total_adapters as f64 / rows.len().max(1) as f64
        );

        // ── the gap ──
        let proj_entity_edges = snap
            .edges
            .iter()
            .filter(|e| !e.target.starts_with("basin:"))
            .count();
        eprintln!("\n── link chart (entity→entity) ──");
        eprintln!("typed edges in source    : {}", g.edge_count());
        eprintln!("typed edges in OSINT view: {proj_entity_edges}");
        eprintln!("══════════════════════════════════════════════\n");

        assert!(g.node_count() > 221, "enrichment grew the graph past base");
    }

    /// One-shot BAKE: pre-enrich the CAM SoA scene to the BINARY wire buffer
    /// (`assets/osint_scene.soa`) so the deploy serves the full enriched view as
    /// raw SoA bytes — no cypher at runtime, no JSON. The cypher harvest is used
    /// only here, at bake time. Ignored; run once after enrichment changes:
    /// `cargo test -p cockpit-server --bin q2-cockpit -- --ignored --nocapture bake_osint_soa`
    #[test]
    #[ignore = "bakes assets/osint_scene.soa from the on-disk harvest; run once"]
    fn bake_osint_soa() {
        let base_candidates = [
            "/home/user/aiwar-neo4j-harvest/data/aiwar_graph.json",
            "cockpit/public/aiwar_graph.json",
            "../../cockpit/public/aiwar_graph.json",
        ];
        let Some(path) = base_candidates.iter().find(|p| Path::new(p).exists()) else {
            eprintln!("harvest not found; cannot bake");
            return;
        };
        let cypher_dir = Path::new(path)
            .parent()
            .and_then(|p| p.parent())
            .map(|r| r.join("cypher"));
        let graph = match &cypher_dir {
            Some(d) if d.is_dir() => {
                aiwar_ingest::load_with_enrichment(path, d).expect("load+enrich")
            }
            _ => aiwar_ingest::load_from_file(path).expect("load base"),
        };
        let rounds = cypher_dir
            .as_ref()
            .and_then(|d| load_encounter_rounds(d).ok())
            .unwrap_or_default();
        let bytes = osint_soa_bytes(&graph, &rounds);
        let dir = format!("{}/assets", env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir_all(&dir).expect("mkdir assets");
        let out = format!("{dir}/osint_scene.soa");
        std::fs::write(&out, &bytes).expect("write osint_scene.soa");
        let nodes = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let edges = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        eprintln!(
            "baked {out} → {nodes} nodes, {edges} edges, {} bytes",
            bytes.len()
        );
        assert!(bytes.len() > 12 && nodes > 221, "baked the enriched SoA");
    }

    #[test]
    fn soa_bytes_have_a_parseable_header() {
        let g = sample();
        let bytes = osint_soa_bytes(&g, &[]);
        assert_eq!(&bytes[0..4], &OSINT_SOA_MAGIC);
        let nodes = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let edges = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        // members + at least one basin hub; size matches the fixed records.
        assert!(nodes > g.node_count());
        assert_eq!(bytes.len(), 12 + nodes * 17 + edges * 5);
    }
}
