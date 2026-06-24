//! Minimal line-oriented Turtle reader for the FMA heart fixture.
//!
//! NOT a general Turtle parser and NOT the production loader: the real
//! Foundational Model of Anatomy (266 MB `fma.owl`, ~1.5M triples, OGIT
//! contextId 13) hydrates through `lance-graph-rdf` /
//! `lance_graph_ontology::hydrate_fma` at the spine. This subset reader lets
//! the light bake (which deliberately excludes the lance/datafusion closure)
//! demonstrate the same `ttl → hydrate → canonical-GUID → .soa` thread on a
//! fixture; swapping in the real OWL is a data change upstream, not a rewrite.
//!
//! Accepted grammar — one triple per line, prefixed names only:
//! ```text
//! fma:Left_ventricle bfo:part_of fma:Heart .
//! ```
//! `#` comments, blank lines, and `@prefix` declarations are skipped.
//! Recognised predicates mirror the canonical hydrator set (pr-d-1):
//! `bfo:part_of` / `fma:regional_part_of` / `fma:constitutional_part_of`
//! (partonomy) and `rdfs:subClassOf` (cross-cutting type).

/// A parsed FMA fragment: containment edges + cross-cutting type edges, each as
/// `(child, parent)` IRIs with prefixes intact (used verbatim as map keys).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Fragment {
    /// `child bfo:part_of parent` — the partonomy cascade.
    pub part_of: Vec<(String, String)>,
    /// `child rdfs:subClassOf type` — the cross-cutting membership.
    pub is_a: Vec<(String, String)>,
}

/// Human label for an IRI: drop the `prefix:` and turn `_` into spaces
/// (`fma:Left_ventricle` → `Left ventricle`).
pub fn label_of(iri: &str) -> String {
    iri.rsplit(':').next().unwrap_or(iri).replace('_', " ")
}

/// Parse the line-oriented Turtle subset. Unrecognised predicates are ignored
/// (the reader is intentionally narrow; the spine loader handles full OWL).
pub fn parse(ttl: &str) -> Fragment {
    let mut frag = Fragment::default();
    for line in ttl.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("@prefix") {
            continue;
        }
        let body = line.strip_suffix('.').unwrap_or(line);
        let mut toks = body.split_whitespace();
        let (Some(s), Some(p), Some(o)) = (toks.next(), toks.next(), toks.next()) else {
            continue;
        };
        match p {
            "bfo:part_of" | "fma:regional_part_of" | "fma:constitutional_part_of" => {
                frag.part_of.push((s.to_string(), o.to_string()));
            }
            "rdfs:subClassOf" => frag.is_a.push((s.to_string(), o.to_string())),
            _ => {}
        }
    }
    frag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_part_of_and_is_a_skipping_noise() {
        let ttl = "# comment\n@prefix fma: <x> .\n\n\
                   fma:Left_ventricle bfo:part_of fma:Heart .\n\
                   fma:Myocardium_of_left_ventricle rdfs:subClassOf fma:Cardiac_muscle_tissue .\n";
        let f = parse(ttl);
        assert_eq!(
            f.part_of,
            vec![("fma:Left_ventricle".into(), "fma:Heart".into())]
        );
        assert_eq!(
            f.is_a,
            vec![(
                "fma:Myocardium_of_left_ventricle".into(),
                "fma:Cardiac_muscle_tissue".into()
            )]
        );
    }

    #[test]
    fn label_strips_prefix_and_underscores() {
        assert_eq!(label_of("fma:Left_ventricle"), "Left ventricle");
    }
}
