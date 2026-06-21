//! OpenAI-compatible API endpoints for q2-cockpit.
//!
//! Wraps ndarray's model router with Axum JSON handlers.
//! All types 1:1 with OpenAI REST API — any OpenAI client library
//! (Python `openai`, JS `openai`, curl) works by pointing `base_url`
//! to `http://localhost:2718/v1`.
//!
//! # Endpoints
//!
//! ```text
//! GET  /v1/models              → list available models
//! GET  /v1/models/:id          → get model details
//! POST /v1/completions         → text completion (GPT-2)
//! POST /v1/chat/completions    → chat completion (OpenChat 3.5, GPT-2 adapter)
//! POST /v1/embeddings          → embeddings (GPT-2 wte)
//! POST /v1/images/generations  → image generation (Stable Diffusion scaffold)
//! ```
//!
//! # Usage
//!
//! ```python
//! from openai import OpenAI
//! client = OpenAI(base_url="http://localhost:2718/v1", api_key="local")
//! client.chat.completions.create(model="openchat_3.5", messages=[...])
//! ```

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Wire types (serde ↔ ndarray api_types) ──────────────────────────────────
// These mirror ndarray::hpc::models::api_types 1:1 but add Serialize/Deserialize
// for the JSON transport layer. The ndarray types are internal (no serde),
// these are the JSON surface.

// /v1/models

#[derive(Serialize)]
struct ModelObject {
    id: String,
    object: &'static str,
    created: u64,
    owned_by: String,
}

#[derive(Serialize)]
struct ModelListResponse {
    object: &'static str,
    data: Vec<ModelObject>,
}

// /v1/completions

#[derive(Deserialize)]
pub struct CompletionReq {
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<usize>,
    pub stream: Option<bool>,
    pub logprobs: Option<usize>,
    pub echo: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub seed: Option<u64>,
    pub suffix: Option<String>,
    pub user: Option<String>,
}

#[derive(Serialize)]
struct CompletionResp {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<CompletionChoice>,
    usage: UsageObj,
}

#[derive(Serialize)]
struct CompletionChoice {
    index: usize,
    text: String,
    logprobs: Option<serde_json::Value>,
    finish_reason: Option<String>,
}

// /v1/chat/completions

#[derive(Deserialize)]
pub struct ChatCompletionReq {
    pub model: Option<String>,
    pub messages: Vec<ChatMessageReq>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<usize>,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub seed: Option<u64>,
    pub user: Option<String>,
    pub tools: Option<serde_json::Value>,
    pub tool_choice: Option<String>,
    pub response_format: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ChatMessageReq {
    pub role: String,
    pub content: Option<String>,
    pub name: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct ChatCompletionResp {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<ChatChoiceObj>,
    usage: UsageObj,
    system_fingerprint: Option<String>,
}

#[derive(Serialize)]
struct ChatChoiceObj {
    index: usize,
    message: ChatMessageObj,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChatMessageObj {
    role: String,
    content: Option<String>,
}

// /v1/embeddings

#[derive(Deserialize)]
pub struct EmbeddingReq {
    pub model: Option<String>,
    pub input: serde_json::Value, // string, array of strings, or token IDs
    pub encoding_format: Option<String>,
    pub dimensions: Option<usize>,
    pub user: Option<String>,
}

#[derive(Serialize)]
struct EmbeddingResp {
    object: &'static str,
    model: String,
    data: Vec<EmbeddingObj>,
    usage: UsageObj,
}

#[derive(Serialize)]
struct EmbeddingObj {
    object: &'static str,
    index: usize,
    embedding: Vec<f32>,
}

// /v1/images/generations

#[derive(Deserialize)]
pub struct ImageGenReq {
    pub model: Option<String>,
    pub prompt: String,
    pub n: Option<usize>,
    pub size: Option<String>,
    pub response_format: Option<String>,
    pub quality: Option<String>,
    pub style: Option<String>,
    pub user: Option<String>,
}

#[derive(Serialize)]
struct ImageGenResp {
    created: u64,
    data: Vec<ImageObj>,
}

#[derive(Serialize)]
struct ImageObj {
    b64_json: Option<String>,
    url: Option<String>,
    revised_prompt: Option<String>,
}

// Shared

#[derive(Serialize)]
struct UsageObj {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Serialize)]
struct ErrorResp {
    error: ErrorObj,
}

#[derive(Serialize)]
struct ErrorObj {
    message: String,
    r#type: String,
    param: Option<String>,
    code: Option<String>,
}

// ── State ───────────────────────────────────────────────────────────────────

/// Shared state for OpenAI endpoints. Holds no model weights by default —
/// models are loaded lazily or at startup via `load_gpt2()` etc.
pub struct OpenAiState {
    models: Vec<ModelObject>,
    request_counter: u64,
}

impl OpenAiState {
    pub fn new() -> Self {
        Self {
            models: vec![
                ModelObject { id: "gpt2".into(), object: "model", created: 0, owned_by: "adaworldapi".into() },
                ModelObject { id: "openchat_3.5".into(), object: "model", created: 0, owned_by: "openchat".into() },
                ModelObject { id: "stable-diffusion-v1-5".into(), object: "model", created: 0, owned_by: "stabilityai".into() },
                ModelObject { id: "text-embedding-jina-v4".into(), object: "model", created: 0, owned_by: "jinaai".into() },
                ModelObject { id: "text-embedding-bert-base".into(), object: "model", created: 0, owned_by: "google".into() },
            ],
            request_counter: 0,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.request_counter += 1;
        format!("{}-{}", prefix, self.request_counter)
    }
}

pub type SharedOpenAiState = Arc<Mutex<OpenAiState>>;

// ── Router ──────────────────────────────────────────────────────────────────

/// Build the `/v1/*` Axum router for OpenAI-compatible endpoints.
pub fn openai_router(state: SharedOpenAiState) -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/models/{model_id}", get(get_model))
        .route("/v1/completions", post(create_completion))
        .route("/v1/chat/completions", post(create_chat_completion))
        .route("/v1/embeddings", post(create_embedding))
        .route("/v1/images/generations", post(create_image))
        .with_state(state)
}

// ── Handlers ────────────────────────────────────────────────────────────────

async fn list_models(
    State(state): State<SharedOpenAiState>,
) -> Json<ModelListResponse> {
    let st = state.lock().await;
    Json(ModelListResponse {
        object: "list",
        data: st.models.iter().map(|m| ModelObject {
            id: m.id.clone(),
            object: "model",
            created: m.created,
            owned_by: m.owned_by.clone(),
        }).collect(),
    })
}

async fn get_model(
    State(state): State<SharedOpenAiState>,
    Path(model_id): Path<String>,
) -> impl IntoResponse {
    let st = state.lock().await;
    match st.models.iter().find(|m| m.id == model_id) {
        Some(m) => (StatusCode::OK, Json(serde_json::json!({
            "id": m.id,
            "object": "model",
            "created": m.created,
            "owned_by": m.owned_by,
        }))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": {
                "message": format!("The model '{}' does not exist", model_id),
                "type": "invalid_request_error",
                "param": "model",
                "code": "model_not_found"
            }
        }))).into_response(),
    }
}

async fn create_completion(
    State(state): State<SharedOpenAiState>,
    Json(req): Json<CompletionReq>,
) -> Json<CompletionResp> {
    let mut st = state.lock().await;
    let id = st.next_id("cmpl");
    let model = req.model.unwrap_or_else(|| "gpt2".into());

    // Scaffold response — actual inference hooks into ndarray::hpc::gpt2 engine
    Json(CompletionResp {
        id,
        object: "text_completion",
        created: 0,
        model: model.clone(),
        choices: vec![CompletionChoice {
            index: 0,
            text: format!("[completion from {} — load weights to enable inference]", model),
            logprobs: None,
            finish_reason: Some("stop".into()),
        }],
        usage: UsageObj { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
    })
}

async fn create_chat_completion(
    State(state): State<SharedOpenAiState>,
    Json(req): Json<ChatCompletionReq>,
) -> Json<ChatCompletionResp> {
    let mut st = state.lock().await;
    let id = st.next_id("chatcmpl");
    let model = req.model.unwrap_or_else(|| "openchat_3.5".into());

    // Build content from messages for scaffold
    let user_msg = req.messages.iter()
        .filter(|m| m.role == "user")
        .filter_map(|m| m.content.as_deref())
        .last()
        .unwrap_or("");

    Json(ChatCompletionResp {
        id,
        object: "chat.completion",
        created: 0,
        model: model.clone(),
        choices: vec![ChatChoiceObj {
            index: 0,
            message: ChatMessageObj {
                role: "assistant".into(),
                content: Some(format!(
                    "[{} — load weights to enable inference. Last user message: '{}']",
                    model,
                    &user_msg[..user_msg.len().min(100)]
                )),
            },
            finish_reason: Some("stop".into()),
        }],
        usage: UsageObj { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        system_fingerprint: None,
    })
}

async fn create_embedding(
    State(state): State<SharedOpenAiState>,
    Json(req): Json<EmbeddingReq>,
) -> Json<EmbeddingResp> {
    let model = req.model.unwrap_or_else(|| "gpt2".into());

    // Scaffold: return zero vector
    let dim = req.dimensions.unwrap_or(768);
    Json(EmbeddingResp {
        object: "list",
        model,
        data: vec![EmbeddingObj {
            object: "embedding",
            index: 0,
            embedding: vec![0.0; dim],
        }],
        usage: UsageObj { prompt_tokens: 1, completion_tokens: 0, total_tokens: 1 },
    })
}

async fn create_image(
    State(state): State<SharedOpenAiState>,
    Json(req): Json<ImageGenReq>,
) -> Json<ImageGenResp> {
    let n = req.n.unwrap_or(1);

    Json(ImageGenResp {
        created: 0,
        data: (0..n).map(|_| ImageObj {
            b64_json: Some("[scaffold — load SD weights to enable generation]".into()),
            url: None,
            revised_prompt: Some(req.prompt.clone()),
        }).collect(),
    })
}
