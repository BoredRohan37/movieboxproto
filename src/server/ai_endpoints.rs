//! AI endpoints: recommendations, mood rows, semantic search, insights,
//! chat assistant, and AI settings.

use std::sync::Arc;

use serde_json::json;

use super::http::{Request, Response};
use super::AppState;
use crate::ai::chat;

fn ok(value: serde_json::Value) -> Response {
    Response::json(200, &value).no_store()
}

fn err(status: u16, message: impl Into<String>) -> Response {
    Response::json_error(status, &message.into()).no_store()
}

fn snapshot(state: &Arc<AppState>) -> (Vec<crate::history::WatchHistoryItem>, Vec<crate::favorites::FavoriteItem>) {
    let history = state
        .history
        .lock()
        .map(|h| h.recent.clone())
        .unwrap_or_default();
    let favorites = state
        .favorites
        .lock()
        .map(|f| f.items.clone())
        .unwrap_or_default();
    (history, favorites)
}

pub fn recommendations(req: &Request, state: &Arc<AppState>) -> Response {
    let _ = req;
    let (history, favorites) = snapshot(state);
    let engine = match state.ai.lock() {
        Ok(e) => e,
        Err(_) => return err(500, "ai lock failed"),
    };

    let profile = engine.taste_profile(&history, &favorites);
    let for_you = engine.recommend(&profile, &history, &favorites, 18);
    let because = engine.because_you_watched(&history, &favorites, 12);

    let because_json: Vec<_> = because
        .iter()
        .map(|(anchor, items)| {
            json!({ "anchor": anchor, "items": items })
        })
        .collect();

    ok(json!({
        "profile_summary": profile.summary(),
        "cold_start": profile.total_signals == 0,
        "for_you": for_you,
        "because_you_watched": because_json,
    }))
}

pub fn mood(req: &Request, state: &Arc<AppState>) -> Response {
    let id = req.query_param("id").unwrap_or("feel-good");
    let limit: usize = req
        .query_param("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(18);
    let engine = match state.ai.lock() {
        Ok(e) => e,
        Err(_) => return err(500, "ai lock failed"),
    };
    match engine.mood_row(id, limit) {
        Some((label, items)) => ok(json!({ "id": id, "label": label, "items": items })),
        None => err(404, "unknown mood"),
    }
}

pub async fn ai_search(req: &Request, state: &Arc<AppState>) -> Response {
    let q = req.query_param("q").unwrap_or("").trim().to_string();
    if q.is_empty() {
        return ok(json!({ "interpreted": "", "results": [], "provider_results": [] }));
    }

    let (history, favorites) = snapshot(state);
    let mut provider_results: Vec<crate::providers::models::CatalogItem> = Vec::new();

    // Phase 1: score local knowledge (lock held only here).
    let reply = {
        let engine = match state.ai.lock() {
            Ok(e) => e,
            Err(_) => return err(500, "ai lock failed"),
        };
        let profile = engine.taste_profile(&history, &favorites);
        let result = engine.semantic_search(&q, &profile, 20);
        json!({
            "interpreted": result.interpreted,
            "results": result.results,
            "core": result.core_query,
        })
    };

    // Phase 2 (lock released): enrich with a live provider search when the
    // local knowledge base is still thin.
    let local_count = reply
        .get("results")
        .and_then(|r| r.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if local_count < 5 {
        let core = reply
            .get("core")
            .and_then(|c| c.as_str())
            .unwrap_or(q.as_str())
            .to_string();
        let search_core = if core.is_empty() { q.as_str() } else { core.as_str() };
        if let Ok(items) = state
            .service
            .search_typed(
                crate::providers::models::ProviderKind::MovieBox,
                search_core,
                1,
            )
            .await
        {
            if let Ok(mut ai) = state.ai.lock() {
                ai.learn_catalog(&items);
            }
            provider_results = items;
        }
    }

    ok(json!({
        "interpreted": reply.get("interpreted").cloned().unwrap_or_default(),
        "results": reply.get("results").cloned().unwrap_or_default(),
        "provider_results": provider_results,
    }))
}

pub fn insights(req: &Request, state: &Arc<AppState>) -> Response {
    let _ = req;
    let (history, favorites) = snapshot(state);
    let engine = match state.ai.lock() {
        Ok(e) => e,
        Err(_) => return err(500, "ai lock failed"),
    };
    let insights = engine.insights(&history, &favorites);
    ok(json!({ "insights": insights }))
}

pub async fn chat(req: &Request, state: &Arc<AppState>) -> Response {
    if req.method != "POST" {
        return err(405, "method not allowed");
    }
    let payload: serde_json::Value = match serde_json::from_slice(&req.body) {
        Ok(p) => p,
        Err(e) => return err(400, format!("bad json: {e}")),
    };
    let message = payload
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if message.is_empty() {
        return err(400, "empty message");
    }

    let (history, favorites) = snapshot(state);
    // Phase 1 (sync): local answer while the AI lock is held.
    let draft = {
        let engine = match state.ai.lock() {
            Ok(e) => e,
            Err(_) => return err(500, "ai lock failed"),
        };
        chat::draft(&message, &engine, &history, &favorites)
    };
    // Phase 2 (lock released): optional LLM enhancement, graceful fallback.
    let reply = match state.llm_config() {
        Some(cfg) => chat::enhance_with_llm(&message, draft, &cfg, &state.http).await,
        None => draft.reply,
    };
    ok(json!({ "reply": reply }))
}

pub fn ai_settings(req: &Request, state: &Arc<AppState>) -> Response {
    match req.method.as_str() {
        "GET" => match state.ai_settings.lock() {
            Ok(s) => {
                let mut masked = s.clone();
                if masked.llm_api_key.len() > 4 {
                    let keep = masked.llm_api_key.clone();
                    masked.llm_api_key = format!("{}****", &keep[..2]);
                }
                ok(json!({ "settings": masked }))
            }
            Err(_) => err(500, "ai settings lock failed"),
        },
        "POST" => {
            let patch: serde_json::Value = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => return err(400, format!("bad json: {e}")),
            };
            match state.ai_settings.lock() {
                Ok(mut s) => {
                    if let Some(v) = patch.get("llm_enabled").and_then(|v| v.as_bool()) {
                        s.llm_enabled = v;
                    }
                    if let Some(v) = patch.get("llm_endpoint").and_then(|v| v.as_str()) {
                        s.llm_endpoint = v.trim().to_string();
                    }
                    // API key only overwritten when a non-masked value arrives.
                    if let Some(v) = patch.get("llm_api_key").and_then(|v| v.as_str()) {
                        if !v.contains("****") {
                            s.llm_api_key = v.trim().to_string();
                        }
                    }
                    if let Some(v) = patch.get("llm_model").and_then(|v| v.as_str()) {
                        s.llm_model = v.trim().to_string();
                    }
                    if let Some(v) = patch.get("local_enabled").and_then(|v| v.as_bool()) {
                        s.local_enabled = v;
                    }
                    s.save(&state.ai_settings_path);
                    ok(json!({ "ok": true, "llm_configured": s.llm_configured() }))
                }
                Err(_) => err(500, "ai settings lock failed"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}
