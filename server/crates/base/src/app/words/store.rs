//! The one store of words (decision 0097, the owner 2026-11-02: "We need a North Star"): each record's genres, styles,
//! moods and tags, and a track's own where they differ, each word with the sources that say it. Every reader reads
//! this: crate conditions, the filter and 0095's reasons (a track's facts), search, the word crates, similarity, a
//! record's rows.
//!
//! - **Layers.** By hand wins for its kind (0089's fields, with Reset); then the files' tags; then AudioMuse (styles,
//!   and moods: mood is AudioMuse's alone); then the services (MusicBrainz, Discogs, Last.fm, Bandcamp, the household's
//!   tags), weighted, a word below the floor left out.
//! - **Record and track.** A word of the files is the record's where half its tracks have it, AudioMuse's where its
//!   average score over the heard tracks reaches [`RISE`] (a mood: half those heard); else the tracks that have it own it.
//!   A track reads its record's words and its own; its mood is its own measure where it was heard.
//! - **Kind by the word (GEN, decision 0097 amended 2026-11-20).** Every word from every source is sorted alike, never by
//!   the field or the service it came from: an ID3v1 number named first, the genre aliases next; a word of the 26
//!   (`shared/genres.json`) is a genre, named as the list names it; one a caller (Discogs' genres and styles, MusicBrainz's
//!   genres, a file's GENRE) names is a style; any other a tag. Digits alone are no word, and so is a word only a file's
//!   STYLE uses that no caller knows (GEN-DESIGN §8). Mood is AudioMuse's and the hand's alone. A service's stored word is
//!   split by that service's rule as it is read (GEN.4).
//! - **One spelling.** A word's key folds case, spaces and hyphens (`super::key`); a kind's aliases name the word it
//!   is; its shown spelling is the 26's name, the alias's, else the most written.

use std::collections::{HashMap, HashSet};

use cratarr_rules::genres::{self, Genres};
use serde::Serialize;

/// The kinds, in the order every answer gives them.
pub const KINDS: [Kind; 4] = [Kind::Genre, Kind::Style, Kind::Mood, Kind::Tag];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Genre,
    Style,
    Mood,
    Tag,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Genre => "genre",
            Kind::Style => "style",
            Kind::Mood => "mood",
            Kind::Tag => "tag",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        KINDS.into_iter().find(|k| k.name() == name)
    }

    /// The most words a record keeps of it from the services (0.9.7: the strongest only; the files', AudioMuse's and
    /// the hand's are always kept).
    fn most(self) -> usize {
        match self {
            Kind::Genre | Kind::Mood => 3,
            Kind::Style | Kind::Tag => 5,
        }
    }
}

/// Who says a word, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Hand,
    Files,
    Audiomuse,
    Household,
    Musicbrainz,
    Discogs,
    Lastfm,
    /// A record's Bandcamp page's tags (GEN.59): last as a serialisation order, not a rank.
    Bandcamp,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Hand => "hand",
            Source::Files => "files",
            Source::Audiomuse => "audiomuse",
            Source::Household => "household",
            Source::Musicbrainz => "musicbrainz",
            Source::Discogs => "discogs",
            Source::Lastfm => "lastfm",
            Source::Bandcamp => "bandcamp",
        }
    }

    pub fn of(name: &str) -> Option<Self> {
        Some(match name {
            "hand" => Source::Hand,
            "files" => Source::Files,
            "audiomuse" => Source::Audiomuse,
            "household" => Source::Household,
            "musicbrainz" => Source::Musicbrainz,
            "discogs" => Source::Discogs,
            "lastfm" => Source::Lastfm,
            "bandcamp" => Source::Bandcamp,
            _ => return None,
        })
    }

    /// How far a service's word is trusted (0.9.7's): the household's as the files'; Bandcamp's as Last.fm's, both free tags
    /// (OQ-GEN-23: no rank of its own).
    fn trust(self, raw_kind: &str) -> f32 {
        match (self, raw_kind) {
            (Source::Hand | Source::Files | Source::Audiomuse | Source::Household, _) => 1.0,
            (Source::Musicbrainz, "tag") => 0.5,
            (Source::Musicbrainz, _) => 0.9,
            (Source::Discogs, _) => 0.8,
            (Source::Lastfm | Source::Bandcamp, _) => 0.5,
        }
    }

    /// How the source writes several words in one value (GEN-DESIGN §4's second layer): each service's shipped rule; what
    /// people type (the hand, the household's tags) one word. A file's fields come split by its library's rules as read.
    fn rule(self) -> &'static [&'static str] {
        match self {
            Source::Discogs => genres::rules::DISCOGS,
            Source::Musicbrainz => genres::rules::MUSICBRAINZ,
            Source::Lastfm => genres::rules::LASTFM,
            Source::Bandcamp => genres::rules::BANDCAMP,
            Source::Audiomuse => genres::rules::AUDIOMUSE,
            Source::Hand | Source::Household | Source::Files => genres::rules::TYPED,
        }
    }

    /// Whether the source, calling a word `raw_kind`, names it a genre or a style (GEN-DESIGN §3: a caller): Discogs, its genres
    /// and styles; MusicBrainz, its genres. A file's GENRE is the third caller ([`TrackIn::genre`]).
    fn calls(self, raw_kind: &str) -> bool {
        matches!((self, raw_kind), (Source::Discogs, "genre" | "style") | (Source::Musicbrainz, "genre"))
    }
}

/// A service's stored word as its words (GEN.4): the one split by the service's own rule, the owner's exceptions first.
fn said_words(s: &Said) -> Vec<String> {
    genres::split(&s.word, &genres::rules::separators(s.source.rule()), &[])
}

/// A typed word as its words (GEN.1 (d), OQ-GEN-24): the owner's exceptions, and no split.
fn typed_words(w: &str) -> Vec<String> {
    genres::split(w, &genres::rules::separators(genres::rules::TYPED), &[])
}

/// Where a word was read, for its sort: a file's STYLE alone can hold a non-word (GEN-DESIGN §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Style,
    Other,
}

/// A service's word below this merged weight is left out (0.9.7's floor).
pub const FLOOR: f32 = 0.15;

/// One track as read: its files' tags (Genre, STYLE, MOOD, Grouping) and what AudioMuse heard (its styles, and the
/// moods it stands out on); its words set by hand (0.15's track Edit), by kind.
#[derive(Debug, Clone, Default)]
pub struct TrackIn {
    pub id: String,
    pub genre: Vec<String>,
    pub style: Vec<String>,
    pub mood: Vec<String>,
    pub tag: Vec<String>,
    pub heard: bool,
    /// AudioMuse's top styles of it, each with its score (GEN.26).
    pub heard_styles: Vec<(String, f32)>,
    pub heard_moods: Vec<String>,
    pub hand: HashMap<Kind, Vec<String>>,
}

/// A service's word for a record (an artist's already shared out to each of their records).
#[derive(Debug, Clone)]
pub struct Said {
    pub source: Source,
    pub word: String,
    /// What the service called it: genre, style, mood or tag.
    pub raw_kind: String,
    /// The service's own strength, 0 to 1.
    pub weight: f32,
}

/// One record as read: its tracks, the services' words, its words set by hand by kind.
#[derive(Debug, Clone, Default)]
pub struct RecordIn {
    pub id: String,
    pub tracks: Vec<TrackIn>,
    pub said: Vec<Said>,
    pub hand: HashMap<Kind, Vec<String>>,
}

//// What the whole library says, for sorting a word into its kind and naming it (GEN-DESIGN §1–§3, §8): the 26 and their words,
/// the words a caller names a genre or a style, the words any source but a file's STYLE uses, and each kind's aliases (a key to
/// the word it is).
#[derive(Debug, Clone)]
pub struct Library {
    /// The 26 and their words (`shared/genres.json`).
    genres: &'static Genres,
    /// Every word a caller names (Discogs' genres and styles, MusicBrainz's genres, a file's GENRE), by its key as named: the
    /// library's style vocabulary, the genres among it aside. Worked out from the stored words: no fixed list (§3).
    called: HashSet<String>,
    /// Every word a source other than a file's STYLE uses, by its key as named: a STYLE word none of them uses, and no caller
    /// knows, is no word (§8, §13 C1).
    used: HashSet<String>,
    aliases: HashMap<Kind, HashMap<String, String>>,
    /// Each word's one spelling across the library: the most written (rule 4, one spelling everywhere).
    spellings: HashMap<String, String>,
    /// The sources switched off for a kind (the Catalogue sheet, 0.15): their words leave it, kept to come back.
    off: HashMap<Kind, HashSet<Source>>,
    /// AudioMuse's threshold ([`RISE`]).
    rise: f32,
}

impl Default for Library {
    fn default() -> Self {
        Self { genres: genres::shipped(), called: HashSet::new(), used: HashSet::new(), aliases: HashMap::new(), spellings: HashMap::new(), off: HashMap::new(), rise: RISE }
    }
}

/// AudioMuse's threshold (GEN.27–GEN.29): a word AudioMuse hears rises to the record where its average score over the record's
/// heard tracks reaches it. Until the rehearsal on production's copy sets it (GEN.29), the scenarios' 0.25.
pub const RISE: f32 = 0.25;

/// A record's or a track's hand lists as an Edit makes them (GEN.25): each `None` where that row is left as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Routed {
    pub genres: Option<Vec<String>>,
    pub styles: Option<Vec<String>>,
}

/// The hand's genre and style rows as an Edit sends them (`genres`, `styles`; none: not sent), against the rows shown at that
/// moment (`shown_genres`, `shown_styles`): the lists to keep by hand (GEN.25; GEN-DESIGN §13, C6, OQ-GEN-16, C12, R9).
/// - Each typed word is the owner's exceptions' words ("RnB/Swing": R&B and New Jack Swing), never split.
/// - Routed by the word: a word of the 26 is the genre row's, any other the style row's, whichever row it was typed into; a
///   word routed out of the row it was typed into edits its own row as that row is shown, never a list of that word alone.
/// - Only an edited row becomes a hand list: one that gains or loses a word of its own kind. A row sent as shown is none.
pub fn route(shown_genres: &[String], shown_styles: &[String], genres: Option<&[String]>, styles: Option<&[String]>) -> Routed {
    let list = genres::shipped();
    let is_genre = |w: &String| list.genre_of(w).is_some();
    let typed = |sent: Option<&[String]>| -> Option<Vec<String>> { sent.map(|s| s.iter().flat_map(|w| typed_words(w)).collect()) };
    let (genres, styles) = (typed(genres), typed(styles));
    // Each row: its own words as sent (else as shown), and the words of its kind typed into the other row.
    let into_genres: Vec<String> = styles.iter().flatten().filter(|&w| is_genre(w)).cloned().collect();
    let into_styles: Vec<String> = genres.iter().flatten().filter(|&w| !is_genre(w)).cloned().collect();
    let row = |shown: &[String], sent: &Option<Vec<String>>, own: &dyn Fn(&String) -> bool, routed: Vec<String>| -> Option<Vec<String>> {
        let kept: Vec<String> = match sent {
            Some(s) => s.iter().filter(|&w| own(w)).cloned().collect(),
            None => shown.to_vec(),
        };
        let edited = !routed.is_empty() || !same(&kept, shown);
        edited.then(|| once(kept.into_iter().chain(routed)))
    };
    let not_genre = |w: &String| !is_genre(w);
    Routed { genres: row(shown_genres, &genres, &is_genre, into_genres), styles: row(shown_styles, &styles, &not_genre, into_styles) }
}

/// Whether two lists hold the same words, by their keys, whatever their order.
fn same(a: &[String], b: &[String]) -> bool {
    let keys = |l: &[String]| l.iter().map(|w| super::key(w)).collect::<HashSet<_>>();
    keys(a) == keys(b)
}

/// Each word once, by its key, in order.
fn once(words: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    words.filter(|w| seen.insert(super::key(w))).collect()
}

impl Library {
    /// From every record's inputs and each kind's aliases (the word, and the spellings that are it).
    pub fn of(records: &[RecordIn], aliases: &HashMap<Kind, Vec<(String, Vec<String>)>>) -> Self {
        let aliases = aliases
            .iter()
            .map(|(kind, list)| {
                let by: HashMap<String, String> = list.iter().flat_map(|(to, from)| from.iter().chain(std::iter::once(to)).map(move |f| (super::key(f), to.clone()))).collect();
                (*kind, by)
            })
            .collect();
        let mut lib = Self { aliases, ..Self::default() };
        // Who calls each word what, and who uses it, every word named first (an ID3v1 number, a genre alias).
        let (mut called, mut used) = (HashSet::new(), HashSet::new());
        for r in records {
            for t in &r.tracks {
                for w in &t.genre {
                    called.insert(lib.named_key(w));
                }
                let heard = t.heard_styles.iter().map(|(w, _)| w);
                let hand = t.hand.values().flatten();
                for w in t.genre.iter().chain(&t.mood).chain(&t.tag).chain(heard).chain(&t.heard_moods).chain(hand) {
                    used.insert(lib.named_key(w));
                }
            }
            for s in &r.said {
                for w in said_words(s) {
                    let k = lib.named_key(&w);
                    if s.source.calls(&s.raw_kind) {
                        called.insert(k.clone());
                    }
                    used.insert(k);
                }
            }
            for w in r.hand.values().flatten() {
                used.insert(lib.named_key(w));
            }
        }
        (lib.called, lib.used) = (called, used);
        // Each word's spellings across the library, the most written shown everywhere.
        let mut written: HashMap<String, HashMap<String, usize>> = HashMap::new();
        for r in records {
            let tracks = r.tracks.iter().flat_map(|t| t.genre.iter().chain(&t.style).chain(&t.mood).chain(&t.tag).chain(t.heard_styles.iter().map(|(w, _)| w)).chain(&t.heard_moods)).cloned();
            for w in tracks.chain(r.said.iter().flat_map(said_words)) {
                *written.entry(super::key(&w)).or_default().entry(w.trim().to_string()).or_default() += 1;
            }
        }
        lib.spellings = written.into_iter().filter_map(|(k, s)| s.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(w, _)| (k, w))).collect();
        lib
    }

    /// The sources switched off for each kind (never the hand: what is set by hand is always the record's).
    pub fn with_off(mut self, off: HashMap<Kind, HashSet<Source>>) -> Self {
        self.off = off.into_iter().map(|(k, mut s)| {
            s.remove(&Source::Hand);
            (k, s)
        }).collect();
        self
    }

    /// AudioMuse's threshold ([`RISE`] unless set: the scenarios pass theirs).
    pub fn with_rise(mut self, rise: f32) -> Self {
        self.rise = rise;
        self
    }

    /// Whether a source's words are out of a kind.
    fn is_off(&self, kind: Kind, source: Source) -> bool {
        self.off.get(&kind).is_some_and(|o| o.contains(&source))
    }

    /// A word as it is named before its sort (GEN-DESIGN §13 C10, OQ-GEN-9): an ID3v1 genre number by its name ("26": Ambient),
    /// then a genre alias's word ("Electronica": Electronic; "dnb": Drum & Bass).
    fn named_first(&self, word: &str) -> String {
        let word = crate::catalogue::normalise::id3v1_genre(word).map_or_else(|| word.trim().to_string(), str::to_string);
        match self.aliases.get(&Kind::Genre).and_then(|a| a.get(&super::key(&word))) {
            Some(to) => to.clone(),
            None => word,
        }
    }

    /// [`Library::named_first`]'s key.
    fn named_key(&self, word: &str) -> String {
        super::key(&self.named_first(word))
    }

    /// A word's kind by the word (GEN-DESIGN §1–§3, §8), and the word as named: a genre where it is one of the 26 or one of
    /// their words; no word where it is digits alone, or read from a file's STYLE (`field`) with no caller knowing it and no
    /// other source using it; a style where a caller names it; else a tag. Never a mood: mood is AudioMuse's.
    fn sort(&self, word: &str, field: Field) -> Option<(Kind, String)> {
        let named = self.named_first(word);
        let k = super::key(&named);
        if k.is_empty() {
            return None;
        }
        if let Some(genre) = self.genres.genre_of_key(&k) {
            return Some((Kind::Genre, genre.to_string()));
        }
        if k.chars().all(|c| c.is_ascii_digit() || c == ' ') {
            return None;
        }
        if self.called.contains(&k) {
            return Some((Kind::Style, named));
        }
        if field == Field::Style && !self.used.contains(&k) {
            return None;
        }
        Some((Kind::Tag, named))
    }

    /// A word's key in its kind: a genre's by the 26 (one of their words: the genre), else an alias's word's.
    fn key(&self, kind: Kind, word: &str) -> String {
        let k = super::key(word);
        let k = match self.aliases.get(&kind).and_then(|a| a.get(&k)) {
            Some(to) => super::key(to),
            None => k,
        };
        match self.genres.genre_of_key(&k).filter(|_| kind == Kind::Genre) {
            Some(genre) => super::key(genre),
            None => k,
        }
    }

    /// The spelling a genre's name or an alias names, if one does.
    fn named(&self, kind: Kind, key: &str) -> Option<&str> {
        if kind == Kind::Genre
            && let Some(genre) = self.genres.genre_of_key(key)
        {
            return Some(genre);
        }
        self.aliases.get(&kind)?.get(key).map(String::as_str)
    }
}

// A word as answered: its one spelling and who says it, strongest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Word {
    pub word: String,
    pub sources: Vec<Source>,
}

/// A kind's words, strongest first; `hand`: set by hand (exactly the hand's list).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindWords {
    pub kind: Kind,
    pub hand: bool,
    pub words: Vec<Word>,
}

/// A track whose words differ from its record's: its kinds that differ, each whole.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackWords {
    pub id: String,
    pub kinds: Vec<KindWords>,
}

/// A record's words (the album view's `words`): its kinds that have any, in `KINDS`' order, and its tracks that differ.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Words {
    pub kinds: Vec<KindWords>,
    pub tracks: Vec<TrackWords>,
}

impl Words {
    /// A track's words of a kind as its readers take them: its own where it differs, else its record's.
    pub fn of_track(&self, track: &str, kind: Kind) -> &[Word] {
        let own = self.tracks.iter().find(|t| t.id == track).and_then(|t| t.kinds.iter().find(|k| k.kind == kind));
        own.or_else(|| self.kinds.iter().find(|k| k.kind == kind)).map_or(&[], |k| k.words.as_slice())
    }
}

/// A word on its way: its spellings as written, its weight, its sources, whether it is kept whatever its weight, and
/// AudioMuse's score of it on a track (GEN.26).
#[derive(Debug, Default)]
struct Heard {
    spellings: HashMap<String, usize>,
    weight: f32,
    sources: Vec<Source>,
    firm: bool,
    score: f32,
}

impl Heard {
    fn add(&mut self, word: &str, source: Source, weight: f32, firm: bool) {
        *self.spellings.entry(word.trim().to_string()).or_default() += 1;
        self.weight += weight;
        self.firm |= firm;
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }
    }
}

/// Words being gathered, by kind and key, in the order first heard.
#[derive(Debug, Default)]
struct Gathered {
    order: Vec<(Kind, String)>,
    by: HashMap<(Kind, String), Heard>,
}

impl Gathered {
    fn add(&mut self, lib: &Library, kind: Kind, word: &str, source: Source, weight: f32, firm: bool) {
        if lib.is_off(kind, source) {
            return;
        }
        let key = lib.key(kind, word);
        if key.is_empty() {
            return;
        }
        let at = (kind, key);
        if !self.by.contains_key(&at) {
            self.order.push(at.clone());
        }
        self.by.entry(at).or_default().add(word, source, weight, firm);
    }

    /// A word AudioMuse heard on a track, with its score: firm, its score kept for the record's average (GEN.26–GEN.28).
    fn heard(&mut self, lib: &Library, kind: Kind, word: &str, score: f32) {
        self.add(lib, kind, word, Source::Audiomuse, 1.0, true);
        if let Some(h) = self.by.get_mut(&(kind, lib.key(kind, word))) {
            h.score = h.score.max(score);
        }
    }

    /// A kind's words kept: the firm ones, then the services' strongest above the floor, up to the kind's most.
    fn words(&self, lib: &Library, kind: Kind) -> Vec<(String, Word, f32)> {
        let mut out: Vec<(String, Word, f32, bool)> = self
            .order
            .iter()
            .filter(|(k, _)| *k == kind)
            .filter_map(|at| {
                let h = &self.by[at];
                let weight = h.weight.min(1.0);
                (h.firm || weight >= FLOOR).then(|| {
                    let mut sources = h.sources.clone();
                    sources.sort();
                    (at.1.clone(), Word { word: shown(lib, kind, &at.1, &h.spellings), sources }, weight, h.firm)
                })
            })
            .collect();
        out.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.total_cmp(&a.2)).then_with(|| a.1.word.cmp(&b.1.word)));
        let mut services = 0;
        out.retain(|(_, _, _, firm)| {
            *firm || {
                services += 1;
                services <= kind.most()
            }
        });
        out.into_iter().map(|(k, w, x, _)| (k, w, x)).collect()
    }
}

/// A word's one spelling: its alias's word, else the library's most written, else this record's (the first such in
/// order of their names).
fn shown(lib: &Library, kind: Kind, key: &str, spellings: &HashMap<String, usize>) -> String {
    if let Some(named) = lib.named(kind, key).or_else(|| lib.spellings.get(key).map(String::as_str)) {
        return named.to_string();
    }
    spellings.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map_or_else(|| key.to_string(), |(w, _)| w.clone())
}

/// A track's words of the files and AudioMuse, by kind and key, with their sources: what it says itself. Each word of its
/// files' four fields and of AudioMuse's styles sorted by the word (GEN.10), never by its field; a non-word left out.
fn own_words(lib: &Library, t: &TrackIn) -> Gathered {
    let mut g = Gathered::default();
    for (field, words) in [(Field::Other, &t.genre), (Field::Style, &t.style), (Field::Other, &t.tag), (Field::Other, &t.mood)] {
        for w in words {
            if let Some((kind, named)) = lib.sort(w, field) {
                g.add(lib, kind, &named, Source::Files, 1.0, true);
            }
        }
    }
    if t.heard {
        for (w, score) in &t.heard_styles {
            if let Some((kind, named)) = lib.sort(w, Field::Other) {
                g.heard(lib, kind, &named, *score);
            }
        }
        for w in &t.heard_moods {
            g.add(lib, Kind::Mood, w, Source::Audiomuse, 1.0, true);
        }
    }
    g
}

/// A hand's list as a kind's words: each once, in its order, folded and named.
fn by_hand(lib: &Library, kind: Kind, list: &[String]) -> KindWords {
    let mut seen = HashSet::new();
    let words = list
        .iter()
        .filter_map(|w| {
            let key = lib.key(kind, w);
            (!key.is_empty() && seen.insert(key.clone())).then(|| Word { word: lib.named(kind, &key).or_else(|| lib.spellings.get(&key).map(String::as_str)).map_or_else(|| w.trim().to_string(), str::to_string), sources: vec![Source::Hand] })
        })
        .collect();
    KindWords { kind, hand: true, words }
}

/// A record's words from its inputs (decision 0097): the store's one merge.
pub fn record(lib: &Library, r: &RecordIn) -> Words {
    let own: Vec<Gathered> = r.tracks.iter().map(|t| own_words(lib, t)).collect();
    let tracks = r.tracks.len();
    let heard = r.tracks.iter().filter(|t| t.heard).count();
    // The files' and AudioMuse's words that are the record's: the files' on half its tracks; AudioMuse's where its average score
    // over the heard tracks reaches the threshold (GEN.27, GEN.28: a heard track without it counts 0, an unheard one not at all),
    // a mood on half those heard.
    let mut on: HashMap<(Kind, String), (usize, usize, f32)> = HashMap::new();
    for g in &own {
        for (at, h) in &g.by {
            let e = on.entry(at.clone()).or_default();
            if h.sources.contains(&Source::Files) {
                e.0 += 1;
            }
            if h.sources.contains(&Source::Audiomuse) {
                e.1 += 1;
                e.2 += h.score;
            }
        }
    }
    let half = |n: usize, of: usize| of > 0 && n * 2 >= of;
    let rises = |kind: Kind, n: usize, scores: f32| match kind {
        Kind::Mood => half(n, heard),
        _ => heard > 0 && scores / heard as f32 >= lib.rise,
    };
    let mut gathered = Gathered::default();
    for g in &own {
        for at in &g.order {
            let (files, heard_by, scores) = on[at];
            let h = &g.by[at];
            for (source, by) in [(Source::Files, half(files, tracks)), (Source::Audiomuse, rises(at.0, heard_by, scores))] {
                if by && h.sources.contains(&source) {
                    for spelling in h.spellings.keys() {
                        gathered.add(lib, at.0, spelling, source, 1.0, true);
                    }
                }
            }
        }
    }
    // The services' words, each split by its service's rule as it is read (GEN.4), then sorted by the word: never a mood.
    for s in &r.said {
        let weight = s.weight.clamp(0.0, 1.0) * s.source.trust(&s.raw_kind);
        for w in said_words(s) {
            if let Some((kind, named)) = lib.sort(&w, Field::Other) {
                gathered.add(lib, kind, &named, s.source, weight, false);
            }
        }
    }
    let mut kinds = Vec::new();
    let mut kept: HashMap<Kind, Vec<(String, Word)>> = HashMap::new();
    for kind in KINDS {
        let k = match r.hand.get(&kind) {
            Some(list) => by_hand(lib, kind, list),
            None => KindWords { kind, hand: false, words: gathered.words(lib, kind).into_iter().map(|(_, w, _)| w).collect() },
        };
        kept.insert(kind, k.words.iter().map(|w| (lib.key(kind, &w.word), w.clone())).collect());
        if !k.words.is_empty() || k.hand {
            kinds.push(k);
        }
    }
    let mut out_tracks = Vec::new();
    for (t, g) in r.tracks.iter().zip(&own) {
        let mut differ = Vec::new();
        for kind in KINDS {
            let record_words = kept.get(&kind).map(Vec::as_slice).unwrap_or_default();
            let words: KindWords = if let Some(list) = t.hand.get(&kind) {
                by_hand(lib, kind, list)
            } else if r.hand.contains_key(&kind) {
                continue;
            } else if kind == Kind::Mood && t.heard {
                // A heard track's mood is its own measure.
                let mine = g.words(lib, Kind::Mood);
                KindWords { kind, hand: false, words: mine.into_iter().map(|(_, w, _)| Word { word: w.word, sources: vec![Source::Audiomuse] }).collect() }
            } else {
                // Its record's, and its own besides.
                let mut words: Vec<Word> = record_words.iter().map(|(_, w)| w.clone()).collect();
                let have: HashSet<&str> = record_words.iter().map(|(k, _)| k.as_str()).collect();
                for (key, w, _) in g.words(lib, kind) {
                    if !have.contains(key.as_str()) {
                        words.push(w);
                    }
                }
                KindWords { kind, hand: false, words }
            };
            let same = !words.hand && {
                let mine: Vec<String> = words.words.iter().map(|w| lib.key(kind, &w.word)).collect();
                let theirs: Vec<&str> = record_words.iter().map(|(k, _)| k.as_str()).collect();
                mine.len() == theirs.len() && mine.iter().all(|k| theirs.contains(&k.as_str()))
            };
            if !same {
                differ.push(words);
            }
        }
        if !differ.is_empty() {
            out_tracks.push(TrackWords { id: t.id.clone(), kinds: differ });
        }
    }
    Words { kinds, tracks: out_tracks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> TrackIn {
        TrackIn { id: id.into(), ..TrackIn::default() }
    }

    fn said(source: Source, word: &str, raw_kind: &str, weight: f32) -> Said {
        Said { source, word: word.into(), raw_kind: raw_kind.into(), weight }
    }

    fn kind<'a>(w: &'a Words, kind: Kind) -> Vec<(&'a str, &'a [Source])> {
        w.kinds.iter().find(|k| k.kind == kind).map(|k| k.words.iter().map(|w| (w.word.as_str(), w.sources.as_slice())).collect()).unwrap_or_default()
    }

    #[test]
    fn a_files_mood_word_is_read_again_and_mood_is_audiomuses() {
        // The owner's "melancholic" crates of whole albums: a MOOD tag is a genre, a style or a tag, never a mood.
        let mut a = track("a");
        a.mood = vec!["Techno".into(), "religious".into(), "melancholic".into()];
        a.heard = true;
        a.heard_moods = vec!["sad".into()];
        let r = RecordIn { id: "r".into(), tracks: vec![a], said: vec![said(Source::Discogs, "Religious", "style", 1.0), said(Source::Lastfm, "dark", "mood", 1.0)], ..RecordIn::default() };
        // Another record's file genre names Techno: a caller's word, so a style everywhere (GEN.9).
        let mut b = track("b");
        b.genre = vec!["techno".into()];
        let other = RecordIn { id: "o".into(), tracks: vec![b], ..RecordIn::default() };
        let lib = Library::of(&[r.clone(), other], &HashMap::new());
        let w = record(&lib, &r);
        // GEN's sort, by design: "religious" is the genre Religious, whoever says it; Techno a style.
        assert_eq!(kind(&w, Kind::Genre), [("Religious", &[Source::Files, Source::Discogs][..])], "one of the 26, named as the list names it");
        assert_eq!(kind(&w, Kind::Style), [("Techno", &[Source::Files][..])], "one spelling, the library's");
        assert_eq!(kind(&w, Kind::Mood), [("sad", &[Source::Audiomuse][..])], "mood measured only");
        let tags: Vec<&str> = kind(&w, Kind::Tag).into_iter().map(|(w, _)| w).collect();
        assert_eq!(tags, ["melancholic", "dark"], "the files' word kept, the service's above the floor");
        assert!(w.tracks.is_empty(), "one track: nothing differs");
    }

    #[test]
    fn a_word_of_half_the_tracks_is_the_records_and_a_few_tracks_own_theirs() {
        let mut ts: Vec<TrackIn> = (0..4).map(|i| track(&format!("t{i}"))).collect();
        for t in &mut ts {
            t.genre = vec!["Dub".into()];
            t.heard = true;
            t.heard_moods = vec!["relaxed".into()];
        }
        ts[0].genre.push("Hip-Hop".into());
        ts[3].heard_moods = vec!["aggressive".into()];
        let r = RecordIn { id: "r".into(), tracks: ts, ..RecordIn::default() };
        let lib = Library::of(std::slice::from_ref(&r), &HashMap::new());
        let w = record(&lib, &r);
        // GEN's sort, by design: Dub, a file's GENRE and none of the 26, is a style; the record's genres are its links' (Electronic,
        // Reggae by the fixture map), which come with the links (GEN step 3).
        assert_eq!(kind(&w, Kind::Style), [("Dub", &[Source::Files][..])], "Dub on every track: the record's");
        assert!(kind(&w, Kind::Genre).is_empty(), "Hip-Hop is one track's");
        assert_eq!(kind(&w, Kind::Mood), [("relaxed", &[Source::Audiomuse][..])]);
        let t0: Vec<&str> = w.of_track("t0", Kind::Genre).iter().map(|w| w.word.as_str()).collect();
        assert_eq!(t0, ["Hip Hop"], "its own, one of the 26, named as the list names it");
        let t3: Vec<&str> = w.of_track("t3", Kind::Mood).iter().map(|w| w.word.as_str()).collect();
        assert_eq!(t3, ["aggressive"], "a heard track's mood is its own measure");
        assert_eq!(w.of_track("t1", Kind::Genre).len(), 0, "a track like its record reads its record's");
        assert_eq!(w.tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t0", "t3"]);
    }

    #[test]
    fn a_source_switched_off_leaves_its_kind_and_the_hand_never_does() {
        // The Catalogue sheet (0.15): Last.fm off for tags, AudioMuse off for moods; the hand on whatever is asked.
        let mut a = track("a");
        a.heard = true;
        a.heard_moods = vec!["sad".into()];
        a.tag = vec!["night".into()];
        let r = RecordIn { id: "r".into(), tracks: vec![a], said: vec![said(Source::Lastfm, "driving", "tag", 1.0)], ..RecordIn::default() };
        let off = HashMap::from([(Kind::Tag, HashSet::from([Source::Lastfm])), (Kind::Mood, HashSet::from([Source::Audiomuse, Source::Hand]))]);
        let lib = Library::of(std::slice::from_ref(&r), &HashMap::new()).with_off(off);
        let w = record(&lib, &r);
        assert_eq!(kind(&w, Kind::Tag), [("night", &[Source::Files][..])], "Last.fm's tag out, the files' kept");
        assert!(kind(&w, Kind::Mood).is_empty(), "AudioMuse's mood out");
        let mut by_hand = r.clone();
        by_hand.hand.insert(Kind::Mood, vec!["calm".into()]);
        assert_eq!(kind(&record(&lib, &by_hand), Kind::Mood), [("calm", &[Source::Hand][..])], "the hand is never off");
    }

    #[test]
    fn the_hand_wins_its_kind_and_one_spelling_is_shown() {
        let mut a = track("a");
        a.genre = vec!["hip hop".into(), "Hip-Hop".into()];
        let mut r = RecordIn { id: "r".into(), tracks: vec![a], said: vec![said(Source::Musicbrainz, "HIP HOP", "genre", 1.0)], ..RecordIn::default() };
        let aliases = HashMap::from([(Kind::Genre, vec![("Hip Hop".to_string(), vec!["rap".to_string()])])]);
        let lib = Library::of(std::slice::from_ref(&r), &aliases);
        let w = record(&lib, &r);
        assert_eq!(kind(&w, Kind::Genre), [("Hip Hop", &[Source::Files, Source::Musicbrainz][..])], "folded into the alias's word");
        r.hand.insert(Kind::Genre, vec!["Rap".into(), "Soul".into()]);
        r.hand.insert(Kind::Mood, vec!["calm".into()]);
        let w = record(&lib, &r);
        let genre = w.kinds.iter().find(|k| k.kind == Kind::Genre).unwrap();
        assert!(genre.hand);
        assert_eq!(genre.words.iter().map(|w| w.word.as_str()).collect::<Vec<_>>(), ["Hip Hop", "Soul"], "rap is Hip Hop");
        assert_eq!(kind(&w, Kind::Mood), [("calm", &[Source::Hand][..])], "the hand may set a mood");
    }

    #[test]
    fn the_shared_record_reads_as_every_reader_must_see_it() {
        // Decision 0097's one reader test (shared/words.json): the page's tests read the same expectations.
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../../shared/words.json")).unwrap();
        let list = |v: &serde_json::Value| -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default() };
        let r = &fixture["record"];
        let tracks = r["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| TrackIn {
                id: t["id"].as_str().unwrap().into(),
                genre: list(&t["genre"]),
                style: list(&t["style"]),
                mood: list(&t["mood"]),
                tag: list(&t["tag"]),
                heard: t["heard"].as_bool().unwrap_or(false),
                // AudioMuse's styles with their scores (GEN.26): `[name, score]`.
                heard_styles: t["heard_styles"].as_array().map(|a| a.iter().map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_f64().unwrap() as f32)).collect()).unwrap_or_default(),
                heard_moods: list(&t["heard_moods"]),
                hand: HashMap::new(),
            })
            .collect();
        let said = r["said"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| Said { source: Source::of(s["source"].as_str().unwrap()).unwrap(), word: s["word"].as_str().unwrap().into(), raw_kind: s["raw_kind"].as_str().unwrap().into(), weight: s["weight"].as_f64().unwrap() as f32 })
            .collect();
        let hand = r["hand"].as_object().unwrap().iter().map(|(k, v)| (Kind::of(k).unwrap(), list(v))).collect();
        let record_in = RecordIn { id: r["id"].as_str().unwrap().into(), tracks, said, hand };
        // The rest of the library: its files' genres, each kind's aliases.
        let mut elsewhere = track("elsewhere");
        elsewhere.genre = list(&fixture["library"]["genres"]);
        let other = RecordIn { id: "elsewhere".into(), tracks: vec![elsewhere], ..RecordIn::default() };
        let aliases = fixture["library"]["aliases"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (Kind::of(k).unwrap(), v.as_array().unwrap().iter().map(|a| (a["to"].as_str().unwrap().to_string(), list(&a["from"]))).collect()))
            .collect();
        let lib = Library::of(&[record_in.clone(), other], &aliases);
        let words = record(&lib, &record_in);
        assert_eq!(serde_json::to_value(&words).unwrap(), fixture["expect"]["words"]);
        // Each word's crate: the tracks whose words of its kind hold it.
        for (id, want) in fixture["expect"]["crates"].as_object().unwrap() {
            let (kind, word) = id.split_once(':').unwrap();
            let kind = Kind::of(kind).unwrap();
            let held: Vec<String> = record_in.tracks.iter().filter(|t| words.of_track(&t.id, kind).iter().any(|w| super::super::key(&w.word) == super::super::key(word))).map(|t| t.id.clone()).collect();
            assert_eq!(held, list(want), "{id}");
        }
    }

    #[test]
    fn a_passing_service_word_stays_out_and_the_services_keep_their_strongest() {
        let mut said_: Vec<Said> = (0..9).map(|i| said(Source::Lastfm, &format!("tag {i}"), "tag", 0.9 - i as f32 * 0.05)).collect();
        said_.push(said(Source::Lastfm, "jazzy", "tag", 0.2));
        let r = RecordIn { id: "r".into(), tracks: vec![track("a")], said: said_, ..RecordIn::default() };
        let lib = Library::of(std::slice::from_ref(&r), &HashMap::new());
        let words = record(&lib, &r);
        let tags: Vec<&str> = kind(&words, Kind::Tag).into_iter().map(|(w, _)| w).collect();
        assert_eq!(tags, ["tag 0", "tag 1", "tag 2", "tag 3", "tag 4"]);
    }

    // ---- GEN steps 2–5 (SCENARIOS-GEN): the sort, the hand, AudioMuse's weight, Bandcamp's words. The links (GEN step 3's
    // GEN.16–GEN.22, GEN.24, GEN.51) wait for the owner's map: where a scenario's Then names a link-given genre, it comes with them.

    fn tracks(n: usize) -> Vec<TrackIn> {
        (1..=n).map(|i| track(&format!("t{i}"))).collect()
    }

    fn rec(id: &str, tracks: Vec<TrackIn>, said: Vec<Said>) -> RecordIn {
        RecordIn { id: id.into(), tracks, said, ..RecordIn::default() }
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    /// A kind's words of a record, each by its key ("Deep House": "deep house").
    fn keys(w: &Words, k: Kind) -> Vec<String> {
        kind(w, k).into_iter().map(|(w, _)| super::super::key(w)).collect()
    }

    /// A track's words of a kind as its readers take them, each by its key.
    fn keys_of(w: &Words, track: &str, k: Kind) -> Vec<String> {
        w.of_track(track, k).iter().map(|x| super::super::key(&x.word)).collect()
    }

    /// Whether a word (by its key) is shown in a kind anywhere in a record's answer: the record's or any track's.
    fn shows(w: &Words, k: Kind, word: &str) -> bool {
        w.kinds.iter().chain(w.tracks.iter().flat_map(|t| t.kinds.iter())).filter(|x| x.kind == k).flat_map(|x| x.words.iter()).any(|x| super::super::key(&x.word) == super::super::key(word))
    }

    fn shown_anywhere(w: &Words, word: &str) -> bool {
        KINDS.into_iter().any(|k| shows(w, k, word))
    }

    /// GEN.4: the services' stored words split as they are read, each by its service's rule, the owner's exceptions first; the
    /// stored rows as they were.
    #[test]
    fn stored_service_words_split_as_they_are_read() {
        // Funk Stack (al-114): Discogs' master as the harvest stores it. Signal Path (al-115): MusicBrainz's group's.
        let funk = rec("al-114", vec![track("t1")], vec![said(Source::Discogs, "Funk / Soul", "genre", 1.0), said(Source::Discogs, "RnB/Swing", "style", 1.0), said(Source::Discogs, "Cut-up/DJ", "style", 1.0)]);
        let signal = rec("al-115", vec![track("t1")], vec![said(Source::Musicbrainz, "deep house", "genre", 1.0), said(Source::Musicbrainz, "r&b/hip-hop", "tag", 0.67)]);
        let lib = Library::of(&[funk.clone(), signal.clone()], &HashMap::new());
        let w = record(&lib, &funk);
        assert_eq!(kind(&w, Kind::Genre), [("Funk", &[Source::Discogs][..]), ("R&B", &[Source::Discogs][..]), ("Soul", &[Source::Discogs][..])], "the services' three, Discogs' 0.8 each");
        assert_eq!(kind(&w, Kind::Style), [("New Jack Swing", &[Source::Discogs][..]), ("Turntablism", &[Source::Discogs][..])]);
        // Turntablism's links (Hip Hop, Electronic) come with the links.
        let s = record(&lib, &signal);
        assert_eq!(kind(&s, Kind::Style), [("deep house", &[Source::Musicbrainz][..])]);
        assert_eq!(kind(&s, Kind::Genre), [("Hip Hop", &[Source::Musicbrainz][..]), ("R&B", &[Source::Musicbrainz][..])], "MusicBrainz's \"/\": 0.67 × 0.5, above the floor");
        // Never: a compound, or a word an exception's form holds, as one word anywhere; a stored row rewritten.
        for (r, w) in [(&funk, &w), (&signal, &s)] {
            for whole in ["Funk / Soul", "RnB/Swing", "Cut-up/DJ", "r&b/hip-hop", "Swing", "DJ", "Cut-up"] {
                assert!(!shown_anywhere(w, whole), "{}: {whole}", r.id);
            }
        }
        assert_eq!((funk.said[0].word.as_str(), signal.said[1].word.as_str()), ("Funk / Soul", "r&b/hip-hop"), "the stored words as stored");
    }

    /// GEN.9: a caller's word (Discogs', MusicBrainz's genres, a file's GENRE) is a style everywhere, and only while one says it;
    /// a Last.fm or MusicBrainz tag, AudioMuse and the hand make no style for anyone.
    #[test]
    fn a_callers_word_is_a_style_everywhere_and_only_while_one_says_it() {
        let a = rec("a", vec![track("t1")], vec![said(Source::Discogs, "Deep House", "style", 1.0), said(Source::Discogs, "Dub", "style", 1.0)]);
        let b = rec("b", vec![track("t1")], vec![said(Source::Musicbrainz, "hauntology", "genre", 1.0)]);
        let mut c = rec("c", tracks(1), Vec::new());
        c.tracks[0].genre = strings(&["Female Vocalists"]);
        let d = rec(
            "d",
            vec![track("t1")],
            vec![
                said(Source::Lastfm, "deep house", "tag", 0.9),
                said(Source::Lastfm, "hauntology", "tag", 0.6),
                said(Source::Lastfm, "female vocalists", "tag", 0.5),
                said(Source::Lastfm, "seen live", "tag", 0.5),
                said(Source::Lastfm, "melancholic", "tag", 0.4),
                said(Source::Musicbrainz, "dub", "tag", 0.4),
            ],
        );
        // Others' words no caller says: a Last.fm tag, a MusicBrainz tag, AudioMuse's, the hand's.
        let mut e = rec("e", tracks(1), vec![said(Source::Lastfm, "chillwave", "tag", 1.0), said(Source::Musicbrainz, "witch house", "tag", 1.0)]);
        e.tracks[0].heard = true;
        e.tracks[0].heard_styles = vec![("glitch hop".into(), 0.9)];
        e.hand.insert(Kind::Style, strings(&["Wonky"]));
        let f = rec("f", vec![track("t1")], ["chillwave", "witch house", "glitch hop", "wonky"].iter().map(|t| said(Source::Lastfm, t, "tag", 1.0)).collect());
        let all = [a.clone(), b.clone(), c.clone(), d.clone(), e.clone(), f.clone()];
        let lib = Library::of(&all, &HashMap::new());
        let w = record(&lib, &d);
        assert_eq!(keys(&w, Kind::Style), ["deep house", "hauntology", "female vocalists", "dub"]);
        assert_eq!(keys(&w, Kind::Tag), ["seen live", "melancholic"]);
        assert!(keys(&w, Kind::Genre).is_empty(), "only the links' (Dance; Electronic, Reggae), which come with the links");
        assert_eq!(keys(&record(&lib, &f), Kind::Tag), ["chillwave", "glitch hop", "witch house", "wonky"], "no style made for others");
        assert!(keys(&record(&lib, &f), Kind::Style).is_empty());
        // B gone, nobody calls "hauntology" any more: a tag at the next build. Nothing stored changes.
        let lib = Library::of(&[a, c, d.clone(), e, f], &HashMap::new());
        let w = record(&lib, &d);
        assert!(keys(&w, Kind::Tag).contains(&"hauntology".to_string()) && !keys(&w, Kind::Style).contains(&"hauntology".to_string()));
        assert_eq!(d.said.len(), 6);
    }

    /// GEN.10: STYLE, Grouping, MOOD and AudioMuse sorted by the word.
    #[test]
    fn every_sorted_source_sorts_by_the_word() {
        let elsewhere = rec("x", tracks(1), vec![said(Source::Discogs, "Deep House", "style", 1.0), said(Source::Discogs, "Techno", "style", 1.0), said(Source::Discogs, "House", "style", 1.0)]);
        let mut chill = rec("y", tracks(1), Vec::new());
        chill.tracks[0].genre = strings(&["Chillout"]);
        let mut lantern = rec("al-101", tracks(2), Vec::new());
        let t1 = &mut lantern.tracks[0];
        t1.style = strings(&["Jazz", "Deep House"]);
        t1.tag = strings(&["Deep House", "Jazz", "night drive"]);
        t1.mood = strings(&["Techno", "religious", "melancholic"]);
        t1.heard = true;
        t1.heard_styles = vec![("electronic".into(), 0.608), ("house".into(), 0.561), ("indie".into(), 0.540), ("chillout".into(), 0.512), ("hip-hop".into(), 0.497)];
        lantern.tracks[1].heard = true;
        lantern.tracks[1].heard_styles = vec![("80s".into(), 0.5)];
        let lib = Library::of(&[elsewhere, chill, lantern.clone()], &HashMap::new()).with_rise(0.25);
        let w = record(&lib, &lantern);
        let table = [
            ("t1", "Jazz", Kind::Genre),
            ("t1", "Deep House", Kind::Style),
            ("t1", "night drive", Kind::Tag),
            ("t1", "Techno", Kind::Style),
            ("t1", "Religious", Kind::Genre),
            ("t1", "melancholic", Kind::Tag),
            ("t1", "Electronic", Kind::Genre),
            ("t1", "House", Kind::Style),
            ("t1", "indie", Kind::Tag),
            ("t1", "chillout", Kind::Style),
            ("t1", "Hip Hop", Kind::Genre),
            ("t2", "80s", Kind::Tag),
        ];
        for (t, word, want) in table {
            for k in KINDS {
                assert_eq!(keys_of(&w, t, k).contains(&super::super::key(word)), k == want, "{t}: {word} as {}", k.name());
            }
        }
        // Never a mood but AudioMuse's (it heard none here).
        assert!(kind(&w, Kind::Mood).is_empty() && w.tracks.iter().all(|t| t.kinds.iter().all(|k| k.kind != Kind::Mood || k.words.is_empty())));
    }

    /// GEN.11: Discogs and MusicBrainz sort by the word, not by their own kind.
    #[test]
    fn services_words_sort_by_the_word() {
        let a116 = rec("al-116", tracks(1), vec![said(Source::Discogs, "Stage & Screen", "genre", 1.0), said(Source::Discogs, "Non-Music", "genre", 1.0), said(Source::Discogs, "Brass & Military", "genre", 1.0), said(Source::Discogs, "Score", "style", 1.0)]);
        let a117 = rec("al-117", tracks(1), vec![said(Source::Discogs, "Electronic", "genre", 1.0), said(Source::Discogs, "Ambient", "style", 1.0), said(Source::Discogs, "Experimental", "style", 1.0)]);
        let a118 = rec("al-118", tracks(1), vec![said(Source::Musicbrainz, "edm", "genre", 1.0), said(Source::Musicbrainz, "deep house", "genre", 0.5), said(Source::Musicbrainz, "dance", "tag", 0.75)]);
        let lib = Library::of(&[a116.clone(), a117.clone(), a118.clone()], &HashMap::new());
        let w = record(&lib, &a116);
        let mut styles = keys(&w, Kind::Style);
        styles.sort();
        assert_eq!(styles, ["brass military", "non music", "score", "stage screen"]);
        assert!(keys(&w, Kind::Genre).is_empty(), "Soundtrack and Spoken Word by their links, which come with the links");
        let w = record(&lib, &a117);
        let mut genres = keys(&w, Kind::Genre);
        genres.sort();
        assert_eq!(genres, ["ambient", "electronic", "experimental"], "a Discogs style of the 26 is a genre");
        assert!(keys(&w, Kind::Style).is_empty());
        let w = record(&lib, &a118);
        assert_eq!(kind(&w, Kind::Genre), [("Dance", &[Source::Musicbrainz][..])], "one word from the genre \"edm\" and the tag \"dance\"");
        assert_eq!(keys(&w, Kind::Style), ["deep house"]);
    }

    /// GEN.12: an ID3v1 number is named first, in every field; a non-word (digits alone, or a word only a file's STYLE uses that
    /// no caller knows) is no word; a STYLE word another source uses is sorted by the word.
    #[test]
    fn a_non_word_is_no_word_and_its_raw_value_stays() {
        let callers = rec("c", tracks(1), vec![said(Source::Discogs, "Classical", "genre", 1.0), said(Source::Discogs, "Sonata", "style", 1.0), said(Source::Discogs, "House", "style", 1.0), said(Source::Discogs, "Techno", "style", 1.0)]);
        let mut brahms = rec("al-106", tracks(3), Vec::new());
        for t in &mut brahms.tracks {
            t.genre = strings(&["Classical"]);
        }
        brahms.tracks[0].style = strings(&["Sonata", "op. 100 \"Thun\""]);
        brahms.tracks[1].style = strings(&["Sonata", "op. 108"]);
        brahms.tracks[2].style = strings(&["F-A-E Sonata"]);
        let mut silent = rec("al-107", tracks(2), Vec::new());
        for t in &mut silent.tracks {
            t.genre = strings(&["Jazz", "Fusion"]);
        }
        silent.tracks[0].style = strings(&["In a Silent Way"]);
        silent.tracks[1].style = strings(&["It's About That Time"]);
        let mut tapes = rec("al-119", tracks(1), Vec::new());
        tapes.tracks[0].style = strings(&["26", "35", "2001"]);
        tapes.tracks[0].tag = strings(&["1999"]);
        tapes.tracks[0].mood = strings(&["18"]);
        let mut indie = rec("indie", tracks(2), Vec::new());
        indie.tracks[0].style = strings(&["indie"]);
        indie.tracks[1].heard = true;
        indie.tracks[1].heard_styles = vec![("indie".into(), 0.6)];
        let mut numbers = rec("numbers", tracks(1), Vec::new());
        numbers.tracks[0].genre = strings(&["17", "(17)", "13"]);
        // A Last.fm tag no caller says, which a file's STYLE elsewhere holds too: STYLE is no caller, so a tag.
        let lastfm = rec("lastfm", tracks(1), vec![said(Source::Lastfm, "Chamber", "tag", 1.0)]);
        let mut chamber = rec("chamber", tracks(1), Vec::new());
        chamber.tracks[0].style = strings(&["Chamber"]);
        let all = [callers, brahms.clone(), silent.clone(), tapes.clone(), indie.clone(), numbers.clone(), lastfm.clone(), chamber];
        let lib = Library::of(&all, &HashMap::new());
        let w = record(&lib, &tapes);
        assert_eq!((keys(&w, Kind::Genre), keys(&w, Kind::Style), keys(&w, Kind::Tag)), (strings(&["ambient"]), strings(&["house", "techno"]), Vec::new()), "26 Ambient, 35 House, 18 Techno");
        let w = record(&lib, &numbers);
        assert_eq!(kind(&w, Kind::Genre), [("Pop", &[Source::Files][..]), ("Rock", &[Source::Files][..])], "17 and (17) Rock, 13 Pop");
        let w = record(&lib, &brahms);
        assert_eq!((keys(&w, Kind::Genre), keys(&w, Kind::Style)), (strings(&["classical"]), strings(&["sonata"])));
        let w = record(&lib, &silent);
        assert_eq!((keys(&w, Kind::Genre), keys(&w, Kind::Style)), (strings(&["jazz"]), strings(&["fusion"])));
        let w = record(&lib, &indie);
        assert!(keys_of(&w, "t1", Kind::Tag).contains(&"indie".to_string()) && !shows(&w, Kind::Style, "indie"), "AudioMuse uses it too: sorted by the word, a tag");
        assert!(shows(&record(&lib, &lastfm), Kind::Tag, "Chamber") && !shows(&record(&lib, &lastfm), Kind::Style, "Chamber"), "a non-word makes no style");
        // Never: a non-word in any kind, a number shown as digits.
        for (r, words) in [
            (&brahms, &["op. 100 \"Thun\"", "op. 108", "F-A-E Sonata"][..]),
            (&silent, &["In a Silent Way", "It's About That Time"][..]),
            (&tapes, &["26", "35", "2001", "1999", "18"][..]),
            (&numbers, &["17", "13"][..]),
        ] {
            let w = record(&lib, r);
            for word in words {
                assert!(!shown_anywhere(&w, word), "{}: {word}", r.id);
            }
        }
        assert_eq!(tapes.tracks[0].style, ["26", "35", "2001"], "the raw values are the file's, as read");
    }

    /// GEN.13: a word only a file's STYLE uses is a style while a caller says it, and no word before and after.
    #[test]
    fn a_non_word_is_a_style_while_a_caller_says_it() {
        let mut held = rec("held", tracks(1), Vec::new());
        held.tracks[0].style = strings(&["Chamber"]);
        let mut arrival = rec("arrival", tracks(1), Vec::new());
        arrival.tracks[0].genre = strings(&["Chamber"]);
        let alone = || Library::of(std::slice::from_ref(&held), &HashMap::new());
        assert!(!shown_anywhere(&record(&alone(), &held), "Chamber"), "no caller: no word");
        let with = Library::of(&[held.clone(), arrival], &HashMap::new());
        assert_eq!(keys(&record(&with, &held), Kind::Style), ["chamber"], "while the arrival's GENRE says it");
        assert!(!shown_anywhere(&record(&alone(), &held), "Chamber"), "the arrival gone: no word again");
    }

    /// GEN.14 (OQ-GEN-9): a genre alias renames, then its word is sorted; one outside the 26 makes no genre.
    #[test]
    fn a_genre_alias_makes_no_genre_outside_the_26() {
        let aliases = HashMap::from([(Kind::Genre, vec![("Electronic".to_string(), strings(&["Electronica", "Electronic Music"])), ("Drum & Bass".to_string(), strings(&["dnb", "Drum n Bass"]))])]);
        let mut r = rec("r", tracks(1), Vec::new());
        r.tracks[0].genre = strings(&["Electronica", "DnB"]);
        let lib = Library::of(std::slice::from_ref(&r), &aliases);
        let w = record(&lib, &r);
        assert_eq!(kind(&w, Kind::Genre), [("Electronic", &[Source::Files][..])]);
        assert_eq!(kind(&w, Kind::Style), [("Drum & Bass", &[Source::Files][..])], "renamed, then sorted: a file's GENRE names it, so a style");
        assert!(!shown_anywhere(&w, "DnB") && !shows(&w, Kind::Genre, "Drum & Bass"));
    }

    /// GEN.23 (a5's call): a hand genre list is final, at either level; the styles still shown. No links exist yet: written first
    /// so they land under it.
    #[test]
    fn a_hand_genre_list_is_final() {
        let mut lantern = rec("al-101", tracks(10), Vec::new());
        for (i, t) in lantern.tracks.iter_mut().enumerate() {
            t.genre = if i < 5 { strings(&["Electronic", "Deep House"]) } else { strings(&["Electronic"]) };
        }
        lantern.hand.insert(Kind::Genre, strings(&["Electronic"]));
        let mut night = rec("al-102", tracks(10), Vec::new());
        for (i, t) in night.tracks.iter_mut().enumerate() {
            t.genre = if i == 2 || i == 6 { strings(&["Electronic", "Deep House"]) } else { strings(&["Electronic"]) };
        }
        night.tracks[2].hand.insert(Kind::Genre, strings(&["Jazz"]));
        let mut moss = rec("al-105", tracks(8), Vec::new());
        for t in &mut moss.tracks {
            t.genre = strings(&["Acid Jazz"]);
        }
        moss.tracks[3].genre.push("Deep House".into());
        moss.hand.insert(Kind::Genre, strings(&["Jazz"]));
        let lib = Library::of(&[lantern.clone(), night.clone(), moss.clone()], &HashMap::new());
        let w = record(&lib, &lantern);
        let genre = w.kinds.iter().find(|k| k.kind == Kind::Genre).unwrap();
        assert!(genre.hand && keys(&w, Kind::Genre) == ["electronic"], "the hand's, marked so");
        assert_eq!(keys(&w, Kind::Style), ["deep house"], "no hand style list: Deep House still shown");
        let w = record(&lib, &night);
        assert_eq!((keys(&w, Kind::Genre), keys(&w, Kind::Style)), (strings(&["electronic"]), Vec::new()));
        assert_eq!((keys_of(&w, "t3", Kind::Genre), keys_of(&w, "t3", Kind::Style)), (strings(&["jazz"]), strings(&["deep house"])), "t3's own: the hand's genres, its Deep House its style");
        let w = record(&lib, &moss);
        assert_eq!((keys(&w, Kind::Genre), keys_of(&w, "t4", Kind::Genre)), (strings(&["jazz"]), strings(&["jazz"])), "a record's hand kind is every track's");
        assert_eq!(keys_of(&w, "t4", Kind::Style), ["acid jazz", "deep house"]);
        for r in [&lantern, &night, &moss] {
            assert!(!shown_anywhere(&record(&lib, r), "Dance"), "{}: never a link-given genre beside a hand list", r.id);
        }
    }

    /// GEN.25 (C6, OQ-GEN-16, C12, R9): a typed word is routed by the word and edits its own kind's row as that row is shown;
    /// only a row that gains or loses a word of its own kind becomes a hand list. The exceptions apply to a typed word, the split
    /// doesn't (GEN.1 (d)).
    #[test]
    fn a_routed_hand_word_edits_its_own_row_as_shown() {
        let s = |w: &[&str]| strings(w);
        let k = |list: &Option<Vec<String>>| list.as_ref().map(|l| l.iter().map(|w| super::super::key(w)).collect::<Vec<_>>());
        let ks = |w: &[&str]| Some(w.iter().map(|x| super::super::key(x)).collect::<Vec<_>>());
        // (rows shown, rows sent, the lists made)
        let cases: [((Vec<String>, Vec<String>), (Option<Vec<String>>, Option<Vec<String>>), (Option<Vec<String>>, Option<Vec<String>>)); 7] = [
            // Lantern Rooms: "Jazz" and "Wonky Jazz" typed into the style row: both rows edited, Dance stays.
            ((s(&["Electronic", "Dance"]), s(&["Deep House"])), (None, Some(s(&["Deep House", "Jazz", "Wonky Jazz"]))), (ks(&["Electronic", "Dance", "Jazz"]), ks(&["Deep House", "Wonky Jazz"]))),
            // Pale Dock: "Dance" typed into the style row: the genre row's, as shown; the style row gained and lost no style.
            (
                (s(&["Electronic", "Dance", "Reggae"]), s(&["Abstract", "Deep House", "Dub", "Electro", "Glitch"])),
                (None, Some(s(&["Abstract", "Deep House", "Dub", "Electro", "Glitch", "Dance"]))),
                (ks(&["Electronic", "Dance", "Reggae"]), None),
            ),
            // Grey Archive (R9): "Leftfield" typed into the genre row edits the style row; the genre row isn't edited.
            ((Vec::new(), s(&["Hauntology"])), (Some(s(&["Leftfield"])), None), (None, ks(&["Hauntology", "Leftfield"]))),
            // Night Shift (C12): "Jazz" alone in the style row edits the genre row only.
            ((s(&["Electronic"]), Vec::new()), (None, Some(s(&["Jazz"]))), (ks(&["Electronic", "Jazz"]), None)),
            // Typed exceptions: "RnB/Swing" is R&B and New Jack Swing, each to its own row.
            ((Vec::new(), Vec::new()), (None, Some(s(&["RnB/Swing"]))), (ks(&["R&B"]), ks(&["New Jack Swing"]))),
            // Typed words aren't split: "Drum/Bass" and "Funk / Soul" are one word each, styles.
            ((s(&["Jazz"]), Vec::new()), (Some(s(&["Jazz", "Funk / Soul"])), Some(s(&["Drum/Bass"]))), (None, ks(&["Drum/Bass", "Funk / Soul"]))),
            // A row sent as shown is no edit.
            ((s(&["Jazz"]), s(&["Fusion"])), (Some(s(&["Jazz"])), Some(s(&["Fusion"]))), (None, None)),
        ];
        for ((shown_g, shown_s), (g, st), (want_g, want_s)) in cases {
            let r = route(&shown_g, &shown_s, g.as_deref(), st.as_deref());
            assert_eq!((k(&r.genres), k(&r.styles)), (want_g, want_s), "shown {shown_g:?} {shown_s:?}, sent {g:?} {st:?}");
            // Never: a word of the 26 in a hand style list, one outside them in a hand genre list; a word shown just before gone.
            let genres = cratarr_rules::genres::shipped();
            assert!(r.genres.iter().flatten().all(|w| genres.genre_of(w).is_some()), "{:?}", r.genres);
            assert!(r.styles.iter().flatten().all(|w| genres.genre_of(w).is_none()), "{:?}", r.styles);
            let after: HashSet<String> = r.genres.clone().unwrap_or(shown_g.clone()).iter().chain(r.styles.clone().unwrap_or(shown_s.clone()).iter()).map(|w| super::super::key(w)).collect();
            assert!(shown_g.iter().chain(&shown_s).all(|w| after.contains(&super::super::key(w))), "nothing shown lost: {after:?}");
        }
    }

    /// GEN.27: a strong AudioMuse style on few tracks against a weak one on many: the average over the heard tracks decides.
    #[test]
    fn audiomuse_rises_by_its_average_score() {
        let elsewhere = rec("x", tracks(1), vec![said(Source::Discogs, "Dub Techno", "style", 1.0), said(Source::Discogs, "Downtempo", "style", 1.0)]);
        let mut r = rec("r", tracks(10), Vec::new());
        for (i, t) in r.tracks.iter_mut().enumerate() {
            t.heard = true;
            if i < 4 {
                t.heard_styles.push(("dub techno".into(), 0.65));
            }
            if i < 6 {
                t.heard_styles.push(("downtempo".into(), 0.40));
            }
        }
        let lib = Library::of(&[elsewhere, r.clone()], &HashMap::new()).with_rise(0.25);
        let w = record(&lib, &r);
        assert_eq!(keys(&w, Kind::Style), ["dub techno"], "0.26 on average: the record's (its link's Dance comes with the links)");
        for t in 1..=10 {
            assert_eq!(keys_of(&w, &format!("t{t}"), Kind::Style).contains(&"downtempo".to_string()), t <= 6, "t{t}: downtempo, 0.24 on average, the six tracks' own");
        }
    }

    /// GEN.28: the edge: the average over the heard tracks alone, a heard track without the word counting 0, reaching T.
    #[test]
    fn audiomuses_average_is_over_the_heard_tracks_and_reaches_its_threshold() {
        let mut r = rec("r", tracks(12), Vec::new());
        for (i, t) in r.tracks.iter_mut().enumerate() {
            t.heard = i < 8;
            if i < 4 {
                t.heard_styles.push(("electronic".into(), 0.50));
            } else if i < 8 {
                t.heard_styles.push(("ambient".into(), 0.49));
            }
        }
        let lib = Library::of(std::slice::from_ref(&r), &HashMap::new()).with_rise(0.25);
        let w = record(&lib, &r);
        assert_eq!(kind(&w, Kind::Genre), [("Electronic", &[Source::Audiomuse][..])], "0.25 over the eight heard: the record's");
        for t in 1..=12 {
            assert_eq!(keys_of(&w, &format!("t{t}"), Kind::Genre).contains(&"ambient".to_string()), (5..=8).contains(&t), "t{t}: Ambient, 0.245, the four tracks' own");
        }
    }

    /// GEN.59: Bandcamp's tags, a record's words sorted by the word, at Last.fm's trust; never a caller.
    #[test]
    fn bandcamps_tags_are_a_records_words_sorted_by_the_word() {
        let quay = rec("al-129", tracks(6), ["electronic", "downtempo", "Paris"].iter().map(|t| said(Source::Bandcamp, t, "tag", 1.0)).collect());
        let discogs = rec("x", tracks(1), vec![said(Source::Discogs, "Downtempo", "style", 1.0)]);
        let lib = Library::of(&[quay.clone(), discogs.clone()], &HashMap::new());
        let w = record(&lib, &quay);
        assert_eq!(kind(&w, Kind::Genre), [("Electronic", &[Source::Bandcamp][..])]);
        assert_eq!(kind(&w, Kind::Style), [("Downtempo", &[Source::Bandcamp][..])], "a caller (Discogs) knows the word");
        assert_eq!(kind(&w, Kind::Tag), [("Paris", &[Source::Bandcamp][..])]);
        assert!(kind(&w, Kind::Mood).is_empty() && w.tracks.is_empty(), "every track reads the three, down");
        // Never a caller: with Discogs' record gone, "downtempo" is a tag; Paris is a style nowhere.
        let lib = Library::of(std::slice::from_ref(&quay), &HashMap::new());
        let w = record(&lib, &quay);
        assert_eq!((keys(&w, Kind::Genre), keys(&w, Kind::Style)), (strings(&["electronic"]), Vec::new()));
        assert_eq!(keys(&w, Kind::Tag), ["paris", "downtempo"], "0.5 each, by the word");
        // Last.fm's trust, no rank of its own: at the floor's edge each is kept or left out alike.
        for source in [Source::Bandcamp, Source::Lastfm] {
            let r = rec("edge", tracks(1), vec![said(source, "kept", "tag", 0.3), said(source, "left", "tag", 0.29)]);
            let lib = Library::of(std::slice::from_ref(&r), &HashMap::new());
            assert_eq!(keys(&record(&lib, &r), Kind::Tag), ["kept"], "{}", source.name());
        }
        assert_eq!((Source::of("bandcamp"), Source::Bandcamp.name()), (Some(Source::Bandcamp), "bandcamp"));
        // A Bandcamp tag is one value: no split, the exceptions first.
        let r = rec("split", tracks(1), ["hip-hop/rap", "drum & bass", "RnB/Swing"].iter().map(|t| said(Source::Bandcamp, t, "tag", 1.0)).collect());
        let lib = Library::of(std::slice::from_ref(&r), &HashMap::new());
        let w = record(&lib, &r);
        assert!(keys(&w, Kind::Tag).contains(&"hip hop rap".to_string()) && keys(&w, Kind::Tag).contains(&"drum bass".to_string()));
        assert_eq!(keys(&w, Kind::Genre), ["r b"], "RnB/Swing: R&B and New Jack Swing");
        assert!(keys(&w, Kind::Tag).contains(&"new jack swing".to_string()), "no caller says New Jack Swing here: a tag");
    }

    /// GEN.61: the default is a tag: a place, a mood, any word no caller names, each alike; mood stays AudioMuse's.
    #[test]
    fn a_word_no_caller_names_is_a_tag_whatever_it_means() {
        let mut rue = rec("al-131", tracks(4), ["Paris", "melancholic", "chillwave"].iter().map(|t| said(Source::Bandcamp, t, "tag", 1.0)).collect());
        rue.said.push(said(Source::Lastfm, "melancholic", "tag", 0.4));
        for t in &mut rue.tracks[..3] {
            t.heard = true;
            t.heard_moods = strings(&["calm"]);
        }
        let lib = Library::of(std::slice::from_ref(&rue), &HashMap::new());
        let w = record(&lib, &rue);
        assert_eq!(kind(&w, Kind::Tag), [("melancholic", &[Source::Lastfm, Source::Bandcamp][..]), ("Paris", &[Source::Bandcamp][..]), ("chillwave", &[Source::Bandcamp][..])], "0.7 first, then 0.5 each by the word");
        assert_eq!(kind(&w, Kind::Mood), [("calm", &[Source::Audiomuse][..])]);
        assert_eq!(w.kinds.iter().map(|k| k.kind).collect::<Vec<_>>(), [Kind::Mood, Kind::Tag], "no genre or style by what a word means");
        // A Discogs style "Chillwave" elsewhere: chillwave a style; Paris and melancholic stay tags. The stored rows as they were.
        let other = rec("x", tracks(1), vec![said(Source::Discogs, "Chillwave", "style", 1.0)]);
        let lib = Library::of(&[rue.clone(), other], &HashMap::new());
        let w = record(&lib, &rue);
        assert_eq!((keys(&w, Kind::Style), keys(&w, Kind::Tag)), (strings(&["chillwave"]), strings(&["melancholic", "paris"])));
        assert_eq!(rue.said.len(), 4);
    }
}
