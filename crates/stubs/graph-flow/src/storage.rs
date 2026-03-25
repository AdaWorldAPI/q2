//! Session storage — typed key-value context for graph execution.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A session holds the execution context for one graph run.
/// Thread-safe via RwLock for async access.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub context: SessionContext,
    current_node: String,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: format!("session-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()),
            context: SessionContext::new(),
            current_node: String::new(),
        }
    }

    /// Create a session for a specific task starting at a given node.
    pub fn new_from_task(id: String, start_node: &str) -> Self {
        Self {
            id,
            context: SessionContext::new(),
            current_node: start_node.to_string(),
        }
    }

    pub fn current_node(&self) -> &str {
        &self.current_node
    }

    pub fn advance_to(&mut self, node: &str) {
        self.current_node = node.to_string();
    }
}

/// Thread-safe key-value context.
/// Values are stored as JSON for type flexibility.
#[derive(Debug, Clone)]
pub struct SessionContext {
    data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a value in the context. Accepts any serializable type.
    pub async fn set<T: serde::Serialize>(&self, key: &str, value: T) {
        let json = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
        self.data.write().await.insert(key.to_string(), json);
    }

    /// Get a value from the context. Returns None if key doesn't exist or type doesn't match.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let data = self.data.read().await;
        data.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get raw JSON value.
    pub async fn get_raw(&self, key: &str) -> Option<serde_json::Value> {
        self.data.read().await.get(key).cloned()
    }

    /// List all keys.
    pub async fn keys(&self) -> Vec<String> {
        self.data.read().await.keys().cloned().collect()
    }
}
