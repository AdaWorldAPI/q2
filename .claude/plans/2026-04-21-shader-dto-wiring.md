# Plan: Wire q2 to cognitive-shader-driver DTOs (parallel integration path)

> **PHASE 1 STATUS: Shipped (commit 3d717b8) — see `2026-05-06-phase2-real-engine.md` for honest review.**
> Phase 1 wired the wire-format DTOs and SSE plumbing, but the engine path was
> theatre (hand-rolled cycles, hashed codebook indices, JSON-only conversion).
> Phase 2 replaces that with the real `thinking_engine::ThinkingEngine`.

> Date: 2026-04-21 (updated 2026-04-25)
> Branch: `claude/design-graph-notebook-frontend-Pwoqh`
> Constraint: semiring stays for external surface only — must NOT leak into StreamDto/BusDto path
> Constraint: NO serde on internal path — same binary, direct Rust calls
> Constraint: current stubs stay as fallback — LazyLock rendering alongside

## Architecture

```
INSIDE THE BINARY (no serde, no HTTP, direct Rust calls):
  ShaderDriver::dispatch() → 208ns/cycle → BusDto (owned struct)
  quarto_core::render_qmd_to_html() → PDF/HTML from graph runbooks
  aiwar_ingest::load_from_json() → graph hydration
  neural_debug::registry().diag() → health overlay

OUTSIDE THE BINARY (serde, HTTP, for external consumers only):
  /v1/shader/* → lab REST endpoints (bardioc R, curl, test harness)
  /mcp/* → MCP endpoints (legacy cockpit stubs)
  /v1/chat/completions → OpenAI-compatible (external LLM consumers)

COCKPIT (React/Vite, reads LazyLock double-buffer):
  JS stubs (seed.ts, aiwar-seed.ts) stay as immediate fallback
  Real data from LazyLock buffer when shader is alive
  Same UI components, two data sources, automatic fallback

BBB (Blood-Brain Barrier — async rate adaptation):
  External consumers (callcenter Supabase, bardioc R, MCP clients)
    → BBB accumulates via Markov chain bundling roles
    → StreamDto enters BindSpace when bundle capacity reached (N ≤ 32)
    → Shader cycles at 208ns, commits BusDto
    → BBB unbundles response back to external consumer clock
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

## Neo4j / Gotham Features (P1 — after core wiring)

### Neo4j Aura Fallback (`--features neo4j-fallback`)
- [ ] neo4j-rs stays as fallback for live demos when lance-graph data isn't loaded
- [ ] `neo4j-fallback` feature gate on cockpit-server — off by default
- [ ] When enabled: queries hit Neo4j Aura if lance-graph returns empty
- [ ] The planner still routes — neo4j-rs is a data source, not an engine
- [ ] Gotham investigation workspace: same cockpit panels, different data source

### Runbook → PDF Pipeline (quarto-core integration)
- [ ] Wire `quarto_core::pipeline::render_qmd_to_html()` directly (same binary, no process boundary)
- [ ] Notebook cells = runbook steps → pampa parses, quarto-core renders, deno_core executes
- [ ] PDF export via the full 9-stage pipeline (not the simplified publisher.rs shelling out to pandoc)
- [ ] Graph visualization in PDF: the vis-network canvas → SVG → embedded in quarto output
- [ ] Foundry ontology export: BindSpace → .qmd with typed objects as YAML metadata
- [ ] Gotham investigation report: selected subgraph → .qmd → PDF with NARS truth annotations

### Inside vs Outside BBB (Blood-Brain Barrier)

**Inside BBB** (same process, no serde, 208ns path):
- ShaderDriver::dispatch() — direct Rust call
- quarto_core::render_qmd_to_html() — direct Rust call
- neural_debug::registry().diag() — direct &'static reference
- LazyLock double-buffer — atomic swap, zero-copy read
- All cognitive DTOs (StreamDto, ResonanceDto, BusDto, ThoughtStruct) as owned structs
- The cockpit reads the LazyLock buffer — no HTTP round trip for data

**Outside BBB** (serde boundary, for external consumers):
- REST /v1/shader/* — lab endpoints, JSON over HTTP
- REST /v1/chat/completions — OpenAI-compatible, external LLM consumers
- MCP /mcp/* — legacy MCP protocol, JSON-RPC over SSE
- Supabase realtime — callcenter BBB, async Markov bundling
- R httr — bardioc bridge, JSON over HTTP
- Neo4j Aura — cold fallback, network hop

**The rule**: anything that crosses a process/network boundary uses serde.
Anything inside the binary uses owned Rust structs. The BBB is the explicit
boundary where async external input gets bundled into the internal clock.

### Lab Feature Gates (codebook generation from Qwen safetensors)

The cognitive-shader-driver already has rich lab endpoints behind `--features lab`:
- `/v1/shader/calibrate` — codec calibration against real token weights
- `/v1/shader/probe` — probe specific codebook entries
- `/v1/shader/tensors` — raw SoA column views
- `/v1/shader/sweep` — parameter sweep over codec candidates
- `/v1/shader/token-agreement` — measure token decode fidelity

These exist for measurement, not production. Two wiring options:

**Option A: Inside BBB (compile the lab into the q2 binary)**
- Add `cognitive-shader-driver` dep with `features = ["serve"]`
- Mount lab router: `.nest("/v1/shader", router())`
- Pro: one binary, no process boundary, researcher uses the same cockpit
- Con: lab code compiles even when not used, binary size grows

**Option B: Outside BBB (lab runs as separate shader-lab binary)**
- `cargo run -p cognitive-shader-driver --features lab --bin shader-serve`
- Cockpit proxies `/v1/shader/*` to `http://localhost:3001`
- Pro: q2 binary stays lean, lab only runs when researcher needs it
- Con: two processes, HTTP round trip, deployment complexity

**Decision**: Option A for Railway deployment (one binary, simple),
Option B for local dev when researcher needs the full lab. Feature-gated:
`--features lab` compiles the endpoints in, but they're not mounted
unless `LAB_ENABLED=true` env var is set at startup.

## LOC Budget

```
Phase 1: +1330 / -664  = +666 net
Phase 2: +800  / -334  = +466 net
Phase 3: +960  / -2221 = -1261 net
─────────────────────────────────
Total:   +3090 / -3219 = -129 net (subtraction)
```
