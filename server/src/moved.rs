//! Ids that move at the first start of 0.16 (decision 0101 point 21, docs/records-and-copies.md), in what is not a table: the
//! JSON stores in the data folder, and the JSON inside the journal's `changes` and a preference. One map goes through them all,
//! made by whoever calls: nothing here reads the catalogue, so a hand-made map tests it. Each store is written beside and
//! renamed, so a stop leaves it whole, old or new, and a second run finds nothing old to move. An id no map holds stays as it
//! is, and is counted. A take goes the same way through what `crates` walks, the pictures, the journal's changes and the filters
//! (`Serve::follow_taken`), as `stores` is too late for a store already open.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::crates::{self, Condition};
use crate::serve::ids;

/// A table or a store as the migration's report reads it (`userdata::put_migration_part`): its rows before and after, how
/// many ids in it moved, and how many rows met another under one key and became one with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub before: u64,
    pub after: u64,
    pub moved: u64,
    pub merged: u64,
}

/// What found no match in one place: how many, and up to ten of them, each named once. They stay as they were.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Unmatched {
    pub what: String,
    pub count: u64,
    pub sample: Vec<String>,
}

/// How many samples an unmatched row keeps.
const SAMPLES: usize = 10;

/// What a step of the move found, in the report's lists. `differed`: two rows of one record said different things where only
/// one can stand; the first met stood, so the other is gone, counted as `Unmatched` is, by the record it spoke of.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub tables: Vec<Table>,
    pub unmatched: Vec<Unmatched>,
    pub differed: Vec<Unmatched>,
}

/// How a list reads in the report. An id that came several times is named once, as the rows of one id come together.
fn listed(what: &str, mut ids: Vec<String>) -> Unmatched {
    let count = ids.len() as u64;
    ids.dedup();
    ids.truncate(SAMPLES);
    Unmatched { what: what.to_string(), count, sample: ids }
}

impl Report {
    pub fn table(&mut self, name: &str, before: u64, after: u64, moved: u64) {
        self.tables.push(Table { name: name.to_string(), before, after, moved, merged: 0 });
    }

    /// A place's ids that found no match: a row for each place, none found too, so the report says it looked.
    pub fn unmatched(&mut self, what: &str, ids: Vec<String>) {
        self.unmatched.push(listed(what, ids));
    }

    /// A place's rows where two said different things: a row for each such place, none found too.
    pub fn differed(&mut self, what: &str, records: Vec<String>) {
        self.differed.push(listed(what, records));
    }

    /// One walk over a place: its rows and what it found.
    pub fn walked(&mut self, name: &str, rows: u64, walk: Walk) {
        self.table(name, rows, rows, walk.moved);
        self.unmatched(name, walk.unmatched);
    }

    /// Another step's rows after these.
    pub fn extend(&mut self, other: Report) {
        self.tables.extend(other.tables);
        self.unmatched.extend(other.unmatched);
        self.differed.extend(other.differed);
    }

    /// The rows of one table, by its name.
    #[cfg(test)]
    pub fn table_named(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// How many found no match, in all.
    #[cfg(test)]
    pub fn unmatched_count(&self) -> u64 {
        self.unmatched.iter().map(|u| u.count).sum()
    }
}

/// What walking one place found.
#[derive(Debug, Default)]
pub struct Walk {
    pub moved: u64,
    pub unmatched: Vec<String>,
}

/// What the move goes by, whoever made it.
#[derive(Debug, Clone, Default)]
pub struct Moved {
    /// An old id to what it became, of any kind, as served: `al-<hash>` to `al-<record>`, `ar-<hash>` to `ar-<id>`, `tr-<a>`
    /// to `tr-<b>` where two copies' tracks became one.
    pub ids: HashMap<String, String>,
    /// A playlist line as a crate's left-out list and what is excluded kept it, to its track's song id (0101: lines to tracks).
    pub lines: HashMap<String, String>,
}

/// The lists of `list`'s key, where it is one: each item of a store's list.
fn items_of<'j>(v: &'j mut Json, key: &str) -> impl Iterator<Item = &'j mut Json> {
    v.get_mut(key).and_then(Json::as_array_mut).into_iter().flatten()
}

/// The values of the map at `key`, where it is one.
fn values_of<'j>(v: &'j mut Json, key: &str) -> impl Iterator<Item = &'j mut Json> {
    v.get_mut(key).and_then(Json::as_object_mut).into_iter().flat_map(|m| m.values_mut())
}

/// A list's strings, each through `one`: two that become one are one.
fn each(list: &mut Json, mut one: impl FnMut(&str) -> Option<String>) {
    let Some(items) = list.as_array_mut() else { return };
    let mut kept: Vec<Json> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let item = item.as_str().and_then(&mut one).map_or(item, Json::from);
        if !kept.contains(&item) {
            kept.push(item);
        }
    }
    *items = kept;
}

/// Two values of one crate that were two before the move. Lists join, each item once, and a fit's by each of its lists; of two
/// settings the first in the store's order stays: they said different things of what is one crate now, and nothing says which
/// was meant.
fn fold(kept: &mut Json, other: Json) {
    match (kept, other) {
        (Json::Array(kept), Json::Array(other)) => {
            for item in other {
                if !kept.contains(&item) {
                    kept.push(item);
                }
            }
        }
        (Json::Object(kept), Json::Object(other)) => {
            for (key, value) in other {
                match kept.get_mut(&key) {
                    Some(mine) => fold(mine, value),
                    None => {
                        kept.insert(key, value);
                    }
                }
            }
        }
        _ => {}
    }
}

impl Moved {
    /// What an id became, counted. An album's or an artist's old id with nowhere to go is noted; a track's id with no merge,
    /// an id for life or another server's is simply as it was.
    fn id(&self, id: &str, walk: &mut Walk) -> Option<String> {
        if let Some(new) = self.ids.get(id) {
            walk.moved += 1;
            return Some(new.clone());
        }
        if (id.starts_with(ids::ALBUM) || id.starts_with(ids::ARTIST)) && !ids::for_life(id) {
            walk.unmatched.push(id.to_string());
        }
        None
    }

    /// A crate's id: a Sounds like goes with its seed, its count kept (`like:<item>[@<count>]`). Any other's is its own.
    fn crate_id(&self, id: &str, walk: &mut Walk) -> Option<String> {
        let rest = id.strip_prefix(crates::LIKE)?;
        let (item, count) = rest.rsplit_once('@').map_or((rest, None), |(item, count)| (item, Some(count)));
        let new = self.id(item, walk)?;
        Some(match count {
            Some(count) => format!("{}{new}@{count}", crates::LIKE),
            None => format!("{}{new}", crates::LIKE),
        })
    }

    /// What a list of tracks holds: a line becomes its track's song id; a song id follows a merge. A line no track holds stays,
    /// reading as nothing, and is noted.
    fn track(&self, held: &str, walk: &mut Walk) -> Option<String> {
        if let Some(track) = self.lines.get(held) {
            walk.moved += 1;
            return Some(track.clone());
        }
        if ids::track_of(held).is_some() {
            return self.id(held, walk);
        }
        walk.unmatched.push(held.to_string());
        None
    }

    /// One field of `item` that holds an id.
    fn field(&self, item: &mut Json, name: &str, walk: &mut Walk) {
        if let Some(new) = item.get(name).and_then(Json::as_str).and_then(|id| self.id(id, walk)) {
            item[name] = new.into();
        }
    }

    /// One field of `item` that holds a crate's id.
    fn crate_field(&self, item: &mut Json, name: &str, walk: &mut Walk) {
        if let Some(new) = item.get(name).and_then(Json::as_str).and_then(|id| self.crate_id(id, walk)) {
            item[name] = new.into();
        }
    }

    /// Conditions as JSON, what `crates::named_ids` reads moved by `crates::move_named`. Only a condition's `value` is written:
    /// a page's filter may hold what the server does not read.
    fn conditions(&self, list: &mut Json, walk: &mut Walk) {
        for c in list.as_array_mut().into_iter().flatten() {
            let Ok(was) = serde_json::from_value::<Condition>(c.clone()) else { continue };
            let mut one = [was.clone()];
            crates::move_named(&mut one, &mut |id| self.id(id, walk));
            if one[0].value != was.value
                && let Ok(value) = serde_json::to_value(&one[0].value)
            {
                c["value"] = value;
            }
        }
    }

    /// A map's keys that are crates' ids. Two old ids of one record meet here, and fold into the one crate.
    fn rekey(&self, map: &mut serde_json::Map<String, Json>, walk: &mut Walk) {
        for (key, value) in std::mem::take(map) {
            let key = self.crate_id(&key, walk).unwrap_or(key);
            match map.get_mut(&key) {
                Some(kept) => fold(kept, value),
                None => {
                    map.insert(key, value);
                }
            }
        }
    }

    /// `crates.json`, anyone's: conditions, albums left out, tracks left out and excluded, and every map kept by a crate's id.
    pub fn crates(&self, v: &mut Json, walk: &mut Walk) -> u64 {
        for c in items_of(v, "crates") {
            if let Some(conditions) = c.get_mut("conditions") {
                self.conditions(conditions, walk);
            }
        }
        for fit in values_of(v, "fits") {
            if let Some(rules) = fit.get_mut("rules") {
                self.conditions(rules, walk);
            }
        }
        for added in values_of(v, "added") {
            self.conditions(added, walk);
        }
        for albums in values_of(v, "left_out_albums") {
            each(albums, |id| self.id(id, walk));
        }
        for tracks in values_of(v, "left_out") {
            each(tracks, |held| self.track(held, walk));
        }
        if let Some(tracks) = v.get_mut("excluded") {
            each(tracks, |held| self.track(held, walk));
        }
        for key in ["favourites", "favourite_albums", "left_out", "left_out_albums", "fits", "added"] {
            if let Some(map) = v.get_mut(key).and_then(Json::as_object_mut) {
                self.rekey(map, walk);
            }
        }
        v.get("crates").and_then(Json::as_array).map_or(0, |c| c.len() as u64)
    }

    /// A queue, a player's or one left behind: its tracks and their albums, and what its Autoplay stays close to and inside.
    fn queue(&self, q: &mut Json, walk: &mut Walk) {
        for track in items_of(q, "tracks") {
            self.field(track, "id", walk);
            self.field(track, "album_id", walk);
        }
        let Some(autoplay) = q.get_mut("autoplay").filter(|a| a.is_object()) else { return };
        if let Some(seed) = autoplay.get_mut("seed") {
            each(seed, |id| self.id(id, walk));
        }
        let Some(within) = autoplay.get_mut("within").filter(|w| w.is_object()) else { return };
        match within.get("kind").and_then(Json::as_str) {
            Some("album") => self.field(within, "id", walk),
            Some("crate") => self.crate_field(within, "id", walk),
            Some("tracks") => {
                if let Some(tracks) = within.get_mut("ids") {
                    each(tracks, |id| self.id(id, walk));
                }
            }
            _ => {}
        }
    }

    /// `queues.json`: each player's queue and those it left behind.
    fn queues(&self, v: &mut Json, walk: &mut Walk) -> u64 {
        let Some(players) = v.as_object_mut() else { return 0 };
        for saved in players.values_mut() {
            self.queue(saved, walk);
            for kept in items_of(saved, "kept") {
                self.queue(kept, walk);
            }
        }
        players.len() as u64
    }

    /// A store of jobs, each with the album it is about at `album.id`: `convert-queue.json`, `track-intake.json`. What waits
    /// finds its album by it when its turn comes.
    fn jobs(&self, v: &mut Json, list: &str, walk: &mut Walk) -> u64 {
        let mut n = 0;
        for job in items_of(v, list) {
            n += 1;
            if let Some(album) = job.get_mut("album") {
                self.field(album, "id", walk);
            }
        }
        n
    }

    /// A left-out list as `Change::LeftOut` kept it, `lines` until the tracks.
    fn left_out(&self, left: &mut Json, walk: &mut Walk) {
        let Some(map) = left.as_object_mut() else { return };
        if let Some(lines) = map.remove("lines") {
            map.insert("tracks".to_string(), lines);
        }
        if let Some(tracks) = map.get_mut("tracks") {
            each(tracks, |held| self.track(held, walk));
        }
        if let Some(albums) = map.get_mut("albums") {
            each(albums, |id| self.id(id, walk));
        }
    }

    /// A kept change of the journal (`Change`, as JSON), as it must read after the move: Undo holds a store against what the
    /// change left, so it must say what the store says. What names no album, artist or track of these stores is left.
    pub fn change(&self, change: &mut Json, walk: &mut Walk) {
        match change.get("kind").and_then(Json::as_str) {
            Some("crate") => {
                for side in ["before", "after"] {
                    if let Some(conditions) = change.get_mut(side).and_then(|c| c.get_mut("conditions")) {
                        self.conditions(conditions, walk);
                    }
                }
            }
            Some("left_out") => {
                self.crate_field(change, "crate_id", walk);
                for side in ["before", "after"] {
                    if let Some(left) = change.get_mut(side) {
                        self.left_out(left, walk);
                    }
                }
            }
            Some("excluded") => {
                for side in ["before", "after"] {
                    if let Some(tracks) = change.get_mut(side) {
                        each(tracks, |held| self.track(held, walk));
                    }
                }
            }
            Some("added") => {
                self.crate_field(change, "crate_id", walk);
                for side in ["before", "after"] {
                    if let Some(conditions) = change.get_mut(side) {
                        self.conditions(conditions, walk);
                    }
                }
            }
            _ => {}
        }
    }

    /// The preference `filters`: every list's filter, by the list.
    pub fn filters(&self, filters: &mut Json, walk: &mut Walk) {
        for filter in filters.as_object_mut().into_iter().flat_map(|f| f.values_mut()) {
            if let Some(conditions) = filter.get_mut("conditions") {
                self.conditions(conditions, walk);
            }
        }
    }

    /// One JSON store, written beside and renamed where it changed. No file is nothing to move; one that cannot be read stops
    /// the move, as Cratarr must not start on half-moved data.
    fn store(&self, file: &Path, name: &str, report: &mut Report, walk_it: impl FnOnce(&mut Json, &mut Walk) -> u64) -> Result<()> {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report.walked(name, 0, Walk::default())),
            Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
        };
        let mut value: Json = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?;
        let was = value.clone();
        let mut walk = Walk::default();
        let rows = walk_it(&mut value, &mut walk);
        if value != was {
            let beside = file.with_extension("json.moving");
            write_beside(&beside, file, &serde_json::to_vec_pretty(&value)?).with_context(|| format!("writing {}", beside.display()))?;
            std::fs::rename(&beside, file).with_context(|| format!("writing {}", file.display()))?;
        }
        report.walked(name, rows, walk);
        Ok(())
    }

    /// `crate-images`: a Sounds like's picture is named by its seed (`crates::image_file`), so it moves with it. A picture
    /// already there stays, and the other is left under its old name, counted: of two pictures of one crate, one can show.
    pub fn images(&self, dir: &Path, report: &mut Report) -> Result<()> {
        let mut walk = Walk::default();
        let mut pictures = 0;
        if let Ok(files) = std::fs::read_dir(dir) {
            for file in files.flatten() {
                pictures += 1;
                let Some(id) = file.file_name().to_str().and_then(crates::image_crate) else { continue };
                let Some(new) = self.crate_id(&id, &mut walk) else { continue };
                let to = dir.join(crates::image_file(&new));
                if to.exists() {
                    walk.unmatched.push(id);
                } else {
                    std::fs::rename(file.path(), &to).with_context(|| format!("moving {}", file.path().display()))?;
                }
            }
        }
        report.walked("crate-images", pictures, walk);
        Ok(())
    }

    /// Every JSON store in the data folder, before anything has read them: a store holds what it read and writes it back, so
    /// after it would be too late. What it did, a row each.
    pub fn stores(&self, data: &Path) -> Result<Report> {
        let mut report = Report::default();
        self.store(&data.join("crates.json"), "crates.json", &mut report, |v, walk| self.crates(v, walk))?;
        if let Ok(users) = std::fs::read_dir(data.join("users")) {
            for user in users.flatten().filter(|user| user.file_type().is_ok_and(|kind| kind.is_dir())) {
                let name = format!("users/{}/crates.json", user.file_name().to_string_lossy());
                self.store(&user.path().join("crates.json"), &name, &mut report, |v, walk| self.crates(v, walk))?;
            }
        }
        self.images(&data.join("crate-images"), &mut report)?;
        self.store(&data.join("queues.json"), "queues.json", &mut report, |v, walk| self.queues(v, walk))?;
        self.store(&data.join("convert-queue.json"), "convert-queue.json", &mut report, |v, walk| self.jobs(v, "list", walk))?;
        self.store(&data.join("track-intake.json"), "track-intake.json", &mut report, |v, walk| self.jobs(v, "items", walk))?;
        Ok(report)
    }
}

/// What takes `file`'s place, as private as it was, and on the disk before it takes the name: after a power cut an empty store
/// would stop the next start, and the old one is gone by then.
fn write_beside(beside: &Path, file: &Path, text: &[u8]) -> std::io::Result<()> {
    let mut out = std::fs::File::create(beside)?;
    out.set_permissions(std::fs::metadata(file)?.permissions())?;
    std::io::Write::write_all(&mut out, text)?;
    out.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::{CrateStore, Favourites, LeftOut, Value};

    const OLD: &str = "al-5vcUvYmuWJAgyxLXEUmaPz";
    const INCOMING: &str = "al-2e3jjYJ6JRxCOcLSRKg8qj";
    const ARTIST: &str = "ar-6ZlhpxBxPkCMTsT1I1QT0d";

    fn moved() -> Moved {
        let ids = [(OLD, "al-7"), (INCOMING, "al-7"), (ARTIST, "ar-3"), ("tr-9", "tr-4")].map(|(a, b)| (a.to_string(), b.to_string()));
        let lines = [("A/1.flac", "tr-1"), ("../in/A/1.mp3", "tr-1"), ("A/2.flac", "tr-2")].map(|(a, b)| (a.to_string(), b.to_string()));
        Moved { ids: ids.into(), lines: lines.into() }
    }

    fn folder(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-moved-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn with_crates(name: &str, store: Json) -> std::path::PathBuf {
        let d = folder(name);
        std::fs::write(d.join("crates.json"), store.to_string()).unwrap();
        d
    }

    fn like(of: &[&str]) -> Json {
        serde_json::json!({ "axis": "like", "not": false, "value": { "kind": "like", "of": of, "count": 20 } })
    }

    /// The seeds of a crate kept, read as a server reads them.
    async fn seeds(d: &Path, id: &str) -> Vec<String> {
        let store = CrateStore::new(d.to_str());
        let list = store.list().await.unwrap();
        crates::named_ids(&list.iter().find(|c| c.id == id).unwrap().conditions)
    }

    #[tokio::test]
    async fn a_saved_crate_keeps_its_seed() {
        let album = serde_json::json!({ "axis": "album", "not": false, "value": { "kind": "any_of", "values": [OLD, "al-99"] } });
        let d = with_crates("seed", serde_json::json!({ "version": 1, "crates": [{ "id": "c1", "name": "One", "conditions": [like(&[OLD, "tr-9"]), album] }] }));
        let report = moved().stores(&d).unwrap();
        assert_eq!(seeds(&d, "c1").await, ["al-7", "tr-4", "al-99"], "a track's merge too, and an id for life stays");
        let t = report.table_named("crates.json").unwrap();
        assert_eq!((t.before, t.after, t.moved), (1, 1, 3));
        assert_eq!(report.unmatched_count(), 0);
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_saved_crate_keeps_its_artist_seed() {
        let d = with_crates("artist", serde_json::json!({ "version": 1, "crates": [{ "id": "c1", "name": "One", "conditions": [like(&[ARTIST])] }] }));
        moved().stores(&d).unwrap();
        assert_eq!(seeds(&d, "c1").await, ["ar-3"]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A Sounds like is kept by its seed: its toggles, what was left out of it and its fit go with it, wherever an old link
    /// reads them. Two old ids of one record are one crate after: lists join, and of two settings the first stays.
    #[tokio::test]
    async fn what_is_kept_by_a_crates_id_follows_its_seed() {
        // The first in the file's order: the store's maps are sorted.
        let (one, two) = (format!("like:{INCOMING}"), format!("like:{OLD}"));
        let store = serde_json::json!({
            "version": 1,
            "crates": [],
            "favourites": { (one.clone()): "only", (two.clone()): "none", "unlisted": "all" },
            "left_out": { (one.clone()): ["A/1.flac"], (two.clone()): ["A/2.flac"] },
            "left_out_albums": { (one.clone()): ["al-99"] },
            "fits": { (one.clone()): { "rules": [like(&[OLD])], "dismissed": ["tempo"] }, (two.clone()): { "dismissed": ["key", "tempo"] } },
            "added": { (format!("like:{ARTIST}")): [{ "axis": "album", "not": true, "value": { "kind": "any_of", "values": [INCOMING] } }] },
        });
        let d = with_crates("keys", store);
        moved().stores(&d).unwrap();
        let kept = CrateStore::new(d.to_str());
        assert_eq!(kept.favourites("like:al-7").await.unwrap(), Favourites::Only, "of two settings the first");
        assert_eq!(kept.favourites("unlisted").await.unwrap(), Favourites::All, "any other crate's stays");
        let left = kept.left_out("like:al-7").await.unwrap();
        assert_eq!(left, LeftOut { tracks: vec!["tr-1".into(), "tr-2".into()], albums: vec!["al-99".into()] });
        let fit = kept.fit("like:al-7").await.unwrap();
        assert_eq!((crates::named_ids(&fit.rules), fit.dismissed), (vec!["al-7".to_string()], vec!["tempo".to_string(), "key".to_string()]));
        assert_eq!(crates::named_ids(&kept.added("like:ar-3").await.unwrap()), ["al-7"]);
        assert_eq!(kept.all_left_out().await.unwrap().len(), 1, "no key left behind");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A count in a crate's id is the crate's own: `like:<item>@<count>`.
    #[test]
    fn a_sounds_likes_count_stays_with_it() {
        let mut walk = Walk::default();
        assert_eq!(moved().crate_id(&format!("like:{OLD}@50"), &mut walk).as_deref(), Some("like:al-7@50"));
        assert_eq!(moved().crate_id("like:list:pl-1", &mut walk), None, "a playlist's goes by the playlist");
        assert_eq!(moved().crate_id("c-12ab", &mut walk), None);
        assert_eq!(walk.moved, 1);
    }

    /// CR6, 0101's "lines to tracks": left out and excluded leave the store as tracks, in anyone's store, each once, and a line
    /// no track holds stays, read as nothing and counted.
    #[tokio::test]
    async fn lines_left_out_and_excluded_become_tracks() {
        let d = with_crates("lines", serde_json::json!({
            "version": 1,
            "crates": [],
            "left_out": { "c1": ["A/1.flac", "../in/A/1.mp3", "Gone/1.flac", "tr-9"] },
            "excluded": ["A/2.flac", "Gone/2.flac"],
        }));
        std::fs::create_dir_all(d.join("users/bob")).unwrap();
        std::fs::write(d.join("users/bob/crates.json"), serde_json::json!({ "version": 1, "crates": [], "excluded": ["A/1.flac"] }).to_string()).unwrap();
        let report = moved().stores(&d).unwrap();
        let kept = CrateStore::new(d.to_str());
        assert_eq!(kept.left_out("c1").await.unwrap().tracks, ["tr-1", "Gone/1.flac", "tr-4"], "two lines of one track are one");
        assert_eq!(kept.excluded().await.unwrap(), ["tr-2", "Gone/2.flac"]);
        assert_eq!(CrateStore::of_account(d.to_str(), "bob").excluded().await.unwrap(), ["tr-1"]);
        let lost = report.unmatched.iter().find(|u| u.what == "crates.json").unwrap();
        assert_eq!((lost.count, lost.sample.as_slice()), (2, ["Gone/1.flac".to_string(), "Gone/2.flac".to_string()].as_slice()));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_crates_picture_follows_its_seed() {
        let d = folder("images");
        let dir = d.join("crate-images");
        std::fs::create_dir_all(&dir).unwrap();
        let (one, two, own) = (format!("like:{OLD}"), format!("like:{INCOMING}"), "c-12ab");
        std::fs::write(dir.join(crates::image_file(&one)), "first").unwrap();
        std::fs::write(dir.join(crates::image_file(&two)), "second").unwrap();
        std::fs::write(dir.join(crates::image_file(own)), "own").unwrap();
        let report = moved().stores(&d).unwrap();
        let shown = std::fs::read_to_string(dir.join(crates::image_file("like:al-7"))).unwrap();
        assert!(["first", "second"].contains(&shown.as_str()), "{shown}");
        assert!(dir.join(crates::image_file(own)).is_file(), "an own crate's is by its own id");
        let kept: Vec<_> = report.unmatched.iter().filter(|u| u.what == "crate-images").flat_map(|u| u.sample.clone()).collect();
        assert_eq!(kept.len(), 1, "the other left under its old name, counted");
        assert!(dir.join(crates::image_file(&kept[0])).is_file());
        assert_eq!(report.table_named("crate-images").unwrap().before, 3);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn moving_a_store_twice_changes_nothing() {
        let d = with_crates("twice", serde_json::json!({
            "version": 1,
            "crates": [{ "id": "c1", "name": "One", "conditions": [like(&[OLD, ARTIST, "al-gonegonegonegonegone00"])] }],
            "favourites": { (format!("like:{OLD}")): "only" },
            "left_out": { "c1": ["A/1.flac", "Gone/1.flac"] },
        }));
        std::fs::write(d.join("queues.json"), serde_json::json!({ "here": { "version": 1, "tracks": [{ "source": "cratarr", "id": "tr-9", "album_id": OLD }] } }).to_string()).unwrap();
        moved().stores(&d).unwrap();
        let read = |name: &str| std::fs::read(d.join(name)).unwrap();
        let when = |name: &str| std::fs::metadata(d.join(name)).unwrap().modified().unwrap();
        let (crates_then, queues_then, written) = (read("crates.json"), read("queues.json"), when("crates.json"));
        let again = moved().stores(&d).unwrap();
        assert_eq!((read("crates.json"), read("queues.json")), (crates_then, queues_then));
        assert_eq!(when("crates.json"), written, "nothing to write, so nothing written");
        assert_eq!(again.tables.iter().map(|t| t.moved).sum::<u64>(), 0);
        assert_eq!(again.unmatched_count(), 2, "what found no match the first time, said again");
        assert!(!d.join("crates.json.moving").exists());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_store_that_cannot_be_read_stops_the_move_and_none_is_nothing() {
        let d = folder("broken");
        assert_eq!(moved().stores(&d).unwrap().tables.iter().map(|t| t.before).sum::<u64>(), 0, "a new folder has nothing to move");
        std::fs::write(d.join("queues.json"), "{ half written").unwrap();
        assert!(moved().stores(&d).is_err());
        assert_eq!(std::fs::read_to_string(d.join("queues.json")).unwrap(), "{ half written", "left for a person to see");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A filter's condition can hold what the server does not read: the value alone is written, and only where it moved.
    #[test]
    fn a_filter_moves_its_ids_and_keeps_the_rest_of_itself() {
        let mut filters = serde_json::json!({
            "albums": { "on": ["a"], "conditions": [{ "axis": "album", "not": false, "value": { "kind": "any_of", "values": [OLD] }, "page": "keeps this" }] },
            "tracks": { "on": [], "conditions": [{ "axis": "genre", "not": false, "value": { "kind": "any_of", "values": [OLD] } }, { "half": "made" }] },
        });
        let mut walk = Walk::default();
        moved().filters(&mut filters, &mut walk);
        assert_eq!(filters["albums"]["conditions"][0]["value"]["values"], serde_json::json!(["al-7"]));
        assert_eq!(filters["albums"]["conditions"][0]["page"], "keeps this");
        assert_eq!(filters["tracks"]["conditions"][0]["value"]["values"], serde_json::json!([OLD]), "a genre's values are names, not ids");
        assert_eq!(filters["tracks"]["conditions"][1], serde_json::json!({ "half": "made" }));
        assert_eq!((walk.moved, walk.unmatched.len()), (1, 0));
    }

    /// What the fixture's stores are made of reads back as a server reads it, moved by a map taken from their own words: it says
    /// nothing of whether the first start's map holds them, which is `nothing_with_somewhere_to_go_stays_under_an_old_id`.
    #[tokio::test]
    async fn the_crates_of_0_15_read_back_as_a_server_reads_them() {
        let d = folder("0-15");
        std::fs::create_dir_all(d.join("users/bob")).unwrap();
        let (first, second) = (include_str!("../tests/fixtures/0.15/crates.json"), include_str!("../tests/fixtures/0.15/users/bob/crates.json"));
        std::fs::write(d.join("crates.json"), first).unwrap();
        std::fs::write(d.join("users/bob/crates.json"), second).unwrap();
        let mut map = Moved::default();
        for word in format!("{first} {second}").split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
            if (word.starts_with("al-") || word.starts_with("ar-")) && word.len() == 25 {
                let new = format!("{}{}", &word[..3], map.ids.len() + 1);
                map.ids.insert(word.to_string(), new);
            }
        }
        let catalogue = rusqlite::Connection::open_in_memory().unwrap();
        catalogue.execute_batch(include_str!("../tests/fixtures/0.15/catalogue.sql")).unwrap();
        let rows = catalogue.prepare("SELECT rel, track FROM files").unwrap().query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        map.lines = rows.into_iter().map(|(rel, track)| (rel, ids::song(track))).collect();
        let report = map.stores(&d).unwrap();
        assert_eq!(report.unmatched_count(), 0, "{:?}", report.unmatched);
        for file in ["crates.json", "users/bob/crates.json"] {
            let text = std::fs::read_to_string(d.join(file)).unwrap();
            assert!(!text.contains(".flac"), "{file}: a line left: {text}");
            let hash = text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').find(|w| w.len() == 25 && (w.starts_with("al-") || w.starts_with("ar-")));
            assert_eq!(hash, None, "{file}");
        }
        let kept = CrateStore::new(d.to_str());
        assert_eq!(kept.list().await.unwrap().len(), 3);
        assert_eq!(kept.left_out("c-shores").await.unwrap().tracks.len(), 1);
        assert_eq!(kept.excluded().await.unwrap().len(), 1);
        assert!(matches!(&kept.list().await.unwrap()[0].conditions[0].value, Value::Like { of, .. } if of.len() == 2));
        std::fs::remove_dir_all(&d).ok();
    }

    /// `queues.json` is its owner's alone: a store moved stays as private as it was.
    #[cfg(unix)]
    #[test]
    fn a_store_stays_as_private_as_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let d = with_crates("private", serde_json::json!({ "version": 1, "excluded": ["A/1.flac"] }));
        let file = d.join("crates.json");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        moved().stores(&d).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("tr-1"), "written");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(&d).ok();
    }

    /// What lies beside the accounts' folders is nobody's crates, and no reason to stop a start.
    #[test]
    fn a_file_among_the_accounts_stops_nothing() {
        let d = folder("beside");
        std::fs::create_dir_all(d.join("users")).unwrap();
        std::fs::write(d.join("users/notes.txt"), b"not an account").unwrap();
        moved().stores(&d).unwrap();
        std::fs::remove_dir_all(&d).ok();
    }

    /// Undo holds a store against what the change left and refuses if they differ, so the move must take both to the same
    /// place: each list by one mapper, lines and albums, the crate's id, two lines of one track as one.
    #[tokio::test]
    async fn an_undo_from_before_the_move_finds_the_store_as_it_left_it() {
        use crate::journal::{Change, Journal};
        let crate_id = format!("like:{OLD}");
        let lines = ["A/1.flac", "../in/A/1.mp3", "Gone/1.flac"];
        let d = with_crates("undo", serde_json::json!({
            "version": 1,
            "crates": [],
            "left_out": { (crate_id.clone()): lines },
            "left_out_albums": { (crate_id.clone()): [INCOMING] },
            "excluded": lines,
        }));
        let j = Journal::open(None).unwrap();
        let held: Vec<String> = lines.map(str::to_string).to_vec();
        let left = Change::LeftOut { crate_id, before: LeftOut::default(), after: LeftOut { tracks: held.clone(), albums: vec![INCOMING.to_string()] } };
        let (one, two) = (j.record_change(1, &left, None).unwrap(), j.record_change(1, &Change::Excluded { before: Vec::new(), after: held }, None).unwrap());
        let map = moved();
        map.stores(&d).unwrap();
        j.move_changes(&map).unwrap();
        let store = CrateStore::new(d.to_str());
        let Change::LeftOut { crate_id, after, .. } = j.change(one).unwrap().unwrap().0 else { panic!("a left-out list") };
        assert_eq!(store.left_out(&crate_id).await.unwrap(), after, "what Undo holds it against");
        let Change::Excluded { after, .. } = j.change(two).unwrap().unwrap().0 else { panic!("what is excluded") };
        assert_eq!(store.excluded().await.unwrap(), after);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_report_says_where_it_looked_and_keeps_ten_samples() {
        let mut r = Report::default();
        r.unmatched("nothing", Vec::new());
        r.unmatched("many", (0..25).map(|n| format!("al-{n}")).collect());
        assert_eq!(r.unmatched_count(), 25);
        assert_eq!((r.unmatched[0].count, r.unmatched[1].sample.len()), (0, 10));
    }
}
