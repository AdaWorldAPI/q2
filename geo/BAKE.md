# Baking the `/geo` + `/helix?scene=osm` artifact

## Why the routes were "not working"

PR #75 shipped the **code** for the OSM viewer but never shipped its **data**:

- `cockpit/src/main.tsx` registers `<Route path="/geo">` and `/helix`;
- `cockpit/src/BodyHelix.tsx` reads `scene=osm` / pathname `/geo` and fetches
  the manifest key `osm_latest` → `bremen.helix.soa.gz`;
- `geo/src/bso2.rs` encodes that artifact (BSO2 ver 6, `Signed360` normals, the
  exact inverse of BodyHelix's decoder — see the round-trip test);
- `body.manifest.json` sets `"osm_latest": "bremen.helix.soa.gz"`.

But **`bremen.helix.soa.gz` was never produced or published.** It is
`.gitignore`d (like every big bake) and is *not* among the
`fma-body-soa-v3-v1` release assets (which are all `body.*`). So BodyHelix's
fetch 404s twice — same-origin `cockpit/public/` (git-ignored, absent) *and*
the release fallback — and the page renders `HTTP 404 fetching
bremen.helix.soa.gz`. `/helix` (the anatomy body) works because its artifact,
`body.20260629c.v6helix.soa.gz`, *is* in that release.

This is the same delivery model as the body bake: big binaries live in the
GitHub **release**, and BodyHelix fetches them client-side (browser → release,
no server involvement). The missing step was simply running the bake and
uploading the result.

## The fix: run the bake once

`./geo/bake_bremen.sh` automates it — download the Bremen extract, build
`osm_helix`, bake, gzip, and upload `bremen.helix.soa.gz` to the release.

```bash
# from the q2 repo root, where OSM data is reachable (a dev machine or CI):
GITHUB_TOKEN=ghp_...  ./geo/bake_bremen.sh          # bake + publish to the release
./geo/bake_bremen.sh --local-only                   # bake into cockpit/public only
PBF=/path/to/bremen-latest.osm.pbf ./geo/bake_bremen.sh   # bake from a local extract
```

> **The q2 cloud sandbox cannot run this.** Its egress proxy denies
> `download.geofabrik.de` (HTTP 403 policy denial), and no Bremen extract is on
> disk. Run it from a machine that can reach Geofabrik (or pass a pre-downloaded
> `PBF=…`). The upload targets the `fma-body-soa-v3-v1` release, which the
> deployed cockpit already falls back to — so end users get the data with no
> proxy in the path.

## The CI arm: `.github/workflows/bake-osm.yml`

GitHub-hosted runners have direct egress, so the bake also ships as a
`workflow_dispatch` workflow — the "or CI" of "a dev machine or CI". It checks
out q2 plus the two sibling clones the `helix` feature's path-deps expect
(`../../ndarray`, `../../lance-graph`), builds `osm_helix`, downloads the
extract, bakes, and uploads to the release via `gh`.

```
Actions → bake-osm → Run workflow
  region: berlin            # any Geofabrik Germany extract name
  stamp:  (empty)           # defaults to <region>.helix.soa.gz
  upload: true              # untick for a dry-run bake without publishing
```

The manifest currently points `osm_latest` at `berlin.helix.soa.gz`, so the
default inputs produce exactly the asset `/geo` fetches.

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
