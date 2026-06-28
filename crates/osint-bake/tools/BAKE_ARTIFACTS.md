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

## Note: `/helix` needs no rebake today

The current production `body.soa.gz` already carries the helix-normal bytes
(`pos3|nrm3` per vertex); `/body` simply skips them. `/helix` reads them as-is, so
the experimental viewer runs against the existing artifact with **zero rebake**. A
stamped `v5f16h2` build is only needed if the helix encoding itself is retuned.
