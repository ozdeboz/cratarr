//! Crates (decision 0024): dynamic cross-sections of the library. A crate is a list of conditions on the library's axes
//! (all must hold, each negatable; OR within an axis through several values or a range), worked out when it is looked
//! at. The permanent crates are definitions in the same model; one's own are kept in `crates.json`. The axes are a
//! registry: each declares its kind and domain, and how a file is tested, so the builder draws its controls from it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::similarity::{Library, Track, Weights};

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

    /// Whether it asks anything yet (the filter API, 2026-10-29: one meaning with the page's `complete`, both reading
    /// web/tests/fixtures/conditions.json): any of none asks nothing, but a playlist's (in any playlist); a range open at
    /// both ends asks nothing; a like with no seeds asks nothing. Not, Any and All change nothing about it. A row that
    /// asks nothing filters nothing: it is left out, as a muted one is.
    pub fn complete(&self) -> bool {
        match &self.value {
            Value::AnyOf { values } => !values.is_empty() || self.axis == "playlist",
            Value::Range { min, max } => min.is_some() || max.is_some(),
            Value::Like { of, .. } => !of.is_empty(),
            _ => true,
        }
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
    /// `fresh`: its seeds themselves left out (Sounds like your week: what was played is not offered again).
    /// `weights`: how much each seed counts, by `of`'s order (Sounds like your week: how often and how lately it was
    /// heard, decision 0064); none, all alike.
    Like {
        of: Vec<String>,
        count: usize,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fresh: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f32>,
        /// Leaning towards tracks in no playlist yet (a playlist's suggestions take in forgotten music), mildly: each
        /// moves ahead of at most `LEAN_UNLISTED` closer ones that are in a playlist.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unlisted: bool,
        /// What it should not be like: the traits learned from what was turned down for it, those dismissed left out
        /// (its fit model's other side, `closest_fit`).
        #[serde(default, skip_serializing_if = "Avoid::is_empty")]
        avoid: Avoid,
    },
    /// Close to a moment (Home's weather and time of day, `app::moment`): the `count` tracks nearest its energy and
    /// moods (`Scales::off`). Made when asked for, never kept.
    Moment {
        target: Target,
        count: usize,
    },
    /// A `Like` or a `Moment` worked out (`resolved`): the lines of those tracks, and the same closest first
    /// (`closest_first`). Never kept.
    #[serde(skip)]
    Within { lines: Arc<HashSet<String>>, ranked: Arc<Vec<String>> },
}

/// A crate: its id never changes; its name is unique among crates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateDef {
    pub id: String,
    pub name: String,
    pub conditions: Vec<Condition>,
}

/// A genre, viewed in the panel: a crate of one condition, made when asked for (`genre:<name>`), never saved. Its tracks
/// are those whose genre holds the name (as the crate builder's genre axis matches: the files' genres and the
/// library's words of that kind, `app::words`). A style, a mood and a tag are crates the same way (0.9.7, the owner:
/// the words' page is the genre crate's): `style:`, `mood:`, `tag:`.
pub const GENRE: &str = "genre:";
const WORDS: [&str; 4] = ["genre", "style", "mood", "tag"];

pub fn genre_crate(id: &str) -> Option<CrateDef> {
    let (axis, name) = id.split_once(':')?;
    let name = name.trim();
    if !WORDS.contains(&axis) || name.is_empty() {
        return None;
    }
    let word = Condition::new(axis, false, Value::AnyOf { values: vec![name.to_string()] });
    Some(CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![word] })
}

/// A track or an album, viewed in the panel as a crate of what sounds like it (the owner, 2026-10-23: "just like genre,
/// albums and tracks can now become seeds for crates"): made when asked for (`like:<id>`), never saved. The caller
/// names it and turns an album into its songs (`like_crate`).
/// Home's shelves as crates (the owner, 2026-10-24: New for you's and Grave digging's › open them, not the Library,
/// whose own filter could hide them): made when asked for, never saved; their tracks the albums the shelf shows, album
/// by album in its order, of every library (what Home showed, whatever is switched off).
pub const HOME_NEW: &str = "home:new";
pub const HOME_GRAVES: &str = "home:graves";
/// Sounds like your week, Home's crate of what sounds like the account's week (`like_crate`'s `WEEK`).
pub const HOME_WEEK: &str = "like:week";
pub const fn home_crate(id: &str) -> bool {
    matches!(id.as_bytes(), b"home:new" | b"home:graves")
}

/// A crate of exactly these lines, in this order (a Home shelf's).
pub fn lines_crate(id: &str, name: &str, lines: Vec<String>) -> CrateDef {
    let within = Condition::new("like", false, Value::Within { lines: Arc::new(lines.iter().cloned().collect()), ranked: Arc::new(lines) });
    CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![within] }
}

pub const LIKE: &str = "like:";
/// The seed of "Sounds like your week" (`like:week[@<count>]`): the songs the asking account played in the last 7
/// days, the closest not among them (the owner, 2026-10-23; for 75's Home).
pub const WEEK: &str = "week";
/// How far back "your week" reaches (s).
pub const WEEK_SECS: i64 = 7 * 24 * 3600;

/// "Sounds like your week": seeded with `played` (song ids, each with how much it counts: none, all alike), which it
/// leaves out.
pub fn week_crate(id: &str, played: Vec<(String, f32)>, count: usize) -> CrateDef {
    let (of, weights) = played.into_iter().unzip();
    let like = Condition::new("like", false, Value::Like { of, count, fresh: true, weights, unlisted: false, avoid: Avoid::default() });
    CrateDef { id: id.to_string(), name: "Sounds like your week".into(), conditions: vec![like] }
}
/// Home's moment (`like:now[@<count>]`, `app::moment`): the tracks nearest what the weather and the time of day ask,
/// named after the moment ("Music for a Rainy Evening"); Home's radio plays inside it.
pub const NOW: &str = "now";

pub fn moment_crate(id: &str, name: &str, target: Target, count: usize) -> CrateDef {
    let near = Condition::new("like", false, Value::Moment { target, count });
    CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![near] }
}

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
    let like = Condition::new("like", false, Value::Like { of: songs, count, fresh: false, weights: Vec::new(), unlisted: false, avoid: Avoid::default() });
    CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![like] }
}

/// A playlist's suggestions (`like:list:<playlist id>`; the owner, 2026-10-23: tracks to add to it, "secretly each
/// playlist would get a crate"): what sounds like its tracks, its own left out, leaning towards tracks in no playlist
/// yet; its fit (`Fit`) as every ranked crate's.
pub const LIST: &str = "list:";

pub fn playlist_crate(id: &str, name: &str, songs: Vec<String>, count: usize) -> CrateDef {
    let like = Condition::new("like", false, Value::Like { of: songs, count, fresh: true, weights: Vec::new(), unlisted: true, avoid: Avoid::default() });
    CrateDef { id: id.to_string(), name: format!("Sounds like {name}"), conditions: vec![like] }
}

/// How many closer tracks in a playlist one in none may pass (`Value::Like`'s `unlisted`), and how many beyond its
/// count a Sounds like weighs for that.
const LEAN_UNLISTED: usize = 3;
const LEAN_POOL: usize = 50;

/// A trait of what a ranked crate should not be like (its fit model's other side; the owner, 2026-10-23: learned from
/// what was turned down for it, shown, and each one can be dismissed: "that's not why").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trait {
    /// An AudioMuse style.
    Style { name: String },
    Artist { name: String },
    /// One of CLAP's moods standing out.
    Mood { name: String },
    /// A tempo band (BPM).
    Tempo { lo: f64, hi: f64 },
    /// An energy band (AudioMuse's, 0 to 100): calm or lively against the library.
    Energy { lo: f64, hi: f64 },
}

impl Trait {
    /// What it is dismissed by: one of each band, a name of the others.
    pub fn key(&self) -> String {
        match self {
            Trait::Style { name } => format!("style:{}", name.to_lowercase()),
            Trait::Artist { name } => format!("artist:{}", name.to_lowercase()),
            Trait::Mood { name } => format!("mood:{}", name.to_lowercase()),
            Trait::Tempo { .. } => "tempo".into(),
            Trait::Energy { .. } => "energy".into(),
        }
    }

    /// Whether a file has it.
    pub fn holds(&self, f: &Facts) -> bool {
        let named = |names: &[String], name: &str| names.iter().any(|n| n.eq_ignore_ascii_case(name));
        match self {
            Trait::Style { name } => named(&f.styles, name),
            Trait::Artist { name } => named(&f.artists, name),
            Trait::Mood { name } => named(&f.moods, name),
            Trait::Tempo { lo, hi } => f.bpm.is_some_and(|b| (*lo..=*hi).contains(&b)),
            Trait::Energy { lo, hi } => f.energy.is_some_and(|e| (*lo..=*hi).contains(&e)),
        }
    }
}

/// What a ranked crate should not be like, and how much that counts against its own pull.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Avoid {
    pub traits: Vec<Trait>,
    pub weight: f64,
}

/// How much what was turned down counts against a crate's own pull at most, and after how many turned down half that
/// (one or two are mostly noise, a handful shape it).
const AVOID_MOST: f64 = 0.75;
const AVOID_HALF: f64 = 3.0;

impl Avoid {
    /// `traits` (those not dismissed), learned from `turned_down` tracks.
    pub fn of(traits: Vec<Trait>, turned_down: usize) -> Self {
        let n = turned_down as f64;
        Avoid { weight: if traits.is_empty() { 0.0 } else { AVOID_MOST * n / (n + AVOID_HALF) }, traits }
    }

    pub fn is_empty(&self) -> bool {
        self.traits.is_empty()
    }

    /// What a file loses of its closeness: the share of the traits it has, weighed.
    pub fn against(&self, f: &Facts) -> f64 {
        if self.traits.is_empty() {
            return 0.0;
        }
        self.weight * self.traits.iter().filter(|t| t.holds(f)).count() as f64 / self.traits.len() as f64
    }
}

/// The fewest turned down to learn from; how many styles, artists and moods at most; how narrow a tempo band (BPM) and
/// how far from the library's middle an energy (its scale's share) to count.
const LEARN_FROM: usize = 2;
const LEARN_NAMES: usize = 3;
const TEMPO_SPAN: f64 = 16.0;
const ENERGY_OFF: f64 = 0.25;

/// What the tracks turned down for a crate share, against the library: their telling styles, an artist on two or more,
/// a mood standing out on half of them, a narrow tempo band, an energy far from the library's middle.
pub fn learned(down: &[&Facts], library: &[&Facts]) -> Vec<Trait> {
    if down.len() < LEARN_FROM {
        return Vec::new();
    }
    let scales = Scales::of(library);
    let mut out: Vec<Trait> =
        telling(down, &scales.styles, scales.tracks, |f| &f.styles).into_iter().take(LEARN_NAMES).map(|name| Trait::Style { name }).collect();
    let by_count = |names: fn(&Facts) -> &[String], least: usize| {
        let mut c: Vec<(usize, String)> = name_counts(down, names).into_values().filter(|(n, _)| *n >= least).collect();
        c.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        c.into_iter().take(LEARN_NAMES).map(|(_, name)| name).collect::<Vec<_>>()
    };
    out.extend(by_count(|f| &f.artists, 2).into_iter().map(|name| Trait::Artist { name }));
    out.extend(by_count(|f| &f.moods, LEARN_FROM.max(down.len().div_ceil(2))).into_iter().map(|name| Trait::Mood { name }));
    let bpms = sorted(down.iter().filter_map(|f| f.bpm));
    if bpms.len() >= 3 {
        let (lo, hi) = (crate::similarity::quantile(&bpms, 0.25), crate::similarity::quantile(&bpms, 0.75));
        if hi - lo <= TEMPO_SPAN {
            out.push(Trait::Tempo { lo: lo.floor(), hi: hi.ceil() });
        }
    }
    let energies = sorted(down.iter().filter_map(|f| f.energy));
    if energies.len() >= LEARN_FROM {
        let (lo, hi) = scales.numbers[1];
        let middle = (crate::similarity::quantile(&energies, 0.5) - lo) / (hi - lo).max(f64::EPSILON);
        if middle >= 0.5 + ENERGY_OFF {
            out.push(Trait::Energy { lo: crate::similarity::quantile(&energies, 0.25).floor(), hi: 100.0 });
        } else if middle <= 0.5 - ENERGY_OFF {
            out.push(Trait::Energy { lo: 0.0, hi: crate::similarity::quantile(&energies, 0.75).ceil() });
        }
    }
    out
}

/// A ranked crate's fit kept beside it (the owner, 2026-10-23): its strict rules (conditions it holds besides its
/// own), and the learned traits dismissed (`Trait::key`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    #[serde(default)]
    pub rules: Vec<Condition>,
    #[serde(default)]
    pub dismissed: Vec<String>,
}

impl Fit {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.dismissed.is_empty()
    }
}

/// `ranked` with each line in no playlist moved ahead of at most `LEAN_UNLISTED` listed ones just before it, the
/// unlisted keeping their order among themselves.
fn lean_unlisted(ranked: &mut Vec<String>, listed: impl Fn(&str) -> bool) {
    let mut out: Vec<String> = Vec::with_capacity(ranked.len());
    for line in ranked.drain(..) {
        let mut at = out.len();
        if !listed(&line) {
            let mut passed = 0;
            while at > 0 && passed < LEAN_UNLISTED && listed(&out[at - 1]) {
                at -= 1;
                passed += 1;
            }
        }
        out.insert(at, line);
    }
    *ranked = out;
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

/// Whether a crate is one of the fixed ones, made by Cratarr with no filter of the listener's (decision 0083): the
/// permanent Unlisted, a genre's (or a style's, a mood's, a tag's), Home's (New for you, Grave digging), and every
/// Sounds like (Sounds like your week, the moment's, a track's, an album's, a playlist's suggestions).
pub fn is_fixed(id: &str) -> bool {
    is_permanent(id) || genre_crate(id).is_some() || home_crate(id) || id.starts_with(LIKE)
}

/// The id a fixed crate's own conditions are kept by: a Sounds like's without its count (as its fit), else its own.
pub fn kept_id(id: &str) -> String {
    match like_of(id) {
        Some((item, _)) => format!("{LIKE}{item}"),
        None => id.to_string(),
    }
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
    profile_in(list, &Scales::of(library))
}

/// What a profile measures a list against, worked out once for the library: each number's scale, each mood's middle
/// and half its spread, and how many tracks have each style tag and artist (their rarity).
pub struct Scales {
    numbers: [(f64, f64); 4],
    moods: [(f64, f64); 5],
    styles: HashMap<String, usize>,
    artists: HashMap<String, usize>,
    tracks: usize,
}

/// The numbers a profile has, in its order, and where each is in a file's facts.
const PROFILE_NUMBERS: [(&str, fn(&Facts) -> Option<f64>); 4] =
    [("tempo", |f| f.bpm), ("energy", |f| f.energy), ("danceability", |f| f.danceability), ("year", |f| f.year)];

fn sorted(vals: impl IntoIterator<Item = f64>) -> Vec<f64> {
    let mut v: Vec<f64> = vals.into_iter().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    v
}

/// How many of `fs` have each name (lowercased), each once per file.
fn name_counts(fs: &[&Facts], names: fn(&Facts) -> &[String]) -> HashMap<String, (usize, String)> {
    let mut c: HashMap<String, (usize, String)> = HashMap::new();
    for f in fs {
        for name in names(f).iter().collect::<std::collections::BTreeSet<_>>() {
            c.entry(name.to_lowercase()).or_insert((0, name.clone())).0 += 1;
        }
    }
    c
}

impl Scales {
    pub fn of(library: &[&Facts]) -> Self {
        let numbers = PROFILE_NUMBERS.map(|(key, get)| match key {
            "tempo" => (TEMPO_LO, TEMPO_LO + TEMPO_BAND * TEMPO_BANDS as f64),
            // The library's oldest to newest (the owner: its 1950s albums were lost at the 5th percentile, 1993), a
            // year before 1900 being a broken tag ("1").
            "year" => {
                let all = sorted(library.iter().filter_map(|f| f.year).filter(|y| *y >= 1900.0));
                (all.first().copied().unwrap_or(1900.0), all.last().copied().unwrap_or(2100.0))
            }
            _ => {
                let all = sorted(library.iter().filter_map(|f| get(f)));
                (crate::similarity::quantile(&all, 0.05), crate::similarity::quantile(&all, 0.95))
            }
        });
        let moods = std::array::from_fn(|k| {
            let all = sorted(library.iter().filter_map(|f| f.clap_moods.map(|m| m[k] as f64)));
            let half = (crate::similarity::quantile(&all, 0.95) - crate::similarity::quantile(&all, 0.05)) / 2.0;
            (crate::similarity::quantile(&all, 0.5), half.max(f64::EPSILON))
        });
        let counts = |names| name_counts(library, names).into_iter().map(|(k, (c, _))| (k, c)).collect();
        Scales { numbers, moods, styles: counts(|f| &f.styles), artists: counts(|f| &f.artists), tracks: library.len() }
    }
}

/// The profile of `list` against the library's `scales`.
pub fn profile_in(list: &[&Facts], scales: &Scales) -> Profile {
    let n = list.len();
    let mut out = Profile { tracks: n, heard: list.iter().filter(|f| f.clap_moods.is_some()).count(), ..Profile::default() };
    out.styles = telling(list, &scales.styles, scales.tracks, |f| &f.styles);
    out.artists = telling(list, &scales.artists, scales.tracks, |f| &f.artists);
    for ((key, get), (lo, hi)) in PROFILE_NUMBERS.into_iter().zip(scales.numbers) {
        let mine = sorted(list.iter().filter_map(|f| get(f)));
        let Some(&first) = mine.first() else { continue };
        if mine.last() == Some(&first) {
            out.numbers.push(NumberRow { key, lo: first, hi: first, bins: Vec::new(), q1: first, mid: first, q3: first, one: Some(first) });
            continue;
        }
        let q = |p| crate::similarity::quantile(&mine, p);
        out.numbers.push(NumberRow { key, lo, hi, bins: density(mine.iter().copied(), lo, hi), q1: q(0.25), mid: q(0.5), q3: q(0.75), one: None });
    }
    if out.heard > 0 {
        for (k, name) in crate::similarity::MOODS.into_iter().enumerate() {
            let mine = sorted(list.iter().filter_map(|f| f.clap_moods.map(|m| m[k] as f64)));
            let (middle, half) = scales.moods[k];
            let lift = (crate::similarity::quantile(&mine, 0.5) - middle) / half;
            out.moods.push(MoodLift { name, lift: lift.clamp(-1.0, 1.0) });
        }
    }
    out
}

/// What a moment asks of the music (`app::moment`): its energy, where on the library's scale (its 5th to 95th
/// percentile: 0 calm, 1 intense), and, when the weather is known, how happy, sad and relaxed against the library (a
/// `MoodLift`'s scale: -1 far below its middle, 1 far above).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub energy: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<[f64; 3]>,
}

/// How much each part of a target counts: energy, then happy, sad and relaxed.
const TARGET_WEIGHTS: [f64; 4] = [2.0, 1.0, 0.7, 0.5];
/// What a track CLAP has not heard is further away, its moods unknown.
const MOODS_UNHEARD: f64 = 0.1;

impl Scales {
    /// How far a file is from a target (0 on it); none without AudioMuse's energy.
    pub fn off(&self, f: &Facts, t: &Target) -> Option<f64> {
        let (lo, hi) = self.numbers[1];
        let energy = ((f.energy? - lo) / (hi - lo).max(f64::EPSILON)).clamp(0.0, 1.0);
        let mut off = TARGET_WEIGHTS[0] * (energy - t.energy).powi(2);
        if let Some(mood) = t.mood {
            match f.clap_moods {
                Some(m) => {
                    for (k, want) in mood.into_iter().enumerate() {
                        let (middle, half) = self.moods[k];
                        let lift = ((m[k] as f64 - middle) / half).clamp(-1.0, 1.0);
                        off += TARGET_WEIGHTS[k + 1] * ((lift - want) / 2.0).powi(2);
                    }
                }
                None => off += MOODS_UNHEARD,
            }
        }
        Some(off)
    }
}

/// The files AudioMuse heard, nearest a target first, each with how far.
pub fn nearest<'a>(among: &[&'a Facts], scales: &Scales, t: &Target) -> Vec<(&'a Facts, f64)> {
    let mut all: Vec<(&Facts, f64)> = among.iter().filter_map(|f| Some((*f, scales.off(f, t)?))).collect();
    all.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.rel.cmp(&b.0.rel)));
    all
}

/// Where a Sounds like crate's handle can stop (as lib/LikeCurve tidies it): every 5 up to 100, every 10 up to 400,
/// every 50 after, from `LIKE_FEWEST` to `most`.
pub fn like_steps(most: usize) -> Vec<usize> {
    (LIKE_FEWEST..=most.min(LIKE_MOST)).filter(|n| if *n < 100 { n % 5 == 0 } else if *n < 400 { n % 10 == 0 } else { n % 50 == 0 }).collect()
}

/// The profile of each of the first `n` of `closest` at every step a handle can stop at (the owner, 2026-10-23: the
/// profile follows the drag), against the library's `scales`, worked out at once so a drag asks nothing more.
pub fn profiles_along(closest: &[&Facts], scales: &Scales) -> Vec<(usize, Profile)> {
    like_steps(closest.len()).into_iter().map(|n| (n, profile_in(&closest[..n], scales))).collect()
}

/// The names most telling of a list: on many of its tracks and few of the library's (`library`: how many of its
/// `total` tracks have each, lowercased; tf-idf, as one similarity weighs words), on at least two of its tracks or a
/// twentieth of them (one odd tag does not lead an album).
fn telling(list: &[&Facts], library: &HashMap<String, usize>, total: usize, names: fn(&Facts) -> &[String]) -> Vec<String> {
    let mine = name_counts(list, names);
    let least = if list.len() < 2 { 1 } else { 2.max(list.len().div_ceil(20)) };
    let total = total.max(1) as f64;
    let mut weighed: Vec<(f64, String)> = mine
        .into_iter()
        .filter(|(_, (c, _))| *c >= least)
        .map(|(k, (c, name))| (c as f64 / list.len() as f64 * (total / library.get(&k).copied().unwrap_or(1).max(1) as f64).ln(), name))
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
    /// The library's words of its album that are tags (0.9.7, `app::words`): Last.fm's, MusicBrainz's, the household's.
    pub tags: Vec<String>,
    /// Its track in the catalogue, and AudioMuse's "sounds like" vector of it (unit length; none where not heard).
    pub track: Option<i64>,
    pub sound: Option<Arc<[f32]>>,
    /// Its artists and album artist (a compilation's left out), for a list's profile.
    pub artists: Vec<String>,
    /// Every artist it credits, by their keys (`app::excluded_artists::credited`: its artists, its artists list and
    /// album artist, each credit split where it joins several, "feat." among them): what the Artist filter and the
    /// artists excluded everywhere are matched against (decision 0083; the owner, 2026-10-30).
    pub credits: Vec<String>,
    /// Each of `credits` as first written, in its order: what the Artist filter offers.
    pub credit_names: Vec<String>,
    /// CLAP's score of each mood, in `similarity::MOODS`' order; none until CLAP heard it.
    pub clap_moods: Option<[f32; 5]>,
    /// Its length (s); lossless or its bitrate's class (`quality_of`); its key, as one spelling (`key_name`).
    pub length: Option<f64>,
    pub quality: Option<String>,
    pub key: Option<String>,
    /// Its album's release types as shown ("EP", "Live"), primary first (0.9.6); empty where not known.
    pub release_types: Vec<String>,
}

/// What a listener did with a line (the owner, 2026-10-23: crates on plays, last played and rating): how often it was
/// played, when last (epoch s), and its album's stars. An account's own: `World::uses`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Use {
    pub plays: u32,
    pub last: Option<i64>,
    pub rating: Option<u8>,
}

/// A file's quality as the crates offer it: lossless, else its bitrate's class.
pub fn quality_of(format: Option<&str>, kbps: Option<i64>) -> Option<String> {
    let lossless = format.is_some_and(|f| ["FLAC", "ALAC", "WAV", "AIFF", "APE", "WAVPACK", "WV"].iter().any(|l| f.eq_ignore_ascii_case(l)));
    Some(match kbps {
        _ if lossless => "Lossless",
        Some(k) if k >= 300 => "320 kbps",
        Some(k) if k >= 240 => "256 kbps",
        Some(k) if k >= 180 => "192 kbps",
        Some(_) => "Under 192 kbps",
        None => return None,
    }
    .to_string())
}

/// A key as one spelling ("D minor", "F# major"), from a tag's ("Dm", "F#maj", "Eb minor") or AudioMuse's; none for
/// what is not a key (Camelot's "8A" and the like).
pub fn key_name(raw: &str) -> Option<String> {
    let t = raw.trim();
    let mut c = t.chars();
    let letter = c.next()?.to_ascii_uppercase();
    if !('A'..='G').contains(&letter) {
        return None;
    }
    let rest: String = c.collect();
    let (accidental, rest) = match rest.chars().next() {
        Some('#' | '♯') => ("#", &rest[rest.chars().next().map_or(0, char::len_utf8)..]),
        // Right after the letter, a "b" is always a flat ("Bb", "Eb minor"); the minor is an "m".
        Some('b' | '♭') => ("b", &rest[rest.chars().next().map_or(0, char::len_utf8)..]),
        _ => ("", rest.as_str()),
    };
    let mode = match rest.trim().to_lowercase().as_str() {
        "" | "maj" | "major" | "ma" => "major",
        "m" | "min" | "minor" | "mi" => "minor",
        _ => return None,
    };
    Some(format!("{letter}{accidental} {mode}"))
}

impl Facts {
    /// What the one similarity knows of the file from its facts (the radio's and the crates' alike: one reading of a
    /// track, the owner's "one profile computation", 2026-10-23): genres, AudioMuse's styles, year, tempo, energy,
    /// danceability and CLAP's moods. `intern` numbers a name of a kind (2 genres, 3 styles; any case). Its playlists,
    /// artists and album tags are the caller's (the radio knows the artists' ids, the crates the playlists' lines).
    pub fn like(&self, mut intern: impl FnMut(usize, &str) -> u32) -> Track {
        Track {
            genres: self.genres.iter().map(|g| intern(2, g)).collect(),
            styles: self.styles.iter().map(|x| intern(3, x)).collect(),
            // The library's words that are tags (0.9.7: the household's album tags among them, with Last.fm's,
            // MusicBrainz's and the files' grouping).
            tags: self.tags.iter().map(|x| intern(4, x)).collect(),
            year: self.year.filter(|y| y.is_finite() && *y > 0.0).map(|y| y as u32),
            bpm: self.bpm.filter(|b| b.is_finite() && *b > 0.0).map(|b| b as f32),
            energy: self.energy.map(|e| (e / 100.0) as f32),
            danceability: self.danceability.map(|d| (d / 100.0) as f32),
            moods: self.clap_moods,
            ..Track::default()
        }
    }
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
    /// What the viewer did with each line (plays, last played, rating); `None`: not known.
    pub uses: Option<&'a HashMap<String, Use>>,
    /// Now (epoch s): Last played and Added are counted in days back from it.
    pub now: i64,
}

const AXES: [(&str, &str, Kind); 21] = [
    ("favourite", "Favourite", Kind::YesNo),
    ("favourite_album", "Favourite album", Kind::YesNo),
    ("playlist", "In playlist", Kind::Membership),
    // The listener's (the owner, 2026-10-23: "a big oversight"): how often and how lately played (days ago; never played
    // is longer ago than any), how lately added (days ago), the album's stars.
    ("plays", "Play count", Kind::Range),
    ("played", "Last played", Kind::Range),
    ("added", "Added", Kind::Range),
    ("rating", "Rating", Kind::Range),
    ("genre", "Genre", Kind::Values),
    ("artist", "Artist", Kind::Values),
    // The album's release type (0.9.6, the owner: hide EPs and singles); an album of no known type has none.
    ("type", "Type", Kind::Values),
    ("year", "Year", Kind::Range),
    ("bpm", "BPM", Kind::Range),
    // Length in minutes; quality (lossless or a bitrate's class); key.
    ("length", "Length", Kind::Range),
    ("quality", "Quality", Kind::Values),
    ("key", "Key", Kind::Values),
    // AudioMuse's (decision 0054; the owner: crates select on all four).
    ("mood", "Mood", Kind::Values),
    ("energy", "Energy", Kind::Range),
    ("danceability", "Danceability", Kind::Range),
    ("style", "Style", Kind::Values),
    // The library's words that are tags (0.9.7).
    ("tag", "Tag", Kind::Values),
    // One similarity's sound (the owner, 2026-10-23); seeded from a track or an album, not added by hand.
    ("like", "Sounds like", Kind::Like),
];

/// Names as the items have them, the most common first (a genre's or a style's, in any case), each with how many items
/// have it among their tracks.
fn by_count<'a>(items: &[&[&'a Facts]], names: fn(&'a Facts) -> &'a [String]) -> Vec<(String, usize)> {
    keyed_count(items, |f| names(f).iter().map(|n| (n.to_lowercase(), n)))
}

/// Each credited artist (`Facts::credits`, by their key), the most common first, as first written, with how many items
/// credit them among their tracks: "Air feat. Beck" counts for Air and for Beck.
fn credits_by_count(items: &[&[&Facts]]) -> Vec<(String, usize)> {
    keyed_count(items, |f| f.credits.iter().cloned().zip(&f.credit_names))
}

/// `by_count` by each name's key (`each`: a file's names, each with its key), each named as first met.
fn keyed_count<'a, I: Iterator<Item = (String, &'a String)>>(items: &[&[&'a Facts]], each: impl Fn(&'a Facts) -> I) -> Vec<(String, usize)> {
    let mut seen: HashMap<String, (String, usize)> = HashMap::new();
    for tracks in items {
        let mut here = HashSet::new();
        for (key, n) in tracks.iter().flat_map(|f| each(*f)) {
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
// The tests' own: nothing else uses it (no warning in the server's build).
#[cfg(test)]
pub fn axes<'a>(files: impl IntoIterator<Item = &'a Facts>, playlists: &[String]) -> Vec<AxisView> {
    let one: Vec<[&Facts; 1]> = files.into_iter().map(|f| [f]).collect();
    let items: Vec<&[&Facts]> = one.iter().map(|f| f.as_slice()).collect();
    list_axes(&items, playlists, None, None)
}

/// The axes over a list in view (decision 0061, the Filter sheet), its items each a group of tracks (a list of tracks:
/// one each): genres, moods and styles most common first with how many items have each among their tracks, the
/// ranges' span over the tracks, and how many items have a value on each axis. `membership` counts the playlists'
/// items too.
pub fn list_axes(items: &[&[&Facts]], playlists: &[String], membership: Option<&HashMap<String, Vec<String>>>, uses: Option<&HashMap<String, Use>>) -> Vec<AxisView> {
    let counted = |names: fn(&Facts) -> &[String]| -> (Vec<String>, Vec<usize>) { by_count(items, names).into_iter().unzip() };
    let tracks = || items.iter().flat_map(|t| t.iter().copied());
    let now = now_secs();
    let used = |f: &Facts| uses.map(|u| u.get(&f.rel).copied().unwrap_or_default());
    // A number axis' value of a file (none where not known; never played has no last play here, only in a test).
    let number = |key: &str, f: &Facts| -> Option<f64> {
        match key {
            "year" => f.year,
            "bpm" => f.bpm,
            "energy" => f.energy,
            "danceability" => f.danceability,
            "length" => f.length.map(minutes),
            "added" => f.born.map(|b| days_ago(now, b)),
            "plays" => used(f).map(|u| f64::from(u.plays)),
            "played" => used(f).and_then(|u| u.last).map(|t| days_ago(now, t)),
            "rating" => used(f).and_then(|u| u.rating).map(f64::from),
            _ => None,
        }
    };
    let span = |key: &str| {
        let vals: Vec<f64> = tracks().filter_map(|f| number(key, f)).collect();
        let min = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
        let max = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        (min, max, items.iter().filter(|t| t.iter().any(|f| number(key, f).is_some())).count())
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
                "artist" => f.credits.join("|"),
                "type" => f.release_types.join("|"),
                "mood" => f.moods.join("|"),
                "style" => f.styles.join("|"),
                "tag" => f.tags.join("|"),
                "quality" => f.quality.clone().unwrap_or_default(),
                "key" => f.key.clone().unwrap_or_default(),
                "played" => format!("{:?}", used(f).map(|u| u.last)),
                key => format!("{:?}", number(key, f)),
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
                // Every artist credited, featured and joint ones too (the owner, 2026-10-30), as the axis matches.
                "artist" => {
                    (a.values, a.counts) = credits_by_count(items).into_iter().unzip();
                    a.known = known(|f| !f.credits.is_empty());
                }
                "type" => {
                    (a.values, a.counts) = counted(|f| &f.release_types);
                    a.known = known(|f| !f.release_types.is_empty());
                }
                "mood" => {
                    (a.values, a.counts) = counted(|f| &f.moods);
                    a.known = known(|f| f.danceability.is_some());
                }
                "style" => {
                    (a.values, a.counts) = counted(|f| &f.styles);
                    a.known = known(|f| !f.styles.is_empty());
                }
                "tag" => {
                    (a.values, a.counts) = counted(|f| &f.tags);
                    a.known = known(|f| !f.tags.is_empty());
                }
                "quality" => {
                    (a.values, a.counts) = counted(|f| f.quality.as_slice());
                    a.known = known(|f| f.quality.is_some());
                }
                "key" => {
                    (a.values, a.counts) = counted(|f| f.key.as_slice());
                    a.known = known(|f| f.key.is_some());
                }
                "like" => a.known = known(|f| f.sound.is_some()),
                _ if kind == Kind::Range => (a.min, a.max, a.known) = span(key),
                _ => {}
            }
            if let (Some(lo), Some(hi)) = (a.min, a.max) {
                a.bins = density(tracks().filter_map(|f| number(key, f)), lo, hi);
            }
            a
        })
        .collect()
}

/// Days from `t` to `now` (epoch s).
fn days_ago(now: i64, t: i64) -> f64 {
    ((now - t).max(0) as f64 / 86_400.0).floor()
}

/// Seconds as minutes, to a tenth.
fn minutes(secs: f64) -> f64 {
    (secs / 6.0).round() / 10.0
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Whether every condition that is on, but Sounds like, holds for a file: the files a Sounds like picks its closest
/// among (`resolved`).
pub fn holds_besides_like(conditions: &[Condition], f: &Facts, w: &World) -> bool {
    conditions.iter().filter(|c| c.enabled && !c.value.ranks()).all(|c| holds(c, f, w))
}

impl Value {
    /// Whether it picks the closest among the files (`resolved`): a Sounds like, a moment.
    pub fn ranks(&self) -> bool {
        matches!(self, Value::Like { .. } | Value::Moment { .. })
    }
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
    let used = || w.uses.map(|u| u.get(&f.rel).copied().unwrap_or_default());
    match (c.axis.as_str(), &c.value) {
        ("plays", Value::Range { min, max }) => within(used().map(|u| f64::from(u.plays)), min, max),
        // Never played: longer ago than any (in "not in the last year", out of "this month").
        ("played", Value::Range { min, max }) => within(used().map(|u| u.last.map_or(f64::INFINITY, |t| days_ago(w.now, t))), min, max),
        ("added", Value::Range { min, max }) => within(f.born.map(|b| days_ago(w.now, b)), min, max),
        ("rating", Value::Range { min, max }) => within(used().and_then(|u| u.rating).map(f64::from), min, max),
        // Any credit, by its key (the owner, 2026-10-30): "Beck" is Air feat. Beck's, Beck & Air's, never Becky Hill's;
        // as the artists excluded everywhere are matched (decision 0083).
        ("artist", Value::AnyOf { values }) => values.iter().any(|v| f.credits.contains(&crate::app::excluded_artists::key(v))),
        ("type", Value::AnyOf { values }) => values.iter().any(|v| f.release_types.iter().any(|t| t.eq_ignore_ascii_case(v))),
        ("length", Value::Range { min, max }) => within(f.length.map(minutes), min, max),
        ("quality", Value::AnyOf { values }) => f.quality.as_ref().is_some_and(|q| values.contains(q)),
        ("key", Value::AnyOf { values }) => f.key.as_ref().is_some_and(|k| values.iter().any(|v| v.eq_ignore_ascii_case(k))),
        ("favourite", Value::Yes) => w.favourites.is_some_and(|s| s.contains(&f.rel)),
        ("favourite_album", Value::Yes) => w.favourite_albums.is_some_and(|s| s.contains(&f.rel)),
        (LEFT_OUT, Value::AnyOf { values }) => values.iter().any(|v| *v == f.rel),
        (CREDITED, Value::AnyOf { values }) => values.iter().any(|v| f.credits.contains(v)),
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
        ("tag", Value::AnyOf { values }) => values.iter().any(|v| f.tags.iter().any(|t| t.eq_ignore_ascii_case(v))),
        ("energy", Value::Range { min, max }) => within(f.energy, min, max),
        ("danceability", Value::Range { min, max }) => within(f.danceability, min, max),
        ("like", Value::Within { lines, .. }) => lines.contains(&f.rel),
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
        "playlist" | LEFT_OUT | CREDITED => true,
        // How it was used, where that is read: every track has a count and a last play (never: longer ago than any).
        // These seven had none (2026-10-29, the Filter sheet's audit): All on them matched nothing, and All negated
        // everything.
        "plays" | "played" => w.uses.is_some(),
        // Stars only where some were given: All goes by the rated tracks, as All on a genre goes by the tagged ones.
        "rating" => w.uses.is_some_and(|u| u.get(&f.rel).is_some_and(|u| u.rating.is_some())),
        "added" => f.born.is_some(),
        "artist" => !f.credits.is_empty(),
        "length" => f.length.is_some(),
        "quality" => f.quality.is_some(),
        "key" => f.key.is_some(),
        "genre" => !f.genres.is_empty(),
        "type" => !f.release_types.is_empty(),
        "year" => f.year.is_some(),
        "bpm" => f.bpm.is_some(),
        // Heard by CLAP, or given moods by the library's words.
        "mood" => f.danceability.is_some() || !f.moods.is_empty(),
        "style" => !f.styles.is_empty(),
        "tag" => !f.tags.is_empty(),
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

/// Whether an item passes the conditions (decision 0061): every one that is on and asks something (a muted one, or
/// one not finished, is ignored), each with its quantifier over the item's tracks. None: every item passes.
pub fn passes(tracks: &[&Facts], conditions: &[Condition], w: &World) -> bool {
    conditions.iter().filter(|c| c.enabled && c.complete()).all(|c| item_holds(c, tracks, w))
}

/// The conditions with each Sounds like worked out once, not for every file: its closest `count` among `among` (the
/// files the crate may show: its other conditions hold for them, so the cut and the count agree; the owner saw 3
/// tracks where the curve was cut at 20), its seeds found in `alike`'s files (they may be anywhere, a library switched
/// off, left out), by the one similarity (`closest_weighted`). Seeds none of which is known: never holds.
// The tests' own: nothing else uses it (no warning in the server's build).
#[cfg(test)]
pub fn resolved(conditions: &[Condition], alike: &Alike, among: &[&Facts], by: &Weights) -> Vec<Condition> {
    resolved_in(conditions, alike, among, by, &HashMap::new())
}

/// `resolved`, with the playlists holding each line (`membership`) for a Sounds like that leans towards tracks in none.
pub fn resolved_in(conditions: &[Condition], alike: &Alike, among: &[&Facts], by: &Weights, membership: &HashMap<String, Vec<String>>) -> Vec<Condition> {
    conditions
        .iter()
        .map(|c| match &c.value {
            Value::Like { of, count, fresh, weights, unlisted, avoid } => {
                let seeds: HashSet<i64> = if *fresh { of.iter().filter_map(|id| crate::serve::ids::track_of(id)).collect() } else { HashSet::new() };
                let mut ranked: Vec<String> = closest_fit(of, weights, avoid, alike, among, by)
                    .into_iter()
                    .filter(|(f, _)| !f.track.is_some_and(|t| seeds.contains(&t)))
                    .take(if *unlisted { count + LEAN_POOL } else { *count })
                    .map(|(f, _)| f.rel.clone())
                    .collect();
                if *unlisted {
                    lean_unlisted(&mut ranked, |l| membership.get(l).is_some_and(|m| !m.is_empty()));
                    ranked.truncate(*count);
                }
                Condition { value: Value::Within { lines: Arc::new(ranked.iter().cloned().collect()), ranked: Arc::new(ranked) }, ..c.clone() }
            }
            Value::Moment { target, count } => {
                let ranked: Vec<String> = nearest(among, &Scales::of(among), target).into_iter().take(*count).map(|(f, _)| f.rel.clone()).collect();
                Condition { value: Value::Within { lines: Arc::new(ranked.iter().cloned().collect()), ranked: Arc::new(ranked) }, ..c.clone() }
            }
            _ => c.clone(),
        })
        .collect()
}

/// The library as the one similarity sees it (`crate::similarity`; the owner, 2026-10-23: Sounds like on it, as the
/// radio and the similar lists): every file's facts as a similarity track (its playlists, artists, genres, AudioMuse's
/// styles, year, tempo, energy, danceability, CLAP's moods and its sound), by the files' places.
pub struct Alike {
    lib: Library,
    tracks: Vec<Track>,
    /// The names by their numbers, per kind (0 playlists, 1 artists, 2 genres, 3 styles, 4 tags), as first spelled.
    names: [Vec<String>; 5],
    /// Each file's place, by its line.
    place: HashMap<String, usize>,
    /// Each file's place, by its track in the catalogue.
    of_track: HashMap<i64, usize>,
}

impl Alike {
    /// From the files and the playlists holding each line.
    pub fn of(all: &[Facts], membership: &HashMap<String, Vec<String>>) -> Self {
        let mut numbers: [HashMap<String, u32>; 5] = Default::default();
        let mut names: [Vec<String>; 5] = Default::default();
        let mut intern = |kind: usize, name: &str| {
            let key = name.trim().to_lowercase();
            let next = numbers[kind].len() as u32;
            *numbers[kind].entry(key).or_insert_with(|| {
                names[kind].push(name.trim().to_string());
                next
            })
        };
        let mut tracks = Vec::with_capacity(all.len());
        let mut sounds = Vec::with_capacity(all.len());
        for f in all {
            let mut t = f.like(|kind, name| intern(kind, name));
            t.playlists = membership.get(&f.rel).map(|ps| ps.iter().map(|p| intern(0, p)).collect()).unwrap_or_default();
            t.artists = f.artists.iter().map(|a| intern(1, a)).collect();
            tracks.push(t);
            sounds.push(f.sound.as_deref().map(<[f32]>::to_vec));
        }
        let lib = Library::new(&tracks, sounds);
        let place = all.iter().enumerate().map(|(i, f)| (f.rel.clone(), i)).collect();
        let of_track = all.iter().enumerate().filter_map(|(i, f)| Some((f.track?, i))).collect();
        Alike { lib, tracks, names, place, of_track }
    }

    /// The profile of places, each weighing its weight (`similarity::Profile::of_weighted`).
    pub fn profile(&self, set: &[(usize, f64)]) -> crate::similarity::Profile {
        crate::similarity::Profile::of_weighted(&self.lib, set, |i| &self.tracks[i])
    }

    /// A word's name: its kind (`Signal::Playlists`, `Artists`, `Genres`, `Styles`, `Tags`) and number.
    pub fn name(&self, kind: crate::similarity::Signal, id: u32) -> Option<&str> {
        use crate::similarity::Signal;
        let k = match kind {
            Signal::Playlists => 0,
            Signal::Artists => 1,
            Signal::Genres => 2,
            Signal::Styles => 3,
            Signal::Tags => 4,
            _ => return None,
        };
        self.names[k].get(id as usize).map(String::as_str)
    }

    /// A file's place, by its line.
    pub fn place_of_line(&self, line: &str) -> Option<usize> {
        self.place.get(line).copied()
    }

    /// A file's place, by its track in the catalogue.
    pub fn place_of_track(&self, track: i64) -> Option<usize> {
        self.of_track.get(&track).copied()
    }

    /// How alike two profiles are, by `by` (the listener's Similar by); none when nothing is known of it.
    pub fn compare(&self, a: &crate::similarity::Profile, b: &crate::similarity::Profile, by: &Weights) -> Option<f64> {
        let signals = a.to_profile(&self.lib, b);
        signals.known().then(|| signals.score(by))
    }

    /// Whether AudioMuse heard the file at a place.
    pub fn heard(&self, i: usize) -> bool {
        self.lib.sound(i).is_some()
    }

    /// Every file as the one similarity sees it, by place.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }
}

/// The files among `among` most like the seeds (song ids, found in `alike`), by the one similarity weighed `by` (the
/// listener's Similar by), closest first, at most `LIKE_MOST`, each with how alike (0 to 100). Each seed counts by its
/// weight (`weights`, by `of`'s order; missing ones 1; none: left out of the seed). Files nothing is known of in common
/// with the seeds are left out; none when no seed is known.
pub fn closest_weighted<'a>(of: &[String], weights: &[f32], alike: &Alike, among: &[&'a Facts], by: &Weights) -> Vec<(&'a Facts, f64)> {
    closest_fit(of, weights, &Avoid::default(), alike, among, by)
}

/// `closest_weighted` as a fit model (the owner, 2026-10-23: "playlist fit models"): each candidate's closeness to the
/// seeds less the share of what it should not be like that it is (`avoid`), as much as there was turned down.
pub fn closest_fit<'a>(of: &[String], weights: &[f32], avoid: &Avoid, alike: &Alike, among: &[&'a Facts], by: &Weights) -> Vec<(&'a Facts, f64)> {
    let mut seed: Vec<usize> = Vec::new();
    for (k, id) in of.iter().enumerate() {
        let Some(&i) = crate::serve::ids::track_of(id).and_then(|t| alike.of_track.get(&t)) else { continue };
        // A weight as how often the track counts: half a listen or more, once; at most ten times.
        let w = weights.get(k).copied().unwrap_or(1.0);
        let times = if w <= 0.0 { 0 } else { ((w * 2.0).round() as usize).clamp(1, 10) };
        seed.extend(std::iter::repeat_n(i, times));
    }
    if seed.is_empty() {
        return Vec::new();
    }
    // A varied seed is several sounds (the owner, 2026-10-23: a week's styles kept apart, not averaged into what lies
    // between): each group its own profile, and the picks taken from the groups by their share of the seed, the
    // closest of each first. One group: closest first, as one profile.
    let groups = sound_groups(&seed, &alike.lib);
    let mut best: HashMap<usize, (f64, f64)> = HashMap::new();
    for group in &groups {
        let share = group.len() as f64 / seed.len() as f64;
        let profile = crate::similarity::Profile::of_counted(&alike.lib, group, |i| &alike.tracks[i]);
        let mut scored: Vec<(usize, f64)> = among
            .iter()
            .enumerate()
            .filter_map(|(k, f)| {
                let &i = alike.place.get(&f.rel)?;
                let signals = profile.to_track(&alike.lib, i, &alike.tracks[i]);
                signals.known().then(|| (k, signals.score(by) - avoid.against(f)))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| among[a.0].rel.cmp(&among[b.0].rel)));
        for (rank, (k, x)) in scored.into_iter().enumerate() {
            // Its turn among the picks: its place in the group's order, a large group's coming round more often.
            let turn = (rank + 1) as f64 / share;
            let e = best.entry(k).or_insert((f64::INFINITY, 0.0));
            if turn < e.0 {
                *e = (turn, x);
            }
        }
    }
    let mut picked: Vec<(usize, f64, f64)> = best.into_iter().map(|(k, (turn, x))| (k, turn, x)).collect();
    picked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| among[a.0].rel.cmp(&among[b.0].rel)));
    picked.truncate(LIKE_MOST);
    // How alike, as the curve draws it: the n-th pick at least this alike to its group (never rising down the list).
    let mut floor = f64::INFINITY;
    picked
        .into_iter()
        .map(|(k, _, x)| {
            floor = floor.min(x);
            (among[k], (floor * 1000.0).round() / 10.0)
        })
        .collect()
}

/// A seed of fewer tracks AudioMuse heard than this is one sound (a track, an album).
const GROUP_FROM: usize = 12;
/// The most groups a seed's sounds fall into.
const GROUPS: usize = 6;
/// Groups whose middles sound at least this alike (cosine) are one: a week of one style stays one group.
const SAME_GROUP: f32 = 0.85;

/// The seed's places (a place as often as it counts) in groups by sound: spherical k-means over the heard ones, started
/// from the heaviest and then the farthest, groups that sound alike joined. The places AudioMuse has not heard go with
/// the largest group.
fn sound_groups(seed: &[usize], lib: &Library) -> Vec<Vec<usize>> {
    let mut weight: HashMap<usize, f32> = HashMap::new();
    for &i in seed {
        *weight.entry(i).or_default() += 1.0;
    }
    let mut heard: Vec<(usize, &[f32], f32)> = weight.iter().filter_map(|(&i, &w)| Some((i, lib.sound(i)?, w))).collect();
    heard.sort_by_key(|h| h.0);
    if heard.len() < GROUP_FROM {
        return vec![seed.to_vec()];
    }
    let dims = heard[0].1.len();
    let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let heaviest = (0..heard.len()).max_by(|&a, &b| heard[a].2.total_cmp(&heard[b].2).then(heard[b].0.cmp(&heard[a].0))).unwrap_or(0);
    let mut centres: Vec<Vec<f32>> = vec![heard[heaviest].1.to_vec()];
    while centres.len() < GROUPS {
        let far = (0..heard.len())
            .map(|k| (k, centres.iter().map(|c| cos(heard[k].1, c)).fold(f32::MIN, f32::max)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, near)| *near < SAME_GROUP);
        let Some((k, _)) = far else { break };
        centres.push(heard[k].1.to_vec());
    }
    let mut of = vec![0usize; heard.len()];
    for _ in 0..10 {
        for (k, h) in heard.iter().enumerate() {
            of[k] = (0..centres.len()).max_by(|&a, &b| cos(h.1, &centres[a]).total_cmp(&cos(h.1, &centres[b]))).unwrap_or(0);
        }
        let mut sums = vec![vec![0.0f32; dims]; centres.len()];
        for (k, h) in heard.iter().enumerate() {
            for (s, x) in sums[of[k]].iter_mut().zip(h.1) {
                *s += h.2 * x;
            }
        }
        for (c, s) in centres.iter_mut().zip(sums) {
            let len = s.iter().map(|x| x * x).sum::<f32>().sqrt();
            if len > 0.0 {
                *c = s.into_iter().map(|x| x / len).collect();
            }
        }
        // Groups that came to sound alike are one.
        if let Some((a, b)) = (0..centres.len()).flat_map(|a| (a + 1..centres.len()).map(move |b| (a, b))).find(|&(a, b)| cos(&centres[a], &centres[b]) >= SAME_GROUP) {
            centres.remove(b);
            for g in of.iter_mut() {
                if *g == b {
                    *g = a;
                } else if *g > b {
                    *g -= 1;
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); centres.len()];
    let group_of: HashMap<usize, usize> = heard.iter().zip(&of).map(|(h, &g)| (h.0, g)).collect();
    let mut unheard = Vec::new();
    for &i in seed {
        match group_of.get(&i) {
            Some(&g) => groups[g].push(i),
            None => unheard.push(i),
        }
    }
    groups.retain(|g| !g.is_empty());
    if let Some(largest) = groups.iter_mut().max_by_key(|g| g.len()) {
        largest.extend(unheard);
    }
    groups
}

/// A Sounds like crate's own order, closest first (the owner, 2026-10-23; it was newest first, as every crate's):
/// `lines` (selected) in the order of its first Sounds like that is on, as `resolved` ranked it. Others as they were.
pub fn closest_first(lines: &mut [String], resolved: &[Condition]) {
    let ranked = resolved.iter().find_map(|c| match &c.value {
        Value::Within { ranked, .. } if c.enabled && !c.not => Some(ranked),
        _ => None,
    });
    let Some(ranked) = ranked else { return };
    let rank: HashMap<&str, usize> = ranked.iter().enumerate().map(|(i, l)| (l.as_str(), i)).collect();
    lines.sort_by_key(|l| rank.get(l.as_str()).copied().unwrap_or(usize::MAX));
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

/// A crate's conditions with the artists excluded everywhere left out (decision 0083): a file crediting one of these
/// (their keys, `Facts::credits`) is not in it, whatever else holds. Not an axis one builds with: every crate carries
/// it unseen, as its left-out list.
pub fn leaving_out_artists(keys: &HashSet<String>, conditions: &[Condition]) -> Vec<Condition> {
    let mut all = conditions.to_vec();
    if !keys.is_empty() {
        let mut values: Vec<String> = keys.iter().cloned().collect();
        values.sort();
        all.push(Condition::new(CREDITED, true, Value::AnyOf { values }));
    }
    all
}

/// The artists excluded everywhere's condition: a file crediting one of these keys.
const CREDITED: &str = "credited";

/// Whether a file credits one of the artists excluded everywhere (their keys): what a pick that is not a crate leaves
/// out (decision 0083).
pub fn credits_out(f: &Facts, keys: &HashSet<String>) -> bool {
    !keys.is_empty() && f.credits.iter().any(|k| keys.contains(k))
}

/// Not in this crate (decision 0083, the owner 2026-10-30): the crate's conditions with "Artist is not `name`", as one
/// would set it by hand: the name added to an "Artist is not" row that is on (of any track), else a row of its own.
/// `None` when the artist is left out so already (any spelling of their key).
pub fn with_not_artist(conditions: &[Condition], name: &str) -> Option<Vec<Condition>> {
    let name = name.trim();
    let key = crate::app::excluded_artists::key(name);
    if key.is_empty() || not_artists(conditions).iter().any(|(k, _)| *k == key) {
        return None;
    }
    let mut all = conditions.to_vec();
    match all.iter().position(|c| is_not_artist(c) && c.enabled && c.quantifier == Quantifier::Any) {
        Some(i) => {
            if let Value::AnyOf { values } = &mut all[i].value {
                values.push(name.to_string());
            }
        }
        None => all.push(Condition::new("artist", true, Value::AnyOf { values: vec![name.to_string()] })),
    }
    Some(all)
}

/// The conditions without the artist of this key in their "Artist is not" rows (a row left empty goes); `None` when no
/// row names them.
pub fn without_not_artist(conditions: &[Condition], key: &str) -> Option<Vec<Condition>> {
    let mut all = conditions.to_vec();
    let mut found = false;
    for c in all.iter_mut().filter(|c| is_not_artist(c)) {
        if let Value::AnyOf { values } = &mut c.value {
            let before = values.len();
            values.retain(|v| crate::app::excluded_artists::key(v) != key);
            found |= values.len() != before;
        }
    }
    all.retain(|c| !is_not_artist(c) || matches!(&c.value, Value::AnyOf { values } if !values.is_empty()));
    found.then_some(all)
}

/// The artists a crate's "Artist is not" rows that are on leave out: each one's key and name, as the crate lists them.
pub fn not_artists(conditions: &[Condition]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for c in conditions.iter().filter(|c| is_not_artist(c) && c.enabled) {
        if let Value::AnyOf { values } = &c.value {
            for v in values {
                let k = crate::app::excluded_artists::key(v);
                if !k.is_empty() && !out.iter().any(|(x, _)| *x == k) {
                    out.push((k, v.clone()));
                }
            }
        }
    }
    out
}

/// An "Artist is not" row: the artist axis, negated, by its values.
fn is_not_artist(c: &Condition) -> bool {
    c.axis == "artist" && c.not && matches!(c.value, Value::AnyOf { .. })
}

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
    /// Each ranked crate's fit (its rules and the learned traits dismissed), by its id without a count.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fits: BTreeMap<String, Fit>,
    /// The conditions the account added to a fixed crate (decision 0083: Not in this crate), by `kept_id`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    added: BTreeMap<String, Vec<Condition>>,
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

    /// A ranked crate's fit (none until set).
    pub async fn fit(&self, id: &str) -> Result<Fit> {
        Ok(self.stored().await?.fits.get(id).cloned().unwrap_or_default())
    }

    /// Every ranked crate's fit that holds anything, by its id.
    pub async fn all_fits(&self) -> Result<BTreeMap<String, Fit>> {
        Ok(self.stored().await?.fits)
    }

    pub async fn set_fit(&self, id: &str, fit: Fit) -> Result<()> {
        let mut s = self.stored().await?;
        if fit.is_empty() {
            s.fits.remove(id);
        } else {
            s.fits.insert(id.to_string(), fit);
        }
        self.write_stored(s).await
    }

    /// The conditions added to a fixed crate (decision 0083), by `kept_id` (none until some are).
    pub async fn added(&self, id: &str) -> Result<Vec<Condition>> {
        Ok(self.stored().await?.added.get(id).cloned().unwrap_or_default())
    }

    pub async fn set_added(&self, id: &str, conditions: Vec<Condition>) -> Result<()> {
        let mut s = self.stored().await?;
        if conditions.is_empty() {
            s.added.remove(id);
        } else {
            s.added.insert(id.to_string(), conditions);
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
        // A Bandcamp preview seeds no crate kept (decision 0071): it is not Cratarr's to keep.
        if conditions.iter().any(|c| matches!(&c.value, Value::Like { of, .. } if of.iter().any(|s| crate::bandcamp::is_preview(s)))) {
            return Err(crate::bandcamp::NOT_KEPT.into());
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
    fn a_sounds_like_crate_is_closest_first_as_made() {
        let ranked: Vec<String> = ["b", "c", "a"].map(String::from).to_vec();
        let like = Condition::new("like", false, Value::Within { lines: Arc::new(ranked.iter().cloned().collect()), ranked: Arc::new(ranked) });
        let mut lines: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        closest_first(&mut lines, std::slice::from_ref(&like));
        assert_eq!(lines, ["b", "c", "a"]);
        // Without one (or with it muted): newest first, as given.
        let mut lines: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        closest_first(&mut lines, &[Condition { enabled: false, ..like }]);
        assert_eq!(lines, ["a", "b", "c"]);
    }

    #[test]
    fn a_genre_is_a_crate_of_its_one_condition() {
        let c = genre_crate("genre:Trip Hop").unwrap();
        assert_eq!(c.name, "Trip Hop");
        assert_eq!(c.conditions, vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Trip Hop".into()] })]);
        assert!(genre_crate("genre: ").is_none());
        // Home's shelves: exactly their lines, in their order (the owner, 2026-10-24).
        assert!(home_crate("home:new") && home_crate("home:graves") && !home_crate("home:") && !home_crate("like:week"));
        let shelf = lines_crate(HOME_NEW, "New for you", vec!["b/1.flac".into(), "a/1.flac".into()]);
        assert_eq!(shelf.name, "New for you");
        assert!(matches!(&shelf.conditions[0].value, Value::Within { ranked, .. } if **ranked == ["b/1.flac", "a/1.flac"]));
        assert!(genre_crate("unlisted").is_none());
        let style = genre_crate("style:Shoegaze").unwrap();
        assert_eq!((style.name.as_str(), style.conditions[0].axis.as_str()), ("Shoegaze", "style"), "a word's crate, by its kind");
        assert!(genre_crate("tempo:fast").is_none(), "no other kind");
    }

    #[test]
    fn keys_and_qualities_read_as_one_spelling() {
        assert_eq!(key_name("Dm").as_deref(), Some("D minor"));
        assert_eq!(key_name("Bb major").as_deref(), Some("Bb major"));
        assert_eq!(key_name("f#").as_deref(), Some("F# major"));
        assert_eq!(key_name("E♭ min").as_deref(), Some("Eb minor"));
        assert_eq!(key_name("8A"), None, "Camelot is no key name");
        assert_eq!(quality_of(Some("FLAC"), Some(900)).as_deref(), Some("Lossless"));
        assert_eq!(quality_of(Some("MP3"), Some(320)).as_deref(), Some("320 kbps"));
        assert_eq!(quality_of(Some("MP3"), Some(128)).as_deref(), Some("Under 192 kbps"));
        assert_eq!(quality_of(Some("MP3"), None), None);
    }

    #[test]
    fn the_listeners_conditions_test_plays_last_play_added_rating_and_the_tracks_facts() {
        let day = 86_400;
        let now = 1000 * day;
        let f = |rel: &str, born: i64, artist: &str, secs: f64| Facts { born: Some(born), length: Some(secs), ..by(rel, &[artist]) };
        let files = vec![f("old.flac", now - 400 * day, "Ana", 180.0), f("new.flac", now - 3 * day, "Bo", 420.0), f("never.flac", now - 50 * day, "Ana", 240.0)];
        let uses = HashMap::from([
            ("old.flac".to_string(), Use { plays: 12, last: Some(now - 200 * day), rating: Some(5) }),
            ("new.flac".to_string(), Use { plays: 1, last: Some(now - day), rating: None }),
        ]);
        let membership = HashMap::new();
        let w = World { membership: &membership, favourites: None, favourite_albums: None, uses: Some(&uses), now };
        let pick = |axis: &str, value: Value| {
            let c = vec![Condition::new(axis, false, value)];
            files.iter().filter(|x| holds_besides_like(&c, x, &w)).map(|x| x.rel.as_str()).collect::<Vec<_>>()
        };
        let range = |min: Option<f64>, max: Option<f64>| Value::Range { min, max };
        assert_eq!(pick("plays", range(Some(10.0), None)), ["old.flac"]);
        assert_eq!(pick("plays", range(None, Some(0.0))), ["never.flac"], "never played: none");
        assert_eq!(pick("played", range(Some(180.0), None)), ["old.flac", "never.flac"], "not in half a year: never played too");
        assert_eq!(pick("played", range(None, Some(7.0))), ["new.flac"]);
        assert_eq!(pick("added", range(None, Some(30.0))), ["new.flac"]);
        assert_eq!(pick("rating", range(Some(4.0), None)), ["old.flac"]);
        assert_eq!(pick("artist", Value::AnyOf { values: vec!["ana".into()] }), ["old.flac", "never.flac"]);
        assert_eq!(pick("length", range(Some(5.0), None)), ["new.flac"], "minutes");
        let unknown = World { uses: None, ..w };
        let c = vec![Condition::new("plays", false, range(None, Some(0.0)))];
        assert!(files.iter().all(|x| !holds_besides_like(&c, x, &unknown)), "not known: none");
        let one: Vec<[&Facts; 1]> = files.iter().map(|f| [f]).collect();
        let items: Vec<&[&Facts]> = one.iter().map(|f| f.as_slice()).collect();
        let axes = list_axes(&items, &[], None, Some(&uses));
        let plays = axes.iter().find(|a| a.key == "plays").unwrap();
        assert_eq!((plays.min, plays.max), (Some(0.0), Some(12.0)));
        assert_eq!(axes.iter().find(|a| a.key == "artist").unwrap().values, ["Ana", "Bo"]);
    }

    #[test]
    fn a_varied_seed_keeps_its_sounds_apart_and_does_not_pick_what_lies_between() {
        let heard = |rel: &str, track: i64, v: [f32; 2]| Facts { rel: rel.into(), track: Some(track), sound: Some(Arc::from(v.as_slice())), ..Facts::default() };
        // A week of two sounds: eight tracks of one, six of the other.
        let mut files: Vec<Facts> = (1..=8).map(|t| heard(&format!("a{t}.flac"), t, [1.0, 0.0])).collect();
        files.extend((9..=14).map(|t| heard(&format!("b{t}.flac"), t, [0.0, 1.0])));
        files.extend([heard("near-a.flac", 20, [0.99, 0.141]), heard("near-b.flac", 21, [0.141, 0.99]), heard("between.flac", 22, [0.707, 0.707])]);
        let alike = Alike::of(&files, &HashMap::new());
        let among: Vec<&Facts> = files.iter().filter(|f| f.track.is_some_and(|t| t >= 20)).collect();
        let of: Vec<String> = (1..=14).map(|t| format!("tr-{t}")).collect();
        let found: Vec<&str> = closest_weighted(&of, &[], &alike, &among, &Weights::EQUAL).into_iter().map(|(f, _)| f.rel.as_str()).collect();
        assert_eq!(found, ["near-a.flac", "near-b.flac", "between.flac"], "each sound's closest first, the larger's first; the middle last");
        // One sound (an album's tracks): one group, closest first as before.
        let one: Vec<String> = (1..=8).map(|t| format!("tr-{t}")).collect();
        let found: Vec<&str> = closest_weighted(&one, &[], &alike, &among, &Weights::EQUAL).into_iter().map(|(f, _)| f.rel.as_str()).collect();
        assert_eq!(found, ["near-a.flac", "between.flac", "near-b.flac"]);
    }

    #[test]
    fn your_week_sounds_like_what_was_played_but_offers_none_of_it_again() {
        let heard = |rel: &str, track: i64, v: [f32; 2]| Facts { rel: rel.into(), track: Some(track), sound: Some(Arc::from(v.as_slice())), ..Facts::default() };
        let files = vec![heard("played.flac", 1, [1.0, 0.0]), heard("near.flac", 2, [0.999, 0.045]), heard("far.flac", 3, [0.0, 1.0])];
        let week = week_crate("like:week@10", vec![("tr-1".into(), 1.0)], 10);
        assert_eq!(week.name, "Sounds like your week");
        let membership = HashMap::new();
        let w = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0 };
        let every: Vec<&Facts> = files.iter().collect();
        let mut lines: Vec<String> = select(&files, &resolved(&week.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w).into_iter().map(String::from).collect();
        closest_first(&mut lines, &resolved(&week.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL));
        assert_eq!(lines, ["near.flac", "far.flac"], "closest first, the played one left out");
        assert_eq!(like_of("like:week@10"), Some((WEEK, 10)));
    }

    #[test]
    fn what_was_turned_down_teaches_traits_that_sink_what_has_them() {
        let heard = |rel: &str, track: i64, v: [f32; 2], artist: &str, bpm: f64| Facts {
            rel: rel.into(),
            track: Some(track),
            sound: Some(Arc::from(v.as_slice())),
            artists: vec![artist.into()],
            bpm: Some(bpm),
            ..Facts::default()
        };
        let files = vec![
            heard("seed.flac", 1, [1.0, 0.0], "Ana", 90.0),
            heard("a-by-the-turned-down.flac", 2, [0.99, 0.14], "Bo", 124.0),
            heard("b-other.flac", 3, [0.99, -0.14], "Cy", 56.0),
            heard("down-1.flac", 4, [0.0, 1.0], "Bo", 122.0),
            heard("down-2.flac", 5, [0.0, 1.0], "Bo", 126.0),
            heard("down-3.flac", 6, [0.0, 1.0], "Di", 125.0),
        ];
        let every: Vec<&Facts> = files.iter().collect();
        let down: Vec<&Facts> = files[3..].iter().collect();
        let traits = learned(&down, &every);
        assert!(traits.contains(&Trait::Artist { name: "Bo".into() }), "an artist on two of them: {traits:?}");
        assert!(traits.iter().any(|t| matches!(t, Trait::Tempo { lo, hi } if *lo <= 124.0 && *hi >= 124.0)), "a narrow tempo band: {traits:?}");
        assert!(learned(&down[..1], &every).is_empty(), "one turned down teaches nothing");
        let alike = Alike::of(&files, &HashMap::new());
        let among: Vec<&Facts> = files[1..3].iter().collect();
        let order = |avoid: &Avoid| closest_fit(&["tr-1".into()], &[], avoid, &alike, &among, &Weights::EQUAL).into_iter().map(|(f, _)| f.rel.clone()).collect::<Vec<_>>();
        assert_eq!(order(&Avoid::default()), ["a-by-the-turned-down.flac", "b-other.flac"], "alike, by name");
        assert_eq!(order(&Avoid::of(traits.clone(), 3)), ["b-other.flac", "a-by-the-turned-down.flac"], "what has their traits sinks");
        let kept: Vec<Trait> = traits.into_iter().filter(|t| t.key() != "artist:bo" && t.key() != "tempo").collect();
        assert_eq!(order(&Avoid::of(kept, 3)), ["a-by-the-turned-down.flac", "b-other.flac"], "dismissed, they no longer count");
    }

    #[test]
    fn a_playlists_suggestions_lean_towards_tracks_in_no_playlist() {
        let mut ranked: Vec<String> = ["a", "b", "c", "d", "u1", "e", "u2"].map(String::from).to_vec();
        lean_unlisted(&mut ranked, |l| !l.starts_with('u'));
        assert_eq!(ranked, ["a", "u1", "b", "u2", "c", "d", "e"], "each unlisted passes at most three listed");
        let list = playlist_crate("like:list:Chill", "Chill", vec!["tr-1".into()], 10);
        assert_eq!(list.name, "Sounds like Chill");
        assert!(matches!(&list.conditions[0].value, Value::Like { fresh: true, unlisted: true, .. }), "its own left out, leaning");
        assert_eq!(like_of("like:list:Chill"), Some(("list:Chill", LIKE_COUNT)));
    }

    #[test]
    fn a_moment_holds_the_tracks_nearest_its_energy_and_moods() {
        // Energy 0 to 100; CLAP's happy, sad, relaxed (aggressive, party unused).
        let heard = |rel: &str, energy: f64, happy: f32, sad: f32| Facts {
            rel: rel.into(),
            energy: Some(energy),
            clap_moods: Some([happy, sad, 0.3, 0.1, 0.1]),
            ..Facts::default()
        };
        let mut files: Vec<Facts> = (0..10).map(|n| heard(&format!("mid{n}.flac"), 50.0, 0.4, 0.4)).collect();
        files.extend([
            heard("calm-sad.flac", 10.0, 0.1, 0.8),
            heard("calm-happy.flac", 10.0, 0.8, 0.1),
            heard("lively-happy.flac", 90.0, 0.8, 0.1),
            Facts { rel: "unheard.flac".into(), ..Facts::default() },
        ]);
        let every: Vec<&Facts> = files.iter().collect();
        let membership = HashMap::new();
        let w = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0 };
        let pick = |target: Target| {
            let c = moment_crate("like:now@1", "Now", target, 1);
            select(&files, &resolved(&c.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w)
        };
        assert_eq!(pick(Target { energy: 0.0, mood: Some([-0.8, 0.7, 0.6]) }), ["calm-sad.flac"], "a rainy night");
        assert_eq!(pick(Target { energy: 0.0, mood: Some([0.8, -0.7, 0.0]) }), ["calm-happy.flac"], "a clear night");
        assert_eq!(pick(Target { energy: 1.0, mood: Some([0.8, -0.7, 0.0]) }), ["lively-happy.flac"], "a sunny noon");
        assert_eq!(pick(Target { energy: 1.0, mood: None }), ["lively-happy.flac"], "no weather: energy alone");
        let c = moment_crate("like:now", "Now", Target { energy: 0.5, mood: None }, 100);
        assert_eq!(select(&files, &resolved(&c.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w).len(), 13, "never the unheard");
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
        let w = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0 };
        let every: Vec<&Facts> = files.iter().collect();
        assert_eq!(select(&files, &resolved(&like.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w), ["seed.flac", "near.flac"], "the closest two");
        let all = like_crate("like:tr-1", "Seed", vec!["tr-1".into()], 10);
        assert_eq!(select(&files, &resolved(&all.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w), ["seed.flac", "near.flac", "far.flac"], "every heard one");
        // The seed left out of what the crate may show: the closest two are then the next two, still two.
        let shown: Vec<&Facts> = files.iter().filter(|f| f.rel != "seed.flac").collect();
        assert_eq!(select(&files, &resolved(&like.conditions, &Alike::of(&files, &HashMap::new()), &shown, &Weights::EQUAL), &w), ["near.flac", "far.flac"]);
        assert_eq!(like_of("like:tr-1@2"), Some(("tr-1", LIKE_FEWEST)));
        let steps = like_steps(1000);
        assert_eq!((steps[0], steps[1], steps.iter().rev().nth(1).copied(), steps.last().copied()), (10, 15, Some(950), Some(1000)));
        assert!(steps.contains(&95) && steps.contains(&100) && steps.contains(&390) && steps.contains(&400) && !steps.contains(&405));
        assert_eq!(like_steps(23), [10, 15, 20], "no further than there are tracks");
        assert_eq!(like_of("like:al-x"), Some(("al-x", LIKE_COUNT)));
        assert_eq!(like_of("like:al-x@5000"), Some(("al-x", LIKE_MOST)));
        // Unworked out, or seeds never heard: nothing.
        assert!(select(&files, &like.conditions, &w).is_empty());
        let unheard = like_crate("like:tr-9", "Unheard", vec!["tr-9".into()], 10);
        assert!(select(&files, &resolved(&unheard.conditions, &Alike::of(&files, &HashMap::new()), &every, &Weights::EQUAL), &w).is_empty());
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
        let axes = list_axes(&slices, &[], None, None);
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
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        assert_eq!(permanent().iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["unlisted"]);
        let unlisted = &permanent()[0].conditions;
        assert_eq!(select(&files, unlisted, &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::All.with(unlisted), &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::Only.with(unlisted), &w), vec!["b"], "only its favourites");
        assert_eq!(select(&files, &Favourites::None.with(unlisted), &w), vec!["d"], "none of them");
        assert_eq!(select(&files, &Favourites::Only.with(&[]), &w), vec!["a", "b"], "the whole library's favourites");
        let unknown = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0 };
        assert!(select(&files, &Favourites::Only.with(unlisted), &unknown).is_empty(), "favourites not known: nothing, not everything");
    }

    #[test]
    fn the_favourite_albums_toggle_adds_its_own_condition_beside_the_favourites_one() {
        let (files, membership, favourites) = library();
        // "a" and "c" are on a favourite album; "a" and "b" are favourite tracks.
        let albums = HashSet::from(["a".to_string(), "c".to_string()]);
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: Some(&albums), uses: None, now: 0 };
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
        let unknown = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
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
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
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
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        let left = vec!["a".to_string()];
        // "a" is a favourite: left out, it stays out with only favourites shown.
        assert_eq!(select(&files, &leaving_out(&left, &Favourites::Only.with(&[])), &w), vec!["b"]);
        assert_eq!(select(&files, &leaving_out(&left, &[]), &w), vec!["b", "c", "d"]);
        assert_eq!(leaving_out(&[], &[]), Vec::new(), "nothing left out, nothing added");
        assert!(!is_axis("line"), "not an axis one builds with");
    }

    /// A file by these artists (its track's and its album's), credited as the catalogue credits it.
    fn by(rel: &str, artists: &[&str]) -> Facts {
        let (credits, credit_names) = crate::app::excluded_artists::credited(artists);
        Facts { rel: rel.into(), genres: vec!["Electronic".into()], artists: artists.iter().map(|a| a.to_string()).collect(), credits, credit_names, ..Facts::default() }
    }

    #[test]
    fn not_in_this_crate_is_one_artist_is_not_row_added_once_and_leaves_them_out_of_that_crate_only() {
        // Decision 0083: the crate's own artist filter, as set by hand.
        let files = [by("a", &["Air"]), by("b", &["Bonobo"]), by("c", &["Caribou"])];
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        let electronic = vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Electronic".into()] })];
        let one = with_not_artist(&electronic, "Air").unwrap();
        assert_eq!(one[1], Condition::new("artist", true, Value::AnyOf { values: vec!["Air".into()] }));
        assert_eq!(with_not_artist(&one, "AIR"), None, "once, in any spelling of their key");
        assert_eq!(select(&files, &one, &w), vec!["b", "c"]);
        assert_eq!(select(&files, &electronic, &w), vec!["a", "b", "c"], "another crate keeps them");
        // A second artist joins the row; taking one back leaves the other, the last back drops the row.
        let two = with_not_artist(&one, "Bonobo").unwrap();
        assert_eq!((two.len(), not_artists(&two)), (2, vec![("air".to_string(), "Air".to_string()), ("bonobo".to_string(), "Bonobo".to_string())]));
        assert_eq!(select(&files, &two, &w), vec!["c"]);
        let back = without_not_artist(&two, "air").unwrap();
        assert_eq!(select(&files, &back, &w), vec!["a", "c"]);
        assert_eq!(without_not_artist(&back, "bonobo").unwrap(), electronic, "taken back, as it was");
        assert_eq!(without_not_artist(&electronic, "air"), None, "nothing to take back");
    }

    #[test]
    fn the_artist_filter_counts_every_credit_by_whole_names() {
        // The owner, 2026-10-30: featured and joint credits count, everywhere the Artist filter is used.
        let files = [
            by("feat", &["Air feat. Beck"]),
            by("and", &["Beck & Air"]),
            by("comma", &["Beck, Air"]),
            by("becky", &["Becky Hill"]),
            by("acdc", &["AC/DC"]),
            by("air", &["Air"]),
        ];
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        let artist = |not: bool, name: &str| vec![Condition::new("artist", not, Value::AnyOf { values: vec![name.to_string()] })];
        assert_eq!(select(&files, &artist(false, "Beck"), &w), vec!["feat", "and", "comma"], "a whole name, never Becky Hill");
        assert_eq!(select(&files, &artist(true, "Beck"), &w), vec!["becky", "acdc", "air"], "is not: featured and joint ones out");
        assert_eq!(select(&files, &artist(false, "AC/DC"), &w), vec!["acdc"]);
        assert!(select(&files, &artist(false, "DC"), &w).is_empty(), "a slash inside a name is the name");
        // Any and All over an album: a featured artist is a value its track has.
        let album = [&files[0], &files[5]];
        let all = Condition { quantifier: Quantifier::All, ..artist(false, "Air")[0].clone() };
        assert!(passes(&album, &[all], &w), "Air on both tracks, one featuring Beck");
        assert!(passes(&album, &artist(false, "Beck"), &w));
        // The values offered, each credited artist once as first written, counted where credited.
        let one: Vec<[&Facts; 1]> = files.iter().map(|f| [f]).collect();
        let items: Vec<&[&Facts]> = one.iter().map(|f| f.as_slice()).collect();
        let axes = list_axes(&items, &[], None, None);
        let a = axes.iter().find(|a| a.key == "artist").unwrap();
        let count = |name: &str| a.values.iter().position(|v| v == name).map(|i| a.counts[i]);
        assert_eq!((count("Beck"), count("Air")), (Some(3), Some(4)), "Beck for his featured track too: {:?}", a.values);
        assert_eq!((count("AC/DC"), count("Becky Hill"), count("Air feat. Beck")), (Some(1), Some(1), None));
        assert!(count("AC").is_none() && count("DC").is_none());
        assert_eq!(a.known, 6);
    }

    #[test]
    fn a_saved_whole_name_artist_row_keeps_its_tracks_where_no_credit_is_joint() {
        // The stored row is as it was; without featured or joint credits it picks what matching whole tags picked.
        let files = [by("a", &["Air"]), by("b", &["air", "Moon Safari Band"]), by("c", &["The Avalanches"]), by("d", &["Bonobo"]), by("e", &["Various Artists"])];
        let (membership, favourites) = (HashMap::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        for saved in [
            r#"{"axis":"artist","not":true,"value":{"kind":"any_of","values":["Air"]}}"#,
            r#"{"axis":"artist","not":false,"value":{"kind":"any_of","values":["AIR","The Avalanches"]}}"#,
            r#"{"axis":"artist","not":false,"value":{"kind":"any_of","values":["Bonobo"]}}"#,
        ] {
            let c: Condition = serde_json::from_str(saved).unwrap();
            let Value::AnyOf { values } = &c.value else { unreachable!() };
            let whole = |f: &Facts| values.iter().any(|v| f.artists.iter().any(|a| a.eq_ignore_ascii_case(v))) != c.not;
            let before: Vec<&str> = files.iter().filter(|&f| whole(f)).map(|f| f.rel.as_str()).collect();
            assert_eq!(select(&files, std::slice::from_ref(&c), &w), before, "{saved}");
        }
    }

    #[test]
    fn a_fixed_crate_is_kept_by_its_id_without_a_count() {
        assert!(is_fixed("unlisted") && is_fixed("genre:Jazz") && is_fixed("home:new") && is_fixed("like:week") && is_fixed("like:al-1@50"));
        assert!(!is_fixed("c18f0a") && !is_fixed("genre:"));
        assert_eq!((kept_id("like:al-1@50"), kept_id("like:al-1"), kept_id("home:graves")), ("like:al-1".to_string(), "like:al-1".to_string(), "home:graves".to_string()));
    }

    #[test]
    fn an_artist_excluded_everywhere_is_out_of_every_crate_by_any_credit() {
        // Decision 0083: featured, a collaboration, the album artist; a crate's own conditions, a fixed one's, both
        // toggles and a Sounds like's pool.
        let files = [by("a", &["Air feat. Beck"]), by("b", &["Bonobo", "Beck"]), by("c", &["Caribou & Beck"]), by("d", &["Daft Punk"]), by("e", &["Beckett"])];
        let (membership, favourites) = (HashMap::new(), HashSet::from(["a".to_string(), "d".to_string()]));
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
        let out = HashSet::from([crate::app::excluded_artists::key("Beck")]);
        let electronic = vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Electronic".into()] })];
        assert_eq!(select(&files, &leaving_out_artists(&out, &electronic), &w), vec!["d", "e"], "a whole name, not a part of one");
        assert_eq!(select(&files, &leaving_out_artists(&out, &Favourites::Only.with(&electronic)), &w), vec!["d"]);
        assert_eq!(select(&files, &leaving_out_artists(&out, &permanent()[0].conditions), &w), vec!["d", "e"], "Unlisted, a fixed crate");
        assert!(files.iter().filter(|f| credits_out(f, &out)).map(|f| f.rel.as_str()).eq(["a", "b", "c"]));
        assert_eq!(leaving_out_artists(&HashSet::new(), &electronic), electronic, "none excluded, nothing added");
        assert!(!is_axis("credited"), "not an axis one builds with");
    }

    #[test]
    fn conditions_combine_with_and_take_several_values_or_a_range_and_can_be_negated() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
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
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
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
    fn every_axis_a_track_can_have_a_value_on_counts_for_all() {
        // A track with every fact: each axis says it has a value, so All goes by it (the audit, 2026-10-29: seven axes
        // said none, so All matched nothing and All negated everything).
        let full = Facts {
            rel: "a/1".into(),
            born: Some(1),
            genres: vec!["Rock".into()],
            year: Some(1985.0),
            bpm: Some(120.0),
            energy: Some(50.0),
            danceability: Some(50.0),
            moods: vec!["happy".into()],
            styles: vec!["Synth".into()],
            tags: vec!["tag".into()],
            sound: Some(Arc::from(vec![0.0_f32; 4].into_boxed_slice())),
            artists: vec!["X".into()],
            credits: vec!["x".into()],
            credit_names: vec!["X".into()],
            length: Some(240.0),
            quality: Some("lossless".into()),
            key: Some("C".into()),
            release_types: vec!["Album".into()],
            ..Facts::default()
        };
        let uses: HashMap<String, Use> = [("a/1".to_string(), Use { plays: 2, last: Some(1), rating: Some(4) })].into();
        let (membership, favourites, albums) = (HashMap::new(), HashSet::new(), HashSet::new());
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: Some(&albums), uses: Some(&uses), now: 10 };
        for (axis, _, kind) in AXES {
            if matches!(kind, Kind::Like) {
                continue;
            }
            let c = Condition::new(axis.to_string(), false, Value::Yes);
            assert!(has_value(&c, &full, &w), "{axis}: a track with every fact has a value on it");
        }
        // And All by them: an album of such tracks is all of a range that holds them all, not none of it.
        let album = [&full, &full];
        for (axis, value) in [("plays", Value::Range { min: Some(1.0), max: None }), ("length", Value::Range { min: Some(1.0), max: None }), ("artist", Value::AnyOf { values: vec!["x".into()] }), ("quality", Value::AnyOf { values: vec!["lossless".into()] }), ("key", Value::AnyOf { values: vec!["c".into()] })] {
            let all = |not: bool| Condition { quantifier: Quantifier::All, ..Condition::new(axis, not, value.clone()) };
            assert!(passes(&album, &[all(false)], &w), "{axis}: all of its tracks");
            assert!(!passes(&album, &[all(true)], &w), "{axis}: negated, not");
        }
    }

    #[test]
    fn a_row_asks_something_as_the_page_reads_it() {
        // The one fixture both suites read (the page's lib/filterRow `complete`).
        let f: serde_json::Value = serde_json::from_str(include_str!("../../web/tests/fixtures/conditions.json")).unwrap();
        let cases = f["complete"].as_array().unwrap();
        assert!(cases.len() >= 16);
        for case in cases {
            let c: Condition = serde_json::from_value(case["condition"].clone()).unwrap();
            assert_eq!(c.complete(), case["complete"].as_bool().unwrap(), "{case}");
        }
    }

    #[test]
    fn a_row_that_asks_nothing_filters_nothing() {
        let items = albums();
        let none = Condition::new("genre", false, Value::AnyOf { values: vec![] });
        let open = Condition::new("energy", true, Value::Range { min: None, max: None });
        assert_eq!(passing(&items, &[none.clone()]), [0, 1, 2], "any of none: not yet asked");
        assert_eq!(passing(&items, &[Condition { not: true, ..none }]), [0, 1, 2]);
        assert_eq!(passing(&items, &[open]), [0, 1, 2], "a range open at both ends: not yet asked");
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
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0 };
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
        let ax = list_axes(&slices, &["Road".into(), "Chill".into()], Some(&membership), None);
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
