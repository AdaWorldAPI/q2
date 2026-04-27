# Chess-NARS Vertical Slice — v2 (ruci + lichess-bot + AriGraph alive)

> **Supersedes** the earlier draft of this doc.
> **Cross-reference:** `.claude/knowledge/positioning-quarto-4d.md` (how
> this demo fits the Neo4j-Browser-plus-Palantir-Gotham positioning) and
> `session-capstone-2026-04-18.md` in `AdaWorldAPI/lance-graph`.

---

## Correction to the earlier draft

The first version of this plan assumed we'd be building:

1. A new `chess-ingest` crate from scratch.
2. A UCI bridge from scratch.
3. A lichess adapter from scratch.
4. An episodic memory retrieval policy.

**All four already exist.** Inventory below.

---

## Existing Pieces (No Build Required)

### `AdaWorldAPI/ruci` — UCI crate

Fork of the public `ruci` crate. Clean UCI Engine ↔ GUI protocol, sync +
async connections, bundles Stockfish binary.

- `src/engine/` — best_move, info, id, option, registration
- `src/gui/` — debug, go, position, register, set_option
- `src/engine_connection{,_async}.rs` — talk-to-engine API
- `resources/stockfish-ubuntu-x86-64-avx2` + windows build

**Role:** cockpit ↔ Stockfish bridge for ground-truth evaluation.
Add as workspace dep. Zero lines of new UCI code.

### `AdaWorldAPI/lichess-bot` — Python bridge

Fork of ShailChoksi's lichess-bot with a custom strategy already in place:

- `strategies/stonksfish_crew.py` — Ada's custom strategy hook
- `lib/lichess.py` — Lichess API wrapper
- `lib/engine_wrapper.py` — UCI engine wrapper
- `lib/conversation.py`, `lib/matchmaking.py`, `lib/timer.py`

**Role:** Lichess.com ↔ our cockpit adapter. Streams game state,
receives our move back. `stonksfish_crew.py` delegates to the cockpit
over HTTP.

### `AdaWorldAPI/lance-graph` — AriGraph (4,696 lines, SHIPPED)

Located at `crates/lance-graph/src/graph/arigraph/`:

| Module | Lines | Role |
|--------|-------|------|
| `episodic.rs` | 210 | `Episode` + `EpisodicMemory` — capacity-bounded, Hamming retrieval, NARS truth |
| `triplet_graph.rs` | 1064 | SPO knowledge graph, NARS truth, BFS association, spatial paths |
| `retrieval.rs` | 447 | Fingerprint-based retrieval policies |
| `sensorium.rs` | 539 | Observation → triplets extractor (position → SPO) |
| `orchestrator.rs` | 1562 | AriGraph coordinator |
| `xai_client.rs` | 521 | xAI enrichment client |
| `language.rs` | 339 | LM bridge |

**Role:** the entire "position → episodic memory → retrieval" layer is
already done. We just feed chess observations into `sensorium.rs` and
query `EpisodicMemory` for "positions that felt similar".

### `AdaWorldAPI/lance-graph` — lance-graph-osint (SHIPPED)

Located at `crates/lance-graph-osint/`:

- `crawler.rs` — HTTP pipeline
- `extractor.rs` — entity / relation extraction
- `pipeline.rs` — ingest orchestration
- `reader.rs` — source adapter
- `lib.rs` — crate root

**Role:** OSINT counterpart to chess ingestion. Same pipeline, different
input stream (feeds, reports, documents). Transfers in place.

### `AdaWorldAPI/lance-graph` — 4-pillar contract (MERGED TO main)

`nars` + `thinking` + `qualia` + `proprioception` + `world_map` +
extended `world_model`. Any consumer inherits all five by depending on
`lance-graph-contract`.

### `AdaWorldAPI/q2` — cockpit-server (DEPLOYED at cubus.up.railway.app)

Axum server with 16 routes defined in source. Currently runs in
`palantir-demo` mode with `engine: "lance-graph (pending)"`. Tier 0.1
(pin Cargo.toml) flips pending → live.

---

## Revised Architecture

```
[Lichess]
   ▲                      WebSocket
   │ move                 game stream
   ▼
[lichess-bot (Python)]
   │ stonksfish_crew.py delegates to HTTP
   ▼
[cockpit-server /api/bot/move]              ← NEW endpoint (small)
   │ POST { fen, time_ms }
   ▼
┌─────────────────────────────────────────────────────────────┐
│  AriGraph (already live)                                    │
│    sensorium.rs:  FEN → triplets (piece, moves_to, square)  │
│    episodic.rs:   insert Episode, Hamming-retrieve similar  │
│    triplet_graph: SPO store with NARS truth                 │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  cognitive-shader-driver (already live)                     │
│    ShaderDispatch over candidate-move rows                  │
│    Emits cycle_fingerprint (analysis signature)             │
│    ProprioceptionAxes classified into StateAnchor           │
│    DriveMode: Explore (novelty) / Exploit (theory)          │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  ruci (already live)                                        │
│    UCI connection to Stockfish                              │
│    Ground-truth eval for NARS confidence calibration        │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
[cockpit-server picks best move + packages WorldModelDto]
                      │
                      ▼
   back to lichess-bot → back to Lichess

[Side channel]
   cockpit-server also pushes WorldModelDto to /mri stream
                      │
                      ▼
   React 3D scene at cubus.up.railway.app/chess
```

Every box except three is already implemented. The three are:

1. `cockpit-server /api/bot/move` — small new Axum handler.
2. The React 3D scene at `/chess`.
3. Thin glue: tell lichess-bot's `stonksfish_crew.py` to POST to our
   cockpit instead of running Stockfish locally.

---

## Revised Tier 0 — 3 days of focused work

### T0.1 — Pin q2 to lance-graph main

Edit `q2/Cargo.toml`:

```toml
[workspace.dependencies]
lance-graph         = { git = "https://github.com/AdaWorldAPI/lance-graph", branch = "main" }
lance-graph-contract = { git = "https://github.com/AdaWorldAPI/lance-graph", branch = "main" }
ruci                = { git = "https://github.com/AdaWorldAPI/ruci",        branch = "master" }
```

Flip cockpit health string from `"lance-graph (pending)"` to `"lance-graph"`.

**Acceptance:** `cargo build -p cockpit-server` compiles with AriGraph +
contract + ruci all importable.

### T0.2 — `/api/bot/move` endpoint

New Axum handler in `cockpit-server/src/main.rs`:

```rust
#[derive(Deserialize)]
struct BotMoveReq { fen: String, time_ms: u64, game_id: Option<String> }

async fn bot_move_handler(Json(req): Json<BotMoveReq>) -> Json<BotMoveResp> {
    // 1. FEN → sensorium → triplets → Episode
    let triplets = arigraph::sensorium::position_to_triplets(&req.fen);
    state.episodic.add(&req.fen, &triplets, step);

    // 2. AriGraph retrieval: similar past positions (Hamming)
    let similar = state.episodic.retrieve_similar(&req.fen, k=8);

    // 3. cognitive-shader-driver dispatch over candidate moves
    let crystal = state.driver.dispatch(&dispatch_req);

    // 4. ruci: UCI sync eval against Stockfish for ground truth
    let uci_eval = state.stockfish.evaluate(&req.fen, req.time_ms).await;

    // 5. Package WorldModelDto with qualia + axes + proprioception
    let wm = build_world_model(&crystal, &uci_eval, &similar);

    // 6. Push to /mri stream channel
    state.mri_tx.send(wm.clone()).ok();

    Json(BotMoveResp {
        move_uci: crystal.top_move_uci,
        world_model: wm,
    })
}
```

**Acceptance:** `curl -X POST cubus.up.railway.app/api/bot/move -d
'{"fen":"...startpos...","time_ms":1000}'` returns a UCI move and a
structured WorldModelDto.

### T0.3 — lichess-bot hook

In `lichess-bot/strategies/stonksfish_crew.py`, change the move
computation to `POST /api/bot/move`:

```python
def search(board, time_limit, ponder, draw_offered, root_moves):
    fen = board.fen()
    r = requests.post(
        f"{COCKPIT_URL}/api/bot/move",
        json={"fen": fen, "time_ms": int(time_limit.time * 1000)},
        timeout=time_limit.time + 2
    )
    return chess.Move.from_uci(r.json()["move_uci"])
```

Configure lichess-bot to use `stonksfish_crew` as the default strategy.

**Acceptance:** run `python3 lichess-bot.py`, accept a challenge from a
random opponent, play a full game to completion using cockpit-computed
moves.

### T0.4 — React 3D scene at `/chess`

Add a new route + component in the cockpit's React build:

```
src/features/chess-mri/
  ChessBoard3D.tsx         — react-three-fiber board + pieces
  ProprioceptionCloud.tsx  — 11 volumetric fog layers, one per axis
  SimilarPositions.tsx     — grid of "felt like this past game" thumbnails (AriGraph retrieval)
  MoveInferenceTree.tsx    — NARS chain as branching edges
  useMoveStream.ts         — WebSocket or 500ms polling on /api/mri/scan
```

The ChessRenderer from the v1 plan still applies — map generic
anchors to chess moods (opening / tactics / attack / positional /
endgame / draw / zugzwang).

**Acceptance:** visit `cubus.up.railway.app/chess` during a live bot
game and see the board rendered in 3D with a volumetric MRI cloud
updating every 500ms. The SimilarPositions grid shows 8 past positions
from AriGraph's episodic memory for the current FEN.

---

## Tier 1 — After the vertical lands (additive)

| # | Extension | Note |
|---|-----------|------|
| T1.1 | Multi-game RL loop | Episodic memory already retains episodes; just add a game-result feedback that revises NARS f/c |
| T1.2 | Multi-analyst blackboard | Contract has `a2a_blackboard`; spin up two `cognitive-shader-driver` instances with different StyleSelectors |
| T1.3 | "This feels like that game" narration | Already in place via `EpisodicMemory::retrieve_similar`; just render nicely |
| T1.4 | xAI enrichment on unusual positions | `xai_client.rs` is live; hook it in for positions with high `classification_distance` (novel/liminal states) |
| T1.5 | OSINT mode switch | Swap the sensorium module from `chess-sensorium` to `osint-sensorium`; same cockpit, same pipeline, different domain. airwar.cloud use case. |

---

## Validation criteria (unchanged)

1. `cargo build` at q2 workspace root compiles everything.
2. `cubus.up.railway.app/health` reports `engine: "lance-graph"`.
3. `POST /api/bot/move` returns valid UCI move + populated WorldModelDto.
4. lichess-bot plays a full game using cockpit-computed moves.
5. `cubus.up.railway.app/chess` renders 3D scene, refreshes every 500ms.
6. **A human watcher, without looking at the evaluation score, can tell
   from the MRI cloud whether the engine is in a good or bad position.**

If criterion 6 passes, the positioning ("live cognitive telemetry" as
advanced graph-analytics feature) is validated. The demo is ready.

---

## Timeline

- T0.1 (pin): 30 min
- T0.2 (bot endpoint): 4-6 hrs
- T0.3 (lichess-bot hook): 1-2 hrs
- T0.4 (React 3D): 1-2 days of frontend work

**Total: 2-3 days of focused execution.** Every other piece exists.
