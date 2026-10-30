//! The library's words (0.9.7, the owner: one structured harvest of descriptive words, and global search finding them):
//! every album's genres, styles, moods and tags from every source, each kept with its source and a weight (the user
//! data's `words`, filled by the harvest's passes), merged here by album. Crates read them (`crate:<kind>:<word>`, the
//! genre crate and its three siblings), and so search and similarity.
//!
//! - **Sources**: the files' genres (as the library reads them); MusicBrainz's genres and tags, Discogs' genres and
//!   styles, Last.fm's tags (by album, and by artist for each of the artist's albums at half their weight); the
//!   household's album tags. AudioMuse's styles and CLAP's moods stay each file's own (`crates::Facts`).
//! - **Kind** by the word, not by who said it, so a word is one kind everywhere: a genre where it is one of the broad
//!   genres (`GENRES`) or any file's genre; a style where a curated source (MusicBrainz's genre list, Discogs' styles)
//!   names it; a mood where it is one (`MOODS`); else a tag.
//! - **Weight**: each source's own strength times how far it is trusted (`trust`), summed by album and capped at 1;
//!   a word below `FLOOR` is left out, so agreement raises a word and one passing remark does not make it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Result;

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

/// Moods: a word among these is one, whoever said it.
const MOODS: [&str; 30] = [
    "happy",
    "sad",
    "relaxed",
    "relaxing",
    "aggressive",
    "party",
    "chill",
    "chillout",
    "melancholic",
    "melancholy",
    "mellow",
    "uplifting",
    "dark",
    "energetic",
    "calm",
    "romantic",
    "dreamy",
    "atmospheric",
    "angry",
    "peaceful",
    "sexy",
    "moody",
    "upbeat",
    "hypnotic",
    "epic",
    "nostalgic",
    "sensual",
    "haunting",
    "playful",
    "bittersweet",
];

/// A word below this merged weight is left out.
const FLOOR: f32 = 0.15;
/// An artist's words count for each of their albums at this share.
const ARTIST_SHARE: f32 = 0.5;

/// An album's words of each kind kept, at most: its strongest.
fn most_of(kind: &str) -> usize {
    match kind {
        "genre" | "mood" => 3,
        _ => 5,
    }
}

/// How far a source's word is trusted: the files and the household's own words the most.
fn trust(source: &str, raw_kind: &str) -> f32 {
    match (source, raw_kind) {
        ("files" | "household", _) => 1.0,
        ("musicbrainz", "tag") => 0.5,
        ("musicbrainz", _) => 0.9,
        ("discogs", _) => 0.8,
        ("lastfm", _) => 0.5,
        _ => 0.5,
    }
}

/// A word's key: its words as search reads them ("Hip-Hop" and "hip hop" are one).
pub fn key(word: &str) -> String {
    crate::search::words(word).join(" ")
}

/// A word of an album's, merged.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// As most sources write it.
    pub word: String,
    pub kind: &'static str,
    pub weight: f32,
    /// The sources that said it (musicbrainz, discogs, lastfm, files, household), each once.
    pub sources: Vec<String>,
}

/// The library's words by album id.
#[derive(Debug, Default)]
pub struct Merged {
    pub albums: HashMap<String, Vec<Word>>,
}

/// One source's word for one subject, as the merge reads it.
#[derive(Debug, Clone)]
pub struct Said {
    /// The library's album ids it holds for (an artist's word: each of their albums).
    pub albums: Vec<String>,
    pub source: String,
    pub word: String,
    /// What the source called it: genre, style, mood or tag.
    pub raw_kind: String,
    /// The source's own strength, 0 to 1 (an artist's already shared out).
    pub weight: f32,
}

/// The words merged from what the sources said (`said`): each word's one kind, each album's words with their weights.
pub fn merge(said: &[Said]) -> Merged {
    let curated: HashSet<String> = said.iter().filter(|s| s.raw_kind != "tag" && s.raw_kind != "mood" && s.source != "household").map(|s| key(&s.word)).collect();
    let file_genres: HashSet<String> = said.iter().filter(|s| s.source == "files").map(|s| key(&s.word)).collect();
    let kind_of = |k: &str, said_mood: bool| -> &'static str {
        if file_genres.contains(k) || GENRES.contains(&k) {
            "genre"
        } else if curated.contains(k) {
            "style"
        } else if said_mood || MOODS.contains(&k) {
            "mood"
        } else {
            "tag"
        }
    };
    let mood_said: HashSet<String> = said.iter().filter(|s| s.raw_kind == "mood").map(|s| key(&s.word)).collect();
    // Each key's spellings, the most used shown.
    let mut spellings: HashMap<String, HashMap<&str, usize>> = HashMap::new();
    let mut weights: HashMap<(&str, String), (f32, Vec<String>)> = HashMap::new();
    for s in said {
        let k = key(&s.word);
        if k.is_empty() {
            continue;
        }
        *spellings.entry(k.clone()).or_default().entry(s.word.as_str()).or_default() += 1;
        let w = s.weight.clamp(0.0, 1.0) * trust(&s.source, &s.raw_kind);
        for a in &s.albums {
            let e = weights.entry((a.as_str(), k.clone())).or_default();
            e.0 += w;
            if !e.1.contains(&s.source) {
                e.1.push(s.source.clone());
            }
        }
    }
    let shown: HashMap<String, String> = spellings
        .into_iter()
        .map(|(k, s)| {
            let best = s.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0))).map_or_else(|| k.clone(), |(w, _)| w.to_string());
            (k, best)
        })
        .collect();
    let mut albums: HashMap<String, Vec<Word>> = HashMap::new();
    for ((album, k), (w, sources)) in weights {
        let w = w.min(1.0);
        if w < FLOOR {
            continue;
        }
        let kind = kind_of(&k, mood_said.contains(&k));
        albums.entry(album.to_string()).or_default().push(Word { word: shown.get(&k).cloned().unwrap_or(k), kind, weight: w, sources });
    }
    for words in albums.values_mut() {
        words.sort_by(|a, b| b.weight.total_cmp(&a.weight).then_with(|| a.word.cmp(&b.word)));
        // The strongest of each kind only (0.9.7): every file carries its album's words in the crates' facts, which are
        // held several times over (the radio, the similarity library, search); a long tail of passing tags cost the
        // sim about 90 MB at half the harvest, and said little.
        let mut kept: HashMap<&'static str, usize> = HashMap::new();
        words.retain(|w| {
            let n = kept.entry(w.kind).or_default();
            *n += 1;
            *n <= most_of(w.kind)
        });
    }
    Merged { albums }
}

/// The merged words as of the last build and the last change of the words (`stamp`), made again when either moved.
static MERGED: Mutex<Option<(i64, Arc<Merged>)>> = Mutex::new(None);

/// What the merge is made from, cheaply: when the library was built, and the words and the album tags as kept.
pub fn stamp(serve: &crate::serve::Serve) -> i64 {
    let words = serve.userdata.words_stamp().unwrap_or(0);
    serve.built_at().wrapping_mul(1_000_003).wrapping_add(words)
}

/// The library's words, merged (kept until the library or the words change). Blocking.
pub fn merged(serve: &crate::serve::Serve) -> Result<Arc<Merged>> {
    let now = stamp(serve);
    if let Some((at, m)) = MERGED.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && *at == now
    {
        return Ok(m.clone());
    }
    let mut said = Vec::new();
    let rules = serve.rules();
    // The library's albums: their ids under their Music-form id (the user data's subjects), by artist.
    let albums = serve.library.album_word_inputs()?;
    let mut by_base: HashMap<&str, Vec<String>> = HashMap::new();
    let mut by_artist: HashMap<&str, Vec<String>> = HashMap::new();
    for (id, base, artist, genres) in &albums {
        by_base.entry(base.as_str()).or_default().push(id.clone());
        by_artist.entry(artist.as_str()).or_default().push(id.clone());
        for g in genres {
            said.push(Said { albums: vec![id.clone()], source: "files".into(), word: g.clone(), raw_kind: "genre".into(), weight: 1.0 });
        }
    }
    for (subject, source, word, raw_kind, weight) in serve.userdata.all_words()? {
        let (held, share) = match subject.split_once(':') {
            Some(("album", base)) => (by_base.get(base), 1.0),
            Some(("artist", artist)) => (by_artist.get(artist), ARTIST_SHARE),
            _ => continue,
        };
        let Some(held) = held else { continue };
        // Named as the rules name genres (their aliases and merges, 0.9.7).
        let word = crate::catalogue::normalise::aliased(&word, &rules.genre_aliases);
        said.push(Said { albums: held.clone(), source, word, raw_kind, weight: weight as f32 * share });
    }
    for (base, tag) in serve.userdata.all_album_tags()? {
        if let Some(held) = by_base.get(base.as_str()) {
            said.push(Said { albums: held.clone(), source: "household".into(), word: tag, raw_kind: "tag".into(), weight: 1.0 });
        }
    }
    let m = Arc::new(merge(&said));
    *MERGED.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, m.clone()));
    Ok(m)
}

/// A file's facts with its album's words (`album`): genres, styles, moods and tags, each once (in any case).
pub fn add_to(f: &mut crate::crates::Facts, words: &[Word]) {
    for w in words {
        let into = match w.kind {
            "genre" => &mut f.genres,
            "style" => &mut f.styles,
            "mood" => &mut f.moods,
            _ => &mut f.tags,
        };
        if !into.iter().any(|x| x.eq_ignore_ascii_case(&w.word) || key(x) == key(&w.word)) {
            into.push(w.word.clone());
        }
    }
}

/// Every word of the library by kind, with the albums that carry it (by the files' crate facts: the files' own genres,
/// AudioMuse's styles and moods, and the merged words): what search finds a word by. A word shown as a genre is not
/// also a style. The labels too (0.11, quick crates: search's label), a kind apart: a label named as a genre is both.
/// Blocking.
pub fn index(facts: &[crate::crates::Facts], album_of: &HashMap<String, String>) -> Vec<(&'static str, String, usize)> {
    let mut by: HashMap<(&'static str, String), (HashMap<String, usize>, HashSet<&str>)> = HashMap::new();
    for f in facts {
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
static INDEX: Mutex<Option<(usize, usize, Arc<Vec<(&'static str, String, usize)>>)>> = Mutex::new(None);

/// Every word of the library by kind with how many albums carry it, for search. Blocking.
pub fn search_index(serve: &crate::serve::Serve, cat: &crate::catalogue::Catalogue) -> Result<Arc<Vec<(&'static str, String, usize)>>> {
    let facts = cat.crate_facts()?;
    let of = (Arc::as_ptr(&facts) as usize, facts.len());
    if let Some((p, n, i)) = INDEX.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && (*p, *n) == of
    {
        return Ok(i.clone());
    }
    let made = Arc::new(index(&facts, &serve.library.song_album_map()?));
    *INDEX.lock().unwrap_or_else(|e| e.into_inner()) = Some((of.0, of.1, made.clone()));
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(album: &str, source: &str, word: &str, raw_kind: &str, weight: f32) -> Said {
        Said { albums: vec![album.into()], source: source.into(), word: word.into(), raw_kind: raw_kind.into(), weight }
    }

    fn words_of<'a>(m: &'a Merged, album: &str) -> Vec<(&'a str, &'static str)> {
        m.albums.get(album).map(|w| w.iter().map(|w| (w.word.as_str(), w.kind)).collect()).unwrap_or_default()
    }

    #[test]
    fn a_word_has_one_kind_by_what_it_is_not_by_who_said_it() {
        let m = merge(&[
            said("a", "files", "Trip Hop", "genre", 1.0),
            said("a", "discogs", "Electronic", "genre", 1.0),
            said("a", "discogs", "Downtempo", "style", 1.0),
            said("a", "lastfm", "downtempo", "tag", 1.0),
            said("a", "lastfm", "melancholic", "tag", 1.0),
            said("a", "lastfm", "seen at the park", "tag", 1.0),
            // Another album's file genre: a genre everywhere.
            said("b", "files", "Shoegaze", "genre", 1.0),
            said("a", "musicbrainz", "shoegaze", "style", 1.0),
        ]);
        let a = words_of(&m, "a");
        assert!(a.contains(&("Trip Hop", "genre")) && a.contains(&("Electronic", "genre")), "{a:?}");
        assert!(a.contains(&("Downtempo", "style")), "curated, so a style however Last.fm calls it: {a:?}");
        assert!(a.contains(&("melancholic", "mood")) && a.contains(&("seen at the park", "tag")), "{a:?}");
        assert!(a.iter().any(|(w, k)| w.eq_ignore_ascii_case("shoegaze") && *k == "genre"), "a file's genre stays a genre: {a:?}");
    }

    #[test]
    fn an_album_keeps_its_strongest_words_of_each_kind() {
        let tags: Vec<Said> = (0..9).map(|i| said("a", "lastfm", &format!("tag {i}"), "tag", 0.9 - i as f32 * 0.05)).collect();
        let m = merge(&tags);
        let a = m.albums.get("a").unwrap();
        assert_eq!(a.len(), 5, "five tags at most");
        assert_eq!(a[0].word, "tag 0", "the strongest first");
    }

    #[test]
    fn agreement_raises_a_word_and_a_passing_remark_does_not_make_one() {
        let m = merge(&[said("a", "lastfm", "jazzy", "tag", 0.2), said("a", "lastfm", "smooth", "tag", 0.6), said("a", "musicbrainz", "smooth", "tag", 0.6)]);
        let a = m.albums.get("a").unwrap();
        assert!(a.iter().all(|w| w.word != "jazzy"), "0.2 × 0.5 is below the floor");
        assert!((a[0].weight - 0.6).abs() < 1e-6 && a[0].word == "smooth", "two sources summed: {a:?}");
    }
}
