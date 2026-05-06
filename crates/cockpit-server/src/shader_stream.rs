//! /v1/shader/stream — SSE endpoint emitting the canonical R1 cognitive-shader
//! pipeline.
//!
//! Φ ShaderDispatch → Ψ ShaderResonance → B ShaderBus → Γ ShaderCrystal
//!
//! Drives `crate::mock_driver::MockShaderDriver` (which implements
//! `lance_graph_contract::cognitive_shader::CognitiveShaderDriver`) over the
//! canonical R1 surface — `dispatch_with_sink(&dispatch, &mut sink)`.
//!
//! Per Cypher act:
//!   1. `crate::scene_player::discover_acts` walks `*.cypher` files.
//!   2. `crate::scene_player::cypher_to_stream` parses each act → codebook
//!      indices.
//!   3. `MockShaderDriver::perturb(&indices)` injects energy.
//!   4. `MockShaderDriver::dispatch_with_sink(&ShaderDispatch::default(), &mut SseSink)`
//!      runs the cycle. The sink buffers `resonance`, `bus`, `crystal` SSE
//!      events; the async stream loop drains them between cycles so the
//!      future stays `Send`.
//!   5. `WireFreeEnergy` is computed inline from the crystal's `ShaderBus`
//!      resonance and emitted as a `health` event.
//!
//! Event names (replaces legacy `stream`/`thought`):
//!   - `scene`     — Cypher act metadata (local SSE helper, not lance-graph DTO)
//!   - `dispatch`  — Wire mirror of `ShaderDispatch` (Φ)
//!   - `resonance` — Wire mirror of `ShaderResonance` (Ψ)
//!   - `bus`       — Wire mirror of `ShaderBus` (B)
//!   - `crystal`   — Wire mirror of `ShaderCrystal` (Γ)
//!   - `health`    — Free-energy heuristic derived from the crystal's resonance.
//!
//! Serde lives only at the SSE boundary; the internal path stays in canonical
//! native types (`ShaderDispatch`, `ShaderResonance`, `ShaderBus`,
//! `ShaderCrystal`).

use std::convert::Infallible;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use serde::Serialize;
use tokio::sync::RwLock;

use lance_graph_contract::cognitive_shader::{
    CognitiveShaderDriver, ShaderBus, ShaderCrystal, ShaderDispatch, ShaderResonance, ShaderSink,
};

use crate::dto_bridge::{WireShaderBus, WireShaderCrystal, WireShaderDispatch, WireShaderResonance};
use crate::mock_driver::MockShaderDriver;

// ── JSON wire types (serde only at SSE boundary) ─────────────────────────────

#[derive(Clone, Serialize)]
pub struct ShaderEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub ts: u64,
    pub payload: serde_json::Value,
}

#[derive(Clone, Serialize)]
pub struct WireSceneAct {
    pub act: u32,
    pub total: u32,
    pub name: String,
    pub cypher_preview: String,
    pub confidence: f32,
}

#[derive(Clone, Serialize)]
pub struct WireFreeEnergy {
    pub likelihood: f32,
    pub kl: f32,
    pub free_energy: f32,
    pub below_homeostasis: bool,
}

// ── Scene state (shared between SSE stream + scene player) ───────────────────

pub struct SceneState {
    pub act: u32,
    pub total_acts: u32,
    pub scene_name: String,
    pub cycle: u64,
    pub free_energy: f32,
}

impl SceneState {
    fn new() -> Self {
        Self {
            act: 0,
            total_acts: 0,
            scene_name: String::from("idle"),
            cycle: 0,
            free_energy: 0.5,
        }
    }
}

pub type SharedSceneState = Arc<RwLock<SceneState>>;

pub fn new_scene_state() -> SharedSceneState {
    Arc::new(RwLock::new(SceneState::new()))
}

/// Process-global scene state. Avoids axum state-type plumbing for the
/// SSE handlers (`Arc<RwLock<...>>` can't host a `FromRef<Arc<AppState>>`
/// impl due to the orphan rule, and per-route `.with_state(...)` collides
/// with the parent router state). One scene per process is correct.
static SCENE: LazyLock<SharedSceneState> = LazyLock::new(new_scene_state);

/// Accessor for the process-global scene state.
pub fn scene() -> SharedSceneState {
    SCENE.clone()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── SSE event builder ─────────────────────────────────────────────────────────

fn shader_event(kind: &'static str, payload: serde_json::Value) -> Event {
    let ev = ShaderEvent { kind, ts: now_ms(), payload };
    let json = serde_json::to_string(&ev).unwrap_or_default();
    Event::default().data(json).event(kind)
}

// ── SseSink — captures driver callbacks as SSE events ────────────────────────

/// `ShaderSink` impl that buffers SSE events for the async stream loop.
///
/// The driver calls `on_resonance` → `on_bus` → `on_crystal` synchronously
/// inside `dispatch_with_sink`. Each callback converts the canonical native
/// type to its `Wire*` mirror and pushes one SSE event into `pending`. The
/// outer async stream drains `pending` between cycles, yielding events with
/// inter-event sleeps so the SSE pacing matches the cockpit's rendering
/// budget.
struct SseSink {
    pending: Vec<Event>,
}

impl SseSink {
    fn new() -> Self {
        Self {
            pending: Vec::with_capacity(3),
        }
    }
}

impl ShaderSink for SseSink {
    fn on_resonance(&mut self, r: &ShaderResonance) -> bool {
        let wire = WireShaderResonance::from(r);
        self.pending.push(shader_event(
            "resonance",
            serde_json::to_value(&wire).unwrap_or_default(),
        ));
        true
    }

    fn on_bus(&mut self, b: &ShaderBus) -> bool {
        let wire = WireShaderBus::from(b);
        self.pending.push(shader_event(
            "bus",
            serde_json::to_value(&wire).unwrap_or_default(),
        ));
        true
    }

    fn on_crystal(&mut self, c: &ShaderCrystal) {
        let wire = WireShaderCrystal::from(c);
        self.pending.push(shader_event(
            "crystal",
            serde_json::to_value(&wire).unwrap_or_default(),
        ));
    }
}

// ── Free-energy heuristic ────────────────────────────────────────────────────

/// Derive `WireFreeEnergy` from a stabilized cycle's `ShaderBus`.
///
///   likelihood = top_k[0].resonance        (clamped to [0, 1])
///   kl         = entropy / 5.0             (rough normalization to [0, 1])
///   free_energy = (1 - likelihood) + kl
///   below_homeostasis = free_energy < 0.3
///
/// `entropy` comes from `ShaderResonance::entropy` which is already in nats
/// over the resonance distribution. Dividing by 5 ≈ ln(top-K=8) +ε buffers
/// the upper bound to roughly unity.
fn make_free_energy_from_bus(bus: &ShaderBus) -> WireFreeEnergy {
    let likelihood = bus.resonance.top_k[0].resonance.clamp(0.0, 1.0);
    let kl = (bus.resonance.entropy / 5.0).clamp(0.0, 1.0);
    let free_energy = ((1.0 - likelihood) + kl).clamp(0.0, 2.0);
    WireFreeEnergy {
        likelihood,
        kl,
        free_energy,
        below_homeostasis: free_energy < 0.3,
    }
}

fn make_free_energy_idle(cycle: u64) -> WireFreeEnergy {
    let t = (cycle as f32 * 0.1).sin().abs();
    let likelihood = 0.3 + t * 0.2;
    let kl = 0.5 - t * 0.1;
    let free_energy = 1.0 - likelihood + kl;
    WireFreeEnergy {
        likelihood,
        kl,
        free_energy,
        below_homeostasis: free_energy < 0.3,
    }
}

// ── SSE handler ───────────────────────────────────────────────────────────────

/// GET /v1/shader/stream — continuous SSE stream of the canonical R1
/// cognitive-shader pipeline.
///
/// Query params:
///   ?cypher_dir=<path>   override Cypher file directory
///   ?cycle_ms=<ms>       ms per cognitive cycle (default 800)
///
/// Per-connection state:
///   - one `MockShaderDriver` (BindSpace row count = 2048) wrapped in `Arc`
///     so the canonical `&self` dispatch surface can be invoked across
///     async yields without Send hazards.
pub async fn shader_stream_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let scene_state = scene();
    let cypher_dir = params
        .get("cypher_dir")
        .cloned()
        .or_else(|| std::env::var("CYPHER_PATH").ok())
        .unwrap_or_default();

    let cycle_ms: u64 = params
        .get("cycle_ms")
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);

    // Build the canonical driver for this connection.
    // BindSpace row count of 2048 matches the cockpit's cognitive sweep budget.
    let driver: Arc<MockShaderDriver> = Arc::new(MockShaderDriver::new(2048));

    let stream = async_stream::stream! {
        let acts = crate::scene_player::discover_acts(&cypher_dir);
        let total = acts.len() as u32;

        // Update scene state
        {
            let mut s = scene_state.write().await;
            s.total_acts = total;
        }

        if acts.is_empty() {
            // No Cypher files — emit idle heartbeat
            loop {
                let cycle = {
                    let mut s = scene_state.write().await;
                    s.cycle += 1;
                    s.cycle
                };
                let fe = make_free_energy_idle(cycle);
                yield Ok(shader_event("health", serde_json::to_value(&fe).unwrap_or_default()));
                tokio::time::sleep(Duration::from_millis(cycle_ms)).await;
            }
        }

        // Scene player loop — drives the canonical R1 cognitive shader.
        let mut act_idx = 0u32;
        loop {
            let act = &acts[act_idx as usize % acts.len()];
            let name = &act.name;
            let content = &act.cypher_text;
            let confidence = act.confidence;

            // 1. Scene event — Cypher act metadata.
            let scene = WireSceneAct {
                act: act_idx + 1,
                total: total.max(1),
                name: name.clone(),
                cypher_preview: crate::scene_player::cypher_preview(content),
                confidence,
            };
            {
                let mut s = scene_state.write().await;
                s.act = act_idx + 1;
                s.scene_name = name.clone();
            }
            yield Ok(shader_event("scene", serde_json::to_value(&scene).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 2. Build a canonical `ShaderDispatch` (default Φ envelope).
            //    Emit it as a `dispatch` event for the FE before driving so
            //    the cockpit can show the request that produced the cycle.
            let dispatch = ShaderDispatch::default();
            let wire_dispatch = WireShaderDispatch::from(&dispatch);
            yield Ok(shader_event(
                "dispatch",
                serde_json::to_value(&wire_dispatch).unwrap_or_default(),
            ));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 3. Cypher → codebook indices via scene_player. Feed them to the
            //    driver via `perturb()` so the shader sweep has energy to
            //    find on this cycle.
            let ts = now_ms();
            let stream_dto = crate::scene_player::cypher_to_stream(content, ts);
            driver.perturb(&stream_dto.codebook_indices);

            // 4. Drive the cycle. The sink buffers resonance/bus/crystal
            //    events. We hold no awaits while the driver runs, so the
            //    `&mut sink` borrow is local to this synchronous block.
            let mut sink = SseSink::new();
            let crystal: ShaderCrystal = driver.dispatch_with_sink(&dispatch, &mut sink);

            // 5. Drain sink events into the SSE stream in callback order:
            //    resonance → bus → crystal. Inter-event sleep paces the
            //    cockpit ribbon.
            let pending = std::mem::take(&mut sink.pending);
            let pending_len = pending.len();
            for (i, ev) in pending.into_iter().enumerate() {
                yield Ok(ev);
                // Don't sleep after the last event — the health event below
                // and the act-boundary sleep handle the rest of the budget.
                if i + 1 < pending_len {
                    tokio::time::sleep(Duration::from_millis(cycle_ms / 8)).await;
                }
            }

            // 6. Free-energy event derived from the crystal's bus resonance.
            //    Update scene-state cycle counter + free-energy snapshot.
            let _ = confidence; // confidence is surfaced in the scene event already
            let fe = make_free_energy_from_bus(&crystal.bus);
            {
                let mut s = scene_state.write().await;
                s.cycle += 1;
                s.free_energy = fe.free_energy;
            }
            yield Ok(shader_event("health", serde_json::to_value(&fe).unwrap_or_default()));

            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            act_idx = (act_idx + 1) % acts.len() as u32;
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// GET /v1/shader/status — current scene state as JSON.
pub async fn shader_status_handler(
    axum::extract::State(scene_state): axum::extract::State<SharedSceneState>,
) -> axum::Json<serde_json::Value> {
    let s = scene_state.read().await;
    axum::Json(serde_json::json!({
        "act": s.act,
        "total_acts": s.total_acts,
        "scene_name": s.scene_name,
        "cycle": s.cycle,
        "free_energy": s.free_energy,
    }))
}
