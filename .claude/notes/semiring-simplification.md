# Semiring Simplification — Future Session Note

> Status: DOCUMENTED (do not delete semiring code — document only)
> Date: 2026-04-21
> Context: Session identified that semiring traits add overhead when everything is Rust in one binary

## Finding

The semiring abstraction (7 HDR semirings in blasgraph, 5 contract semiring choices,
semiring_selection.rs in the planner) exists for pluggable algebra across different
traversal patterns. In a single Rust binary where all composition is direct function
calls, the trait boundary doesn't earn its cost.

The DTOs (StreamDto → ResonanceDto → BusDto → ThoughtStruct) compose without semiring
selection. The 34 cognitive primitives in ndarray::hpc::styles each have concrete
signatures `fn(Base17, NarsTruth) → result`. Compositions are direct calls.

## What to simplify (future session)

1. `lance-graph-planner::thinking::semiring_selection.rs` — the selector can become
   a direct match on thinking style → concrete fn, no trait dispatch
2. `lance-graph-contract::nars::SemiringChoice` — 5 variants that map to 5 concrete
   fns. The enum stays (documentation value), but no trait boundary needed
3. The 7 HDR semirings in blasgraph — they're already concrete impls behind a trait.
   The trait can be removed, leaving the impls as named fns
4. `notebook_server.rs` `graph_semiring` MCP tool — external endpoint, low priority,
   keep as UI convenience

## What NOT to change

- Do NOT delete semiring code
- Do NOT remove the SemiringChoice enum (it documents the options)
- Do NOT change the external MCP `graph_semiring` tool
- The operations (XOR bundle, HammingMin, SimilarityMax, etc.) all stay — they're real
  algebraic operations. Only the trait dispatch layer is unnecessary.

## P0 check result

No internal DTO routing uses semiring selection. The DTOs flow directly.
Semiring appears only in:
- External MCP tool (`graph_semiring` endpoint in notebook_server.rs)
- Debug metadata logging in planner
- Documentation

**No P0 violation found.**
