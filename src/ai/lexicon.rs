//! Genre / mood lexicon powering semantic understanding.
//!
//! Maps colloquial phrases ("funny", "mind-bending", "90s flick") to the genre
//! vocabularies used by the providers, and defines curated mood rows.

/// Mood → genre keywords. Keys are exposed as AI mood rows in the GUI.
pub const MOODS: &[(&str, &str, &[&str])] = &[
    (
        "feel-good",
        "Feel-Good & Uplifting",
        &["comedy", "family", "romance", "music", "feel-good"],
    ),
    (
        "adrenaline",
        "High-Octane Adrenaline",
        &["action", "thriller", "adventure", "war", "crime"],
    ),
    (
        "mind-bending",
        "Mind-Bending Picks",
        &["science fiction", "sci-fi", "mystery", "thriller", "surreal"],
    ),
    (
        "spooky",
        "Spooky Nights",
        &["horror", "thriller", "supernatural", "slasher", "monster"],
    ),
    (
        "cozy",
        "Cozy & Comfort Watch",
        &["animation", "family", "comedy", "fantasy", "romance"],
    ),
    (
        "tearjerker",
        "Emotional Tearjerkers",
        &["drama", "romance", "music", "biography"],
    ),
    (
        "laugh-out-loud",
        "Laugh Out Loud",
        &["comedy", "satire", "sitcom", "parody"],
    ),
    (
        "true-crime",
        "True Crime & Mystery",
        &["crime", "documentary", "mystery", "thriller", "investigation"],
    ),
    (
        "epic",
        "Epics & Adventures",
        &["adventure", "fantasy", "action", "history", "epic"],
    ),
    (
        "anime-vibes",
        "Anime Vibes",
        &["anime", "animation", "shounen", "isekai"],
    ),
];

/// Colloquial phrase → canonical genre(s). Used by semantic search.
pub const SYNONYMS: &[(&str, &[&str])] = &[
    ("funny", &["comedy"]),
    ("hilarious", &["comedy"]),
    ("laugh", &["comedy"]),
    ("comedy", &["comedy"]),
    ("scary", &["horror"]),
    ("frightening", &["horror"]),
    ("creepy", &["horror", "thriller"]),
    ("horror", &["horror"]),
    ("spooky", &["horror", "supernatural"]),
    ("romantic", &["romance"]),
    ("love story", &["romance", "drama"]),
    ("romance", &["romance"]),
    ("sad", &["drama"]),
    ("emotional", &["drama"]),
    ("tearjerker", &["drama", "romance"]),
    ("action", &["action"]),
    ("fight", &["action", "martial arts"]),
    ("martial arts", &["martial arts", "action"]),
    ("karate", &["martial arts"]),
    ("kung fu", &["martial arts"]),
    ("space", &["science fiction", "sci-fi"]),
    ("alien", &["science fiction", "sci-fi"]),
    ("sci-fi", &["science fiction"]),
    ("science fiction", &["science fiction"]),
    ("future", &["science fiction"]),
    ("dystopia", &["science fiction", "drama"]),
    ("dragon", &["fantasy"]),
    ("magic", &["fantasy"]),
    ("wizard", &["fantasy"]),
    ("fantasy", &["fantasy"]),
    ("superhero", &["action", "adventure", "superhero"]),
    ("detective", &["mystery", "crime"]),
    ("whodunit", &["mystery", "crime"]),
    ("murder", &["mystery", "crime", "thriller"]),
    ("mystery", &["mystery"]),
    ("thriller", &["thriller"]),
    ("suspense", &["thriller", "mystery"]),
    ("crime", &["crime"]),
    ("gangster", &["crime", "drama"]),
    ("mafia", &["crime", "drama"]),
    ("heist", &["crime", "thriller"]),
    ("war", &["war", "history"]),
    ("military", &["war", "action"]),
    ("history", &["history"]),
    ("historical", &["history"]),
    ("period drama", &["history", "drama", "romance"]),
    ("kids", &["family", "animation"]),
    ("children", &["family", "animation"]),
    ("family", &["family"]),
    ("cartoon", &["animation"]),
    ("animated", &["animation"]),
    ("anime", &["anime", "animation"]),
    ("manga", &["anime"]),
    ("documentary", &["documentary"]),
    ("docuseries", &["documentary"]),
    ("true story", &["biography", "drama"]),
    ("biopic", &["biography"]),
    ("music", &["music"]),
    ("musical", &["music", "musical"]),
    ("dance", &["music", "drama"]),
    ("sports", &["sport"]),
    ("football", &["sport", "drama"]),
    ("basketball", &["sport", "drama"]),
    ("zombie", &["horror", "zombie"]),
    ("vampire", &["horror", "fantasy"]),
    ("monster", &["horror", "monster"]),
    ("western", &["western"]),
    ("cowboy", &["western"]),
    ("spy", &["thriller", "action", "spy"]),
    ("espionage", &["spy", "thriller"]),
    ("court", &["drama", "crime"]),
    ("legal", &["drama", "crime"]),
    ("hospital", &["drama", "medical"]),
    ("medical", &["medical", "drama"]),
    ("school", &["drama", "comedy", "teen"]),
    ("teen", &["teen", "drama", "romance"]),
    ("high school", &["teen", "drama", "comedy"]),
    ("k-drama", &["korean", "drama", "romance"]),
    ("korean", &["korean"]),
    ("bollywood", &["bollywood"]),
    ("hindi", &["hindi"]),
    ("tollywood", &["tollywood"]),
];

/// Intent words that hint at what the user wants from search.
pub const INTENT_WORDS: &[&str] = &[
    "movie", "movies", "film", "films", "show", "shows", "series", "tv",
    "something", "watch", "recommend", "suggest", "find", "good", "best",
    "new", "recent", "classic", "old", "give", "me", "a", "an", "the",
    "want", "id", "i", "like", "to", "please", "tonight", "now", "some",
    "any", "my", "for", "with", "of", "in", "on", "that", "this", "it",
    "is", "are", "whats", "what's", "got", "get", "looking", "look",
];

/// Decade hints: "90s", "1990s", "eighties".
pub fn decade_from_token(token: &str) -> Option<u16> {
    let raw = token.trim().to_ascii_lowercase();
    let words: &[(&str, u16)] = &[
        ("twenties", 2020),
        ("twenty-twenties", 2020),
        ("twenty-tens", 2010),
        ("two-thousands", 2000),
        ("naughties", 2000),
        ("nineties", 1990),
        ("eighties", 1980),
        ("seventies", 1970),
        ("sixties", 1960),
        ("fifties", 1950),
    ];
    for (w, d) in words {
        if raw.eq_ignore_ascii_case(w) {
            return Some(*d);
        }
    }
    // Digit forms: trim trailing "s" ("90s", "1990s").
    let t = raw.trim_end_matches(['s', 'S']);
    if t.len() == 2 && t.chars().all(|c| c.is_ascii_digit()) {
        // "90s" → trimmed to "90"; map 30..=99 to 1900s, 0..=29 to 2000s.
        if let Ok(n) = t.parse::<u16>() {
            return match n {
                30..=99 => Some(1900 + n),
                0..=29 => Some(2000 + n),
                _ => None,
            };
        }
    }
    if t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(y) = t.parse::<u16>() {
            if (1920..=2035).contains(&y) {
                return Some(y - y % 10);
            }
        }
    }
    None
}

/// Expanded genre set for a raw user query.
pub fn expand_genres(query: &str) -> Vec<String> {
    let q = query.to_ascii_lowercase();
    let mut found = Vec::new();
    for (phrase, genres) in SYNONYMS {
        if q.contains(phrase) {
            for g in *genres {
                if !found.iter().any(|f: &String| f.eq_ignore_ascii_case(g)) {
                    found.push(g.to_string());
                }
            }
        }
    }
    found
}

/// Strip intent filler words, returning the "core" of the query.
pub fn strip_intent(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|w| !INTENT_WORDS.contains(&w.to_ascii_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decades() {
        assert_eq!(decade_from_token("90s"), Some(1990));
        assert_eq!(decade_from_token("1990s"), Some(1990));
        assert_eq!(decade_from_token("2020s"), Some(2020));
        assert_eq!(decade_from_token("eighties"), Some(1980));
        assert_eq!(decade_from_token("banana"), None);
    }

    #[test]
    fn expand() {
        let g = expand_genres("something funny and scary for tonight");
        assert!(g.contains(&"comedy".to_string()));
        assert!(g.contains(&"horror".to_string()));
    }

    #[test]
    fn strip() {
        assert_eq!(strip_intent("give me a good action movie"), "action");
        assert_eq!(strip_intent("dune"), "dune");
    }
}
