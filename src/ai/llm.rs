//! Optional remote LLM support (OpenAI-compatible /chat/completions).
//!
//! The app is fully usable offline; when the user configures an endpoint +
//! key in Settings → AI, chat responses and recommendation explanations are
//! upgraded by the remote model. Failures degrade gracefully to local answers.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct LlmTurn {
    pub role: &'static str,
    pub content: String,
}

impl LlmTurn {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system", content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user", content: content.into() }
    }
}

pub fn normalize_endpoint(endpoint: &str) -> String {
    let e = endpoint.trim().trim_end_matches('/');
    if e.ends_with("/chat/completions") {
        e.to_string()
    } else if e.ends_with("/v1") {
        format!("{e}/chat/completions")
    } else {
        format!("{e}/v1/chat/completions")
    }
}

pub async fn complete(
    client: &reqwest::Client,
    config: &LlmConfig,
    turns: &[LlmTurn],
    max_tokens: u32,
) -> Result<String, String> {
    if config.endpoint.trim().is_empty() || config.api_key.trim().is_empty() {
        return Err("LLM not configured".to_string());
    }
    let url = normalize_endpoint(&config.endpoint);
    let body = json!({
        "model": config.model,
        "messages": turns.iter().map(|t| json!({"role": t.role, "content": t.content})).collect::<Vec<_>>(),
        "max_tokens": max_tokens,
        "temperature": 0.7,
    });

    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(45))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("LLM returned status {}", resp.status()));
    }
    let payload: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("LLM bad payload: {e}"))?;
    payload
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "LLM returned no content".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_normalization() {
        assert_eq!(
            normalize_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_endpoint("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_endpoint("https://x.io/v1/chat/completions/"),
            "https://x.io/v1/chat/completions"
        );
    }
}
