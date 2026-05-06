//! /v1/shader/stream — SSE endpoint emitting the DTO pipeline.
//!
//! Φ StreamDto → Ψ ResonanceDto → B BusDto → Γ ThoughtStruct
//!
//! Scene player reads Cypher enrichment files (30 acts from aiwar-neo4j-harvest)
//! and emits them as streaming acts. Each act drives a simulated cognitive cycle.
//!
//! NO serde on internal path. JSON only at the SSE boundary.

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use serde::Serialize;
use tokio::sync::RwLock;

// ── JSON wire types (serde only at SSE boundary) ─────────────────────────────

#[derive(Clone, Serialize)]
pub struct ShaderEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub ts: u64,
    pub payload: serde_json::Value,
}

#[derive(Clone, Serialize)]
pub struct WireStreamDto {
    pub source: &'static str,
    pub codebook_indices: Vec<u16>,
    pub timestamp: u64,
}

#[derive(Clone, Serialize)]
pub struct WireResonanceDto {
    /// Sparse top-k representation (full 4096-entry field would be 16KB per frame)
    pub top_k: Vec<(u16, f32)>,
    pub cycle_count: u16,
    pub converged: bool,
    pub entropy: f32,
    pub active_count: u32,
}

#[derive(Clone, Serialize)]
pub struct WireBusDto {
    pub codebook_index: u16,
    pub energy: f32,
    pub top_k: Vec<(u16, f32)>,
    pub cycle_count: u16,
    pub converged: bool,
}

#[derive(Clone, Serialize)]
pub struct WireThoughtStruct {
    pub bus: WireBusDto,
    pub text: Option<String>,
    pub style: &'static str,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Scene player: loads Cypher files, emits acts ─────────────────────────────

/// Discover Cypher enrichment files in `dir`, version-ordered.
pub fn discover_cypher_files(dir: &str) -> Vec<(String, String)> {
    let path = Path::new(dir);
    if !path.exists() {
        return Vec::new();
    }
    let mut files: Vec<(String, String)> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()?.to_str()? == "cypher" {
                let name = p.file_stem()?.to_string_lossy().into_owned();
                let content = std::fs::read_to_string(&p).ok()?;
                Some((name, content))
            } else {
                None
            }
        })
        .collect();

    // Sort by name (version-ordered: v0 < v31 < v40 < v43)
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Confidence score from filename (higher for verified/corrected files).
fn confidence_from_name(name: &str) -> f32 {
    if name.contains("corrections") || name.contains("verified") { 0.92 }
    else if name.contains("patch") { 0.78 }
    else if name.contains("allin") { 0.85 }
    else if name.contains("enriched") || name.contains("full") { 0.70 }
    else { 0.65 }
}

/// Extract first non-empty line of Cypher as preview.
fn cypher_preview(content: &str) -> String {
    content.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//"))
        .next()
        .unwrap_or("// empty")
        .chars()
        .take(120)
        .collect()
}

// ── Simulated cognitive cycle (placeholder until thinking-engine is wired) ───

/// Generate a WireStreamDto from a Cypher act.
/// Simulates codebook_indices by hashing the content.
fn make_stream_dto(content: &str, ts: u64) -> WireStreamDto {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    let seed = h.finish();

    // Generate 8-16 pseudo-random codebook indices from the content hash
    let indices: Vec<u16> = (0u64..12)
        .map(|i| ((seed.wrapping_mul(6364136223846793005).wrapping_add(i * 1442695040888963407)) >> 48) as u16 % 4096)
        .collect();

    WireStreamDto { source: "AriGraph", codebook_indices: indices, timestamp: ts }
}

fn make_resonance_dto(stream: &WireStreamDto, cycle_count: u16) -> WireResonanceDto {
    // Simulate energy peaks at the codebook indices
    let top_k: Vec<(u16, f32)> = stream.codebook_indices.iter()
        .enumerate()
        .map(|(i, &idx)| {
            let energy = 0.9 - (i as f32 * 0.08);
            (idx, energy.max(0.1))
        })
        .take(8)
        .collect();

    let entropy = 2.1 + (cycle_count as f32 * 0.03);
    let active_count = (stream.codebook_indices.len() * 4 + 12) as u32;

    WireResonanceDto {
        top_k,
        cycle_count,
        converged: cycle_count > 5,
        entropy,
        active_count,
    }
}

fn make_bus_dto(resonance: &WireResonanceDto) -> WireBusDto {
    let top = resonance.top_k.first().copied().unwrap_or((0, 0.0));
    WireBusDto {
        codebook_index: top.0,
        energy: top.1,
        top_k: resonance.top_k.clone(),
        cycle_count: resonance.cycle_count,
        converged: resonance.converged,
    }
}

fn make_thought(bus: WireBusDto, act_name: &str, confidence: f32) -> WireThoughtStruct {
    let text = format!(
        "[{}] codebook[{}] energy={:.2} confidence={:.2}",
        act_name, bus.codebook_index, bus.energy, confidence
    );
    WireThoughtStruct {
        bus,
        text: Some(text),
        style: "Focused",
    }
}

fn make_free_energy(cycle: u64, confidence: f32) -> WireFreeEnergy {
    let t = (cycle as f32 * 0.1).sin().abs();
    let likelihood = 0.6 + t * 0.3 * confidence;
    let kl = 0.4 - t * 0.2 * confidence;
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

/// GET /v1/shader/stream — continuous SSE stream of the DTO pipeline.
///
/// Query params:
///   ?cypher_dir=<path>   override Cypher file directory
///   ?cycle_ms=<ms>       ms per cognitive cycle (default 800)
pub async fn shader_stream_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    axum::extract::State(scene_state): axum::extract::State<SharedSceneState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let cypher_dir = params
        .get("cypher_dir")
        .cloned()
        .or_else(|| std::env::var("CYPHER_PATH").ok())
        .unwrap_or_default();

    let cycle_ms: u64 = params
        .get("cycle_ms")
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);

    let stream = async_stream::stream! {
        let acts = discover_cypher_files(&cypher_dir);
        let total = acts.len() as u32;

        // Update scene state
        {
            let mut s = scene_state.write().await;
            s.total_acts = total;
        }

        if acts.is_empty() {
            // No Cypher files — emit idle heartbeat
            loop {
                let mut cycle = {
                    let mut s = scene_state.write().await;
                    s.cycle += 1;
                    s.cycle
                };
                let fe = make_free_energy(cycle, 0.5);
                yield Ok(shader_event("health", serde_json::to_value(&fe).unwrap_or_default()));
                tokio::time::sleep(Duration::from_millis(cycle_ms)).await;
            }
        }

        // Scene player loop
        let mut act_idx = 0u32;
        loop {
            let (name, content) = &acts[act_idx as usize % acts.len()];
            let confidence = confidence_from_name(name);

            // 1. Scene event
            let scene = WireSceneAct {
                act: act_idx + 1,
                total: total.max(1),
                name: name.clone(),
                cypher_preview: cypher_preview(content),
                confidence,
            };
            {
                let mut s = scene_state.write().await;
                s.act = act_idx + 1;
                s.scene_name = name.clone();
            }
            yield Ok(shader_event("scene", serde_json::to_value(&scene).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 2. StreamDto
            let ts = now_ms();
            let stream_dto = make_stream_dto(content, ts);
            yield Ok(shader_event("stream", serde_json::to_value(&stream_dto).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 3. ResonanceDto (simulate a few cycles)
            let cycle_count = 3u16 + (act_idx % 7) as u16;
            let resonance = make_resonance_dto(&stream_dto, cycle_count);
            yield Ok(shader_event("resonance", serde_json::to_value(&resonance).unwrap_or_default()));
            tokio::time::sleep(Duration::from_millis(cycle_ms / 4)).await;

            // 4. BusDto
            let bus = make_bus_dto(&resonance);
            yield Ok(shader_event("bus", serde_json::to_value(&bus).unwrap_or_default()));

            // 5. ThoughtStruct
            let thought = make_thought(bus, name, confidence);
            yield Ok(shader_event("thought", serde_json::to_value(&thought).unwrap_or_default()));

            // 6. Free energy
            let cycle = {
                let mut s = scene_state.write().await;
                s.cycle += 1;
                s.free_energy = 1.0 - confidence * 0.7;
                s.cycle
            };
            let fe = make_free_energy(cycle, confidence);
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
