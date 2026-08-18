//! The multi-region OSM bake catalogue — the consumer side of
//! `lance_graph::soa_config`'s boot-config pattern
//! (`docs/SOA_BAKE_DEPLOYMENT.md` §6 in the `lance-graph` repo).
//!
//! # The problem this closes
//!
//! Before this module, exactly one region was servable per deploy, chosen by
//! the `OSM_BAKE_REGION` env var (`osm_slab_hydrate::bake_region`). Adding or
//! switching a region meant editing that env var on Railway, which triggers a
//! redeploy — even though the bake itself already lives in S3
//! (`q2/bakes/<region>-v1`) and needs no code change at all.
//!
//! This module reads a declarative catalogue instead: `.config/q2/config.yaml`
//! in the same bucket, via [`lance_graph::soa_config`]'s shared schema and
//! parser — the SAME module/file every other AdaWorldAPI deployment reads its
//! own bake catalogue from (`SoaConfig`/`BakeEntry`). Adding a region to the
//! catalogue is an S3 object edit, not a redeploy.
//!
//! # `table`/`classid` are validated but not used here
//!
//! [`lance_graph::soa_config::BakeEntry`] was designed for bakes written as
//! Lance tables (`soa_to_lance`). This deployment's OSM bakes are still raw
//! `.soa`/`.books`/`.chains` file triplets fetched by checksum
//! (`osm_slab_hydrate::artifacts`) — a different, older hydration shape that
//! predates `soa_config` and is not being migrated by this change. Reusing
//! `soa_config`'s YAML schema and parser here is deliberate: `name` and
//! `hydrate` are exactly what a region catalogue needs, and reusing the
//! shared, already-hardened parser (unknown-key rejection, duplicate-name
//! rejection, `deny_unknown_fields`) is strictly better than hand-rolling a
//! near-identical one. `table` and `classid` still have to be present and
//! valid for a bake entry to parse (the schema requires them), but nothing in
//! this module reads either — a `table` value can be any non-empty string, by
//! convention the region's own bake identifier (e.g. `brandenburg-v1`).
//!
//! # `hydrate: false` is a real state, not a soft error
//!
//! A region declared with `hydrate: false` shows up in the
//! [`RegionEntry`] catalogue — and therefore in `/api/osm/regions`'s response,
//! for a UI region menu — but [`crate::osm_slab_hydrate::ensure_slab_local`]
//! never downloads its multi-GB artifact triplet. That is the entire point:
//! declaring a region costs nothing until an operator flips it to
//! `hydrate: true` (or names it via `OSM_BAKE_REGION`, see below).
//!
//! # The `OSM_BAKE_REGION` escape hatch still works, unchanged
//!
//! `OSM_BAKE_REGION`, when set, always selects the ACTIVE region — even if it
//! is absent from the catalogue entirely, or declared `hydrate: false` there.
//! This is the pre-existing single-region behavior and this module does not
//! narrow it: a deploy that has never heard of `.config/q2/config.yaml`
//! behaves identically to before, because [`pick_active`] with no catalogue
//! (or an empty one) and an env override just returns that override.

use std::sync::OnceLock;

use axum::Json;
use serde::Serialize;

/// One declared region. `hydrate` is whether THIS deployment pulls its
/// artifact triplet to local disk at boot — see the module doc's "declared
/// but not pulled" section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionEntry {
    pub name: String,
    pub hydrate: bool,
}

/// Parse `.config/<repo>/config.yaml`'s body into a region catalogue.
///
/// Delegates entirely to [`lance_graph::soa_config::parse`] for validation
/// (schema version, duplicate names/tables, malformed classid, unknown
/// keys) — this function's only job is projecting the validated
/// [`lance_graph::soa_config::BakeEntry`] list onto the two fields this
/// deployment actually reads. Returns `None` on any parse/validation
/// failure; the caller falls back to the single-region default rather than
/// serving a half-understood catalogue (same "absent config is not an
/// error, but never silent" posture as `osm_slab_hydrate`).
pub(crate) fn parse_catalogue_yaml(yaml: &str) -> Option<Vec<RegionEntry>> {
    let config = match lance_graph::soa_config::parse(yaml) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "osm regions: config.yaml did not parse; ignoring it");
            return None;
        }
    };
    Some(
        config
            .bakes
            .iter()
            .map(|b| RegionEntry {
                name: b.name.clone(),
                hydrate: b.hydrate,
            })
            .collect(),
    )
}

/// Which region name is ACTIVE — the one whose slab gets hydrated,
/// converted to Lance, mmap'd, and served.
///
/// `override_region` (from `OSM_BAKE_REGION`) always wins when set, whether
/// or not it names a catalogue entry — see the module doc. Otherwise the
/// first `hydrate: true` catalogue entry wins (declaration order is
/// priority order); `None` if the catalogue is empty and there is no
/// override, meaning nothing is configured to serve.
pub(crate) fn pick_active<'a>(
    catalogue: &'a [RegionEntry],
    override_region: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(name) = override_region {
        return Some(name);
    }
    catalogue
        .iter()
        .find(|e| e.hydrate)
        .map(|e| e.name.as_str())
}

/// The catalogue snapshot [`crate::osm_slab_hydrate::ensure_slab_local`]
/// publishes at boot, for `/api/osm/regions` to read without redoing any
/// S3 I/O or re-parsing YAML per request.
struct CatalogueSnapshot {
    regions: Vec<RegionEntry>,
    active: String,
}

static CATALOGUE: OnceLock<Option<CatalogueSnapshot>> = OnceLock::new();

/// Publish the boot-resolved catalogue. Called exactly once, from
/// `ensure_slab_local`, before the listener binds — see that function's own
/// single-threaded-startup safety note. A second call is a silent no-op
/// (`OnceLock::set` returns `Err` and this function drops it) rather than a
/// panic, so a future refactor that accidentally calls this twice fails
/// quietly instead of crashing the boot sequence over a diagnostics
/// side-channel.
pub(crate) fn publish(regions: Vec<RegionEntry>, active: String) {
    let _ = CATALOGUE.set(Some(CatalogueSnapshot { regions, active }));
}

/// `(regions, active)` as published at boot, or `None` if OSM hydration was
/// never configured on this deploy (mirrors `ensure_slab_local` returning
/// `None`) — `/api/osm/regions` reads this directly.
pub(crate) fn snapshot() -> Option<(&'static [RegionEntry], &'static str)> {
    CATALOGUE
        .get()
        .and_then(|opt| opt.as_ref())
        .map(|s| (s.regions.as_slice(), s.active.as_str()))
}

/// `GET /api/osm/regions` — the map-menu data source: every declared
/// region, whether it is hydrated (immediately servable), and which one is
/// currently active.
///
/// Read-only discovery. This deployment still serves exactly one ACTIVE
/// region at a time (the `open_slab()` mmap is process-global) — this
/// endpoint does not let a client switch it, only see the catalogue an
/// operator has declared in `.config/q2/config.yaml`. A region shown with
/// `"hydrated": false` needs the operator to flip it to `hydrate: true` (or
/// set `OSM_BAKE_REGION`) and restart before it can become active.
pub async fn osm_regions_handler() -> Json<serde_json::Value> {
    match snapshot() {
        Some((regions, active)) => Json(serde_json::json!({
            "active": active,
            "regions": regions.iter().map(|r| serde_json::json!({
                "name": r.name,
                "hydrated": r.hydrate,
                "active": r.name == active,
            })).collect::<Vec<_>>(),
        })),
        None => Json(serde_json::json!({
            "active": serde_json::Value::Null,
            "regions": [],
            "note": "OSM hydration is not configured on this deploy",
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_with_two_regions() -> &'static str {
        r#"
version: 1
ledger_prefix: "q2/ledger"
bakes:
  - name: berlin
    table: berlin-v1
    classid: "0x0F020000"
    hydrate: true
  - name: brandenburg
    table: brandenburg-v1
    classid: "0x0F020000"
    hydrate: false
"#
    }

    #[test]
    fn parse_catalogue_yaml_projects_name_and_hydrate() {
        let entries = parse_catalogue_yaml(yaml_with_two_regions()).expect("must parse");
        assert_eq!(
            entries,
            vec![
                RegionEntry {
                    name: "berlin".to_string(),
                    hydrate: true
                },
                RegionEntry {
                    name: "brandenburg".to_string(),
                    hydrate: false
                },
            ]
        );
    }

    /// The whole point of a hydrate:false entry: it survives the parse and
    /// is neither dropped nor silently promoted to hydrate:true.
    #[test]
    fn hydrate_false_entries_are_kept_not_dropped() {
        let entries = parse_catalogue_yaml(yaml_with_two_regions()).unwrap();
        assert_eq!(entries.len(), 2, "a hydrate:false entry must not vanish");
        let brandenburg = entries.iter().find(|e| e.name == "brandenburg").unwrap();
        assert!(!brandenburg.hydrate);
    }

    #[test]
    fn parse_catalogue_yaml_rejects_malformed_yaml() {
        assert!(parse_catalogue_yaml("not: [valid").is_none());
    }

    /// The shared parser's own duplicate-name rejection must be visible
    /// through this projection, not swallowed — a config with two regions
    /// named "berlin" is a real authoring mistake and must fail loudly.
    #[test]
    fn parse_catalogue_yaml_propagates_shared_parser_validation() {
        let yaml = r#"
version: 1
ledger_prefix: "q2/ledger"
bakes:
  - name: berlin
    table: a
    classid: "0x0F020000"
  - name: berlin
    table: b
    classid: "0x0F020000"
"#;
        assert!(
            parse_catalogue_yaml(yaml).is_none(),
            "duplicate region name must be rejected, not silently deduped"
        );
    }

    #[test]
    fn pick_active_prefers_the_env_override_even_when_absent_from_the_catalogue() {
        let catalogue = parse_catalogue_yaml(yaml_with_two_regions()).unwrap();
        // "munich" names neither catalogue entry — the override must still win,
        // preserving the pre-catalogue OSM_BAKE_REGION escape hatch exactly.
        assert_eq!(pick_active(&catalogue, Some("munich")), Some("munich"));
    }

    #[test]
    fn pick_active_prefers_the_env_override_over_a_hydrate_true_entry() {
        let catalogue = parse_catalogue_yaml(yaml_with_two_regions()).unwrap();
        // berlin is hydrate:true and would win by default — the override
        // (naming the hydrate:false brandenburg) must still take priority.
        assert_eq!(
            pick_active(&catalogue, Some("brandenburg")),
            Some("brandenburg")
        );
    }

    #[test]
    fn pick_active_falls_back_to_the_first_hydrate_true_entry() {
        let catalogue = parse_catalogue_yaml(yaml_with_two_regions()).unwrap();
        assert_eq!(pick_active(&catalogue, None), Some("berlin"));
    }

    /// Paired negative case: an ALL-false catalogue has no active region
    /// without an override — this must not silently pick a declared-but-
    /// unhydrated region and then fail to find its files on disk.
    #[test]
    fn pick_active_returns_none_when_nothing_is_hydrate_true_and_no_override() {
        let catalogue = vec![RegionEntry {
            name: "brandenburg".to_string(),
            hydrate: false,
        }];
        assert_eq!(pick_active(&catalogue, None), None);
    }

    #[test]
    fn pick_active_on_an_empty_catalogue_with_no_override_is_none() {
        assert_eq!(pick_active(&[], None), None);
    }

    #[test]
    fn pick_active_on_an_empty_catalogue_still_honors_an_override() {
        // The synthetic single-entry fallback path: no bucket config exists
        // at all, so the catalogue is empty, but OSM_BAKE_REGION is set.
        assert_eq!(pick_active(&[], Some("berlin")), Some("berlin"));
    }

    #[test]
    fn snapshot_is_none_before_publish() {
        // NOTE: this test only holds if it runs before any other test in
        // this process calls `publish` — OnceLock is process-global. Given
        // nextest's one-process-per-test model (already relied on elsewhere
        // in this crate, e.g. osm_features.rs's slab_digest tests), that is
        // safe here.
        assert!(snapshot().is_none());
    }

    #[test]
    fn publish_then_snapshot_round_trips() {
        let regions = vec![RegionEntry {
            name: "berlin".to_string(),
            hydrate: true,
        }];
        publish(regions.clone(), "berlin".to_string());
        let (got_regions, got_active) = snapshot().expect("must be published now");
        assert_eq!(got_regions, regions.as_slice());
        assert_eq!(got_active, "berlin");
    }

    #[test]
    fn a_second_publish_is_a_silent_no_op_not_a_panic() {
        publish(
            vec![RegionEntry {
                name: "x".to_string(),
                hydrate: true,
            }],
            "x".to_string(),
        );
        // Must not panic — OnceLock::set failing on the second call is
        // expected and handled, not propagated.
        publish(
            vec![RegionEntry {
                name: "y".to_string(),
                hydrate: true,
            }],
            "y".to_string(),
        );
    }
}
