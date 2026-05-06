//! /v1/shader/stream — SSE endpoint emitting the REAL DTO pipeline.
//!
//! Φ StreamDto → Ψ ResonanceDto → B BusDto → Γ ThoughtStruct
//!
//! Drives the actual `thinking_engine::ThinkingEngine`:
//!   1. Cypher file → `crate::scene_player::cypher_to_stream` produces a real
//!      `thinking_engine::dto::StreamDto` (codebook indices, source, ts).
//!   2. `engine.perturb(&stream.codebook_indices)` injects energy.
//!   3. `engine.think(max_cycles)` runs MatVec cycles → returns `ResonanceDto`.
//!   4. `engine.commit()` collapses the dominant peak → `BusDto`.
//!   5. `crate::dto_bridge::*` converts each engine DTO → `Wire*` for SSE.
//!
//! NO simulated cycles. NO content hashing. NO fabricated codebook indices.
//! Serde lives only at the SSE boundary; the internal path stays in native types.

use std::convert::Infallible;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use thinking_engine::engine::ThinkingEngine;
use thinking_engine::dto::{StreamDto as EngStreamDto, ResonanceDto as EngResonanceDto,
                           BusDto as EngBusDto, ThoughtStruct as EngThoughtStruct,
                           SourceType};

use crate::dto_bridge::{WireStreamDto, WireResonanceDto, WireBusDto, WireThoughtStruct};

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

// ── Scene player: see `crate::scene_player` for discovery + Cypher → StreamDto.

// ── Free-energy heuristic ────────────────────────────────────────────────────

fn make_free_energy_from_resonance(resonance: &EngResonanceDto, confidence: f32) -> WireFreeEnergy {
    // Likelihood: dominant peak energy (how strongly one atom won).
    let likelihood = resonance.top_k.first().map(|(_, e)| *e).unwrap_or(0.0)
        .clamp(0.0, 1.0);
    // KL divergence proxy: entropy of the distribution (high entropy = far from prior).
    let mut h = 0.0f32;
    for &e in &resonance.energy {
        if e > 1e-10 { h -= e * e.ln(); }
    }
    let kl = (h / 8.0).clamp(0.0, 1.0); // normalize entropy to roughly [0,1]
    let free_energy = ((1.0 - likelihood) + kl - confidence * 0.3).clamp(0.0, 2.0);
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

// ── SSE event builder ─────────────────────────────────────────────────────────

fn shader_event(kind: &'static str, payload: serde_json::Value) -> Event {
    let ev = ShaderEvent { kind, ts: now_ms(), payload };
    let json = serde_json::to_string(&ev).unwrap_or_default();
    Event::default().data(json).event(kind)
}

// ── SSE handler ───────────────────────────────────────────────────────────────

/// GET /v1/shader/stream — continuous SSE stream of the REAL DTO pipeline.
///
/// Query params:
///   ?cypher_dir=<path>   override Cypher file directory
///   ?cycle_ms=<ms>       ms per cognitive cycle (default 800)
///
/// Per-connection state:
///   - one `ThinkingEngine` (4096×4096 distance table from `crate::codebook`)
///     wrapped in `Arc<Mutex<>>` so the engine can be retained across yields
///     in the async stream without holding a non-Send borrow over an await.
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

    // Build the real ThinkingEngine for this connection.
    // The distance table comes from crate::codebook (agent #6 owns).
    let distance_table = crate::codebook::default_distance_table();
    let engine = Arc::new(Mutex::new(ThinkingEngine::new(distance_table)));

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

        // Scene player loop — drives the REAL thinking-engine.
        let mut act_idx = 0u32;
        loop {
            let act = &acts[act_idx as usize % acts.len()];
            let name = &act.name;
            let content = &act.cypher_text;
            let confidence = act.confidence;

            // 1. Scene event
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

            // 2. StreamDto — Cypher → real codebook indices via scene_player.
            let ts = now_ms();
            let stream_dto: EngStreamDto = crate::scene_player::cypher_to_stream(content, ts);
            let wire_stream = WireStreamDto::from(&stream_dto);
            yield Ok(shader_event("stream", serde_json::to_value(&wire_stream).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 3. Drive the engine: perturb → think → commit.
            //    All engine work happens under a lock that is RELEASED before
            //    awaiting the next sleep, keeping the future Send.
            let (resonance_dto, bus_dto): (EngResonanceDto, EngBusDto) = {
                let mut eng = engine.lock().await;
                // New thought starts fresh (per-act).
                eng.reset();
                eng.perturb(&stream_dto.codebook_indices);
                // think() runs MatVec cycles up to convergence.
                let resonance = eng.think(10);
                let bus = eng.commit();
                (resonance, bus)
            };

            // 4. Resonance event — converted via dto_bridge.
            let wire_resonance = WireResonanceDto::from(&resonance_dto);
            yield Ok(shader_event("resonance", serde_json::to_value(&wire_resonance).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 5. Bus event.
            let wire_bus = WireBusDto::from(&bus_dto);
            yield Ok(shader_event("bus", serde_json::to_value(&wire_bus).unwrap_or_default()));

            // 6. Thought event — pair the bus with sensor contributions.
            let thought = EngThoughtStruct::from_engine(
                bus_dto.clone(),
                vec![(stream_dto.source, stream_dto.codebook_indices.clone())],
            );
            let mut wire_thought = WireThoughtStruct::from(&thought);
            // Lazy text rendering: the engine doesn't text-render, we annotate here.
            wire_thought.text = Some(format!(
                "[{}] codebook[{}] energy={:.3} cycles={} converged={}",
                name, bus_dto.codebook_index, bus_dto.energy,
                bus_dto.cycle_count, bus_dto.converged
            ));
            yield Ok(shader_event("thought", serde_json::to_value(&wire_thought).unwrap_or_default()));

            // 7. Free-energy event derived from the actual resonance distribution.
            let cycle_n = {
                let mut s = scene_state.write().await;
                s.cycle += 1;
                let fe_val = (1.0 - bus_dto.energy) + (resonance_dto.entropy() / 8.0);
                s.free_energy = fe_val.clamp(0.0, 2.0);
                s.cycle
            };
            let _ = cycle_n; // available for downstream use
            let fe = make_free_energy_from_resonance(&resonance_dto, confidence);
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

// Suppress unused-import warning for SourceType when the bridge changes signature.
#[allow(dead_code)]
fn _source_type_witness(_s: SourceType) {}
