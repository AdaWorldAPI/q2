# Chess-NARS Vertical Slice — First Consumer of the lance-graph 4-Pillar Contract

> **Cross-reference:** `.claude/knowledge/session-capstone-2026-04-18.md` in
> `AdaWorldAPI/lance-graph`. That doc explains the contract pillars and
> loose ends. This doc plans the first end-to-end demo that exercises them.

---

## 1. Why Chess, Why Now

The 4-pillar agent contract (NARS + thinking styles + qualia + proprioception)
just landed in `lance-graph-contract`. It compiles, tests pass, DTOs are
clean. What's missing is **a real consumer under real load**.

Chess is the ideal first domain because:

- **Ground truth.** Engine evaluation is objective. Every `ProprioceptionAxes`
  reading and every NARS inference can be validated against "was that move
  actually good" — we know if we lied to ourselves.
- **Bounded.** Deterministic state, finite move set, clear terminal conditions.
  No ambiguity when testing the pipeline.
- **Clean SPO.** Pieces are subjects, verbs are moves, squares are objects.
  Positions are states. Games are sequences of edges. Maps onto
  `lance-graph/graph/spo/` directly.
- **Every pillar fires.** NARS infers, thinking styles pick opening vs endgame
  mode, qualia surfaces as positional feel, proprioception classifies the
  engine's own state, world_model carries opponent inference (empathy as
  theory-of-mind), blackboard lets multiple analyst agents argue about a
  position.
- **Transfer.** Every piece of this architecture maps 1:1 to airwar.cloud
  OSINT: actor ↔ piece, event ↔ move, geopolitical state ↔ position,
  scenario ↔ game. The contract doesn't change when the domain does.

---

## 2. Architecture

```
[lichess API / chess.com API]
       │ PGN stream, FEN positions, engine eval
       ▼
┌──────────────────────────────────────────────────────────────┐
│  chess-ingest  (new crate in q2)                             │
│    PGN → SPO triples                                         │
│      (Piece, moves_to, Square)                               │
│      (White, threatens, Black_Pawn_e5)                       │
│      (Position_A, leads_to, Position_B) + NARS truth         │
│    Position FEN → cycle_fingerprint (content-addressable)    │
└─────────────────────────┬────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────────┐
│  lance-graph-contract + lance-graph                          │
│    BindSpace row per position                                │
│      content_fp     = FEN hash                               │
│      cycle_fp       = engine's analysis signature            │
│      edge           = CausalEdge64 (move + NARS f/c)         │
│      qualia         = QualiaVector (17D observation)         │
│      axes           = ProprioceptionAxes (11D state)         │
│      proprioception = StateReport (engine's current mode)    │
│    Cypher:                                                   │
│      MATCH (p:Pos)-[m:Move]->(q:Pos)                         │
│      WHERE q.eval > 0.9 AND m.nars_confidence > 0.7          │
│      RETURN p, m, q                                          │
└─────────────────────────┬────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────────┐
│  cognitive-shader-driver  (per-move cycle)                   │
│    ShaderDispatch (row window = candidate moves)             │
│    CognitiveShader::cascade over position plane              │
│    Emits:                                                    │
│      cycle_fingerprint = analysis signature                  │
│      CausalEdge64 per move (f,c,plane)                       │
│      GateDecision (Flow=play, Hold=ponder, Block=blunder?)   │
└─────────────────────────┬────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────────┐
│  cockpit-server (already deployed at cubus.up.railway.app)   │
│    /api/mri/scan    → WorldModelDto as JSON                  │
│    /api/debug/osint → NARS inference chain                   │
│    /mri             → 500ms refresh live view                │
│    /debug           → neural-debug strategy health           │
└─────────────────────────┬────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────────┐
│  React 3D cockpit (new component)                            │
│    - Board as plane (three.js, react-three-fiber)            │
│    - Pieces as nodes                                         │
│    - Moves as directed edges (arrow thickness = NARS conf)   │
│    - ProprioceptionAxes as volumetric cloud above the board  │
│        (11 colored fog layers, each axis = layer)            │
│    - Timeline axis for move history                          │
│    - Forks branch at each decision point                     │
│    - "Positions that felt similar" = Hamming sweep over      │
│        cycle_fingerprint columns in BindSpace                │
│    - Gotham-style object explorer in sidebar                 │
└──────────────────────────────────────────────────────────────┘
```

---

## 3. Tier 0 — Minimum Viable Vertical (3-5 days)

### T0.1 — Pin q2 to lance-graph post-capstone

Edit `q2/Cargo.toml` (workspace) so `lance-graph-contract` points at the
main commit that includes `proprioception`, `qualia`, `world_map`, and the
extended `world_model`. Currently the cockpit build uses `engine: "lance-graph
(pending)"`; this pin replaces the stub.

**Acceptance:** `cargo build -p cockpit-server` completes with the new types
importable via `lance_graph_contract::{qualia, proprioception, world_map}`.

### T0.2 — chess-ingest crate

New crate at `q2/crates/chess-ingest/`:

```rust
pub struct ChessIngestor { /* lichess token, cache, etc. */ }

impl ChessIngestor {
    pub async fn pull_game(&self, id: &str) -> Result<Game>;
    pub fn to_spo(&self, game: &Game) -> Vec<SpoTriple>;
    pub fn to_edges(&self, game: &Game) -> Vec<CausalEdge64>;
    pub fn to_positions(&self, game: &Game) -> Vec<PositionRow>;
}
```

Output: a `Vec<PositionRow>` that feeds directly into `BindSpace`, one row
per position. FEN → 16K bit content fingerprint via deterministic hash.

**Acceptance:** ingesting 10 lichess rapid games produces ~800 rows, all
addressable by position FEN hash, with valid CausalEdge64 moves between them.

### T0.3 — ChessRenderer

Drop-in `WorldMapRenderer` that relabels the generic anchors as chess phases:

```rust
impl WorldMapRenderer for ChessRenderer {
    fn anchor_label(&self, a: StateAnchor) -> &str {
        match a {
            StateAnchor::Intake   => "opening",
            StateAnchor::Focused  => "tactics",
            StateAnchor::Rest     => "drawn_endgame",
            StateAnchor::Flow     => "attack",
            StateAnchor::Observer => "positional",
            StateAnchor::Balanced => "middlegame",
            StateAnchor::Baseline => "repertoire",
        }
    }
    fn axis_label(&self, idx: usize) -> &str {
        const L: [&str; 11] = [
            "initiative",   // warmth
            "certainty",    // clarity
            "complexity",   // depth
            "king_safety",  // safety
            "activity",     // vitality
            "intuition",    // insight
            "coordination", // contact
            "threat",       // tension
            "novelty",      // novelty
            "beauty",       // wonder
            "harmony",      // attunement
        ];
        L.get(idx).copied().unwrap_or("")
    }
}
```

**Acceptance:** rendered `WorldMapDto` reads as chess commentary, not
generic state telemetry.

### T0.4 — Wire `/api/mri/scan` to real data

In `cockpit-server/src/main.rs`, replace the stub in `mri_scan_handler` with:

1. Load current position from game state.
2. Run `cognitive-shader-driver` dispatch over candidate moves.
3. Build `WorldModelDto` with filled `qualia`, `axes`, `proprioception`.
4. Render via `ChessRenderer`.
5. Return JSON.

**Acceptance:** `curl cubus.up.railway.app/api/mri/scan` returns structured
JSON with 11 named axes, anchor classification, NARS inference chain, and
opponent-model state.

### T0.5 — 3D React scene

New React component in the cockpit frontend:

```
src/features/chess-mri/
  ChessBoard3D.tsx         — react-three-fiber board + pieces
  ProprioceptionCloud.tsx  — 11 volumetric fog layers, axis-colored
  InferenceTree.tsx        — NARS chain as branching edges
  PositionSimilarity.tsx   — "positions that felt similar" grid
  useMriStream.ts          — 500ms polling on /api/mri/scan
```

Palette: Gotham-inspired dark theme. Pieces render as glowing nodes.
Axes as transparent colored fog (warmth=warm red, clarity=cool white,
tension=orange, etc). NARS edges animate in/out as confidence updates.

**Acceptance:** loading `cubus.up.railway.app/chess` (new route) shows a
live 3D board with ambient ProprioceptionAxes cloud, updating every 500ms.
A human watcher can tell visually when the engine is "feeling confident"
vs "feeling tense".

---

## 4. Tier 1 — Additive after the vertical lands

| # | Extension | Value |
|---|-----------|-------|
| T1.1 | Multi-game RL loop (self-play or bot matches) | NARS confidence updates across thousands of positions — the engine "learns" positional themes |
| T1.2 | Multiple analyst agents on A2A blackboard | "Tactical analyst" and "positional analyst" argue about a move; blackboard consensus resolves |
| T1.3 | Similarity retrieval ("this feels like that game") | Hamming sweep over cycle_fingerprint columns — first real use of BindSpace as episodic memory |
| T1.4 | 3D Quarto cell adaptation | Each position is a notebook cell; moves are reactive dependencies; timeline is the cell graph |
| T1.5 | Gotham object-explorer sidebar | Click any piece → see its "object page" with relationship graph (attacks, defends, pinned-by) |
| T1.6 | Live engine thinking feed (WebSocket) | Stream NARS inferences as they fire, visualize as text + graph pulses |
| T1.7 | airwar.cloud transfer | Replace chess ingestor with OSINT ingestor; same contract, same cockpit, real-world use case |

---

## 5. Validation Criteria

The vertical is "done" when all of these are true:

1. `cargo build` at the q2 workspace root compiles everything including
   `chess-ingest`, `cockpit-server` with real lance-graph, and the React
   frontend.
2. `cargo test -p chess-ingest` passes on a corpus of 10 lichess games.
3. `cubus.up.railway.app/health` reports `engine: "lance-graph"` (not
   "pending").
4. `cubus.up.railway.app/api/mri/scan` returns a `WorldModelDto`-shaped JSON
   with non-null qualia, axes, proprioception, cycle_fingerprint fields.
5. `cubus.up.railway.app/chess` loads the 3D scene and refreshes every 500ms.
6. A human watcher, without looking at the evaluation score, can tell from
   the MRI cloud whether the engine is in a good or bad position.

If criterion 6 is met, the contract's qualia/proprioception framing is
vindicated — the engine's internal state is externally legible. That's the
hard part of AGI reduced to a visual.

---

## 6. Known Unknowns

- **FEN → 16K fingerprint.** Which hash function preserves positional
  similarity? Pure SHA-256 loses it. Positional encoding via BindSpace
  content column (piece positions as bit pattern) preserves Hamming
  similarity between related positions — that's the right approach but
  needs verification.
- **Engine choice.** Stockfish via UCI vs built-in neural eval? Start with
  lichess cloud eval (no local engine needed).
- **NARS frequency/confidence sourcing.** From game result (white win/black
  win/draw) aggregated over thousands of games? Or from engine eval as proxy?
  Probably both, weighted.
- **3D vs 2D.** 3D looks great for the demo but 2D with carefully designed
  Gotham-style widgets may be more information-dense. Build both, let
  usability decide.
- **Live-thinking latency.** 500ms is cockpit default but the engine cycle
  may be faster or slower. Double-buffer as the MRI endpoint already does.

---

## 7. Why This Matters Beyond Chess

If this vertical works, we have proof that:

1. The 4-pillar contract survives real load (thousands of positions, RL updates).
2. The game-engine / state-estimation framing generalizes (chess → OSINT with
   no contract changes).
3. The Gotham-style cockpit is a viable primary UI (not just a demo).
4. Live proprioceptive telemetry is legible to humans (criterion 6 above).
5. BindSpace + cycle_fingerprint + WorldModelDto form a working episodic
   memory substrate.

At that point airwar.cloud becomes a swap-the-ingestor task, not a new
architecture project.
