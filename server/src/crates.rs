//! Crates (decision 0024): dynamic cross-sections of the library. A crate is a list of conditions on the library's axes
//! (all must hold, each negatable; OR within an axis through several values or a range), worked out when it is looked
//! at. The permanent crates are definitions in the same model; one's own are kept in `crates.json`. The axes are a
//! registry: each declares its kind and domain, and how a file is tested, so the builder draws its controls from it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The stored format's version (decision 0024: groups, OR across axes, may come later).
pub const VERSION: u32 = 1;

/// One condition: an axis, the value(s) it is tested against, and whether it is negated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub axis: String,
    #[serde(default)]
    pub not: bool,
    pub value: Value,
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
    let genre = Condition { axis: "genre".into(), not: false, value: Value::AnyOf { values: vec![name.to_string()] } };
    Some(CrateDef { id: id.to_string(), name: name.to_string(), conditions: vec![genre] })
}

/// The permanent crates, in the same model (decision 0024). Favourites and Unlisted favourites went (amended
/// 2026-10-14): any crate shows only its favourites, or none of them, with the panel's favourites toggle.
pub fn permanent() -> Vec<CrateDef> {
    let unlisted = Condition { axis: "playlist".into(), not: true, value: Value::AnyOf { values: Vec::new() } };
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
            all.push(Condition { axis: axis.into(), not: self == Favourites::None, value: Value::Yes });
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
}

/// An axis as the builder sees it: its kind and its domain (the values there are, or the numbers' span).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AxisView {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub values: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// How many of Music's files have a value on this axis (BPM is only where the files say it).
    pub known: usize,
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

/// What the axes test about one Music file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// The playlist line (relative to Music).
    pub rel: String,
    pub genres: Vec<String>,
    pub year: Option<f64>,
    pub bpm: Option<f64>,
}

/// What the axes test about the library as a whole: which playlists hold a line, which lines are favourites.
pub struct World<'a> {
    pub membership: &'a HashMap<String, Vec<String>>,
    /// `None`: the favourites are not known (the media server could not be asked).
    pub favourites: Option<&'a HashSet<String>>,
    /// The lines whose album has the viewer's heart on (the Library's album favourites); `None`: not known.
    pub favourite_albums: Option<&'a HashSet<String>>,
}

const AXES: [(&str, &str, Kind); 6] = [
    ("favourite", "Favourite", Kind::YesNo),
    ("favourite_album", "Favourite album", Kind::YesNo),
    ("playlist", "In playlist", Kind::Membership),
    ("genre", "Genre", Kind::Values),
    ("year", "Year", Kind::Range),
    ("bpm", "BPM", Kind::Range),
];

pub fn is_axis(key: &str) -> bool {
    AXES.iter().any(|(k, _, _)| *k == key)
}

/// The axes with their domains in this library: genres most common first, the years' and BPMs' span.
pub fn axes(files: &[Facts], playlists: &[String]) -> Vec<AxisView> {
    let mut genres: HashMap<String, (String, usize)> = HashMap::new();
    for f in files {
        for g in &f.genres {
            genres.entry(g.to_lowercase()).or_insert_with(|| (g.clone(), 0)).1 += 1;
        }
    }
    let mut genres: Vec<(String, usize)> = genres.into_values().collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    let span = |get: fn(&Facts) -> Option<f64>| {
        let vals: Vec<f64> = files.iter().filter_map(get).collect();
        let min = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))));
        let max = vals.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        (min, max, vals.len())
    };
    AXES.iter()
        .map(|&(key, label, kind)| {
            let mut a = AxisView { key, label, kind, values: Vec::new(), min: None, max: None, known: files.len() };
            match key {
                "playlist" => a.values = playlists.to_vec(),
                "genre" => {
                    a.values = genres.iter().map(|(g, _)| g.clone()).collect();
                    a.known = files.iter().filter(|f| !f.genres.is_empty()).count();
                }
                "year" => (a.min, a.max, a.known) = span(|f| f.year),
                "bpm" => (a.min, a.max, a.known) = span(|f| f.bpm),
                _ => {}
            }
            a
        })
        .collect()
}

/// Whether a condition holds for a file. A condition on an unknown axis, or with a value its axis does not take, never
/// holds (negated: always), so a crate from a newer version is shown narrower rather than wider.
fn holds(c: &Condition, f: &Facts, w: &World) -> bool {
    let within = |v: Option<f64>, min: &Option<f64>, max: &Option<f64>| {
        v.is_some_and(|v| min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m))
    };
    let yes = match (c.axis.as_str(), &c.value) {
        ("favourite", Value::Yes) => w.favourites.is_some_and(|s| s.contains(&f.rel)),
        ("favourite_album", Value::Yes) => w.favourite_albums.is_some_and(|s| s.contains(&f.rel)),
        ("playlist", Value::AnyOf { values }) => match w.membership.get(&f.rel) {
            None => false,
            Some(lists) if values.is_empty() => !lists.is_empty(),
            Some(lists) => values.iter().any(|v| lists.contains(v)),
        },
        ("genre", Value::AnyOf { values }) => values.iter().any(|v| f.genres.iter().any(|g| g.eq_ignore_ascii_case(v))),
        ("year", Value::Range { min, max }) => within(f.year, min, max),
        ("bpm", Value::Range { min, max }) => within(f.bpm, min, max),
        _ => false,
    };
    yes != c.not
}

/// A crate's files, in the order given (newest first).
pub fn select<'a>(files: &'a [Facts], conditions: &[Condition], w: &World) -> Vec<&'a str> {
    files.iter().filter(|f| conditions.iter().all(|c| holds(c, f, w))).map(|f| f.rel.as_str()).collect()
}

/// Whether working the conditions out needs the media server's favourites.
pub fn needs_favourites(conditions: &[Condition]) -> bool {
    conditions.iter().any(|c| c.axis == "favourite")
}

/// Whether working the conditions out needs the favourite albums (the Library's hearts).
pub fn needs_favourite_albums(conditions: &[Condition]) -> bool {
    conditions.iter().any(|c| c.axis == "favourite_album")
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
        assert_eq!(c.conditions, vec![Condition { axis: "genre".into(), not: false, value: Value::AnyOf { values: vec!["Trip Hop".into()] } }]);
        assert!(genre_crate("genre: ").is_none());
        assert!(genre_crate("unlisted").is_none());
    }

    fn f(rel: &str, genres: &[&str], year: Option<f64>, bpm: Option<f64>) -> Facts {
        Facts { rel: rel.into(), genres: genres.iter().map(|g| g.to_string()).collect(), year, bpm }
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
    fn conditions_combine_with_and_take_several_values_or_a_range_and_can_be_negated() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites), favourite_albums: None };
        let c = |axis: &str, not: bool, value: Value| Condition { axis: axis.into(), not, value };
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
        let rock = vec![Condition { axis: "genre".into(), not: false, value: Value::AnyOf { values: vec!["rock".into()] } }];
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
