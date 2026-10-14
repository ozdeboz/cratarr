//! Crates (decision 0024): dynamic cross-sections of the library. A crate is a list of conditions on the library's axes
//! (all must hold, each negatable; OR within an axis through several values or a range), worked out when it is looked
//! at. The permanent crates are definitions in the same model; one's own are kept in `crates.json`. The axes are a
//! registry: each declares its kind and domain, and how a file is tested, so the builder draws its controls from it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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
        let mut all = conditions.to_vec();
        if self != Favourites::All {
            all.push(Condition { axis: "favourite".into(), not: self == Favourites::None, value: Value::Yes });
        }
        all
    }
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
}

const AXES: [(&str, &str, Kind); 5] = [
    ("favourite", "Favourite", Kind::YesNo),
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

// --- Freshness of the media server's favourites (decision 0024, issue 5) ---

static FAVOURITES_CHANGED: AtomicU64 = AtomicU64::new(0);

/// Cratarr itself changed a favourite: the kept favourites are asked for again.
pub fn favourites_changed() {
    FAVOURITES_CHANGED.fetch_add(1, Ordering::Relaxed);
}

pub fn favourites_generation() -> u64 {
    FAVOURITES_CHANGED.load(Ordering::Relaxed)
}

// --- One's own crates, kept in crates.json ---

#[derive(Clone, Default, Serialize, Deserialize)]
struct Stored {
    version: u32,
    crates: Vec<CrateDef>,
    /// Each crate's favourites setting (permanent ones too), by id; a crate not in it shows none.
    #[serde(default)]
    favourites: BTreeMap<String, Favourites>,
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

pub fn bpm_of(raw: &str) -> Option<f64> {
    raw.trim().replace(',', ".").parse::<f64>().ok().filter(|b| *b > 0.0 && *b < 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let w = World { membership: &membership, favourites: Some(&favourites) };
        assert_eq!(permanent().iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["unlisted"]);
        let unlisted = &permanent()[0].conditions;
        assert_eq!(select(&files, unlisted, &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::All.with(unlisted), &w), vec!["b", "d"]);
        assert_eq!(select(&files, &Favourites::Only.with(unlisted), &w), vec!["b"], "only its favourites");
        assert_eq!(select(&files, &Favourites::None.with(unlisted), &w), vec!["d"], "none of them");
        assert_eq!(select(&files, &Favourites::Only.with(&[]), &w), vec!["a", "b"], "the whole library's favourites");
        let unknown = World { membership: &membership, favourites: None };
        assert!(select(&files, &Favourites::Only.with(unlisted), &unknown).is_empty(), "favourites not known: nothing, not everything");
    }

    #[test]
    fn conditions_combine_with_and_take_several_values_or_a_range_and_can_be_negated() {
        let (files, membership, favourites) = library();
        let w = World { membership: &membership, favourites: Some(&favourites) };
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
        assert!(dir.join("crates.json.bak").exists(), "the version before kept");
        assert!(again.delete(&id).await.unwrap());
        assert!(again.list().await.unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
