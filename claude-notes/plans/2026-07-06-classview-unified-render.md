# ClassView unified render — one projection, many routes (2026-07-06)

## Overview

Operator-ratified unification: any app renders a node's detail via the same
`resolve → carve → emit` projection — `tool:partof:isa:render()` in rail
vocabulary. The is_a rail resolves which card (with fallback up the taxonomy),
the FieldMask carves which fields, the emitter is a thin per-target skin.
Routes differ per app; the classview+fieldmask core is ONE code path.

First consumer proof: the OSINT cockpit gets cheap server-rendered detail
popups — click a node in the SPA, fetch an askama fragment filled from the
node's ClassView fieldmask, values read from the 12-byte content-blind facet
(le-contract §3: "a dumb byte register the ClassView projects").

Two PRs, ordered (q2's Dockerfile clones lance-graph @ main HEAD, so the
contract lands first):

1. **lance-graph** `claude/classview-unified-render` — contract additions
   (additive, zero-dep): `ValueRow`, `ClassView::facet_rows(class, mask,
   &[u8;12])`, `is_a_parent()` + `resolve_render_class()` (depth-capped,
   cycle-safe zero-fallback walk: bespoke card → ancestor card → caller
   renders generic facet dump; never fails).
2. **q2** `claude/classview-unified-render` — cockpit-server card fragment +
   guid-valued rows + generic `/api/card.html?guid=` route beside the OSINT
   one ("different route, same classview fieldmask"), plus the React popup.

## Work Items

### Wave 1 (parallel)
- [ ] Contract: `ValueRow` + `facet_rows` + `is_a_parent`/`resolve_render_class`
      with tests, board hygiene in same commit (Opus, lance-graph)
- [ ] Recon: cockpit guid→node-bytes seam, React click path, card handler map
      (Opus, read-only)

### Wave 2 (after Wave 1)
- [ ] cockpit-server: `card_fragment.html` (bare, theme-inheriting) — existing
      full-page `osint_card.html` unchanged
- [ ] cockpit-server: `guid` param on card handlers → classid + facet lookup →
      `facet_rows` value column
- [ ] cockpit-server: generic `/api/card.html?guid=&mask=` route via
      `resolve_render_class` (any classid renders; unknown → generic facet dump)
- [ ] cockpit React: node click → fetch fragment → positioned popup div
      (SPA stays schema-blind; new classids need zero cockpit rebuild)

### Wave 3
- [ ] Central verify: cargo clippy/test both repos, React build, e2e curl of
      routes with a real baked guid, screenshot of the popup
- [ ] Review gate, push, PRs cross-linked, lance-graph merges first

## Details / decisions

- **ClassId is the canon-high u16 concept** (contract `class_view::ClassId`) —
  resolution by concept means apps sharing a concept share the card by prefix;
  the lo u16 app-prefix only picks a divergent skin when minted.
- **Positions ≥ 12 are value-slab fields** — out of facet scope, skipped by
  `facet_rows` (documented, mirroring the FieldMask 64-guard: never folded).
- **`is_a_parent` defaults to `None`** — the walk is inert until an implementor
  (OgarClassView) wires its taxonomy; generic route still renders everything
  via the fallback. OGAR wiring is follow-up, not this PR.
- **No behavior in cards** — ActionDef/KausalSpec stay on the Core node; a card
  may link an action, never carry one.
- **Popup fragments are cacheable** by (classid, mask, node version); fragment
  emits plain semantic HTML inheriting the cockpit cascade (theme-agnostic).
