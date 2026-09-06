//! The canonical **OSINT ClassView** — the `0x07XX` AIRO/AIwar domain.
//!
//! `0x07XX` is the operator-ratified canonical OSINT domain. There is **ONE
//! OSINT node class** — the V3 SoA bake (`data/osint-v3/`) stamps a single
//! classid `0x0701_1000` (`CLASSID_OSINT_V3`) on stakeholder AND person nodes
//! alike, so the stakeholder↔person edges (an institution supplying a person
//! the instrument of power) live inside the class, never blocked by a class
//! boundary. The two ids below (`0x0700` / `0x0701`) are **facet card keys**,
//! not node classes: `0x0700` selects the situational/institutional card
//! (GUID1 AIRO dims), `0x0701` the personal card (GUID2 McClelland dims). A
//! consumer picks the card by GUID slot (GUID1 vs GUID2), not by a node's
//! classid. Node KIND (person vs institution) is an `is_a` rail *property*, not
//! a class — see `data/osint-v3/OSINT_V3_BAKE_README.md` "is_a-kind gap".
//!
//! - [`OSINT_SYSTEM_CLASS`] `0x0700` — the **AI system** card: the 12 AIRO/VAIR
//!   dims packed into GUID1's `6×(8:8)` tier cascade (HEEL `currentStatus:type`,
//!   HIP `militaryUse:civicUse`, TWIG `MLTask:MLType`, LEAF `purpose:capacity`,
//!   family `output:impact`, identity `stakeholder:airo_type`).
//! - [`OSINT_PERSON_CLASS`] `0x0701` — the **person** card: the 5 McClelland /
//!   Rubicon dims from GUID2 (HEEL `stage:need`, HIP `receptor:rubicon`,
//!   TWIG `motive`). This is the Epstein-archetype lens: motive (`nPow`/`nAch`/
//!   `nAff`) × Rubicon crossing × power receptor.
//!
//! This is the holy-grail schema for the OSINT domain: the ordered field card
//! whose *labels* live here (in the ClassView, above the SoA) while the *values*
//! live in the node's SoA tier bytes. The [`FieldMask`] is the Redmine-style
//! ViewFilter — a bitmask selecting which fields render; the askama template is
//! the XSLT that draws the projected rows (`class_view.rs` doctrine header). Bit
//! `i` == field `i` == the `i`-th tier byte, in the exact GUID order above.
//!
//! The `predicate_iri` prefix carries the **reasoning role** (need / offer /
//! intent / causality / person / …) so the two orthogonal axes (Demand
//! `offer⟷need`, Causality `intent⟷impact`) and the Person×Situation split are
//! read from the schema, not hard-coded.
//!
//! Canonical home is OGAR (`ogar-vocab`'s `osint_system` / `osint_person`
//! `Class` fns, lifted by `ogar-class-view::OgarClassView`); this q2-local impl
//! is the owner-authored definition kept byte-aligned with that mirror. It
//! follows the existing cockpit pattern of impl'ing a contract trait locally
//! (cf. `mock_driver.rs` impl'ing `CognitiveShaderDriver`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use askama::Template;
use axum::http::StatusCode;
use axum::response::Html;
use axum::{Json, extract::Query};
use lance_graph_contract::class_view::{ClassId, ClassView, FieldMask, RenderRow, ValueRow};
use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};
use serde::Deserialize;

/// classid `0x0700` — the OSINT **AI system** concept (GUID1 / AIRO dims).
pub const OSINT_SYSTEM_CLASS: ClassId = 0x0700;
/// classid `0x0701` — the OSINT **person** concept (GUID2 / McClelland dims).
pub const OSINT_PERSON_CLASS: ClassId = 0x0701;

/// The AI-system card: 12 AIRO/VAIR fields in GUID1 `6×(8:8)` tier order. The
/// `predicate_iri` prefix is the reasoning **role**: `need` / `offer` (Demand
/// axis) · `intent` / `causality` (Causality axis) · `person` (actor role) ·
/// `identity` / `state` / `relation` (context).
static OSINT_SYSTEM_FIELDS: LazyLock<[FieldRef; 12]> = LazyLock::new(|| {
    [
        // HEEL hi:lo
        FieldRef::new("aiwar:state/currentStatus", "currentStatus"), // 0  STATE
        FieldRef::new("aiwar:identity/type", "type"),                // 1  IDENTITY
        // HIP hi:lo — the dual-use NEED pair
        FieldRef::new("aiwar:need/militaryUse", "militaryUse"), // 2  NEED
        FieldRef::new("aiwar:need/civicUse", "civicUse"),       // 3  NEED
        // TWIG hi:lo
        FieldRef::new("aiwar:need/mlTask", "MLTask"), // 4  NEED (the task)
        FieldRef::new("aiwar:offer/mlType", "MLType"), // 5  OFFER (the technique)
        // LEAF hi:lo
        FieldRef::new("aiwar:intent/purpose", "purpose:vair"), // 6  INTENT (explicit)
        FieldRef::new("aiwar:offer/capacity", "capacity:airo"), // 7  OFFER
        // family hi:lo
        FieldRef::new("aiwar:offer/output", "output:airo"), // 8  OFFER
        FieldRef::new("aiwar:causality/impact", "impact:vair"), // 9  CAUSALITY (implicit)
        // identity hi:lo
        FieldRef::new("aiwar:relation/stakeholder", "stakeholder"), // 10 RELATION (edge)
        FieldRef::new("aiwar:person/airoRole", "airo:type"),        // 11 PERSON (actor role)
    ]
});

/// The person card: 5 McClelland / Rubicon fields in GUID2 tier order. Every
/// field is the `person` role — this is the Person side of Person×Situation
/// (the trait), where the system card carries the Situation (need/offer/impact).
static OSINT_PERSON_FIELDS: LazyLock<[FieldRef; 5]> = LazyLock::new(|| {
    [
        // HEEL hi:lo
        FieldRef::new("aiwar:person/stage", "stage"), // 0  Rubicon stage I..IV
        FieldRef::new("aiwar:person/need", "need"),   // 1  McClelland nPow/nAch/nAff
        // HIP hi:lo
        FieldRef::new("aiwar:person/receptor", "receptor"), // 2  power receptor
        FieldRef::new("aiwar:person/rubicon", "rubicon"),   // 3  Rubicon crossing
        // TWIG hi
        FieldRef::new("aiwar:person/motive", "motive"), // 4  dominant motive
    ]
});

/// The owner-authored ClassView for the OSINT domain. `0x0700` resolves to the
/// AI-system card, `0x0701` to the person card; every other classid is the
/// zero-fallback empty shape.
pub struct OsintClassView;

impl ClassView for OsintClassView {
    fn fields(&self, class: ClassId) -> &[FieldRef] {
        match class {
            OSINT_SYSTEM_CLASS => &OSINT_SYSTEM_FIELDS[..],
            OSINT_PERSON_CLASS => &OSINT_PERSON_FIELDS[..],
            _ => &[],
        }
    }

    fn template(&self, _class: ClassId) -> DisplayTemplate {
        DisplayTemplate::Card
    }

    fn dolce_category_id(&self, _class: ClassId) -> u8 {
        // OSINT node = a DOLCE endurant/object; the cache maps 0 → its default.
        0
    }
}

/// The human-readable concept name for a known OSINT classid (for the card
/// header). Falls back to the hex id for anything else.
fn concept_name(class: ClassId) -> &'static str {
    match class {
        OSINT_SYSTEM_CLASS => "osint_system",
        OSINT_PERSON_CLASS => "osint_person",
        _ => "unknown",
    }
}

// ── V3 name index — resolving a clicked node's label to its V3 GUID ────────
//
// The LIVE gotham scene's node GUIDs (`osint_gotham.rs`) are HHTL routing
// tiers, NOT the content-blind facet payload this card projects (recon
// finding R1, `claude-notes/plans/2026-07-06-classview-unified-render.md`).
// The true 12-byte facet GUIDs live in the unserved `data/osint-v3/` bake.
// This index resolves a clicked node's display `name` (also indexed by its
// short `id`, for resilience) to that bake's `guid1`/`guid2` bytes so the
// popup can show real values without wiring a full V3-backed graph store.

/// One `data/osint-v3/osint_v3_nodes.json` row, parsed into raw key bytes.
#[derive(Debug, Clone, Copy)]
struct V3Entry {
    guid1: [u8; 16],
    guid2: Option<[u8; 16]>,
    is_person: bool,
}

/// The on-disk row shape (`osint_v3_nodes.json`) — see
/// `data/osint-v3/OSINT_V3_BAKE_README.md`.
#[derive(Deserialize)]
struct V3NodeRaw {
    id: String,
    name: String,
    guid1: String,
    guid2: Option<String>,
    is_person: bool,
}

/// Resolve `data/osint-v3/<filename>` relative to the repo root, trying the
/// compile-time-anchored path first (works regardless of runtime cwd — the
/// Railway/Docker launch dir need not be the repo root) and a few cwd-
/// relative fallbacks (works for `cargo test` / `cargo run` from various
/// working directories). Returns `None` if nothing on disk matches; callers
/// degrade to an empty index rather than panicking.
fn resolve_data_path(filename: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/osint-v3")).join(filename),
        PathBuf::from("data/osint-v3").join(filename),
        PathBuf::from("../data/osint-v3").join(filename),
        PathBuf::from("../../data/osint-v3").join(filename),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Parse a raw GUID hex string into its 16 key bytes. Accepts dashes
/// (stripped) and requires exactly 32 hex digits after stripping — anything
/// else (wrong length, non-hex junk) is `None`, never a panic.
fn parse_guid_hex(s: &str) -> Option<[u8; 16]> {
    let cleaned: String = s.chars().filter(|c| *c != '-').collect();
    if cleaned.len() != 32 || !cleaned.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// The V3 name/id → GUID index, loaded once from `osint_v3_nodes.json`.
/// Empty (never panics) if the bake isn't on disk — routes then fall back
/// to the schema-only card, the honest zero-fallback rung.
static OSINT_V3_INDEX: LazyLock<HashMap<String, V3Entry>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    let Some(path) = resolve_data_path("osint_v3_nodes.json") else {
        tracing::warn!(
            "osint_classview: osint_v3_nodes.json not found on disk — value rows disabled, schema-only fallback active"
        );
        return map;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        tracing::warn!("osint_classview: failed to read {}", path.display());
        return map;
    };
    let raw: Vec<V3NodeRaw> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("osint_classview: failed to parse {}: {e}", path.display());
            return map;
        }
    };
    for n in raw {
        let Some(guid1) = parse_guid_hex(&n.guid1) else {
            continue;
        };
        let guid2 = n.guid2.as_deref().and_then(parse_guid_hex);
        let entry = V3Entry {
            guid1,
            guid2,
            is_person: n.is_person,
        };
        // Index by both the display name and the short id — both are
        // legitimate exact-match keys from the same row, so this only
        // improves hit rate; it never introduces ambiguity for either key.
        map.insert(n.name, entry);
        map.entry(n.id).or_insert(entry);
    }
    tracing::info!(
        "osint_classview: V3 name index loaded ({} entries)",
        map.len()
    );
    map
});

// ── Codebook value decode — byte → controlled-vocabulary name ──────────────
//
// `airo_vocab` (guid1 / AI-system facet) is a plain `name -> u8` dict per
// field, so its reverse map (`u8 -> name`) is mechanical and unambiguous.
// `mcclelland_vocab` (guid2 / person facet) is a plain ARRAY per field with
// no explicit value labels; PR #84 left it undecoded because 0- vs 1-indexing
// was a guess, not a fact read off the codebook. The PR #86 rebake settled it
// empirically: person tiers are **1-indexed** — byte value `v` decodes to
// `vocab[v-1]`. Proof: across all 133 persons every field's observed max
// equals its vocab length (stage 5/5, need 3/3, receptor 5/5, rubicon 5/5,
// motive 5/5); 0-indexing would overflow the vocab for 26/133 persons.
// Byte 0 (unset) and `v > len` stay undecoded (`None`) — never a guess. The
// guid2 TWIG lo position is the `_` padding byte (0 for all 133 persons); it
// has no vocab and is never rendered ([`OSINT_PERSON_FIELDS`] declares only
// positions 0..=4).

/// The subset of `osint_v3_codebook.json` this module actually reads.
#[derive(Deserialize)]
struct CodebookRaw {
    guid1_tiers: HashMap<String, String>,
    airo_vocab: HashMap<String, HashMap<String, u8>>,
    guid2_tiers: HashMap<String, String>,
    mcclelland_vocab: HashMap<String, Vec<String>>,
}

fn resolve_codebook_path() -> Option<PathBuf> {
    resolve_data_path("osint_v3_codebook.json")
}

/// `(slot, position, value) -> name` — slot `1` (guid1/system) reverses the
/// `airo_vocab` `name -> u8` dicts; slot `2` (guid2/person) applies the
/// #86-ruled 1-indexed rule over the `mcclelland_vocab` arrays (see the
/// section doc above). A miss (byte 0, out-of-vocab byte, padding position)
/// returns `None` from `.get()` — undecoded, never guessed.
static OSINT_DECODE: LazyLock<HashMap<(u8, u8, u8), String>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    let Some(path) = resolve_codebook_path() else {
        tracing::warn!(
            "osint_classview: osint_v3_codebook.json not found on disk — value decode disabled"
        );
        return map;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        tracing::warn!("osint_classview: failed to read {}", path.display());
        return map;
    };
    let cb: CodebookRaw = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("osint_classview: failed to parse {}: {e}", path.display());
            return map;
        }
    };
    // guid1 (slot 1) field order: HEEL, HIP, TWIG, LEAF, family, identity,
    // each tier's "hi:lo" name split into two consecutive positions — the
    // exact order OSINT_SYSTEM_FIELDS above declares.
    let mut position: u8 = 0;
    for tier in ["HEEL", "HIP", "TWIG", "LEAF", "family", "identity"] {
        let Some(pair) = cb.guid1_tiers.get(tier) else {
            continue;
        };
        for field in pair.split(':') {
            if let Some(vocab) = cb.airo_vocab.get(field) {
                for (name, value) in vocab {
                    map.insert((1u8, position, *value), name.clone());
                }
            }
            position += 1;
        }
    }
    // guid2 (slot 2) field order: HEEL `stage:need`, HIP `receptor:rubicon`,
    // TWIG `motive:_` — positions 0..=4 are the five person fields in the
    // exact order OSINT_PERSON_FIELDS above declares; position 5 is the `_`
    // padding byte (no vocab entry in mcclelland_vocab, so the loop skips it
    // mechanically while still advancing the position counter). 1-indexed
    // per the #86 empirical ruling: byte value v (1..=len) -> vocab[v-1].
    let mut position: u8 = 0;
    for tier in ["HEEL", "HIP", "TWIG"] {
        let Some(pair) = cb.guid2_tiers.get(tier) else {
            continue;
        };
        for field in pair.split(':') {
            if let Some(vocab) = cb.mcclelland_vocab.get(field) {
                for (i, name) in vocab.iter().enumerate() {
                    map.insert((2u8, position, (i + 1) as u8), name.clone());
                }
            }
            position += 1;
        }
    }
    map
});

/// `?class=<u16>&mask=<u64>` — the ViewFilter. `class` omitted = the AI-system
/// card (`0x0700`); `mask` omitted = FULL. `guid`/`name`/`fragment` are the
/// unified-render additions (see module-level docs on [`resolve_card`]);
/// they are ignored by the plain JSON handler, which keeps the pre-existing
/// class/mask-only projection.
#[derive(Deserialize)]
pub struct CardQuery {
    class: Option<u16>,
    mask: Option<u64>,
    /// Raw 16-byte node key (32 hex chars, dashes optional) — the
    /// content-blind V3 facet payload lives at bytes 4..16.
    guid: Option<String>,
    /// A node's display name (or short id) — resolved against
    /// [`OSINT_V3_INDEX`] to its `guid1`/`guid2` bytes.
    name: Option<String>,
    /// `1` → render the bare `<table>` fragment (no doctype/head/style);
    /// omitted/anything else → the existing full page.
    fragment: Option<u8>,
}

impl CardQuery {
    fn resolve(&self) -> (ClassId, FieldMask) {
        let class = self.class.unwrap_or(OSINT_SYSTEM_CLASS);
        let mask = self.mask.map(FieldMask).unwrap_or(FieldMask::FULL);
        (class, mask)
    }
}

/// `GET /api/osint/card?class=<id>&mask=<bits>` — project the card through the
/// FieldMask and return the surviving `(label, predicate)` rows. This is the
/// Redmine ERB ViewFilter, server-side: the mask selects the columns, the
/// ClassView resolves the labels, nothing is computed on the client.
///
/// Unchanged by the unified-render addition — `guid`/`name`/`fragment` are
/// JSON-irrelevant; this handler keeps the original class/mask-only shape.
pub async fn osint_card_handler(Query(q): Query<CardQuery>) -> Json<serde_json::Value> {
    let (class, mask) = q.resolve();
    let cv = OsintClassView;
    let rows: Vec<serde_json::Value> = cv
        .render_rows(class, mask)
        .into_iter()
        .map(|r| serde_json::json!({ "label": r.label, "predicate": r.predicate }))
        .collect();
    Json(serde_json::json!({
        "classid": format!("0x{class:04x}"),
        "concept": concept_name(class),
        "mask": mask.0,
        "field_count": cv.field_count(class),
        "shown": rows.len(),
        "rows": rows,
    }))
}

/// One projected card row — the askama template iterates these. `role` is the
/// reasoning role parsed from the predicate prefix (`aiwar:<role>/<field>`).
/// `value`/`decoded` are `None` for schema-only rows (the pre-existing
/// class/mask projection — no `guid`/`name` given) and `Some` for the
/// value-projected renders (guid/name lookups, the generic facet dump). One
/// struct, optional value fields: the templates branch on ROW SHAPE
/// (`has_values` picks whether the value column exists at all; `row.value`/
/// `row.decoded` pick its per-row content), never on a per-field basis.
struct ValueCardRow {
    role: String,
    label: String,
    predicate: String,
    value: Option<u8>,
    decoded: Option<String>,
}

/// `predicate = "aiwar:<role>/<field>"` — the reasoning role is the prefix.
fn role_from_predicate(predicate: &str) -> String {
    predicate
        .strip_prefix("aiwar:")
        .and_then(|s| s.split('/').next())
        .unwrap_or("")
        .to_string()
}

/// Schema-only rows (no guid/name resolved) — the pre-existing projection,
/// now wrapped in [`ValueCardRow`] with `value`/`decoded` left `None`.
fn render_rows_to_cardrows(rows: Vec<RenderRow<'_>>) -> Vec<ValueCardRow> {
    rows.into_iter()
        .map(|r| ValueCardRow {
            role: role_from_predicate(r.predicate),
            label: r.label.to_string(),
            predicate: r.predicate.to_string(),
            value: None,
            decoded: None,
        })
        .collect()
}

/// Value-projected rows from a resolved guid/name facet — decode each byte
/// against [`OSINT_DECODE`] for the given slot (`1` = guid1/system, `2` =
/// guid2/person, 1-indexed per the #86 ruling); a miss (byte 0, or an
/// out-of-vocab byte) leaves `decoded: None`, never a guess.
fn value_rows_to_cardrows(rows: Vec<ValueRow<'_>>, slot: u8) -> Vec<ValueCardRow> {
    rows.into_iter()
        .map(|r| {
            let decoded = OSINT_DECODE.get(&(slot, r.position, r.value)).cloned();
            ValueCardRow {
                role: role_from_predicate(r.predicate),
                label: r.label.to_string(),
                predicate: r.predicate.to_string(),
                value: Some(r.value),
                decoded,
            }
        })
        .collect()
}

/// The generic facet dump — the last rung of the zero-fallback ladder for a
/// classid `resolve_render_class` couldn't resolve to a bespoke/inherited
/// card (empty field set). Twelve raw `b0..b11` rows, no bespoke labels,
/// never a 404 for a well-formed guid.
fn generic_dump_rows(facet: &[u8; 12]) -> Vec<ValueCardRow> {
    (0u8..12)
        .map(|i| ValueCardRow {
            role: "facet".to_string(),
            label: format!("b{i}"),
            predicate: String::new(),
            value: Some(facet[i as usize]),
            decoded: None,
        })
        .collect()
}

/// The 12-byte content-blind facet payload — bytes 4..16 of a node's 16-byte
/// key (bytes 0..4 are the classid).
fn facet_of(key: &[u8; 16]) -> [u8; 12] {
    key[4..16]
        .try_into()
        .expect("a 16-byte key always slices to a 12-byte facet at [4..16]")
}

/// The person/system slot rule for a V3 index hit: person WITH a guid2 →
/// the person card (slot 2, GUID2 McClelland facet); otherwise → the system
/// card (slot 1, GUID1 AIRO facet). Pure function (no disk/LazyLock access)
/// so the rule is directly unit-testable.
fn slot_for_entry(
    is_person: bool,
    guid1: [u8; 16],
    guid2: Option<[u8; 16]>,
) -> (ClassId, u8, [u8; 12]) {
    if is_person && let Some(g2) = guid2 {
        return (OSINT_PERSON_CLASS, 2, facet_of(&g2));
    }
    // Not a person, OR is_person but no guid2 on the row (a data gap) — fall
    // through to the guid1/system facet rather than rendering nothing.
    (OSINT_SYSTEM_CLASS, 1, facet_of(&guid1))
}

/// A resolved render target — which class/mask to project, and (if a
/// guid/name resolved to a real payload) which slot + facet bytes back it.
struct ResolvedCard {
    class: ClassId,
    mask: FieldMask,
    /// `Some((slot, facet))` when `guid`/`name` resolved to a payload;
    /// `None` for the pre-existing schema-only projection.
    facet: Option<(u8, [u8; 12])>,
}

/// The ONE resolution algorithm behind every unified-render entry point
/// (`osint_card_html_handler`'s guid/name branches, `generic_card_handler`)
/// — different routes, same ClassView fieldmask (per
/// `claude-notes/plans/2026-07-06-classview-unified-render.md`).
///
/// Resolution order:
/// 1. `name` → [`OSINT_V3_INDEX`] lookup → [`slot_for_entry`]. A miss falls
///    through to the schema-only projection (honest zero-fallback: the
///    popup still shows something).
/// 2. `guid` → parse the raw key; card class = the explicit `class` param if
///    given, else `resolve_render_class(hi u16 of the key's classid)`; slot
///    is a property of two-GUID classes (`class == 0x0701` ⇒ slot 2),
///    defaulting to 1 — the explicit `class` param is the disambiguator, not
///    the slot inferred from the classid bits (the V3 bake stamps the SAME
///    classid on every node regardless of system/person kind, so the classid
///    alone cannot pick the slot — see the module-level caveat below).
/// 3. Neither → the original class/mask-only projection (`facet: None`).
///
/// **Caveat (flagged, not silently patched):** the V3 bake's `guid1`/`guid2`
/// share one classid (`0x0701_1000`, "ONE OSINT class, deliberately" —
/// `OSINT_V3_BAKE_README.md`), so `resolve_render_class(hi u16)` on a bare
/// `guid=` (no `name`, no explicit `class`) always resolves to `0x0701`
/// (person card, 5 fields) even when the bytes came from a system node's
/// `guid1`. Callers who want the system card from a raw `guid1` must pass
/// `?class=0x0700` explicitly. This is exactly what the spec's resolution
/// order describes; it is not a bug in this function.
fn resolve_card(
    class_param: Option<u16>,
    mask_param: Option<u64>,
    guid_param: Option<&str>,
    name_param: Option<&str>,
) -> ResolvedCard {
    let cv = OsintClassView;
    let mask = mask_param.map(FieldMask).unwrap_or(FieldMask::FULL);

    if let Some(name) = name_param {
        if let Some(entry) = OSINT_V3_INDEX.get(name) {
            let (class, slot, facet) = slot_for_entry(entry.is_person, entry.guid1, entry.guid2);
            return ResolvedCard {
                class,
                mask,
                facet: Some((slot, facet)),
            };
        }
        // name miss → fall through to the schema-only projection below,
        // using the class/mask defaults (today's behavior).
    } else if let Some(guid) = guid_param
        && let Some(key) = parse_guid_hex(guid)
    {
        let classid = u32::from_le_bytes(key[0..4].try_into().unwrap());
        let hi = (classid >> 16) as u16;
        let class = class_param.unwrap_or_else(|| cv.resolve_render_class(hi));
        let slot: u8 = if class == OSINT_PERSON_CLASS { 2 } else { 1 };
        return ResolvedCard {
            class,
            mask,
            facet: Some((slot, facet_of(&key))),
        };
    }
    // A malformed guid (or a name miss, above) falls through here to the
    // schema-only projection using the class/mask defaults — the generic
    // route 400s on a malformed guid before ever calling this function;
    // the HTML card route degrades honestly instead.

    let class = class_param.unwrap_or(OSINT_SYSTEM_CLASS);
    ResolvedCard {
        class,
        mask,
        facet: None,
    }
}

/// Project a [`ResolvedCard`] into its render rows: schema-only projection,
/// value-projected facet rows, or (an unknown classid on the facet path) the
/// generic facet dump — the zero-fallback ladder's last rung.
fn resolved_rows(cv: &OsintClassView, resolved: &ResolvedCard) -> Vec<ValueCardRow> {
    match resolved.facet {
        Some((_slot, facet)) if cv.fields(resolved.class).is_empty() => generic_dump_rows(&facet),
        Some((slot, facet)) => {
            value_rows_to_cardrows(cv.facet_rows(resolved.class, resolved.mask, &facet), slot)
        }
        None => render_rows_to_cardrows(cv.render_rows(resolved.class, resolved.mask)),
    }
}

/// The card view — a dumb askama loop over the mask-filtered rows (the "XSLT"
/// over the FieldMask projection). No per-field conditionals: the ViewFilter
/// already carved the row set in Rust.
#[derive(Template)]
#[template(path = "osint_card.html")]
struct OsintCardTemplate {
    classid_hex: String,
    concept: &'static str,
    mask_hex: String,
    shown: usize,
    total: usize,
    has_values: bool,
    rows: Vec<ValueCardRow>,
}

/// The bare-table fragment — same row shape as [`OsintCardTemplate`], no
/// doctype/head/style/h1. Presentation (theme, spacing, color) is owned by
/// the consuming popup container (the React click-to-inspect panel).
#[derive(Template)]
#[template(path = "osint_card_fragment.html")]
struct OsintCardFragmentTemplate {
    has_values: bool,
    rows: Vec<ValueCardRow>,
}

/// Shared response builder: resolve → rows → pick template (full page vs
/// fragment) → render. The ONE code path behind both `osint_card_html_handler`
/// and `generic_card_handler` — different routes, same ClassView fieldmask.
fn render_card_response(
    cv: &OsintClassView,
    resolved: ResolvedCard,
    fragment: bool,
) -> Html<String> {
    let has_values = resolved.facet.is_some();
    let is_generic_dump = has_values && cv.fields(resolved.class).is_empty();
    let rows = resolved_rows(cv, &resolved);
    let total = if is_generic_dump {
        12
    } else {
        cv.field_count(resolved.class)
    };

    if fragment {
        let tpl = OsintCardFragmentTemplate { has_values, rows };
        return match tpl.render() {
            Ok(body) => Html(body),
            Err(e) => Html(format!("<pre>osint card fragment render error: {e}</pre>")),
        };
    }

    let tpl = OsintCardTemplate {
        classid_hex: format!("0x{:04x}", resolved.class),
        concept: concept_name(resolved.class),
        mask_hex: format!("0x{:03x}", resolved.mask.0),
        shown: rows.len(),
        total,
        has_values,
        rows,
    };
    // askama render is infallible for this static template; fall back to a
    // terse error body rather than panicking in a request handler.
    match tpl.render() {
        Ok(body) => Html(body),
        Err(e) => Html(format!("<pre>osint card render error: {e}</pre>")),
    }
}

/// `GET /api/osint/card.html?class=<id>&mask=<bits>` — the same ViewFilter,
/// rendered server-side via the compile-time-checked askama template. The
/// *projection* (mask → rows) is identical to the JSON handler; askama is only
/// the XSLT. Each row shows its reasoning role (parsed from the predicate
/// prefix), the field label, and the predicate key.
///
/// Extended (additive) with `guid`/`name`/`fragment` — see [`resolve_card`].
/// With none of the three given, this reproduces the pre-existing output
/// exactly (regression-tested).
pub async fn osint_card_html_handler(Query(q): Query<CardQuery>) -> Html<String> {
    let cv = OsintClassView;
    let resolved = resolve_card(q.class, q.mask, q.guid.as_deref(), q.name.as_deref());
    render_card_response(&cv, resolved, q.fragment == Some(1))
}

/// `GET /api/card.html?guid=<32-hex>&class=<id>&mask=<bits>&fragment=<0|1>`
/// — the generic, OSINT-agnostic sibling of `/api/osint/card.html`. Requires
/// `guid`; a `class`/`mask`/`fragment` mirror the OSINT route's knobs.
/// Different route, same ClassView fieldmask core ([`resolve_card`] +
/// [`render_card_response`]) — that shared core is the point of this route.
pub async fn generic_card_handler(
    Query(q): Query<CardQuery>,
) -> Result<Html<String>, (StatusCode, String)> {
    let Some(guid) = q.guid.as_deref() else {
        return Err((
            StatusCode::BAD_REQUEST,
            "GET /api/card.html requires ?guid=<32-hex-char node key> (dashes optional)"
                .to_string(),
        ));
    };
    if parse_guid_hex(guid).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "malformed guid {guid:?}: expected 32 hex characters (dashes optional), got a value that doesn't parse"
            ),
        ));
    }
    let cv = OsintClassView;
    let resolved = resolve_card(q.class, q.mask, Some(guid), None);
    Ok(render_card_response(&cv, resolved, q.fragment == Some(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_card_has_twelve_fields_in_tier_order() {
        let cv = OsintClassView;
        assert_eq!(cv.field_count(OSINT_SYSTEM_CLASS), 12);
        // GUID1 tier order: HEEL currentStatus:type, HIP mil:civ, …
        assert_eq!(cv.field_label(OSINT_SYSTEM_CLASS, 0), Some("currentStatus"));
        assert_eq!(cv.field_label(OSINT_SYSTEM_CLASS, 2), Some("militaryUse"));
        assert_eq!(cv.field_label(OSINT_SYSTEM_CLASS, 9), Some("impact:vair"));
        assert_eq!(cv.field_label(OSINT_SYSTEM_CLASS, 11), Some("airo:type"));
        // unknown class = zero-fallback empty shape.
        assert_eq!(cv.field_count(0x0000), 0);
    }

    #[test]
    fn person_card_has_five_mcclelland_fields() {
        let cv = OsintClassView;
        assert_eq!(cv.field_count(OSINT_PERSON_CLASS), 5);
        assert_eq!(cv.field_label(OSINT_PERSON_CLASS, 0), Some("stage"));
        assert_eq!(cv.field_label(OSINT_PERSON_CLASS, 1), Some("need"));
        assert_eq!(cv.field_label(OSINT_PERSON_CLASS, 4), Some("motive"));
    }

    #[test]
    fn field_mask_is_the_view_filter() {
        let cv = OsintClassView;
        // full mask → all 12 system rows.
        assert_eq!(
            cv.render_rows(OSINT_SYSTEM_CLASS, FieldMask::FULL).len(),
            12
        );
        // mask with only the Causality axis ends (intent bit 6 + impact bit 9).
        let causal = FieldMask::EMPTY.with(6).with(9);
        let rows = cv.render_rows(OSINT_SYSTEM_CLASS, causal);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "purpose:vair");
        assert_eq!(rows[1].label, "impact:vair");
    }

    #[test]
    fn html_card_renders_through_askama() {
        // Smoke the askama path end-to-end for the person card: header +
        // one row per selected field, role parsed from the predicate prefix.
        let cv = OsintClassView;
        let rows: Vec<ValueCardRow> = cv
            .render_rows(OSINT_PERSON_CLASS, FieldMask::FULL)
            .into_iter()
            .map(|r| ValueCardRow {
                role: r
                    .predicate
                    .strip_prefix("aiwar:")
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("")
                    .to_string(),
                label: r.label.to_string(),
                predicate: r.predicate.to_string(),
                value: None,
                decoded: None,
            })
            .collect();
        let tpl = OsintCardTemplate {
            classid_hex: "0x0701".to_string(),
            concept: "osint_person",
            mask_hex: "0x01f".to_string(),
            shown: rows.len(),
            total: 5,
            has_values: false,
            rows,
        };
        let body = tpl.render().expect("askama render");
        assert!(body.contains("osint_person"));
        assert!(body.contains("motive"));
        assert!(body.contains("aiwar:person/motive"));
        // dumb-loop template: every selected field is a row.
        assert_eq!(body.matches("<tr>").count(), 5 + 1); // 5 rows + header row
    }

    // ── unified ClassView render: facet value binding + is_a/slot rules ────

    /// `facet_of` + `cv.facet_rows` binds field position `i` to raw byte `i`
    /// of the key's `[4..16]` slice — the exact structural contract the
    /// spec's `facet: &[u8;12] = key[4..16].try_into()` construction promises.
    /// No semantic (vocab) validity is asserted here — only the mechanical
    /// position↔byte binding.
    #[test]
    fn facet_binds_position_to_byte_from_key_slice() {
        let cv = OsintClassView;
        let key: [u8; 16] = [
            0x00, 0x10, 0x01, 0x07, // classid 0x0701_1000 LE
            10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, // facet bytes
        ];
        let facet = facet_of(&key);
        assert_eq!(facet, [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
        let rows = value_rows_to_cardrows(
            cv.facet_rows(OSINT_SYSTEM_CLASS, FieldMask::FULL, &facet),
            1,
        );
        assert_eq!(rows.len(), 12, "all 12 system fields are facet-backed");
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row.value,
                Some(facet[i]),
                "position {i} binds to facet byte {i}"
            );
            assert_eq!(facet[i], key[4 + i], "facet byte {i} is key byte {}", 4 + i);
        }
    }

    /// The person/system slot rule: a name-index hit that is `is_person` AND
    /// carries a `guid2` picks the person card (classid `0x0701`, slot 2,
    /// 5 rows) — pure, no disk access (`slot_for_entry` takes explicit
    /// fields rather than reading `OSINT_V3_INDEX`, so this is directly
    /// testable without a fake index entry).
    #[test]
    fn name_route_person_with_guid2_picks_person_card() {
        let cv = OsintClassView;
        let guid1 = [0u8; 16];
        let guid2 = [7u8; 16];
        let (class, slot, facet) = slot_for_entry(true, guid1, Some(guid2));
        assert_eq!(class, OSINT_PERSON_CLASS);
        assert_eq!(slot, 2);
        assert_eq!(facet, facet_of(&guid2));
        let rows = value_rows_to_cardrows(cv.facet_rows(class, FieldMask::FULL, &facet), slot);
        assert_eq!(rows.len(), 5, "all 5 person fields are facet-backed");

        // A non-person entry (or a person row missing guid2 — a data gap)
        // falls through to the system card, slot 1.
        let (class2, slot2, facet2) = slot_for_entry(false, guid1, None);
        assert_eq!(class2, OSINT_SYSTEM_CLASS);
        assert_eq!(slot2, 1);
        assert_eq!(facet2, facet_of(&guid1));
        let (class3, slot3, _) = slot_for_entry(true, guid1, None);
        assert_eq!(
            class3, OSINT_SYSTEM_CLASS,
            "is_person with no guid2 falls through to the system facet, not a blank card"
        );
        assert_eq!(slot3, 1);
    }

    /// A classid whose hi u16 is neither `0x0700` nor `0x0701` has an empty
    /// field set on this `ClassView` and no `is_a_parent` (default `None`),
    /// so `resolve_render_class` returns it unchanged — the caller's signal
    /// to render the generic facet dump (12 raw `b0..b11` rows), never a 404.
    #[test]
    fn unknown_classid_renders_generic_facet_dump() {
        let cv = OsintClassView;
        let unknown_class: ClassId = 0x1234;
        assert!(cv.fields(unknown_class).is_empty());
        assert_eq!(cv.resolve_render_class(unknown_class), unknown_class);

        let facet = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let rows = generic_dump_rows(&facet);
        assert_eq!(rows.len(), 12);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.role, "facet");
            assert_eq!(row.label, format!("b{i}"));
            assert_eq!(row.value, Some(facet[i]));
            assert_eq!(row.decoded, None);
        }

        // End-to-end through `resolve_card` + `resolved_rows`: a guid whose
        // classid hi u16 = 0x1234 (unknown) → generic dump, not a panic/404.
        let key: [u8; 16] = [
            0x00, 0x00, 0x34, 0x12, // classid 0x1234_0000 LE → hi u16 = 0x1234
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        ];
        let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
        let resolved = resolve_card(None, None, Some(&hex), None);
        assert_eq!(resolved.class, 0x1234);
        let rows = resolved_rows(&cv, &resolved);
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0].label, "b0");
        assert_eq!(rows[0].value, Some(1));
    }

    /// Regression: `/api/osint/card.html` with no new params (`guid`/`name`/
    /// `fragment` all absent) must render EXACTLY what it rendered before
    /// this PR — same assertions the pre-existing `html_card_renders_through_
    /// askama` test made (substring + `<tr>` count), now exercised through
    /// the actual route handler rather than a hand-built template struct.
    #[tokio::test]
    async fn html_route_with_no_new_params_matches_pre_existing_output() {
        let q = CardQuery {
            class: None,
            mask: None,
            guid: None,
            name: None,
            fragment: None,
        };
        let Html(body) = osint_card_html_handler(Query(q)).await;
        // Default card is the AI-system card (0x0700), FULL mask → 12 rows.
        assert!(body.contains("osint_system"));
        assert!(body.contains("currentStatus"));
        assert!(body.contains("aiwar:state/currentStatus"));
        assert_eq!(body.matches("<tr>").count(), 12 + 1); // 12 rows + header
        // No value column when has_values is false (schema-only path).
        assert!(!body.contains("class=\"v\""));
    }

    /// `fragment=1` renders the bare `<table>` fragment — no doctype, no
    /// `<head>`, no `<style>`, no `<h1>`; presentation is the caller's job.
    #[tokio::test]
    async fn fragment_route_renders_bare_table_with_no_doctype() {
        let q = CardQuery {
            class: None,
            mask: None,
            guid: None,
            name: None,
            fragment: Some(1),
        };
        let Html(body) = osint_card_html_handler(Query(q)).await;
        assert!(body.contains("<table"));
        assert!(!body.to_lowercase().contains("<!doctype"));
        assert!(!body.to_lowercase().contains("<style"));
        assert!(!body.contains("<h1"));
    }

    /// `parse_guid_hex`: plain 32-hex ok, dashed form parses to the same
    /// bytes, and non-hex/wrong-length input is `None`, never a panic.
    #[test]
    fn parse_guid_hex_handles_dashes_and_rejects_junk() {
        // classid bytes "00100107" + 12 zero bytes = 32 hex chars total.
        let plain = "00100107".to_string() + &"00".repeat(12);
        assert_eq!(plain.len(), 32);
        let parsed = parse_guid_hex(&plain).expect("32 plain hex chars must parse");
        assert_eq!(parsed[0..4], [0x00, 0x10, 0x01, 0x07]);

        let dashed = format!(
            "{}-{}-{}-{}-{}",
            &plain[0..8],
            &plain[8..12],
            &plain[12..16],
            &plain[16..20],
            &plain[20..32]
        );
        assert_eq!(
            parse_guid_hex(&dashed),
            Some(parsed),
            "dashes are stripped before parsing"
        );

        assert_eq!(parse_guid_hex("not-hex-at-all-zz"), None);
        assert_eq!(parse_guid_hex(&plain[..30]), None, "too short");
        assert_eq!(parse_guid_hex(&(plain.clone() + "ab")), None, "too long");
    }

    // ── guid2 / McClelland slot-2 decode (1-indexed per the #86 ruling) ────

    /// A known guid2 facet decodes through the full value-row path: byte
    /// value `v` → `mcclelland_vocab[field][v-1]` at positions 0..=4, and
    /// the `_` padding byte (position 5) is never rendered as a row — the
    /// person card declares exactly 5 fields.
    #[test]
    fn guid2_person_facet_decodes_one_indexed() {
        let cv = OsintClassView;
        // stage=4→IV, need=1→nPow, receptor=2→STATUS, rubicon=3→ACTIONAL,
        // motive=2→STATUS, padding=0 (+ 6 unused tail bytes).
        let facet: [u8; 12] = [4, 1, 2, 3, 2, 0, 0, 0, 0, 0, 0, 0];
        let rows = value_rows_to_cardrows(
            cv.facet_rows(OSINT_PERSON_CLASS, FieldMask::FULL, &facet),
            2,
        );
        assert_eq!(rows.len(), 5, "5 person fields; padding position absent");
        let decoded: Vec<Option<&str>> = rows.iter().map(|r| r.decoded.as_deref()).collect();
        assert_eq!(
            decoded,
            vec![
                Some("IV"),
                Some("nPow"),
                Some("STATUS"),
                Some("ACTIONAL"),
                Some("STATUS"),
            ]
        );
        assert!(
            rows.iter().all(|r| r.label != "_"),
            "the `_` padding byte is never a rendered row"
        );
    }

    /// Boundary rule for the 1-indexed slot-2 decode: byte 0 (unset) is
    /// undecoded, `v == len` reaches the LAST vocab entry, and `v == len+1`
    /// overflows to undecoded — exactly the #86 empirical ruling (observed
    /// max == vocab len on every field; 0-indexing would overflow 26/133
    /// persons).
    #[test]
    fn guid2_decode_boundaries_zero_len_and_overflow() {
        // stage (position 0): 5 entries, vocab[4] = "IV_performed".
        assert_eq!(OSINT_DECODE.get(&(2, 0, 0)), None, "v=0 stays undecoded");
        assert_eq!(
            OSINT_DECODE.get(&(2, 0, 5)).map(String::as_str),
            Some("IV_performed"),
            "v == len decodes to the last vocab entry"
        );
        assert_eq!(OSINT_DECODE.get(&(2, 0, 6)), None, "v = len+1 overflows");
        // need (position 1): 3 entries, vocab[2] = "nAff".
        assert_eq!(
            OSINT_DECODE.get(&(2, 1, 3)).map(String::as_str),
            Some("nAff")
        );
        assert_eq!(OSINT_DECODE.get(&(2, 1, 4)), None);
        // padding (position 5, the `_` byte): no vocab, never decoded.
        for v in 0..=u8::MAX {
            assert_eq!(OSINT_DECODE.get(&(2, 5, v)), None, "padding never decodes");
        }
    }
}
