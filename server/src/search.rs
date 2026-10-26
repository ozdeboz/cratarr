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

/// Whether a text holds every word, each as a word or a word's start, or with the typos it may have (0.9.7, `typos`),
/// read as the query is (for what the FTS tables do not hold: playlists' and crates' names, genres).
pub fn matches(text: &str, words: &[String]) -> bool {
    typos(text, words).is_some()
}

/// How many typos a query's word may have (0.9.7, the owner: "make it great"; Meilisearch's measure): none under five
/// letters, one from five, two from nine; none in a word with a figure (a year, a catalogue number). As the web's
/// lib/search.ts (the shared examples, web/tests/fixtures/search-typos.json).
pub fn typo_budget(w: &str) -> usize {
    if w.chars().any(char::is_numeric) {
        return 0;
    }
    match w.chars().count() {
        n if n >= 9 => 2,
        n if n >= 5 => 1,
        _ => 0,
    }
}

/// The edits from `a` to `b` (a letter added, dropped or changed, or two side by side swapped: Damerau's, as optimal
/// string alignment), or `most` + 1 as soon as there are more than `most`.
fn edits(a: &[char], b: &[char], most: usize) -> usize {
    if a.len().abs_diff(b.len()) > most {
        return most + 1;
    }
    let mut before: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i];
        let mut least = i;
        for j in 1..=b.len() {
            let mut d = (prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(before[j - 2] + 1);
            }
            row.push(d);
            least = least.min(d);
        }
        if least > most {
            return most + 1;
        }
        before = std::mem::replace(&mut prev, row);
    }
    prev[b.len()]
}

/// How a query's word meets a text's word: whole or as its start, no typos; else, within its typos and its first
/// letter right, the word or its start (as long as the query's word, give or take a letter: a word still being
/// typed) with that many; none where it does not.
pub fn meet(w: &str, h: &str) -> Option<usize> {
    if h.starts_with(w) {
        return Some(0);
    }
    let most = typo_budget(w);
    let (w, h): (Vec<char>, Vec<char>) = (w.chars().collect(), h.chars().collect());
    if most == 0 || w.first() != h.first() {
        return None;
    }
    let mut lengths = [h.len(), w.len().saturating_sub(1), w.len(), w.len() + 1];
    lengths.sort_unstable();
    let best = lengths.iter().filter(|&&k| k > 1 && k <= h.len()).map(|&k| edits(&w, &h[..k], most)).min().unwrap_or(most + 1);
    (best <= most).then_some(best)
}

/// The typos a text needs to hold every word (each word by its best meeting with the text's words), none where one
/// meets none.
pub fn typos(text: &str, words: &[String]) -> Option<usize> {
    if words.is_empty() {
        return None;
    }
    let have = self::words(text);
    words.iter().map(|w| have.iter().filter_map(|h| meet(w, h)).min()).sum()
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

/// How much of a name the words make up (0.9.7, the owner: "player" is all of a setting called Player, a small part
/// of a long track title): their letters over the name's, at most 1.
pub fn coverage(text: &str, words: &[String]) -> f32 {
    let name: usize = self::words(text).iter().map(|w| w.chars().count()).sum();
    let asked: usize = words.iter().map(|w| w.chars().count()).sum();
    if name == 0 { 0.0 } else { (asked as f32 / name as f32).min(1.0) }
}

/// A result's rank within its group (0.9.7), higher first: a direct hit (its own name holds every word) before one
/// whose name holds them with typos (the fewer first), and both before any found through what it holds; among direct
/// hits the closer (`closeness`), then the more of its name the words make up (`coverage`); then the place its source
/// gave it (`at` of `of`).
pub fn rank(text: &str, words: &[String], at: usize, of: usize) -> f32 {
    ranked(closeness(text, words), typos(text, words), coverage(text, words), at, of)
}

/// `rank` from its parts: direct hits from 3, those with typos from 1 to 2, the rest below 1 (as a playlist found by
/// its tracks scores).
pub fn ranked(close: u8, typos: Option<usize>, covered: f32, at: usize, of: usize) -> f32 {
    let order = 0.09 * (1.0 - at as f32 / of.max(1) as f32);
    match typos {
        Some(0) if close > 0 => 2.0 + f32::from(close) + 0.9 * covered + order,
        Some(t) if t > 0 => 1.0 + 0.9 / (1 + t) as f32 + order,
        _ => order,
    }
}

/// Every index term a query's word meets with typos (`meet`), the fewest first, at most `TYPO_TERMS`: what the FTS
/// query adds for it (`fts_with`). `terms` are the index's terms beginning with the word's first letter.
pub fn typo_terms<'a>(w: &str, terms: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    if typo_budget(w) == 0 {
        return Vec::new();
    }
    let mut found: Vec<(usize, &str)> = terms.into_iter().filter(|t| !t.starts_with(w)).filter_map(|t| Some((meet(w, t)?, t))).collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    found.into_iter().take(TYPO_TERMS).map(|(_, t)| t.to_string()).collect()
}

/// The index terms a word may stand for with typos, at most.
pub const TYPO_TERMS: usize = 24;

/// The FTS5 query for these words with their typos (`typo_terms`, by word): every word, as a word, a word's start or
/// one of its terms with typos.
pub fn fts_with(words: &[String], typo: &[Vec<String>]) -> Option<String> {
    (!words.is_empty()).then(|| {
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let more: String = typo.get(i).into_iter().flatten().filter(|t| !t.contains('"')).map(|t| format!(" OR \"{t}\"")).collect();
                format!("(\"{w}\" OR \"{w}\"*{more})")
            })
            .collect::<Vec<_>>()
            .join(" AND ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct TypoExample {
        q: String,
        text: String,
        typos: Option<usize>,
    }

    #[test]
    fn typos_are_allowed_as_the_pages_allow_them() {
        // The examples the web's lib/search.ts is held to as well.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/tests/fixtures/search-typos.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("the shared examples ({}): {e}; run with the repository mounted", path.display()));
        let examples: Vec<TypoExample> = serde_json::from_str(&text).unwrap();
        assert!(examples.len() >= 10);
        for e in examples {
            assert_eq!(typos(&e.text, &words(&e.q)), e.typos, "{:?} in {:?}", e.q, e.text);
        }
    }

    #[test]
    fn a_word_with_typos_adds_its_terms_to_the_index_query() {
        let w = words("radiohed");
        let terms = typo_terms(&w[0], ["radiohead", "radio", "rainbows", "radiohed"]);
        assert_eq!(terms, ["radiohead"], "its typos only; begun already, it is found as a start");
        assert_eq!(fts_with(&w, &[terms]).as_deref(), Some("(\"radiohed\" OR \"radiohed\"* OR \"radiohead\")"));
        assert!(typo_terms("ros", ["rose"]).is_empty(), "no typos in a short word");
        let with = rank("Radiohead", &w, 0, 1);
        assert!(with > 1.0 && with < rank("Radiohed", &w, 0, 1) && with > rank("Thom Yorke", &w, 0, 1), "below a direct hit, above a deep one");
        assert!(rank("Kraftwerk", &words("kraftwrek"), 0, 1) > rank("Kraftwerk", &words("kraftwrekk"), 0, 1), "fewer typos first");
    }

    #[test]
    fn a_direct_hit_ranks_first_then_the_closer_then_the_more_of_its_name() {
        let w = words("player");
        let setting = rank("Player", &w, 4, 5);
        let long = rank("The Player of the Long Night Remastered", &w, 0, 5);
        let deep = rank("Night Songs", &w, 0, 5);
        assert!(setting > long, "all of its name before a small part, whatever the order given");
        assert!(long > deep, "a direct hit before one found through what it holds");
        assert!(rank("Players", &w, 0, 5) < setting, "a start ranks below the whole word");
        assert!((coverage("Player", &w) - 1.0).abs() < f32::EPSILON);
        assert_eq!(coverage("", &w), 0.0);
    }

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
        assert_eq!(fts_with(&words("Björk Post"), &[]).as_deref(), Some("(\"bjork\" OR \"bjork\"*) AND (\"post\" OR \"post\"*)"));
        assert_eq!(fts_with(&words("\" *"), &[]), None);
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
