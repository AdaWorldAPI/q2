# Body SoA bake artifacts — stamp, don't clobber

The `/body` (BodyV3) and `/helix` (BodyHelix) cockpit viewers both consume the
BSO2 SoA wire. **A new bake must never delete or overwrite a working artifact** —
experiments that turn out bad must not take down the deployed viewer. Stamp every
build; keep the old ones.

## Naming

```
body.<YYYYMMDD>[-<n>].<fmt>.soa.gz        # stamped, immutable once written
body.<YYYYMMDD>[-<n>].<fmt>.blocks        # paired HHTL block bounds (cockpit-server /api/body/lod)
```

`<fmt>` records the wire format so two encodings on the same day stay distinct:

| `<fmt>` | meaning |
|---|---|
| `v5f16`   | ver-5 wire, F16 (IEEE half) positions — current `/body` production |
| `v5f16h2` | same, helix-normal tuned (2-byte refinement validated) — `/helix` target |
| `v3f32`   | ver-3 wire, raw f32 positions (legacy / debugging) |

`-<n>` is an optional same-day rebuild counter (`body.20260628-2.v5f16.soa.gz`).

## The two stable names are pointers, not bakes

- `body.soa.gz` — the artifact `/body` serves. It is a **copy of the current
  production stamp**, never a fresh bake written in place. Re-point it by copying
  a stamped file over it *after* the stamp is validated.
- `body.manifest.json` — served same-origin; the viewers read it to find the
  current stamps:

```json
{
  "body_latest":  "body.20260628.v5f16.soa.gz",
  "helix_latest": "body.20260628.v5f16h2.soa.gz",
  "builds": [
    { "stamp": "body.20260628.v5f16.soa.gz",   "ver": 5, "fmt": "v5f16",   "verts": 4221000, "concepts": 1658, "note": "production" },
    { "stamp": "body.20260628.v5f16h2.soa.gz", "ver": 5, "fmt": "v5f16h2", "verts": 4221000, "concepts": 1658, "note": "helix experiment" }
  ]
}
```

`BodyHelix` prefers `helix_latest` (then falls back to the shared `body.soa.gz`);
`BodyV3` reads `body.soa.gz` directly. A bad helix experiment is rolled back by
editing one line of the manifest — the production `/body` artifact is never touched.

## Producing a stamped bake

The bake binaries take the output name as `argv[2]`, so the stamp is the caller's
responsibility (`{out}.blocks` is derived automatically):

```sh
STAMP="body.$(date +%Y%m%d).v5f16"
./soabake fma_concepts.json "$STAMP.soa"      # writes $STAMP.soa + $STAMP.blocks
gzip -k "$STAMP.soa"                           # → $STAMP.soa.gz  (keep the raw .soa too)
# validate, then (and only then) promote:
cp "$STAMP.soa.gz" body.soa.gz                 # re-point /body, old stamps retained
```

Never `rm` a prior stamp. Disk is cheap; a black-screen deploy from a clobbered
artifact is not.

## `/helix` needs the canonical bake (`helixbake`), NOT the old artifact

The production `body.soa.gz` stores its per-vertex normal with the OLD ndarray
`helix_orient` codec (a place-blind 3-byte golden-spiral cascade). The canonical
`/helix` viewer decodes the **place-coupled `lance-graph::helix::Signed360`**
(6-byte: rim endpoint pair + signed polar lift + golden azimuth), so it CANNOT read
the old bytes — it would render garbage. `/helix` therefore reads only the stamped
canonical artifact named by `helix_latest`, and shows "no canonical helix bake yet"
until one is published.

Produce it with the **separate** bake crate `scratch-fma/helixbake` (soabake — the
`/body` bake — is left byte-identical; helixbake is its own crate so the old
pipeline never resolves helix):

```sh
cd scratch-fma/helixbake
STAMP="body.$(date +%Y%m%d).v6helix"
cargo run --release -- /path/to/soa "$STAMP.soa"   # writes $STAMP.soa (BSO2 ver 6) + .blocks
gzip -k "$STAMP.soa"                                 # → $STAMP.soa.gz
# then add to body.manifest.json:  "helix_latest": "$STAMP.soa.gz"
```

The normal is generated via `helix::ResidueEncoder::encode_signed(place, n, sign)`
— `place` = the concept's HHTL path, `n` = the nearest spherical-Fibonacci index of
the world normal, `sign` = its hemisphere. `cargo test` in that crate runs the
encode↔decode round-trip (the same decode BodyHelix.tsx uses) on synthetic normals,
no FMA data required.

(Build note: `helix` depends on `ndarray` via git; `helixbake/Cargo.toml` patches it
to the local `../../../ndarray` fork. A bake host that can't fetch the git source
relies on that patch; this sandbox's proxy blocks the fetch, so the crate is
validated by the round-trip test on a network-enabled host, not here.)
