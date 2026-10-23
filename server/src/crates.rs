//! Crates (decision 0024): dynamic cross-sections of the library. A crate is a list of conditions on the library's axes
//! (all must hold, each negatable; OR within an axis through several values or a range), worked out when it is looked
//! at. The permanent crates are definitions in the same model; one's own are kept in `crates.json`. The axes are a
//! registry: each declares its kind and domain, and how a file is tested, so the builder draws its controls from it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The stored format's version (decision 0024: groups, OR across axes, may come later).
pub const VERSION: u32 = 1;

/// One condition: an axis, the value(s) it is tested against, and whether it is negated. A filter's (decision 0061)
/// can be muted (`enabled` false: ignored, kept to switch back on) and, on items that hold tracks, says of how many of
/// them it must hold (`quantifier`). Saved crates without either are on, of any track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub axis: String,
    #[serde(default)]
    pub not: bool,
    pub value: Value,
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Quantifier::is_any")]
    pub quantifier: Quantifier,
}

impl Condition {
    /// A condition that is on, of any track.
    pub fn new(axis: impl Into<String>, not: bool, value: Value) -> Self {
        Self { axis: axis.into(), not, value, enabled: true, quantifier: Quantifier::Any }
    }
}

fn on() -> bool {
    true
}

fn is_on(b: &bool) -> bool {
    *b
}

/// Of how many of an item's tracks a condition must hold (decision 0061: an album, an artist, a playlist).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantifier {
    /// At least one of them.
    #[default]
    Any,
    /// Every one of them that has a value on the axis (see `item_holds`).
    All,
}

impl Quantifier {
    fn is_any(&self) -> bool {
        *self == Quantifier::Any
    }
}

/// What an axis is tested against: holds (a yes/no axis), any of these values (values, membership: none = any), or a
/// range (either end open).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    Yes,
    AnyOf {
        #[serde(default)]
        values: Vec<String>,
    },
    Range {
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Sounds like its seeds (song ids; an album's are its songs'): the `count` tracks that sound most like them
    /// (the owner, 2026-10-23: how many, not how alike; one setting means one size for every seed).
    Like { of: Vec<String>, count: usize },
    /// A `Like` worked out (`resolved`): the lines of those tracks. Never kept.
    #[serde(skip)]
    Within { lines: Arc<HashSet<String>> },
}

/// A crate: its id never changes; its name is unique among crates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateDef {
    pub id: String,
    pub name: String,
    pub conditions: Vec<Condition>,
}

/// A genre, viewed in the panel: a crate of one condition, made when asked for (`genre:<name>`), never saved. Its tracks
/// are those whose genre tags hold the name (as the crate builder's genre axis matches).
pub const GENRE: &str = "genre:";

pub fn genre_crate(id: &str) -> Option<CrateDef> {
    let name = id.strip_prefix(GENRE)?.trim();
    if name.is_empty() {
        return None;
    }
    let genre = Condition::new("genre", false, Value::AnyOf { values: vec![name.to_string()] });
    Some(CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![genre] })
}

/// A track or an album, viewed in the panel as a crate of what sounds like it (the owner, 2026-10-23: "just like genre,
/// albums and tracks can now become seeds for crates"): made when asked for (`like:<id>`), never saved. The caller
/// names it and turns an album into its songs (`like_crate`).
pub const LIKE: &str = "like:";
/// How many tracks it holds until its curve says otherwise, and the fewest and most it can.
pub const LIKE_COUNT: usize = 200;
pub const LIKE_FEWEST: usize = 10;
pub const LIKE_MOST: usize = 1000;

/// A seed crate's id: the item (a song's or an album's id), and how many it holds when not the default
/// (`like:<item>@<count>`).
pub fn like_of(id: &str) -> Option<(&str, usize)> {
    let rest = id.strip_prefix(LIKE)?;
    Some(match rest.rsplit_once('@') {
        Some((item, n)) => (item, n.parse().map_or(LIKE_COUNT, |n: usize| n.clamp(LIKE_FEWEST, LIKE_MOST))),
        None => (rest, LIKE_COUNT),
    })
}

pub fn like_crate(id: &str, name: &str, songs: Vec<String>, count: usize) -> CrateDef {
    let like = Condition::new("like", false, Value::Like { of: songs, count });
    CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![like] }
}

/// The permanent crates, in the same model (decision 0024). Favourites and Unlisted favourites went (amended
/// 2026-10-14): any crate shows only its favourites, or none of them, with the panel's favourites toggle.
pub fn permanent() -> Vec<CrateDef> {
    let unlisted = Condition::new("playlist", true, Value::AnyOf { values: Vec::new() });
    vec![CrateDef { id: "unlisted".into(), name: "Unlisted".into(), conditions: vec![unlisted] }]
}

/// The crate panel's favourites toggle (decision 0024, amended): all a crate's tracks, only its favourites, or none of
/// them. Not part of the crate's definition (its undo, its conditions): one setting per crate, kept beside the crates
/// in `crates.json` (amended 2026-10-14; it was the browser's), and it adds one condition when the crate is looked at.
/// A crate not set yet shows no favourites.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Favourites {
    #[default]
    All,
    Only,
    None,
}

impl Favourites {
    /// The crate's conditions with the toggle's.
    pub fn with(self, conditions: &[Condition]) -> Vec<Condition> {
        self.on("favourite", conditions)
    }

    /// The crate's conditions with the favourite albums toggle's, set as this (decision 0024, amended 2026-10-19: a
    /// second toggle, the same three settings, all a crate's tracks until set).
    pub fn with_albums(self, conditions: &[Condition]) -> Vec<Condition> {
        self.on("favourite_album", conditions)
    }

    fn on(self, axis: &str, conditions: &[Condition]) -> Vec<Condition> {
        let mut all = conditions.to_vec();
        if self != Favourites::All {
            all.push(Condition::new(axis, self == Favourites::None, Value::Yes));
        }
        all
    }
}

/// A crate's conditions as its two toggles show it: favourite tracks (`tracks`), and favourite albums (`albums`).
pub fn shown(conditions: &[Condition], tracks: Favourites, albums: Favourites) -> Vec<Condition> {
    albums.with_albums(&tracks.with(conditions))
}

pub fn is_permanent(id: &str) -> bool {
    permanent().iter().any(|c| c.id == id)
}

// --- The axes ---

/// How an axis is chosen from in the builder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Holds or not (favourite).
    YesNo,
    /// One or more of its values (genre).
    Values,
    /// In one or more of the playlists, or in any.
    Membership,
    /// Between two numbers (year, BPM).
    Range,
    /// Like its seeds, a track's or an album's, by how much (sounds like).
    Like,
}

/// An axis as the builder sees it: its kind and its domain (the values there are, or the numbers' span).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AxisView {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub values: Vec<String>,
    /// How many items have each of `values` (a list in view's, decision 0061; a track list's items are its tracks);
    /// empty where not counted (the builder's playlists).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<usize>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// How many of the files (a list's items) have a value on this axis (BPM is only where the files say it).
    pub known: usize,
    /// A range's density (the owner, 2026-10-23: the profile's bars as the sliders): how many tracks fall in each of
    /// `DENSITY_BINS` equal steps from `min` to `max`; empty for other kinds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<u32>,
    /// Whether the tracks within one item differ on it, so "any" or "all of its tracks" can mean different things (an
    /// album's tracks share its year and its heart: they cannot; a playlist's can). False where items are tracks.
    pub varies: bool,
}

/// How many steps a range's density has.
pub const DENSITY_BINS: usize = 24;

/// How many of `vals` fall in each of `DENSITY_BINS` equal steps from `lo` to `hi` (those outside in the end ones).
pub fn density(vals: impl IntoIterator<Item = f64>, lo: f64, hi: f64) -> Vec<u32> {
    let mut bins = vec![0; DENSITY_BINS];
    let width = (hi - lo).max(f64::EPSILON);
    for v in vals.into_iter().filter(|v| v.is_finite()) {
        bins[(((v - lo) / width) * DENSITY_BINS as f64).floor().clamp(0.0, (DENSITY_BINS - 1) as f64) as usize] += 1;
    }
    bins
}

/// The tempo spread (decision 0041): 5-BPM bands from 60 to 180 BPM, always the same scale; a tempo under 60 counts in
/// the first band, one from 180 up in the last. The page's lib/Spread keeps the same scale.
pub const TEMPO_LO: f64 = 60.0;
pub const TEMPO_BAND: f64 = 5.0;
pub const TEMPO_BANDS: usize = 24;

/// How many tracks fall in each tempo band, how many have a tempo, and how many there are.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TempoBands {
    pub bands: Vec<u32>,
    pub with_tempo: usize,
    pub total: usize,
}

/// The spread of some tracks' tempos (`None`: no tempo, counted in `total` only). Pure, for a crate's tracks and, later,
/// the crate editor's BPM axis over the whole library.
pub fn tempo_bands(bpms: impl IntoIterator<Item = Option<f64>>) -> TempoBands {
    let mut out = TempoBands { bands: vec![0; TEMPO_BANDS], ..TempoBands::default() };
    for bpm in bpms {
        out.total += 1;
        let Some(b) = bpm.filter(|b| b.is_finite() && *b > 0.0) else { continue };
        let band = ((b - TEMPO_LO) / TEMPO_BAND).floor().clamp(0.0, (TEMPO_BANDS - 1) as f64) as usize;
        out.bands[band] += 1;
        out.with_tempo += 1;
    }
    out
}

/// A list's profile (the owner, 2026-10-23): what its tracks are like, shown on top of it. Its most telling style tags
/// and artists (common in the list, rare in the library, on at least a few of its tracks), each number's density with
/// its middle half and middle on one scale for every list (the library's own, tempo today's 60 to 180 BPM), and how
/// far each of CLAP's moods sits above or below the library's. A number all its tracks share (an album's year) is
/// that one value, no spread (`one`; the owner, 2026-10-23: shown as the value, not as a graph).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Profile {
    pub tracks: usize,
    /// How many CLAP heard (the moods, danceability).
    pub heard: usize,
    pub styles: Vec<String>,
    pub artists: Vec<String>,
    pub numbers: Vec<NumberRow>,
    pub moods: Vec<MoodLift>,
}

/// One number of a profile: its scale (`lo` to `hi`), the density over it (`DENSITY_BINS`), its middle half and middle.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NumberRow {
    /// "tempo", "energy", "danceability", "year".
    pub key: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub bins: Vec<u32>,
    pub q1: f64,
    pub mid: f64,
    pub q3: f64,
    /// Every track has this one value: the row is the value, no density.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one: Option<f64>,
}

/// One of CLAP's moods: the list's middle against the library's, -1 (far below) to 1 (far above).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MoodLift {
    pub name: &'static str,
    pub lift: f64,
}

/// How many telling style tags and artists a profile names.
const TELLING: usize = 4;

/// The profile of `list` (its tracks' facts) against `library` (every file's).
pub fn profile(list: &[&Facts], library: &[&Facts]) -> Profile {
    let n = list.len();
    let mut out = Profile { tracks: n, heard: list.iter().filter(|f| f.clap_moods.is_some()).count(), ..Profile::default() };
    out.styles = telling(list, library, |f| &f.styles);
    out.artists = telling(list, library, |f| &f.artists);
    let sorted = |vals: Vec<f64>| {
        let mut v: Vec<f64> = vals.into_iter().filter(|x| x.is_finite()).collect();
        v.sort_by(f64::total_cmp);
        v
    };
    let numbers: [(&str, fn(&Facts) -> Option<f64>); 4] =
        [("tempo", |f| f.bpm), ("energy", |f| f.energy), ("danceability", |f| f.danceability), ("year", |f| f.year)];
    for (key, get) in numbers {
        let mine = sorted(list.iter().filter_map(|f| get(f)).collect());
        let Some(&first) = mine.first() else { continue };
        if mine.last() == Some(&first) {
            out.numbers.push(NumberRow { key, lo: first, hi: first, bins: Vec::new(), q1: first, mid: first, q3: first, one: Some(first) });
            continue;
        }
        let (lo, hi) = if key == "tempo" {
            (TEMPO_LO, TEMPO_LO + TEMPO_BAND * TEMPO_BANDS as f64)
        } else if key == "year" {
            // The library's oldest to newest (the owner: its 1950s albums were lost at the 5th percentile, 1993), a
            // year before 1900 being a broken tag ("1").
            let all = sorted(library.iter().filter_map(|f| f.year).filter(|y| *y >= 1900.0).collect());
            (all.first().copied().unwrap_or(1900.0), all.last().copied().unwrap_or(2100.0))
        } else {
            let all = sorted(library.iter().filter_map(|f| get(f)).collect());
            (crate::similarity::quantile(&all, 0.05), crate::similarity::quantile(&all, 0.95))
        };
        let q = |p| crate::similarity::quantile(&mine, p);
        out.numbers.push(NumberRow { key, lo, hi, bins: density(mine.iter().copied(), lo, hi), q1: q(0.25), mid: q(0.5), q3: q(0.75), one: None });
    }
    if out.heard > 0 {
        for (k, name) in crate::similarity::MOODS.into_iter().enumerate() {
            let of = |fs: &[&Facts]| sorted(fs.iter().filter_map(|f| f.clap_moods.map(|m| m[k] as f64)).collect());
            let (mine, all) = (of(list), of(library));
            let half = (crate::similarity::quantile(&all, 0.95) - crate::similarity::quantile(&all, 0.05)) / 2.0;
            let lift = (crate::similarity::quantile(&mine, 0.5) - crate::similarity::quantile(&all, 0.5)) / half.max(f64::EPSILON);
            out.moods.push(MoodLift { name, lift: lift.clamp(-1.0, 1.0) });
        }
    }
    out
}

/// The names most telling of a list: on many of its tracks and few of the library's (tf-idf, as one similarity
/// weighs words), on at least two of its tracks or a twentieth of them (one odd tag does not lead an album).
fn telling(list: &[&Facts], library: &[&Facts], names: fn(&Facts) -> &[String]) -> Vec<String> {
    let count = |fs: &[&Facts]| {
        let mut c: HashMap<String, (usize, String)> = HashMap::new();
        for f in fs {
            for name in names(f).iter().collect::<std::collections::BTreeSet<_>>() {
                c.entry(name.to_lowercase()).or_insert((0, name.clone())).0 += 1;
            }
        }
        c
    };
    let (mine, all) = (count(list), count(library));
    let least = if list.len() < 2 { 1 } else { 2.max(list.len().div_ceil(20)) };
    let total = library.len().max(1) as f64;
    let mut weighed: Vec<(f64, String)> = mine
        .into_iter()
        .filter(|(_, (c, _))| *c >= least)
        .map(|(k, (c, name))| (c as f64 / list.len() as f64 * (total / all.get(&k).map_or(1, |x| x.0).max(1) as f64).ln(), name))
        .filter(|(w, _)| *w > 0.0)
        .collect();
    weighed.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    weighed.into_iter().take(TELLING).map(|(_, n)| n).collect()
}

/// What the axes test about one file of the libraries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// The playlist line: relative to Music, climbing out of it for a file in an Incoming folder
    /// ("../lidarr-music/X/01.flac"), so a Music file's line never is an Incoming one's.
    pub rel: String,
    /// The catalogue folder it is in: 0 is Music, 1 on the Incoming ones (what a library switched off leaves out).
    pub folder: i64,
    /// When the catalogue first saw its track (epoch seconds; a moved file keeps it): when it came into the library.
    pub born: Option<i64>,
    pub genres: Vec<String>,
    pub year: Option<f64>,
    pub bpm: Option<f64>,
    /// AudioMuse's (decision 0054): its energy and CLAP's danceability as 0 to 100, CLAP's moods that stand out, its
    /// style tags; none where AudioMuse has not heard the file.
    pub energy: Option<f64>,
    pub danceability: Option<f64>,
    pub moods: Vec<String>,
    pub styles: Vec<String>,
    /// Its track in the catalogue, and AudioMuse's "sounds like" vector of it (unit length; none where not heard).
    pub track: Option<i64>,
    pub sound: Option<Arc<[f32]>>,
    /// Its artists and album artist (a compilation's left out), for a list's profile.
    pub artists: Vec<String>,
    /// CLAP's score of each mood, in `similarity::MOODS`' order; none until CLAP heard it.
    pub clap_moods: Option<[f32; 5]>,
}

/// A compilation's album artist ("Various Artists"): no artist.
pub fn various(name: &str) -> bool {
    matches!(name.trim().to_lowercase().as_str(), "various artists" | "various" | "va" | "verschiedene interpreten")
}

/// What the axes test about the library as a whole: which playlists hold a line, which lines are favourites.
pub struct World<'a> {
    pub membership: &'a HashMap<String, Vec<String>>,
    /// `None`: the favourites are not known (the media server could not be asked).
    pub favourites: Option<&'a HashSet<String>>,
    /// The lines whose album has the viewer's heart on (the Library's album favourites); `None`: not known.
    pub favourite_albums: Option<&'a HashSet<String>>,
}

const AXES: [(&str, &str, Kind); 11] = [
    ("favourite", "Favourite", Kind::YesNo),
    ("favourite_album", "Favourite album", Kind::YesNo),
    ("playlist", "In playlist", Kind::Membership),
    ("genre", "Genre", Kind::Values),
    ("year", "Year", Kind::Range),
    ("bpm", "BPM", Kind::Range),
    // AudioMuse's (decision 0054; the owner: crates select on all four).
    ("mood", "Mood", Kind::Values),
    ("energy", "Energy", Kind::Range),
    ("danceability", "Danceability", Kind::Range),
    ("style", "Style", Kind::Values),
    // One similarity's sound (the owner, 2026-10-23); seeded from a track or an album, not added by hand.
    ("like", "Sounds like", Kind::Like),
];

/// Names as the items have them, the most common first (a genre's or a style's, in any case), each with how many items
/// have it among their tracks.
fn by_count<'a>(items: &[&[&'a Facts]], names: fn(&'a Facts) -> &'a [String]) -> Vec<(String, usize)> {
    let mut seen: HashMap<String, (String, usize)> = HashMap::new();
    for tracks in items {
        let mut here = HashSet::new();
        for n in tracks.iter().flat_map(|f| names(f)) {
            let key = n.to_lowercase();
            if here.insert(key.clone()) {
                seen.entry(key).or_insert_with(|| (n.clone(), 0)).1 += 1;
            }
        }
    }
    let mut all: Vec<(String, usize)> = seen.into_values().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    all
}

pub fn is_axis(key: &str) -> bool {
    AXES.iter().any(|(k, _, _)| *k == key)
}

/// The axes with their domains in this library: genres, moods and styles most common first, the years', BPMs',
/// energies' and danceabilities' span.
pub fn axes<'a>(files: impl IntoIterator<Item = &'a Facts>, playlists: &[String]) -> Vec<AxisView> {
    let one: Vec<[&Facts; 1]> = files.into_iter().map(|f| [f]).collect();
    let items: Vec<&[&Facts]> = one.iter().map(|f| f.as_slice()).collect();
    list_axes(&items, playlists, None)
}

/// The axes over a list in view (decision 0061, the Filter sheet), its items each a group of tracks (a list of tracks:
/// one each): genres, moods and styles most common first with how many items have each among their tracks, the
/// ranges' span over the tracks, and how many items have a value on each axis. `membership` counts the playlists'
/// items too.
pub fn list_axes(items: &[&[&Facts]], playlists: &[String], membership: Option<&HashMap<String, Vec<String>>>) -> Vec<AxisView> {
    let counted = |names: fn(&Facts) -> &[String]| -> (Vec<String>, Vec<usize>) { by_count(items, names).into_iter().unzip() };
    let tracks = || items.iter().flat_map(|t| t.iter().copied());
    let span = |get: fn(&Facts) -> Option<f64>| {
        let vals: Vec<f64> = tracks().filter_map(get).collect();
        let min = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
        let max = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        (min, max, items.iter().filter(|t| t.iter().any(|f| get(f).is_some())).count())
    };
    let known = |has: fn(&Facts) -> bool| items.iter().filter(|t| t.iter().any(|f| has(f))).count();
    // Whether some item's tracks differ on an axis: by what each says on it.
    let varies = |key: &str| {
        let said = |f: &Facts| -> String {
            match key {
                "favourite" | "like" => f.rel.clone(),
                // One album, one heart: its tracks are in one folder.
                "favourite_album" => f.rel.rsplit_once('/').map_or("", |(d, _)| d).to_string(),
                "playlist" => membership.and_then(|m| m.get(&f.rel)).map(|l| l.join("|")).unwrap_or_default(),
                "genre" => f.genres.iter().map(|g| g.to_lowercase()).collect::<Vec<_>>().join("|"),
                "mood" => f.moods.join("|"),
                "style" => f.styles.join("|"),
                "year" => format!("{:?}", f.year),
                "bpm" => format!("{:?}", f.bpm),
                "energy" => format!("{:?}", f.energy),
                "danceability" => format!("{:?}", f.danceability),
                _ => String::new(),
            }
        };
        items.iter().any(|t| t.len() > 1 && t.iter().any(|f| said(f) != said(t[0])))
    };
    AXES.iter()
        .map(|&(key, label, kind)| {
            let mut a = AxisView { key, label, kind, values: Vec::new(), counts: Vec::new(), min: None, max: None, known: items.len(), bins: Vec::new(), varies: varies(key) };
            match key {
                "playlist" => {
                    a.values = playlists.to_vec();
                    if let Some(m) = membership {
                        let holds = |t: &[&Facts], p: &String| t.iter().any(|f| m.get(&f.rel).is_some_and(|l| l.contains(p)));
                        a.counts = playlists.iter().map(|p| items.iter().filter(|t| holds(t, p)).count()).collect();
                    }
                }
                "genre" => {
                    (a.values, a.counts) = counted(|f| &f.genres);
                    a.known = known(|f| !f.genres.is_empty());
                }
                "year" => (a.min, a.max, a.known) = span(|f| f.year),
                "bpm" => (a.min, a.max, a.known) = span(|f| f.bpm),
                "mood" => {
                    (a.values, a.counts) = counted(|f| &f.moods);
                    a.known = known(|f| f.danceability.is_some());
                }
                "energy" => (a.min, a.max, a.known) = span(|f| f.energy),
                "danceability" => (a.min, a.max, a.known) = span(|f| f.danceability),
                "style" => {
                    (a.values, a.counts) = counted(|f| &f.styles);
                    a.known = known(|f| !f.styles.is_empty());
                }
                "like" => a.known = known(|f| f.sound.is_some()),
                _ => {}
            }
            if let (Some(lo), Some(hi)) = (a.min, a.max) {
                let get: fn(&Facts) -> Option<f64> = match key {
                    "year" => |f| f.year,
                    "bpm" => |f| f.bpm,
                    "energy" => |f| f.energy,
                    _ => |f| f.danceability,
                };
                a.bins = density(tracks().filter_map(get), lo, hi);
            }
            a
        })
        .collect()
}

/// Whether every condition that is on, but Sounds like, holds for a file: the files a Sounds like picks its closest
/// among (`resolved`).
pub fn holds_besides_like(conditions: &[Condition], f: &Facts, w: &World) -> bool {
    conditions.iter().filter(|c| c.enabled && !matches!(c.value, Value::Like { .. })).all(|c| holds(c, f, w))
}

/// Whether a condition holds for a file. A condition on an unknown axis, or with a value its axis does not take, never
/// holds (negated: always), so a crate from a newer version is shown narrower rather than wider.
fn holds(c: &Condition, f: &Facts, w: &World) -> bool {
    matches(c, f, w) != c.not
}

/// Whether a file matches a condition, its negation aside.
fn matches(c: &Condition, f: &Facts, w: &World) -> bool {
    let within = |v: Option<f64>, min: &Option<f64>, max: &Option<f64>| {
        v.is_some_and(|v| min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m))
    };
    match (c.axis.as_str(), &c.value) {
        ("favourite", Value::Yes) => w.favourites.is_some_and(|s| s.contains(&f.rel)),
        ("favourite_album", Value::Yes) => w.favourite_albums.is_some_and(|s| s.contains(&f.rel)),
        (LEFT_OUT, Value::AnyOf { values }) => values.iter().any(|v| *v == f.rel),
        ("playlist", Value::AnyOf { values }) => match w.membership.get(&f.rel) {
            None => false,
            Some(lists) if values.is_empty() => !lists.is_empty(),
            Some(lists) => values.iter().any(|v| lists.contains(v)),
        },
        ("genre", Value::AnyOf { values }) => values.iter().any(|v| f.genres.iter().any(|g| g.eq_ignore_ascii_case(v))),
        ("year", Value::Range { min, max }) => within(f.year, min, max),
        ("bpm", Value::Range { min, max }) => within(f.bpm, min, max),
        ("mood", Value::AnyOf { values }) => values.iter().any(|v| f.moods.iter().any(|m| m.eq_ignore_ascii_case(v))),
        ("style", Value::AnyOf { values }) => values.iter().any(|v| f.styles.iter().any(|s| s.eq_ignore_ascii_case(v))),
        ("energy", Value::Range { min, max }) => within(f.energy, min, max),
        ("danceability", Value::Range { min, max }) => within(f.danceability, min, max),
        ("like", Value::Within { lines }) => lines.contains(&f.rel),
        _ => false,
    }
}

/// Whether a file has a value on a condition's axis: what All counts (decision 0061). A yes/no axis has one where it
/// is known (the favourites read), membership always; genres, styles where the tags or AudioMuse give some; moods
/// where CLAP heard the file; a range where there is a number.
fn has_value(c: &Condition, f: &Facts, w: &World) -> bool {
    match c.axis.as_str() {
        "favourite" => w.favourites.is_some(),
        "favourite_album" => w.favourite_albums.is_some(),
        "playlist" | LEFT_OUT => true,
        "genre" => !f.genres.is_empty(),
        "year" => f.year.is_some(),
        "bpm" => f.bpm.is_some(),
        "mood" => f.danceability.is_some(),
        "style" => !f.styles.is_empty(),
        "energy" => f.energy.is_some(),
        "danceability" => f.danceability.is_some(),
        "like" => f.sound.is_some(),
        _ => false,
    }
}

/// Whether an item that holds tracks (an album, an artist, a playlist) passes one condition (decision 0061).
/// - Any: at least one of its tracks holds it, as a crate tests a track (negation included).
/// - All: every one of its tracks that has a value on the axis matches; a track without one (not analysed yet, no
///   genre tag) does not count. Not negated, at least one track must have a value: an album none of whose tracks is
///   analysed is not "all high energy". Negated, none of the tracks with a value matches, which an item with none
///   passes, as a single track without a value passes a negated condition.
///
/// A track alone is an item of one: Any and All say the same of it.
fn item_holds(c: &Condition, tracks: &[&Facts], w: &World) -> bool {
    match c.quantifier {
        Quantifier::Any => tracks.iter().any(|f| holds(c, f, w)),
        Quantifier::All => {
            let mut valued = tracks.iter().filter(|f| has_value(c, f, w)).peekable();
            if c.not {
                valued.all(|f| !matches(c, f, w))
            } else {
                valued.peek().is_some() && valued.all(|f| matches(c, f, w))
            }
        }
    }
}

/// Whether an item passes the conditions (decision 0061): every one that is on (a muted one is ignored), each with
/// its quantifier over the item's tracks. No condition on: every item passes.
pub fn passes(tracks: &[&Facts], conditions: &[Condition], w: &World) -> bool {
    conditions.iter().filter(|c| c.enabled).all(|c| item_holds(c, tracks, w))
}

/// The conditions with each Sounds like worked out once, not for every file: its closest `count` among `among` (the
/// files the crate may show: its other conditions hold for them, so the cut and the count agree; the owner saw 3
/// tracks where the curve was cut at 20), its seeds found in `all` (they may be anywhere, a library switched off, left
/// out). Seeds none of which was heard: never holds.
pub fn resolved(conditions: &[Condition], all: &[Facts], among: &[&Facts]) -> Vec<Condition> {
    conditions
        .iter()
        .map(|c| match &c.value {
            Value::Like { of, count } => {
                let lines = closest(of, all, among).into_iter().take(*count).map(|(f, _)| f.rel.clone()).collect();
                Condition { value: Value::Within { lines: Arc::new(lines) }, ..c.clone() }
            }
            _ => c.clone(),
        })
        .collect()
}

/// The files among `among` that sound most like the seeds (song ids, their sounds found in `all`), closest first, at
/// most `LIKE_MOST`, each with how alike (0 to 100: 0 two random tracks, 100 the same sound); none when none of the
/// seeds was heard.
pub fn closest<'a>(of: &[String], all: &[Facts], among: &[&'a Facts]) -> Vec<(&'a Facts, f64)> {
    let seeds: HashSet<i64> = of.iter().filter_map(|id| crate::serve::ids::track_of(id)).collect();
    let heard: Vec<&[f32]> = all.iter().filter(|f| f.track.is_some_and(|t| seeds.contains(&t))).filter_map(|f| f.sound.as_deref()).collect();
    let Some(seed) = crate::similarity::mean_sound(&heard) else { return Vec::new() };
    let mut scored: Vec<(&Facts, f32)> = among
        .iter()
        .copied()
        .filter_map(|f| f.sound.as_deref().filter(|s| s.len() == seed.len()).map(|s| (f, s.iter().zip(&seed).map(|(a, b)| a * b).sum::<f32>())))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.rel.cmp(&b.0.rel)));
    scored.truncate(LIKE_MOST);
    scored.into_iter().map(|(f, cos)| (f, crate::similarity::sound_alike(cos) * 100.0)).collect()
}

/// A crate's files, in the order given (newest first): those all its conditions that are on hold for.
pub fn select<'a>(files: impl IntoIterator<Item = &'a Facts>, conditions: &[Condition], w: &World) -> Vec<&'a str> {
    let on: Vec<&Condition> = conditions.iter().filter(|c| c.enabled).collect();
    files.into_iter().filter(|f| on.iter().all(|c| holds(c, f, w))).map(|f| f.rel.as_str()).collect()
}

/// Whether working the conditions out needs the media server's favourites.
pub fn needs_favourites(conditions: &[Condition]) -> bool {
    conditions.iter().any(|c| c.enabled && c.axis == "favourite")
}

/// A crate's conditions with its tracks left out by hand (decision 0024, amended 2026-10-19: a crate's left-out list,
/// over its conditions and both toggles, so a favourite left out stays out). Not an axis one builds with: the crate
/// carries it unseen.
pub fn leaving_out(lines: &[String], conditions: &[Condition]) -> Vec<Condition> {
    let mut all = conditions.to_vec();
    if !lines.is_empty() {
        all.push(Condition::new(LEFT_OUT, true, Value::AnyOf { values: lines.to_vec() }));
    }
    all
}

/// The left-out list's condition: a line that is one of these.
const LEFT_OUT: &str = "line";

/// A crate's tracks left out by hand (decision 0024, amended 2026-10-19): single tracks by their lines, and whole albums
/// by their ids, each album a rule (its tracks now and later).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftOut {
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub albums: Vec<String>,
}

impl LeftOut {
    /// How many it holds: its tracks and its albums.
    pub fn len(&self) -> usize {
        self.lines.len() + self.albums.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.albums.is_empty()
    }
}

/// Whether working the conditions out needs the favourite albums (the Library's hearts).
pub fn needs_favourite_albums(conditions: &[Condition]) -> bool {
    conditions.iter().any(|c| c.enabled && c.axis == "favourite_album")
}

// --- Freshness of the media server's favourites (decision 0024, issue 5) ---

/// The favourites Cratarr itself changed, numbered: (number, server, track id, favourite now). The kept favourites
/// take these at once instead of being asked for again: asking Jellyfin for 4,786 took a minute (2026-10-14).
static FAVOURITES_CHANGED: std::sync::Mutex<(u64, Vec<(u64, String, String, bool)>)> = std::sync::Mutex::new((0, Vec::new()));

fn favourite_log() -> std::sync::MutexGuard<'static, (u64, Vec<(u64, String, String, bool)>)> {
    FAVOURITES_CHANGED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cratarr itself made `track` a favourite on `source` (`on`), or not.
pub fn favourite_changed(source: &str, track: &str, on: bool) {
    let mut log = favourite_log();
    log.0 += 1;
    let n = log.0;
    log.1.push((n, source.to_string(), track.to_string(), on));
}

/// The number of the latest change: an answer asked for now holds every change up to it.
pub fn favourite_changes_made() -> u64 {
    favourite_log().0
}

/// The changes on `source` after number `after`, oldest first: (track id, favourite now).
pub fn favourite_changes_since(source: &str, after: u64) -> Vec<(String, bool)> {
    favourite_log().1.iter().filter(|(n, s, _, _)| *n > after && s == source).map(|(_, _, t, on)| (t.clone(), *on)).collect()
}

/// An answer from `source` holds its changes up to `upto`: those are no longer needed.
pub fn favourite_changes_held(source: &str, upto: u64) {
    favourite_log().1.retain(|(n, s, _, _)| *n > upto || s != source);
}

// --- One's own crates, kept in crates.json ---

#[derive(Clone, Default, Serialize, Deserialize)]
struct Stored {
    version: u32,
    crates: Vec<CrateDef>,
    /// Each crate's favourites setting (permanent ones too), by id; a crate not in it shows none.
    #[serde(default)]
    favourites: BTreeMap<String, Favourites>,
    /// Each crate's favourite albums setting (decision 0024, amended 2026-10-19), by id; a crate not in it shows all.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    favourite_albums: BTreeMap<String, Favourites>,
    /// Each crate's tracks left out by hand (decision 0024, amended 2026-10-19), by id: their lines, relative to Music.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    left_out: BTreeMap<String, Vec<String>>,
    /// Each crate's albums left out by hand, by id: the albums' ids (a rule: an album's tracks now and later).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    left_out_albums: BTreeMap<String, Vec<String>>,
    /// The tracks excluded from every automatic pick (decision 0024, amended 2026-10-19: global exclusions): their lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    excluded: Vec<String>,
}

/// One's own crates (decision 0024, issue 2): one file in Cratarr's data folder, the version before each change kept
/// beside it (`crates.json.bak`). Without a data folder they live in memory only.
pub struct CrateStore {
    path: Option<PathBuf>,
    memory: tokio::sync::Mutex<Option<Stored>>,
}

impl CrateStore {
    pub fn new(data_dir: Option<&str>) -> Self {
        Self { path: data_dir.map(|d| PathBuf::from(d).join("crates.json")), memory: tokio::sync::Mutex::new(None) }
    }

    /// Another account's crates (decision 0051: one's own, not shared), in its folder of the data folder.
    pub fn of_account(data_dir: Option<&str>, account: &str) -> Self {
        let path = data_dir.map(|d| PathBuf::from(d).join("users").join(account).join("crates.json"));
        Self { path, memory: tokio::sync::Mutex::new(None) }
    }

    async fn stored(&self) -> Result<Stored> {
        let mut slot = self.memory.lock().await;
        if slot.is_none() {
            *slot = Some(self.read().await?);
        }
        Ok(slot.clone().unwrap_or_default())
    }

    pub async fn list(&self) -> Result<Vec<CrateDef>> {
        Ok(self.stored().await?.crates)
    }

    /// A crate's favourites setting (none until set).
    pub async fn favourites(&self, id: &str) -> Result<Favourites> {
        Ok(self.stored().await?.favourites.get(id).copied().unwrap_or(Favourites::None))
    }

    /// Every crate's favourites setting that was set.
    pub async fn all_favourites(&self) -> Result<BTreeMap<String, Favourites>> {
        Ok(self.stored().await?.favourites)
    }

    pub async fn set_favourites(&self, id: &str, favourites: Favourites) -> Result<()> {
        let mut s = self.stored().await?;
        s.favourites.insert(id.to_string(), favourites);
        self.write_stored(s).await
    }

    /// A crate's favourite albums setting (all until set).
    pub async fn favourite_albums(&self, id: &str) -> Result<Favourites> {
        Ok(self.stored().await?.favourite_albums.get(id).copied().unwrap_or(Favourites::All))
    }

    /// Every crate's favourite albums setting that was set.
    pub async fn all_favourite_albums(&self) -> Result<BTreeMap<String, Favourites>> {
        Ok(self.stored().await?.favourite_albums)
    }

    pub async fn set_favourite_albums(&self, id: &str, albums: Favourites) -> Result<()> {
        let mut s = self.stored().await?;
        s.favourite_albums.insert(id.to_string(), albums);
        self.write_stored(s).await
    }

    /// A crate's tracks and albums left out by hand (none until some are).
    pub async fn left_out(&self, id: &str) -> Result<LeftOut> {
        let s = self.stored().await?;
        Ok(LeftOut { lines: s.left_out.get(id).cloned().unwrap_or_default(), albums: s.left_out_albums.get(id).cloned().unwrap_or_default() })
    }

    /// Every crate's left-out list that holds any.
    pub async fn all_left_out(&self) -> Result<BTreeMap<String, LeftOut>> {
        let s = self.stored().await?;
        let mut all: BTreeMap<String, LeftOut> = BTreeMap::new();
        for (id, lines) in s.left_out {
            all.entry(id).or_default().lines = lines;
        }
        for (id, albums) in s.left_out_albums {
            all.entry(id).or_default().albums = albums;
        }
        Ok(all)
    }

    pub async fn set_left_out(&self, id: &str, left: LeftOut) -> Result<()> {
        let mut s = self.stored().await?;
        for (kept, list) in [(&mut s.left_out, left.lines), (&mut s.left_out_albums, left.albums)] {
            if list.is_empty() {
                kept.remove(id);
            } else {
                kept.insert(id.to_string(), list);
            }
        }
        self.write_stored(s).await
    }

    /// The tracks excluded from every automatic pick (their lines).
    pub async fn excluded(&self) -> Result<Vec<String>> {
        Ok(self.stored().await?.excluded)
    }

    pub async fn set_excluded(&self, lines: Vec<String>) -> Result<()> {
        let mut s = self.stored().await?;
        s.excluded = lines;
        self.write_stored(s).await
    }

    async fn read(&self) -> Result<Stored> {
        let Some(path) = &self.path else { return Ok(Stored::default()) };
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Replaces the crates as one write (the previous file kept as `.bak`); their favourites settings stay.
    async fn write(&self, crates: Vec<CrateDef>) -> Result<()> {
        let s = self.stored().await?;
        self.write_stored(Stored { crates, ..s }).await
    }

    async fn write_stored(&self, mut s: Stored) -> Result<()> {
        s.version = VERSION;
        if let Some(path) = &self.path {
            let bytes = serde_json::to_vec_pretty(&s)?;
            if let Some(dir) = path.parent() {
                tokio::fs::create_dir_all(dir).await.context("making the crates' folder")?;
            }
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                tokio::fs::copy(path, path.with_extension("json.bak")).await.context("keeping the crates' previous version")?;
            }
            let tmp = path.with_extension("json.tmp");
            tokio::fs::write(&tmp, bytes).await.context("writing the crates")?;
            tokio::fs::rename(&tmp, path).await.context("writing the crates")?;
        }
        *self.memory.lock().await = Some(s);
        Ok(())
    }

    /// Saves a crate: a new one (no id) gets one; a known id is changed. The name must be unique among crates and not a
    /// permanent crate's. Returns its id.
    pub async fn save(&self, id: Option<&str>, name: &str, conditions: Vec<Condition>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err("A crate needs a name (at most 100 characters).".into());
        }
        if let Some(c) = conditions.iter().find(|c| !is_axis(&c.axis)) {
            return Err(format!("Unknown axis: {}.", c.axis));
        }
        let mut crates = self.list().await.map_err(|e| format!("{e:#}"))?;
        let taken = permanent().iter().chain(crates.iter()).any(|c| Some(c.id.as_str()) != id && c.name.eq_ignore_ascii_case(name));
        if taken {
            return Err("A crate with that name already exists.".into());
        }
        let id = match id {
            Some(id) => {
                let c = crates.iter_mut().find(|c| c.id == id).ok_or("That crate does not exist (any more).")?;
                c.name = name.to_string();
                c.conditions = conditions;
                id.to_string()
            }
            None => {
                let id = new_id(&crates);
                crates.push(CrateDef { id: id.clone(), name: name.to_string(), conditions });
                id
            }
        };
        self.write(crates).await.map_err(|e| format!("{e:#}"))?;
        Ok(id)
    }

    /// Puts a crate back as it was (an undo, decision 0026): replaced by its id, or added again. Refused when another
    /// crate has its name now.
    pub async fn put(&self, def: CrateDef) -> Result<(), String> {
        let mut crates = self.list().await.map_err(|e| format!("{e:#}"))?;
        if crates.iter().any(|c| c.id != def.id && c.name.eq_ignore_ascii_case(&def.name)) {
            return Err("Another crate has that name now; nothing was undone.".into());
        }
        match crates.iter_mut().find(|c| c.id == def.id) {
            Some(c) => *c = def,
            None => crates.push(def),
        }
        self.write(crates).await.map_err(|e| format!("{e:#}"))
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let mut crates = self.list().await?;
        let before = crates.len();
        crates.retain(|c| c.id != id);
        if crates.len() == before {
            return Ok(false);
        }
        self.write(crates).await?;
        Ok(true)
    }
}

/// A short id no crate has: `c` and a number from the clock.
fn new_id(crates: &[CrateDef]) -> String {
    let mut n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
    loop {
        let id = format!("c{n:x}");
        if !crates.iter().any(|c| c.id == id) && !is_permanent(&id) {
            return id;
        }
        n += 1;
    }
}

/// The years and BPM a file's tags give: the original date's year before the date's; a BPM above zero.
pub fn year_of(tags: &BTreeMap<&str, &str>) -> Option<f64> {
    let year = |s: &str| s.get(..4).and_then(|y| y.parse::<f64>().ok()).filter(|y| *y > 0.0);
    tags.get("originaldate").and_then(|d| year(d)).or_else(|| tags.get("date").and_then(|d| year(d)))
}

/// The genres most of these tracks have (each track's own list), most first, at most `n`: what the panel shows on top
/// of an album or a crate. Spelled as first seen; "rock" and "Rock" are one.
pub fn top_genres<'a>(tracks: impl IntoIterator<Item = &'a [String]>, n: usize) -> Vec<String> {
    let mut count: HashMap<String, (usize, String)> = HashMap::new();
    for genres in tracks {
        let mut seen = HashSet::new();
        for g in genres {
            let key = g.trim().to_lowercase();
            if !key.is_empty() && seen.insert(key.clone()) {
                count.entry(key).or_insert_with(|| (0, g.trim().to_string())).0 += 1;
            }
        }
    }
    let mut out: Vec<(usize, String)> = count.into_values().collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    out.into_iter().take(n).map(|(_, g)| g).collect()
}

pub fn bpm_of(raw: &str) -> Option<f64> {
    raw.trim().replace(',', ".").parse::<f64>().ok().filter(|b| *b > 0.0 && *b < 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genre_is_a_crate_of_its_one_condition() {
        let c = genre_crate("genre:Trip Hop").unwrap();
        assert_eq!(c.name, "Trip Hop");
        assert_eq!(c.conditions, vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Trip Hop".into()] })]);
        assert!(genre_crate("genre: ").is_none());
        assert!(genre_crate("unlisted").is_none());
    }

    #[test]
    fn sounds_like_holds_for_what_sounds_like_the_seeds() {
        let heard = |rel: &str, track: i64, v: [f32; 2]| Facts { rel: rel.into(), track: Some(track), sound: Some(Arc::from(v.as_slice())), ..Facts::default() };
        let files = vec![
            heard("seed.flac", 1, [1.0, 0.0]),
            heard("near.flac", 2, [0.999, 0.045]),
            heard("far.flac", 3, [0.0, 1.0]),
            Facts { rel: "unheard.flac".into(), ..Facts::default() },
        ];
        let like = like_crate("like:tr-1@2", "Seed", vec!["tr-1".into()], 2);
        let membership = HashMap::new();
        let w = World { membership: &membership, favourites: None, favourite_albums: None };
        let every: Vec<&Facts> = files.iter().collect();
        assert_eq!(select(&files, &resolved(&like.conditions, &files, &every), &w), ["seed.flac", "near.flac"], "the closest two");
        let all = like_crate("like:tr-1", "Seed", vec!["tr-1".into()], 10);
        assert_eq!(select(&files, &resolved(&all.conditions, &files, &every), &w), ["seed.flac", "near.flac", "far.flac"], "every heard one");
        // The seed left out of what the crate may show: the closest two are then the next two, still two.
        let shown: Vec<&Facts> = files.iter().filter(|f| f.rel != "seed.flac").collect();
        assert_eq!(select(&files, &resolved(&like.conditions, &files, &shown), &w), ["near.flac", "far.flac"]);
        assert_eq!(like_of("like:tr-1@2"), Some(("tr-1", LIKE_FEWEST)));
        assert_eq!(like_of("like:al-x"), Some(("al-x", LIKE_COUNT)));
        assert_eq!(like_of("like:al-x@5000"), Some(("al-x", LIKE_MOST)));
        // Unworked out, or seeds never heard: nothing.
        assert!(select(&files, &like.conditions, &w).is_empty());
        let unheard = like_crate("like:tr-9", "Unheard", vec!["tr-9".into()], 10);
        assert!(select(&files, &resolved(&unheard.conditions, &files, &every), &w).is_empty());
    }

    #[test]
    fn a_profile_names_what_tells_and_leaves_out_one_value() {
        let t = |rel: &str, bpm: f64, styles: &[&str]| Facts {
            rel: rel.into(),
            year: Some(2003.0),
            bpm: Some(bpm),
            styles: styles.iter().map(|s| s.to_string()).collect(),
            ..Facts::default()
        };
        let album = [t("A/1.flac", 170.0, &["jungle", "electronic"]), t("A/2.flac", 172.0, &["jungle", "electronic"]), t("A/3.flac", 120.0, &["odd", "electronic"])];
        let mut library: Vec<Facts> = (0..40).map(|n| t(&format!("L/{n}.flac"), 100.0, &["electronic"])).collect();
        library.extend(album.iter().cloned());
        let (list, all): (Vec<&Facts>, Vec<&Facts>) = (album.iter().collect(), library.iter().collect());
        let p = profile(&list, &all);
        assert_eq!(p.styles, ["jungle"], "the rare one; electronic is everywhere, one track's odd tag not enough");
        assert_eq!(p.numbers.iter().map(|r| (r.key, r.one)).collect::<Vec<_>>(), [("tempo", None), ("year", Some(2003.0))], "the year is one value");
        assert_eq!(p.numbers[0].mid, 170.0);
    }

    #[test]
    fn any_or_all_is_offered_only_where_an_items_tracks_differ() {
        let t = |rel: &str, year: f64, bpm: f64| Facts { rel: rel.into(), year: Some(year), bpm: Some(bpm), ..Facts::default() };
        let album = [t("A/1.flac", 1998.0, 120.0), t("A/2.flac", 1998.0, 128.0)];
        let items: Vec<Vec<&Facts>> = vec![album.iter().collect()];
        let slices: Vec<&[&Facts]> = items.iter().map(Vec::as_slice).collect();
        let axes = list_axes(&slices, &[], None);
        let varies = |k: &str| axes.iter().find(|a| a.key == k).unwrap().varies;
        assert!(!varies("year") && !varies("favourite_album"), "an album's tracks share its year and its heart");
        assert!(varies("bpm") && varies("favourite"));
    }

    fn f(rel: &str, genres: &[&str], year: Option<f64>, bpm: Option<f64>) -> Facts {
        Facts { rel: rel.into(), genres: genres.iter().map(|g| g.to_string()).collect(), year, bpm, ..Facts::default() }
    }

    fn library() -> (Vec<Facts>, HashMap<String, Vec<String>>, HashSet<String>) {
        let files = vec![
            f("a", &["Rock"], Some(1994.0), Some(120.0)),
            f("b", &["Jazz"], Some(1972.0), None),
            f("c", &["rock", "Indie"], Some(2005.0), Some(90.0)),
            f("d", &[], None, Some(128.0)),
        ];
        let membership = HashMap::from([("a".to_string(), vec!["Chill".to_string()]), ("c".to_string(), vec!["Road".to_string()])]);
        let favourites = HashSet::from(["a".to_string(), "b".to_string()]);
        (files, membership, favourites)
    }

    #[test]
    fn the_permanent_crate_is_conditions_like_any_other_and_the_favourites_toggle_adds_one() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        assert_eq!(permanent().iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["unlisted"]);
        let unlisted = &permanent()[0].conditions;
        assert_eq!(select(&files, unlisted, &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::All.with(unlisted), &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::Only.with(unlisted), &w), vec!["b"], "only its favourites");
        assert_eq!(select(&files, &Favourites::None.with(unlisted), &w), vec!["d"], "none of them");
        assert_eq!(select(&files, &Favourites::Only.with(&[]), &w), vec!["a", "b"], "the whole library's favourites");
        let unknown = World { membership: &membership, favourites: None, favourite_albums: None };
        assert!(select(&files, &Favourites::Only.with(unlisted), &unknown).is_empty(), "favourites not known: nothing, not everything");
    }

    #[test]
    fn the_favourite_albums_toggle_adds_its_own_condition_beside_the_favourites_one() {
        let (files, membership, favourites) = library();
        // "a" and "c" are on a favourite album; "a" and "b" are favourite tracks.
        let albums = HashSet::from(["a".to_string(), "c".to_string()]);
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: Some(&albums) };
        let all = select(&files, &shown(&[], Favourites::All, Favourites::All), &w);
        let on: Vec<&str> = all.iter().copied().filter(|r| albums.contains(*r)).collect();
        let off: Vec<&str> = all.iter().copied().filter(|r| !albums.contains(*r)).collect();
        assert_eq!(select(&files, &shown(&[], Favourites::All, Favourites::Only), &w), on);
        assert_eq!(select(&files, &shown(&[], Favourites::All, Favourites::None), &w), off);
        assert_eq!(select(&files, &shown(&[], Favourites::Only, Favourites::None), &w), vec!["b"], "a favourite track not on a favourite album");
        assert!(needs_favourite_albums(&shown(&[], Favourites::All, Favourites::Only)));
        assert!(!needs_favourite_albums(&shown(&[], Favourites::Only, Favourites::All)));
        assert!(is_axis("favourite_album"));
        // Not known: nothing is shown on that ground.
        let unknown = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        assert!(select(&files, &shown(&[], Favourites::All, Favourites::Only), &unknown).is_empty());
    }

    #[test]
    fn audiomuses_axes_select_on_mood_energy_danceability_and_style() {
        let mk = |rel: &str, energy, dance, moods: &[&str], styles: &[&str]| Facts {
            rel: rel.into(),
            energy,
            danceability: dance,
            moods: moods.iter().map(|s| s.to_string()).collect(),
            styles: styles.iter().map(|s| s.to_string()).collect(),
            ..Facts::default()
        };
        let files = vec![
            mk("a", Some(80.0), Some(70.0), &["happy", "party"], &["electronic"]),
            mk("b", Some(20.0), Some(10.0), &["relaxed"], &["ambient", "electronic"]),
            mk("c", None, None, &[], &[]),
        ];
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        let c = |axis: &str, value: Value| Condition::new(axis, false, value);
        assert_eq!(select(&files, &[c("mood", Value::AnyOf { values: vec!["Happy".into()] })], &w), vec!["a"]);
        assert_eq!(select(&files, &[c("energy", Value::Range { min: Some(50.0), max: None })], &w), vec!["a"]);
        assert_eq!(select(&files, &[c("danceability", Value::Range { min: None, max: Some(50.0) })], &w), vec!["b"]);
        assert_eq!(select(&files, &[c("style", Value::AnyOf { values: vec!["electronic".into()] })], &w), vec!["a", "b"]);
        let ax = axes(&files, &[]);
        let get = |k: &str| ax.iter().find(|a| a.key == k).unwrap();
        assert_eq!(get("style").values, vec!["electronic", "ambient"], "the most common first");
        assert_eq!((get("energy").min, get("energy").max, get("energy").known), (Some(20.0), Some(80.0), 2));
        assert_eq!(get("mood").known, 2, "heard by CLAP");
    }

    #[test]
    fn tracks_left_out_by_hand_stay_out_over_conditions_and_both_toggles() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        let left = vec!["a".to_string()];
        // "a" is a favourite: left out, it stays out with only favourites shown.
        assert_eq!(select(&files, &leaving_out(&left, &Favourites::Only.with(&[])), &w), vec!["b"]);
        assert_eq!(select(&files, &leaving_out(&left, &[]), &w), vec!["b", "c", "d"]);
        assert_eq!(leaving_out(&[], &[]), Vec::new(), "nothing left out, nothing added");
        assert!(!is_axis("line"), "not an axis one builds with");
    }

    #[test]
    fn conditions_combine_with_and_take_several_values_or_a_range_and_can_be_negated() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        let c = |axis: &str, not: bool, value: Value| Condition::new(axis, not, value);
        let rock_or_jazz = c("genre", false, Value::AnyOf { values: vec!["ROCK".into(), "jazz".into()] });
        assert_eq!(select(&files, &[rock_or_jazz.clone()], &w), vec!["a", "b", "c"], "any of, case ignored");
        let nineties = c("year", false, Value::Range { min: Some(1990.0), max: Some(1999.0) });
        assert_eq!(select(&files, &[rock_or_jazz.clone(), nineties], &w), vec!["a"]);
        let not_chill = c("playlist", true, Value::AnyOf { values: vec!["Chill".into()] });
        assert_eq!(select(&files, &[rock_or_jazz, not_chill], &w), vec!["b", "c"]);
        let fast = c("bpm", false, Value::Range { min: Some(110.0), max: None });
        assert_eq!(select(&files, &[fast], &w), vec!["a", "d"], "an open end; files without a BPM never match");
        let future = c("mood", false, Value::Yes);
        assert!(select(&files, &[future], &w).is_empty(), "an unknown axis narrows, never widens");
    }

    #[test]
    fn saved_conditions_load_on_of_any_track_and_save_as_they_were() {
        let saved = r#"{"axis":"genre","not":false,"value":{"kind":"any_of","values":["Dub"]}}"#;
        let c: Condition = serde_json::from_str(saved).unwrap();
        assert_eq!((c.enabled, c.quantifier), (true, Quantifier::Any));
        assert_eq!(serde_json::to_string(&c).unwrap(), saved, "crates.json unchanged");
        let muted: Condition = serde_json::from_str(r#"{"axis":"bpm","value":{"kind":"range","min":120,"max":null},"enabled":false,"quantifier":"all"}"#).unwrap();
        assert_eq!((muted.enabled, muted.quantifier), (false, Quantifier::All));
    }

    /// Albums of a list in view: "x" all rock and analysed; "y" one rock, one jazz, one without a genre or energy;
    /// "z" no genre and no energy at all.
    fn albums() -> Vec<Vec<Facts>> {
        let t = |rel: &str, genre: Option<&str>, energy: Option<f64>| Facts {
            rel: rel.into(),
            genres: genre.map(|g| vec![g.to_string()]).unwrap_or_default(),
            energy,
            ..Facts::default()
        };
        vec![
            vec![t("x1", Some("Rock"), Some(80.0)), t("x2", Some("rock"), Some(70.0))],
            vec![t("y1", Some("Rock"), Some(90.0)), t("y2", Some("Jazz"), Some(20.0)), t("y3", None, None)],
            vec![t("z1", None, None), t("z2", None, None)],
        ]
    }

    fn passing(items: &[Vec<Facts>], conditions: &[Condition]) -> Vec<usize> {
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        items
            .iter()
            .enumerate()
            .filter(|(_, t)| passes(&t.iter().collect::<Vec<_>>(), conditions, &w))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn any_is_one_of_an_items_tracks_and_all_every_one_with_a_value() {
        let items = albums();
        let rock = |q: Quantifier, not: bool| Condition { quantifier: q, ..Condition::new("genre", not, Value::AnyOf { values: vec!["rock".into()] }) };
        assert_eq!(passing(&items, &[rock(Quantifier::Any, false)]), [0, 1], "one rock track is enough");
        assert_eq!(passing(&items, &[rock(Quantifier::All, false)]), [0], "y's jazz track counts, its untagged one not; z has no value at all");
        // Negated: Any, a track that is not rock (an untagged one is not); All, no track with a value is rock.
        assert_eq!(passing(&items, &[rock(Quantifier::Any, true)]), [1, 2]);
        assert_eq!(passing(&items, &[rock(Quantifier::All, true)]), [2], "z: none of its tracks has a genre, so none is rock");
        let high = |q: Quantifier| Condition { quantifier: q, ..Condition::new("energy", false, Value::Range { min: Some(60.0), max: None }) };
        assert_eq!(passing(&items, &[high(Quantifier::All)]), [0]);
        // All conditions hold, each with its own quantifier.
        assert_eq!(passing(&items, &[rock(Quantifier::Any, false), high(Quantifier::Any)]), [0, 1]);
        assert_eq!(passing(&items, &[rock(Quantifier::Any, false), high(Quantifier::All)]), [0]);
        // A track alone: Any and All say the same.
        let one: Vec<Vec<Facts>> = items.iter().flatten().map(|f| vec![f.clone()]).collect();
        for not in [false, true] {
            assert_eq!(passing(&one, &[rock(Quantifier::Any, not)]), passing(&one, &[rock(Quantifier::All, not)]));
        }
    }

    #[test]
    fn a_muted_condition_is_ignored() {
        let items = albums();
        let jazz = Condition::new("genre", false, Value::AnyOf { values: vec!["jazz".into()] });
        assert_eq!(passing(&items, &[jazz.clone()]), [1]);
        let muted = Condition { enabled: false, ..jazz };
        assert_eq!(passing(&items, &[muted.clone()]), [0, 1, 2], "nothing on: everything passes");
        let files: Vec<Facts> = items.into_iter().flatten().collect();
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        assert_eq!(select(&files, &[muted], &w).len(), files.len(), "in a crate too");
        let fav = Condition { enabled: false, ..Condition::new("favourite", false, Value::Yes) };
        assert!(!needs_favourites(&[fav]), "a muted favourite needs no favourites");
    }

    #[test]
    fn a_lists_axes_count_its_items() {
        let items = albums();
        let refs: Vec<Vec<&Facts>> = items.iter().map(|t| t.iter().collect()).collect();
        let slices: Vec<&[&Facts]> = refs.iter().map(Vec::as_slice).collect();
        let membership = HashMap::from([("x1".to_string(), vec!["Road".to_string()]), ("y2".to_string(), vec!["Road".to_string()])]);
        let ax = list_axes(&slices, &["Road".into(), "Chill".into()], Some(&membership));
        let get = |k: &str| ax.iter().find(|a| a.key == k).unwrap();
        assert_eq!((get("genre").values.clone(), get("genre").counts.clone()), (vec!["Rock".to_string(), "Jazz".to_string()], vec![2, 1]), "albums, not tracks: x's two rock tracks count once");
        assert_eq!(get("genre").known, 2, "z has none");
        assert_eq!((get("energy").min, get("energy").max, get("energy").known), (Some(20.0), Some(90.0), 2));
        assert_eq!(get("playlist").counts, vec![2, 0]);
        assert!(axes(&items[0], &["Road".into()]).iter().find(|a| a.key == "playlist").unwrap().counts.is_empty(), "the builder's: not counted");
    }

    #[test]
    fn the_axes_know_their_domains() {
        let (files, _, _) = library();
        let axes = axes(&files, &["Chill".into(), "Road".into()]);
        let get = |k: &str| axes.iter().find(|a| a.key == k).unwrap();
        assert_eq!(get("genre").values, vec!["Rock", "Indie", "Jazz"], "most common first, rock and Rock as one");
        assert_eq!((get("year").min, get("year").max, get("year").known), (Some(1972.0), Some(2005.0), 3));
        assert_eq!((get("bpm").min, get("bpm").max, get("bpm").known), (Some(90.0), Some(128.0), 3));
        assert_eq!(get("playlist").values, vec!["Chill", "Road"]);
    }

    #[test]
    fn tags_give_years_and_bpms() {
        let tags = BTreeMap::from([("date", "2001-05-01"), ("originaldate", "1999")]);
        assert_eq!(year_of(&tags), Some(1999.0), "the original release first");
        assert_eq!(year_of(&BTreeMap::from([("date", "2001")])), Some(2001.0));
        assert_eq!((bpm_of("128"), bpm_of(" 92,5 "), bpm_of("0"), bpm_of("fast")), (Some(128.0), Some(92.5), None, None));
    }

    #[tokio::test]
    async fn crates_are_kept_with_unique_names_and_stable_ids() {
        let dir = std::env::temp_dir().join(format!("cratarr-crates-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = CrateStore::new(dir.to_str());
        let rock = vec![Condition::new("genre", false, Value::AnyOf { values: vec!["rock".into()] })];
        let id = store.save(None, "Rock", rock.clone()).await.unwrap();
        assert!(store.save(None, "rock", rock.clone()).await.is_err(), "names are unique, case ignored");
        assert!(store.save(None, "Unlisted", rock.clone()).await.is_err(), "a permanent crate's name");
        assert_eq!(store.save(Some(&id), "Rock all", rock.clone()).await.unwrap(), id, "a rename keeps the id");
        assert_eq!(store.favourites(&id).await.unwrap(), Favourites::None, "a new crate shows no favourites");
        store.set_favourites(&id, Favourites::Only).await.unwrap();
        store.set_favourites("unlisted", Favourites::All).await.unwrap();
        let again = CrateStore::new(dir.to_str());
        assert_eq!(again.list().await.unwrap()[0].name, "Rock all", "kept in crates.json");
        assert_eq!(again.favourites(&id).await.unwrap(), Favourites::Only, "the setting too");
        assert_eq!(again.favourites("unlisted").await.unwrap(), Favourites::All, "a permanent crate's as well");
        again.save(Some(&id), "Rock", rock.clone()).await.unwrap();
        assert_eq!(again.favourites(&id).await.unwrap(), Favourites::Only, "a change to the crate keeps it");
        // The favourite albums toggle (decision 0024, amended 2026-10-19): all until set, kept the same way.
        assert_eq!(again.favourite_albums(&id).await.unwrap(), Favourites::All, "all until set");
        again.set_favourite_albums(&id, Favourites::None).await.unwrap();
        again.save(Some(&id), "Rock", rock.clone()).await.unwrap();
        assert_eq!(CrateStore::new(dir.to_str()).favourite_albums(&id).await.unwrap(), Favourites::None, "kept, and through a change");
        // Tracks and albums left out by hand: kept per crate, through a change of it; an empty list is none.
        let left = LeftOut { lines: vec!["x.flac".into()], albums: vec!["al-1".into()] };
        again.set_left_out(&id, left.clone()).await.unwrap();
        again.save(Some(&id), "Rock", rock.clone()).await.unwrap();
        assert_eq!(CrateStore::new(dir.to_str()).left_out(&id).await.unwrap(), left);
        assert_eq!(again.all_left_out().await.unwrap().get(&id).map(LeftOut::len), Some(2));
        again.set_left_out(&id, LeftOut::default()).await.unwrap();
        assert!(again.all_left_out().await.unwrap().is_empty());
        // Excluded everywhere: one list for the account, kept.
        again.set_excluded(vec!["y.flac".into()]).await.unwrap();
        assert_eq!(CrateStore::new(dir.to_str()).excluded().await.unwrap(), vec!["y.flac".to_string()]);
        assert!(dir.join("crates.json.bak").exists(), "the version before kept");
        assert!(again.delete(&id).await.unwrap());
        assert!(again.list().await.unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn an_accounts_crates_are_its_own() {
        let dir = std::env::temp_dir().join(format!("cratarr-crates-ann-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (first, ann) = (CrateStore::new(dir.to_str()), CrateStore::of_account(dir.to_str(), "ann"));
        ann.save(None, "Late", Vec::new()).await.unwrap();
        assert!(dir.join("users/ann/crates.json").is_file(), "in her folder, made with her first crate");
        assert!(first.list().await.unwrap().is_empty(), "not the first administrator's");
        first.save(None, "Late", Vec::new()).await.unwrap();
        assert_eq!(ann.list().await.unwrap().len(), 1, "a name is unique among one's own only");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_top_genres_are_the_most_shared_counted_once_per_track() {
        let t = |g: &[&str]| g.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let tracks = [t(&["Jazz", "jazz", "Soul"]), t(&["jazz"]), t(&["Funk", "Soul"]), t(&[])];
        assert_eq!(top_genres(tracks.iter().map(|v| v.as_slice()), 5), vec!["Jazz", "Soul", "Funk"]);
        assert_eq!(top_genres(tracks.iter().map(|v| v.as_slice()), 1), vec!["Jazz"]);
    }

    #[test]
    fn own_favourite_changes_are_numbered_per_server_and_dropped_once_heard() {
        // Its own server names: the log is shared by every test.
        let before = favourite_changes_made();
        favourite_changed("test-a", "t1", true);
        favourite_changed("test-b", "t2", true);
        favourite_changed("test-a", "t1", false);
        assert_eq!(favourite_changes_since("test-a", before), vec![("t1".to_string(), true), ("t1".to_string(), false)]);
        let held = favourite_changes_made();
        favourite_changed("test-a", "t3", true);
        favourite_changes_held("test-a", held);
        assert_eq!(favourite_changes_since("test-a", before), vec![("t3".to_string(), true)]);
        assert_eq!(favourite_changes_since("test-b", before), vec![("t2".to_string(), true)]);
    }

    #[test]
    fn tempos_fall_in_five_bpm_bands_from_60_to_180() {
        let t = tempo_bands([Some(60.0), Some(64.9), Some(65.0), Some(119.5), Some(120.0), None, Some(0.0), Some(40.0), Some(181.0), Some(179.9)]);
        assert_eq!((t.total, t.with_tempo), (10, 8), "no tempo and 0 are not counted in a band");
        assert_eq!(t.bands.len(), TEMPO_BANDS);
        assert_eq!((t.bands[0], t.bands[1]), (3, 1), "60, 64.9 and the 40 below the scale; 65");
        assert_eq!((t.bands[11], t.bands[12]), (1, 1), "119.5; 120");
        assert_eq!(t.bands[23], 2, "179.9 and the 181 above the scale");
        assert_eq!(tempo_bands([]), TempoBands { bands: vec![0; TEMPO_BANDS], with_tempo: 0, total: 0 });
    }
}
