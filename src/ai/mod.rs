//! MovieBox AI — local-first intelligence layer.
//!
//! Everything runs offline on the device (no account, no cloud required):
//!  - `TasteProfile`   learned from watch history + favorites (recency-weighted)
//!  - `recommend`      content-based scoring with "because you watched" rows
//!  - `semantic_search` natural-language query understanding (moods, decades,
//!                      intent words) with fuzzy matching
//!  - `mood_rows`      curated vibe categories
//!  - `insights`       viewing analytics ("Your Wrap")
//!  - `chat`           local assistant; transparently upgrades to a remote
//!                     OpenAI-compatible LLM when the user configures one.

pub mod chat;
pub mod engine;
pub mod lexicon;
pub mod llm;

pub use engine::{AiEngine, IndexedItem};

use serde::{Deserialize, Serialize};

/// User-configurable AI settings, persisted to `<data_dir>/ai-settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    /// When true, chat/recommendation explanations may use the remote LLM.
    pub llm_enabled: bool,
    /// OpenAI-compatible base URL, e.g. `https://api.openai.com/v1`.
    pub llm_endpoint: String,
    /// API key (kept on-device only).
    pub llm_api_key: String,
    /// Model name, e.g. `gpt-4o-mini`.
    pub llm_model: String,
    /// Enable local AI features (recommendations/semantic search). Default on.
    pub local_enabled: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            llm_enabled: false,
            llm_endpoint: String::new(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".to_string(),
            local_enabled: true,
        }
    }
}

impl AiSettings {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, json);
        }
    }

    pub fn llm_configured(&self) -> bool {
        self.llm_enabled && !self.llm_endpoint.is_empty() && !self.llm_api_key.is_empty()
    }
}
