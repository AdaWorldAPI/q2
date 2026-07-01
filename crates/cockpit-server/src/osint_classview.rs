//! The canonical **OSINT ClassView** — the `0x07XX` AIRO/AIwar domain.
//!
//! `0x07XX` is the operator-ratified canonical OSINT domain; the low byte (the
//! slot) is the owner's to assign. Two concepts are minted, mirroring the V3 SoA
//! bake (`data/osint-v3/`, `(APP_PREFIX 0x1000)<<16 | concept`):
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

use std::sync::LazyLock;

use askama::Template;
use axum::response::Html;
use axum::{extract::Query, Json};
use lance_graph_contract::class_view::{ClassId, ClassView, FieldMask};
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
        FieldRef::new("aiwar:need/mlTask", "MLTask"),  // 4  NEED (the task)
        FieldRef::new("aiwar:offer/mlType", "MLType"), // 5  OFFER (the technique)
        // LEAF hi:lo
        FieldRef::new("aiwar:intent/purpose", "purpose:vair"),  // 6  INTENT (explicit)
        FieldRef::new("aiwar:offer/capacity", "capacity:airo"), // 7  OFFER
        // family hi:lo
        FieldRef::new("aiwar:offer/output", "output:airo"),     // 8  OFFER
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

/// `?class=<u16>&mask=<u64>` — the ViewFilter. `class` omitted = the AI-system
/// card (`0x0700`); `mask` omitted = FULL.
#[derive(Deserialize)]
pub struct CardQuery {
    class: Option<u16>,
    mask: Option<u64>,
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
struct OsintCardRow {
    role: String,
    label: String,
    predicate: String,
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
    rows: Vec<OsintCardRow>,
}

/// `GET /api/osint/card.html?class=<id>&mask=<bits>` — the same ViewFilter,
/// rendered server-side via the compile-time-checked askama template. The
/// *projection* (mask → rows) is identical to the JSON handler; askama is only
/// the XSLT. Each row shows its reasoning role (parsed from the predicate
/// prefix), the field label, and the predicate key.
pub async fn osint_card_html_handler(Query(q): Query<CardQuery>) -> Html<String> {
    let (class, mask) = q.resolve();
    let cv = OsintClassView;
    let rows: Vec<OsintCardRow> = cv
        .render_rows(class, mask)
        .into_iter()
        .map(|r| {
            // predicate = "aiwar:<role>/<field>" — the reasoning role is the prefix.
            let role = r
                .predicate
                .strip_prefix("aiwar:")
                .and_then(|s| s.split('/').next())
                .unwrap_or("")
                .to_string();
            OsintCardRow {
                role,
                label: r.label.to_string(),
                predicate: r.predicate.to_string(),
            }
        })
        .collect();
    let tpl = OsintCardTemplate {
        classid_hex: format!("0x{class:04x}"),
        concept: concept_name(class),
        mask_hex: format!("0x{:03x}", mask.0),
        shown: rows.len(),
        total: cv.field_count(class),
        rows,
    };
    // askama render is infallible for this static template; fall back to a
    // terse error body rather than panicking in a request handler.
    match tpl.render() {
        Ok(body) => Html(body),
        Err(e) => Html(format!("<pre>osint card render error: {e}</pre>")),
    }
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
        let rows: Vec<OsintCardRow> = cv
            .render_rows(OSINT_PERSON_CLASS, FieldMask::FULL)
            .into_iter()
            .map(|r| OsintCardRow {
                role: r
                    .predicate
                    .strip_prefix("aiwar:")
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("")
                    .to_string(),
                label: r.label.to_string(),
                predicate: r.predicate.to_string(),
            })
            .collect();
        let tpl = OsintCardTemplate {
            classid_hex: "0x0701".to_string(),
            concept: "osint_person",
            mask_hex: "0x01f".to_string(),
            shown: rows.len(),
            total: 5,
            rows,
        };
        let body = tpl.render().expect("askama render");
        assert!(body.contains("osint_person"));
        assert!(body.contains("motive"));
        assert!(body.contains("aiwar:person/motive"));
        // dumb-loop template: every selected field is a row.
        assert_eq!(body.matches("<tr>").count(), 5 + 1); // 5 rows + header row
    }
}
