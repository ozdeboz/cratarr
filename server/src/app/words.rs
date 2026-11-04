//! The library's words (0.9.7; decision 0097, the one store): every record's genres, styles, moods and tags from
//! every source, merged in `store` and read by every reader through the crates' facts (`build`: each file's facts take
//! its record's words and its own), and by a record's view and a word's menu (`stored`).
//!
//! - **Sources**: the files' tags (Genre, STYLE, MOOD read again, Grouping), AudioMuse's styles and moods (mood is
//!   AudioMuse's alone), MusicBrainz's genres and tags, Discogs' genres and styles, Last.fm's tags (by album, and by
//!   artist for each of the artist's albums at half their weight), the household's album tags, and what is set by
//!   hand (0089).
//! - **Kind** by the word, not by who said it (`store::Library`); **weight** and floor as 0.9.7's (`store::FLOOR`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Result;

pub mod catalogue;
pub mod store;

/// The kinds, in search's order.
pub const KINDS: [&str; 4] = ["genre", "style", "mood", "tag"];

/// The broad genres (Discogs' top level, and the names others give its parts): a word among these is a genre.
const GENRES: [&str; 26] = [
    "rock",
    "electronic",
    "jazz",
    "hip hop",
    "funk soul",
    "funk",
    "soul",
    "classical",
    "pop",
    "folk world country",
    "folk",
    "world",
    "country",
    "reggae",
    "latin",
    "blues",
    "stage screen",
    "soundtrack",
    "non music",
    "children s",
    "brass military",
    "metal",
    "punk",
    "r b",
    "indie",
    "alternative",
];

/// An artist's words count for each of their albums at this share.
const ARTIST_SHARE: f32 = 0.5;

/// Words changed, by any way (a record's or a track's set by hand, aliases, a merge, a kind's sources or writing): the
/// pages read the open record, the Library and Health again (the live channel's `changed` of area `words`).
static CHANGED: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> = std::sync::OnceLock::new();

fn changed_channel() -> &'static tokio::sync::broadcast::Sender<()> {
    CHANGED.get_or_init(|| tokio::sync::broadcast::channel(16).0)
}

/// Told that the words changed.
pub fn tell_changed() {
    let _ = changed_channel().send(());
}

/// Listened to by the live channel.
pub fn subscribe() -> tokio::sync::broadcast::Receiver<()> {
    changed_channel().subscribe()
}

/// A word's key: its words as search reads them ("Hip-Hop" and "hip hop" are one).
pub fn key(word: &str) -> String {
    crate::search::words(word).join(" ")
}

/// What the merge is made from, cheaply: when the library was built, and the words and the album tags as kept.
pub fn stamp(serve: &crate::serve::Serve) -> i64 {
    let words = serve.userdata.words_stamp().unwrap_or(0);
    serve.built_at().wrapping_mul(1_000_003).wrapping_add(words)
}

/// The store as last built with the crates' facts (`build`): each record's words by its id (Cratarr's own library's),
/// and every word by kind and key with its one spelling and every source that says it anywhere.
#[derive(Debug, Default)]
pub struct Stored {
    pub records: HashMap<String, store::Words>,
    pub words: HashMap<(store::Kind, String), (String, Vec<store::Source>)>,
}

impl Stored {
    /// A word as a word's menu names it: its kind, its one spelling, its sources (`kind:` as the word crates' ids).
    pub fn word(&self, kind: &str, word: &str) -> Option<WordOf> {
        let kind = store::Kind::of(kind)?;
        let (shown, sources) = self.words.get(&(kind, key(word)))?;
        Some(WordOf { kind, word: shown.clone(), sources: sources.clone() })
    }
}

/// A word with every source that says it anywhere (a word crate's `word`: its menu's header).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WordOf {
    pub kind: store::Kind,
    pub word: String,
    pub sources: Vec<store::Source>,
}

static STORED: Mutex<Option<Arc<Stored>>> = Mutex::new(None);

/// The store as last built, without building it: for a reader that holds the facts it was built with.
pub fn stored_now() -> Option<Arc<Stored>> {
    STORED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The store as built with the crates' facts, built now where they were not. Blocking.
pub fn stored(serve: &crate::serve::Serve) -> Result<Arc<Stored>> {
    serve.catalogue().crate_facts()?;
    Ok(STORED.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default())
}

/// The store built (decision 0097) from the crates' facts as read (each file's tags and AudioMuse's), the services'
/// words, the household's tags and what is set by hand, and the facts' words made the store's: a file's genres,
/// styles, moods and tags become its record's and its own, so every reader of the facts reads the store. A file of no
/// record of the library is a record of its own. Blocking.
pub fn build(serve: &crate::serve::Serve, album_of: &HashMap<String, String>, facts: &mut [crate::crates::Facts]) -> Result<()> {
    use store::{Kind, RecordIn, Said, Source, TrackIn};
    let track_id = |f: &crate::crates::Facts| f.track.map_or_else(|| f.rel.clone(), crate::serve::ids::song);
    let mut order: Vec<String> = Vec::new();
    let mut files: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in facts.iter().enumerate() {
        let record = f.track.and_then(|t| album_of.get(&crate::serve::ids::song(t))).cloned().unwrap_or_else(|| format!("line:{}", f.rel));
        files
            .entry(record.clone())
            .or_insert_with(|| {
                order.push(record);
                Vec::new()
            })
            .push(i);
    }
    // The services' words and the household's tags, by the records they are said of (an artist's: each of theirs).
    let mut base_of: HashMap<String, String> = HashMap::new();
    let mut by_base: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_artist: HashMap<String, Vec<String>> = HashMap::new();
    for (id, base, artist, _) in serve.library.album_word_inputs()? {
        by_base.entry(base.clone()).or_default().push(id.clone());
        by_artist.entry(artist).or_default().push(id.clone());
        base_of.insert(id, base);
    }
    let mut said: HashMap<String, Vec<Said>> = HashMap::new();
    for (subject, source, word, raw_kind, weight) in serve.userdata.all_words()? {
        let Some(source) = Source::of(&source) else { continue };
        let (held, share) = match subject.split_once(':') {
            Some(("album", base)) => (by_base.get(base), 1.0),
            Some(("artist", artist)) => (by_artist.get(artist), ARTIST_SHARE),
            _ => continue,
        };
        for a in held.into_iter().flatten() {
            said.entry(a.clone()).or_default().push(Said { source, word: word.clone(), raw_kind: raw_kind.clone(), weight: weight as f32 * share });
        }
    }
    for (base, tag) in serve.userdata.all_album_tags()? {
        for a in by_base.get(&base).into_iter().flatten() {
            said.entry(a.clone()).or_default().push(Said { source: Source::Household, word: tag.clone(), raw_kind: "tag".into(), weight: 1.0 });
        }
    }
    let overrides = serve.userdata.overrides()?;
    // Each kind's aliases and the sources switched off for it (the Catalogue sheet, 0.15); a track's own words by hand.
    let aliases = catalogue::aliases(serve);
    let off = catalogue::off(serve);
    let mut own: HashMap<String, HashMap<Kind, Vec<String>>> = HashMap::new();
    for (track, kind, words) in serve.userdata.all_track_words()? {
        if let Some(k) = Kind::of(&kind) {
            own.entry(track).or_default().insert(k, words);
        }
    }
    let records: Vec<RecordIn> = order
        .iter()
        .map(|id| {
            let tracks = files[id]
                .iter()
                .map(|&i| {
                    let f = &facts[i];
                    let (style, mood) = f.file_words.as_deref().cloned().unwrap_or_default();
                    TrackIn {
                        id: track_id(f),
                        genre: f.genres.clone(),
                        style,
                        mood,
                        tag: f.tags.clone(),
                        heard: f.clap_moods.is_some(),
                        heard_styles: f.styles.clone(),
                        heard_moods: f.moods.clone(),
                        hand: own.get(&track_id(f)).cloned().unwrap_or_default(),
                    }
                })
                .collect();
            let hand = base_of.get(id).and_then(|b| overrides.get(b)).map(hand_of).unwrap_or_default();
            RecordIn { id: id.clone(), tracks, said: said.remove(id).unwrap_or_default(), hand }
        })
        .collect();
    let lib = store::Library::of(&records, &aliases).with_off(off);
    let mut out = Stored::default();
    for r in &records {
        let w = store::record(&lib, r);
        for (t, &i) in r.tracks.iter().zip(&files[&r.id]) {
            let list = |k: Kind| -> Vec<String> { w.of_track(&t.id, k).iter().map(|x| x.word.clone()).collect() };
            let f = &mut facts[i];
            (f.genres, f.styles, f.moods, f.tags) = (list(Kind::Genre), list(Kind::Style), list(Kind::Mood), list(Kind::Tag));
            f.file_words = None;
        }
        for k in w.kinds.iter().chain(w.tracks.iter().flat_map(|t| t.kinds.iter())) {
            for x in &k.words {
                let e = out.words.entry((k.kind, key(&x.word))).or_insert_with(|| (x.word.clone(), Vec::new()));
                for s in &x.sources {
                    if !e.1.contains(s) {
                        e.1.push(*s);
                    }
                }
            }
        }
        if !r.id.starts_with("line:") {
            out.records.insert(r.id.clone(), w);
        }
    }
    for (_, sources) in out.words.values_mut() {
        sources.sort();
    }
    *STORED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(out));
    Ok(())
}

/// A record's words set by hand (0089's fields), by kind.
fn hand_of(o: &crate::userdata::overrides::Overrides) -> HashMap<store::Kind, Vec<String>> {
    use store::Kind;
    [(Kind::Genre, &o.genres), (Kind::Style, &o.styles), (Kind::Mood, &o.moods), (Kind::Tag, &o.tags)].into_iter().filter_map(|(k, v)| v.clone().map(|v| (k, v))).collect()
}

/// Every word of the library by kind, with the albums that carry it (by the files' crate facts: the files' own genres,
/// AudioMuse's styles and moods, and the merged words): what search finds a word by. A word shown as a genre is not
/// also a style. The labels too (0.11, quick crates: search's label), a kind apart: a label named as a genre is both.
/// Blocking.
pub fn index(facts: &[crate::crates::Facts], album_of: &HashMap<String, String>, off: &HashSet<i64>) -> Vec<(&'static str, String, usize)> {
    let mut by: HashMap<(&'static str, String), (HashMap<String, usize>, HashSet<&str>)> = HashMap::new();
    // Of the libraries switched on (`off`: the folders of those switched off): what a word's crate holds.
    for f in facts.iter().filter(|f| !off.contains(&f.folder)) {
        let Some(album) = f.track.and_then(|t| album_of.get(&crate::serve::ids::song(t))) else { continue };
        for (kind, list) in [("genre", &f.genres), ("style", &f.styles), ("mood", &f.moods), ("tag", &f.tags), ("label", &f.labels)] {
            for w in list {
                let k = key(w);
                if k.is_empty() {
                    continue;
                }
                let e = by.entry((kind, k)).or_default();
                *e.0.entry(w.clone()).or_default() += 1;
                e.1.insert(album.as_str());
            }
        }
    }
    let genres: HashSet<String> = by.keys().filter(|(kind, _)| *kind == "genre").map(|(_, k)| k.clone()).collect();
    let mut out: Vec<(&'static str, String, usize)> = by
        .into_iter()
        .filter(|((kind, k), _)| matches!(*kind, "genre" | "label") || !genres.contains(k))
        .map(|((kind, _), (spellings, albums))| {
            let shown = spellings.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(w, _)| w).unwrap_or_default();
            (kind, shown, albums.len())
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    out
}

/// The search index (`index`) as of the files' crate facts: made again when they are.
static INDEX: Mutex<Option<(usize, usize, Vec<i64>, Arc<Vec<(&'static str, String, usize)>>)>> = Mutex::new(None);

/// Every word of the library by kind with how many albums carry it, for search. Blocking.
pub fn search_index(serve: &crate::serve::Serve, cat: &crate::catalogue::Catalogue, off: &HashSet<i64>) -> Result<Arc<Vec<(&'static str, String, usize)>>> {
    let facts = cat.crate_facts()?;
    let of = (Arc::as_ptr(&facts) as usize, facts.len());
    let mut folders: Vec<i64> = off.iter().copied().collect();
    folders.sort_unstable();
    if let Some((p, n, f, i)) = INDEX.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && (*p, *n) == of
        && *f == folders
    {
        return Ok(i.clone());
    }
    let made = Arc::new(index(&facts, &serve.library.song_album_map()?, off));
    *INDEX.lock().unwrap_or_else(|e| e.into_inner()) = Some((of.0, of.1, folders, made.clone()));
    Ok(made)
}
