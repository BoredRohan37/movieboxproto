//! The MovieBox AI engine — local-first recommendations, semantic search,
//! mood browsing and viewing insights. Zero network required; optionally
//! upgraded by a user-configured LLM (see `llm.rs`).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::lexicon;
use crate::favorites::FavoriteItem;
use crate::history::WatchHistoryItem;
use crate::models::MediaType;
use crate::providers::models::CatalogItem;

/// One title the AI knows about. Learned passively as the user browses,
/// searches and opens details; persisted so knowledge survives restarts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexedItem {
    pub provider: String,
    pub id: String,
    pub title: String,
    /// 1 = movie, 2 = series (mirrors upstream stype).
    pub stype: i64,
    pub year: String,
    pub poster_url: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub rating: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub last_seen: u64,
}

impl IndexedItem {
    pub fn key(&self) -> String {
        format!("{}|{}", self.provider.to_ascii_lowercase(), self.id)
    }

    pub fn from_catalog_item(item: &CatalogItem) -> Self {
        Self {
            provider: item.id.provider.cache_key().to_string(),
            id: item.id.value.clone(),
            title: item.title.clone(),
            stype: if item.media_type == MediaType::Series {
                2
            } else {
                1
            },
            year: item.year.clone().unwrap_or_default(),
            poster_url: item.poster_url.clone(),
            genres: Vec::new(),
            rating: None,
            description: None,
            last_seen: now_secs(),
        }
    }

    pub fn media_type_label(&self) -> &'static str {
        if self.stype == 2 {
            "series"
        } else {
            "movie"
        }
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Weight of an interaction, by strength of signal.
const WEIGHT_FAVORITE: f64 = 3.0;
const WEIGHT_COMPLETED: f64 = 2.0;
const WEIGHT_IN_PROGRESS: f64 = 1.5;
const WEIGHT_STARTED: f64 = 1.0;
const HALF_LIFE_DAYS: f64 = 21.0;

/// A learned taste profile over genres, types and decades.
#[derive(Debug, Default, Clone)]
pub struct TasteProfile {
    pub genre_weights: HashMap<String, f64>,
    pub movie_share: f64,
    pub series_share: f64,
    pub decade_weights: HashMap<u16, f64>,
    pub total_signals: usize,
    /// Titles anchoring the strongest taste signals (for "Because you watched").
    pub anchors: Vec<String>,
}

impl TasteProfile {
    pub fn top_genres(&self, n: usize) -> Vec<(String, f64)> {
        let mut pairs: Vec<(String, f64)> = self
            .genre_weights
            .iter()
            .map(|(g, w)| (g.clone(), *w))
            .collect();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.truncate(n);
        pairs
    }

    pub fn top_decades(&self, n: usize) -> Vec<(u16, f64)> {
        let mut pairs: Vec<(u16, f64)> = self
            .decade_weights
            .iter()
            .map(|(d, w)| (*d, *w))
            .collect();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.truncate(n);
        pairs
    }

    /// Compact human-readable summary (used in LLM prompts too).
    pub fn summary(&self) -> String {
        if self.total_signals == 0 {
            return "No viewing history yet — open to anything.".to_string();
        }
        let genres = self
            .top_genres(5)
            .iter()
            .map(|(g, w)| format!("{} ({:.1})", g, w))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{} taste signals; favorite genres: {}; {:.0}% movies / {:.0}% series",
            self.total_signals,
            genres,
            self.movie_share * 100.0,
            self.series_share * 100.0
        )
    }
}

/// Persistence wrapper for the learned knowledge base.
#[derive(Debug, Default)]
pub struct AiEngine {
    knowledge: HashMap<String, IndexedItem>,
    knowledge_path: Option<PathBuf>,
}

impl AiEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_persistence(path: PathBuf) -> Self {
        let knowledge = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<IndexedItem>>(&bytes).ok())
            .map(|items| items.into_iter().map(|i| (i.key(), i)).collect())
            .unwrap_or_default();
        Self {
            knowledge,
            knowledge_path: Some(path),
        }
    }

    pub fn save(&self) {
        if let Some(path) = &self.knowledge_path {
            let items: Vec<&IndexedItem> = self.knowledge.values().collect();
            if let Ok(json) = serde_json::to_vec(&items) {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn learn_catalog(&mut self, items: &[CatalogItem]) {
        let now = now_secs();
        for item in items {
            let mut indexed = IndexedItem::from_catalog_item(item);
            let key = indexed.key();
            match self.knowledge.get_mut(&key) {
                Some(existing) => {
                    existing.last_seen = now;
                    if existing.poster_url.is_none() {
                        existing.poster_url = indexed.poster_url.take();
                    }
                    if existing.year.is_empty() {
                        existing.year = indexed.year;
                    }
                }
                None => {
                    self.knowledge.insert(key, indexed);
                }
            }
        }
    }

    /// Learn rich metadata when the user opens a details page.
    pub fn learn_details(&mut self, details: &crate::models::MediaDetails) {
        let mut item = IndexedItem::from_catalog_item(&CatalogItem {
            id: details.id.clone(),
            title: details.title.clone(),
            media_type: details.media_type,
            year: details.year.clone(),
            poster_url: details.poster_url.clone(),
            season_count: if details.seasons.is_empty() {
                None
            } else {
                Some(details.seasons.len())
            },
        });
        item.genres = details.genres.iter().map(|g| g.to_ascii_lowercase()).collect();
        item.rating = details.imdb_rating.clone();
        item.description = details.description.clone();
        item.last_seen = now_secs();
        self.knowledge.insert(item.key(), item);
    }

    /// Infer genres for catalog items from the homepage section they appeared
    /// in (e.g. section "Sci-Fi Future" → science fiction). Only fills items
    /// that have no genres yet — details data stays authoritative.
    pub fn learn_section_genres(&mut self, items: &[CatalogItem], genres: &[String]) {
        if genres.is_empty() {
            return;
        }
        for item in items {
            let key = format!(
                "{}|{}",
                item.id.provider.cache_key().to_ascii_lowercase(),
                item.id.value
            );
            if let Some(existing) = self.knowledge.get_mut(&key) {
                if existing.genres.is_empty() {
                    existing.genres = genres.to_vec();
                }
            }
        }
    }

    pub fn get(&self, provider: &str, id: &str) -> Option<&IndexedItem> {
        self.knowledge
            .get(&format!("{}|{}", provider.to_ascii_lowercase(), id))
    }

    pub fn knowledge_len(&self) -> usize {
        self.knowledge.len()
    }

    pub fn all(&self) -> Vec<IndexedItem> {
        let mut items: Vec<IndexedItem> = self.knowledge.values().cloned().collect();
        items.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        items
    }

    // ------------------------------------------------------------------
    // Taste profile
    // ------------------------------------------------------------------

    pub fn taste_profile(&self, history: &[WatchHistoryItem], favorites: &[FavoriteItem]) -> TasteProfile {
        let mut profile = TasteProfile::default();
        let now = now_secs();

        let mut apply_signal =
            |profile: &mut TasteProfile,
             title: &str,
             stype: i64,
             year: &str,
             genres: &[String],
             weight: f64,
             ts: u64| {
                let days = ((now.saturating_sub(ts)) / 86400) as f64;
                let decay = 0.5f64.powf(days / HALF_LIFE_DAYS);
                let w = weight * decay;
                if genres.is_empty() {
                    // No genre data — still count type/decade signals.
                } else {
                    for g in genres {
                        *profile.genre_weights.entry(g.clone()).or_default() += w;
                    }
                }
                if stype == 2 {
                    profile.series_share += w;
                } else {
                    profile.movie_share += w;
                }
                if let Some(decade) = decade_of_year(year) {
                    *profile.decade_weights.entry(decade).or_default() += w;
                }
                profile.total_signals += 1;
                if profile.anchors.len() < 6 {
                    profile.anchors.push(title.to_string());
                }
            };

        for fav in favorites {
            let genres = self
                .get(&fav.provider, &fav.subject_id)
                .map(|i| i.genres.clone())
                .unwrap_or_default();
            apply_signal(&mut profile, &fav.title, fav.stype, &fav.release_year, &genres, WEIGHT_FAVORITE, fav.added_at);
        }

        for h in history {
            let weight = if h.completed {
                WEIGHT_COMPLETED
            } else if h.progress_seconds > 0 {
                WEIGHT_IN_PROGRESS
            } else {
                WEIGHT_STARTED
            };
            let genres = self
                .get(&h.provider, &h.subject_id)
                .map(|i| i.genres.clone())
                .unwrap_or_default();
            apply_signal(&mut profile, &h.title, h.stype, &h.release_year, &genres, weight, h.timestamp);
        }

        let total_type = profile.movie_share + profile.series_share;
        if total_type > 0.0 {
            profile.movie_share /= total_type;
            profile.series_share /= total_type;
        }
        profile
    }

    // ------------------------------------------------------------------
    // Recommendations
    // ------------------------------------------------------------------

    /// Score a candidate against the profile. Returns None when the item
    /// should be excluded (already watched, missing data).
    pub fn score_candidate(
        &self,
        item: &IndexedItem,
        profile: &TasteProfile,
        exclude_keys: &std::collections::HashSet<String>,
        seed: u64,
    ) -> Option<f64> {
        if exclude_keys.contains(&item.key()) {
            return None;
        }
        let mut score = 0.0;

        // Genre affinity — the dominant factor when we know genres.
        if !item.genres.is_empty() {
            let max_weight = profile
                .genre_weights
                .values()
                .copied()
                .fold(0.0f64, f64::max);
            let mut genre_score = 0.0;
            for g in &item.genres {
                if let Some(w) = profile.genre_weights.get(g) {
                    genre_score += w;
                }
            }
            if max_weight > 0.0 {
                score += 0.55 * (genre_score / (max_weight * 2.0)).min(1.0);
            } else {
                // Cold start: nudge well-known crowd-pleaser genres lightly.
                score += 0.15;
            }
        } else {
            score += 0.1; // unknown genre — mild neutral
        }

        // Type affinity
        let type_affinity = if item.stype == 2 {
            profile.series_share
        } else {
            profile.movie_share
        };
        score += 0.2 * type_affinity;

        // Decade affinity
        if let Some(decade) = decade_of_year(&item.year) {
            if let Some(w) = profile.decade_weights.get(&decade) {
                score += 0.1 * (w.min(3.0) / 3.0);
            }
        }

        // Recency of content (newer titles slightly preferred)
        if let Ok(y) = item.year.parse::<i32>() {
            let age = (2026 - y).clamp(0, 60) as f64;
            score += 0.1 * (1.0 - age / 60.0);
        }

        // Deterministic serendipity: daily-varying jitter so rows feel alive
        // but don't reshuffle on every request.
        let jitter = ((seed.wrapping_mul(0x9E3779B97F4A7C15))
            .wrapping_add(item.id.len() as u64 * 31)
            % 1000) as f64
            / 1000.0;
        score += 0.05 * jitter;

        if score <= 0.0 {
            None
        } else {
            Some(score)
        }
    }

    /// Personalized recommendations ("For You").
    pub fn recommend(
        &self,
        profile: &TasteProfile,
        history: &[WatchHistoryItem],
        favorites: &[FavoriteItem],
        limit: usize,
    ) -> Vec<ScoredItem> {
        let exclude = Self::exclude_set(history, favorites);
        let seed = (now_secs() / 86400) as u64;

        let mut scored: Vec<ScoredItem> = self
            .knowledge
            .values()
            .filter_map(|item| {
                self.score_candidate(item, profile, &exclude, seed.wrapping_add(item.key().len() as u64))
                    .map(|s| ScoredItem {
                        item: item.clone(),
                        score: s,
                        reason: self.reason_for(item, profile),
                    })
            })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    /// "Because you watched <anchor>" rows built from the strongest anchors.
    pub fn because_you_watched(
        &self,
        history: &[WatchHistoryItem],
        favorites: &[FavoriteItem],
        limit: usize,
    ) -> Vec<(String, Vec<ScoredItem>)> {
        let exclude = Self::exclude_set(history, favorites);
        let seed = (now_secs() / 86400) as u64;
        let mut rows = Vec::new();

        // Strongest anchors = completed / favorite titles first.
        let mut anchors: Vec<&WatchHistoryItem> = history.iter().collect();
        anchors.sort_by(|a, b| {
            let ka = if a.completed { 2 } else { 1 };
            let kb = if b.completed { 2 } else { 1 };
            kb.cmp(&ka).then(b.timestamp.cmp(&a.timestamp))
        });

        let mut used_keys = exclude.clone();
        for anchor in anchors.iter().take(3) {
            let anchor_genres = self
                .get(&anchor.provider, &anchor.subject_id)
                .map(|i| i.genres.clone())
                .unwrap_or_default();
            if anchor_genres.is_empty() {
                continue;
            }
            let mut fake_profile = TasteProfile::default();
            for g in &anchor_genres {
                fake_profile.genre_weights.insert(g.clone(), 3.0);
                fake_profile.total_signals += 1;
            }
            fake_profile.anchors.push(anchor.title.clone());

            let mut items: Vec<ScoredItem> = self
                .knowledge
                .values()
                .filter(|cand| {
                    cand.id != anchor.subject_id
                        && !used_keys.contains(&cand.key())
                        && !cand.genres.is_empty()
                })
                .filter_map(|cand| {
                    self.score_candidate(
                        cand,
                        &fake_profile,
                        &used_keys,
                        seed.wrapping_add(anchor.subject_id.len() as u64),
                    )
                    .map(|s| ScoredItem {
                        item: cand.clone(),
                        score: s,
                        reason: format!("Shares {} with {}", anchor_genres.join(", "), anchor.title),
                    })
                })
                .collect();
            items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            items.truncate(limit);
            if items.len() >= 4 {
                for it in &items {
                    used_keys.insert(it.item.key());
                }
                rows.push((anchor.title.clone(), items));
            }
        }
        rows
    }

    fn exclude_set(
        history: &[WatchHistoryItem],
        favorites: &[FavoriteItem],
    ) -> std::collections::HashSet<String> {
        let mut set = std::collections::HashSet::new();
        for h in history {
            // Only exclude when meaningfully watched (>10% or completed).
            let meaningful = h.completed
                || h.progress_seconds > 60
                || h.duration_seconds
                    .map(|d| h.progress_seconds as f64 / d.max(1) as f64 > 0.1)
                    .unwrap_or(false);
            if meaningful {
                set.insert(format!("{}|{}", h.provider.to_ascii_lowercase(), h.subject_id));
            }
        }
        for f in favorites {
            set.insert(format!("{}|{}", f.provider.to_ascii_lowercase(), f.subject_id));
        }
        set
    }

    fn reason_for(&self, item: &IndexedItem, profile: &TasteProfile) -> String {
        let top = profile.top_genres(3);
        let matched: Vec<&String> = item
            .genres
            .iter()
            .filter(|g| top.iter().any(|(tg, _)| tg.eq_ignore_ascii_case(g)))
            .collect();
        if !matched.is_empty() {
            format!("Matches your taste for {}", matched.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" & "))
        } else if !item.genres.is_empty() {
            format!("Popular {} — a fresh pick outside your usual", item.genres.join("/"))
        } else {
            "Trending on MovieBox".to_string()
        }
    }

    // ------------------------------------------------------------------
    // Semantic search
    // ------------------------------------------------------------------

    /// Understand a natural-language query and score the known catalog.
    /// Returns parsed intent + scored local matches.
    pub fn semantic_search(
        &self,
        query: &str,
        profile: &TasteProfile,
        limit: usize,
    ) -> SemanticSearchResult {
        let q = query.trim();
        let lower = q.to_ascii_lowercase();

        let mut wanted_genres = lexicon::expand_genres(&lower);
        let decade = lower
            .split_whitespace()
            .find_map(lexicon::decade_from_token);
        let wants_series = ["series", "show", "shows", "tv", "episode", "season"]
            .iter()
            .any(|w| lower.contains(w));
        let wants_movie = ["movie", "film", "flick"]
            .iter()
            .any(|w| lower.contains(w));

        let core = lexicon::strip_intent(q);
        let core_lower = core.to_ascii_lowercase();

        let mut results: Vec<ScoredItem> = Vec::new();
        for item in self.knowledge.values() {
            let mut score = 0.0f64;
            let mut matched_genres = Vec::new();

            for g in &wanted_genres {
                if item
                    .genres
                    .iter()
                    .any(|ig| ig.contains(g.as_str()) || g.contains(ig.as_str()))
                {
                    score += 0.4;
                    matched_genres.push(g.clone());
                }
            }

            // Title match (fuzzy: token overlap + substring)
            if !core_lower.is_empty() {
                let title_lower = item.title.to_ascii_lowercase();
                if title_lower.contains(&core_lower) {
                    score += 1.2;
                } else {
                    let t_tokens: Vec<&str> = title_lower.split_whitespace().collect();
                    let q_tokens: Vec<&str> = core_lower.split_whitespace().collect();
                    let overlap = q_tokens
                        .iter()
                        .filter(|qt| t_tokens.iter().any(|tt| tt.starts_with(*qt)))
                        .count();
                    if overlap > 0 {
                        score += 0.5 * overlap as f64 / q_tokens.len() as f64;
                    }
                }
                // Description keywords
                if let Some(desc) = &item.description {
                    let d = desc.to_ascii_lowercase();
                    for token in core_lower.split_whitespace() {
                        if token.len() >= 4 && d.contains(token) {
                            score += 0.15;
                        }
                    }
                }
            }

            // Decade / type filters
            if let Some(dec) = decade {
                match decade_of_year(&item.year) {
                    Some(d) if d == dec => score += 0.35,
                    Some(_) => continue, // hard filter
                    None => {}
                }
            }
            if wants_series && item.stype != 2 && (wants_movie || !wanted_genres.is_empty()) {
                // soft penalty only when intent is explicit both ways
                score -= 0.1;
            }
            if wants_movie && item.stype == 2 && (wants_series || !wanted_genres.is_empty()) {
                score -= 0.1;
            }

            if score <= 0.05 {
                continue;
            }
            results.push(ScoredItem {
                item: item.clone(),
                score,
                reason: if !matched_genres.is_empty() {
                    format!("{} — matches your ask", matched_genres.join(", "))
                } else {
                    "Title match".to_string()
                },
            });
        }
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Genre-only queries (no title signal): blend taste ranking within genre
        if core_lower.is_empty() && !wanted_genres.is_empty() {
            for r in &mut results {
                let taste = profile
                    .genre_weights
                    .iter()
                    .filter(|(g, _)| {
                        r.item
                            .genres
                            .iter()
                            .any(|ig| ig.contains(g.as_str()) || g.contains(ig.as_str()))
                    })
                    .map(|(_, w)| w)
                    .sum::<f64>();
                r.score += taste * 0.05;
            }
            results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        }

        let mut interpreted = Vec::new();
        if !wanted_genres.is_empty() {
            interpreted.push(format!("genres: {}", wanted_genres.join(", ")));
        }
        if let Some(d) = decade {
            interpreted.push(format!("decade: {}s", d));
        }
        if wants_series && !wants_movie {
            interpreted.push("type: series".into());
        }
        if wants_movie && !wants_series {
            interpreted.push("type: movies".into());
        }
        if !core.is_empty() {
            interpreted.push(format!("keywords: \"{}\"", core));
        }

        results.truncate(limit);
        SemanticSearchResult {
            interpreted: interpreted.join(" · "),
            wanted_genres,
            core_query: core,
            results,
        }
    }

    // ------------------------------------------------------------------
    // Moods & insights
    // ------------------------------------------------------------------

    pub fn mood_row(&self, mood_id: &str, limit: usize) -> Option<(String, Vec<ScoredItem>)> {
        let (label, genres) = lexicon::MOODS
            .iter()
            .find(|(id, _, _)| *id == mood_id)
            .map(|(_, l, g)| (*l, g.iter().map(|s| s.to_string()).collect::<Vec<_>>()))?;
        let mut items: Vec<ScoredItem> = self
            .knowledge
            .values()
            .filter(|i| {
                !i.genres.is_empty()
                    && i.genres.iter().any(|ig| {
                        genres
                            .iter()
                            .any(|g| ig.contains(g.as_str()) || g.contains(ig.as_str()))
                    })
            })
            .map(|i| {
                ScoredItem {
                    item: i.clone(),
                    score: 1.0,
                    reason: label.to_string(),
                }
            })
            .collect();
        // Prefer higher-rated within mood
        items.sort_by(|a, b| {
            let ra = a.item.rating.as_deref().and_then(|r| r.parse::<f64>().ok()).unwrap_or(0.0);
            let rb = b.item.rating.as_deref().and_then(|r| r.parse::<f64>().ok()).unwrap_or(0.0);
            rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(limit);
        Some((label.to_string(), items))
    }

    pub fn insights(&self, history: &[WatchHistoryItem], favorites: &[FavoriteItem]) -> Insights {
        let now = now_secs();
        let total_titles: std::collections::HashSet<&str> = history
            .iter()
            .map(|h| h.subject_id.as_str())
            .collect();
        let watch_seconds: u64 = history.iter().map(|h| h.progress_seconds).sum();
        let completed = history.iter().filter(|h| h.completed).count();
        let in_progress = history
            .iter()
            .filter(|h| h.is_in_progress())
            .count();
        let mut by_day: HashMap<u64, u64> = HashMap::new();
        for h in history {
            let day = h.timestamp / 86400;
            *by_day.entry(day).or_default() += h.progress_seconds;
        }
        let active_days = by_day.len();
        // Streak: consecutive days ending today/yesterday with any watch time.
        let today = now / 86400;
        let mut streak = 0u32;
        let mut cursor = today;
        if !by_day.contains_key(&cursor) {
            cursor = cursor.saturating_sub(1);
        }
        while by_day.contains_key(&cursor) {
            streak += 1;
            cursor = cursor.saturating_sub(1);
        }
        let profile = self.taste_profile(history, favorites);
        Insights {
            unique_titles: total_titles.len(),
            watch_seconds,
            completed,
            in_progress,
            active_days,
            streak_days: streak,
            favorite_genres: profile.top_genres(5),
            top_titles: history
                .iter()
                .take(5)
                .map(|h| h.title.clone())
                .collect(),
            summary: profile.summary(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoredItem {
    #[serde(flatten)]
    pub item: IndexedItem,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticSearchResult {
    pub interpreted: String,
    pub wanted_genres: Vec<String>,
    pub core_query: String,
    pub results: Vec<ScoredItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Insights {
    pub unique_titles: usize,
    pub watch_seconds: u64,
    pub completed: usize,
    pub in_progress: usize,
    pub active_days: usize,
    pub streak_days: u32,
    pub favorite_genres: Vec<(String, f64)>,
    pub top_titles: Vec<String>,
    pub summary: String,
}

fn decade_of_year(year: &str) -> Option<u16> {
    let y: u16 = year.trim().get(0..4)?.parse().ok()?;
    Some(y - y % 10)
}
