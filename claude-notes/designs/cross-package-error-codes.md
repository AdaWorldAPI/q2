# Error-code identity across package boundaries

**Status:** Design philosophy (proposed)
**Driver:** bd-egcyeym9 (extracting `quarto-yaml-validation`), but deliberately
written to be general — it governs *any* error that is **defined in one package
and surfaced by another product**.
**Related:** `claude-notes/plans/2026-06-26-extract-quarto-yaml-validation-design.md`

## The problem, stated generally

Quarto 2 assigns every user-facing error a code in a single flat, navigable space
— `Q-<subsystem>-<n>` — with one curated docs page per code at
`quarto.org/docs/errors/...`. Two goals are now in tension:

1. **Quarto-2 users** should see *one* code space that is unique and easy to
   navigate. They must **not** have to know how Quarto is internally decomposed
   into packages — which crate emitted an error is an implementation detail of
   *who builds Quarto*, not of *how Quarto reports errors*.
2. **Independently-developed packages** (e.g. a standalone `quarto-yaml-schema`
   that non-Quarto developers also depend on) should own **their own** error
   codes, meaningful to *their* users, with no knowledge of Quarto's `Q-*` scheme.

These reconcile only if a code has **two identities** and the *product*, not the
*package*, owns the bridge between them.

## What the inspirations actually teach

The Quarto error-code scheme was inspired by the **TypeScript compiler** (numeric
`TSxxxx` diagnostics in a single `diagnosticMessages.json`). That model is an
excellent template for the *presentation* layer — flat, dense, centrally curated,
one docs page per code — **but it offers nothing for the cross-package case**: the
compiler is a monolith; all diagnostics are administered in one file by one team.
A flat numeric space cannot be minted by independently-developed packages without
a central allocator they all coordinate with — which is exactly the coupling we
are trying to remove.

The precedents that *do* solve the cross-package case use **namespaced** codes —
and, crucially, they show what to borrow *and* what to add.

### How Clippy does it (alongside rustc)

- rustc owns a flat **numeric central** registry (`E0277`) with a central error
  index (`doc.rust-lang.org/error_codes/E0277.html`) — the TS-compiler model.
- Clippy, developed independently, does **not** mint `E`-codes. It uses **named,
  namespaced** lints: `clippy::needless_return`. The `clippy::` prefix is a
  **registered tool namespace** — rustc has a first-class notion of "tool lints"
  (`register_tool`), so the host's attribute/level machinery
  (`#[allow(clippy::…)]`, allow/warn/deny) can refer to a subsystem's diagnostics
  *generically*, without rustc knowing any specific Clippy lint.
- Clippy ships its **own** catalog (categories: correctness/style/perf/pedantic/…)
  and its **own** docs site (`rust-lang.github.io/rust-clippy`).
- **Lesson:** the host provides shared *infrastructure* (the lint store, level
  machinery, the renderer) but **not the identity namespace**. Uniqueness comes
  from the `tool::name` structure, not a central allocator. This is exactly the
  role split we want: `quarto-error-reporting` = the lint-store/renderer analog;
  each library = a "tool" that owns its names.

### How ESLint does it (core + plugins)

- Core rules are bare (`no-unused-vars`); plugin rules are **namespaced by
  package**: `@typescript-eslint/no-unused-vars`, `import/no-cycle`. The namespace
  *is* the package identity (the npm package, minus the `eslint-plugin-`
  convention).
- ESLint has a **self-description protocol**: each rule object carries
  `meta.docs.url`. The plugin declares *where its own docs live*, and
  editors/formatters surface that URL. The host does not own the docs; the rule
  points at them.
- **Lesson:** package-prefix namespacing for composability **+ a per-code docs URL
  the library owns**. This is precisely the pluggable-docs-URL mechanism our
  `CatalogProvider` needs.

### The thing neither does — and why Q2 must

ESLint surfaces `@typescript-eslint/…` *to the user*; Clippy surfaces `clippy::…`
*to the user*. They **expose** the package decomposition because their users
intentionally assemble toolchains — they are *platforms*. Quarto 2 is a *product*,
and its principle is the opposite: **a Q2 user must never have to follow a chain of
library dependencies to explain an error.** So Q2 must add the one thing the
platforms omit — a **product-owned remap** to a single `Q-*` namespace.

**Takeaway:** borrow the *library-side discipline* from Clippy/ESLint (namespaced,
package-owned codes that self-describe their docs) and the *presentation layer*
from the TS compiler (flat, navigable, central). Add the piece neither has: a
**product-owned remap** that bridges them. The two layers meet only through that
remap.

## The two-identity model

Every error carries:

### 1. An **origin code** — owned by the package that *defines* the error
- **Namespaced by package:** `<package-ns>/<slug>`, e.g.
  `yaml-schema/type-mismatch`. Namespacing is mandatory and structural; it is what
  makes codes from independently-developed packages composable with **no central
  registry**.
- **Stable for the package's own users:** adding origin codes is non-breaking;
  removing or repurposing one is a breaking change that bumps the package version.
- The package may ship its **own** catalog (titles, docs, since-version) for its
  own direct users, or ship none and let every embedder supply its own.

### 2. A **presentation code** — owned by the *product* that surfaces the error
- Quarto 2's `Q-<subsystem>-<n>`: flat, navigable, centrally curated, one docs
  page per code. **Unchanged from today** — this is the contract Q2 users rely on.
- The product owns a **remap**: `origin code → presentation code`. This is the
  *only* place the two namespaces meet.
- Q2 users see **only** presentation codes and navigate quarto.org. They never
  learn that `Q-1-11` was *defined* in an external `yaml-schema` crate.

## The fallback hierarchy (and the contract it implies)

When a product surfaces an error that a library defined, three outcomes are
possible, in descending order of quality:

1. **Best — remapped.** The product has a presentation code for this origin code;
   the user sees `Q-1-11` and navigates quarto.org. The package decomposition is
   invisible.
2. **Acceptable — passthrough.** No presentation code, but the library's **own
   stable origin code** shows through (`yaml-schema/type-mismatch`, library docs).
   The user has *a* stable handle to search/report — strictly better than nothing.
3. **Forbidden — codeless.** A library error with no stable code at all. The user
   has nothing durable to navigate.

The discipline that **guarantees tier 3 never happens**: every diagnostic a
participating library emits MUST carry a stable, namespaced origin code. Then the
worst case is always tier 2, and the embedder's remap is a pure *upgrade*
(tier 2 → tier 1) — optional, per-code, and never load-bearing for correctness.

This is why the audit only *encourages* full remap coverage rather than *failing*
on an unmapped code (it refines invariant I3): an unmapped origin code is
acceptable, not broken.

## Roles are per-node, not per-package-type

"Library" and "product/embedder" are **not** two kinds of crate — they are two
*roles a single crate can play at once*. Every node that uses the reporting crate
is simultaneously:

- a **definer** — it mints its own *terminal* codes (errors it originates), and
- optionally a **remapper** — it relabels codes surfaced from its dependencies
  under its own scheme.

Quarto 2 is not a special "product" role; it is simply the **terminal remapper** in
a chain — the node whose presentation codes happen to be user-facing. A mid-chain
library that wraps `quarto-yaml-validation` and re-exposes some of its errors under
its own codes performs the *exact same* operation Q2 does. The chain always bottoms
out at **terminal** codes, and the library contract below ("every emittable error
carries a stable code") is what **guarantees every chain terminates**.

## Terminal vs remapped: the developer-provenance lane

A diagnostic's code is one of two kinds, and the crate exposes the distinction:

- **Terminal** — a code this node *originates*. The definition lives here.
- **Remapped** — a code this node presents in place of a code surfaced from a
  *dependency*. This node is relabelling someone else's terminal error.

The remapped/terminal distinction is **not for end users** (a Q2 user wants only
`Q-1-11`); it is a **developer-facing provenance breadcrumb** — it lets a
developer or a bug report trace where an error *ultimately* originates, and where
to go to fix it. It rides in structured/JSON output, never in human error text.

Three rules keep this cheap and decoupled:

1. **Provenance is inert data, never a typed edge.** A remap discloses its upstream
   as a *string code + optional source URL* — e.g.
   `{ code: "yaml-schema/type-mismatch", source: "https://github.com/posit-dev/…" }`
   — and **must not** depend on, or `use`, the upstream crate's error types. A
   typed provenance edge would silently rebuild the compile-time coupling the whole
   extraction exists to remove. Stringly-typed *on purpose*.
2. **Disclose the *immediate* upstream, not a resolved ultimate.** "Terminal" is
   only well-defined transitively: a node can truthfully describe the dependency it
   *directly* depends on (it can read that dependency's catalog), but cannot
   reliably assert the chain's ultimate base — if a mid-chain node later re-bases
   its own code, a hard-coded "ultimate" disclosure goes silently stale and
   **nothing re-resolves it** (we deliberately do not traverse — see rule 3). So
   each remap discloses its immediate upstream code + optional URL, and each node
   carries a self-declared `terminal: bool`. A developer reconstructs a deep chain
   by walking immediate disclosures hop-by-hop until a `terminal` node. **In the
   common 1-hop case (Q2 ← `yaml-schema`), immediate *is* terminal**, so this costs
   nothing today; it only keeps rare deep chains honest.
3. **No automated cross-repo traversal, and no resolved-ultimate pointer.**
   Disclosure is a **best-effort breadcrumb, not a guarantee**: there is no
   build-time check across repos (you do not have the upstream repo when you
   compile), so a disclosed code/URL can rot. That is accepted. Optionally pin a
   version or commit-ish in the URL when reproducibility matters; never mandate it.
   Intra-workspace a lint *could* check disclosures against local crates;
   cross-repo it cannot, and the discipline must not pretend otherwise.

> **Design-for-1-hop.** Permit chaining so the model is never *wrong*, but optimize
> ergonomics for a single hop and build **zero** chain-resolution machinery.

## The three contracts

The design is a division of responsibility across three roles. A single crate may
play the first two at once (see "Roles are per-node"). Spelling them out *is* the
spec a library author follows.

### Library-author contract (e.g. `quarto-yaml-validation`)
- **Reserve a namespace** (`<ns>/…`, e.g. `yaml-schema`) and mint all codes under
  it. Never emit another party's codes (no `Q-*` in the library — the Clippy rule:
  Clippy never emits `E`-codes).
- **Every emittable diagnostic carries a code.** No anonymous errors (guarantees
  tier 2 is always available).
- **Codes are stable and append-only:** additive evolution is non-breaking; a code
  is *retired* (stops being emitted) rather than deleted, stays documented forever,
  and is **never** repurposed to a new meaning (see "Codes are append-only").
- **Self-describe:** ship a `CatalogProvider` mapping each code → at least a title,
  optionally a docs URL on the library's own site (the ESLint `meta.docs.url`
  lesson). A library may also ship *no* catalog and let embedders supply
  everything.

### Embedder/remapper contract (any node that relabels a dependency's codes — Q2, `n2`, or a mid-chain library)
- **Own a remap** `immediate-origin → presentation` for the codes it chooses to
  elevate to tier 1, plus a catalog for its own presentation codes.
- **Disclose provenance** on each remapped code: the *immediate* upstream code +
  optional source URL, as inert data (see "Terminal vs remapped"). Mark the code
  `remapped`, not `terminal`.
- **Choose the unmapped policy:** Q2 lets the origin code pass through (tier 2) and
  audit-*warns* to encourage coverage. A different node could hide unmapped errors,
  or fail its build — its call.
- **Never leak its own scheme upstream:** the remap lives at this node's reporting
  boundary, not in the dependency.

### Shared-infrastructure contract (`quarto-error-reporting` — the reusable host crate; keeps its name when it moves to `posit-dev/`, decided 2026-06-27)
- **Namespace-agnostic:** carries whatever code string it is handed; knows neither
  `yaml-schema/*` nor `Q-*`.
- **Provides the seams:** the `CatalogProvider` trait (title/docs lookup) and the
  remap hook applied before render; the diagnostic type, builder, and renderer.
- **Carries the provenance metadata:** the diagnostic model exposes `terminal` vs
  `remapped` and an optional inert `{ code, source_url? }` provenance, so any
  embedder's wire format can serialize it. (The *concept* lives here even though
  Q2's specific JSON wire shape stays q2-side.)
- This is the rustc-`LintStore` analog: it supplies the machinery and the
  rendering, and owns **no** identity policy.

## Multiple embedders (the `n2` case)

Consider `n2`, a different product that also depends on `quarto-yaml-validation`
and also does not want its users to see `yaml-schema/*` codes. Nothing special is
needed: `n2` supplies **its own** remap + catalog over the same
`quarto-error-reporting`. The remap is **not a Q2 feature** — it is a per-embedder
facility the shared crate provides. Same infrastructure, different policy tables.
This is the proof that the design is embedder-agnostic, and the reason the remap
hook must live in `quarto-error-reporting`, not in any Quarto-specific crate.

## Invariants (what makes this sound)

- **I1 — Subsystem ≠ package.** The `<subsystem>` in `Q-<subsystem>-<n>` is a
  *product taxonomy of meaning*, not a map of the crate graph. The product may
  route two packages into one subsystem, split one package across subsystems, or
  renumber subsystems — all without touching any package. A package must never
  assume its errors land in a particular subsystem. **This is the firewall** that
  lets internal package decomposition change freely without disturbing the
  user-facing code space. (Directly answers the user's requirement: Q2 users
  don't see, and aren't affected by, the package decomposition.)

- **I2 — Origin collisions are structurally impossible.** Because every origin
  code carries its package namespace, two independently-developed packages can
  never mint the same origin code. No central allocator. (This is precisely the
  gap in the flat-numeric model.)

- **I3 — The remap is product-owned and *partial-by-design*.** The product — not
  the package — decides *which* origin errors it elevates to a presentation code
  and *how* it groups them. A mapped code yields tier 1; an **unmapped** code falls
  back to tier 2 (the library's own stable origin code passes through). So the
  remap need not be total: Quarto 2 lets unmapped codes pass through and the audit
  *warns* (not fails) to encourage coverage. (This refines the earlier
  "unmapped = build failure" stance: the fallback hierarchy makes unmapped
  *acceptable*, not broken.)

- **I4 — Presentation uniqueness is the product's existing guarantee, untouched.**
  Presentation codes live entirely in Q2's flat space, so the *existing* `Q-*`
  audit's uniqueness/coverage guarantees carry over unchanged. The remap adds
  **one** new check (totality over surfaced origin codes), not a rework.

- **I5 — Provenance survives but does not navigate.** The origin code travels in
  structured/JSON output and verbose diagnostics as *provenance* (a bug report can
  say "`yaml-schema/type-mismatch`, surfaced as `Q-1-11`"), but the *navigational
  handle* a user follows is always the presentation code. This satisfies "users
  shouldn't have to know the package decomposition" while keeping debuggability.

## Mechanism (how it maps onto the crates)

- The package emits diagnostics tagged with **origin codes**
  (`ValidationErrorKind::code() -> "yaml-schema/..."`).
- The reusable reporting core (`quarto-error-reporting`) is namespace-agnostic: it
  carries whatever code string it is handed and renders it, consulting an installed
  `CatalogProvider` for title/docs. It knows about neither `yaml-schema/*` nor
  `Q-*`.
- The **product** installs two things at startup:
  - a **`RemapTable`** (`origin → presentation`), applied at the reporting
    boundary *before* render, so the diagnostic's primary code becomes `Q-1-11`
    and the origin code is retained as provenance (I5);
  - a **`CatalogProvider`** over `error_catalog.json`, keyed by presentation codes.
- A non-Quarto embedder installs neither (or its own): origin codes render as-is,
  with the embedder's catalog or none.

```
package (yaml-schema)        product (Quarto 2)                user
─────────────────────        ──────────────────────────       ──────────────
ValidationErrorKind          RemapTable:                       sees: Q-1-11
  .code()                      yaml-schema/type-mismatch        docs: quarto.org/
  = "yaml-schema/                 → Q-1-11                            docs/errors/
     type-mismatch"          CatalogProvider:                        yaml/Q-1-11
                               Q-1-11 → {title, docs_url}        (origin code only
(also usable standalone,     quarto-error-reporting renders        in JSON/verbose
 with the package's own        with Q-1-11 primary,              as provenance)
 catalog or none)              yaml-schema/… as provenance
```

## Rejected alternatives

- **Flat shared numeric space across packages (literal TS-compiler model).**
  Needs a central allocator every package coordinates with; defeats independent
  development. Rejected.
- **Package emits product codes directly (status quo: `error_code() -> "Q-1-11"`).**
  Bakes product policy into the package; the package can't ship to non-Quarto
  users; Q2's subsystem taxonomy leaks across the boundary. This is exactly what
  we are undoing. Rejected.
- **No product codes; show origin codes to users.** Q2 users would see
  `yaml-schema/type-mismatch` next to `Q-2-5` — an inconsistent, un-navigable
  space, and precisely the "must know the package decomposition" we are avoiding.
  Rejected.

## Codes are append-only ("cool URLs for error codes")

Error codes are **unique and, in principle, never deleted or repurposed** — the
"cool URLs don't change" covenant (Berners-Lee, 1998) applied to error
identifiers. A docs page accumulates codes the current version no longer emits,
**and that is fine**: someone is running an old version, or online content
(issues, Stack Overflow, bookmarks) references the code. A frozen, resolvable code
is the whole point.

This also *protects the provenance breadcrumb* (rules under "Terminal vs
remapped"): if codes are never deleted or redefined, a disclosed upstream
code/URL degrades gracefully — at worst it resolves to a *retired-but-documented*
code, never a 404 and never a silently-different meaning.

**Three lifecycle states, and which transitions are legal:**

- **Active** — emitted by the current version.
- **Retired** — no longer emitted, but still documented (old versions / external
  references still resolve).
- **Never** — never existed.

Legal: `Active → Retired`. **Forbidden: `Active → Never` (deletion) and
`code → different meaning` (repurposing).** Meaning is frozen at allocation,
forever. So a library *retires* (stops emitting) a code rather than deleting it,
and **never** reuses a code string for a new meaning — this is stronger than "major
version bump"; repurposing is simply off the table.

**The freeze binds at first *public* exposure, not first commit.** A code added on
a dev branch and never shipped — never emitted in a release, never publicly
documented — may still be renumbered or dropped (semver pre-1.0 logic). The
covenant starts at first public *emission or documentation*, whichever comes first.
This keeps ordinary development unconstrained.

**Encourage cross-repo; enforce intra-repo.** Append-only is *checkable within a
repo* (diff the catalog against git history or a committed snapshot — no entry
removed, no meaning changed) and should be a CI lint there. It is *not* checkable
across repos (you do not have the upstream repo at build time), so for a
dependency's catalog this is documented expectation, self-enforced by that library
in its own CI — the same intra/cross asymmetry as provenance staleness.

> **Audit consequence (concrete).** Q2's existing bidirectional
> `scripts/audit-error-codes.py` must change: **keep** "every *emitted* code is
> documented" (forward), **drop** "every *documented* code is emitted" (reverse —
> it contradicts retirement), and **add** an append-only check. A retired or
> dormant code is a legitimate catalog-only entry. `ErrorCodeInfo.since_version`
> already records the "introduced" end; an optional `retired_in`/`last_emitted`
> records the other, so a docs page can show the emitting window.

## Governance

- Each package documents its origin-code namespace and stability promise in its
  own repo.
- Quarto 2 owns `error_catalog.json` + the remap table + the audit. Surfacing a new
  external error = add origin code (upstream) **+** add remap entry **+** add
  catalog entry (q2); the audit enforces the three-way join.
- **Versioning:** upstream adding an origin code is non-breaking; Q2 surfaces it on
  its own schedule by adding remap+catalog entries. Upstream *retiring* a code
  (ceasing to emit it) keeps the code documented (append-only — see "Codes are
  append-only"); upstream *repurposing* a code is forbidden, not merely a major
  bump. Q2's remap pins the upstream version it maps so its provenance disclosure
  stays accurate.

## Open questions

- **Origin-code shape:** namespaced slugs (`yaml-schema/type-mismatch`,
  recommended — human-readable, obviously package-owned) vs. namespaced numerics
  (`yaml-schema:0001`). Slugs are recommended; the only argument for numerics is
  parity with the TS-compiler aesthetic, which does not compose and is not needed
  at the package layer.
- **Provenance surfacing:** confirm I5's "JSON/verbose only" placement vs. always
  showing origin code in parentheses. (Recommendation: structured/verbose only.)
  Mechanism decided — see "Terminal vs remapped": inert `{ code, source_url? }`,
  immediate upstream only, self-declared `terminal` flag, no chain resolution.
- **`terminal`/`remapped` exposure:** decided to expose it as developer-facing
  provenance. Remaining nit: whether `terminal` is a boolean flag or implied by the
  *absence* of provenance (a code with no upstream disclosure is terminal). The
  latter is more economical; the former is more explicit. (Lean: implied-by-absence,
  with an explicit flag only if a node needs to assert "terminal here" while also
  carrying unrelated metadata.)
- **Multi-product future:** resolved — see "Multiple embedders (the `n2` case)".
  Captured here only as a reminder that the remap hook must live in
  `quarto-error-reporting`, never in a Quarto-specific crate.

## Sequencing (decided 2026-06-26)

This discipline is the *host* contract, so it is designed and proven **before**
its first client:

1. Extract the **diagnostics foundation** first — `quarto-source-map` (leaf) +
   `quarto-error-reporting` (carrying the three contracts above: `CatalogProvider`,
   remap hook, namespace-agnostic rendering) — into a standalone repo under the
   **`posit-dev/`** GitHub org. Publish to crates.io; validate standalone.
2. Only then extract **`quarto-yaml` + `quarto-yaml-validation`** as the **first
   client** of the discipline (it adopts a `yaml-schema/*` origin namespace).
3. Only then **migrate q2** to consume the published crates (q2 keeps its in-tree
   copies until the external ones are proven). The motivating external consumers
   are invisible internal Posit users of `quarto-yaml-validation`.

The yaml-extraction mechanics live in
`claude-notes/plans/2026-06-26-extract-quarto-yaml-validation-design.md`; the
error-reporting extraction (step 1) warrants its own plan doc once this
discipline is accepted.
