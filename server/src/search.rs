//! One search, structured (decision 0072): a query read one way, on the server as in the browser (web/src/lib/search.ts,
//! both held to web/tests/fixtures/search-words.json), and answered by kind, deep, in one shape (`app::search`).
//!
//! A query's words: Unicode NFKD (a full-width letter, a ligature, a Roman numeral made plain), combining marks dropped
//! (as the library's FTS tables' `remove_diacritics 2`), case folded, everything but letters and digits a space between
//! words. No operators: quotes and FTS syntax are words like any other.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// The words of a query, as every search reads them.
pub fn words(q: &str) -> Vec<String> {
    let plain: String = q.nfkd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect();
    plain.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// The FTS5 query for these words: every one (AND), each as a word or a word's start (`"w" OR "w"*`). None without words.
pub fn fts(words: &[String]) -> Option<String> {
    (!words.is_empty()).then(|| words.iter().map(|w| format!("(\"{w}\" OR \"{w}\"*)")).collect::<Vec<_>>().join(" AND "))
}

/// Whether a text holds every word, each as a word or a word's start, read as the query is (for what the FTS tables do
/// not hold: playlists' and crates' names, genres).
pub fn matches(text: &str, words: &[String]) -> bool {
    let have = self::words(text);
    !words.is_empty() && words.iter().all(|w| have.iter().any(|h| h.starts_with(w.as_str())))
}

/// How well a text matches the words, for ranking (higher is better): the whole text the query, 3; every word whole,
/// 2; some words only as a start, 1 (Navidrome's lesson: a whole name above a prefix); not at all, 0.
pub fn closeness(text: &str, words: &[String]) -> u8 {
    let have = self::words(text);
    if words.is_empty() || !words.iter().all(|w| have.iter().any(|h| h.starts_with(w.as_str()))) {
        0
    } else if have == words {
        3
    } else if words.iter().all(|w| have.contains(w)) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Example {
        q: String,
        words: Vec<String>,
    }

    #[test]
    fn a_query_is_read_as_the_pages_read_it() {
        // The examples the web's lib/search.ts is held to as well (decision 0072).
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/tests/fixtures/search-words.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("the shared examples ({}): {e}; run with the repository mounted", path.display()));
        let examples: Vec<Example> = serde_json::from_str(&text).unwrap();
        assert!(examples.len() >= 10);
        for e in examples {
            assert_eq!(words(&e.q), e.words, "{:?}", e.q);
        }
    }

    #[test]
    fn the_fts_query_wants_every_word_whole_or_begun_and_nothing_else() {
        assert_eq!(fts(&words("Björk Post")).as_deref(), Some("(\"bjork\" OR \"bjork\"*) AND (\"post\" OR \"post\"*)"));
        assert_eq!(fts(&words("\" *")), None);
    }

    #[test]
    fn a_whole_name_ranks_above_whole_words_above_a_start() {
        let q = words("air");
        assert_eq!(closeness("Air", &q), 3);
        assert_eq!(closeness("Air Supply", &q), 2);
        assert_eq!(closeness("Airbag", &q), 1);
        assert_eq!(closeness("Hair", &q), 0, "a word's start only, never inside it");
        assert!(matches("Sigur Rós", &words("ros sig")) && !matches("Sigur Rós", &words("rosa")));
    }
}
