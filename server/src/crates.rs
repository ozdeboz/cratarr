//! Crates (decision 0024): how a crate is worked out lives in the rules crate (`cratarr_rules::crates`), and is named here
//! too, so the server says `crate::crates` as it always did. What stays is the server's own: one's crates kept in
//! `crates.json`, and the log of the favourites Cratarr itself changed.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use cratarr_rules::crates::*;

use crate::moved::Moved;

/// Each artist the files name, their library id by their name, for `Alike::of`, `known` being the catalogue's ids by key: the
/// catalogue is no rule. A part of a credit no one is named by alone has no id: nothing lists them, so nothing seeds by them.
pub fn artist_ids(all: &[Facts], known: &HashMap<String, i64>) -> HashMap<String, String> {
    use crate::serve::ids;
    credited_names(all).into_keys().filter_map(|n| Some((n.to_string(), ids::artist(*known.get(&ids::artist_key(n))?)))).collect()
}

/// A crate's picture's file name: its id where that is a plain name, else the id in hex.
pub fn image_file(id: &str) -> String {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        format!("{id}.jpg")
    } else {
        format!("x{}.jpg", id.bytes().map(|b| format!("{b:02x}")).collect::<String>())
    }
}

/// The crate a picture in hex is of: `image_file` read back. None for a plain name, whose crate is one's own and keeps it.
pub fn image_crate(file: &str) -> Option<String> {
    let hex = file.strip_prefix('x')?.strip_suffix(".jpg")?;
    let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok().filter(|id| image_file(id) == file)
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
    /// Each crate's tracks left out by hand (decision 0024, amended 2026-10-19), by id: their song ids.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    left_out: BTreeMap<String, Vec<String>>,
    /// Each crate's albums left out by hand, by id: the albums' ids (a rule: an album's tracks now and later).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    left_out_albums: BTreeMap<String, Vec<String>>,
    /// The tracks excluded from every automatic pick (decision 0024, amended 2026-10-19: global exclusions): their song ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    excluded: Vec<String>,
    /// Each ranked crate's fit (its rules and the learned traits dismissed), by its id without a count.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fits: BTreeMap<String, Fit>,
    /// The conditions the account added to a fixed crate (decision 0083: Not in this crate), by `kept_id`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    added: BTreeMap<String, Vec<Condition>>,
}

impl Stored {
    /// What names these ids names what they became, everywhere `Moved::crates` looks. Whether anything did.
    fn follow(&mut self, moved: &Moved) -> Result<bool> {
        if moved.ids.is_empty() {
            return Ok(false);
        }
        let was = serde_json::to_value(&*self)?;
        let mut now = was.clone();
        moved.crates(&mut now, &mut Default::default());
        if now == was {
            return Ok(false);
        }
        *self = serde_json::from_value(now)?;
        Ok(true)
    }
}

/// One's own crates (decision 0024, issue 2): one file in Cratarr's data folder, the version before each change kept
/// beside it (`crates.json.bak`). Without a data folder they live in memory only.
pub struct CrateStore {
    path: Option<PathBuf>,
    memory: tokio::sync::Mutex<Option<Stored>>,
    /// The ids `follow` has moved since the start: a change that read them before writes them after, here and in whoever asked
    /// (`crate_leave_out` reads, works out and sets), and must not bring back what went.
    followed: std::sync::Mutex<Moved>,
}

impl CrateStore {
    fn at(path: Option<PathBuf>) -> Self {
        Self { path, memory: tokio::sync::Mutex::new(None), followed: Default::default() }
    }

    pub fn new(data_dir: Option<&str>) -> Self {
        Self::at(data_dir.map(|d| PathBuf::from(d).join("crates.json")))
    }

    /// Another account's crates (decision 0051: one's own, not shared), in its folder of the data folder.
    pub fn of_account(data_dir: Option<&str>, account: &str) -> Self {
        Self::at(data_dir.map(|d| PathBuf::from(d).join("users").join(account).join("crates.json")))
    }

    /// What names these ids follows them to what they became (a take, `Serve::follow_taken`), read, changed and written in one
    /// hold of the store, so no change lands between. The second time finds nothing to move and writes nothing. Whether what is
    /// excluded moved: a device keeps its own copy of that list.
    pub async fn follow(&self, moved: &Moved) -> Result<bool> {
        let mut memory = self.memory.lock().await;
        self.followed.lock().unwrap_or_else(|p| p.into_inner()).ids.extend(moved.ids.clone());
        let mut s = match memory.clone() {
            Some(s) => s,
            None => self.read().await?,
        };
        let excluded = s.excluded.clone();
        if !s.follow(moved)? {
            return Ok(false);
        }
        let told = s.excluded != excluded;
        self.written(&mut memory, s).await?;
        Ok(told)
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

    /// A crate's favourites setting (all until set: the owner, 2026-11-07, favourites taken in everywhere).
    pub async fn favourites(&self, id: &str) -> Result<Favourites> {
        Ok(self.stored().await?.favourites.get(id).copied().unwrap_or(Favourites::All))
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
        Ok(LeftOut { tracks: s.left_out.get(id).cloned().unwrap_or_default(), albums: s.left_out_albums.get(id).cloned().unwrap_or_default() })
    }

    /// Every crate's left-out list that holds any.
    pub async fn all_left_out(&self) -> Result<BTreeMap<String, LeftOut>> {
        let s = self.stored().await?;
        let mut all: BTreeMap<String, LeftOut> = BTreeMap::new();
        for (id, tracks) in s.left_out {
            all.entry(id).or_default().tracks = tracks;
        }
        for (id, albums) in s.left_out_albums {
            all.entry(id).or_default().albums = albums;
        }
        Ok(all)
    }

    pub async fn set_left_out(&self, id: &str, left: LeftOut) -> Result<()> {
        let mut s = self.stored().await?;
        for (kept, list) in [(&mut s.left_out, left.tracks), (&mut s.left_out_albums, left.albums)] {
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

    /// Every fixed crate with conditions added to it, by `kept_id`.
    pub async fn all_added(&self) -> Result<BTreeMap<String, Vec<Condition>>> {
        Ok(self.stored().await?.added)
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

    /// The tracks excluded from every automatic pick (their song ids).
    pub async fn excluded(&self) -> Result<Vec<String>> {
        Ok(self.stored().await?.excluded)
    }

    pub async fn set_excluded(&self, tracks: Vec<String>) -> Result<()> {
        let mut s = self.stored().await?;
        s.excluded = tracks;
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

    /// Held to the end of the write, or `follow` could land between this reading what followed and writing what it holds.
    async fn write_stored(&self, mut s: Stored) -> Result<()> {
        let mut memory = self.memory.lock().await;
        s.follow(&self.followed.lock().unwrap_or_else(|p| p.into_inner()))?;
        self.written(&mut memory, s).await
    }

    async fn written(&self, memory: &mut Option<Stored>, mut s: Stored) -> Result<()> {
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
        *memory = Some(s);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artist_seed_finds_their_songs_by_the_id_the_library_makes() {
        let (credits, credit_names) = crate::app::excluded_artists::credited(&["Air feat. Beck"]);
        let files = vec![Facts { rel: "a.flac".into(), artists: vec!["Air feat. Beck".into()], credits, credit_names, ..Facts::default() }];
        use crate::serve::ids;
        let known: HashMap<String, i64> = [(ids::artist_key("Beck"), 7), (ids::artist_key("Air feat. Beck"), 8), (ids::artist_key("Cy"), 9)].into();
        let alike = Alike::of(&files, &HashMap::new(), &artist_ids(&files, &known));
        let places = |id: i64| alike.seed_places(&ids::artist(id));
        assert_eq!(places(7), [0], "a credit");
        assert_eq!(places(8), [0], "a credit as the tags write it");
        assert!(places(9).is_empty(), "credited on nothing here");
        assert!(artist_ids(&files, &HashMap::new()).is_empty(), "no id is made up");
    }

    #[test]
    fn a_pictures_name_reads_back_as_its_crate() {
        assert_eq!(image_crate(&image_file("like:al-1")).as_deref(), Some("like:al-1"));
        assert_eq!(image_crate(&image_file("../x")).as_deref(), Some("../x"));
        assert_eq!(image_crate("c-12ab.jpg"), None, "one's own, by its own name");
        assert_eq!(image_crate("xylophone.jpg"), None, "no hex");
        assert_eq!(image_crate("xabc.jpg"), None, "half a byte");
        assert_eq!(image_crate("x41.jpg"), None, "one's own, whose name reads as hex");
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
        assert_eq!(store.favourites(&id).await.unwrap(), Favourites::All, "a new crate takes them all");
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
        let left = LeftOut { tracks: vec!["tr-1".into()], albums: vec!["al-1".into()] };
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

    fn taken() -> Moved {
        Moved { ids: [("tr-5".to_string(), "tr-1".to_string())].into(), ..Default::default() }
    }

    fn seeded(of: &[&str]) -> Vec<Condition> {
        let of = of.iter().map(|s| s.to_string()).collect();
        vec![Condition::new("like", false, Value::Like { of, count: 20, min: None, own: None, mode: None, fresh: false, weights: Vec::new(), unlisted: false, avoid: Avoid::default() })]
    }

    /// A take names no crate, so it walks all of a store: lists, seeds, and what goes by a Sounds like's id. A stop before the mark
    /// goes over it again, and that second time must leave the file alone: `.bak` is the version before the last real change.
    #[tokio::test]
    async fn a_store_follows_a_take_once() {
        let dir = std::env::temp_dir().join(format!("cratarr-crates-take-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let store = CrateStore::new(dir.to_str());
        let id = store.save(None, "Late", seeded(&["tr-5", "al-1"])).await.unwrap();
        let left = LeftOut { tracks: vec!["tr-5".into(), "tr-1".into(), "tr-2".into()], albums: Vec::new() };
        store.set_left_out(&id, left).await.unwrap();
        store.set_left_out("like:tr-5@50", LeftOut { tracks: vec!["tr-2".into()], albums: Vec::new() }).await.unwrap();
        store.set_fit("like:tr-5", Fit { rules: seeded(&["tr-5"]), dismissed: vec!["tempo".into()] }).await.unwrap();
        store.set_added("like:tr-5", seeded(&["tr-5"])).await.unwrap();
        store.set_favourites("like:tr-5", Favourites::Only).await.unwrap();
        store.set_excluded(vec!["tr-5".into(), "tr-9".into()]).await.unwrap();

        assert!(store.follow(&taken()).await.unwrap(), "what is excluded moved");
        for kept in [&store, &CrateStore::new(dir.to_str())] {
            assert_eq!(named_ids(&kept.list().await.unwrap()[0].conditions), ["tr-1", "al-1"], "its seed");
            assert_eq!(kept.left_out(&id).await.unwrap().tracks, ["tr-1", "tr-2"], "two ids of one track are one");
            assert_eq!(kept.left_out("like:tr-1@50").await.unwrap().tracks, ["tr-2"], "its count stays with it");
            assert_eq!(named_ids(&kept.fit("like:tr-1").await.unwrap().rules), ["tr-1"]);
            assert_eq!(named_ids(&kept.added("like:tr-1").await.unwrap()), ["tr-1"]);
            assert_eq!(kept.favourites("like:tr-1").await.unwrap(), Favourites::Only);
            assert_eq!(kept.excluded().await.unwrap(), ["tr-1", "tr-9"]);
            assert!(kept.left_out("like:tr-5@50").await.unwrap().is_empty(), "no key left behind");
        }

        let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
        let (file, before) = (read("crates.json"), read("crates.json.bak"));
        assert!(!store.follow(&taken()).await.unwrap(), "nothing moved, nothing told");
        assert_eq!((read("crates.json"), read("crates.json.bak")), (file.clone(), before.clone()), "nothing written");
        assert!(!CrateStore::new(dir.to_str()).follow(&taken()).await.unwrap(), "the same from the file alone");
        assert_eq!((read("crates.json"), read("crates.json.bak")), (file, before));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_store_nobody_opened_follows_from_its_file() {
        let dir = std::env::temp_dir().join(format!("cratarr-crates-closed-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        CrateStore::new(dir.to_str()).set_excluded(vec!["tr-5".into()]).await.unwrap();
        assert!(CrateStore::new(dir.to_str()).follow(&taken()).await.unwrap());
        assert_eq!(CrateStore::new(dir.to_str()).excluded().await.unwrap(), ["tr-1"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store with no file makes none by following: an account with no crates stays one.
    #[tokio::test]
    async fn following_makes_no_file_for_a_store_with_nothing_in_it() {
        let dir = std::env::temp_dir().join(format!("cratarr-crates-none-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!CrateStore::of_account(dir.to_str(), "ann").follow(&taken()).await.unwrap());
        assert!(!dir.join("users").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `crate_leave_out` reads the list, works out the new one and sets it. A take that lands between must not lose to it: what it
    /// sets holds the id that went, and nothing would ever follow that id again.
    #[tokio::test]
    async fn a_change_that_read_before_a_take_does_not_bring_its_id_back() {
        let store = CrateStore::new(None);
        store.set_left_out("c1", LeftOut { tracks: vec!["tr-5".into()], albums: Vec::new() }).await.unwrap();
        store.set_excluded(vec!["tr-5".into()]).await.unwrap();
        let (mut read, mut excluded) = (store.left_out("c1").await.unwrap(), store.excluded().await.unwrap());
        store.follow(&taken()).await.unwrap();
        read.tracks.push("tr-2".into());
        excluded.push("tr-3".into());
        store.set_left_out("c1", read).await.unwrap();
        store.set_excluded(excluded).await.unwrap();
        assert_eq!(store.left_out("c1").await.unwrap().tracks, ["tr-1", "tr-2"]);
        assert_eq!(store.excluded().await.unwrap(), ["tr-1", "tr-3"]);
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
}
