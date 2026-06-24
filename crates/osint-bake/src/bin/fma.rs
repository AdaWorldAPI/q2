//! FMA anatomy slice — the "real test" of the dual-membership lattice.
//!
//! **Hydrated from an FMA `.ttl` fixture** (`data/fma-heart.fixture.ttl`) via
//! [`hydrate_fma`] — no longer hand-built. Stands up a Foundational-Model-of-
//! Anatomy-shaped slice of the **heart** (organ → chambers → wall layers) and
//! proves that one node resolves to BOTH addresses at once:
//!
//! * **part-of position** (basin-local): HEEL=[Organ:Heart], HIP=[Chamber:id],
//!   TWIG=[Wall:id] — where the node *is* in the body, read straight off the key
//!   (the partonomy walk fills the cascade; deeper tiers stay 0 until the real
//!   75K FMA hydrates tissues/cells through the same walk).
//! * **leaf-limited global type** (the CEILING pole, HEEL=HIP=TWIG=0xFFFF,
//!   LEAF=type): "cardiac muscle tissue", "endothelium" — cross-cutting types
//!   that appear in *every* chamber. The deepest sentinel run (through TWIG)
//!   makes the LEAF the sole discriminator: "limited to the leaf".
//!
//! The same `Cardiac muscle tissue` ceiling node is the `is-a` target for the
//! myocardium tissue in all four chambers — visibly cross-cutting. Emits
//! `cockpit/public/fma.soa` (OSO1, the cockpit's `/fma` view reads it) and
//! prints the dual-membership proof.
//!
//! ## Relation to OGAR PR #116 (the FMA canon)
//!
//! OGAR's `ogar-fma-skeleton` canonized the FMA address as a `[container:member]`
//! tier model with two `HhtlMode` readings of one 16-byte key
//! (`docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`):
//! * **`Located`** — HEEL/HIP carry a *spatial* Morton position (coronal x:y /
//!   depth z). OGAR's bones use this: they ARE the position anchor.
//! * **`Cascade`** — HEEL/HIP carry an *ontology* rung (parent:child class),
//!   pure part-of containment, no spatial address. Soft tissue uses this.
//!
//! This heart slice is the **`Cascade` reading**, addressed as a stack of 8:8
//! `[container:identity]` HHTL tiers: the container byte is the KIND **mixin
//! node** (Organ/Chamber/Wall/Tissue/Cell — the family everything of that level
//! attaches *on*), the identity byte is the instance attached to it — 256×256 =
//! 64k deterministic per tier. HEEL=[Organ:Heart], HIP=[Chamber:id],
//! TWIG=[Wall:id], LEAF=[Tissue:id], family=[Cell:id], so the **partonomy IS the
//! key** — no Morton-in-identity, no edge lookup needed to place a node. OGAR's
//! skeleton is the `Located` sibling (the same 8:8 tiers carry spatial Morton
//! cells instead of ontology rungs). Edges are still drawn (a viz convenience);
//! OGAR's canon supersedes the 12+4 EdgeBlock with exactly this family-node
//! grouping — the container byte names the mixin/family node.
//!
//! Run from the workspace root:  `cargo run -p osint-bake --bin fma`

use lance_graph_contract::canonical_node::NodeGuid;
use osint_bake::fma_ttl;
use std::path::{Path, PathBuf};

/// The CEILING global-category pole (HEEL=HIP=0xFFFF; sentinel through TWIG = leaf-grain).
const CEILING: u16 = 0xFFFF;
/// FMA classid — `anatomical_structure` (`0x0A01`) in OGAR's
/// `ConceptDomain::Anatomy` (high byte `0x0A`; resolves via
/// `ogar_vocab::canonical_concept_domain`). The heart slice is soft-tissue
/// anatomy, so it takes the universal-root concept; OGAR reserves
/// `0x0A02..0x0A04` for skeleton/bone/joint. Aligned to OGAR PR #116
/// (`docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`) — was the ad-hoc `0x00F0_0A00`.
const CLASSID_FMA: u32 = 0x0000_0A01;

// class bytes → cockpit colour/label (see FmaGraph.tsx).
const C_ORGAN: u8 = 0;
const C_CHAMBER: u8 = 1;
const C_WALL: u8 = 2;
const C_TISSUE: u8 = 3;
const C_CELL: u8 = 4;
const C_TYPE: u8 = 5; // the leaf-limited global type categories (ceiling pole)

// rel bytes (rel ≥ 2 renders in the cockpit view; 0/1 reserved like OSINT).
const REL_PART_OF: u8 = 2; // structure → its container (basin-local hierarchy)
const REL_IS_A: u8 = 3; // structure → its cross-cutting global type (ceiling)

// ── HHTL 8:8 [container:identity] tiers ──
// Each tier is one u16 = `(container << 8) | identity` = 256×256 = 64k. The
// container (high byte) is the KIND **mixin node** — the family everything of
// that level attaches on; the identity (low byte) is the instance attached to
// it. Mirrors OGAR's `[bodypart:bone]` LeafTile / `[container:member]` Tier.
const MX_ORGAN: u8 = 0x01; // the Organ mixin node (the Heart attaches here)
const MX_CHAMBER: u8 = 0x02; // the Chamber mixin node
const MX_WALL: u8 = 0x03; // the Wall mixin node
const MX_TISSUE: u8 = 0x04; // the Tissue mixin node
const MX_CELL: u8 = 0x05; // the Cell mixin node
const ID_HEART: u8 = 0x01; // the sole organ instance in this slice

/// One 8:8 HHTL tier: `[container : identity]` = `[mixin-node : instance-on-it]`.
const fn tier(container: u8, identity: u8) -> u16 {
    ((container as u16) << 8) | identity as u16
}

struct Node {
    label: String,
    class: u8,
    key: NodeGuid,
}

struct Builder {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize, u8)>,
}

impl Builder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// A part-of node addressed by its `[kind-mixin : instance]` HHTL cascade.
    /// Each tier is one 8:8 `[container:identity]` pair; the non-zero tiers ARE
    /// the partonomy path — HEEL=which organ, HIP=which chamber, TWIG=which wall,
    /// LEAF=which tissue, family=which cell. The address says where the node sits;
    /// no Morton path, no edge lookup needed. `(chamber, wall, tissue, cell)` are
    /// the instance ids at each level (0 = "this node isn't that deep").
    fn part_of_node(
        &mut self,
        label: &str,
        class: u8,
        chamber: u8,
        wall: u8,
        tissue: u8,
        cell: u8,
    ) -> usize {
        let i = self.nodes.len();
        let key = NodeGuid::new_v2(
            CLASSID_FMA,
            tier(MX_ORGAN, ID_HEART), // HEEL  [Organ:Heart]
            if chamber > 0 {
                tier(MX_CHAMBER, chamber)
            } else {
                0
            }, // HIP   [Chamber:id]
            if wall > 0 { tier(MX_WALL, wall) } else { 0 }, // TWIG  [Wall:id]
            if tissue > 0 {
                tier(MX_TISSUE, tissue)
            } else {
                0
            }, // LEAF  [Tissue:id]
            if cell > 0 { tier(MX_CELL, cell) } else { 0 }, // family[Cell:id]
            i as u16,                 // identity — stable node id
        );
        self.nodes.push(Node {
            label: label.to_string(),
            class,
            key,
        });
        i
    }

    /// A leaf-limited global TYPE category: HEEL=HIP=TWIG=0xFFFF, LEAF=type idx.
    fn type_node(&mut self, label: &str, type_idx: u16) -> usize {
        let i = self.nodes.len();
        let key = NodeGuid::new_v2(
            CLASSID_FMA,
            CEILING,  // HEEL sentinel
            CEILING,  // HIP  sentinel
            CEILING,  // TWIG sentinel → leaf-grain ("limited to the leaf")
            type_idx, // LEAF — the sole discriminator
            0,        // family — global, no basin
            i as u16,
        );
        self.nodes.push(Node {
            label: label.to_string(),
            class: C_TYPE,
            key,
        });
        i
    }

    fn edge(&mut self, s: usize, t: usize, rel: u8) {
        self.edges.push((s, t, rel));
    }
}

/// Embedded FMA heart fixture — real class names + the canonical FMA predicate
/// set. The production path hydrates the 266 MB `fma.owl` through
/// `lance-graph-rdf` at the spine; this light bake hydrates the fixture so
/// `/fma` renders without the lance/datafusion closure. See
/// `data/fma-heart.fixture.ttl`.
const FMA_TTL: &str = include_str!("../../data/fma-heart.fixture.ttl");

/// Hydrate an FMA `.ttl` fragment into the bake's [`Builder`] — the light-bake
/// twin of `lance_graph_ontology::hydrate_fma`. Walk the `bfo:part_of` partonomy
/// into the canonical HHTL cascade (each node's sibling-rank at each depth → the
/// 8:8 `[mixin:instance]` tier), and project each `rdfs:subClassOf` onto the
/// cross-cutting global-type ceiling. Depth (organ→chamber→wall→…) is the
/// distance from the partonomy root; nothing is hardcoded to "heart", so the
/// real 75K FMA hydrates through the exact same walk.
fn hydrate_fma(ttl: &str) -> Builder {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let frag = fma_ttl::parse(ttl);

    // child → parent (part_of); parent → IRI-sorted children (stable sibling
    // ranks ⇒ a reproducible, byte-deterministic asset).
    let parent_of: BTreeMap<&str, &str> = frag
        .part_of
        .iter()
        .map(|(c, p)| (c.as_str(), p.as_str()))
        .collect();
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut in_tree: BTreeSet<&str> = BTreeSet::new();
    for (c, p) in &frag.part_of {
        children.entry(p.as_str()).or_default().push(c.as_str());
        in_tree.insert(c.as_str());
        in_tree.insert(p.as_str());
    }
    for v in children.values_mut() {
        v.sort_unstable();
    }
    // 1-based sibling rank under the parent — the tier identity byte (0 = root).
    let rank_of = |node: &str| -> u8 {
        parent_of
            .get(node)
            .and_then(|p| children[p].iter().position(|&c| c == node))
            .map_or(0, |k| (k as u8) + 1)
    };
    // (depth, [chamber, wall, tissue, cell]) — sibling ranks along the ancestor
    // chain, root-first; depth 0 = the partonomy root (the organ).
    let path_of = |node: &str| -> (u8, [u8; 4]) {
        let mut chain: Vec<&str> = Vec::new();
        let mut cur = node;
        while let Some(&p) = parent_of.get(cur) {
            chain.push(cur);
            cur = p;
        }
        chain.reverse();
        let mut ids = [0u8; 4];
        for (k, &n) in chain.iter().enumerate().take(4) {
            ids[k] = rank_of(n);
        }
        (chain.len() as u8, ids)
    };
    let class_for = |depth: u8| match depth {
        0 => C_ORGAN,
        1 => C_CHAMBER,
        2 => C_WALL,
        3 => C_TISSUE,
        _ => C_CELL,
    };

    let mut b = Builder::new();
    let mut idx: BTreeMap<&str, usize> = BTreeMap::new();

    // BFS from the root(s) so every parent is built before its children (the
    // edge list references node indices).
    let mut queue: VecDeque<&str> = in_tree
        .iter()
        .copied()
        .filter(|n| !parent_of.contains_key(n))
        .collect();
    while let Some(n) = queue.pop_front() {
        if idx.contains_key(n) {
            continue;
        }
        let (depth, ids) = path_of(n);
        let node = b.part_of_node(
            &fma_ttl::label_of(n),
            class_for(depth),
            ids[0],
            ids[1],
            ids[2],
            ids[3],
        );
        idx.insert(n, node);
        if let Some(cs) = children.get(n) {
            queue.extend(cs.iter().copied());
        }
    }

    // cross-cutting tissue-type ceiling nodes (subClassOf targets not in the tree).
    let mut type_idx: BTreeMap<&str, usize> = BTreeMap::new();
    for (_c, ty) in &frag.is_a {
        if idx.contains_key(ty.as_str()) || type_idx.contains_key(ty.as_str()) {
            continue;
        }
        let t = b.type_node(&fma_ttl::label_of(ty), type_idx.len() as u16);
        type_idx.insert(ty.as_str(), t);
    }

    // part_of edges (containment) + is-a edges (the dual membership).
    for (c, p) in &frag.part_of {
        if let (Some(&ci), Some(&pi)) = (idx.get(c.as_str()), idx.get(p.as_str())) {
            b.edge(ci, pi, REL_PART_OF);
        }
    }
    for (c, ty) in &frag.is_a {
        if let (Some(&ci), Some(&ti)) = (idx.get(c.as_str()), type_idx.get(ty.as_str())) {
            b.edge(ci, ti, REL_IS_A);
        }
    }
    b
}

/// Emit the OSO1 wire buffer the cockpit decodes (magic | counts | nodes
/// [guid|class] | edges [s|t|rel] | label tail). No tenant tail (FMA has no
/// dual-use facets); the decoder treats it as absent.
fn emit_oso1(b: &Builder) -> Vec<u8> {
    let n = b.nodes.len();
    let mut out = Vec::with_capacity(12 + n * 17 + b.edges.len() * 5);
    out.extend_from_slice(b"OSO1");
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(b.edges.len() as u32).to_le_bytes());
    for node in &b.nodes {
        out.extend_from_slice(node.key.as_bytes());
        out.push(node.class);
    }
    for &(s, t, rel) in &b.edges {
        out.extend_from_slice(&(s as u16).to_le_bytes());
        out.extend_from_slice(&(t as u16).to_le_bytes());
        out.push(rel);
    }
    for node in &b.nodes {
        let bytes = node.label.as_bytes();
        let l = bytes.len().min(255);
        out.push(l as u8);
        out.extend_from_slice(&bytes[..l]);
    }
    out
}

fn main() {
    let b = hydrate_fma(FMA_TTL);
    let bytes = emit_oso1(&b);

    // dual-membership proof: a hydrated wall layer carries BOTH addresses.
    let tissue = b
        .nodes
        .iter()
        .position(|x| x.label == "Myocardium of left ventricle")
        .expect("LV myocardium hydrated from the fixture");
    let key = &b.nodes[tissue].key;
    println!("── FMA dual-membership proof ──");
    println!("node: {}", b.nodes[tissue].label);
    // the partonomy IS the key: each HHTL tier is an 8:8 [mixin:identity] pair.
    let show = |t: u16| format!("[{:02x}:{:02x}]", t >> 8, t & 0xFF);
    println!(
        "  part-of address — HHTL 8:8 [mixin:identity]: HEEL {} HIP {} TWIG {} LEAF {} family {}",
        show(key.heel()),
        show(key.hip()),
        show(key.twig()),
        show(key.leaf()),
        show(key.family_v2()),
    );
    // its is-a edge → the leaf-limited global type (ceiling pole)
    let gtype = b
        .edges
        .iter()
        .find(|&&(s, _, rel)| s == tissue && rel == REL_IS_A)
        .map(|&(_, t, _)| t)
        .expect("tissue is-a a global type");
    let gk = &b.nodes[gtype].key;
    println!(
        "  is-a global type (ceiling): {}  HEEL={:#06x} HIP={:#06x} TWIG={:#06x} LEAF={}",
        b.nodes[gtype].label,
        gk.heel(),
        gk.hip(),
        gk.twig(),
        gk.leaf()
    );
    // cross-cutting: how many chambers' tissues share this one global type?
    let members = b
        .edges
        .iter()
        .filter(|&&(_, t, rel)| t == gtype && rel == REL_IS_A)
        .count();
    println!(
        "  '{}' is the is-a target of {members} tissues across the chambers (cross-cutting)",
        b.nodes[gtype].label
    );

    let n_types = b.nodes.iter().filter(|x| x.class == C_TYPE).count();
    println!(
        "\nslice: {} nodes ({} part-of + {} ceiling types), {} edges",
        b.nodes.len(),
        b.nodes.len() - n_types,
        n_types,
        b.edges.len()
    );

    // write to where the cockpit serves it (Vite copies public/* into dist/).
    let candidates = ["cockpit/public/fma.soa", "../../cockpit/public/fma.soa"];
    let out: PathBuf = candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.parent().is_some_and(Path::exists))
        .unwrap_or_else(|| PathBuf::from(candidates[0]));
    std::fs::write(&out, &bytes).expect("write fma.soa");
    println!("baked {} ({} bytes)", out.display(), bytes.len());
}
