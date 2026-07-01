//! The canonical **OSINT ClassView** — classid `0x0700`, the AIRO/AIwar card.
//!
//! This is the holy-grail schema for the OSINT domain: the ordered 12-field card
//! whose *labels* live here (in the ClassView, above the SoA) while the *values*
//! live in the node's ValueTenant bytes. The `FieldMask` is the Redmine-style
//! ViewFilter — a bitmask selecting which fields render; an askama template is the
//! XSLT that draws the projected rows (`class_view.rs` doctrine header).
//!
//! bit `i` == ValueTenant position `i` (the N3 append-only discriminant). Order
//! MUST match `write_facet_tenant` in `osint_gotham.rs` (value bytes 1..=12) and
//! `FACET_AXES_UI`/`AX` in `cockpit/src/OsintGraph.tsx`. The `predicate_iri`
//! carries the **reasoning role** (need/offer/intent/impact/person/…) so the two
//! orthogonal axes (Demand `offer⟷need`, Causality `intent⟷impact`) and the
//! Person×Situation split are read from the schema, not hard-coded.
//!
//! Canonical home is OGAR (`ogar-vocab`'s `osint` ObjectView); this q2-local impl
//! is the working owner-authored definition until it is mirrored upstream. It
//! follows the existing cockpit pattern of impl'ing a contract trait locally
//! (cf. `mock_driver.rs` impl'ing `CognitiveShaderDriver`).

use std::sync::LazyLock;

use axum::{extract::Query, Json};
use lance_graph_contract::class_view::{ClassId, ClassView, FieldMask};
use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};
use serde::Deserialize;

/// classid `0x0700` — the OSINT concept (the low u16 of the GUID classid).
pub const OSINT_CLASS: ClassId = 0x0700;

/// The canonical OSINT card: 12 AIRO/VAIR fields in FieldMask-bit order. The
/// `predicate_iri` prefix is the reasoning **role**:
/// `need` / `offer` (Demand axis) · `intent` / `causality` (Causality axis) ·
/// `person` (McClelland/Freud trait) · `identity` / `state` / `relation` (context).
static OSINT_FIELDS: LazyLock<[FieldRef; 12]> = LazyLock::new(|| {
    [
        FieldRef::new("aiwar:need/militaryUse", "militaryUse"), // 0  NEED
        FieldRef::new("aiwar:need/civicUse", "civicUse"),       // 1  NEED
        FieldRef::new("aiwar:person/airoRole", "airo:type"),    // 2  PERSON (power P1..P4)
        FieldRef::new("aiwar:need/mlTask", "MLTask"),           // 3  NEED
        FieldRef::new("aiwar:intent/purpose", "purpose:vair"),  // 4  INTENT (explicit)
        FieldRef::new("aiwar:offer/capacity", "capacity:airo"), // 5  OFFER
        FieldRef::new("aiwar:state/currentStatus", "currentStatus"), // 6  STATE
        FieldRef::new("aiwar:identity/type", "type"),           // 7  IDENTITY
        FieldRef::new("aiwar:offer/output", "output:airo"),     // 8  OFFER
        FieldRef::new("aiwar:causality/impact", "impact:vair"), // 9  CAUSALITY (implicit)
        FieldRef::new("aiwar:relation/stakeholder", "stakeholder"), // 10 RELATION (edge)
        FieldRef::new("aiwar:person/motive", "motive"),         // 11 PERSON (McClelland nPow/nAch/nAff)
    ]
});

/// The owner-authored ClassView for classid `0x0700`. Only `0x0700` resolves to
/// the card; every other classid is the zero-fallback empty shape.
pub struct OsintClassView;

impl ClassView for OsintClassView {
    fn fields(&self, class: ClassId) -> &[FieldRef] {
        if class == OSINT_CLASS {
            &OSINT_FIELDS[..]
        } else {
            &[]
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

/// `?mask=<u64>` — the ViewFilter bitmask (bit i = show field i). Omitted = FULL.
#[derive(Deserialize)]
pub struct CardQuery {
    mask: Option<u64>,
}

/// `GET /api/osint/card?mask=<bits>` — project the `0x0700` card through the
/// FieldMask and return the surviving `(label, predicate)` rows. This is the
/// Redmine ERB ViewFilter, server-side: the mask selects the columns, the
/// ClassView resolves the labels, nothing is computed on the client.
pub async fn osint_card_handler(Query(q): Query<CardQuery>) -> Json<serde_json::Value> {
    let mask = q.mask.map(FieldMask).unwrap_or(FieldMask::FULL);
    let cv = OsintClassView;
    let rows: Vec<serde_json::Value> = cv
        .render_rows(OSINT_CLASS, mask)
        .into_iter()
        .map(|r| serde_json::json!({ "label": r.label, "predicate": r.predicate }))
        .collect();
    Json(serde_json::json!({
        "classid": format!("0x{OSINT_CLASS:04x}"),
        "mask": mask.0,
        "field_count": cv.field_count(OSINT_CLASS),
        "shown": rows.len(),
        "rows": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_has_twelve_fields_in_bit_order() {
        let cv = OsintClassView;
        assert_eq!(cv.field_count(OSINT_CLASS), 12);
        assert_eq!(cv.field_label(OSINT_CLASS, 0), Some("militaryUse"));
        assert_eq!(cv.field_label(OSINT_CLASS, 9), Some("impact:vair"));
        assert_eq!(cv.field_label(OSINT_CLASS, 11), Some("motive"));
        // unknown class = zero-fallback empty shape.
        assert_eq!(cv.field_count(0x0000), 0);
    }

    #[test]
    fn field_mask_is_the_view_filter() {
        let cv = OsintClassView;
        // full mask → all 12 rows.
        assert_eq!(cv.render_rows(OSINT_CLASS, FieldMask::FULL).len(), 12);
        // mask with only the Causality axis ends (intent bit 4 + impact bit 9).
        let causal = FieldMask::EMPTY.with(4).with(9);
        let rows = cv.render_rows(OSINT_CLASS, causal);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "purpose:vair");
        assert_eq!(rows[1].label, "impact:vair");
    }
}
