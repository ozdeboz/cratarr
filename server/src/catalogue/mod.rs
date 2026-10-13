//! The catalogue (decision 0008): every audio file in the playlist folder and the extra folders, its tags as stored,
//! and each server's id for it. Derived data in one SQLite file in the data folder (or in memory without one):
//! deleting it costs only a refill.
//!
//! Passes run in the background (`run`): walk the folders (size and modification time decide what changed), read
//! the tags of new and changed files a batch at a time, then ask every server for its tracks (paged, then only what
//! changed) and link each to a file. A folder that cannot be read keeps its rows and reports the error: an
//! unmounted folder must never look like "everything was deleted".

mod id3;
pub mod identity;
pub mod normalise;
mod tags;
mod walk;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub use normalise::Rules;

use crate::source::Source;

/// Tags read per batch; the lock is released between batches.
const TAG_BATCH: usize = 100;
/// Tracks asked of a server per page (measured on Jellyfin 10.11: 500 in about 5 s).
const PAGE: usize = 500;
/// Between passes when nothing asks for one sooner.
const PASS_EVERY: Duration = Duration::from_secs(300);
/// A full listing of each server library at least this often, to forget tracks the server dropped.
const FULL_EVERY_SECS: i64 = 24 * 3600;

// temp_store: the image has no /tmp, and building an index (or a large sort) needs temporary storage.
const SCHEMA: &str = "
PRAGMA temp_store = MEMORY;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS folders (id INTEGER PRIMARY KEY, path TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  state INTEGER NOT NULL DEFAULT 0, format TEXT, bitrate INTEGER, duration_ms INTEGER, error TEXT,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS tags (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, n INTEGER NOT NULL, raw TEXT NOT NULL, name TEXT, value TEXT NOT NULL,
  PRIMARY KEY (folder, rel, n)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS tags_by_name ON tags (name, value);
CREATE TABLE IF NOT EXISTS source_ids (
  source TEXT NOT NULL, track_id TEXT NOT NULL, library TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL,
  seen INTEGER NOT NULL, PRIMARY KEY (source, track_id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS source_ids_by_file ON source_ids (folder, rel);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tracks (id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER);
";

/// A file the catalogue counts: neither gone (no longer where it was, waiting to be recognised elsewhere) nor in the
/// trash.
const PRESENT: &str = "gone_at IS NULL AND trashed IS NULL";

/// The meta key of `Catalogue::generation`.
const GENERATION: &str = "generation";

/// How long a gone file waits to be recognised as moved before its track id is retired (decision 0011 §4).
const GONE_KEEP_SECS: i64 = 24 * 3600;

/// Columns added after the first schema; added to an existing database when missing.
const ADDED_COLUMNS: [(&str, &str); 5] = [
    ("files", "issue TEXT"),
    ("files", "issue_detail TEXT"),
    // Decision 0011: the Cratarr track id, when the file was last seen missing, and the trash token while trashed.
    ("files", "track INTEGER"),
    ("files", "gone_at INTEGER"),
    ("files", "trashed TEXT"),
];

/// File states.
const PENDING: i64 = 0;
const READ: i64 = 1;
const FAILED: i64 = 2;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    /// What the background pass is doing now, if anything: "walking", "reading tags", "linking <server>".
    pub doing: Option<String>,
    /// Epoch seconds of the last completed pass.
    pub last_pass: Option<i64>,
    /// Problems of the last pass (unreadable folders, a server that failed), one line each.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// The folder as Cratarr sees it, and the file in it.
    pub folder: String,
    pub rel: String,
    /// "broken", "damaged_tag", "no_tags" or "no_length".
    pub kind: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderStatus {
    pub path: String,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub folders: Vec<FolderStatus>,
    pub files: u64,
    pub tags_pending: u64,
    pub tags_failed: u64,
    /// Files with a known id, per server.
    pub linked: BTreeMap<String, u64>,
    /// Files with a problem, per kind: "broken" (cannot be read), "damaged_tag", "no_tags", "no_length".
    pub issues: BTreeMap<String, u64>,
    pub progress: Progress,
}

/// One album directory as the catalogue knows it (see `Catalogue::album_facts`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumFact {
    /// Relative to its folder.
    pub dir: String,
    pub tracks: u32,
    pub formats: std::collections::BTreeSet<String>,
    pub min_kbps: Option<u32>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artist: Option<String>,
    pub release_group: Option<String>,
}

/// One file as Insights counts it (see `Catalogue::track_rows`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackRow {
    /// Relative to its folder; for the Music folder, the same text as a playlist line.
    pub rel: String,
    /// Upper case: "FLAC", "MP3".
    pub format: Option<String>,
    pub kbps: Option<u32>,
    pub duration_ms: Option<u64>,
    pub size: u64,
    /// (name, raw value), only the names in `TAGS`.
    pub tags: Vec<(String, String)>,
}

#[allow(dead_code)] // wired up with the Insights page (after the App split, decision 0011)
impl TrackRow {
    pub const TAGS: [&'static str; 9] =
        ["album", "albumartist", "albumartists", "artist", "artists", "title", "genre", "date", "originaldate"];

    /// The raw values of one tag.
    pub fn tag<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.tags.iter().filter(move |(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The artists of `field` (`artist` or `albumartist`), split per `rules`: from its list tag (`artists`,
    /// `albumartists`) when the file has one, since that names each artist apart while the text tag holds the credit
    /// as written ("King Tubby meets Lee 'Scratch' Perry", decision 0016); else from the text tag.
    pub fn artists(&self, rules: &Rules, field: &str) -> Vec<String> {
        let list = match field {
            "albumartist" => "albumartists",
            "artist" => "artists",
            other => other,
        };
        let from = if self.tag(list).next().is_some() { list } else { field };
        rules.values(from, &self.tag(from).map(str::to_string).collect::<Vec<_>>())
    }
}

pub struct Catalogue {
    db: Mutex<Connection>,
    /// Index = folder id; 0 is the playlist folder.
    folders: RwLock<Vec<PathBuf>>,
    progress: Mutex<Progress>,
    /// Asks the background loop for a pass now (settings changed, a playlist was edited…).
    poke: tokio::sync::Notify,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Catalogue {
    /// `file`: the database; `None` keeps it in memory (no data folder).
    pub fn open(file: Option<&Path>, folders: Vec<PathBuf>) -> Result<Self> {
        let conn = match file {
            Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
            None => Connection::open_in_memory()?,
        };
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA).context("creating the catalogue tables")?;
        for (table, column) in ADDED_COLUMNS {
            let name = column.split_whitespace().next().unwrap_or(column);
            let has: bool = conn
                .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?
                .exists([name])?;
            if !has {
                conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column}"))?;
            }
        }
        conn.execute_batch("CREATE INDEX IF NOT EXISTS files_by_track ON files (track);")?;
        let cat = Self { db: Mutex::new(conn), folders: RwLock::new(Vec::new()), progress: Mutex::default(), poke: Default::default() };
        // A better reader may read what an older one could not.
        if cat.meta("reader").as_deref() != Some(&tags::READER_VERSION.to_string()) {
            cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2", params![PENDING, FAILED])?;
            cat.set_meta("reader", &tags::READER_VERSION.to_string());
        }
        // Track ids are this catalogue's own: one made afresh numbers them anew, and what holds ids from the old one
        // (the sync's agreed lists) can tell by this.
        if cat.meta(GENERATION).is_none() {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            cat.set_meta(GENERATION, &now.to_string());
        }
        cat.set_folders(folders)?;
        Ok(cat)
    }

    /// Which catalogue this is: set when it is made, the same for as long as its track ids are.
    pub fn generation(&self) -> String {
        self.meta(GENERATION).unwrap_or_default()
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn folders(&self) -> Vec<PathBuf> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Use these folders (index = id). A folder whose path changed loses its rows, unless the same files are there
    /// under the new path (a mount moved, say `/music` to `/media/Music`): then it keeps them and nothing is read
    /// again. The others keep theirs.
    pub fn set_folders(&self, folders: Vec<PathBuf>) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let known: HashMap<i64, String> = {
            let mut st = tx.prepare("SELECT id, path FROM folders")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
        };
        let wanted: HashMap<i64, String> =
            folders.iter().enumerate().map(|(i, p)| (i as i64, p.to_string_lossy().into_owned())).collect();
        let moved = |id: i64, to: &str| -> Result<bool> {
            let mut st = tx.prepare(&format!("SELECT rel FROM files WHERE folder = ?1 AND {PRESENT} LIMIT 20"))?;
            let rels: Vec<String> = st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            Ok(!rels.is_empty() && rels.iter().all(|rel| Path::new(to).join(rel).is_file()))
        };
        for (id, path) in &known {
            let kept = match wanted.get(id) {
                Some(to) if to != path => moved(*id, to)?,
                _ => false,
            };
            if kept {
                tracing::info!("catalogue: {path} is now {}, same files", wanted[id]);
            }
            if wanted.get(id) != Some(path) && !kept {
                tx.execute(
                    "UPDATE tracks SET retired_at = ?2 WHERE id IN (SELECT track FROM files WHERE folder = ?1 AND track IS NOT NULL)",
                    params![id, now()],
                )?;
                for table in ["files", "tags", "source_ids"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1"), [id])?;
                }
                tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
            }
        }
        for (id, path) in &wanted {
            tx.execute("INSERT OR REPLACE INTO folders (id, path) VALUES (?1, ?2)", params![id, path])?;
        }
        tx.commit()?;
        drop(db);
        *self.folders.write().unwrap_or_else(|p| p.into_inner()) = folders;
        self.poke.notify_one();
        Ok(())
    }

    /// Ask for a pass now.
    pub fn poke(&self) {
        self.poke.notify_one();
    }

    fn set_doing(&self, doing: Option<String>) {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).doing = doing;
    }

    /// Walk every folder and record what is new, changed or gone. Returns problems (unreadable folders). Blocking.
    pub fn walk_all(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (id, folder) in self.folders().into_iter().enumerate() {
            match walk::walk(&folder) {
                Ok((found, skipped)) => {
                    if let Err(e) = self.record_walk(id as i64, &found) {
                        problems.push(format!("{}: {e:#}", folder.display()));
                    }
                    if skipped > 0 {
                        problems.push(format!("{}: {skipped} entries could not be read", folder.display()));
                    }
                }
                // Not readable: keep what we knew, say so.
                Err(e) => problems.push(format!("{} cannot be read: {e}", folder.display())),
            }
        }
        problems
    }

    fn record_walk(&self, folder: i64, found: &[walk::Found]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        // rel -> (size, mtime, gone); trashed rows are not walked (the trash is a hidden folder) and stay as they are.
        let known: HashMap<String, (i64, i64, bool)> = {
            let mut st = tx.prepare("SELECT rel, size, mtime, gone_at IS NOT NULL FROM files WHERE folder = ?1 AND trashed IS NULL")?;
            st.query_map([folder], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?.collect::<rusqlite::Result<_>>()?
        };
        {
            let mut upsert = tx.prepare(
                "INSERT INTO files (folder, rel, size, mtime, state) VALUES (?1, ?2, ?3, ?4, 0)
                 ON CONFLICT (folder, rel) DO UPDATE SET size = ?3, mtime = ?4, state = 0, error = NULL, gone_at = NULL",
            )?;
            let mut back = tx.prepare("UPDATE files SET gone_at = NULL WHERE folder = ?1 AND rel = ?2")?;
            for f in found {
                match known.get(&f.rel) {
                    Some(&(size, mtime, gone)) if (size, mtime) == (f.size as i64, f.mtime) => {
                        if gone {
                            back.execute(params![folder, f.rel])?;
                        }
                    }
                    _ => {
                        upsert.execute(params![folder, f.rel, f.size as i64, f.mtime])?;
                    }
                }
            }
        }
        // Missing now: gone, not deleted. It keeps its track id and tags until it is recognised elsewhere (a move made
        // outside Cratarr) or `GONE_KEEP_SECS` pass. Its server links go at once: they point at a file that is not there.
        let present: std::collections::HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        let now = now();
        for rel in known.iter().filter(|(r, k)| !k.2 && !present.contains(r.as_str())).map(|(r, _)| r) {
            tx.execute("UPDATE files SET gone_at = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, now])?;
            tx.execute("DELETE FROM source_ids WHERE folder = ?1 AND rel = ?2", params![folder, rel])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Read the tags of up to `n` files waiting for it. Returns how many were handled. Blocking.
    pub fn read_tags_batch(&self, n: usize) -> Result<usize> {
        let todo: Vec<(i64, String)> = {
            let db = self.db();
            let mut st = db.prepare(&format!("SELECT folder, rel FROM files WHERE state = ?1 AND {PRESENT} LIMIT ?2"))?;
            st.query_map(params![PENDING, n as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
        };
        let folders = self.folders();
        // Read outside the lock: this is the slow part.
        let results: Vec<(i64, String, Result<(tags::FileTags, Option<(tags::Issue, String)>)>)> = todo
            .into_iter()
            .map(|(folder, rel)| {
                let r = match folders.get(folder as usize) {
                    Some(root) => tags::read(&root.join(&rel)),
                    None => Err(anyhow::anyhow!("unknown folder")),
                };
                (folder, rel, r)
            })
            .collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        for (folder, rel, r) in &results {
            tx.execute("DELETE FROM tags WHERE folder = ?1 AND rel = ?2", params![folder, rel])?;
            match r {
                Ok((t, issue)) => {
                    tx.execute(
                        "UPDATE files SET state = ?3, format = ?4, bitrate = ?5, duration_ms = ?6, error = NULL, issue = ?7, issue_detail = ?8
                         WHERE folder = ?1 AND rel = ?2",
                        params![
                            folder,
                            rel,
                            READ,
                            t.format,
                            t.bitrate_kbps.map(i64::from),
                            t.duration_ms.map(|d| d as i64),
                            issue.as_ref().map(|(i, _)| i.code()),
                            issue.as_ref().map(|(_, d)| d.as_str())
                        ],
                    )?;
                    let mut ins = tx.prepare_cached("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
                    for (i, (raw, name, value)) in t.items.iter().enumerate() {
                        ins.execute(params![folder, rel, i as i64, raw, name, value])?;
                    }
                }
                Err(e) => {
                    tx.execute(
                        "UPDATE files SET state = ?3, error = ?4, issue = 'broken', issue_detail = ?4 WHERE folder = ?1 AND rel = ?2",
                        params![folder, rel, FAILED, tags::field_problem(&format!("{e:#}"))],
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(results.len())
    }

    /// Link a server's tracks to files. `stamp` marks them as seen in this pass. Blocking.
    pub fn record_links(&self, source: &str, library: &str, links: &[(String, i64, String)], stamp: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT OR REPLACE INTO source_ids (source, track_id, library, folder, rel, seen) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (id, folder, rel) in links {
                st.execute(params![source, id, library, folder, rel, stamp])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// After a full listing: forget this library's links not seen in it. Blocking.
    pub fn forget_unseen(&self, source: &str, library: &str, stamp: i64) -> Result<usize> {
        Ok(self.db().execute("DELETE FROM source_ids WHERE source = ?1 AND library = ?2 AND seen < ?3", params![source, library, stamp])?)
    }

    /// Whether a file is in the catalogue (used to find where a server's paths lie). Blocking.
    pub fn has_file(&self, folder: i64, rel: &str) -> bool {
        self.db()
            .query_row(&format!("SELECT 1 FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"), params![folder, rel], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    /// The server's track id for each file of a folder, `None` where not known (yet). Blocking.
    pub fn track_ids(&self, source: &str, folder: i64, rels: &[String]) -> Result<Vec<Option<String>>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT track_id FROM source_ids WHERE source = ?1 AND folder = ?2 AND rel = ?3 LIMIT 1")?;
        rels.iter()
            .map(|rel| Ok(st.query_row(params![source, folder, rel], |r| r.get::<_, String>(0)).optional()?))
            .collect()
    }

    /// One row per album directory in `folder`: track count, formats, lowest bitrate, and the album tags Incoming needs
    /// to recognise an album that is already in the library. Read-only; blocking.
    pub fn album_facts(&self, folder: i64) -> Result<Vec<AlbumFact>> {
        let db = self.db();
        let dir_of = |rel: &str| rel.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string());
        let mut by_dir: BTreeMap<String, AlbumFact> = BTreeMap::new();
        let mut st = db.prepare(&format!("SELECT rel, format, bitrate FROM files WHERE folder = ?1 AND {PRESENT}"))?;
        let rows = st.query_map(params![folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?)))?;
        for row in rows {
            let (rel, format, kbps) = row?;
            let dir = dir_of(&rel);
            let e = by_dir.entry(dir.clone()).or_insert_with(|| AlbumFact { dir, ..AlbumFact::default() });
            e.tracks += 1;
            if let Some(f) = format {
                e.formats.insert(f.to_uppercase());
            }
            if let Some(k) = kbps.filter(|k| *k > 0).map(|k| k as u32) {
                e.min_kbps = Some(e.min_kbps.map_or(k, |m| m.min(k)));
            }
        }
        let mut st = db.prepare(
            "SELECT rel, name, value FROM tags WHERE folder = ?1 AND name IN ('album', 'albumartist', 'artist', 'musicbrainz_releasegroupid') ORDER BY rel, n",
        )?;
        let rows = st.query_map(params![folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (rel, name, value) = row?;
            let Some(e) = by_dir.get_mut(&dir_of(&rel)) else { continue };
            let slot = match name.as_str() {
                "album" => &mut e.album,
                "albumartist" => &mut e.album_artist,
                "artist" => &mut e.artist,
                _ => &mut e.release_group,
            };
            if slot.is_none() && !value.trim().is_empty() {
                *slot = Some(value);
            }
        }
        Ok(by_dir.into_values().collect())
    }

    /// The audio files of one album folder (`dir`, relative to `folder`) with all their named tags, in path order:
    /// what Keep names the files from (decision 0013). Read-only; blocking.
    pub fn album_tags(&self, folder: i64, dir: &str) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let db = self.db();
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut st = db.prepare(&format!(
            "SELECT rel FROM files WHERE folder = ?1 AND substr(rel, 1, ?2) = ?3 AND {PRESENT} ORDER BY rel"
        ))?;
        let rels = st.query_map(params![folder, prefix.chars().count() as i64, prefix], |r| r.get::<_, String>(0))?;
        let mut index = std::collections::HashMap::new();
        for rel in rels {
            let rel = rel?;
            index.insert(rel.clone(), out.len());
            out.push((rel, Vec::new()));
        }
        let mut st = db.prepare(
            "SELECT rel, name, value FROM tags WHERE folder = ?1 AND substr(rel, 1, ?2) = ?3 AND name IS NOT NULL ORDER BY rel, n",
        )?;
        let rows = st.query_map(params![folder, prefix.chars().count() as i64, prefix], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (rel, name, value) = row?;
            if let Some(&i) = index.get(&rel) {
                out[i].1.push((name, value));
            }
        }
        Ok(out)
    }

    /// Every audio file of `folder` with its tags named in `names`, in path order: what a library's naming template is
    /// judged from (decision 0013). Read-only; blocking.
    pub fn folder_tags(&self, folder: i64, names: &[&str]) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let db = self.db();
        let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut index = std::collections::HashMap::new();
        let mut st = db.prepare(&format!("SELECT rel FROM files WHERE folder = ?1 AND {PRESENT} ORDER BY rel"))?;
        for rel in st.query_map(params![folder], |r| r.get::<_, String>(0))? {
            let rel = rel?;
            index.insert(rel.clone(), out.len());
            out.push((rel, Vec::new()));
        }
        let mut st = db.prepare("SELECT rel, name, value FROM tags WHERE folder = ?1 AND name IS NOT NULL ORDER BY rel, n")?;
        let rows = st.query_map(params![folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (rel, name, value) = row?;
            if names.iter().any(|n| n.eq_ignore_ascii_case(&name))
                && let Some(&i) = index.get(&rel)
            {
                out[i].1.push((name, value));
            }
        }
        Ok(out)
    }

    /// Every file in `folder` with the facts Insights counts: format, bitrate, length, size and the tags named in
    /// `TrackRow::TAGS` (raw values, not split). Read-only; blocking.
    pub fn track_rows(&self, folder: i64) -> Result<Vec<TrackRow>> {
        let db = self.db();
        let mut rows: Vec<TrackRow> = Vec::new();
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut st = db.prepare(&format!("SELECT rel, format, bitrate, duration_ms, size FROM files WHERE folder = ?1 AND {PRESENT}"))?;
        let files = st.query_map(params![folder], |r| {
            Ok(TrackRow {
                rel: r.get(0)?,
                format: r.get::<_, Option<String>>(1)?.map(|f| f.to_uppercase()),
                kbps: r.get::<_, Option<i64>>(2)?.filter(|k| *k > 0).map(|k| k as u32),
                duration_ms: r.get::<_, Option<i64>>(3)?.filter(|d| *d > 0).map(|d| d as u64),
                size: r.get::<_, i64>(4)?.max(0) as u64,
                tags: Vec::new(),
            })
        })?;
        for f in files {
            let f = f?;
            index.insert(f.rel.clone(), rows.len());
            rows.push(f);
        }
        let names = TrackRow::TAGS.map(|n| format!("'{n}'")).join(", ");
        let mut st = db.prepare(&format!("SELECT rel, name, value FROM tags WHERE folder = ?1 AND name IN ({names}) ORDER BY rel, n"))?;
        let tags = st.query_map(params![folder], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for t in tags {
            let (rel, name, value) = t?;
            if let Some(&i) = index.get(&rel) {
                rows[i].tags.push((name, value));
            }
        }
        Ok(rows)
    }

    /// Files with a problem, worst first ("broken", then damaged tags, …), at most `limit`. Blocking.
    pub fn problems(&self, limit: usize) -> Result<Vec<Problem>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT folder, rel, issue, issue_detail FROM files WHERE issue IS NOT NULL AND {PRESENT}
             ORDER BY CASE issue WHEN 'broken' THEN 0 WHEN 'damaged_tag' THEN 1 WHEN 'no_length' THEN 2 ELSE 3 END, rel LIMIT ?1"
        ))?;
        let rows = st.query_map([limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })?;
        rows.map(|r| {
            let (folder, rel, kind, detail) = r?;
            Ok(Problem { folder: folders.get(folder as usize).map(|p| p.display().to_string()).unwrap_or_default(), rel, kind, detail })
        })
        .collect()
    }

    /// Tests elsewhere: treat every file waiting for its tags as read (test files are not audio).
    #[cfg(test)]
    pub fn pretend_read_for_tests(&self) {
        self.db().execute("UPDATE files SET state = 1, duration_ms = 200000 WHERE state = 0", []).unwrap();
    }

    // ----- Track identity (decision 0011 §4) -----

    /// Give every identified file a track id: a new file that is a gone one moved (by the rules of
    /// `identity::match_moves`) takes its id; any other gets a new one. Files whose tags are not read yet wait. Gone
    /// files nobody claimed within `GONE_KEEP_SECS`, and trashed files whose trash was emptied, retire their ids.
    /// Returns (moves recognised, new ids). Blocking.
    pub fn identify(&self) -> Result<(usize, usize)> {
        let now = now();
        let folders = self.folders();
        let mut db = self.db();
        let tx = db.transaction()?;
        let facts = |sql: &str| -> Result<Vec<(i64, String, Option<i64>, Option<i64>)>> {
            let mut st = tx.prepare(sql)?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        let new = facts(&format!(
            "SELECT folder, rel, duration_ms, track FROM files WHERE track IS NULL AND state != {PENDING} AND {PRESENT}"
        ))?;
        let gone = facts("SELECT folder, rel, duration_ms, track FROM files WHERE gone_at IS NOT NULL AND track IS NOT NULL")?;
        let mut moved = 0;
        let mut claimed: std::collections::HashSet<(i64, String)> = std::collections::HashSet::new();
        let mut taken: std::collections::HashSet<usize> = std::collections::HashSet::new();
        if !new.is_empty() && !gone.is_empty() {
            let mut tag = tx.prepare_cached(
                "SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IN ('musicbrainz_trackid', 'title', 'artist') ORDER BY n",
            )?;
            let mut file_facts = |(folder, rel, ms, _): &(i64, String, Option<i64>, Option<i64>)| -> Result<identity::FileFacts> {
                let mut f = identity::FileFacts { name: identity::name_of(rel), duration_ms: *ms, ..Default::default() };
                for row in tag.query_map(params![folder, rel], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                    let (name, value) = row?;
                    let slot = match name.as_str() {
                        "musicbrainz_trackid" => &mut f.recording,
                        "title" => &mut f.title,
                        _ => &mut f.artist,
                    };
                    if slot.is_none() && !value.trim().is_empty() {
                        *slot = Some(identity::fold(&value));
                    }
                }
                Ok(f)
            };
            let new_facts = new.iter().map(&mut file_facts).collect::<Result<Vec<_>>>()?;
            let gone_facts = gone.iter().map(&mut file_facts).collect::<Result<Vec<_>>>()?;
            drop(tag);
            for (n, g) in identity::match_moves(&new_facts, &gone_facts) {
                let ((nf, nr, _, _), (gf, gr, _, track)) = (&new[n], &gone[g]);
                tx.execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![nf, nr, track])?;
                for table in ["files", "tags"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![gf, gr])?;
                }
                claimed.insert((*nf, nr.clone()));
                taken.insert(g);
                moved += 1;
            }
        }
        let mut fresh = 0;
        {
            let mut born = tx.prepare_cached("INSERT INTO tracks (born) VALUES (?1)")?;
            let mut give = tx.prepare_cached("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2")?;
            for (folder, rel, _, _) in new.iter().filter(|(f, r, _, _)| !claimed.contains(&(*f, r.clone()))) {
                born.execute([now])?;
                give.execute(params![folder, rel, tx.last_insert_rowid()])?;
                fresh += 1;
            }
        }
        // Retire: gone too long, or trashed and the trash emptied.
        let mut retire: Vec<(i64, String)> = gone
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken.contains(i))
            .map(|(_, (f, r, _, _))| (*f, r.clone()))
            .collect();
        {
            let mut st = tx.prepare("SELECT folder, rel FROM files WHERE gone_at IS NOT NULL AND gone_at >= ?1")?;
            let recent: std::collections::HashSet<(i64, String)> =
                st.query_map([now - GONE_KEEP_SECS], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            retire.retain(|k| !recent.contains(k));
            let mut st = tx.prepare("SELECT DISTINCT folder, trashed FROM files WHERE trashed IS NOT NULL")?;
            let trashed: Vec<(i64, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            for (folder, token) in trashed {
                let still = folders.get(folder as usize).is_some_and(|f| f.join(crate::trash::TRASH_DIR).join(&token).exists());
                if !still {
                    let mut st = tx.prepare("SELECT rel FROM files WHERE folder = ?1 AND trashed = ?2")?;
                    let rels: Vec<String> = st.query_map(params![folder, token], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                    retire.extend(rels.into_iter().map(|r| (folder, r)));
                }
            }
        }
        for (folder, rel) in &retire {
            tx.execute(
                "UPDATE tracks SET retired_at = ?3 WHERE id = (SELECT track FROM files WHERE folder = ?1 AND rel = ?2)",
                params![folder, rel, now],
            )?;
            for table in ["files", "tags", "source_ids"] {
                tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![folder, rel])?;
            }
        }
        tx.commit()?;
        Ok((moved, fresh))
    }

    /// An album folder (or file) Cratarr moved into the trash of its folder: its rows follow it there, keep their
    /// track ids, and are marked trashed with the trash token. Their server links go. Blocking.
    pub fn mark_trashed(&self, folder: i64, rel: &str, token: &str) -> Result<usize> {
        self.rekey(folder, rel, folder, &format!("{}/{token}", crate::trash::TRASH_DIR), Some(token))
    }

    /// Undo `mark_trashed`: the rows go back to where the files are again. Blocking.
    pub fn unmark_trashed(&self, folder: i64, rel: &str, token: &str) -> Result<usize> {
        self.rekey(folder, &format!("{}/{token}", crate::trash::TRASH_DIR), folder, rel, None)
    }

    /// Files Cratarr moved itself (adopting an album, say): their rows follow, track ids and tags kept, so nothing is
    /// read again and no playlist entry is lost. Blocking.
    #[allow(dead_code)] // used by Keep (adopt) in Incoming, decision 0011
    pub fn move_files(&self, from_folder: i64, from: &str, to_folder: i64, to: &str) -> Result<usize> {
        self.rekey(from_folder, from, to_folder, to, None)
    }

    /// The rows under `from` (a file, or a folder and everything in it) now live under `to`. `trashed` is set on them.
    fn rekey(&self, from_folder: i64, from: &str, to_folder: i64, to: &str, trashed: Option<&str>) -> Result<usize> {
        let (from, to) = (from.trim_end_matches('/'), to.trim_end_matches('/'));
        let mut db = self.db();
        let tx = db.transaction()?;
        let rels: Vec<String> = {
            let mut st = tx.prepare("SELECT rel FROM files WHERE folder = ?1 AND (rel = ?2 OR substr(rel, 1, ?3) = ?4)")?;
            let prefix = format!("{from}/");
            st.query_map(params![from_folder, from, prefix.chars().count() as i64, prefix], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for rel in &rels {
            let new_rel = format!("{to}{}", &rel[from.len()..]);
            tx.execute("DELETE FROM source_ids WHERE folder = ?1 AND rel = ?2", params![from_folder, rel])?;
            tx.execute(
                "UPDATE files SET folder = ?3, rel = ?4, trashed = ?5, gone_at = NULL WHERE folder = ?1 AND rel = ?2",
                params![from_folder, rel, to_folder, new_rel, trashed],
            )?;
            tx.execute("UPDATE tags SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2", params![from_folder, rel, to_folder, new_rel])?;
        }
        tx.commit()?;
        Ok(rels.len())
    }

    /// Rows Cratarr moved into the trash of another folder (Replace: old files from Music into Incoming's trash):
    /// they follow, marked trashed with the trash token, so they retire when the trash is emptied. Blocking.
    pub fn move_into_trash(&self, from: (i64, &str), to: (i64, &str), token: &str) -> Result<usize> {
        self.rekey(from.0, from.1, to.0, to.1, Some(token))
    }

    /// Give files these track ids, all or none (Replace: a new file takes over an old one's id and the old file lets go
    /// of it; Undo gives them back). `None` lets go. Absolute values, so safe to repeat. Blocking.
    pub fn set_tracks(&self, files: &[((i64, String), Option<i64>)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        for ((folder, rel), track) in files {
            tx.execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, track])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Whether the catalogue has a row for this path, present or gone (a row Cratarr moves must not land on one).
    /// Blocking.
    pub fn has_row(&self, folder: i64, rel: &str) -> bool {
        self.db().prepare_cached("SELECT 1 FROM files WHERE folder = ?1 AND rel = ?2").and_then(|mut st| st.exists(params![folder, rel])).unwrap_or(false)
    }

    /// The length of each audio file in an album folder (`dir`, relative to `folder`), by path. Read-only; blocking.
    pub fn album_lengths(&self, folder: i64, dir: &str) -> Result<std::collections::HashMap<String, u64>> {
        let db = self.db();
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let mut st = db.prepare(&format!(
            "SELECT rel, duration_ms FROM files WHERE folder = ?1 AND substr(rel, 1, ?2) = ?3 AND duration_ms > 0 AND {PRESENT}"
        ))?;
        let rows = st.query_map(params![folder, prefix.chars().count() as i64, prefix], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The track id of a file, once it has one. Blocking.
    #[allow(dead_code)] // used by the playlist module, decision 0011
    pub fn track_of(&self, folder: i64, rel: &str) -> Option<i64> {
        self.db()
            .query_row(&format!("SELECT track FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"), params![folder, rel], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    /// The track a server's item id stands for (decision 0012), when the item is linked to a file the catalogue
    /// counts. Blocking.
    pub fn track_of_source(&self, source: &str, item: &str) -> Option<i64> {
        self.db()
            .query_row(
                &format!(
                    "SELECT f.track FROM source_ids s JOIN files f ON f.folder = s.folder AND f.rel = s.rel
                     WHERE s.source = ?1 AND s.track_id = ?2 AND {}",
                    PRESENT.replace("gone_at", "f.gone_at").replace("trashed", "f.trashed")
                ),
                params![source, item],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    /// A track's item id on a server (decision 0012), when it is linked. Blocking.
    pub fn source_id_of(&self, source: &str, track: i64) -> Option<String> {
        self.db()
            .query_row(
                &format!(
                    "SELECT s.track_id FROM files f JOIN source_ids s ON s.folder = f.folder AND s.rel = f.rel
                     WHERE f.track = ?2 AND s.source = ?1 AND {} LIMIT 1",
                    PRESENT.replace("gone_at", "f.gone_at").replace("trashed", "f.trashed")
                ),
                params![source, track],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Where a track is now: its folder id and relative path; `None` when it is gone, trashed or retired. Blocking.
    #[allow(dead_code)] // used by the playlist module, decision 0011
    pub fn file_of(&self, track: i64) -> Option<(i64, String)> {
        self.db()
            .query_row(&format!("SELECT folder, rel FROM files WHERE track = ?1 AND {PRESENT}"), [track], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()
            .flatten()
    }

    fn meta(&self, key: &str) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_meta(&self, key: &str, value: &str) {
        let _ = self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![key, value]);
    }

    pub fn status(&self) -> Result<Status> {
        let db = self.db();
        let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<u64> { Ok(db.query_row(sql, p, |r| r.get::<_, i64>(0))? as u64) };
        let folders = self
            .folders()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                Ok(FolderStatus {
                    path: p.display().to_string(),
                    files: count(&format!("SELECT COUNT(*) FROM files WHERE folder = ?1 AND {PRESENT}"), &[&(i as i64)])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut linked = BTreeMap::new();
        {
            let mut st = db.prepare("SELECT source, COUNT(*) FROM source_ids GROUP BY source")?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (s, n) = row?;
                linked.insert(s, n as u64);
            }
        }
        let mut issues = BTreeMap::new();
        {
            let mut st = db.prepare(&format!("SELECT issue, COUNT(*) FROM files WHERE issue IS NOT NULL AND {PRESENT} GROUP BY issue"))?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (k, n) = row?;
                issues.insert(k, n as u64);
            }
        }
        Ok(Status {
            issues,
            folders,
            files: count(&format!("SELECT COUNT(*) FROM files WHERE {PRESENT}"), &[])?,
            tags_pending: count(&format!("SELECT COUNT(*) FROM files WHERE state = ?1 AND {PRESENT}"), &[&PENDING])?,
            tags_failed: count(&format!("SELECT COUNT(*) FROM files WHERE state = ?1 AND {PRESENT}"), &[&FAILED])?,
            linked,
            progress: self.progress.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        })
    }
}

/// Where a server's paths lie in each folder: the prefix to strip, per folder id. Found from the catalogue's own
/// files (the same idea as `placement::find_root`, without touching the disk).
#[derive(Default)]
struct Roots(Vec<(i64, String)>);

impl Roots {
    fn locate(&mut self, cat: &Catalogue, path: &str) -> Option<(i64, String)> {
        let norm = path.replace('\\', "/");
        for (folder, root) in &self.0 {
            if let Some(rel) = norm.strip_prefix(root.as_str()) {
                if crate::placement::safe_relative(rel) {
                    return Some((*folder, rel.to_string()));
                }
            }
        }
        // Unknown so far: strip leading folders until the rest is a catalogued file.
        let absolute = norm.starts_with('/');
        let segs: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
        for folder in 0..cat.folders().len() as i64 {
            for i in 0..segs.len().saturating_sub(1) {
                let rel = segs[i..].join("/");
                if crate::placement::safe_relative(&rel) && cat.has_file(folder, &rel) {
                    let root = format!("{}{}/", if absolute { "/" } else { "" }, segs[..i].join("/"));
                    self.0.push((folder, root));
                    return Some((folder, rel));
                }
            }
        }
        None
    }
}

/// One server: link its tracks to files, a page at a time. A full listing when due, else only what changed.
async fn link_source(cat: &Arc<Catalogue>, id: &str, source: &Arc<dyn Source>) -> Result<()> {
    if !source.lists_tracks() {
        return Ok(());
    }
    let libraries = source.libraries().await?;
    let started = now();
    let last_full: i64 = cat.meta(&format!("full:{id}")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let last_pass: i64 = cat.meta(&format!("pass:{id}")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let full = started - last_full >= FULL_EVERY_SECS;
    let since = (!full).then_some(last_pass);
    let mut roots = Roots::default();
    for lib in &libraries {
        let mut start = 0;
        loop {
            let page = source.track_paths(&lib.id, since, start, PAGE).await?;
            let n = page.items.len();
            let cat2 = cat.clone();
            let library = lib.id.clone();
            let source_id = id.to_string();
            let items = page.items;
            // Resolving touches the database; keep it off the async threads.
            let mut moved_roots = std::mem::take(&mut roots);
            moved_roots = tokio::task::spawn_blocking(move || -> Result<Roots> {
                let links: Vec<(String, i64, String)> =
                    items.into_iter().filter_map(|(tid, path)| moved_roots.locate(&cat2, &path).map(|(f, r)| (tid, f, r))).collect();
                cat2.record_links(&source_id, &library, &links, started)?;
                Ok(moved_roots)
            })
            .await??;
            roots = moved_roots;
            start += n;
            if n < PAGE || start >= page.total {
                break;
            }
        }
        if full {
            let (cat2, sid, lid) = (cat.clone(), id.to_string(), lib.id.clone());
            tokio::task::spawn_blocking(move || cat2.forget_unseen(&sid, &lid, started)).await??;
        }
    }
    if full {
        cat.set_meta(&format!("full:{id}"), &started.to_string());
    }
    cat.set_meta(&format!("pass:{id}"), &started.to_string());
    Ok(())
}

/// The background loop: a pass now, then every few minutes or when poked.
pub async fn run(cat: Arc<Catalogue>, sources: impl Fn() -> Vec<(String, Arc<dyn Source>)> + Send + 'static) {
    loop {
        let mut problems = Vec::new();
        cat.set_doing(Some("walking the folders".into()));
        let c = cat.clone();
        match tokio::task::spawn_blocking(move || c.walk_all()).await {
            Ok(p) => problems.extend(p),
            Err(e) => problems.push(format!("walk: {e}")),
        }
        cat.set_doing(Some("reading tags".into()));
        loop {
            let c = cat.clone();
            match tokio::task::spawn_blocking(move || c.read_tags_batch(TAG_BATCH)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => tokio::task::yield_now().await,
                Ok(Err(e)) => {
                    problems.push(format!("tags: {e:#}"));
                    break;
                }
                Err(e) => {
                    problems.push(format!("tags: {e}"));
                    break;
                }
            }
        }
        let c = cat.clone();
        match tokio::task::spawn_blocking(move || c.identify()).await {
            Ok(Ok((moved, _))) if moved > 0 => tracing::info!("catalogue: recognised {moved} moved file(s)"),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => problems.push(format!("track ids: {e:#}")),
            Err(e) => problems.push(format!("track ids: {e}")),
        }
        for (id, source) in sources() {
            cat.set_doing(Some(format!("linking {id}")));
            if let Err(e) = link_source(&cat, &id, &source).await {
                tracing::warn!("catalogue: linking {id}: {e:#}");
                problems.push(format!("{id}: {e:#}"));
            }
        }
        {
            let mut p = cat.progress.lock().unwrap_or_else(|p| p.into_inner());
            p.doing = None;
            p.last_pass = Some(now());
            p.problems = problems;
        }
        tokio::select! {
            _ = tokio::time::sleep(PASS_EVERY) => {}
            _ = cat.poke.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music() -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-cat-{}-{n}", std::process::id()));
        std::fs::create_dir_all(d.join("A - B (2020)")).unwrap();
        std::fs::write(d.join("A - B (2020)/01 One.mp3"), b"not really an mp3").unwrap();
        std::fs::write(d.join("A - B (2020)/02 Two.flac"), b"nor a flac").unwrap();
        d
    }

    #[test]
    fn a_walk_records_new_changed_and_gone_files() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        assert!(cat.walk_all().is_empty());
        let s = cat.status().unwrap();
        assert_eq!((s.files, s.tags_pending), (2, 2));
        // Unreadable files are recorded as failed, not retried forever.
        assert_eq!(cat.read_tags_batch(10).unwrap(), 2);
        assert_eq!(cat.read_tags_batch(10).unwrap(), 0);
        let s = cat.status().unwrap();
        assert_eq!(s.tags_failed, 2);
        assert_eq!(s.issues.get("broken"), Some(&2), "unreadable files are classified as broken");
        assert_eq!(cat.problems(10).unwrap().len(), 2);
        // A changed file is read again; a removed one is forgotten.
        std::fs::write(d.join("A - B (2020)/01 One.mp3"), b"changed and longer than before").unwrap();
        std::fs::remove_file(d.join("A - B (2020)/02 Two.flac")).unwrap();
        cat.walk_all();
        let s = cat.status().unwrap();
        assert_eq!((s.files, s.tags_pending), (1, 1));
        std::fs::remove_dir_all(&d).ok();
    }

    /// Pretend the tags of every file waiting were read: a length (the test files are not audio).
    fn read_as_audio(cat: &Catalogue) {
        cat.db().execute("UPDATE files SET state = 1, duration_ms = 200000 WHERE state = 0", []).unwrap();
    }

    #[test]
    fn a_catalogue_keeps_its_generation_and_a_new_one_gets_its_own() {
        let d = std::env::temp_dir().join(format!("cratarr-generation-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("catalogue.sqlite");
        let first = Catalogue::open(Some(&file), Vec::new()).unwrap().generation();
        assert!(!first.is_empty());
        assert_eq!(Catalogue::open(Some(&file), Vec::new()).unwrap().generation(), first);
        std::fs::remove_dir_all(&d).unwrap();
        std::fs::create_dir_all(&d).unwrap();
        assert_ne!(Catalogue::open(Some(&file), Vec::new()).unwrap().generation(), first);
        std::fs::remove_dir_all(&d).ok();
    }

    fn retired(cat: &Catalogue, track: i64) -> bool {
        cat.db().query_row("SELECT retired_at FROM tracks WHERE id = ?1", [track], |r| r.get::<_, Option<i64>>(0)).unwrap().is_some()
    }

    #[test]
    fn every_identified_file_gets_a_lasting_track_id() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        assert_eq!(cat.identify().unwrap(), (0, 0), "tags not read yet: no ids");
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (0, 2));
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020)/01 One.mp3".to_string())));
        assert_eq!(cat.identify().unwrap(), (0, 0), "ids are given once");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_folder_renamed_outside_cratarr_keeps_its_track_ids() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        let two = cat.track_of(0, "A - B (2020)/02 Two.flac").unwrap();
        std::fs::rename(d.join("A - B (2020)"), d.join("A - B (2020) [FLAC]")).unwrap();
        cat.walk_all();
        assert_eq!(cat.status().unwrap().files, 2, "gone files are not counted twice");
        assert_eq!(cat.file_of(one), None, "gone, waiting");
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (2, 0), "same file names and lengths: recognised, no new ids");
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020) [FLAC]/01 One.mp3".to_string())));
        assert_eq!(cat.track_of(0, "A - B (2020) [FLAC]/02 Two.flac"), Some(two));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_that_comes_back_keeps_its_id_and_one_that_never_does_is_retired() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        let away = d.join("one.away");
        std::fs::rename(d.join("A - B (2020)/01 One.mp3"), &away).unwrap();
        cat.walk_all();
        std::fs::rename(&away, d.join("A - B (2020)/01 One.mp3")).unwrap();
        cat.walk_all();
        assert_eq!(cat.track_of(0, "A - B (2020)/01 One.mp3"), Some(one), "back where it was: same id, tags kept");
        std::fs::remove_file(d.join("A - B (2020)/01 One.mp3")).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
        cat.identify().unwrap();
        assert!(retired(&cat, one), "unclaimed for a day: retired");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn trashed_files_keep_their_ids_until_the_trash_is_emptied() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        let token = crate::trash::trash(&d, "A - B (2020)", 1_000).unwrap();
        assert_eq!(cat.mark_trashed(0, "A - B (2020)", &token).unwrap(), 2);
        cat.walk_all();
        cat.identify().unwrap();
        assert_eq!(cat.status().unwrap().files, 0, "trashed files are not counted");
        assert_eq!(cat.file_of(one), None);
        assert!(!retired(&cat, one), "still in the trash: not retired");
        // Undo: back, same ids.
        crate::trash::restore(&d, &token).unwrap();
        assert_eq!(cat.unmark_trashed(0, "A - B (2020)", &token).unwrap(), 2);
        cat.walk_all();
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020)/01 One.mp3".to_string())));
        // Trashed again and the trash emptied: retired.
        let token = crate::trash::trash(&d, "A - B (2020)", 2_000).unwrap();
        cat.mark_trashed(0, "A - B (2020)", &token).unwrap();
        crate::trash::purge(&d, 2_000 + 30 * 86_400, 1).unwrap();
        cat.identify().unwrap();
        assert!(retired(&cat, one));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn moves_and_replacements_made_by_cratarr_keep_track_ids() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(1, "A - B (2020)/01 One.mp3").unwrap();
        // Adopt: the album moves from folder 1 to folder 0 under a new name.
        std::fs::rename(b.join("A - B (2020)"), a.join("A - B (2020) adopted")).unwrap();
        assert_eq!(cat.move_files(1, "A - B (2020)", 0, "A - B (2020) adopted").unwrap(), 2);
        cat.walk_all();
        assert_eq!(cat.identify().unwrap(), (0, 0), "nothing new, nothing recognised: the rows moved");
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020) adopted/01 One.mp3".to_string())));
        // Replace: a new file takes the old one's id.
        std::fs::write(a.join("A - B (2020) adopted/01 One.flac"), b"flac now").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let (old, new) = ((0, "A - B (2020) adopted/01 One.mp3".to_string()), (0, "A - B (2020) adopted/01 One.flac".to_string()));
        let own = cat.track_of(0, &new.1);
        cat.set_tracks(&[(old.clone(), None), (new.clone(), Some(one))]).unwrap();
        assert_eq!(cat.file_of(one), Some(new.clone()));
        // Undo: each its own again.
        cat.set_tracks(&[(new.clone(), own), (old.clone(), Some(one))]).unwrap();
        assert_eq!(cat.file_of(one), Some(old));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn an_unreadable_folder_keeps_its_rows() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        std::fs::rename(&d, d.with_extension("gone")).unwrap();
        let problems = cat.walk_all();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(cat.status().unwrap().files, 2, "an unmounted folder must not look emptied");
        std::fs::remove_dir_all(d.with_extension("gone")).ok();
    }

    #[test]
    fn server_paths_are_linked_to_files_and_unseen_links_forgotten() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let mut roots = Roots::default();
        let a = roots.locate(&cat, "/data/media/Music/A - B (2020)/01 One.mp3").unwrap();
        assert_eq!(a, (0, "A - B (2020)/01 One.mp3".to_string()));
        assert_eq!(roots.0, vec![(0, "/data/media/Music/".to_string())], "the root is learnt once");
        assert!(roots.locate(&cat, "/data/media/lidarr-music/X/01.mp3").is_none());
        cat.record_links("jellyfin", "lib", &[("t1".into(), 0, a.1.clone())], 100).unwrap();
        let ids = cat.track_ids("jellyfin", 0, &[a.1.clone(), "nope.mp3".into()]).unwrap();
        assert_eq!(ids, vec![Some("t1".to_string()), None]);
        assert_eq!(cat.forget_unseen("jellyfin", "lib", 101).unwrap(), 1);
        assert_eq!(cat.track_ids("jellyfin", 0, &[a.1]).unwrap(), vec![None]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn changing_a_folder_drops_only_its_rows() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        assert_eq!(cat.status().unwrap().files, 4);
        cat.set_folders(vec![a.clone()]).unwrap();
        assert_eq!(cat.status().unwrap().files, 2);
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn a_folder_that_moved_with_its_files_keeps_its_rows() {
        let a = music();
        let cat = Catalogue::open(None, vec![a.clone()]).unwrap();
        cat.walk_all();
        let moved = a.with_extension("moved");
        std::fs::rename(&a, &moved).unwrap();
        cat.set_folders(vec![moved.clone()]).unwrap();
        assert_eq!(cat.status().unwrap().files, 2, "same files under the new path: nothing to read again");
        let empty = moved.with_extension("empty");
        std::fs::create_dir_all(&empty).unwrap();
        cat.set_folders(vec![empty.clone()]).unwrap();
        assert_eq!(cat.status().unwrap().files, 0, "different files: the rows go");
        std::fs::remove_dir_all(&moved).ok();
        std::fs::remove_dir_all(&empty).ok();
    }
}
