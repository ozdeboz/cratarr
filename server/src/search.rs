//! One search, structured (decision 0072): a query read one way, on the server as in the browser (web/src/lib/search.ts,
//! both held to web/tests/fixtures/search-words.json), and answered by kind, deep, in one shape (`app::search`).
//!
//! A query's words: Unicode NFKD (a full-width letter, a ligature, a Roman numeral made plain), combining marks dropped
//! (as the library's FTS tables' `remove_diacritics 2`), case folded, everything but letters and digits a space between
//! words. No operators: quotes and FTS syntax are words like any other.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use cratarr_rules::matcher;
use cratarr_rules::matcher::aliases;

/// The words of a query, as every search reads them (the rules crate's `matcher::words`, 0.12).
pub fn words(q: &str) -> Vec<String> {
    matcher::words(q)
}

/// Whether a text holds every word, each as a word or a word's start, or with the typos it may have (0.9.7, `typos`),
/// read as the query is (for what the FTS tables do not hold: playlists' and crates' names, genres); or (0.12: one
/// matcher, the owner 2026-10-30) whether the rules crate's matcher finds it at all: separators don't count ("hiphop"
/// finds Hip Hop), aliases, trigrams for what is alike beyond the typos (`matcher::score`, its floor keeping nonsense
/// out).
pub fn matches(text: &str, words: &[String]) -> bool {
    !words.is_empty() && (typos(text, words).is_some() || matcher::score(&words.join(" "), text).is_some())
}

/// Whether a name or one of its aliases holds the words (0.12: one matcher, genre aliases): as
/// `matches` for its name, and the matcher's for the name with its aliases ("dnb" finds Drum and Bass, `How::Alias`).
pub fn matches_name(name: &matcher::Name, words: &[String]) -> bool {
    matches(&name.text, words) || (!name.aliases.is_empty() && !words.is_empty() && matcher::score_name(&words.join(" "), name).is_some())
}

/// Where a name found by one of its aliases ranks (`rank_name`): above every start and every name holding the words
/// as whole words, below the name itself.
const ALIAS_RANK: f32 = 4.9;

/// `rank` for a name with its aliases (0.12: one matcher, genre aliases): by its own name as `rank`
/// has it; a name all of one of its aliases (`How::Alias`) at `ALIAS_RANK`, as the matcher puts an alias right after
/// the name itself; an alias begun or alike as the matcher's trigrams rank (from 0.5, by its score).
pub fn rank_name(name: &matcher::Name, words: &[String], at: usize, of: usize) -> f32 {
    let r = rank(&name.text, words, at, of);
    if name.aliases.is_empty() || r >= ALIAS_RANK {
        return r;
    }
    let order = 0.09 * (1.0 - at as f32 / of.max(1) as f32);
    match matcher::score_name(&words.join(" "), name) {
        Some((_, matcher::How::Alias)) => r.max(ALIAS_RANK + order),
        Some((s, _)) => r.max(0.5 + 0.4 * s + order),
        None => r,
    }
}

/// The other names genres go by (0.12: one matcher, genre aliases): the rules crate's hand-kept list
/// (`aliases::GENRES`; the owner, 2026-10-30: no reading of MusicBrainz's website, the list extended when a gap is
/// found). Each genre asked keeps its aliases here; only the library's genres are asked (by search, the words' kinds
/// and the page).
#[derive(Default)]
pub struct GenreAliases {
    of: HashMap<String, Arc<[String]>>,
}

impl GenreAliases {
    /// A genre's aliases, by the genre as written in the library; none for a genre in no group.
    pub fn of(&mut self, genre: &str) -> Arc<[String]> {
        if let Some(a) = self.of.get(genre) {
            return a.clone();
        }
        let a: Arc<[String]> = aliases::genre(genre).into();
        self.of.insert(genre.to_string(), a.clone());
        a
    }

    /// A genre as the matcher takes it: its name with its aliases.
    pub fn name(&mut self, genre: &str) -> matcher::Name {
        matcher::Name { text: genre.to_string(), aliases: self.of(genre).to_vec() }
    }

    /// The library's genres (`genres`, as written there) with their aliases, those with none left out: what the page's
    /// matcher is given (`GET /api/genres/aliases`).
    pub fn library<'a>(&mut self, genres: impl IntoIterator<Item = &'a str>) -> BTreeMap<String, Vec<String>> {
        let mut out = BTreeMap::new();
        for g in genres {
            let a = self.of(g);
            if !a.is_empty() {
                out.insert(g.to_string(), a.to_vec());
            }
        }
        out
    }
}

/// The server's genre aliases, each library genre's worked out once.
static GENRE_ALIASES: Mutex<Option<GenreAliases>> = Mutex::new(None);

fn genre_aliases<T>(f: impl FnOnce(&mut GenreAliases) -> T) -> T {
    let mut g = GENRE_ALIASES.lock().unwrap_or_else(|p| p.into_inner());
    f(g.get_or_insert_with(GenreAliases::default))
}

/// A genre of the library as the matcher takes it (0.12): its name with the other names it goes by.
pub fn genre_name(genre: &str) -> matcher::Name {
    genre_aliases(|g| g.name(genre))
}

/// Genres of the library with their aliases, those with none left out (`GenreAliases::library`).
pub fn library_genre_aliases<'a>(genres: impl IntoIterator<Item = &'a str>) -> BTreeMap<String, Vec<String>> {
    genre_aliases(|g| g.library(genres))
}

/// How many typos a query's word may have (0.9.7, the owner: "make it great"; Meilisearch's measure): none under five
/// letters, one from five, two from nine; none in a word with a figure (a year, a catalogue number). The rules crate's
/// (0.12), as the web's lib/search.ts (the shared examples, web/tests/fixtures/search-typos.json).
pub fn typo_budget(w: &str) -> usize {
    matcher::typo_budget(w)
}

/// How a query's word meets a text's word (the rules crate's `matcher::meet`): whole or as its start, no typos; else,
/// within its typos and its first letter right, the word or its start with that many; none where it does not.
pub fn meet(w: &str, h: &str) -> Option<usize> {
    matcher::meet(w, h)
}

/// The typos a text needs to hold every word (the rules crate's `matcher::typos`): each word by its best meeting with
/// the text's words, or (0.12) the words run together meeting the text's letters from one of its words on; none where
/// neither meets.
pub fn typos(text: &str, words: &[String]) -> Option<usize> {
    matcher::typos(text, words)
}

/// How well a text matches the words, for ranking (higher is better): the whole text the query, 3; every word whole,
/// 2; some words only as a start, 1 (Navidrome's lesson: a whole name above a prefix); not at all, 0. Separators don't
/// count (0.12): "hiphop" is all of Hip Hop (3); the query's letters beginning the text's from one of its words on
/// ("hiph"), 1.
pub fn closeness(text: &str, words: &[String]) -> u8 {
    let have = self::words(text);
    if words.is_empty() {
        return 0;
    }
    if have == words || have.concat() == words.concat() {
        return 3;
    }
    if words.iter().all(|w| have.iter().any(|h| h.starts_with(w.as_str()))) {
        return if words.iter().all(|w| have.contains(w)) { 2 } else { 1 };
    }
    let q = words.concat();
    u8::from((0..have.len()).any(|k| have[k..].concat().starts_with(&q)))
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
///
/// What only the matcher's trigrams find (0.12, `matches`) ranks between the two: from 0.5, by its score, below every
/// hit within its typos and above every one found through what it holds.
pub fn rank(text: &str, words: &[String], at: usize, of: usize) -> f32 {
    let r = ranked(closeness(text, words), typos(text, words), coverage(text, words), at, of);
    if r >= 1.0 {
        return r;
    }
    match matcher::score(&words.join(" "), text) {
        Some((s, _)) => r.max(0.5 + 0.4 * s + 0.09 * (1.0 - at as f32 / of.max(1) as f32)),
        None => r,
    }
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
///
/// The library's FTS tables hold words, so through them a word is found as a word, a word's start or one of its typo
/// terms only; separators and trigram likeness (a query "hiphop" for an artist "Hip Hop Orchestra", "lofi" for an album
/// "Lo-Fi Beats", "xadiohead" for Radiohead) come from the trigram tables beside them (0.12: one matcher, search in
/// general; `trigram_query`), by the names' own letters: artists' and albums' names, tracks' titles. What is still
/// missing there: a track found by its artist's or album's name spelled otherwise (the trigram tables hold titles
/// only), and aliases (a genre's are matched in memory with the genres' names, `matches_name` with `genre_name`, 0.12;
/// the library's other names have none).
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

/// The query for the library's trigram tables (0.12: one matcher, search in general): every trigram of the words'
/// letters run together (`matcher::letters` of the query, as the tables hold each name's), any of them, so a name is a
/// candidate however it is spelled or spaced and with typos ("radiohed" shares five of its six with Radiohead), the
/// tables' bm25 putting those sharing more, and rarer, first; the matcher judges them. None under three letters: a
/// trigram table finds nothing shorter.
pub fn trigram_query(words: &[String]) -> Option<String> {
    let letters: Vec<char> = words.concat().chars().collect();
    if letters.len() < 3 {
        return None;
    }
    let mut grams: Vec<String> = Vec::new();
    for t in letters.windows(3) {
        let g: String = t.iter().collect();
        if !grams.contains(&g) {
            grams.push(g);
        }
    }
    Some(grams.iter().map(|g| format!("\"{g}\"")).collect::<Vec<_>>().join(" OR "))
}

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
    fn the_trigram_query_asks_for_any_trigram_of_the_letters_run_together() {
        // 0.12: separators don't count ("Hip-Hop" is hiphop), each trigram once, nothing under three letters.
        assert_eq!(trigram_query(&words("Hip-Hop")).as_deref(), Some("\"hip\" OR \"iph\" OR \"pho\" OR \"hop\""));
        assert_eq!(trigram_query(&words("lo fi")).as_deref(), Some("\"lof\" OR \"ofi\""));
        assert_eq!(trigram_query(&words("aaaa")).as_deref(), Some("\"aaa\""));
        assert_eq!(trigram_query(&words("Björk")).as_deref(), Some("\"bjo\" OR \"jor\" OR \"ork\""));
        assert_eq!(trigram_query(&words("a b")), None);
        assert_eq!(trigram_query(&words("\" *")), None);
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

    #[test]
    fn a_genre_is_found_by_its_aliases_and_ranks_below_its_own_name() {
        // 0.12: one matcher, genre aliases (the hand-kept list).
        let mut seed = GenreAliases::default();
        let dnb = seed.name("Drum and Bass");
        let q = words("dnb");
        assert!(!matches("Drum and Bass", &q) && matches_name(&dnb, &q));
        assert_eq!(matcher::score_name("dnb", &dnb).map(|s| s.1), Some(matcher::How::Alias));
        let alias = rank_name(&dnb, &q, 0, 1);
        assert!(alias > rank("Drum and Bass Classics", &words("drum"), 0, 1) && alias < rank("DnB", &q, 0, 1), "{alias}");
        assert!(matches_name(&seed.name("R&B"), &words("rnb")));
        assert!(!matches_name(&seed.name("Dub"), &q) && seed.of("Dub").is_empty());
        assert_eq!(rank_name(&seed.name("Dub"), &words("dub"), 0, 1), rank("Dub", &words("dub"), 0, 1), "its own name as ever");
    }

    #[test]
    fn the_page_gets_the_aliases_of_the_librarys_genres_only() {
        // `GET /api/genres/aliases`: the library's genres as written there; one in no group is left out, and a genre the
        // library lacks is never there (its aliases with it).
        let mut g = GenreAliases::default();
        let library = g.library(["Drum & Bass", "R&B", "Jazz"]);
        assert_eq!(library.keys().collect::<Vec<_>>(), ["Drum & Bass", "R&B"]);
        assert_eq!(library["Drum & Bass"], ["Drum and Bass", "Drum 'n' Bass", "DnB", "D&B"]);
        assert_eq!(library["R&B"], ["Rhythm and Blues", "RnB"]);
        assert!(!library.contains_key("Drum and Bass") && !library.contains_key("Jazz"));
        assert!(GenreAliases::default().library(["Jazz", "Shoegaze"]).is_empty(), "no genre of the list: nothing");
    }

    #[test]
    fn separators_dont_count_and_the_matcher_finds_what_typos_miss() {
        // 0.12: one matcher (the rules crate's). "hiphop" is all of Hip Hop, a direct hit; Trip Hop is not found.
        let q = words("hiphop");
        assert!(matches("Hip Hop", &q) && matches("Hip-Hop", &q) && !matches("Trip Hop", &q));
        assert_eq!(closeness("Hip Hop", &q), 3);
        assert_eq!(closeness("Hip Hop Classics", &words("hiph")), 1);
        assert!(rank("Hip Hop", &q, 0, 1) > 3.0);
        assert!(matches("Lofi", &words("lo fi")) && matches("Drum and Bass", &words("drumandbass")));
        // A wrong first letter is beyond the typos but within the trigrams: found, ranked below every typo hit and
        // above what is found through what it holds.
        let x = words("xadiohead");
        assert_eq!(typos("Radiohead", &x), None);
        assert!(matches("Radiohead", &x));
        let r = rank("Radiohead", &x, 0, 1);
        assert!(r > 0.5 && r < 1.0 && r < rank("Radiohead", &words("radiohed"), 0, 1) && r > rank("Thom Yorke", &x, 0, 1), "{r}");
    }
}
