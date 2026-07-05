#!/usr/bin/env bash
# bake_bremen.sh — produce and publish the OSM `/geo` + `/helix?scene=osm` artifact.
#
# PR #75 wired the frontend routes (`/geo`, `/helix?scene=osm`), the BSO2
# Signed360 encoder (`geo/src/bso2.rs`), and the manifest key
# `osm_latest → bremen.helix.soa.gz` — but never produced or published the
# artifact itself. `bremen.helix.soa.gz` is `.gitignore`d (big binary) and is
# absent from the `fma-body-soa-v3-v1` release, so BodyHelix.tsx's fetch 404s
# (same-origin AND the release fallback) and both routes render an error. This
# script is the missing step: bake the artifact and upload it to the release the
# frontend falls back to.
#
# It must run somewhere OSM data hosts are reachable (a dev machine or CI). The
# q2 cloud sandbox cannot run it — its egress proxy denies download.geofabrik.de
# (HTTP 403 policy denial). The end-USER's browser is unaffected: once the asset
# is on the release, BodyHelix fetches it client-side with no proxy involved.
#
# Usage:
#   GITHUB_TOKEN=ghp_... ./geo/bake_bremen.sh                 # bake + upload
#   ./geo/bake_bremen.sh --local-only                         # bake into cockpit/public, no upload
#   PBF=/path/to/bremen-latest.osm.pbf ./geo/bake_bremen.sh   # bake from a pre-downloaded extract
#   REGION=hamburg STAMP=hamburg.helix.soa.gz ./geo/bake_bremen.sh   # a different city (also set osm_latest)
#
# Deps: curl, gzip, cargo. Upload uses `gh` if present, else curl + $GITHUB_TOKEN.
set -euo pipefail

# ── config (env-overridable) ──────────────────────────────────────────────
REGION="${REGION:-bremen}"
PBF_URL="${PBF_URL:-https://download.geofabrik.de/europe/germany/${REGION}-latest.osm.pbf}"
STAMP="${STAMP:-bremen.helix.soa.gz}"          # MUST match body.manifest.json osm_latest
RELEASE_TAG="${RELEASE_TAG:-fma-body-soa-v3-v1}"  # MUST match BodyHelix.tsx REL constant
REPO="${REPO:-AdaWorldAPI/q2}"

GEO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
Q2_ROOT="$(cd "$GEO_DIR/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

local_only=false
[[ "${1:-}" == "--local-only" ]] && local_only=true

# curl through a CA bundle if the environment set one (harmless otherwise).
CURL=(curl -fSL --retry 3)
[[ -n "${CURL_CA_BUNDLE:-}" ]] && CURL+=(--cacert "$CURL_CA_BUNDLE")

# ── 1. obtain the .osm.pbf ────────────────────────────────────────────────
if [[ -n "${PBF:-}" ]]; then
  echo "==> using pre-downloaded PBF: $PBF"
  pbf="$PBF"
else
  pbf="$WORK/${REGION}.osm.pbf"
  echo "==> downloading $PBF_URL"
  "${CURL[@]}" -o "$pbf" "$PBF_URL"
fi
echo "    $(du -h "$pbf" | cut -f1) $pbf"

# ── 2. build the baker ────────────────────────────────────────────────────
echo "==> cargo build --release --features osm,helix --bin osm_helix"
( cd "$GEO_DIR" && cargo build --release --features osm,helix --bin osm_helix )
osm_helix="$GEO_DIR/target/release/osm_helix"

# ── 3. bake → BSO2 (+ .blocks sidecar) ────────────────────────────────────
soa="$WORK/bake.soa"
echo "==> baking"
"$osm_helix" "$pbf" "$soa"      # writes $soa + $soa's .blocks sibling

# ── 4. gzip (BodyHelix inflates via DecompressionStream) ──────────────────
gz="$WORK/$STAMP"
gzip -9 -c "$soa" > "$gz"
echo "==> $(du -h "$gz" | cut -f1) $gz"

# ── 5a. local dev copy (cockpit serves cockpit/public/ same-origin first) ──
cp "$gz" "$Q2_ROOT/cockpit/public/$STAMP"
echo "==> copied to cockpit/public/$STAMP (git-ignored; local dev only)"

if $local_only; then
  echo "==> --local-only: skipping release upload. Run \`cd cockpit && npm run build\` then the cockpit to see /geo."
  exit 0
fi

# ── 5b. publish to the release BodyHelix falls back to ────────────────────
echo "==> uploading $STAMP to $REPO release $RELEASE_TAG"
if command -v gh >/dev/null 2>&1; then
  gh release upload "$RELEASE_TAG" "$gz" --repo "$REPO" --clobber
else
  : "${GITHUB_TOKEN:?set GITHUB_TOKEN (or install gh) to upload}"
  tok="$(printf '%s' "$GITHUB_TOKEN" | tr -d '"'\')"   # strip stray quotes some sandboxes add
  rel_id="$("${CURL[@]}" -H "Authorization: Bearer $tok" \
    "https://api.github.com/repos/$REPO/releases/tags/$RELEASE_TAG" | grep -m1 '"id"' | grep -oE '[0-9]+')"
  [[ -n "$rel_id" ]] || { echo "could not resolve release id for tag $RELEASE_TAG"; exit 1; }
  # delete a same-named asset first (the upload API 422s on a duplicate name).
  old="$("${CURL[@]}" -H "Authorization: Bearer $tok" \
    "https://api.github.com/repos/$REPO/releases/$rel_id/assets" \
    | grep -B1 "\"name\": \"$STAMP\"" | grep -m1 '"id"' | grep -oE '[0-9]+' || true)"
  [[ -n "$old" ]] && "${CURL[@]}" -X DELETE -H "Authorization: Bearer $tok" \
    "https://api.github.com/repos/$REPO/releases/assets/$old" || true
  "${CURL[@]}" -H "Authorization: Bearer $tok" -H "Content-Type: application/gzip" \
    --data-binary @"$gz" \
    "https://uploads.github.com/repos/$REPO/releases/$rel_id/assets?name=$STAMP" >/dev/null
fi
echo "==> done. /geo and /helix?scene=osm now resolve $STAMP from the release."
