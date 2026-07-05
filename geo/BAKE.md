# Baking the `/geo` + `/helix?scene=osm` artifact

## Status (2026-07-05)

**Resolved.** `berlin.helix.soa.gz` (91.8 MB, 534k Berlin buildings) is
published on the `fma-body-soa-v3-v1` release. `body.manifest.json` points
`osm_latest` → `berlin.helix.soa.gz`. Both `/geo` and `/helix?scene=osm`
resolve the artifact from the release via BodyHelix's client-side fetch.

## Root cause (PR #75)

PR #75 shipped the **code** for the OSM viewer but never shipped its **data**:

- `cockpit/src/main.tsx` registers `<Route path="/geo">` and `/helix`;
- `cockpit/src/BodyHelix.tsx` reads `scene=osm` / pathname `/geo` and fetches
  the manifest key `osm_latest`;
- `geo/src/bso2.rs` encodes that artifact (BSO2 ver 6, `Signed360` normals, the
  exact inverse of BodyHelix's decoder — see the round-trip test);
- `body.manifest.json` sets `"osm_latest": "berlin.helix.soa.gz"`.

The fix was running the bake (from Berlin OSM data, since Geofabrik was
proxy-blocked in the cloud sandbox) and publishing the result to the release.

## Re-baking (for a different city or to refresh)

`./geo/bake_bremen.sh` automates it — download the extract, build
`osm_helix`, bake, gzip, and upload to the release.

```bash
# from the q2 repo root, where OSM data is reachable (a dev machine or CI):
GITHUB_TOKEN=ghp_...  ./geo/bake_bremen.sh          # bake + publish to the release
./geo/bake_bremen.sh --local-only                   # bake into cockpit/public only
PBF=/path/to/berlin-latest.osm.pbf ./geo/bake_bremen.sh   # bake from a local extract
```

> **The q2 cloud sandbox cannot run the upload step.** Its egress proxy blocks
> `download.geofabrik.de` (HTTP 403 policy denial) and the session type lacks
> release-asset upload permission. Run it from a machine that can reach Geofabrik
> and has write access to the release. The upload targets the `fma-body-soa-v3-v1`
> release, which the deployed cockpit already falls back to — so end users get the
> data with no proxy in the path.

## Verifying

After the upload, hard-load `/geo` (or `/helix?scene=osm`) in the cockpit. The
header should read `… verts · … structures · helix::Signed360 normals`. If it
still 404s, confirm the asset name on the release matches `osm_latest` in
`cockpit/public/body.manifest.json` and the `REL` tag in `BodyHelix.tsx`.

## Baking a different city

The wire is domain-agnostic. Point at another Geofabrik extract and update the
manifest slot to match:

```bash
REGION=hamburg STAMP=hamburg.helix.soa.gz ./geo/bake_bremen.sh
# then set body.manifest.json "osm_latest": "hamburg.helix.soa.gz"
```
