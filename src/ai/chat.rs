//! The MovieBox AI assistant.
//!
//! Local intent engine answers instantly and offline from the live catalog +
//! watch history. When an LLM is configured, the local answer is used as the
//! factual context ("grounding") and the model polishes/extends it — so the
//! assistant never hallucinates titles that aren't in the app.

use serde::{Deserialize, Serialize};

use super::engine::{AiEngine, ScoredItem};
use super::llm::{self, LlmConfig, LlmTurn};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub struct ChatContext<'a> {
    pub engine: &'a AiEngine,
    pub history: &'a [crate::history::WatchHistoryItem],
    pub favorites: &'a [crate::favorites::FavoriteItem],
}

#[derive(Debug, Serialize)]
pub struct ChatReply {
    pub answer: String,
    pub source: &'static str,
    pub suggestions: Vec<String>,
    /// Items the answer references (for rich cards in the GUI).
    pub items: Vec<ScoredItem>,
}

const SYSTEM_PROMPT: &str = "\
You are MovieBox AI, the friendly concierge of a movie & TV streaming app. \
You recommend titles using ONLY the grounding data provided (known catalog and \
watch history). Never invent titles that are not in the grounding data. \
Keep answers under 120 words, warm and cinematic, no markdown lists of more \
than 5 items, end with a short inviting question when natural.";

pub struct ChatDraft {
    pub reply: ChatReply,
    pub grounding: String,
}

/// Phase 1 (sync, no awaits): compute the local answer + grounding while
/// the caller holds whatever locks are needed. Phase 2 (`enhance_with_llm`)
/// runs without holding any lock.
pub fn draft(
    message: &str,
    engine: &AiEngine,
    history: &[crate::history::WatchHistoryItem],
    favorites: &[crate::favorites::FavoriteItem],
) -> ChatDraft {
    let ctx = ChatContext {
        engine,
        history,
        favorites,
    };
    let reply = local_answer(message, &ctx);
    let grounding = build_grounding(&ctx);
    ChatDraft { reply, grounding }
}

pub async fn enhance_with_llm(
    message: &str,
    draft: ChatDraft,
    cfg: &LlmConfig,
    client: &reqwest::Client,
) -> ChatReply {
    let turns = vec![
        LlmTurn::system(SYSTEM_PROMPT),
        LlmTurn::system(format!("Grounding data:\n{}", draft.grounding)),
        LlmTurn::user(format!(
            "User message: {message}\n\nLocal engine draft (factual, use it): {}",
            draft.reply.answer
        )),
    ];
    match llm::complete(client, cfg, &turns, 300).await {
        Ok(text) => ChatReply {
            answer: text,
            source: "llm",
            suggestions: draft.reply.suggestions,
            items: draft.reply.items,
        },
        Err(_) => draft.reply, // graceful degradation to the local answer
    }
}

fn build_grounding(ctx: &ChatContext<'_>) -> String {
    let profile = ctx.engine.taste_profile(ctx.history, ctx.favorites);
    let mut out = String::new();
    out.push_str(&format!("Taste: {}\n", profile.summary()));

    let in_progress: Vec<String> = ctx
        .history
        .iter()
        .filter(|h| h.is_in_progress())
        .take(5)
        .map(|h| {
            format!(
                "{} S{}E{} ({}%)",
                h.title,
                h.season,
                h.episode,
                h.progress_percentage().map(|p| format!("{:.0}", p * 100.0)).unwrap_or_default()
            )
        })
        .collect();
    if !in_progress.is_empty() {
        out.push_str(&format!("Continue watching: {}\n", in_progress.join("; ")));
    }

    let recs = ctx.engine.recommend(&profile, ctx.history, ctx.favorites, 8);
    let rec_str = recs
        .iter()
        .map(|r| {
            let genres = r.item.genres.join("/");
            let year = r.item.year.clone();
            format!("{} [{} {}{}] — {}", r.item.title, r.item.media_type_label(), year, if genres.is_empty() { String::new() } else { format!(" {genres}") }, r.reason)
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push_str(&format!("Available recommendations:\n{rec_str}\n"));
    out
}

fn local_answer(message: &str, ctx: &ChatContext<'_>) -> ChatReply {
    let lower = message.to_ascii_lowercase();
    let profile = ctx.engine.taste_profile(ctx.history, ctx.favorites);

    // Intent: continue watching
    if lower.contains("continue") || lower.contains("resume") || lower.contains("left off") {
        let items: Vec<ScoredItem> = ctx
            .history
            .iter()
            .filter(|h| h.is_in_progress())
            .take(6)
            .map(|h| ScoredItem {
                item: ctx
                    .engine
                    .get(&h.provider, &h.subject_id)
                    .cloned()
                    .unwrap_or_else(|| super::engine::IndexedItem {
                        provider: h.provider.clone(),
                        id: h.subject_id.clone(),
                        title: h.title.clone(),
                        stype: h.stype,
                        year: h.release_year.clone(),
                        poster_url: h.cover_url.clone(),
                        ..Default::default()
                    }),
                score: 1.0,
                reason: format!(
                    "Paused at {}",
                    h.formatted_progress()
                ),
            })
            .collect();
        let answer = if items.is_empty() {
            "You're all caught up — nothing in progress. Want me to recommend something new tonight?"
                .to_string()
        } else {
            let names: Vec<String> = items
                .iter()
                .take(3)
                .map(|i| {
                    format!("{} ({})", i.item.title, i.reason.replace("Paused at ", ""))
                })
                .collect();
            format!(
                "Picking up where you left off: {}. Shall I open one of them?",
                names.join(", ")
            )
        };
        return ChatReply {
            answer,
            source: "local",
            suggestions: vec![
                "What should I watch tonight?".into(),
                "Show my viewing stats".into(),
            ],
            items,
        };
    }

    // Intent: stats / wrap
    if lower.contains("stat") || lower.contains("wrap") || lower.contains("how much") || lower.contains("streak") {
        let ins = ctx.engine.insights(ctx.history, ctx.favorites);
        let hours = ins.watch_seconds as f64 / 3600.0;
        let answer = format!(
            "Here's your MovieBox wrap: {} titles explored, {:.1} hours watched, {} finished, {} in progress, and a {}-day streak 🔥. Your flavor leans {}.",
            ins.unique_titles,
            hours,
            ins.completed,
            ins.in_progress,
            ins.streak_days,
            ins
                .favorite_genres
                .first()
                .map(|(g, _)| g.as_str())
                .unwrap_or("curious")
        );
        return ChatReply {
            answer,
            source: "local",
            suggestions: vec![
                "Recommend something in my top genre".into(),
                "What are the top moods?".into(),
            ],
            items: Vec::new(),
        };
    }

    // Intent: mood / genre / recommendation — all flow through semantic search
    let mood_match = super::lexicon::MOODS
        .iter()
        .find(|(id, label, _)| lower.contains(id) || lower.contains(&label.to_ascii_lowercase()));

    if let Some((id, label, _)) = mood_match {
        if let Some((_, items)) = ctx.engine.mood_row(id, 6) {
            let names: Vec<String> = items
                .iter()
                .take(3)
                .map(|i| i.item.title.clone())
                .collect();
            return ChatReply {
                answer: format!(
                    "Feeling {label}? I've got you: {}. Tap a card and enjoy.",
                    names.join(", ")
                ),
                source: "local",
                suggestions: vec![
                    "Something funnier".into(),
                    "Something mind-bending".into(),
                ],
                items,
            };
        }
    }

    let semantic = ctx.engine.semantic_search(message, &profile, 8);
    let wants_reco = ["recommend", "suggest", "what should", "watch tonight", "surprise", "good"]
        .iter()
        .any(|w| lower.contains(w));

    if !semantic.results.is_empty() {
        let names: Vec<String> = semantic
            .results
            .iter()
            .take(if wants_reco { 4 } else { 3 })
            .map(|i| {
                let y = if i.item.year.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", i.item.year)
                };
                format!("{}{}", i.item.title, y)
            })
            .collect();
        let answer = if wants_reco || semantic.core_query.is_empty() {
            format!(
                "Based on your taste ({}), I'd pick {}. {}\nWant more like this?",
                profile.summary().split(';').nth(1).unwrap_or("your genres").trim(),
                names.join(", "),
                semantic.interpreted
            )
        } else {
            format!(
                "Closest matches for \"{}\": {}. {}",
                semantic.core_query,
                names.join(", "),
                semantic.interpreted
            )
        };
        return ChatReply {
            answer,
            source: "local",
            suggestions: vec![
                "What should I watch tonight?".into(),
                "Show my viewing stats".into(),
            ],
            items: semantic.results,
        };
    }

    // Intent: recommend from empty knowledge
    let recs = ctx.engine.recommend(&profile, ctx.history, ctx.favorites, 5);
    if !recs.is_empty() {
        let names: Vec<String> = recs.iter().take(3).map(|r| r.item.title.clone()).collect();
        return ChatReply {
            answer: format!(
                "Fresh picks tuned to you: {}. Give one a try — I'll learn from what you enjoy.",
                names.join(", ")
            ),
            source: "local",
            suggestions: vec![
                "Something funny".into(),
                "Continue watching".into(),
            ],
            items: recs,
        };
    }

    ChatReply {
        answer: "I'm still learning your taste — search or play a few titles and I'll \
                 craft personal picks. Meanwhile, try the Trending row or ask me for a mood \
                 like \"something mind-bending\"."
            .to_string(),
        source: "local",
        suggestions: vec![
            "Trending tonight".into(),
            "Something funny".into(),
            "Continue watching".into(),
        ],
        items: Vec::new(),
    }
}
