# Region hot-plug — a map menu instead of a global env var

**Status:** design, not started. Written 2026-08-14 from the fuse→hotplug
migration (lance-graph #954) and the federation ruling in lance-graph #902.

## The ask

A dropdown of maps, like the helix menu: the menu enumerates what is available
from S3, selecting one sinks it into Lance, using `/volume01` when present and
the working directory otherwise — so a PR redeploy **reuses the warm `.lance`**
instead of re-fetching from S3.

## What already exists (measured, not assumed)

Most of the machinery is built. It is built **single-valued**.

| Piece | Where | State |
|---|---|---|
| S3 → local, checksum-gated | `osm_slab_hydrate::ensure_slab_local` | ✅ works |
| volume root with fallback | same (`OSM_SLAB_CACHE_DIR` / `RAILWAY_VOL` / `/volume01`) | ✅ works |
| sink into Lance | `osm_lance::ensure_lance_local` | ✅ works |
| **warm reuse across deploys** | `osm_lance::reopen_if_warm` — matches row count **and** slab digest | ✅ works |
| oversized-region fallback | the Arrow row ceiling guard (q2 #129) | ✅ works |
| **more than one region at a time** | — | ❌ **the gap** |

So *"reuse `.lance` from volume01 rather than redeploy from S3 each PR"* is
**already true** — for whichever single region `OSM_BAKE_REGION` names.

## The gap is the fuse shape, again

`OSM_BAKE_REGION` is **one global env var, resolved at boot, requiring a
restart**. Berlin and Brandenburg cannot coexist. That is structurally the
mistake `COUNT_FUSE` made in lance-graph:

> a **global** value where the need is **per-use**.

The menu is therefore not a UI feature bolted on top. It is the same inversion
the hot-plug migration applied to classids, applied to regions:

| | fuse shape | plug shape |
|---|---|---|
| classids | one mirror must equal the whole codebook | a consumer plugs the ids it uses |
| regions | one env var names the served region | a request names the region it wants |
| failure | breaks everything, including unrelated users | named, scoped to the requester |

And lance-graph #902's federation ruling already says this is the intended
model: *"there is not one bake. Several domain bakes coexist — each with its
own classid space, its own ClassView."* Regions are exactly that; nothing
applied it to them.

## Shape

**1. A manifest is the announce side.** S3 gains `q2/bakes/INDEX.json`, listing
each published region: prefix, rows, slab digest, byte sizes, bake date. It is
written by the publish step (`publish_bake.py`, which already computes every
one of those values) — never hand-maintained, or it becomes a mirror to drift.

**2. `GET /api/osm/regions` enumerates.** Reads the manifest, and for each
region reports whether it is `warm` (a matching `.lance` or `.soa` already on
the volume), `available` (in S3, not yet local), or `degraded` (past the
4,194,303-row Arrow ceiling → serves from the raw `.soa`, correct but slower).
The menu renders that; a region nobody selects costs nothing.

**3. Selection is per-request, not per-process.** Tiles take the region as a
parameter; the server holds a small map of open regions instead of one
`OnceLock`. Hydration for an unselected region never runs — the inert property
that makes plug-and-play work.

**4. Eviction is explicit.** The volume is finite (Brandenburg alone is
4.13 GB, and there is still **no free-space preflight** — see
`2026-08-13-brandenburg-bake.md`). Adding regions without an eviction policy
turns a menu into a disk-exhaustion bug. LRU by last-served, with a floor of
one region, is the obvious start; it must be decided before the menu ships,
not after.

## What this is NOT

- **Not new hydration machinery.** Steps 1–4 are plumbing around code that
  already works. Rewriting the hydrator would be the mistake.
- **Not a `.lance` format change.** Each region already lands as its own
  `<slab>.lance` beside its own slab; nothing about the layout is shared.
- **Not #902's `identity_quad`.** That is the cross-bake *join* (a shared
  identity slot + edge rows carrying full `(classid, identity)`). A map menu
  needs bakes to **coexist**, not to join. If cross-region routing is ever
  wanted, #902's federation section is the place to start — and note its own
  warning that the join-key slot position is *not yet pinned*.

## Open questions, in the order they block

1. **Eviction policy** — needed before shipping (see above).
2. **Does a region need its own classid?** Today every OSM row uses the canon's
   dormant `0x0000_0000`. Under the federation model each bake has its own
   classid space. Minting per-region classids is the "correct" answer and is
   also a bake-format change; serving several regions does not require it.
3. **Free-space preflight** — an unfixed gap that a menu makes materially worse,
   because the failure mode (truncated download → checksum reject → no file →
   silent retry loop) is invisible from outside.

## Cross-references

- The inversion this borrows: lance-graph **#954** + OGAR
  `docs/HOTPLUG-MIGRATION-GUIDE.md`
- The federation ruling: lance-graph **#902** body
- Volume-capacity risk + the Arrow ceiling:
  `claude-notes/plans/2026-08-13-brandenburg-bake.md`
- Publish tooling that would emit the manifest: `q2/bakes/tools/publish_bake.py`
  in the bake bucket
