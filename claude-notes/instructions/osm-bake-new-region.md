# Baking a new OSM region (worked example: Baden-Württemberg)

Serving a second region is **config plus a bake**, not a code change. The
baker was already region-agnostic; as of this change the consumer is too
(`OSM_BAKE_REGION`). What follows is the whole procedure with measured
numbers, because the sizing is the part that bites.

## What the region name controls

One env var drives every name (`osm_slab_hydrate.rs`):

| | value for `OSM_BAKE_REGION=baden-wuerttemberg` |
|---|---|
| S3 prefix | `q2/bakes/baden-wuerttemberg-v1` (override: `OSM_SLAB_S3_PREFIX`) |
| artifacts | `baden-wuerttemberg.{soa,books,chains}` + `SHA256SUMS` |
| volume cache | `$RAILWAY_VOL/osm/baden-wuerttemberg.soa` (+ sidecars) |

The name is validated to `[a-z0-9-]`, ≤64 chars — it is interpolated into an
S3 key *and* joined onto a filesystem path, so `..` or `/` would traverse.
An invalid value logs a warning and falls back to `berlin` rather than
building that path.

Because the cache dir is region-keyed, **two regions coexist on one volume**:
flipping `OSM_BAKE_REGION` and restarting switches maps, and the other
region's files stay warm (no re-download when you flip back). Size the volume
for the sum if you intend to keep both.

## Sizing — measured, not estimated

| | Berlin (shipped) | Baden-Württemberg (projected) |
|---|---|---|
| Geofabrik `.osm.pbf` | **99 MB** (measured) | **645 MB** (measured) |
| ratio to Berlin | 1× | **6.5×** |
| `.soa` slab | **1.35 GB** = 2.64 M rows × 512 B | **~8.7 GB** |
| `.chains` | 64 MB | ~0.4 GB |
| all three | ~1.42 GB | **~9.1 GB** |

The slab is one 512-byte row per kept feature, so it scales with feature
count rather than with PBF bytes — the 6.5× is the best available proxy and
the projection is therefore **approximate**. Treat ~9 GB as the planning
figure and read the real size off the first bake.

**Machine requirements for the bake step:**

- **Disk ≥ 12 GB free** — 645 MB PBF + ~9.1 GB output + headroom for the
  `.part` files the baker renames into place.
- **RAM: budget 32 GB.** `read::read_features_with_chains` reads *all*
  features plus every way's vertex chain into memory before sorting
  (`Vec<Keyed>` + the chain map), so peak RSS scales with the region. This is
  the constraint that actually decides where you can run it — not disk.
- Runtime: Berlin bakes in minutes; expect roughly 6-7× that.

**Railway volume ≥ 12 GB** for serving (the container only needs the output,
not the PBF).

## Procedure

```bash
# 1. The extract (Geofabrik publishes daily; ~645 MB)
curl -fSL -o bw.osm.pbf \
  https://download.geofabrik.de/europe/germany/baden-wuerttemberg-latest.osm.pbf

# 2. Bake. Region-agnostic: PBF in, slab out. Sidecars land beside the slab.
cargo run --release -p osm-soa-bake --bin bake -- \
  bw.osm.pbf baden-wuerttemberg.soa

# 3. Pin the bytes. The hydrator verifies every file against this on download
#    AND on a warm cache hit — there is no unverified path.
sha256sum baden-wuerttemberg.soa \
          baden-wuerttemberg.books \
          baden-wuerttemberg.chains > SHA256SUMS

# 4. Publish all four together, into the region's own prefix.
aws s3 cp --recursive . "s3://$AWS_S3_BUCKET_NAME/q2/bakes/baden-wuerttemberg-v1/" \
  --exclude '*' --include 'baden-wuerttemberg.*' --include 'SHA256SUMS'

# 5. Serve it: set OSM_BAKE_REGION=baden-wuerttemberg on the service, redeploy.
```

Publish the sidecars **before or with** the slab, never after: the hydrator
requires all three, and a prefix holding a slab without its chains is a stale
bake rather than a valid state.

## Verifying the switch

The boot log names the region it resolved, so one line settles which bake is
live:

```
INFO osm slab: resolving from S3 region=baden-wuerttemberg bucket=… prefix=q2/bakes/baden-wuerttemberg-v1 dir=…
INFO osm slab: hydrated and verified path=/volume01/osm/baden-wuerttemberg.soa
```

Then, from the outside:

- `GET /api/osm/geometry/tile-bin/12/2133/1420` (Stuttgart-ish) returns
  `application/octet-stream` starting with the `OSM1` magic, not a 503.
- The cockpit's status line reports a shape/row count for a BW viewport.
- The `.chains` digest pin is enforced against the slab, so a mismatched pair
  refuses loudly (`osm chains: sidecar pinned to a DIFFERENT slab`) instead
  of drawing one bake's geometry against another's identities.

## Why this was not baked in the dev container

Recorded so the next session does not retry it: the container had **9.8 GB
free disk and 15 GB RAM**, against ~9.1 GB of output plus a 645 MB input and
a peak RSS that scales 6.5× off Berlin's — no headroom on either axis. More
decisively, `AWS_S3_BUCKET_NAME` is not set there, so the result could not be
published and would have died with the container. The bake belongs on a box
with the bucket credentials and real memory.
