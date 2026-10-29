//! A listener's taste (the owner, 2026-10-23: Insights › Listening › Profile): what they
//! like, from their favourites, the playlists they made and what they played, and what they do not, from the tracks
//! they excluded and the ones they skipped. Two profiles of the one similarity (`similarity::Profile::of_weighted`),
//! "taste" and "avoid", over the library as `crates::Alike` sees it. The taste's words are net of the avoided ones';
//! its numbers and moods are its own (a middle cannot be taken from another).

use std::collections::HashMap;

use serde::Serialize;

use crate::crates::Alike;
use crate::similarity::{MOODS, Profile, Signal};

/// How much a favourite weighs; a playlist holding the track (each, up to `PLAYLISTS_MOST`); and an excluded track.
const FAVOURITE: f64 = 3.0;
const PLAYLIST: f64 = 1.0;
const PLAYLISTS_MOST: usize = 3;
const EXCLUDED: f64 = 3.0;
/// A skip weighs one, up to this many of a track.
const SKIPS_MOST: u32 = 5;
/// The most words shown per kind.
const WORDS_MOST: usize = 10;

/// What the taste is made from, each by its place in the library (`Alike`): the favourites, the playlists holding each
/// track (how many), each track's plays, the excluded tracks, each track's skips.
#[derive(Debug, Default)]
pub struct Sources {
    pub favourites: Vec<usize>,
    pub playlists: Vec<(usize, usize)>,
    pub plays: Vec<(usize, u32)>,
    pub excluded: Vec<usize>,
    pub skipped: Vec<(usize, u32)>,
}

/// The taste and what is avoided, as the page reads them.
#[derive(Debug, Serialize)]
pub struct TasteView {
    pub taste: Side,
    pub avoid: Side,
}

#[derive(Debug, Serialize)]
pub struct Side {
    /// How many tracks each source added: favourites, playlists and plays for the taste; excluded and skipped for what
    /// is avoided.
    pub sources: HashMap<&'static str, usize>,
    pub tracks: usize,
    /// Of those, how many AudioMuse has heard (styles, moods, energy and danceability are theirs only).
    pub heard: usize,
    pub words: Vec<WordKind>,
    pub numbers: Vec<Number>,
    pub moods: Vec<Mood>,
}

#[derive(Debug, Serialize)]
pub struct WordKind {
    pub kind: &'static str,
    pub title: &'static str,
    pub top: Vec<Word>,
}

#[derive(Debug, Serialize)]
pub struct Word {
    pub name: String,
    pub w: f64,
}

#[derive(Debug, Serialize)]
pub struct Number {
    pub name: &'static str,
    /// `value` (a year, BPM) or `share` (0 to 1: energy, danceability).
    pub unit: &'static str,
    pub mid: f64,
    pub spread: f64,
    /// The library's own range for it (its 2nd and 98th percentiles), the page's axis.
    pub lo: f64,
    pub hi: f64,
}

#[derive(Debug, Serialize)]
pub struct Mood {
    pub name: &'static str,
    /// CLAP's score's middle (about 0.5 to 0.65 for every mood), and how far above the library's middle it is in the
    /// library's spreads (what makes a mood stand out; the order).
    pub mid: f64,
    pub lift: f64,
}

const KINDS: [(Signal, &str, &str); 4] =
    [(Signal::Genres, "genre", "Genres"), (Signal::Artists, "artist", "Artists"), (Signal::Styles, "style", "Styles"), (Signal::Playlists, "playlist", "Playlists")];
const NUMBERS: [(Signal, &str, &str); 4] =
    [(Signal::Era, "Era", "value"), (Signal::Tempo, "Tempo", "value"), (Signal::Energy, "Energy", "share"), (Signal::Danceability, "Danceability", "share")];

/// The taste and what is avoided as two profiles of the one similarity (`taste` shows them; Home's Grave digging
/// compares albums with the taste).
pub fn profiles(alike: &Alike, sources: &Sources, off: &[&str]) -> (Profile, Profile) {
    let on = |name: &str| !off.contains(&name);
    let mut liked: Vec<(usize, f64)> = Vec::new();
    if on("favourites") {
        liked.extend(sources.favourites.iter().map(|&i| (i, FAVOURITE)));
    }
    if on("playlists") {
        liked.extend(sources.playlists.iter().map(|&(i, n)| (i, PLAYLIST * n.min(PLAYLISTS_MOST) as f64)));
    }
    if on("plays") {
        // Plays count less and less: a track played a hundred times is no hundred favourites.
        liked.extend(sources.plays.iter().map(|&(i, n)| (i, (1.0 + n as f64).ln())));
    }
    let mut avoided: Vec<(usize, f64)> = Vec::new();
    if on("excluded") {
        avoided.extend(sources.excluded.iter().map(|&i| (i, EXCLUDED)));
    }
    if on("skipped") {
        avoided.extend(sources.skipped.iter().map(|&(i, n)| (i, n.min(SKIPS_MOST) as f64)));
    }
    (alike.profile(&liked), alike.profile(&avoided))
}

/// The sources one can switch off (the owner, 2026-10-23: a switch for each on the page).
pub const SOURCES: [&str; 5] = ["favourites", "playlists", "plays", "skipped", "excluded"];

/// The taste and what is avoided, from `sources`, over `alike`; a source in `off` adds nothing (its count stays, what
/// it would add).
pub fn taste(alike: &Alike, sources: &Sources, off: &[&str]) -> TasteView {
    let (taste, avoid) = profiles(alike, sources, off);
    let library = Library::of(alike);
    let count = |set: &[usize]| set.iter().collect::<std::collections::HashSet<_>>().len();
    let taste_sources = HashMap::from([
        ("favourites", count(&sources.favourites)),
        ("playlists", count(&sources.playlists.iter().map(|p| p.0).collect::<Vec<_>>())),
        ("plays", count(&sources.plays.iter().map(|p| p.0).collect::<Vec<_>>())),
    ]);
    let avoid_sources = HashMap::from([("excluded", count(&sources.excluded)), ("skipped", count(&sources.skipped.iter().map(|p| p.0).collect::<Vec<_>>()))]);
    let taste_moods = moods(&taste, &library);
    let shown: Vec<&str> = taste_moods.iter().map(|m| m.name).collect();
    let mut avoid_moods = moods(&avoid, &library);
    avoid_moods.retain(|m| shown.contains(&m.name));
    TasteView {
        taste: side(alike, &taste, Some(&avoid), taste_sources, &library, taste_moods),
        avoid: side(alike, &avoid, None, avoid_sources, &library, avoid_moods),
    }
}

fn side(alike: &Alike, p: &Profile, less: Option<&Profile>, sources: HashMap<&'static str, usize>, library: &Library, moods: Vec<Mood>) -> Side {
    let words = KINDS
        .iter()
        .map(|&(kind, name, title)| {
            let minus: HashMap<u32, f64> = less.map(|l| l.words(kind).into_iter().collect()).unwrap_or_default();
            let mut top: Vec<(u32, f64)> = p.words(kind).into_iter().map(|(w, x)| (w, x - minus.get(&w).copied().unwrap_or(0.0))).filter(|(_, x)| *x > 0.0).collect();
            top.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let top = top
                .into_iter()
                .filter_map(|(w, x)| Some(Word { name: alike.name(kind, w)?.to_string(), w: (x * 1000.0).round() / 1000.0 }))
                .take(WORDS_MOST)
                .collect();
            WordKind { kind: name, title, top }
        })
        .filter(|k| !k.top.is_empty())
        .collect();
    let numbers = NUMBERS
        .iter()
        .zip(library.ranges)
        .filter_map(|(&(s, name, unit), (lo, hi))| {
            let (mid, spread) = p.number(s)?;
            Some(Number { name, unit, mid: round(mid), spread: round(spread), lo: round(lo), hi: round(hi) })
        })
        .collect();
    Side { sources, tracks: p.tracks(), heard: p.members().iter().filter(|&&i| alike.heard(i)).count(), words, numbers, moods }
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A side's moods, the most lifted first.
fn moods(p: &Profile, library: &Library) -> Vec<Mood> {
    let mut out: Vec<Mood> = MOODS
        .iter()
        .zip(p.moods())
        .zip(library.moods)
        .filter_map(|((&name, mine), (middle, spread))| {
            let (mid, _) = mine?;
            Some(Mood { name, mid: round(mid), lift: round((mid - middle) / spread.max(f64::EPSILON)) })
        })
        .collect();
    out.sort_by(|a, b| b.lift.total_cmp(&a.lift));
    out
}

/// The library's own ranges (its 2nd and 98th percentiles) of the numbers, and each mood's middle and spread.
struct Library {
    ranges: [(f64, f64); 4],
    moods: [(f64, f64); MOODS.len()],
}

impl Library {
    fn of(alike: &Alike) -> Self {
        let sorted = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v
        };
        let tracks = alike.tracks();
        let columns: [Vec<f64>; 4] = [
            sorted(tracks.iter().filter_map(|t| t.year.map(f64::from)).collect()),
            sorted(tracks.iter().filter_map(|t| t.bpm.map(f64::from)).collect()),
            sorted(tracks.iter().filter_map(|t| t.energy.map(f64::from)).collect()),
            sorted(tracks.iter().filter_map(|t| t.danceability.map(f64::from)).collect()),
        ];
        let q = crate::similarity::quantile;
        let ranges = columns.map(|v| (q(&v, 0.02), q(&v, 0.98)));
        let moods = std::array::from_fn(|m| {
            let v = sorted(tracks.iter().filter_map(|t| t.moods.map(|x| f64::from(x[m]))).collect());
            (q(&v, 0.5), (q(&v, 0.75) - q(&v, 0.25)) / 2.0)
        });
        Library { ranges, moods }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::Facts;

    fn file(rel: &str, genre: &str, year: f64) -> Facts {
        Facts { rel: rel.into(), genres: vec![genre.into()], year: Some(year), ..Facts::default() }
    }

    #[test]
    fn the_taste_leans_to_the_liked_and_its_words_are_net_of_the_avoided() {
        let files = vec![file("a.flac", "Jazz", 1960.0), file("b.flac", "Jazz", 1965.0), file("c.flac", "Techno", 1995.0), file("d.flac", "Techno", 1998.0), file("e.flac", "Folk", 1970.0)];
        let alike = Alike::of(&files, &HashMap::new());
        let at = |rel: &str| alike.place_of_line(rel).unwrap();
        let sources = Sources {
            favourites: vec![at("a.flac")],
            plays: vec![(at("b.flac"), 20), (at("c.flac"), 1)],
            excluded: vec![at("d.flac")],
            skipped: vec![(at("c.flac"), 2)],
            ..Sources::default()
        };
        let t = taste(&alike, &sources, &[]);
        assert_eq!((t.taste.tracks, t.avoid.tracks), (3, 2));
        assert_eq!((t.taste.sources["favourites"], t.taste.sources["plays"], t.avoid.sources["excluded"], t.avoid.sources["skipped"]), (1, 2, 1, 1));
        let genres = &t.taste.words.iter().find(|k| k.kind == "genre").unwrap().top;
        assert_eq!(genres.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["Jazz"], "Techno is liked a little and avoided more: net, gone");
        let avoided = &t.avoid.words.iter().find(|k| k.kind == "genre").unwrap().top;
        assert_eq!(avoided[0].name, "Techno");
        let era = t.taste.numbers.iter().find(|n| n.name == "Era").unwrap();
        assert!(era.mid < 1970.0 && era.lo < 1961.0 && era.hi > 1990.0, "the taste's era among the library's range: {era:?}");
        assert_eq!(t.taste.heard, 0, "AudioMuse heard none");
        // Sources switched off add nothing; their counts stay.
        let t = taste(&alike, &sources, &["plays", "skipped", "excluded"]);
        assert_eq!((t.taste.tracks, t.avoid.tracks, t.taste.sources["plays"], t.avoid.sources["excluded"]), (1, 0, 2, 1));
        assert!(t.avoid.words.is_empty() && t.avoid.numbers.is_empty());
    }
}
