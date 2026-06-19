//! The aiwar typed object model, built on `lance-graph-contract`.
//!
//! A node label resolves to its `EntityTypeId` through the contract's
//! [`entity_type_id`] — the shared `u16` codebook (the Palantir-Foundry
//! "Object Type" / BindSpace Column-H). This is the canonical register every
//! consumer uses (`lance-graph-callcenter`, `openproject-nexgen-rs`); q2 must
//! NOT mint its own per-column `u16` dictionary (the `e43630f` mistake).

use std::sync::LazyLock;

use lance_graph_contract::ontology::{entity_type_id, EntityTypeId, Ontology};
use lance_graph_contract::property::Schema;

/// The aiwar ontology: five node entity types. Schema **order fixes the
/// `EntityTypeId`** (1-based; `entity_type_id` returns `position + 1`, with
/// `0` reserved for untyped). So System=1, Stakeholder=2, Civic=3,
/// Historical=4, Person=5 — one shared codebook across every column and table.
pub static AIWAR_ONTOLOGY: LazyLock<Ontology> = LazyLock::new(|| {
    Ontology::builder("aiwar")
        .schema(
            Schema::builder("System")
                .required("id")
                .required("name")
                .optional("type")
                .optional("year")
                .optional("currentStatus")
                .build(),
        )
        .schema(
            Schema::builder("Stakeholder")
                .required("id")
                .required("name")
                .optional("type")
                .build(),
        )
        .schema(
            Schema::builder("Civic")
                .required("id")
                .required("name")
                .optional("type")
                .optional("year")
                .build(),
        )
        .schema(
            Schema::builder("Historical")
                .required("id")
                .required("name")
                .optional("type")
                .optional("year")
                .build(),
        )
        .schema(
            Schema::builder("Person")
                .required("id")
                .required("name")
                .optional("type")
                .build(),
        )
        .build()
});

/// Resolve a node label to its canonical [`EntityTypeId`] (the shared `u16`
/// codebook), or `0` if the label is not a known entity type. This is the
/// single source of truth for label typing — never a per-column Arrow
/// dictionary.
pub fn label_type_id(label: &str) -> EntityTypeId {
    entity_type_id(&AIWAR_ONTOLOGY, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_entity_type_ids_come_from_the_contract() {
        // 1-based, contract convention; 0 = untyped. These ids are shared —
        // the same `u16` means "System" everywhere, not a per-column index.
        assert_eq!(label_type_id("System"), 1);
        assert_eq!(label_type_id("Stakeholder"), 2);
        assert_eq!(label_type_id("Civic"), 3);
        assert_eq!(label_type_id("Historical"), 4);
        assert_eq!(label_type_id("Person"), 5);
        assert_eq!(label_type_id("Nonexistent"), 0);
    }

    #[test]
    fn ontology_has_the_five_aiwar_entity_types() {
        assert_eq!(AIWAR_ONTOLOGY.schemas.len(), 5);
    }
}
