//! One matcher for names (0.12: one matcher, the owner 2026-10-30, after a run-together-words stopgap he called "a very
//! rudimentary fix"): how a query meets a name, for the server's search and for the page through WebAssembly alike.
//!
//! - **Reading** ([`fold`], [`words`], [`letters`]): Unicode NFKD, combining marks dropped, case folded; everything but
//!   letters and digits a gap between words. Separators don't count: a name is also compared as its letters run
//!   together, so Hip Hop = Hip-Hop = HipHop = hiphop, and word by word too.
//! - **Strengths**, best first ([`How`]): the name itself (its letters the query's, [`How::Exact`]); one of its aliases
//!   ([`How::Alias`]); its start or a word's start, as a query still being typed ([`How::Prefix`]); else alike enough
//!   ([`How::Fuzzy`]): trigrams (pg_trgm's, of the letters run together and of each word) or the typos a word may have
//!   (Meilisearch's budget, [`typo_budget`]), at least [`FLOOR`].
//! - **Asking** ([`Index`] built once, [`Index::query`] at every key; [`score`] for one name): hits best first, each
//!   with its score (0..1, within its strength) and how it was found.
//!
//! Pure as the rest of the crate: no I/O, no clock; aliases come as data (genre aliases in [`aliases`]).

use serde::{Deserialize, Serialize};
#[cfg(feature = "nfkd")]
use unicode_normalization::UnicodeNormalization;
#[cfg(feature = "nfkd")]
use unicode_normalization::char::is_combining_mark;

pub mod aliases;
pub mod json;

/// The least score a fuzzy hit has (0.12): below it, nonsense ("jello" for Hello, "hiphop" for Trip Hop). A hit within
/// its typos ([`typos`]) always reaches it.
pub const FLOOR: f32 = 0.5;

/// A text as search reads it: compatibility-decomposed (ﬁ, ＡＢＣ, Ⅳ), its marks dropped (é, ö, ç), case folded. As
/// SQLite's `unicode61 remove_diacritics 2` and the page's lib/search.ts read it.
#[cfg(feature = "nfkd")]
pub fn fold(text: &str) -> String {
    text.nfkd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

/// Without NFKD (the page's WebAssembly): every text comes folded already, as the page folds it before it calls in,
/// `text.normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase()` (a mark is General_Category M, as
/// `is_combining_mark`); only case is folded again, which changes nothing then.
#[cfg(not(feature = "nfkd"))]
pub fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// A text's words (`fold`ed; everything but letters and digits a gap): what every search reads a query as (decision
/// 0072; web/tests/fixtures/search-words.json).
pub fn words(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// A text's letters and digits run together: what separators don't count leaves ("Hip-Hop", "hip hop": "hiphop").
pub fn letters(text: &str) -> String {
    words(text).concat()
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
        before = core::mem::replace(&mut prev, row);
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

/// The typos a text needs to hold every word: each word by its best meeting with the text's words, or (0.12:
/// separators don't count) the words run together meeting the text's letters from one of its words on ("hiphop" in
/// Hip Hop, "lo fi" in Lofi, "aphx twin" in Aphex Twin), the fewer; none where neither meets.
pub fn typos(text: &str, words: &[String]) -> Option<usize> {
    typos_in(&self::words(text), words)
}

fn typos_in(have: &[String], words: &[String]) -> Option<usize> {
    if words.is_empty() || have.is_empty() {
        return None;
    }
    let apart: Option<usize> = words.iter().map(|w| have.iter().filter_map(|h| meet(w, h)).min()).sum();
    let q = words.concat();
    let together = (0..have.len()).filter_map(|k| meet(&q, &have[k..].concat())).min();
    match (apart, together) {
        (Some(a), Some(t)) => Some(a.min(t)),
        (a, t) => a.or(t),
    }
}

/// How a hit was found, the strongest first (0.12): the name itself, one of its aliases, its start (or a word's) as a
/// query being typed, or alike enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Exact,
    Alias,
    Prefix,
    Fuzzy,
}

impl How {
    pub fn as_str(self) -> &'static str {
        match self {
            How::Exact => "exact",
            How::Alias => "alias",
            How::Prefix => "prefix",
            How::Fuzzy => "fuzzy",
        }
    }
}

/// A name to find, with the other names it goes by (a genre's aliases: [`aliases::genre`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    pub text: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Name {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), aliases: Vec::new() }
    }
}

/// A name found: its place among the index's names, its score (0..1, within its strength) and how it was found.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub i: usize,
    pub score: f32,
    pub how: How,
}

/// A text read once for matching: its words, its letters run together (and where each word starts in them), its
/// trigrams (of the letters and of each word, as one set) and each word's and each two neighbours' trigrams.
#[derive(Debug, Clone)]
struct Form {
    words: Vec<String>,
    letters: Vec<char>,
    joined: String,
    starts: Vec<usize>,
    grams: Vec<u64>,
    pieces: Vec<Vec<u64>>,
}

impl Form {
    fn new(text: &str) -> Self {
        let words = self::words(text);
        let mut letters: Vec<char> = Vec::new();
        let mut starts = Vec::with_capacity(words.len());
        let mut chars: Vec<Vec<char>> = Vec::with_capacity(words.len());
        for w in &words {
            starts.push(letters.len());
            let c: Vec<char> = w.chars().collect();
            letters.extend_from_slice(&c);
            chars.push(c);
        }
        let mut all = grams(&letters);
        // Each word's first (a query's words are its first pieces), then each two neighbours' run together.
        let mut pieces: Vec<Vec<u64>> = chars.iter().map(|c| set(grams(c))).collect();
        for p in &pieces {
            all.extend_from_slice(p);
        }
        pieces.extend(chars.windows(2).map(|two| set(grams(&[two[0].as_slice(), two[1].as_slice()].concat()))));
        Self { joined: words.concat(), words, letters, starts, grams: set(all), pieces }
    }
}

/// A string's trigrams, padded as pg_trgm pads a word (two spaces before, one after), each three letters as one number.
fn grams(s: &[char]) -> Vec<u64> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut p = Vec::with_capacity(s.len() + 3);
    p.extend_from_slice(&[' ', ' ']);
    p.extend_from_slice(s);
    p.push(' ');
    p.windows(3).map(|t| (u64::from(u32::from(t[0])) << 42) | (u64::from(u32::from(t[1])) << 21) | u64::from(u32::from(t[2]))).collect()
}

fn set(mut v: Vec<u64>) -> Vec<u64> {
    v.sort_unstable();
    v.dedup();
    v
}

/// The trigrams two sorted sets share.
fn shared(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            core::cmp::Ordering::Less => i += 1,
            core::cmp::Ordering::Greater => j += 1,
            core::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

/// pg_trgm's similarity: the trigrams shared over all the two have.
fn similarity(a: &[u64], b: &[u64]) -> f32 {
    let s = shared(a, b);
    let all = a.len() + b.len() - s;
    if all == 0 { 0.0 } else { s as f32 / all as f32 }
}

fn f(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

/// A query's start of a name (0.12): the query's letters begin the name's from one of its words on ("rad" in
/// Radiohead, "hiph" in Hip Hop, "rollingst" in The Rolling Stones), or each of its words begins one of the name's, in
/// any order ("ros sig" in Sigur Rós). Its score: 0.4 for how much of the name the query makes up, 0.3 when it ends
/// at a word's end (whole words: "radio" finds Radio Birdman before Radiohead), 0.3 when it begins the name ("rad"
/// finds Radiohead before Radio Birdman: more of it typed).
fn prefix(q: &Form, n: &Form) -> Option<f32> {
    let (lq, ln) = (q.letters.len(), n.letters.len());
    let cov = (lq as f32 / ln as f32).min(1.0);
    let mut best: Option<f32> = None;
    for (k, &at) in n.starts.iter().enumerate() {
        if n.letters[at..].starts_with(&q.letters) {
            let end = at + lq;
            let s = 0.4 * cov + 0.3 * f(end == ln || n.starts.contains(&end)) + 0.3 * f(k == 0);
            best = Some(best.map_or(s, |b| b.max(s)));
        }
    }
    if q.words.len() > 1 && q.words.iter().all(|w| n.words.iter().any(|h| h.starts_with(w.as_str()))) {
        let whole = q.words.iter().filter(|w| n.words.contains(w)).count() as f32 / q.words.len() as f32;
        let first = n.words.first().is_some_and(|h| h.starts_with(q.words[0].as_str()));
        let s = 0.4 * cov + 0.3 * whole + 0.3 * f(first);
        best = Some(best.map_or(s, |b| b.max(s)));
    }
    best
}

/// How alike a query and a name are by trigrams (0.12): half pg_trgm's similarity of the two (letters run together and
/// each word, as one set each), half the query's words each against the name's closest word or two neighbours run
/// together (weighted by their letters), so word order and a few words of a long name count.
fn trigrams(q: &Form, n: &Form) -> f32 {
    let whole = similarity(&q.grams, &n.grams);
    let (mut part, mut total) = (0.0f32, 0usize);
    for (w, g) in q.words.iter().zip(&q.pieces) {
        let best = n.pieces.iter().map(|p| similarity(g, p)).fold(0.0f32, f32::max);
        let len = w.chars().count();
        part += best * len as f32;
        total += len;
    }
    let part = if total == 0 { 0.0 } else { part / total as f32 };
    (whole + part) / 2.0
}

/// Alike enough (0.12): the better of the trigrams' score and a score for the typos it has within its budget
/// (`typos`: 0.5 + 0.4 / (1 + typos) + 0.1 for how much of the name the query makes up: one typo in a whole name
/// 0.8, two 0.73; always above [`FLOOR`]); none below the floor. A query's word with a figure is never guessed at (as
/// the typo budget has it): the name holds it as written, or it does not match.
fn fuzzy(q: &Form, n: &Form) -> Option<f32> {
    if !q.words.iter().filter(|w| w.chars().any(char::is_numeric)).all(|w| n.joined.contains(w.as_str())) {
        return None;
    }
    let cov = (q.letters.len() as f32 / n.letters.len() as f32).min(1.0);
    let typo = typos_in(&n.words, &q.words).map_or(0.0, |t| 0.5 + 0.4 / (1 + t) as f32 + 0.1 * cov);
    let s = trigrams(q, n).max(typo);
    (s >= FLOOR).then_some(s)
}

/// How a query meets one text (its name or an alias), its strength and score.
fn against(q: &Form, n: &Form) -> Option<(How, f32)> {
    if q.letters.is_empty() || n.letters.is_empty() {
        return None;
    }
    if q.letters == n.letters {
        return Some((How::Exact, 1.0));
    }
    if let Some(s) = prefix(q, n) {
        return Some((How::Prefix, s));
    }
    fuzzy(q, n).map(|s| (How::Fuzzy, s))
}

/// The better of two meetings: the stronger, then the higher score.
fn better(a: Option<(How, f32)>, b: Option<(How, f32)>) -> Option<(How, f32)> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if y.0 < x.0 || (y.0 == x.0 && y.1 > x.1) { y } else { x }),
        (x, y) => x.or(y),
    }
}

#[derive(Debug, Clone)]
struct Entry {
    name: Form,
    aliases: Vec<Form>,
}

impl Entry {
    fn new(name: &Name) -> Self {
        Self { name: Form::new(&name.text), aliases: name.aliases.iter().map(|a| Form::new(a)).collect() }
    }

    /// How a query meets a name: by the name, or by an alias (all of one: [`How::Alias`]; its start or alike, a little
    /// below the name's own, so the name's own wins a tie).
    fn judge(&self, q: &Form) -> Option<(How, f32)> {
        let mut best = against(q, &self.name);
        if matches!(best, Some((How::Exact, _))) {
            return best;
        }
        for a in &self.aliases {
            let by = match against(q, a) {
                Some((How::Exact, _)) => Some((How::Alias, 1.0)),
                Some((How::Prefix, s)) => Some((How::Prefix, 0.9 * s)),
                Some((How::Fuzzy, s)) => Some((How::Fuzzy, 0.95 * s)).filter(|b| b.1 >= FLOOR),
                _ => None,
            };
            best = better(best, by);
        }
        best
    }
}

/// Names read once, asked at every key (0.12): a list of genres, artists, crates.
#[derive(Debug, Clone, Default)]
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    pub fn new(names: Vec<Name>) -> Self {
        Self { entries: names.iter().map(Entry::new).collect() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The names the query finds, best first, at most `limit`: exact before alias before prefix before fuzzy; within
    /// one, the higher score; then the index's order.
    pub fn query(&self, q: &str, limit: usize) -> Vec<Hit> {
        let q = Form::new(q);
        if q.letters.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut hits: Vec<Hit> = self.entries.iter().enumerate().filter_map(|(i, e)| e.judge(&q).map(|(how, score)| Hit { i, score, how })).collect();
        hits.sort_by(|a, b| a.how.cmp(&b.how).then(b.score.total_cmp(&a.score)).then(a.i.cmp(&b.i)));
        hits.truncate(limit);
        hits
    }
}

/// How a query meets one text (no aliases), for a one-off check: its score and strength, none below the floor.
pub fn score(q: &str, text: &str) -> Option<(f32, How)> {
    score_name(q, &Name::new(text))
}

/// How a query meets one name with its aliases.
pub fn score_name(q: &str, name: &Name) -> Option<(f32, How)> {
    let q = Form::new(q);
    if q.letters.is_empty() {
        return None;
    }
    Entry::new(name).judge(&q).map(|(how, s)| (s, how))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Vectors {
        score: Vec<ScoreCase>,
        order: Vec<OrderCase>,
    }

    #[derive(serde::Deserialize)]
    struct ScoreCase {
        q: String,
        text: String,
        #[serde(default)]
        aliases: Vec<String>,
        how: Option<How>,
    }

    #[derive(serde::Deserialize)]
    struct OrderCase {
        q: String,
        names: Vec<Name>,
        /// The first hits, in this order.
        first: Vec<usize>,
        /// How each of those was found.
        #[serde(default)]
        how: Vec<How>,
        /// Names not found at all.
        #[serde(default)]
        absent: Vec<usize>,
    }

    fn vectors() -> Vectors {
        serde_json::from_str(include_str!("../tests/vectors/matcher.json")).unwrap()
    }

    #[test]
    fn each_name_is_found_as_the_shared_cases_say() {
        let v = vectors();
        assert!(v.score.len() >= 20);
        for c in v.score {
            let got = score_name(&c.q, &Name { text: c.text.clone(), aliases: c.aliases.clone() });
            assert_eq!(got.map(|g| g.1), c.how, "{:?} for {:?}: {got:?}", c.q, c.text);
            if let Some((s, _)) = got {
                assert!((0.0..=1.0).contains(&s), "{:?} for {:?}: {s}", c.q, c.text);
            }
        }
    }

    #[test]
    fn names_rank_as_the_shared_cases_say() {
        for c in vectors().order {
            let hits = Index::new(c.names.clone()).query(&c.q, 100);
            let first: Vec<usize> = hits.iter().take(c.first.len()).map(|h| h.i).collect();
            assert_eq!(first, c.first, "{:?}: {hits:?}", c.q);
            for (h, how) in hits.iter().zip(&c.how) {
                assert_eq!(h.how, *how, "{:?}: {hits:?}", c.q);
            }
            for a in &c.absent {
                assert!(hits.iter().all(|h| h.i != *a), "{:?} found {:?}: {hits:?}", c.q, c.names[*a].text);
            }
        }
    }

    #[test]
    fn separators_dont_count() {
        for (q, text) in [("hiphop", "Hip Hop"), ("hiphop", "Hip-Hop"), ("hip hop", "HipHop"), ("drumandbass", "Drum and Bass"), ("lo fi", "Lofi"), ("Lo-Fi", "lofi")] {
            assert_eq!(score(q, text), Some((1.0, How::Exact)), "{q:?} for {text:?}");
        }
        assert_eq!(letters("Hip-Hop"), "hiphop");
        assert_eq!(fold("Björk ﬁre Ⅳ"), "bjork fire iv");
    }

    #[test]
    fn hiphop_is_not_trip_hop() {
        assert_eq!(score("hiphop", "Trip Hop"), None, "below the floor");
        let names = vec![Name::new("Trip Hop"), Name::new("Hip Hop")];
        let hits = Index::new(names).query("hip hop", 10);
        assert_eq!(hits[0], Hit { i: 1, score: 1.0, how: How::Exact });
        assert!(hits.iter().skip(1).all(|h| h.how == How::Fuzzy && h.score < 0.7), "Trip Hop, if at all, far below: {hits:?}");
    }

    #[test]
    fn typos_and_swaps_are_found_fewer_first() {
        let (one, _) = score("radiohed", "Radiohead").unwrap();
        let (swap, how) = score("raidohead", "Radiohead").unwrap();
        assert_eq!(how, How::Fuzzy);
        assert!(one > FLOOR && swap > FLOOR);
        let (k1, _) = score("kraftwrek", "Kraftwerk").unwrap();
        let (k2, _) = score("kraftwrekk", "Kraftwerk").unwrap();
        assert!(k1 > k2, "fewer typos first: {k1} {k2}");
    }

    #[test]
    fn a_query_being_typed_finds_its_name_from_the_first_letters() {
        // "rad": Radiohead first (more of it typed); "radio": Radio Birdman first (a whole word).
        let ix = Index::new(vec![Name::new("Radio Birdman"), Name::new("Radiohead"), Name::new("Raidohead")]);
        let rad: Vec<(usize, How)> = ix.query("rad", 10).iter().map(|h| (h.i, h.how)).collect();
        assert_eq!(&rad[..2], &[(1, How::Prefix), (0, How::Prefix)]);
        let radio: Vec<usize> = ix.query("radio", 10).iter().map(|h| h.i).collect();
        assert_eq!(&radio[..2], &[0, 1]);
        let radioh = ix.query("radioh", 10);
        assert_eq!((radioh[0].i, radioh[0].how), (1, How::Prefix));
        assert!(radioh.iter().any(|h| h.i == 2 && h.how == How::Fuzzy), "a typo while typing, below the start: {radioh:?}");
        assert_eq!(ix.query("rad", 1).len(), 1, "at most the limit");
        assert!(ix.query("  ", 10).is_empty() && ix.query("rad", 0).is_empty());
    }

    #[test]
    fn an_alias_finds_its_genre() {
        let names = vec![Name::new("DnB Classics"), Name { text: "Drum and Bass".into(), aliases: aliases::genre("Drum and Bass") }, Name::new("Dub")];
        let ix = Index::new(names);
        let dnb = ix.query("dnb", 10);
        assert_eq!((dnb[0].i, dnb[0].how), (1, How::Alias));
        assert_eq!((dnb[1].i, dnb[1].how), (0, How::Prefix));
        assert!(dnb.iter().all(|h| h.i != 2));
        assert_eq!(ix.query("d&b", 10)[0].i, 1);
        let rnb = Name { text: "R&B".into(), aliases: aliases::genre("R&B") };
        assert_eq!(score_name("rnb", &rnb).map(|s| s.1), Some(How::Alias));
        assert_eq!(score_name("rhythm and blues", &rnb).map(|s| s.1), Some(How::Alias));
    }

    #[test]
    fn nonsense_finds_nothing() {
        for (q, text) in [("jello", "Hello"), ("aia", "Air"), ("1998", "1999"), ("zzzz", "Radiohead"), ("rosa", "Sigur Rós"), ("", "Radiohead"), ("radiohead", "")] {
            assert_eq!(score(q, text), None, "{q:?} for {text:?}");
        }
    }

    #[derive(serde::Deserialize)]
    struct TypoExample {
        q: String,
        text: String,
        typos: Option<usize>,
    }

    #[derive(serde::Deserialize)]
    struct WordsExample {
        q: String,
        words: Vec<String>,
    }

    #[test]
    fn the_pages_shared_examples_still_hold() {
        // web/tests/fixtures: the page's lib/search.ts is held to them too, until it takes this matcher.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/tests/fixtures");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_else(|e| panic!("the shared examples ({f}): {e}; run with the repository mounted"));
        let typo: Vec<TypoExample> = serde_json::from_str(&read("search-typos.json")).unwrap();
        for e in typo {
            assert_eq!(typos(&e.text, &words(&e.q)), e.typos, "{:?} in {:?}", e.q, e.text);
            if e.typos.is_some() {
                assert!(score(&e.q, &e.text).is_some(), "the matcher finds what the typos allow: {:?} in {:?}", e.q, e.text);
            }
        }
        let read_as: Vec<WordsExample> = serde_json::from_str(&read("search-words.json")).unwrap();
        for e in read_as {
            assert_eq!(words(&e.q), e.words, "{:?}", e.q);
        }
    }

    #[test]
    fn typos_count_words_run_together_too() {
        assert_eq!(typos("Hip Hop", &words("hiphop")), Some(0));
        assert_eq!(typos("HipHop", &words("hip hop")), Some(0));
        assert_eq!(typos("Aphex Twin", &words("aphx twin")), Some(1));
        assert_eq!(typos("Trip Hop", &words("hiphop")), None);
    }
}
