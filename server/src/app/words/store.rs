//! The one store of words (decision 0097, the owner 2026-11-02: "We need a North Star"): each record's genres, styles,
//! moods and tags, and a track's own where they differ, each word with the sources that say it. Every reader reads
//! this: crate conditions, the filter and 0095's reasons (a track's facts), search, the word crates, similarity, a
//! record's rows.
//!
//! - **Layers.** By hand wins for its kind (0089's fields, with Reset); then the files' tags; then AudioMuse (styles,
//!   and moods: mood is AudioMuse's alone); then the services (MusicBrainz, Discogs, Last.fm, the household's tags),
//!   weighted, a word below the floor left out.
//! - **Record and track.** A word of the files or AudioMuse is the record's where half its tracks (half those heard)
//!   have it; else the tracks that have it own it. A track reads its record's words and its own; its mood is its own
//!   measure where it was heard.
//! - **Kind by the word.** A genre where it is a broad genre or any file's genre; a style where a curated source
//!   (MusicBrainz's genre list, Discogs) names it, or a file's STYLE; else a tag. The files' and the services' mood
//!   words are read again so (never a mood): "religious", Discogs' style, a style.
//! - **One spelling.** A word's key folds case, spaces and hyphens (`super::key`); a kind's aliases name the word it
//!   is; its shown spelling is the alias's, else the most written.

use std::collections::{HashMap, HashSet};

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
            _ => return None,
        })
    }

    /// How far a service's word is trusted (0.9.7's): the household's as the files'.
    fn trust(self, raw_kind: &str) -> f32 {
        match (self, raw_kind) {
            (Source::Hand | Source::Files | Source::Audiomuse | Source::Household, _) => 1.0,
            (Source::Musicbrainz, "tag") => 0.5,
            (Source::Musicbrainz, _) => 0.9,
            (Source::Discogs, _) => 0.8,
            (Source::Lastfm, _) => 0.5,
        }
    }
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
    pub heard_styles: Vec<String>,
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

/// What the whole library says, for sorting a word into its kind and naming it: every file's genres and the broad
/// genres, the words a curated source names, and each kind's aliases (a key to the word it is).
#[derive(Debug, Clone, Default)]
pub struct Library {
    genres: HashSet<String>,
    curated: HashSet<String>,
    aliases: HashMap<Kind, HashMap<String, String>>,
    /// Each word's one spelling across the library: the most written (rule 4, one spelling everywhere).
    spellings: HashMap<String, String>,
    /// The sources switched off for a kind (the Catalogue sheet, 0.15): their words leave it, kept to come back.
    off: HashMap<Kind, HashSet<Source>>,
}

impl Library {
    /// From every record's inputs and each kind's aliases (the word, and the spellings that are it).
    pub fn of(records: &[RecordIn], aliases: &HashMap<Kind, Vec<(String, Vec<String>)>>) -> Self {
        let mut genres: HashSet<String> = super::GENRES.iter().map(|g| super::key(g)).collect();
        let mut curated = HashSet::new();
        for r in records {
            genres.extend(r.tracks.iter().flat_map(|t| t.genre.iter().map(|g| super::key(g))));
            curated.extend(r.said.iter().filter(|s| matches!(s.source, Source::Musicbrainz | Source::Discogs) && matches!(s.raw_kind.as_str(), "genre" | "style")).map(|s| super::key(&s.word)));
            curated.extend(r.tracks.iter().flat_map(|t| t.style.iter().map(|s| super::key(s))));
        }
        let aliases = aliases
            .iter()
            .map(|(kind, list)| {
                let by: HashMap<String, String> = list.iter().flat_map(|(to, from)| from.iter().chain(std::iter::once(to)).map(move |f| (super::key(f), to.clone()))).collect();
                (*kind, by)
            })
            .collect();
        // Each word's spellings across the library, the most written shown everywhere.
        let mut written: HashMap<String, HashMap<&str, usize>> = HashMap::new();
        for r in records {
            let tracks = r.tracks.iter().flat_map(|t| t.genre.iter().chain(&t.style).chain(&t.mood).chain(&t.tag).chain(&t.heard_styles).chain(&t.heard_moods));
            for w in tracks.chain(r.said.iter().map(|s| &s.word)) {
                *written.entry(super::key(w)).or_default().entry(w.trim()).or_default() += 1;
            }
        }
        let spellings = written.into_iter().filter_map(|(k, s)| s.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0))).map(|(w, _)| (k, w.to_string()))).collect();
        // A genre alias's word is a genre wherever it comes from.
        let mut lib = Self { genres, curated, aliases, spellings, off: HashMap::new() };
        let named: Vec<String> = lib.aliases.get(&Kind::Genre).map(|a| a.values().map(|w| super::key(w)).collect()).unwrap_or_default();
        lib.genres.extend(named);
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

    /// Whether a source's words are out of a kind.
    fn is_off(&self, kind: Kind, source: Source) -> bool {
        self.off.get(&kind).is_some_and(|o| o.contains(&source))
    }

    /// A word's kind by the word (never a mood: mood is AudioMuse's).
    fn sort(&self, key: &str) -> Kind {
        if self.genres.contains(key) {
            Kind::Genre
        } else if self.curated.contains(key) {
            Kind::Style
        } else {
            Kind::Tag
        }
    }

    /// A word's key in its kind: an alias's word's.
    fn key(&self, kind: Kind, word: &str) -> String {
        let k = super::key(word);
        match self.aliases.get(&kind).and_then(|a| a.get(&k)) {
            Some(to) => super::key(to),
            None => k,
        }
    }

    /// The spelling an alias names, if one does.
    fn named(&self, kind: Kind, key: &str) -> Option<&str> {
        self.aliases.get(&kind)?.get(key).map(String::as_str)
    }
}

/// A word as answered: its one spelling and who says it, strongest first.
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

/// A word on its way: its spellings as written, its weight, its sources, and whether it is kept whatever its weight.
#[derive(Debug, Default)]
struct Heard {
    spellings: HashMap<String, usize>,
    weight: f32,
    sources: Vec<Source>,
    firm: bool,
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

/// A track's words of the files and AudioMuse, by kind and key, with their sources: what it says itself. Its files'
/// MOOD words are read again as genre, style or tag.
fn own_words(lib: &Library, t: &TrackIn) -> Gathered {
    let mut g = Gathered::default();
    for w in &t.genre {
        g.add(lib, Kind::Genre, w, Source::Files, 1.0, true);
    }
    for w in &t.style {
        g.add(lib, Kind::Style, w, Source::Files, 1.0, true);
    }
    for w in &t.tag {
        g.add(lib, Kind::Tag, w, Source::Files, 1.0, true);
    }
    for w in &t.mood {
        let kind = lib.sort(&super::key(w));
        g.add(lib, kind, w, Source::Files, 1.0, true);
    }
    if t.heard {
        for w in &t.heard_styles {
            g.add(lib, Kind::Style, w, Source::Audiomuse, 1.0, true);
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
    // The files' and AudioMuse's words that are the record's: on half its tracks (AudioMuse's: half those heard).
    let mut on: HashMap<(Kind, String), (usize, usize)> = HashMap::new();
    for g in &own {
        for (at, h) in &g.by {
            let e = on.entry(at.clone()).or_default();
            if h.sources.contains(&Source::Files) {
                e.0 += 1;
            }
            if h.sources.contains(&Source::Audiomuse) {
                e.1 += 1;
            }
        }
    }
    let half = |n: usize, of: usize| of > 0 && n * 2 >= of;
    let mut gathered = Gathered::default();
    for g in &own {
        for at in &g.order {
            let (files, heard_by) = on[at];
            let h = &g.by[at];
            for (source, by) in [(Source::Files, half(files, tracks)), (Source::Audiomuse, half(heard_by, heard))] {
                if by && h.sources.contains(&source) {
                    for spelling in h.spellings.keys() {
                        gathered.add(lib, at.0, spelling, source, 1.0, true);
                    }
                }
            }
        }
    }
    for s in &r.said {
        // Mood is AudioMuse's: a service's mood word is read again by the word, as every word of theirs is.
        let kind = lib.sort(&super::key(&s.word));
        gathered.add(lib, kind, &s.word, s.source, s.weight.clamp(0.0, 1.0) * s.source.trust(&s.raw_kind), false);
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
        // Another record's file genre makes Techno a genre everywhere.
        let mut b = track("b");
        b.genre = vec!["techno".into()];
        let other = RecordIn { id: "o".into(), tracks: vec![b], ..RecordIn::default() };
        let lib = Library::of(&[r.clone(), other], &HashMap::new());
        let w = record(&lib, &r);
        assert_eq!(kind(&w, Kind::Genre), [("Techno", &[Source::Files][..])]);
        assert_eq!(kind(&w, Kind::Style), [("Religious", &[Source::Files, Source::Discogs][..])], "one spelling, the library's");
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
        assert_eq!(kind(&w, Kind::Genre).len(), 1, "Dub; Hip-Hop is one track's");
        assert_eq!(kind(&w, Kind::Mood), [("relaxed", &[Source::Audiomuse][..])]);
        let t0: Vec<&str> = w.of_track("t0", Kind::Genre).iter().map(|w| w.word.as_str()).collect();
        assert_eq!(t0, ["Dub", "Hip-Hop"], "its record's and its own");
        let t3: Vec<&str> = w.of_track("t3", Kind::Mood).iter().map(|w| w.word.as_str()).collect();
        assert_eq!(t3, ["aggressive"], "a heard track's mood is its own measure");
        assert_eq!(w.of_track("t1", Kind::Genre).len(), 1, "a track like its record reads its record's");
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
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../shared/words.json")).unwrap();
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
                heard_styles: list(&t["heard_styles"]),
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
}
