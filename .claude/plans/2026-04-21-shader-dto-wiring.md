# Plan: Wire q2 to cognitive-shader-driver DTOs (parallel integration path)

> Date: 2026-04-21
> Branch: `claude/design-graph-notebook-frontend-Pwoqh`
> Constraint: semiring stays for external surface only — must NOT leak into StreamDto/BusDto path

## Architecture

```
USER INPUT (human-timed)
  ↓
POST /v1/shader/ingest → StreamDto { source, codebook_indices, timestamp }
  ↓
SHADER LOOP (208ns/cycle, can't resist thinking)
  BindSpace encode (bind + braid + bundle) → decode (unbind + margin + F)
  if F > homeostasis_floor → cycle again
  ↓
SSE /v1/shader/stream → ResonanceDto (f32[4096] energy) | BusDto (committed thought) | ThoughtStruct (crystallized)
  ↓
COCKPIT (60fps, reads LazyLock double-buffer, pre-baked camera paths)
```

Semiring stays at:
- External MCP tool `graph_semiring` (notebook_server.rs line 746)
- Planner debug logging (notebook-query lib.rs line 159, 233)
- lance-graph-contract::nars::SemiringChoice enum (documentation value)
- blasgraph internal algebra (7 HDR semirings — their operations stay, trait dispatch optional)

Semiring does NOT touch:
- StreamDto fields
- ResonanceDto energy field
- BusDto committed result
- ThoughtStruct crystallized output
- Any SSE event shape
- Any cockpit panel data

## Phase 1 — The Show (watch the stack reason)

### Server side (lance-graph crates/cognitive-shader-driver/)

- [ ] `src/stream.rs` — SSE endpoint `GET /v1/shader/stream`
  - Event types: `stream | resonance | bus | thought | scene_begin | scene_end`
  - No semiring field in any event
  - Uses tokio broadcast channel, same pattern as existing `/mcp/sse`
- [ ] `src/scene_player.rs` — 30 Cypher file scene player
  - Loads encounter rounds via aiwar-ingest::load_encounter_rounds()
  - Each round → StreamDto { source: AriGraph, codebook_indices: [...], timestamp }
  - Feeds to shader.cycle(), emits events per cycle
  - Scene transitions: F drops below homeostasis OR 10s elapsed
- [ ] `src/lazy_render.rs` — LazyLock double-buffer for pre-rendered frames
  - 500ms tick, atomic swap, same pattern as existing /mri pre-render
  - Reader (cockpit) never waits on writer (shader)

### Server side (q2 crates/cockpit-server/)

- [ ] Add `cognitive-shader-driver` dep with `features = ["serve"]`
- [ ] Mount shader router: `.nest("/v1/shader", cognitive_shader_driver::lab::router())`
- [ ] Keep existing /mcp/* endpoints (parallel operation, not replacement)
- [ ] Keep existing stubs at their current routes (outage fallback)

### Cockpit side (q2 cockpit/src/)

- [ ] `transport.ts` — add `subscribeShaderStream()` SSE client
  - Connects to `/v1/shader/stream`
  - Dispatches events to zustand store
  - No semiring field parsed
- [ ] `components/EnergyField.tsx` — Canvas 2D spectrogram
  - Reads ResonanceDto.energy (f32[4096]) per cycle
  - 4096 columns × 200 time slices, scrolling left
  - Color temperature = activation intensity
- [ ] `components/BusTicker.tsx` — committed thought ticker
  - Scrolling list of BusDto entries
  - Shows codebook_index, energy, cycle_count, converged
- [ ] `components/ThoughtLog.tsx` — ThoughtStruct history
  - Committed thoughts with provenance chain
  - Revision tracking when later scenes override earlier ones
- [ ] `components/SceneBreadcrumb.tsx` — scene navigation
  - Shows v0 → v1 → ... → v43 progression
  - Highlights current scene
- [ ] `components/FreeEnergyDial.tsx` — the only number that matters
  - F high = field active, F low = shader at rest
- [ ] `pages/ReasoningDemo.tsx` — compose all above at `/reasoning` route
  - Three layers: energy field (center), brain overlay (Three.js), bus ticker (right)
  - Play/Pause/Step controls
  - Pre-baked camera orbit from existing orbit.ts
- [ ] `main.tsx` — add `/reasoning` route

### Acceptance

- Visit `/reasoning`, click Play
- 30 scenes run start-to-finish (~4 min)
- No frame drops on 2020 MacBook
- SSE disconnect → field persists from last frame, reconnects
- No semiring field anywhere in the event stream

## Phase 2 — The Audit (cognitive debugging IS the watch)

- [ ] Diagnostic event type added to SSE stream: `diagnostic`
  - Shows strategies_fired, strategies_dormant, neurons_touched, nans, pipeline_breaks
  - Sources from `GET /v1/shader/health` (neural-debug overlay already built in)
- [ ] `components/StrategyMatrix.tsx` — 16 rows, live activation bars
- [ ] `components/PipelineChecks.tsx` — chain visualization (Parse→Scan→Collapse etc.)
- [ ] `components/NeuronOverlay.tsx` — function-level dots on the brain
- [ ] EnergyField gains vertical strategy boundary bands
- [ ] BusTicker gains NaN alert lane
- [ ] ThoughtLog gains dependency chain expansion
- [ ] `[D]` toggle on ReasoningDemo.tsx

### Acceptance

- Toggle [D] during any scene
- See which strategies fire, which neurons activate, which pipelines break
- 7 dead strategies + 3 NaN producers visible within 10s

## Phase 3 — The Steering (thinking buttons dispatch)

- [ ] Style buttons become live dispatch: WireStyleSelector::Ordinal(n) → shader.cycle()
  - Direct index into 34 primitive compositions, zero trait dispatch
  - No semiring selection in the dispatch path
- [ ] Compare mode: shift-click two styles → split field view
- [ ] 36-brain superposition: [S] key → 36 parallel dispatches, additive energy merge
- [ ] `components/StyleSelector.tsx` — 36 buttons with cluster colors
- [ ] `components/SuperpositionView.tsx` — consensus/fault-line/blind-spot map
- [ ] `components/CompareView.tsx` — side-by-side field comparison
- [ ] DELETE stubs (replaced by shader dispatch):
  - orchestrator.rs (1563 LOC)
  - graph-flow stub (171 LOC)
  - analyst.rs (371 LOC) — buckets become WireStyleSelector::Named dispatches
  - thinking.rs (116 LOC)
  - diagnostics.rs (334 LOC) — replaced by /v1/shader/health

### Acceptance

- Click any of 36 style buttons → field reshapes within one cycle (<500ms)
- Shift-click two → side-by-side compare
- [S] → 36-brain superposition in <2s
- Stubs deleted, net LOC reduction

## LOC Budget

```
Phase 1: +1330 / -664  = +666 net
Phase 2: +800  / -334  = +466 net
Phase 3: +960  / -2221 = -1261 net
─────────────────────────────────
Total:   +3090 / -3219 = -129 net (subtraction)
```
