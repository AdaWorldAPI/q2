//! The aiwar typed object model, built on `lance-graph-contract`.
//!
//! A node label resolves to its `EntityTypeId` through the contract's
//! [`entity_type_id`] — the shared `u16` codebook (the Palantir-Foundry
//! "Object Type" / BindSpace Column-H). This is the canonical register every
//! consumer uses (`lance-graph-callcenter`, `openproject-nexgen-rs`); q2 must
//! NOT mint its own per-column `u16` dictionary (the `e43630f` mistake).

use std::sync::LazyLock;

use lance_graph_contract::ontology::{entity_type_id, EntityTypeId, Ontology};
use lance_graph_contract::property::{LinkSpec, Schema};

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
        // The six relationship types — the canonical *link* codebook on the
        // Edge union (`reltype` u16, the relationship analogue of `entity_type`).
        // The union connects entities, so every link is `Entity → Entity`;
        // `LinkSpec` order fixes the `rel_type_id` (1-based, mirroring how
        // schema order fixes `EntityTypeId`). CONNECTED_TO=1 … HIERARCHICAL=6.
        .link(LinkSpec::many_to_many("Entity", "CONNECTED_TO", "Entity"))
        .link(LinkSpec::one_to_many("Entity", "DEVELOPED_BY", "Entity"))
        .link(LinkSpec::one_to_many("Entity", "DEPLOYED_BY", "Entity"))
        .link(LinkSpec::many_to_many("Entity", "USED_IN", "Entity"))
        .link(LinkSpec::many_to_many("Entity", "PERSON_LINK", "Entity"))
        .link(LinkSpec::one_to_many("Entity", "HIERARCHICAL", "Entity"))
        .build()
});

/// Resolve a node label to its canonical [`EntityTypeId`] (the shared `u16`
/// codebook), or `0` if the label is not a known entity type. This is the
/// single source of truth for label typing — never a per-column Arrow
/// dictionary.
pub fn label_type_id(label: &str) -> EntityTypeId {
    entity_type_id(&AIWAR_ONTOLOGY, label)
}

/// Resolve a relationship type (edge predicate) to its canonical `u16`
/// link-type id, or `0` if unknown. The relationship analogue of
/// [`label_type_id`]: 1-based position in `Ontology.links`, keyed by
/// predicate. Predicates are unique in `AIWAR_ONTOLOGY`, so the position is
/// well-defined. This is the codebook for the Edge union's `reltype` column —
/// never a per-column Arrow dictionary.
pub fn rel_type_id(rel: &str) -> EntityTypeId {
    AIWAR_ONTOLOGY
        .links
        .iter()
        .position(|l| l.predicate == rel)
        .map(|idx| (idx + 1) as EntityTypeId)
        .unwrap_or(0)
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

    #[test]
    fn canonical_rel_type_ids_come_from_the_contract_links() {
        // 1-based, contract convention; 0 = unknown. The same `u16` means
        // the same relationship type everywhere — the link codebook, shared.
        assert_eq!(rel_type_id("CONNECTED_TO"), 1);
        assert_eq!(rel_type_id("DEVELOPED_BY"), 2);
        assert_eq!(rel_type_id("DEPLOYED_BY"), 3);
        assert_eq!(rel_type_id("USED_IN"), 4);
        assert_eq!(rel_type_id("PERSON_LINK"), 5);
        assert_eq!(rel_type_id("HIERARCHICAL"), 6);
        assert_eq!(rel_type_id("NotARelType"), 0);
    }

    #[test]
    fn ontology_has_the_six_aiwar_relationship_types() {
        assert_eq!(AIWAR_ONTOLOGY.links.len(), 6);
    }
}
