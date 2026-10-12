//! The catalogue (decision 0008): every audio file in the playlist folder and the extra folders, its tags as stored,
//! and each server's id for it. Derived data in one SQLite file in the data folder (or in memory without one):
//! deleting it costs only a refill.
//!
//! Passes run in the background (`run`): walk the folders (size and modification time decide what changed), read
//! the tags of new and changed files a batch at a time, then ask every server for its tracks (paged, then only what
//! changed) and link each to a file. A folder that cannot be read keeps its rows and reports the error: an
//! unmounted folder must never look like "everything was deleted".

mod id3;
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

const SCHEMA: &str = "
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
";

/// Columns added after the first schema; added to an existing database when missing.
const ADDED_COLUMNS: [(&str, &str); 2] = [("files", "issue TEXT"), ("files", "issue_detail TEXT")];

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
        let cat = Self { db: Mutex::new(conn), folders: RwLock::new(Vec::new()), progress: Mutex::default(), poke: Default::default() };
        // A better reader may read what an older one could not.
        if cat.meta("reader").as_deref() != Some(&tags::READER_VERSION.to_string()) {
            cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2", params![PENDING, FAILED])?;
            cat.set_meta("reader", &tags::READER_VERSION.to_string());
        }
        cat.set_folders(folders)?;
        Ok(cat)
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
            let mut st = tx.prepare("SELECT rel FROM files WHERE folder = ?1 LIMIT 20")?;
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
        let known: HashMap<String, (i64, i64)> = {
            let mut st = tx.prepare("SELECT rel, size, mtime FROM files WHERE folder = ?1")?;
            st.query_map([folder], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))?.collect::<rusqlite::Result<_>>()?
        };
        {
            let mut upsert = tx.prepare(
                "INSERT INTO files (folder, rel, size, mtime, state) VALUES (?1, ?2, ?3, ?4, 0)
                 ON CONFLICT (folder, rel) DO UPDATE SET size = ?3, mtime = ?4, state = 0, error = NULL",
            )?;
            for f in found {
                if known.get(&f.rel) != Some(&(f.size as i64, f.mtime)) {
                    upsert.execute(params![folder, f.rel, f.size as i64, f.mtime])?;
                }
            }
        }
        let present: std::collections::HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        for rel in known.keys().filter(|r| !present.contains(r.as_str())) {
            for table in ["files", "tags", "source_ids"] {
                tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![folder, rel])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Read the tags of up to `n` files waiting for it. Returns how many were handled. Blocking.
    pub fn read_tags_batch(&self, n: usize) -> Result<usize> {
        let todo: Vec<(i64, String)> = {
            let db = self.db();
            let mut st = db.prepare("SELECT folder, rel FROM files WHERE state = ?1 LIMIT ?2")?;
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
            .query_row("SELECT 1 FROM files WHERE folder = ?1 AND rel = ?2", params![folder, rel], |_| Ok(()))
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
        let mut st = db.prepare("SELECT rel, format, bitrate FROM files WHERE folder = ?1")?;
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

    /// Files with a problem, worst first ("broken", then damaged tags, …), at most `limit`. Blocking.
    pub fn problems(&self, limit: usize) -> Result<Vec<Problem>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(
            "SELECT folder, rel, issue, issue_detail FROM files WHERE issue IS NOT NULL
             ORDER BY CASE issue WHEN 'broken' THEN 0 WHEN 'damaged_tag' THEN 1 WHEN 'no_length' THEN 2 ELSE 3 END, rel LIMIT ?1",
        )?;
        let rows = st.query_map([limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })?;
        rows.map(|r| {
            let (folder, rel, kind, detail) = r?;
            Ok(Problem { folder: folders.get(folder as usize).map(|p| p.display().to_string()).unwrap_or_default(), rel, kind, detail })
        })
        .collect()
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
                Ok(FolderStatus { path: p.display().to_string(), files: count("SELECT COUNT(*) FROM files WHERE folder = ?1", &[&(i as i64)])? })
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
            let mut st = db.prepare("SELECT issue, COUNT(*) FROM files WHERE issue IS NOT NULL GROUP BY issue")?;
            for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
                let (k, n) = row?;
                issues.insert(k, n as u64);
            }
        }
        Ok(Status {
            issues,
            folders,
            files: count("SELECT COUNT(*) FROM files", &[])?,
            tags_pending: count("SELECT COUNT(*) FROM files WHERE state = ?1", &[&PENDING])?,
            tags_failed: count("SELECT COUNT(*) FROM files WHERE state = ?1", &[&FAILED])?,
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
