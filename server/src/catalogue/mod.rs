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
pub mod tags;
pub mod walk;

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
/// Tracks asked of a server per page (measured on Jellyfin 10.11: 500 in about 5 s; on 2026-10-14, busy, 500 took
/// more than the 20 s a request may take, so 250).
const PAGE: usize = 250;
/// Between passes when nothing asks for one sooner.
const PASS_EVERY: Duration = Duration::from_secs(300);
/// A full listing of each server library at least this often, to forget tracks the server dropped.
const FULL_EVERY_SECS: i64 = 24 * 3600;

// Temporary storage in memory: set where the connection is opened (crate::db).
// journal_size_limit: a write-ahead log is folded in as it goes, but its file keeps the size of the largest burst (a full
// pass, a vacuum: 86 MB seen on the NAS) unless it is cut back after each checkpoint; 8 MB.
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS folders (id INTEGER PRIMARY KEY, path TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  state INTEGER NOT NULL DEFAULT 0, format TEXT, bitrate INTEGER, duration_ms INTEGER, error TEXT,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
-- The cover files beside the audio (0.9.7), as the last walk found them: one changed builds the library again, so its
-- album's cover stamp moves.
CREATE TABLE IF NOT EXISTS cover_files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS tags (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, n INTEGER NOT NULL, raw TEXT NOT NULL, name TEXT, value TEXT NOT NULL,
  PRIMARY KEY (folder, rel, n)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS source_ids (
  source TEXT NOT NULL, track_id TEXT NOT NULL, library TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL,
  seen INTEGER NOT NULL, PRIMARY KEY (source, track_id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS source_ids_by_file ON source_ids (folder, rel);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tracks (id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER);
CREATE TABLE IF NOT EXISTS analysis (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, version INTEGER NOT NULL,
  duration_secs REAL, loudness_lufs REAL, track_gain_db REAL, track_peak REAL, bpm REAL, bpm_confidence REAL,
  waveform BLOB, blocks TEXT, analysed_at INTEGER NOT NULL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
-- The analysis' large parts, read only for an album's gain and the seekbar's waveform: apart from the small numbers
-- every list reads (decision 0035, amendment 2026-10-17), so those stay a few pages (as one table with the 512-byte
-- waveform and the gating blocks, its 30,000 rows overflowed: 132 MB, read through by every scan).
CREATE TABLE IF NOT EXISTS analysis_shape (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, waveform BLOB, blocks BLOB,
  PRIMARY KEY (folder, rel));
-- AudioMuse's analysis (decision 0054), by track: taken in whole from the PC where AudioMuse runs.
CREATE TABLE IF NOT EXISTS audiomuse (
  track INTEGER PRIMARY KEY, energy REAL, key TEXT, style TEXT, moods TEXT, embedding BLOB);
-- Beat this!'s beats and downbeats (decision 0049, amended), by file as it was heard (its size and mtime): from the PC's
-- pass over the library or the NAS's over new music. Milliseconds, four bytes each, little-endian.
CREATE TABLE IF NOT EXISTS beat_this (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  beats BLOB NOT NULL, downbeats BLOB NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
";

/// A file's beats and downbeats by Beat this! (decision 0049, amended), as heard: milliseconds from the start.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatThisRow {
    pub folder: i64,
    pub rel: String,
    pub size: u64,
    pub mtime: i64,
    pub beats: Vec<u32>,
    pub downbeats: Vec<u32>,
}

/// A track's AudioMuse analysis as kept (decision 0054): AudioMuse's own words for it (`app::audiomuse` reads them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioMuseRow {
    pub track: i64,
    pub energy: Option<f64>,
    /// "D minor".
    pub key: Option<String>,
    /// AudioMuse's style tags with their scores, as it writes them.
    pub style: Option<String>,
    /// CLAP's moods with their scores, as AudioMuse writes them.
    pub moods: Option<String>,
    /// The similarity vector: little-endian 32-bit floats.
    pub embedding: Option<Vec<u8>>,
}

fn audiomuse_row(r: &rusqlite::Row) -> rusqlite::Result<AudioMuseRow> {
    Ok(AudioMuseRow { track: r.get(0)?, energy: r.get(1)?, key: r.get(2)?, style: r.get(3)?, moods: r.get(4)?, embedding: r.get(5)? })
}

/// A file the catalogue counts: neither gone (no longer where it was, waiting to be recognised elsewhere) nor in the
/// trash.
/// A file that counts: in a folder in use (a parked folder's are negative, see `set_folders`), there, not in the trash.
const PRESENT: &str = "folder >= 0 AND gone_at IS NULL AND trashed IS NULL";
/// The same, with the files table named `f` (in a join with `source_ids`, which has a folder too).
const PRESENT_F: &str = "f.folder >= 0 AND f.gone_at IS NULL AND f.trashed IS NULL";

/// How long a folder out of use keeps its rows (parked) before they are forgotten.
const PARKED_KEEP_SECS: i64 = 30 * 24 * 3600;

/// The meta key of `Catalogue::generation`.
const GENERATION: &str = "generation";

/// How long a gone file waits to be recognised as moved before its track id is retired (decision 0011 §4).
const GONE_KEEP_SECS: i64 = 24 * 3600;

/// Columns added after the first schema; added to an existing database when missing.
const ADDED_COLUMNS: [(&str, &str); 11] = [
    ("files", "issue TEXT"),
    ("files", "issue_detail TEXT"),
    // Decision 0011: the Cratarr track id, when the file was last seen missing, and the trash token while trashed.
    ("files", "track INTEGER"),
    ("files", "gone_at INTEGER"),
    ("files", "trashed TEXT"),
    // Decision 0035: whether the analysis decoded the whole stream (NULL: not known, analysed before it was recorded).
    ("analysis", "complete INTEGER"),
    // Decision 0046: the track's beats (`analysis::Beats`; empty: no tempo; NULL: not worked out yet).
    ("analysis_shape", "beats BLOB"),
    // Decision 0046, amended: the sound's signals for the visuals (`analysis::signals::Signals`; the column kept its first
    // name), kept only once the visuals asked for them.
    ("analysis_shape", "bands BLOB"),
    // Decision 0049, amended: whose the kept beats are (NULL: the analysis' own grid; 1: Beat this!'s).
    ("analysis_shape", "beats_by INTEGER"),
    // The waveform in three bands (`analysis::band_wave::BandWave`; NULL: not worked out yet), 2026-10-24.
    ("analysis_shape", "band_wave BLOB"),
    // The spectrum, 32 bands 20 times a second (`analysis::spectrum::Spectrum`; NULL: not worked out yet: only on a
    // play), 2026-10-24.
    ("analysis_shape", "spectrum BLOB"),
];

/// The crates' facts as last read (`Catalogue::crate_facts`), and what they were read from.
struct KeptFacts {
    changes: u64,
    folders: Vec<PathBuf>,
    built: i64,
    words: i64,
    /// The classical marks as read (0.10): a mark reads them again at once.
    marks: i64,
    at: std::time::Instant,
    facts: Arc<Vec<crate::crates::Facts>>,
}

/// The crates' facts are read again for a catalogue write or a harvested word at most this often (a build, at once).
const FACTS_EVERY: Duration = Duration::from_secs(300);

/// File states.
const PENDING: i64 = 0;
const READ: i64 = 1;
const FAILED: i64 = 2;

/// The files the library serves (0.9.7): those read, and those changed since they were read, with what was read of
/// them until they are read again (an album being retagged no longer leaves the library meanwhile; the owner,
/// 2026-10-26). `alias` is the files table's, with its dot ("f.") or none.
fn served(alias: &str) -> String {
    format!("({alias}state = {READ} OR ({alias}state = {PENDING} AND {alias}format IS NOT NULL))")
}

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

/// One file as a playlist row shows it (see `Catalogue::file_rows`): `None` where the catalogue does not know.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileRow {
    /// The server's id of the track, when matched.
    pub track_id: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    /// The album artist, the disc and track number, and the (original) year: the panel's orders.
    pub album_artist: Option<String>,
    pub disc: Option<u32>,
    pub track: Option<u32>,
    pub year: Option<i32>,
    /// Its tempo (decision 0035): the BPM tag, else the analysis' (while the file is as analysed); none without a beat.
    pub bpm: Option<u32>,
}

/// One album directory as the catalogue knows it (see `Catalogue::album_facts`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumFact {
    /// Relative to its folder.
    pub dir: String,
    pub tracks: u32,
    pub formats: std::collections::BTreeSet<String>,
    pub min_kbps: Option<u32>,
    /// Its files' lengths summed; `None` when any file's is unknown (as `source::total_secs`).
    pub duration_ms: Option<u64>,
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

/// One file as the OpenSubsonic library reads it (see `Catalogue::served_rows`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServedRow {
    /// Relative to its folder.
    pub rel: String,
    /// The Cratarr track id (decision 0011), once the file has one.
    pub track: Option<i64>,
    /// When the track was first seen, epoch seconds.
    pub born: Option<i64>,
    /// Upper case: "FLAC", "MP3".
    pub format: Option<String>,
    pub kbps: Option<u32>,
    pub duration_ms: Option<u64>,
    pub size: u64,
    pub mtime: i64,
    /// (name, raw value), only the names in `TAGS`, in the file's order.
    pub tags: Vec<(String, String)>,
    /// Its analysis (decision 0035), while the file is as it was then.
    pub analysis: Option<RowAnalysis>,
}

/// What the served library takes from a file's analysis: its gain and peak, its tempo when confident, and its album's
/// gain and peak once every track of the album has one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowAnalysis {
    pub track_gain_db: f32,
    pub track_peak: f32,
    pub bpm: Option<f32>,
    pub album: Option<(f32, f32)>,
}

impl ServedRow {
    pub const TAGS: [&'static str; 26] = [
        "title",
        "artist",
        "artists",
        "albumartist",
        "albumartists",
        "album",
        "tracknumber",
        "discnumber",
        "date",
        "originaldate",
        "releasedate",
        "genre",
        "bpm",
        "compilation",
        "musicbrainz_trackid",
        "musicbrainz_releasetrackid",
        "musicbrainz_albumid",
        "musicbrainz_artistid",
        "musicbrainz_albumartistid",
        "musicbrainz_releasegroupid",
        "replaygain_track_gain",
        "replaygain_track_peak",
        "replaygain_album_gain",
        "replaygain_album_peak",
        "r128_track_gain",
        "r128_album_gain",
    ];

    /// The first value of a tag, trimmed; `None` when missing or blank.
    pub fn first(&self, name: &str) -> Option<&str> {
        self.tags.iter().filter(|(n, _)| n == name).map(|(_, v)| v.trim()).find(|v| !v.is_empty())
    }

    /// Every raw value of a tag.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.tags.iter().filter(move |(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

pub struct Catalogue {
    db: Mutex<Connection>,
    /// Index = folder id; 0 is the playlist folder.
    folders: RwLock<Vec<PathBuf>>,
    progress: Mutex<Progress>,
    /// Asks the background loop for a pass now (settings changed, a playlist was edited…).
    poke: tokio::sync::Notify,
    /// Told when files were read again outside a pass (`reread`): the library is built from them at once.
    reread: tokio::sync::Notify,
    /// How much the passes have done since the server started (files read, folders walked, links made): what shows a
    /// long pass moving (0.9.7: a reader's reread of every file takes hours on the NAS, and Cratarr's own library
    /// looked stopped to the watch on the servers' scans).
    worked: std::sync::atomic::AtomicU64,
    /// The crates' facts, as of the database's change count and the folders in use (see `crate_facts`).
    facts: Mutex<Option<KeptFacts>>,
    /// The database file, for readers with their own connection (`reader`); `None` in memory.
    file: Option<PathBuf>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Catalogue {
    /// `file`: the database; `None` keeps it in memory (no data folder).
    pub fn open(file: Option<&Path>, folders: Vec<PathBuf>) -> Result<Self> {
        let conn = crate::db::open(file)?;
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
        // Once: the tags no query reads go, the unnamed ones (lyrics, pictures' text: 63,000 rows of 455,000 here) and the
        // index by name and value (73 MB of a 200 MB catalogue), so what is read fits in a small container's memory.
        // Their pages are reused by what is written next.
        conn.execute_batch("DROP INDEX IF EXISTS tags_by_name;")?;
        let slim: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'tags'", [], |r| r.get(0)).optional()?;
        if slim.as_deref() != Some("named") {
            conn.execute_batch("DELETE FROM tags WHERE name IS NULL; INSERT OR REPLACE INTO meta (key, value) VALUES ('tags', 'named');")?;
        }
        move_analysis_shapes(&conn)?;
        let cat = Self {
            db: Mutex::new(conn),
            folders: RwLock::new(Vec::new()),
            progress: Mutex::default(),
            poke: Default::default(),
            reread: Default::default(),
            worked: Default::default(),
            facts: Mutex::default(),
            file: file.map(Path::to_path_buf),
        };
        // A better reader may read what an older one could not.
        if cat.meta("reader").as_deref() != Some(&tags::READER_VERSION.to_string()) {
            cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2", params![PENDING, FAILED])?;
            // Reader 5 names ID3v2's TBPM and MP4's tmpo "bpm" as it already named Vorbis's BPM: the tags read before.
            cat.db().execute("UPDATE tags SET name = 'bpm' WHERE raw IN ('TBPM', 'tmpo') AND name IS NULL", [])?;
            // Reader 6 names a file's mood, style and grouping (0.9.7, the library's words), which no reader before kept:
            // every file read before is read again, and served as it was read until then.
            if cat.meta("reader").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v < 6) {
                cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2", params![PENDING, READ])?;
            }
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

    /// How many tracks each folder in use holds (decision 0044: a folder's row), with its path.
    pub fn tracks_per_folder(&self) -> Result<Vec<(PathBuf, u64)>> {
        let counts: std::collections::HashMap<i64, u64> = {
            let db = self.db();
            let mut q = db.prepare(&format!("SELECT folder, count(*) FROM files WHERE {PRESENT} GROUP BY folder"))?;
            q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64)))?.collect::<rusqlite::Result<_>>()?
        };
        Ok(self.folders().into_iter().enumerate().map(|(i, p)| (p, counts.get(&(i as i64)).copied().unwrap_or(0))).collect())
    }

    /// Use these folders (index = id). A folder whose path changed keeps its rows when the same files are there under
    /// the new path (a mount moved, say `/music` to `/media/Music`), and nothing is read again.
    ///
    /// A folder that is no longer in use is parked, not forgotten: its rows (files, tags, links to the servers' ids)
    /// move to a negative id and keep their track ids, and nothing counts them while parked. Back in use, at any place
    /// in the list, with its files where they were, it picks them up again: the same ids, only changed files read
    /// again. A source switched off and on (2026-10-15) cost Incoming its links and its track ids before this. Parked
    /// folders are forgotten after `PARKED_KEEP_SECS`, or when other files are found at their path.
    pub fn set_folders(&self, folders: Vec<PathBuf>) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let wanted: Vec<String> = folders.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let same_files = |id: i64, at: &str| -> Result<bool> {
            let mut st = tx.prepare("SELECT rel FROM files WHERE folder = ?1 AND gone_at IS NULL AND trashed IS NULL LIMIT 20")?;
            let rels: Vec<String> = st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            Ok(!rels.is_empty() && rels.iter().all(|rel| Path::new(at).join(rel).is_file()))
        };
        let rekey = |from: i64, to: i64| -> Result<()> {
            for table in ["files", "tags", "source_ids", "analysis"] {
                tx.execute(&format!("UPDATE {table} SET folder = ?2 WHERE folder = ?1"), params![from, to])?;
            }
            tx.execute("UPDATE folders SET id = ?2 WHERE id = ?1", params![from, to])?;
            Ok(())
        };
        let known = |tx: &rusqlite::Transaction| -> Result<Vec<(i64, String)>> {
            let mut st = tx.prepare("SELECT id, path FROM folders")?;
            Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
        };
        let now_s = now();
        // 1. Every folder in use that is not where it is wanted steps aside (parked), freeing its place.
        let mut lowest = known(&tx)?.iter().map(|(id, _)| *id).min().unwrap_or(0).min(0);
        let mut stepped: Vec<i64> = Vec::new();
        for (id, path) in known(&tx)? {
            if id < 0 || wanted.get(id as usize) == Some(&path) {
                continue;
            }
            lowest -= 1;
            rekey(id, lowest)?;
            stepped.push(lowest);
        }
        // 2. Each wanted folder takes the rows kept under its path (parked a while, or only just stepped aside), when its
        //    files are still there; else the rows of a folder that stepped aside and whose files are here now (a mount
        //    that moved, say `/music` to `/media/Music`).
        let mut forget: Vec<i64> = Vec::new();
        for (i, path) in wanted.iter().enumerate() {
            let i = i as i64;
            let rows = known(&tx)?;
            if rows.iter().any(|(id, p)| *id == i && p == path) {
                continue;
            }
            let own = rows.iter().find(|(id, p)| *id < 0 && p == path).map(|(id, _)| *id);
            let taken = match own {
                Some(p) if same_files(p, path)? => Some(p),
                Some(p) => {
                    forget.push(p);
                    None
                }
                None => None,
            };
            let taken = match taken {
                Some(p) => Some(p),
                None => {
                    let mut moved = None;
                    for &p in &stepped {
                        let unclaimed = rows.iter().any(|(id, q)| *id == p && !wanted.contains(q));
                        if unclaimed && same_files(p, path)? {
                            moved = Some(p);
                            break;
                        }
                    }
                    moved
                }
            };
            if let Some(p) = taken {
                rekey(p, i)?;
                tx.execute("UPDATE folders SET path = ?2 WHERE id = ?1", params![i, path])?;
                tx.execute("DELETE FROM meta WHERE key = ?1", [format!("parked:{p}")])?;
                stepped.retain(|s| *s != p);
                tracing::info!("catalogue: {path} is in use again, with its rows");
                // Links may have gone stale while it was away: every server lists its tracks again.
                tx.execute("DELETE FROM meta WHERE key LIKE 'full-folders:%'", [])?;
            }
        }
        // 3. What stepped aside and was not taken is parked, from now on; parked too long, it is forgotten.
        for p in &stepped {
            tx.execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)", params![format!("parked:{p}"), now_s.to_string()])?;
        }
        for (p, path) in known(&tx)?.into_iter().filter(|(id, _)| *id < 0) {
            let since: i64 = tx
                .query_row("SELECT value FROM meta WHERE key = ?1", [format!("parked:{p}")], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|v| v.parse().ok())
                .unwrap_or(now_s);
            if stepped.contains(&p) && now_s == since {
                tracing::info!("catalogue: {path} is out of use; its rows are kept for {} days", PARKED_KEEP_SECS / 86_400);
            }
            if now_s - since >= PARKED_KEEP_SECS {
                forget.push(p);
            }
        }
        for p in forget {
            tx.execute(
                "UPDATE tracks SET retired_at = ?2 WHERE id IN (SELECT track FROM files WHERE folder = ?1 AND track IS NOT NULL)",
                params![p, now_s],
            )?;
            for table in ["files", "tags", "source_ids", "analysis"] {
                tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1"), [p])?;
            }
            tx.execute("DELETE FROM folders WHERE id = ?1", [p])?;
            tx.execute("DELETE FROM meta WHERE key = ?1", [format!("parked:{p}")])?;
            // Its links to the servers' tracks went with it: every server lists all its tracks again on the next pass.
            tx.execute("DELETE FROM meta WHERE key LIKE 'full-folders:%'", [])?;
        }
        let wanted: HashMap<i64, String> = wanted.into_iter().enumerate().map(|(i, p)| (i as i64, p)).collect();
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
    /// When the last background pass finished, epoch seconds.
    pub fn last_pass(&self) -> Option<i64> {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).last_pass
    }

    /// What the background pass is doing now, if anything (see `Progress::doing`).
    pub fn doing(&self) -> Option<String> {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).doing.clone()
    }

    /// How much the passes have done since the server started (`worked`): it moves while a pass does.
    pub fn worked(&self) -> u64 {
        self.worked.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn work(&self, n: u64) {
        self.worked.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn poke(&self) {
        self.poke.notify_one();
    }

    fn set_doing(&self, doing: Option<String>) {
        self.work(1);
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).doing = doing;
    }

    /// Walk every folder and record what is new, changed or gone. Returns problems (unreadable folders). Blocking.
    pub fn walk_all(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (id, folder) in self.folders().into_iter().enumerate() {
            match walk::walk_with_covers(&folder) {
                Ok((found, covers, skipped)) => {
                    self.work(1);
                    if let Err(e) = self.record_walk(id as i64, &found).and_then(|()| self.record_covers(id as i64, &covers)) {
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

    /// The folder's cover files as the walk found them, written only where they differ from what was kept (a pass that
    /// finds them as they were changes nothing). Blocking.
    fn record_covers(&self, folder: i64, found: &[walk::Found]) -> Result<()> {
        let mut db = self.db();
        let kept: HashMap<String, (i64, i64)> = {
            let mut st = db.prepare_cached("SELECT rel, size, mtime FROM cover_files WHERE folder = ?1")?;
            st.query_map([folder], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?))))?.collect::<rusqlite::Result<_>>()?
        };
        let now: HashMap<&str, (i64, i64)> = found.iter().map(|f| (f.rel.as_str(), (f.size as i64, f.mtime))).collect();
        if now.len() == kept.len() && now.iter().all(|(rel, v)| kept.get(*rel) == Some(v)) {
            return Ok(());
        }
        let tx = db.transaction()?;
        for rel in kept.keys().filter(|rel| !now.contains_key(rel.as_str())) {
            tx.execute("DELETE FROM cover_files WHERE folder = ?1 AND rel = ?2", params![folder, rel])?;
        }
        for (rel, (size, mtime)) in now.iter().filter(|(rel, v)| kept.get(**rel) != Some(*v)) {
            tx.execute("INSERT OR REPLACE INTO cover_files (folder, rel, size, mtime) VALUES (?1, ?2, ?3, ?4)", params![folder, rel, size, mtime])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every present file's values of these named tags: (folder, path, name, value). Blocking.
    pub fn tag_values(&self, names: &[&str]) -> Result<Vec<(i64, String, String, String)>> {
        let db = self.db();
        let list = names.iter().map(|n| format!("'{}'", n.replace('\'', ""))).collect::<Vec<_>>().join(", ");
        let mut st = db.prepare(&format!(
            "SELECT t.folder, t.rel, t.name, t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel WHERE {PRESENT_F} AND t.name IN ({list})"
        ))?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Read the tags of up to `n` files waiting for it. Returns how many were handled. Blocking.
    pub fn read_tags_batch(&self, n: usize) -> Result<usize> {
        let todo: Vec<(i64, String)> = {
            let db = self.db();
            let mut st = db.prepare(&format!("SELECT folder, rel FROM files WHERE state = ?1 AND {PRESENT} LIMIT ?2"))?;
            st.query_map(params![PENDING, n as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
        };
        self.read_tags(todo)
    }

    /// Files Cratarr wrote itself (0.9.7: Renumber, a cover set or uploaded or undone, a tempo; the owner, 2026-10-26),
    /// read again now rather than at the next pass: each one's new size and time noted, its analysis and beats carried
    /// over (their audio is as it was), its tags read; then the library is told to build at once. Files the catalogue
    /// does not know are left to the pass. How many were read. Blocking.
    pub fn reread(&self, paths: &[PathBuf]) -> Result<usize> {
        let folders = self.folders();
        let mut todo = Vec::new();
        for path in paths {
            let Some((folder, rel)) = folders.iter().enumerate().find_map(|(i, root)| {
                let rel = path.strip_prefix(root).ok()?;
                let rel: Vec<&str> = rel.components().filter_map(|c| c.as_os_str().to_str()).collect();
                (!rel.is_empty()).then(|| (i as i64, rel.join("/")))
            }) else {
                continue;
            };
            let Ok(meta) = std::fs::metadata(path) else { continue };
            let after = (meta.len(), meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64));
            let before: Option<(i64, i64)> = self
                .db()
                .query_row("SELECT size, mtime FROM files WHERE folder = ?1 AND rel = ?2", params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let Some(before) = before else { continue };
            if (before.0 as u64, before.1) != after {
                self.carry_over(folder, &rel, (before.0 as u64, before.1), after)?;
            }
            todo.push((folder, rel));
        }
        let read = self.read_tags(todo)?;
        if read > 0 {
            self.progress.lock().unwrap_or_else(|p| p.into_inner()).last_pass = Some(now());
            self.reread.notify_one();
        }
        Ok(read)
    }

    /// Files were read again outside a pass (`reread`).
    pub async fn reread_done(&self) {
        self.reread.notified().await;
    }

    /// The tags of these files read, and kept. How many were handled. Blocking.
    fn read_tags(&self, todo: Vec<(i64, String)>) -> Result<usize> {
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
                    // Only the tags Cratarr knows by name: no query reads the others.
                    for (i, (raw, name, value)) in t.items.iter().enumerate().filter(|(_, (_, name, _))| name.is_some()) {
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
        self.work(results.len() as u64);
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
        self.work(links.len() as u64);
        Ok(())
    }

    /// After a full listing: forget this library's links not seen in it. Blocking.
    pub fn forget_unseen(&self, source: &str, library: &str, stamp: i64) -> Result<usize> {
        Ok(self.db().execute("DELETE FROM source_ids WHERE source = ?1 AND library = ?2 AND seen < ?3", params![source, library, stamp])?)
    }

    /// Whether a file is in the catalogue (used to find where a server's paths lie). Blocking.
    /// A present file's size and mtime (what an analysis is checked against). Blocking.
    pub fn file_stamp(&self, folder: i64, rel: &str) -> Option<(u64, i64)> {
        self.db()
            .prepare_cached(&format!("SELECT size, mtime FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"))
            .ok()?
            .query_row(params![folder, rel], |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?)))
            .optional()
            .ok()
            .flatten()
    }

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

    /// The server's id of the file each playlist line names (`place_of_line`), in order. Blocking.
    pub fn line_track_ids(&self, source: &str, lines: &[String]) -> Result<Vec<Option<String>>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT track_id FROM source_ids WHERE source = ?1 AND folder = ?2 AND rel = ?3 LIMIT 1")?;
        lines
            .iter()
            .map(|l| match self.place_of_line(l) {
                Some((folder, rel)) => Ok(st.query_row(params![source, folder, rel], |r| r.get::<_, String>(0)).optional()?),
                None => Ok(None),
            })
            .collect()
    }

    /// The files present in every folder in use, newest first (when the catalogue first saw the track; a moved file
    /// keeps its time), each with its folder and that time: what the crates are worked out on (decision 0024).
    /// Blocking.
    // The tests' own: nothing else uses it (no warning in the server's build).
    #[cfg(test)]
    pub fn files_newest_first(&self) -> Result<Vec<(i64, String, Option<i64>)>> {
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT folder, rel, tracks.born FROM files LEFT JOIN tracks ON tracks.id = files.track WHERE {PRESENT}
             ORDER BY tracks.born DESC, folder, rel"
        ))?;
        let files = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?;
        Ok(files.collect::<rusqlite::Result<_>>()?)
    }

    /// The files present in every folder in use (the Music folder and the Incoming ones, decision 0024 amended
    /// 2026-10-23: a crate draws from every library switched on), newest first, each as its playlist line (`line_of`)
    /// with its folder and what the crates' axes test: genres, year, BPM, AudioMuse's. Kept until the catalogue or its
    /// folders change: reading them means reading every file's tags. Blocking.
    pub fn crate_facts(&self) -> Result<Arc<Vec<crate::crates::Facts>>> {
        // Read again when the library is built again too: its albums' release types (0.9.6) come from there.
        let served = crate::serve::instance();
        let (changes, folders, built) = (self.db().total_changes(), self.folders(), served.as_ref().map_or(0, |s| s.built_at()));
        // And when the library's words change (0.9.7, `app::words::stamp`), or what Classical is made of (0.10).
        let words = served.as_ref().map_or(0, |s| crate::app::words::stamp(s).wrapping_mul(31).wrapping_add(crate::app::classical::stamp(s)));
        let marks = served.as_ref().map_or(0, |s| s.userdata.marks_stamp().unwrap_or(0));
        let mut kept = self.facts.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(k) = kept.as_ref()
            && k.folders == folders
            && k.built == built
            && k.marks == marks
            // A write to the catalogue or a word harvested reads them again only every so often (0.9.7: a reader's
            // reread of every file writes all along for hours, the harvest a word at a time; each Home read them
            // all again, and the sim went past 768 MB). A build or other folders read them at once.
            && ((k.changes == changes && k.words == words) || k.at.elapsed() < FACTS_EVERY)
        {
            return Ok(k.facts.clone());
        }
        let mut facts = self.read_crate_facts(&folders)?;
        if let Some(s) = &served
            && let Ok(types) = s.library.song_release_types()
        {
            for f in &mut facts {
                if let Some(t) = f.track.and_then(|t| types.get(&crate::serve::ids::song(t))) {
                    f.release_types = t.clone();
                }
            }
        }
        // And Classical where its album is one (0.10): a Type value of every list and crate.
        if let Some(s) = &served
            && let Err(e) = crate::app::classical::add_kind(s, &mut facts)
        {
            tracing::warn!("crates: Classical not read: {e:#}");
        }
        if let Some(s) = &served
            && let Ok(albums) = s.library.song_album_map()
        {
            // Each file with its album's words (0.9.7): genres, styles, moods and tags from every source.
            if let Ok(merged) = crate::app::words::merged(s) {
                for f in &mut facts {
                    if let Some(words) = f.track.and_then(|t| albums.get(&crate::serve::ids::song(t))).and_then(|a| merged.albums.get(a)) {
                        crate::app::words::add_to(f, words);
                    }
                }
            }
            // And with its album's id (0.11, quick crates: the Album axis, an album seed), one copy for all its
            // tracks; its labels Discogs' for its album where its tags name none, as kept (never asked for here).
            match (s.library.album_bases(), crate::app::discogs_facts::kept_labels(s)) {
                (Ok(bases), Ok(labels)) => {
                    let mut shared: HashMap<&str, Arc<str>> = HashMap::new();
                    for f in &mut facts {
                        let Some(album) = f.track.and_then(|t| albums.get(&crate::serve::ids::song(t))) else { continue };
                        let Some(base) = bases.get(album) else { continue };
                        f.album = Some(shared.entry(album.as_str()).or_insert_with(|| Arc::from(album.as_str())).clone());
                        if f.labels.is_empty()
                            && let Some(l) = labels.get(base)
                        {
                            f.labels = l.clone();
                        }
                    }
                }
                (Err(e), _) | (_, Err(e)) => tracing::warn!("crates: albums and labels not read: {e:#}"),
            }
        }
        let facts = Arc::new(facts);
        *kept = Some(KeptFacts { changes, folders, built, words, marks, at: std::time::Instant::now(), facts: facts.clone() });
        Ok(facts)
    }

    fn read_crate_facts(&self, folders: &[PathBuf]) -> Result<Vec<crate::crates::Facts>> {
        let db = self.db();
        type Place = (i64, String);
        // Every file present, newest first, with its track (the one similarity finds a file by it, heard or not), its
        // length and its format and bitrate (the crates' Length and Quality).
        type File = (i64, String, Option<i64>, Option<i64>, Option<i64>, Option<String>, Option<i64>);
        let mut st = db.prepare(&format!(
            "SELECT folder, rel, tracks.born, files.track, files.duration_ms, files.format, files.bitrate
             FROM files LEFT JOIN tracks ON tracks.id = files.track WHERE {PRESENT} ORDER BY tracks.born DESC, folder, rel"
        ))?;
        let files: Vec<File> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // The key its tags say (ID3v2 TKEY, Vorbis INITIALKEY or KEY).
        let mut st = db.prepare("SELECT folder, rel, value FROM tags WHERE folder >= 0 AND raw IN ('TKEY', 'INITIALKEY', 'KEY') ORDER BY folder, rel, n")?;
        let mut tag_keys: HashMap<Place, String> = HashMap::new();
        for row in st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?)))? {
            let (place, value) = row?;
            tag_keys.entry(place).or_insert(value);
        }
        // Its labels (0.11, quick crates): Vorbis LABEL or ORGANIZATION and MP4's LABEL (read as "label"), ID3v2 TPUB and
        // Vorbis PUBLISHER (lofty reads those as the publisher: kept by their raw keys). Every tag is kept, so no file
        // needs reading again for them.
        let mut st = db.prepare("SELECT folder, rel, value FROM tags WHERE folder >= 0 AND (name = 'label' OR raw IN ('TPUB', 'PUBLISHER')) ORDER BY folder, rel, n")?;
        let mut tag_labels: HashMap<Place, Vec<String>> = HashMap::new();
        for row in st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?)))? {
            let (place, value) = row?;
            let into = tag_labels.entry(place).or_default();
            for l in crate::crates::labels_of(&value) {
                if !into.iter().any(|x| x.eq_ignore_ascii_case(&l)) {
                    into.push(l);
                }
            }
        }
        let mut st = db.prepare(
            "SELECT folder, rel, name, value FROM tags WHERE folder >= 0 AND name IN ('genre', 'date', 'originaldate', 'bpm', 'artist', 'albumartist', 'artists', 'mood', 'style', 'grouping') ORDER BY folder, rel, n",
        )?;
        let mut by: HashMap<Place, Vec<(String, String)>> = HashMap::new();
        for row in st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
            let (place, name, value) = row?;
            by.entry(place).or_default().push((name, value));
        }
        // The tempo where the file has no BPM tag: its analysis' when sure enough (decision 0035), while the file is as
        // it was then. The tag comes first.
        let mut st = db.prepare(
            "SELECT a.folder, a.rel, a.bpm FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel
             WHERE a.folder >= 0 AND a.bpm IS NOT NULL AND a.size = f.size AND a.mtime = f.mtime
               AND (a.complete IS NULL OR a.complete != 0)",
        )?;
        let analysed: HashMap<Place, f64> =
            st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, f64>(2)?)))?.collect::<rusqlite::Result<_>>()?;
        // AudioMuse's (decision 0054), where it heard the file: energy, style tags, CLAP's moods.
        type Muse = (Option<f64>, Option<String>, Option<String>, i64, Option<Vec<u8>>, Option<String>);
        let mut st = db.prepare(&format!(
            "SELECT f.folder, f.rel, a.energy, a.style, a.moods, a.track, a.embedding, a.key FROM audiomuse a JOIN files f ON f.track = a.track WHERE {PRESENT_F}"
        ))?;
        let mut muse: HashMap<Place, Muse> = st
            .query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), (r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))))?
            .collect::<rusqlite::Result<_>>()?;
        // A mood a file has: one that stands out against the library's (the owner, 2026-10-23: CLAP scores every mood
        // between about 0.5 and 0.65, so a fixed 0.3 gave every heard file all five, every mood pill the same count).
        let stand_out = crate::app::audiomuse::stand_out(muse.values().filter_map(|m| crate::app::audiomuse::mood_scores(m.2.as_deref())));
        Ok(files
            .into_iter()
            .filter_map(|(folder, rel, born, file_track, duration_ms, format, bitrate)| {
                let place = (folder, rel);
                // A file no line can name (in a folder there is no way to from Music) is in no crate.
                let line = line_in(folders, place.0, &place.1)?;
                let tags = by.remove(&place).unwrap_or_default();
                let first: BTreeMap<&str, &str> =
                    tags.iter().rev().map(|(n, v)| (n.as_str(), v.as_str())).collect::<BTreeMap<_, _>>();
                let (mut genres, mut artists): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
                // The tags' own artists list (ARTISTS), where the file has one: credited, not shown (decision 0083).
                let mut listed: Vec<String> = Vec::new();
                // The file's own moods, styles and grouping (0.9.7, the library's words), each value split where it
                // holds several ("Chill; Dreamy").
                let (mut file_moods, mut file_styles, mut file_tags): (Vec<String>, Vec<String>, Vec<String>) = (Vec::new(), Vec::new(), Vec::new());
                for (n, v) in &tags {
                    let (into, parts): (&mut Vec<String>, Vec<&str>) = match n.as_str() {
                        "genre" => (&mut genres, vec![v.as_str()]),
                        // A compilation's album artist is no artist (as the radio has it).
                        "artist" | "albumartist" if !crate::crates::various(v) => (&mut artists, vec![v.as_str()]),
                        "artists" if !crate::crates::various(v) => (&mut listed, vec![v.as_str()]),
                        "mood" => (&mut file_moods, v.split([';', ',', '/']).collect()),
                        "style" => (&mut file_styles, v.split([';', ',', '/']).collect()),
                        "grouping" => (&mut file_tags, v.split([';', ',', '/']).collect()),
                        _ => continue,
                    };
                    for p in parts.into_iter().map(str::trim).filter(|p| !p.is_empty()) {
                        if !into.iter().any(|g| g.eq_ignore_ascii_case(p)) {
                            into.push(p.to_string());
                        }
                    }
                }
                let (mut energy, mut danceability, mut moods, mut styles, mut sound, mut clap_moods, mut muse_key) = (None, None, Vec::new(), Vec::new(), None, None, None);
                let mut track = file_track;
                if let Some((e, style, clap, t, embedding, k)) = muse.remove(&place) {
                    track = Some(t);
                    muse_key = k;
                    // Its "sounds like" (decision 0054), of unit length: what a Sounds like condition compares.
                    let v: Vec<f32> = embedding.unwrap_or_default().chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    sound = crate::similarity::mean_sound(&[&v]).map(std::sync::Arc::from);
                    // Every mood's score, for a list's profile; none until CLAP heard the file (all 0).
                    clap_moods = crate::app::audiomuse::mood_scores(clap.as_deref());
                    moods = crate::app::audiomuse::moods_of(clap_moods, &stand_out).into_iter().map(|(m, _)| m.to_string()).collect();
                    let d = crate::app::audiomuse::danceability(&AudioMuseRow { moods: clap, ..AudioMuseRow::default() });
                    energy = e.map(|e| (e * 100.0).round());
                    danceability = d.map(|d| (d * 100.0).round());
                    styles = crate::app::audiomuse::weighted(style.as_deref()).into_iter().take(crate::app::audiomuse::STYLES).map(|(n, _)| n).collect();
                }
                for (into, own) in [(&mut moods, file_moods), (&mut styles, file_styles)] {
                    for w in own {
                        if !into.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
                            into.push(w);
                        }
                    }
                }
                let (credits, credit_names) = crate::app::excluded_artists::credited(&artists.iter().chain(&listed).collect::<Vec<_>>());
                Some(crate::crates::Facts {
                    year: crate::crates::year_of(&first),
                    bpm: first.get("bpm").and_then(|b| crate::crates::bpm_of(b)).or_else(|| analysed.get(&place).map(|b| b.round())),
                    genres,
                    rel: line,
                    folder: place.0,
                    born,
                    energy,
                    danceability,
                    moods,
                    styles,
                    track,
                    sound,
                    credits,
                    credit_names,
                    artists,
                    clap_moods,
                    length: duration_ms.filter(|d| *d > 0).map(|d| d as f64 / 1000.0),
                    quality: crate::crates::quality_of(format.as_deref(), bitrate),
                    // The key the tags say, else AudioMuse's; as one spelling ("D minor").
                    key: tag_keys.remove(&place).and_then(|k| crate::crates::key_name(&k)).or_else(|| muse_key.and_then(|k| crate::crates::key_name(&k))),
                    release_types: Vec::new(),
                    tags: file_tags,
                    album: None,
                    labels: tag_labels.remove(&place).unwrap_or_default(),
                })
            })
            .collect())
    }

    /// The playlist lines of the files these server track ids are, in any folder in use (`line_of`); ids not known are
    /// left out. Blocking.
    pub fn lines_of(&self, source: &str, ids: &[String]) -> Result<std::collections::HashSet<String>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel FROM source_ids WHERE source = ?1 AND track_id = ?2 AND folder >= 0")?;
        let mut out = std::collections::HashSet::new();
        for id in ids {
            for place in st.query_map(params![source, id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                let (folder, rel) = place?;
                out.extend(line_in(&folders, folder, &rel));
            }
        }
        Ok(out)
    }

    /// A file's playlist line (`place_of_line` the other way round): its path relative to the Music folder (folder 0);
    /// one in another folder climbs out of it ("../lidarr-music/X/01.flac", an Incoming track). `None` for a folder
    /// not in use, or one there is no way to from the Music folder.
    pub fn line_of(&self, folder: i64, rel: &str) -> Option<String> {
        line_in(&self.folders(), folder, rel)
    }

    /// The file a playlist line names, on the disk (`place_of_line`): Music's, or another folder's for a line that
    /// climbs out of it.
    pub fn path_of_line(&self, line: &str) -> Option<PathBuf> {
        let (folder, rel) = self.place_of_line(line)?;
        Some(self.folders().get(usize::try_from(folder).ok()?)?.join(rel))
    }

    /// Where a playlist line points: (folder, rel). Lines are relative to the Music folder (folder 0); a line for a file
    /// in another folder climbs out of it ("../lidarr-music/X/01.flac", an Incoming track). None for a line that lands
    /// in no folder of the catalogue.
    pub fn place_of_line(&self, line: &str) -> Option<(i64, String)> {
        if !line.starts_with("../") {
            return Some((0, line.to_string()));
        }
        let folders = self.folders();
        let mut target = folders.first()?.clone();
        for part in line.split('/') {
            match part {
                ".." => {
                    if !target.pop() {
                        return None;
                    }
                }
                "" | "." => {}
                p => target.push(p),
            }
        }
        folders.iter().enumerate().skip(1).find_map(|(i, f)| {
            let rel = target.strip_prefix(f).ok()?.to_string_lossy().replace('\\', "/");
            (!rel.is_empty()).then_some((i as i64, rel))
        })
    }

    /// What the catalogue knows of the file each playlist line names, in order: a playlist's rows (decision 0023).
    /// Lines are relative to the Music folder (`place_of_line`). Files it has not read leave the facts empty. Blocking.
    pub fn line_rows(&self, source: &str, lines: &[String]) -> Result<Vec<FileRow>> {
        let places: Vec<Option<(i64, String)>> = lines.iter().map(|l| self.place_of_line(l)).collect();
        self.rows_at(source, &places)
    }

    /// What the catalogue knows of each of these tracks, by a source's ids, in order (a player's queue sent by ids
    /// only). Ids it cannot place leave the facts empty. Blocking.
    pub fn source_rows(&self, source: &str, ids: &[String]) -> Result<Vec<FileRow>> {
        let places: Vec<Option<(i64, String)>> = ids.iter().map(|id| self.track_of_source(source, id).and_then(|t| self.file_of(t))).collect();
        self.rows_at(source, &places)
    }

    /// What the catalogue knows of each of these files (folder, rel), in order; `None` leaves the facts empty. Blocking.
    pub fn place_rows(&self, places: &[Option<(i64, String)>]) -> Result<Vec<FileRow>> {
        self.rows_at("", places)
    }

    /// A file's format ("FLAC", "MP3", …) and bit rate (kb/s) as read, when known: an export's estimates (decision
    /// 0058). Blocking.
    pub fn format_of(&self, folder: i64, rel: &str) -> (Option<String>, Option<u32>) {
        self.db()
            .query_row(&format!("SELECT format, bitrate FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"), params![folder, rel], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?))
            })
            .optional()
            .ok()
            .flatten()
            .map(|(f, k)| (f, k.filter(|k| *k > 0).map(|k| k as u32)))
            .unwrap_or((None, None))
    }

    /// What the catalogue knows of each of these files of a folder, in order. Blocking.
    #[cfg(test)]
    pub fn file_rows(&self, source: &str, folder: i64, rels: &[String]) -> Result<Vec<FileRow>> {
        let places: Vec<Option<(i64, String)>> = rels.iter().map(|r| Some((folder, r.clone()))).collect();
        self.rows_at(source, &places)
    }

    fn rows_at(&self, source: &str, places: &[Option<(i64, String)>]) -> Result<Vec<FileRow>> {
        let db = self.db();
        let mut file = db.prepare_cached(&format!("SELECT duration_ms FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"))?;
        let mut tags = db.prepare_cached(
            "SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2
             AND name IN ('title', 'artist', 'album', 'albumartist', 'tracknumber', 'discnumber', 'originaldate', 'date', 'bpm') ORDER BY n",
        )?;
        let mut analysed = db.prepare_cached(
            "SELECT a.bpm, a.duration_secs, a.complete FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel
             WHERE a.folder = ?1 AND a.rel = ?2 AND a.size = f.size AND a.mtime = f.mtime",
        )?;
        // "3", "3/12": the number before a slash.
        let number = |v: &str| v.split('/').next().and_then(|n| n.trim().parse::<u32>().ok());
        // "1994", "1994-05-01": its first four digits.
        let year = |v: &str| v.get(..4).and_then(|y| y.parse::<i32>().ok());
        let mut ids = db.prepare_cached("SELECT track_id FROM source_ids WHERE source = ?1 AND folder = ?2 AND rel = ?3 LIMIT 1")?;
        places
            .iter()
            .map(|place| {
                let Some((folder, rel)) = place else { return Ok(FileRow::default()) };
                let mut row = FileRow { duration_ms: file.query_row(params![folder, rel], |r| r.get::<_, Option<i64>>(0)).optional()?.flatten(), ..FileRow::default() };
                let found = tags.query_map(params![folder, rel], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
                let (mut original, mut date, mut bpm) = (None, None, None);
                for t in found {
                    let (name, value) = t?;
                    // The first value of each (a tag can hold several).
                    match name.as_str() {
                        "title" => drop(row.title.get_or_insert(value)),
                        "artist" => drop(row.artist.get_or_insert(value)),
                        "album" => drop(row.album.get_or_insert(value)),
                        "albumartist" => drop(row.album_artist.get_or_insert(value)),
                        "tracknumber" => row.track = row.track.or_else(|| number(&value)),
                        "discnumber" => row.disc = row.disc.or_else(|| number(&value)),
                        "originaldate" => original = original.or_else(|| year(&value)),
                        "bpm" => bpm = bpm.or_else(|| crate::crates::bpm_of(&value)),
                        _ => date = date.or_else(|| year(&value)),
                    }
                }
                row.year = original.or(date);
                let (analysed_bpm, measured, complete) = analysed
                    .query_row(params![folder, rel], |r| Ok((r.get::<_, Option<f64>>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, Option<bool>>(2)?)))
                    .optional()?
                    .unwrap_or((None, None, None));
                // As the served library reads them (243a89db): nothing from a decode that stopped early, and a whole
                // decode's length where the file's header said otherwise by more than 1.5 s.
                let trusted = complete != Some(false);
                row.bpm = bpm.or(analysed_bpm.filter(|_| trusted)).map(f64::round).filter(|b| *b > 0.0).map(|b| b as u32);
                if complete == Some(true)
                    && let Some(secs) = measured.filter(|s| *s > 0.0)
                {
                    let ms = (secs * 1000.0).round() as i64;
                    if row.duration_ms.is_none_or(|d| (d - ms).abs() > 1_500) {
                        row.duration_ms = Some(ms);
                    }
                }
                row.track_id = ids.query_row(params![source, folder, rel], |r| r.get::<_, String>(0)).optional()?;
                Ok(row)
            })
            .collect()
    }

    /// One row per album directory in `folder`: track count, formats, lowest bitrate, length, and the album tags
    /// Incoming needs to recognise an album that is already in the library. Read-only; blocking.
    pub fn album_facts(&self, folder: i64) -> Result<Vec<AlbumFact>> {
        let db = self.db();
        let dir_of = |rel: &str| rel.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string());
        let mut by_dir: BTreeMap<String, AlbumFact> = BTreeMap::new();
        let mut st = db.prepare(&format!("SELECT rel, format, bitrate, duration_ms FROM files WHERE folder = ?1 AND {PRESENT}"))?;
        let rows = st.query_map(params![folder], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?))
        })?;
        for row in rows {
            let (rel, format, kbps, ms) = row?;
            let dir = dir_of(&rel);
            let e = by_dir.entry(dir.clone()).or_insert_with(|| AlbumFact { dir, ..AlbumFact::default() });
            // Summed while every file's length is known; one unknown leaves the whole unknown.
            let sum = if e.tracks == 0 { Some(0) } else { e.duration_ms };
            e.duration_ms = sum.zip(ms.filter(|m| *m > 0)).map(|(d, m)| d + m as u64);
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

    /// Every present file of `folder` with what the OpenSubsonic library is built from (decision 0030): its Cratarr track
    /// id and when that was first seen, the file's facts, and the tags named in `ServedRow::TAGS` in the file's order.
    /// Read-only; blocking.
    pub fn served_rows(&self, folder: i64) -> Result<Vec<ServedRow>> {
        served_rows(&self.db(), folder)
    }

    /// The served rows of the files right in `dir` of `folder` (0.9.8: the library builds just those again).
    pub fn served_rows_in(&self, folder: i64, dir: &str) -> Result<Vec<ServedRow>> {
        served_rows_in(&self.db(), folder, Some(dir))
    }

    /// Each folder's cover files as a print (`cover_prints`).
    pub fn cover_prints(&self) -> Result<HashMap<(i64, String), String>> {
        cover_prints(&self.db())
    }

    /// A read-only connection of its own to the catalogue's file, for long reads that must not hold up the catalogue
    /// (WAL lets it read beside the writer). `None` for a catalogue in memory.
    pub fn reader(&self) -> Option<Connection> {
        let file = self.file.as_ref()?;
        let conn = crate::db::open_read_only(file).ok()?;
        conn.busy_timeout(Duration::from_secs(10)).ok()?;
        Some(conn)
    }

    /// A cheap summary of the served files (count, sizes, times, track ids): when it changes, `served_rows` would too.
    pub fn served_fingerprint(&self) -> Result<String> {
        served_fingerprint(&self.db())
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
            for table in ["files", "tags", "source_ids", "analysis"] {
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
    /// A walk can have seen the files at `to` first (they moved before this was called) and made rows of its own
    /// there, with new track ids: those go, and the moved rows, with their track ids, take their place.
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
            if (to_folder, new_rel.as_str()) != (from_folder, rel.as_str()) {
                for table in ["files", "tags", "source_ids", "analysis"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![to_folder, new_rel])?;
                }
            }
            tx.execute("DELETE FROM source_ids WHERE folder = ?1 AND rel = ?2", params![from_folder, rel])?;
            tx.execute(
                "UPDATE files SET folder = ?3, rel = ?4, trashed = ?5, gone_at = NULL WHERE folder = ?1 AND rel = ?2",
                params![from_folder, rel, to_folder, new_rel, trashed],
            )?;
            tx.execute("UPDATE tags SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2", params![from_folder, rel, to_folder, new_rel])?;
            tx.execute("UPDATE analysis SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2", params![from_folder, rel, to_folder, new_rel])?;
            tx.execute("UPDATE analysis_shape SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2", params![from_folder, rel, to_folder, new_rel])?;
        }
        tx.commit()?;
        Ok(rels.len())
    }

    /// Rows Cratarr moved into a trash (Replace: the old files into Music's trash; before 2026-10-16, Incoming's):
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
    /// counts. Cratarr's own source (decision 0032) needs no links: its ids are the track ids. Blocking.
    pub fn track_of_source(&self, source: &str, item: &str) -> Option<i64> {
        track_of_source(&self.db(), source, item)
    }

    /// Each of these server tracks' file (folder id, relative path) and length, nearest first, as `track_of_source`,
    /// `file_of` and `duration_of` find them one by one; up to the first it has no file of. In one hold of the catalogue
    /// (0.10.2: the slow answers of 2026-10-30): a wind through the queue looked up its 64 tracks with three takes of
    /// the lock each, 192 waits behind whatever else held it, before its headers went. Blocking.
    pub fn files_of_sources(&self, items: &[(&str, &str)]) -> Vec<(i64, String, Option<u64>)> {
        let db = self.db();
        items
            .iter()
            .map_while(|(source, item)| {
                let (folder, rel) = file_of(&db, track_of_source(&db, source, item)?)?;
                let duration_ms = duration_of(&db, folder, &rel);
                Some((folder, rel, duration_ms))
            })
            .collect()
    }

    /// A track's item id on a server (decision 0012), when it is linked. Blocking.
    pub fn source_id_of(&self, source: &str, track: i64) -> Option<String> {
        self.db()
            .query_row(
                &format!(
                    "SELECT s.track_id FROM files f JOIN source_ids s ON s.folder = f.folder AND s.rel = f.rel
                     WHERE f.track = ?2 AND s.source = ?1 AND {} LIMIT 1",
                    PRESENT_F
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
    /// Each of these server tracks' genres (its genre tags as read), in order; none for a track not linked to a file.
    /// Blocking.
    pub fn genres_of_ids(&self, source: &str, ids: &[String]) -> Vec<Vec<String>> {
        ids.iter()
            .map(|id| {
                let Some((folder, rel)) = self.track_of_source(source, id).and_then(|t| self.file_of(t)) else { return Vec::new() };
                let db = self.db();
                let Ok(mut st) = db.prepare_cached("SELECT value FROM tags WHERE folder = ?1 AND rel = ?2 AND name = 'genre' ORDER BY n") else {
                    return Vec::new();
                };
                st.query_map(params![folder, rel], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
            })
            .collect()
    }

    /// A file's genre tags as read, in order (the analysis' tempo rules). Blocking.
    pub fn genres_of_file(&self, folder: i64, rel: &str) -> Vec<String> {
        let db = self.db();
        let Ok(mut st) = db.prepare_cached("SELECT value FROM tags WHERE folder = ?1 AND rel = ?2 AND name = 'genre' ORDER BY n") else {
            return Vec::new();
        };
        st.query_map(params![folder, rel], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// A file's length as read from it (milliseconds), when known.
    /// How long a file's sound is as the analysis decoded it whole (ms), where it has (0.9.14, Health's fixes).
    pub fn heard_ms(&self, folder: i64, rel: &str) -> Option<i64> {
        self.db()
            .query_row(
                "SELECT CAST(a.duration_secs * 1000 AS INTEGER) FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel AND f.size = a.size
                 WHERE a.folder = ?1 AND a.rel = ?2",
                params![folder, rel],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .ok()
            .flatten()
            .filter(|ms| *ms > 0)
    }

    /// Files whose sound, as the analysis decoded it whole, ends more than `slack_ms` before the length they give
    /// (0.9.14, Health): worst first, with their folder, path, given length and heard length (ms).
    pub fn shorter_than_heard(&self, slack_ms: i64) -> Result<Vec<(i64, String, PathBuf, i64, i64)>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT f.folder, f.rel, f.duration_ms, CAST(a.duration_secs * 1000 AS INTEGER) AS heard FROM analysis a
             JOIN files f ON f.folder = a.folder AND f.rel = a.rel AND f.size = a.size
             WHERE {PRESENT_F} AND f.duration_ms > 0 AND a.duration_secs > 0 AND f.duration_ms - heard > ?1
             ORDER BY f.duration_ms - heard DESC"
        ))?;
        let rows = st.query_map([slack_ms], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (folder, rel, given, heard) = row?;
            if let Some(dir) = usize::try_from(folder).ok().and_then(|i| folders.get(i)) {
                out.push((folder, rel.clone(), dir.join(&rel), given, heard));
            }
        }
        Ok(out)
    }

    pub fn duration_of(&self, folder: i64, rel: &str) -> Option<u64> {
        duration_of(&self.db(), folder, rel)
    }

    // --- A file's analysis (decision 0035) ---
    //
    // Kept per file as it was when analysed (its size and mtime): a file changed since, or analysed by an older
    // `analysis::VERSION`, waits to be analysed again (`unanalysed`). It follows its file through moves, a parked
    // folder and the trash, as the tags do.

    /// A file's analysis and beats carried over to its new size and time (its audio as it was), and the file noted as
    /// changed, its tags to read again. Blocking.
    fn carry_over(&self, folder: i64, rel: &str, before: (u64, i64), after: (u64, i64)) -> Result<()> {
        let db = self.db();
        for table in ["analysis", "beat_this"] {
            db.execute(
                &format!("UPDATE {table} SET size = ?3, mtime = ?4 WHERE folder = ?1 AND rel = ?2 AND size = ?5 AND mtime = ?6"),
                params![folder, rel, after.0 as i64, after.1, before.0 as i64, before.1],
            )?;
        }
        db.execute("UPDATE files SET size = ?3, mtime = ?4, state = 0 WHERE folder = ?1 AND rel = ?2", params![folder, rel, after.0 as i64, after.1])?;
        Ok(())
    }

    /// Keeps a file's analysis (the pass, or an import): replaces what it had. Blocking.
    pub fn store_analysis(&self, folder: i64, rel: &str, size: u64, mtime: i64, a: &crate::analysis::Analysis) -> Result<()> {
        store_analysis(&self.db(), folder, rel, size, mtime, a, now_secs())
    }

    /// AudioMuse's analysis (decision 0054), all of it replaced by `rows`: what the PC's export holds now. Blocking.
    pub fn replace_audiomuse(&self, rows: &[AudioMuseRow]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute("DELETE FROM audiomuse", [])?;
        {
            let mut st = tx.prepare("INSERT OR REPLACE INTO audiomuse (track, energy, key, style, moods, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for r in rows {
                st.execute(params![r.track, r.energy, r.key, r.style, r.moods, r.embedding])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A track's AudioMuse analysis, where it has one. Blocking.
    pub fn audiomuse_of(&self, track: i64) -> Option<AudioMuseRow> {
        self.db()
            .query_row("SELECT track, energy, key, style, moods, embedding FROM audiomuse WHERE track = ?1", [track], audiomuse_row)
            .optional()
            .ok()
            .flatten()
    }

    /// Several at once, in one transaction (an import's batch). Blocking.
    pub fn store_analyses(&self, items: &[(i64, String, u64, i64, crate::analysis::Analysis)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let now = now_secs();
        for (folder, rel, size, mtime, a) in items {
            store_analysis(&tx, *folder, rel, *size, *mtime, a, now)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A file's analysis, while the file is as it was then (same size and mtime); any version. Blocking.
    pub fn analysis_of(&self, folder: i64, rel: &str) -> Option<crate::analysis::Analysis> {
        let db = self.db();
        db.query_row(
            &format!(
                "SELECT {ANALYSIS_COLS} FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel {SHAPE_JOIN}
                 WHERE a.folder = ?1 AND a.rel = ?2 AND a.size = f.size AND a.mtime = f.mtime"
            ),
            params![folder, rel],
            analysis_row,
        )
        .optional()
        .ok()
        .flatten()
    }

    /// When a file's analysis was made, and its version (the waveform's ETag), while the file is as it was then.
    pub fn analysis_stamp(&self, folder: i64, rel: &str) -> Option<(i64, u32)> {
        self.db()
            .query_row(
                "SELECT a.analysed_at, a.version FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel
                 WHERE a.folder = ?1 AND a.rel = ?2 AND a.size = f.size AND a.mtime = f.mtime",
                params![folder, rel],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// A file's beats (decision 0046), while it is as it was analysed: `None` without such an analysis, `Some(None)`
    /// when that analysis has none worked out yet (it is from before they were kept). Blocking.
    pub fn beats_of(&self, folder: i64, rel: &str) -> Option<Option<crate::analysis::Beats>> {
        self.shape_blob(folder, rel, "beats").map(|b| b.map(crate::analysis::Beats))
    }

    /// Keeps a file's beats beside its analysis, which stays as it is, when that analysis is of the file as it was
    /// (`size`, `mtime`) when the beats were worked out; an analysis without a shape row gets one holding only them.
    /// Whether they were kept. Blocking.
    /// `by_beat_this`: they are Beat this!'s (decision 0049, amended), not the analysis' own grid.
    pub fn store_beats(&self, folder: i64, rel: &str, size: u64, mtime: i64, beats: &crate::analysis::Beats, by_beat_this: bool) -> Result<bool> {
        let kept = self.store_shape_blob(folder, rel, size, mtime, "beats", &beats.0)?;
        if kept {
            self.db().execute(
                "UPDATE analysis_shape SET beats_by = ?3 WHERE folder = ?1 AND rel = ?2",
                params![folder, rel, by_beat_this.then_some(1)],
            )?;
        }
        Ok(kept)
    }

    /// Whether a file's kept beats are Beat this!'s (decision 0049, amended). Blocking.
    pub fn beats_are_beat_this(&self, folder: i64, rel: &str) -> bool {
        self.db()
            .query_row("SELECT beats_by FROM analysis_shape WHERE folder = ?1 AND rel = ?2", params![folder, rel], |r| r.get::<_, Option<i64>>(0))
            .optional()
            .ok()
            .flatten()
            .flatten()
            == Some(1)
    }

    /// Beat this!'s beats and downbeats (decision 0049, amended), in one transaction: each replaces what its file had.
    /// Blocking.
    pub fn store_beat_this(&self, rows: &[BeatThisRow]) -> Result<()> {
        let millis = |v: &[u32]| v.iter().flat_map(|m| m.to_le_bytes()).collect::<Vec<u8>>();
        let mut db = self.db();
        let tx = db.transaction()?;
        let now = now_secs();
        {
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO beat_this (folder, rel, size, mtime, beats, downbeats, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for r in rows {
                st.execute(params![r.folder, r.rel, r.size as i64, r.mtime, millis(&r.beats), millis(&r.downbeats), now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A file's beats and downbeats by Beat this! (seconds), while the file is as it was heard. Blocking.
    pub fn beat_this_of(&self, folder: i64, rel: &str) -> Option<(Vec<f64>, Vec<f64>)> {
        let seconds = |b: Vec<u8>| b.chunks_exact(4).map(|c| f64::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])) / 1000.0).collect::<Vec<f64>>();
        self.db()
            .query_row(
                &format!(
                    "SELECT b.beats, b.downbeats FROM beat_this b JOIN files f ON f.folder = b.folder AND f.rel = b.rel
                     WHERE b.folder = ?1 AND b.rel = ?2 AND b.size = f.size AND b.mtime = f.mtime AND {PRESENT_F}"
                ),
                params![folder, rel],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .ok()
            .flatten()
            .map(|(b, d)| (seconds(b), seconds(d)))
    }

    /// Present audio files without Beat this!'s beats for them as they are, newest first (by when the track was first
    /// seen), at most `limit`: all of them (`since` none: the PC's list), or those first seen or changed since `since`
    /// (seconds; the NAS's own, new music). (folder, rel, path, size, mtime). Blocking.
    pub fn without_beat_this(&self, since: Option<i64>, limit: usize) -> Result<Vec<(i64, String, PathBuf, u64, i64)>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT f.folder, f.rel, f.size, f.mtime FROM files f
               LEFT JOIN beat_this b ON b.folder = f.folder AND b.rel = f.rel AND b.size = f.size AND b.mtime = f.mtime
               LEFT JOIN tracks t ON t.id = f.track
             WHERE {PRESENT_F} AND f.state = ?1 AND b.rel IS NULL AND (?2 IS NULL OR t.born >= ?2 OR f.mtime >= ?2)
             ORDER BY t.born DESC, f.mtime DESC, f.rel LIMIT ?3"
        ))?;
        let rows = st.query_map(params![READ, since, i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?.max(0) as u64, r.get::<_, i64>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (folder, rel, size, mtime) = row?;
            if let Some(root) = usize::try_from(folder).ok().and_then(|i| folders.get(i)) {
                let path = root.join(&rel);
                out.push((folder, rel, path, size, mtime));
            }
        }
        Ok(out)
    }

    /// A file's signals (decision 0046, amended), as `beats_of`: `Some(None)` until the visuals first asked.
    pub fn signals_of(&self, folder: i64, rel: &str) -> Option<Option<crate::analysis::signals::Signals>> {
        self.shape_blob(folder, rel, "bands").map(|b| b.map(crate::analysis::signals::Signals))
    }

    /// Keeps a file's signals beside its analysis, as `store_beats` keeps beats (and leaves them as they are).
    pub fn store_signals(&self, folder: i64, rel: &str, size: u64, mtime: i64, signals: &crate::analysis::signals::Signals) -> Result<bool> {
        self.store_shape_blob(folder, rel, size, mtime, "bands", &signals.0)
    }

    /// One of the shape's blobs worked out apart (`beats`, `bands`), while the file is as it was analysed.
    /// A file's waveform in three bands, as `signals_of`: `Some(None)` until it is first worked out.
    pub fn band_wave_of(&self, folder: i64, rel: &str) -> Option<Option<crate::analysis::band_wave::BandWave>> {
        self.shape_blob(folder, rel, "band_wave").map(|b| b.map(crate::analysis::band_wave::BandWave))
    }

    /// Keeps a file's waveform in three bands beside its analysis (false: the file has no analysis as it is now).
    pub fn store_band_wave(&self, folder: i64, rel: &str, size: u64, mtime: i64, wave: &crate::analysis::band_wave::BandWave) -> Result<bool> {
        self.store_shape_blob(folder, rel, size, mtime, "band_wave", &wave.0)
    }

    /// A file's spectrum (`analysis::spectrum`): None without an analysis as the file is now; Some(None) not worked
    /// out yet.
    pub fn spectrum_of(&self, folder: i64, rel: &str) -> Option<Option<crate::analysis::spectrum::Spectrum>> {
        self.shape_blob(folder, rel, "spectrum").map(|b| b.map(crate::analysis::spectrum::Spectrum))
    }

    /// Keeps a file's spectrum beside its analysis (false: the file has no analysis as it is now); or the mark of one
    /// that could not be worked out (`spectrum::FAILED`).
    pub fn store_spectrum(&self, folder: i64, rel: &str, size: u64, mtime: i64, spectrum: &crate::analysis::spectrum::Spectrum) -> Result<bool> {
        self.store_shape_blob(folder, rel, size, mtime, "spectrum", &spectrum.0)
    }

    /// Marks a file its waveform in three bands could not be worked out of (`band_wave::FAILED`), as it is now.
    pub fn store_band_wave_failed(&self, folder: i64, rel: &str, size: u64, mtime: i64) -> Result<bool> {
        self.store_shape_blob(folder, rel, size, mtime, "band_wave", &[crate::analysis::band_wave::FAILED])
    }

    /// Present files with an analysis as they are now but no waveform in three bands (or one of an older form), and not
    /// marked as failed: the analysis pass's work when it has nothing else to do, newest music first. Blocking.
    pub fn without_band_wave(&self, limit: usize) -> Result<Vec<(i64, String, PathBuf, u64, i64)>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT f.folder, f.rel, f.size, f.mtime FROM files f
               JOIN analysis a ON a.folder = f.folder AND a.rel = f.rel AND a.size = f.size AND a.mtime = f.mtime
               LEFT JOIN analysis_shape s ON s.folder = a.folder AND s.rel = a.rel
               LEFT JOIN tracks t ON t.id = f.track
             WHERE {PRESENT_F} AND f.state = ?1 AND (a.complete IS NULL OR a.complete != 0)
               AND (s.band_wave IS NULL OR (s.band_wave != ?5 AND (length(s.band_wave) != ?2 OR substr(s.band_wave, 1, 1) != ?3)))
             ORDER BY t.born DESC, f.mtime DESC LIMIT ?4"
        ))?;
        let format = vec![crate::analysis::band_wave::FORMAT];
        let failed = vec![crate::analysis::band_wave::FAILED];
        let rows = st.query_map(params![READ, crate::analysis::band_wave::LEN as i64, format, limit as i64, failed], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?.max(0) as u64, r.get::<_, i64>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (folder, rel, size, mtime) = row?;
            let Some(root) = usize::try_from(folder).ok().and_then(|i| folders.get(i)) else { continue };
            out.push((folder, rel.clone(), root.join(&rel), size, mtime));
        }
        Ok(out)
    }

    fn shape_blob(&self, folder: i64, rel: &str, column: &'static str) -> Option<Option<Vec<u8>>> {
        self.db()
            .query_row(
                &format!(
                    "SELECT s.{column} FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel {SHAPE_JOIN}
                     WHERE a.folder = ?1 AND a.rel = ?2 AND a.size = f.size AND a.mtime = f.mtime"
                ),
                params![folder, rel],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    fn store_shape_blob(&self, folder: i64, rel: &str, size: u64, mtime: i64, column: &'static str, blob: &[u8]) -> Result<bool> {
        let n = self.db().execute(
            &format!(
                "INSERT INTO analysis_shape (folder, rel, {column})
                   SELECT ?1, ?2, ?5 WHERE EXISTS (SELECT 1 FROM analysis a WHERE a.folder = ?1 AND a.rel = ?2 AND a.size = ?3 AND a.mtime = ?4)
                 ON CONFLICT (folder, rel) DO UPDATE SET {column} = excluded.{column}"
            ),
            params![folder, rel, size as i64, mtime, blob],
        )?;
        Ok(n > 0)
    }

    /// A present file's path, size and mtime (an analysis worked out when it is asked for). Blocking.
    pub fn file_to_analyse(&self, folder: i64, rel: &str) -> Option<(PathBuf, u64, i64)> {
        let root = usize::try_from(folder).ok().and_then(|i| self.folders().get(i).cloned())?;
        let (size, mtime): (i64, i64) = self
            .db()
            .query_row(&format!("SELECT size, mtime FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"), params![folder, rel], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .ok()
            .flatten()?;
        Some((root.join(rel), size.max(0) as u64, mtime))
    }

    /// Present files that want an analysis: none yet, the file changed since (size or mtime), or an older version;
    /// newest first (by when the track was first seen), at most `limit`: (folder, rel, path, size, mtime). Blocking.
    pub fn unanalysed(&self, limit: usize) -> Result<Vec<(i64, String, PathBuf, u64, i64)>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT f.folder, f.rel, f.size, f.mtime FROM files f
               LEFT JOIN analysis a ON a.folder = f.folder AND a.rel = f.rel
               LEFT JOIN tracks t ON t.id = f.track
             WHERE {PRESENT_F} AND f.state = ?1
               AND (a.rel IS NULL OR a.size != f.size OR a.mtime != f.mtime OR a.version < ?2)
             ORDER BY t.born DESC, f.mtime DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(params![READ, crate::analysis::VERSION, limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?.max(0) as u64, r.get::<_, i64>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (folder, rel, size, mtime) = row?;
            let Some(root) = usize::try_from(folder).ok().and_then(|i| folders.get(i)) else { continue };
            out.push((folder, rel.clone(), root.join(&rel), size, mtime));
        }
        Ok(out)
    }

    /// How many present files have an analysis that is up to date, of how many (the pass's progress). Blocking.
    pub fn analysis_counts(&self) -> Result<(u64, u64)> {
        let db = self.db();
        let (done, all): (i64, i64) = db.query_row(
            &format!(
                "SELECT count(a.rel), count(*) FROM files f LEFT JOIN analysis a
                   ON a.folder = f.folder AND a.rel = f.rel AND a.size = f.size AND a.mtime = f.mtime AND a.version >= ?2
                 WHERE {PRESENT_F} AND f.state = ?1"
            ),
            params![READ, crate::analysis::VERSION],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((done.max(0) as u64, all.max(0) as u64))
    }

    /// The albums' gain and peak in a folder, by album directory (an album is a folder of files, as the served library
    /// counts it), from their tracks' analyses (`analysis::album_gain` over their loudness blocks): only where every
    /// track of the directory has one, so a half-analysed album has none yet. Blocking. (The served library reads them
    /// through its own connection, `album_gains`; this one is the tests'.)
    #[cfg(test)]
    pub fn album_gains(&self, folder: i64) -> Result<HashMap<String, (f32, f32)>> {
        album_gains(&self.db(), folder)
    }

    /// Where a track's file is, or was last seen while it is away (gone, not yet retired). Blocking.
    pub fn file_of_any(&self, track: i64) -> Option<(i64, String)> {
        self.db()
            .query_row("SELECT folder, rel FROM files WHERE track = ?1 ORDER BY gone_at IS NOT NULL LIMIT 1", [track], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()
            .flatten()
    }

    /// When a track was first seen, epoch seconds: when it came into the library.
    pub fn born_of(&self, track: i64) -> Option<i64> {
        self.db().query_row("SELECT born FROM tracks WHERE id = ?1", [track], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn file_of(&self, track: i64) -> Option<(i64, String)> {
        file_of(&self.db(), track)
    }

    /// A note the catalogue keeps (its `meta` table), by key.
    pub fn meta(&self, key: &str) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn set_meta(&self, key: &str, value: &str) {
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

/// A file's playlist line among these folders (see `Catalogue::line_of`).
fn line_in(folders: &[PathBuf], folder: i64, rel: &str) -> Option<String> {
    if folder == 0 {
        return Some(rel.to_string());
    }
    crate::playlists::relative_to(folders.first()?, &folders.get(usize::try_from(folder).ok()?)?.join(rel))
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
    // Also when the folders changed since the last full listing: a folder added later (Lidarr's, 2026-10-14) got
    // only the tracks the server changed since, and its older ones stayed unplayable until the next day's listing.
    let folders = cat.folders().iter().map(|f| f.display().to_string()).collect::<Vec<_>>().join("
");
    let full = started - last_full >= FULL_EVERY_SECS || cat.meta(&format!("full-folders:{id}")).as_deref() != Some(&folders);
    let since = (!full).then_some(last_pass);
    // A full listing that failed goes on from its last page the next pass, instead of asking for everything again:
    // (library, next page's start, when that listing began). What it began with stays the mark for what was not seen.
    let progress = format!("full-from:{id}");
    let resume = full
        .then(|| cat.meta(&progress))
        .flatten()
        .and_then(|v| {
            let mut p = v.split(' ');
            Some((p.next()?.to_string(), p.next()?.parse::<usize>().ok()?, p.next()?.parse::<i64>().ok()?))
        })
        .and_then(|(lib, from, began)| libraries.iter().position(|l| l.id == lib).map(|i| (i, from, began)));
    let began = resume.map_or(started, |r| r.2);
    let mut roots = Roots::default();
    for (i, lib) in libraries.iter().enumerate() {
        let mut start = match resume {
            Some((at, _, _)) if i < at => continue,
            Some((at, from, _)) if i == at => from,
            _ => 0,
        };
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
            if full {
                cat.set_meta(&progress, &format!("{} {start} {began}", lib.id));
            }
        }
        if full {
            let (cat2, sid, lid) = (cat.clone(), id.to_string(), lib.id.clone());
            tokio::task::spawn_blocking(move || cat2.forget_unseen(&sid, &lid, began)).await??;
            if let Some(next) = libraries.get(i + 1) {
                cat.set_meta(&progress, &format!("{} 0 {began}", next.id));
            }
        }
    }
    if full {
        cat.set_meta(&format!("full:{id}"), &began.to_string());
        cat.set_meta(&format!("full-folders:{id}"), &folders);
        cat.set_meta(&progress, "");
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

/// See `Catalogue::served_rows`; any connection to the catalogue's file.
/// How far a file's header may be from the length a whole decode measured before the measured one is served.
const LENGTH_SLACK_MS: u64 = 1_500;

pub fn served_rows(db: &Connection, folder: i64) -> Result<Vec<ServedRow>> {
    served_rows_in(db, folder, None)
}

/// A file's folder within its library folder ("" at its top).
pub fn dir_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The range of `rel`s under `dir` (its files and deeper), for a query on the index: `dir/` up to `dir0` ('0' follows
/// '/'). None for the top, which holds everything.
fn rel_range(dir: Option<&str>) -> Option<(String, String)> {
    dir.filter(|d| !d.is_empty()).map(|d| (format!("{d}/"), format!("{d}0")))
}

/// `served_rows`, of the files right in `dir` only where given (0.9.8: the library builds again just the albums
/// Cratarr wrote to, `Library::update_dirs`).
pub fn served_rows_in(db: &Connection, folder: i64, dir: Option<&str>) -> Result<Vec<ServedRow>> {
    let range = rel_range(dir);
    let (lo, hi) = range.clone().unzip();
    let within = |col: &str| if range.is_some() { format!(" AND {col} >= ?2 AND {col} < ?3") } else { " AND ?2 IS NULL AND ?3 IS NULL".to_string() };
    let mut rows: Vec<ServedRow> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut st = db.prepare(&format!(
        "SELECT f.rel, f.track, t.born, f.format, f.bitrate, f.duration_ms, f.size, f.mtime
         FROM files f LEFT JOIN tracks t ON t.id = f.track WHERE f.folder = ?1 AND {PRESENT} AND {}{}",
        served("f."),
        within("f.rel")
    ))?;
    let files = st.query_map(params![folder, lo, hi], |r| {
        Ok(ServedRow {
            rel: r.get(0)?,
            track: r.get(1)?,
            born: r.get(2)?,
            format: r.get::<_, Option<String>>(3)?.map(|f| f.to_uppercase()),
            kbps: r.get::<_, Option<i64>>(4)?.filter(|k| *k > 0).map(|k| k as u32),
            duration_ms: r.get::<_, Option<i64>>(5)?.filter(|d| *d > 0).map(|d| d as u64),
            size: r.get::<_, i64>(6)?.max(0) as u64,
            mtime: r.get(7)?,
            tags: Vec::new(),
            analysis: None,
        })
    })?;
    for f in files {
        let f = f?;
        // Right in the folder asked for, not deeper.
        if dir.is_some_and(|d| dir_of(&f.rel) != d) {
            continue;
        }
        index.insert(f.rel.clone(), rows.len());
        rows.push(f);
    }
    // Their analyses, while the files are as they were then, and their albums' gains (decision 0035).
    let albums = album_gains_in(db, folder, dir)?;
    let mut st = db.prepare(&format!(
        "SELECT a.rel, a.track_gain_db, a.track_peak, a.bpm, a.duration_secs, a.complete FROM analysis a
         JOIN files f ON f.folder = a.folder AND f.rel = a.rel WHERE a.folder = ?1 AND a.size = f.size AND a.mtime = f.mtime{}",
        within("a.rel")
    ))?;
    let found = st.query_map(params![folder, lo, hi], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<f64>>(1)?,
            r.get::<_, Option<f64>>(2)?,
            r.get::<_, Option<f64>>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, Option<bool>>(5)?,
        ))
    })?;
    for a in found {
        let (rel, gain, peak, bpm, measured, complete) = a?;
        let Some(&i) = index.get(&rel) else { continue };
        match complete {
            // A decode of the whole track: its length stands where the header says otherwise (Gerry Mulligan's Night
            // Lights says ten times its length; 2026-10-16 ffmpeg check of the 52 that disagree).
            Some(true) => {
                if let Some(ms) = measured.filter(|s| *s > 0.0).map(|s| (s * 1000.0).round() as u64)
                    && rows[i].duration_ms.is_none_or(|d| d.abs_diff(ms) > LENGTH_SLACK_MS)
                {
                    rows[i].duration_ms = Some(ms);
                }
            }
            // The decoder stopped early (Symphonia on Jon Hopkins' Contact Note: 7 s of 468): its length, gain, peak
            // and tempo describe a part of the track, so none of it is served.
            Some(false) => continue,
            // Analysed before this was known: served as before.
            None => {}
        }
        let (Some(gain), Some(peak)) = (gain, peak) else { continue };
        let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
        rows[i].analysis = Some(RowAnalysis { track_gain_db: gain as f32, track_peak: peak as f32, bpm: bpm.map(|b| b as f32), album: albums.get(dir).copied() });
    }
    let names = ServedRow::TAGS.map(|n| format!("'{n}'")).join(", ");
    let mut st = db.prepare(&format!("SELECT rel, name, value FROM tags WHERE folder = ?1 AND name IN ({names}){} ORDER BY rel, n", within("rel")))?;
    let tags = st.query_map(params![folder, lo, hi], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    for t in tags {
        let (rel, name, value) = t?;
        if let Some(&i) = index.get(&rel) {
            rows[i].tags.push((name, value));
        }
    }
    Ok(rows)
}

// The analysis' columns as `analysis_row` reads them (the struct's order).
/// The shape's columns come from `analysis_shape s`, joined by every query that names these.
const ANALYSIS_COLS: &str =
    "a.version, a.duration_secs, a.loudness_lufs, a.track_gain_db, a.track_peak, s.blocks, a.bpm, a.bpm_confidence, s.waveform, a.complete";
/// The join `ANALYSIS_COLS` needs, after `analysis a`.
const SHAPE_JOIN: &str = "LEFT JOIN analysis_shape s ON s.folder = a.folder AND s.rel = a.rel";

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn store_analysis(db: &Connection, folder: i64, rel: &str, size: u64, mtime: i64, a: &crate::analysis::Analysis, now: i64) -> Result<()> {
    db.prepare_cached(
        "INSERT OR REPLACE INTO analysis (folder, rel, size, mtime, version, duration_secs, loudness_lufs, track_gain_db,
           track_peak, bpm, bpm_confidence, waveform, blocks, analysed_at, complete)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12, ?13)",
    )?
    .execute(params![
        folder,
        rel,
        size as i64,
        mtime,
        a.version,
        a.duration_secs,
        a.loudness_lufs,
        a.track_gain_db,
        a.track_peak,
        a.bpm,
        a.bpm_confidence,
        now,
        a.complete
    ])?;
    db.prepare_cached("INSERT OR REPLACE INTO analysis_shape (folder, rel, waveform, blocks, beats) VALUES (?1, ?2, ?3, ?4, ?5)")?
        .execute(params![folder, rel, a.waveform, blocks_bytes(&a.blocks), a.beats.as_ref().map(|b| b.0.as_slice())])?;
    Ok(())
}

fn analysis_row(r: &rusqlite::Row) -> rusqlite::Result<crate::analysis::Analysis> {
    analysis_row_at(r, 0)
}

/// The first byte of the stored gating blocks: their format (1: six bytes a bin follow).
const BLOCKS_FORMAT: u8 = 1;

/// The gating blocks as stored: a format byte, then six bytes a bin (its index, then its count, little-endian).
fn blocks_bytes(blocks: &crate::analysis::Loudness) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + blocks.0.len() * 6);
    out.push(BLOCKS_FORMAT);
    for (bin, count) in &blocks.0 {
        out.extend_from_slice(&bin.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
    }
    out
}

/// The gating blocks from their stored form: bytes, or (kept before the shape moved apart) JSON text.
fn blocks_from(value: rusqlite::types::ValueRef) -> crate::analysis::Loudness {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Blob([BLOCKS_FORMAT, b @ ..]) => crate::analysis::Loudness(
            b.chunks_exact(6)
                .map(|c| (u16::from_le_bytes([c[0], c[1]]), u32::from_le_bytes([c[2], c[3], c[4], c[5]])))
                .collect(),
        ),
        ValueRef::Text(t) => std::str::from_utf8(t).ok().and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default(),
        _ => Default::default(),
    }
}

/// The waveforms and gating blocks kept in `analysis` itself move to `analysis_shape` (the blocks re-encoded), and are
/// cleared there: at every start, which finds nothing to do but after the first (or after an older build kept some there,
/// a rollback). The first time, the file is vacuumed too, so it shrinks: the move is one transaction, marked `moved`;
/// the vacuum after it, marked `apart`; a start that finds `moved` (stopped while vacuuming) vacuums again. Runs before
/// the catalogue is shared, on its only connection.
fn move_analysis_shapes(conn: &Connection) -> Result<()> {
    let mark: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'analysis_shape'", [], |r| r.get(0)).optional()?;
    let tx = conn.unchecked_transaction()?;
    let rows: Vec<(i64, String, Option<Vec<u8>>, crate::analysis::Loudness)> = {
        let mut st = tx.prepare("SELECT folder, rel, waveform, blocks FROM analysis WHERE waveform IS NOT NULL OR blocks IS NOT NULL")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, blocks_from(r.get_ref(3)?))))?.collect::<rusqlite::Result<_>>()?
    };
    if rows.is_empty() && mark.as_deref() == Some("apart") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    if !rows.is_empty() {
        tracing::info!("catalogue: moving {} analyses' waveforms and gating blocks apart (decision 0035)", rows.len());
        let mut put = tx.prepare("INSERT OR REPLACE INTO analysis_shape (folder, rel, waveform, blocks) VALUES (?1, ?2, ?3, ?4)")?;
        for (folder, rel, waveform, blocks) in &rows {
            put.execute(params![folder, rel, waveform, blocks_bytes(blocks)])?;
        }
        drop(put);
        tx.execute("UPDATE analysis SET waveform = NULL, blocks = NULL WHERE waveform IS NOT NULL OR blocks IS NOT NULL", [])?;
    }
    if mark.as_deref() != Some("apart") {
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('analysis_shape', 'moved')", [])?;
    }
    tx.commit()?;
    if mark.as_deref() != Some("apart") {
        tracing::info!("catalogue: vacuuming the file once, so it shrinks (about a minute for a large library)");
        // In the memory the connection keeps temporary storage in (crate::db): the image has no /tmp. Never set back to
        // the default here: this is the connection the catalogue keeps, and every large sort after it failed (Crates,
        // preview 50's first start, 2026-10-17).
        conn.execute_batch(
            "VACUUM;
             INSERT OR REPLACE INTO meta (key, value) VALUES ('analysis_shape', 'apart');",
        )?;
    }
    tracing::info!("catalogue: analyses' shapes apart ({} moved) in {:.1} s", rows.len(), started.elapsed().as_secs_f64());
    Ok(())
}

/// An analysis from `ANALYSIS_COLS` starting at column `at`.
fn analysis_row_at(r: &rusqlite::Row, at: usize) -> rusqlite::Result<crate::analysis::Analysis> {
    let blocks = blocks_from(r.get_ref(at + 5)?);
    Ok(crate::analysis::Analysis {
        version: r.get(at)?,
        duration_secs: r.get::<_, Option<f64>>(at + 1)?.unwrap_or(0.0),
        loudness_lufs: r.get::<_, Option<f64>>(at + 2)?.unwrap_or(0.0) as f32,
        track_gain_db: r.get::<_, Option<f64>>(at + 3)?.unwrap_or(0.0) as f32,
        track_peak: r.get::<_, Option<f64>>(at + 4)?.unwrap_or(0.0) as f32,
        blocks,
        bpm: r.get::<_, Option<f64>>(at + 6)?.map(|b| b as f32),
        bpm_confidence: r.get::<_, Option<f64>>(at + 7)?.unwrap_or(0.0) as f32,
        waveform: r.get::<_, Option<Vec<u8>>>(at + 8)?.unwrap_or_default(),
        complete: r.get(at + 9)?,
        // Read apart (`Catalogue::beats_of`): a few kilobytes no list needs.
        beats: None,
    })
}

/// The albums' gain and peak in a folder, by album directory (an album is a folder of files, as the served library
/// counts it), from their tracks' analyses (`analysis::album_gain` over their loudness blocks): only where every track
/// of the directory has one, so a half-analysed album has none yet. Any connection to the catalogue's file.
#[cfg(test)]
pub fn album_gains(db: &Connection, folder: i64) -> Result<HashMap<String, (f32, f32)>> {
    album_gains_in(db, folder, None)
}

/// `album_gains`, of the albums right in `dir` only where given.
fn album_gains_in(db: &Connection, folder: i64, dir: Option<&str>) -> Result<HashMap<String, (f32, f32)>> {
    let range = rel_range(dir);
    let (lo, hi) = range.clone().unzip();
    let within = if range.is_some() { " AND f.rel >= ?2 AND f.rel < ?3" } else { " AND ?2 IS NULL AND ?3 IS NULL" };
    let mut st = db.prepare(&format!(
        "SELECT f.rel, {ANALYSIS_COLS} FROM files f
           LEFT JOIN analysis a ON a.folder = f.folder AND a.rel = f.rel AND a.size = f.size AND a.mtime = f.mtime
           {SHAPE_JOIN}
         WHERE {PRESENT_F} AND f.folder = ?1 AND {}{within}",
        served("f.")
    ))?;
    let mut dirs: HashMap<String, Option<Vec<crate::analysis::Analysis>>> = HashMap::new();
    let rows = st.query_map(params![folder, lo, hi], |r| {
        let rel: String = r.get(0)?;
        // The version: NULL where the file has no analysis (the LEFT JOIN).
        let has: Option<i64> = r.get(1)?;
        Ok((rel, if has.is_some() { Some(analysis_row_at(r, 1)?) } else { None }))
    })?;
    for row in rows {
        let (rel, a) = row?;
        let in_dir = dir_of(&rel);
        if dir.is_some_and(|d| in_dir != d) {
            continue;
        }
        let entry = dirs.entry(in_dir.to_string()).or_insert_with(|| Some(Vec::new()));
        match (entry.as_mut(), a) {
            // A track whose analysis covers only part of it: as a track without one (decision 0035, 2026-10-17).
            (Some(_), Some(a)) if a.complete == Some(false) => *entry = None,
            (Some(list), Some(a)) => list.push(a),
            // A track without one: the album waits.
            (Some(_), None) => *entry = None,
            (None, _) => {}
        }
    }
    Ok(dirs
        .into_iter()
        .filter_map(|(dir, list)| {
            let list = list?;
            let refs: Vec<&crate::analysis::Analysis> = list.iter().collect();
            crate::analysis::album_gain(&refs).map(|g| (dir, g))
        })
        .collect())
}

/// See `Catalogue::track_of_source`; on the catalogue's connection, held (`files_of_sources` asks for many at once).
fn track_of_source(db: &Connection, source: &str, item: &str) -> Option<i64> {
    if source == crate::source::cratarr::KIND.id {
        let track = crate::serve::ids::track_of(item)?;
        let present = format!("SELECT 1 FROM files WHERE track = ?1 AND {PRESENT}");
        return db.query_row(&present, [track], |_| Ok(())).optional().ok().flatten().map(|()| track);
    }
    db.query_row(
        &format!(
            "SELECT f.track FROM source_ids s JOIN files f ON f.folder = s.folder AND f.rel = s.rel
             WHERE s.source = ?1 AND s.track_id = ?2 AND {}",
            PRESENT_F
        ),
        params![source, item],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
}

/// See `Catalogue::file_of`.
fn file_of(db: &Connection, track: i64) -> Option<(i64, String)> {
    db.query_row(&format!("SELECT folder, rel FROM files WHERE track = ?1 AND {PRESENT}"), [track], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()
}

/// See `Catalogue::duration_of`.
fn duration_of(db: &Connection, folder: i64, rel: &str) -> Option<u64> {
    db.query_row(&format!("SELECT duration_ms FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"), params![folder, rel], |r| {
        r.get::<_, Option<i64>>(0)
    })
    .optional()
    .ok()
    .flatten()
    .flatten()
    .filter(|d| *d > 0)
    .map(|d| d as u64)
}

/// The cover files of each folder of a library folder (0.9.8), as a print: an album built again when its folder's
/// cover files change (`Library`, an album's `src`).
pub fn cover_prints(db: &Connection) -> Result<HashMap<(i64, String), String>> {
    let mut st = db.prepare("SELECT folder, rel, size, mtime FROM cover_files ORDER BY folder, rel")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))?;
    let mut out: HashMap<(i64, String), String> = HashMap::new();
    for row in rows {
        let (folder, rel, size, mtime) = row?;
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let p = out.entry((folder, dir_of(&rel).to_string())).or_default();
        p.push_str(&format!("{name}:{size}:{mtime};"));
    }
    Ok(out)
}

/// See `Catalogue::served_fingerprint`; any connection to the catalogue's file.
pub fn served_fingerprint(db: &Connection) -> Result<String> {
    // How many are read counts too: a file changed (served as it was read) and then read again has the same size and
    // time, and its new tags are to be built.
    let (n, size, mtime, tracks, folders, read): (i64, f64, f64, f64, f64, i64) = db.query_row(
        &format!(
            "SELECT count(*), total(size), total(mtime), total(track), total(folder), count(CASE WHEN state = {READ} THEN 1 END)
             FROM files WHERE {PRESENT} AND {}",
            served("")
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
    )?;
    // A cover file changed, came or went (0.9.7) builds it again too: its album's stamp moves.
    let (covers, cover_size, cover_mtime): (i64, f64, f64) =
        db.query_row("SELECT count(*), total(size), total(mtime) FROM cover_files", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    // New analyses build the library again too (their gains and tempi are the songs').
    let (analysed, at): (i64, f64) = db.query_row("SELECT count(*), total(analysed_at) FROM analysis", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    // How many are read goes with the analyses' tail (0.9.7): tags read again over unchanged files (a new reader's
    // reread of every file, hours on the NAS) build the library at most every so often, as new analyses do, and not
    // after every pass: each build had the radio and the crates read everything again, and the sim went past 768 MB.
    // A file Cratarr wrote changes its size and time, which still builds at once.
    Ok(format!("{n}/{size}/{mtime}/{tracks}/{folders} covers {covers}/{cover_size}/{cover_mtime} analysis {analysed}/{at}/{read}"))
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
    fn the_write_ahead_log_is_cut_back_after_a_burst() {
        let d = music();
        let file = d.join("catalogue.sqlite");
        let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        let wal = || std::fs::metadata(d.join("catalogue.sqlite-wal")).map(|m| m.len()).unwrap_or(0);
        // About 26 MB in one transaction, far over the limit.
        cat.db()
            .execute_batch(
                "CREATE TABLE burst (b BLOB);
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 400)
                 INSERT INTO burst SELECT randomblob(65536) FROM n;",
            )
            .unwrap();
        assert!(wal() > 20 << 20, "the burst went through the log: {} bytes", wal());
        cat.db().execute_batch("PRAGMA wal_checkpoint(PASSIVE); DELETE FROM burst; INSERT INTO meta (key, value) VALUES ('x', '1');").unwrap();
        cat.db().execute_batch("PRAGMA wal_checkpoint(PASSIVE); INSERT OR REPLACE INTO meta (key, value) VALUES ('x', '2');").unwrap();
        assert!(wal() <= 8 << 20, "cut back to the limit: {} bytes", wal());
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_albums_length_is_its_files_summed_and_unknown_when_one_is() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        let facts = cat.album_facts(0).unwrap();
        let f = facts.iter().find(|f| f.dir == "A - B (2020)").unwrap();
        assert_eq!((f.tracks, f.duration_ms), (2, Some(400_000)));
        cat.db().execute("UPDATE files SET duration_ms = NULL WHERE rel = 'A - B (2020)/02 Two.flac'", []).unwrap();
        let facts = cat.album_facts(0).unwrap();
        let f = facts.iter().find(|f| f.dir == "A - B (2020)").unwrap();
        assert_eq!((f.tracks, f.duration_ms), (2, None), "one file's length unknown: the album's is");
        std::fs::remove_dir_all(&d).ok();
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

    fn an_analysis(gain: f32, bin: u16) -> crate::analysis::Analysis {
        crate::analysis::Analysis {
            version: crate::analysis::VERSION,
            duration_secs: 200.0,
            loudness_lufs: -18.0 - gain,
            track_gain_db: gain,
            track_peak: 0.9,
            blocks: crate::analysis::Loudness(vec![(bin, 40)]),
            bpm: Some(120.0),
            bpm_confidence: 0.9,
            waveform: vec![7; crate::analysis::WAVEFORM_BINS],
            complete: Some(true),
            beats: None,
        }
    }

    #[test]
    fn beats_are_kept_beside_their_analysis_and_only_for_the_file_as_it_was() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let todo = cat.unanalysed(10).unwrap();
        let (folder, one, path, size, mtime) = todo.iter().find(|t| t.1.ends_with("One.mp3")).unwrap().clone();
        let two = todo.iter().find(|t| t.1.ends_with("Two.flac")).unwrap().clone();
        assert_eq!(cat.file_to_analyse(folder, &one), Some((path, size, mtime)));
        assert_eq!(cat.beats_of(folder, &one), None, "no analysis");
        // An analysis from before beats were kept: none worked out.
        cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
        assert_eq!(cat.beats_of(folder, &one), Some(None));
        // Not for the file as it was before: nothing kept.
        let b = crate::analysis::Beats(vec![244, 1, 0, 0, 255, 232, 3, 0, 0, 128]);
        assert!(!cat.store_beats(folder, &one, size + 1, mtime, &b, false).unwrap());
        assert_eq!(cat.beats_of(folder, &one), Some(None));
        assert!(cat.store_beats(folder, &one, size, mtime, &b, false).unwrap());
        assert_eq!(cat.beats_of(folder, &one), Some(Some(b.clone())));
        assert_eq!(b.each().collect::<Vec<_>>(), vec![(0.5, 255), (1.0, 128)]);
        // The rest of the analysis is as it was; its reads leave the beats out.
        let back = cat.analysis_of(folder, &one).unwrap();
        assert_eq!((back.track_gain_db, back.waveform.len(), back.beats), (-3.0, 512, None));
        // None (no tempo) is an answer, not "not worked out"; and a whole analysis keeps its beats too.
        let none = crate::analysis::Analysis { beats: Some(crate::analysis::Beats::default()), ..an_analysis(-5.0, 480) };
        cat.store_analysis(two.0, &two.1, two.3, two.4, &none).unwrap();
        assert_eq!(cat.beats_of(two.0, &two.1), Some(Some(crate::analysis::Beats::default())));
        // An analysis without a shape row (none is written so; an older build's might be) gets one for its beats.
        cat.db().execute("DELETE FROM analysis_shape WHERE folder = ?1 AND rel = ?2", params![folder, one]).unwrap();
        assert_eq!(cat.beats_of(folder, &one), Some(None));
        assert!(!cat.store_beats(folder, &one, size, mtime + 1, &b, false).unwrap(), "still only for the file as analysed");
        assert!(cat.store_beats(folder, &one, size, mtime, &b, true).unwrap());
        assert!(cat.beats_are_beat_this(folder, &one));
        assert!(cat.store_beats(folder, &one, size, mtime, &b, false).unwrap());
        assert!(!cat.beats_are_beat_this(folder, &one), "the analysis' own again");
        assert_eq!(cat.beats_of(folder, &one), Some(Some(b.clone())));
        // A file changed since has none.
        std::fs::write(d.join(&one), b"changed").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.beats_of(folder, &one), None);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_whose_bands_fail_is_marked_and_not_tried_again_until_it_changes() {
        use crate::analysis::band_wave::{BandWave, FAILED, FORMAT, LEN};
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let todo = cat.unanalysed(10).unwrap();
        let (folder, one, _, size, mtime) = todo.iter().find(|t| t.1.ends_with("One.mp3")).unwrap().clone();
        let two = todo.iter().find(|t| t.1.ends_with("Two.flac")).unwrap().clone();
        cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
        cat.store_analysis(two.0, &two.1, two.3, two.4, &an_analysis(-5.0, 480)).unwrap();
        let wanted = |cat: &Catalogue| cat.without_band_wave(10).unwrap().into_iter().map(|t| t.1).collect::<Vec<_>>();
        assert_eq!(wanted(&cat).len(), 2);
        // One fails: marked, and the pass no longer asks for it; the other, worked out, neither.
        assert!(cat.store_band_wave_failed(folder, &one, size, mtime).unwrap());
        assert!(cat.band_wave_of(folder, &one).unwrap().unwrap().failed());
        let mut w = vec![FORMAT];
        w.resize(LEN, 9);
        assert!(cat.store_band_wave(two.0, &two.1, two.3, two.4, &BandWave(w)).unwrap());
        assert!(wanted(&cat).is_empty());
        // An older form's mark is not this one's: asked for again.
        cat.store_band_wave_failed(folder, &one, size, mtime).unwrap();
        cat.db().execute("UPDATE analysis_shape SET band_wave = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, one, vec![FAILED - 1]]).unwrap();
        assert_eq!(wanted(&cat), vec![one.clone()]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_analysis_serves_while_its_file_is_as_it_was_and_follows_a_move() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let todo = cat.unanalysed(10).unwrap();
        assert_eq!(todo.len(), 2);
        let (folder, one, path, size, mtime) = todo.iter().find(|t| t.1.ends_with("One.mp3")).unwrap().clone();
        assert_eq!(path, d.join(&one));
        cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
        let back = cat.analysis_of(folder, &one).unwrap();
        assert_eq!((back.track_gain_db, back.bpm, back.waveform.len(), back.blocks.0.clone()), (-3.0, Some(120.0), 512, vec![(500, 40)]));
        assert_eq!(back.complete, Some(true));
        // Not known, as for an analysis from before it was recorded, stays not known.
        cat.store_analysis(folder, &one, size, mtime, &crate::analysis::Analysis { complete: None, ..an_analysis(-3.0, 500) }).unwrap();
        assert_eq!(cat.analysis_of(folder, &one).unwrap().complete, None);
        assert_eq!(cat.analysis_counts().unwrap(), (1, 2));
        // The crates' BPM range finds it by its analysis (the file has no BPM tag).
        assert_eq!(cat.crate_facts().unwrap().iter().find(|f| f.rel == one).unwrap().bpm, Some(120.0));
        assert!(cat.album_gains(folder).unwrap().is_empty(), "the album waits for its last track");
        let two = cat.unanalysed(10).unwrap().remove(0);
        assert!(two.1.ends_with("Two.flac"));
        cat.store_analysis(two.0, &two.1, two.3, two.4, &an_analysis(-5.0, 480)).unwrap();
        assert!(cat.unanalysed(10).unwrap().is_empty());
        assert!(cat.album_gains(folder).unwrap().contains_key("A - B (2020)"));
        // An analysis of only part of its track: no BPM from it, and its album waits as for a track without one.
        let partial = crate::analysis::Analysis { complete: Some(false), ..an_analysis(-5.0, 480) };
        cat.store_analysis(two.0, &two.1, two.3, two.4, &partial).unwrap();
        assert_eq!(cat.crate_facts().unwrap().iter().find(|f| f.rel == two.1).unwrap().bpm, None);
        assert!(cat.album_gains(folder).unwrap().is_empty(), "a partial track holds its album back");
        cat.store_analysis(two.0, &two.1, two.3, two.4, &an_analysis(-5.0, 480)).unwrap();
        assert!(cat.album_gains(folder).unwrap().contains_key("A - B (2020)"));
        // A file changed since is analysed again, and its old analysis no longer serves.
        std::fs::write(d.join(&one), b"changed, so analysed again").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert!(cat.analysis_of(folder, &one).is_none());
        assert_eq!(cat.unanalysed(10).unwrap().len(), 1);
        // It follows its file.
        cat.move_files(folder, "A - B (2020)", folder, "A - B (2020) moved").unwrap();
        assert!(cat.analysis_of(folder, &two.1.replace("A - B (2020)", "A - B (2020) moved")).is_some());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_changed_file_is_served_as_it_was_read_until_it_is_read_again() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let folder = cat.unanalysed(10).unwrap()[0].0;
        let before = cat.served_fingerprint().unwrap();
        let rows = cat.served_rows(folder).unwrap();
        let one = rows.iter().find(|r| r.rel.ends_with("One.mp3")).unwrap().rel.clone();
        cat.db().execute("UPDATE files SET format = 'mp3' WHERE format IS NULL", []).unwrap();
        // Changed on disk (Cratarr's own write, or another's): waiting to be read, served as it was.
        std::fs::write(d.join(&one), b"its tags written again").unwrap();
        cat.walk_all();
        let state: i64 = cat.db().query_row("SELECT state FROM files WHERE rel = ?1", [&one], |r| r.get(0)).unwrap();
        assert_eq!(state, PENDING);
        assert!(cat.served_rows(folder).unwrap().iter().any(|r| r.rel == one), "still served");
        let pending = cat.served_fingerprint().unwrap();
        assert_ne!(pending, before, "its new size is the library's to build");
        // Read again as it is now: the same size and time, but read, so built again with its new tags.
        cat.db().execute("UPDATE files SET state = ?1 WHERE rel = ?2", params![READ, one]).unwrap();
        assert_ne!(cat.served_fingerprint().unwrap(), pending, "its new tags are the library's to build");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_cover_file_changed_on_disk_builds_once_and_an_embedded_cover_adds_nothing() {
        let d = music();
        // The other album's cover is inside its files only: no file beside them.
        std::fs::create_dir_all(d.join("C - D (2021)")).unwrap();
        std::fs::write(d.join("C - D (2021)/01 Three.mp3"), b"its cover inside").unwrap();
        std::fs::write(d.join("A - B (2020)/cover.jpg"), b"a picture").unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let kept: i64 = cat.db().query_row("SELECT count(*) FROM cover_files", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, 1, "the cover beside A - B; none for the embedded one");
        let first = cat.served_fingerprint().unwrap();
        cat.walk_all();
        assert_eq!(cat.served_fingerprint().unwrap(), first, "as it was: no change");
        // Touched only (the same picture, another time): one change, not one each pass.
        let f = std::fs::File::options().write(true).open(d.join("A - B (2020)/cover.jpg")).unwrap();
        f.set_modified(std::time::SystemTime::now() + Duration::from_secs(60)).unwrap();
        drop(f);
        cat.walk_all();
        let touched = cat.served_fingerprint().unwrap();
        assert_ne!(touched, first, "the library builds, the stamp moves");
        cat.walk_all();
        assert_eq!(cat.served_fingerprint().unwrap(), touched, "counted once");
        // Gone: a change again.
        std::fs::remove_file(d.join("A - B (2020)/cover.jpg")).unwrap();
        cat.walk_all();
        assert_ne!(cat.served_fingerprint().unwrap(), touched);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_reread_reads_cratarrs_own_write_at_once() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let (folder, one, path, size, mtime) = cat.unanalysed(10).unwrap().into_iter().find(|t| t.1.ends_with("One.mp3")).unwrap();
        cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
        std::fs::write(&path, b"its tags written again, its audio as it was").unwrap();
        assert_eq!(cat.reread(&[path.clone(), d.join("not known.mp3")]).unwrap(), 1, "the file the catalogue knows");
        let (state, size_now): (i64, i64) =
            cat.db().query_row("SELECT state, size FROM files WHERE folder = ?1 AND rel = ?2", params![folder, one], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_ne!(state, PENDING, "read now, not at the next pass");
        assert_eq!(size_now as u64, std::fs::metadata(&path).unwrap().len());
        assert!(cat.analysis_of(folder, &one).is_some(), "its analysis carried over");
        assert!(cat.last_pass().is_some(), "the library builds from it");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_measured_length_is_served_only_from_a_whole_decode() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let todo = cat.unanalysed(10).unwrap();
        let (folder, one) = (todo[0].0, todo.iter().find(|t| t.1.ends_with("One.mp3")).unwrap().clone());
        let two = todo.iter().find(|t| t.1.ends_with("Two.flac")).unwrap().clone();
        let served = |end: &str| cat.served_rows(folder).unwrap().into_iter().find(|r| r.rel.ends_with(end)).unwrap();
        // Night Lights: the header says ten times the length a whole decode found.
        cat.db().execute("UPDATE files SET duration_ms = 2000000 WHERE rel = ?1", params![one.1]).unwrap();
        let measured = |complete| crate::analysis::Analysis { duration_secs: 200.0, complete, ..an_analysis(-3.0, 500) };
        cat.store_analysis(folder, &one.1, one.3, one.4, &measured(Some(true))).unwrap();
        assert_eq!(served("One.mp3").duration_ms, Some(200_000), "a whole decode");
        // Contact Note: the decoder stopped early. Neither its length nor its gain is served.
        cat.store_analysis(folder, &one.1, one.3, one.4, &measured(Some(false))).unwrap();
        let row = served("One.mp3");
        assert_eq!((row.duration_ms, row.analysis.is_none()), (Some(2_000_000), true), "stopped early");
        // From before it was known: as before, the header's length and the gain.
        cat.store_analysis(folder, &one.1, one.3, one.4, &measured(None)).unwrap();
        let row = served("One.mp3");
        assert_eq!((row.duration_ms, row.analysis.is_some()), (Some(2_000_000), true), "not known");
        // A second off is the header's to keep.
        cat.db().execute("UPDATE files SET duration_ms = 201000 WHERE rel = ?1", params![two.1]).unwrap();
        cat.store_analysis(folder, &two.1, two.3, two.4, &measured(Some(true))).unwrap();
        assert_eq!(served("Two.flac").duration_ms, Some(201_000));
        std::fs::remove_dir_all(&d).ok();
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

    #[test]
    fn analyses_kept_whole_move_their_shape_apart_once_and_read_the_same() {
        let d = music();
        let file = d.join("catalogue.sqlite");
        let (folder, one, size, mtime) = {
            let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
            cat.walk_all();
            read_as_audio(&cat);
            let (folder, one, _, size, mtime) = cat.unanalysed(10).unwrap().into_iter().find(|t| t.1.ends_with("One.mp3")).unwrap();
            cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
            let (shape_waveform, kept_whole): (Vec<u8>, Option<Vec<u8>>) = cat
                .db()
                .query_row("SELECT s.waveform, a.waveform FROM analysis a JOIN analysis_shape s USING (folder, rel) WHERE a.rel = ?1", [&one], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap();
            assert_eq!((shape_waveform.len(), kept_whole), (crate::analysis::WAVEFORM_BINS, None), "kept apart from the start");
            // As a build before the move kept it: whole, the blocks as JSON, no shape row, no mark.
            cat.db()
                .execute_batch(&format!(
                    "UPDATE analysis SET waveform = x'{}', blocks = '[[500,40],[501,2]]';
                     DELETE FROM analysis_shape; DELETE FROM meta WHERE key = 'analysis_shape';",
                    "07".repeat(crate::analysis::WAVEFORM_BINS)
                ))
                .unwrap();
            (folder, one, size, mtime)
        };
        let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        let a = cat.analysis_of(folder, &one).unwrap();
        assert_eq!((a.waveform.len(), a.waveform[0], a.blocks.0.clone()), (crate::analysis::WAVEFORM_BINS, 7, vec![(500, 40), (501, 2)]));
        let (whole, mark): (i64, String) = cat
            .db()
            .query_row("SELECT (SELECT count(*) FROM analysis WHERE waveform IS NOT NULL OR blocks IS NOT NULL), (SELECT value FROM meta WHERE key = 'analysis_shape')", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((whole, mark.as_str()), (0, "apart"));
        let temp_store: i64 = cat.db().query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(temp_store, 2, "temporary storage stays in memory after the vacuum: the image has no /tmp");
        // The album's gain reads the moved blocks; the file as it is is analysed again only when changed.
        assert!(cat.unanalysed(10).unwrap().iter().all(|t| t.1 != one));
        let _ = (size, mtime);
        drop(cat);
        // Opened again: nothing to do.
        let again = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        assert_eq!(again.analysis_of(folder, &one).unwrap().blocks.0, vec![(500, 40), (501, 2)]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn blocks_keep_their_format_byte_and_older_json_still_reads() {
        use rusqlite::types::ValueRef;
        let blocks = crate::analysis::Loudness(vec![(0, 1), (700, 70_000), (u16::MAX, u32::MAX)]);
        let bytes = blocks_bytes(&blocks);
        assert_eq!((bytes[0], bytes.len()), (BLOCKS_FORMAT, 1 + 3 * 6));
        assert_eq!(blocks_from(ValueRef::Blob(&bytes)).0, blocks.0);
        assert_eq!(blocks_from(ValueRef::Text(b"[[500,40]]")).0, vec![(500, 40)]);
        assert!(blocks_from(ValueRef::Blob(&[9, 1, 2, 3, 4, 5, 6])).0.is_empty(), "an unknown format reads as none");
        assert!(blocks_from(ValueRef::Null).0.is_empty());
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
        // Moved again, and a walk saw the files at their new place before the rows followed: the rows it made there
        // give way, and the track keeps its id.
        std::fs::rename(a.join("A - B (2020) adopted"), a.join("A - B (2020) kept")).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.move_files(0, "A - B (2020) adopted", 0, "A - B (2020) kept").unwrap(), 2);
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020) kept/01 One.mp3".to_string())));
        std::fs::rename(a.join("A - B (2020) kept"), a.join("A - B (2020) adopted")).unwrap();
        assert_eq!(cat.move_files(0, "A - B (2020) kept", 0, "A - B (2020) adopted").unwrap(), 2);
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
    fn a_playlist_line_names_a_file_in_music_or_beside_it() {
        let root = music();
        let (m, inc) = (root.join("Music"), root.join("lidarr-music"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X/New")).unwrap();
        std::fs::write(m.join("A/01.mp3"), "a").unwrap();
        std::fs::write(inc.join("X/New/01.flac"), "x").unwrap();
        let cat = Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        assert_eq!(cat.place_of_line("A/01.mp3"), Some((0, "A/01.mp3".to_string())));
        assert_eq!(cat.place_of_line("../lidarr-music/X/New/01.flac"), Some((1, "X/New/01.flac".to_string())));
        assert_eq!(cat.place_of_line("../elsewhere/01.flac"), None, "in no folder");
        assert_eq!(cat.place_of_line("../../../../../../../../x"), None);
        cat.record_links("jf", "lib", &[("i1".to_string(), 1, "X/New/01.flac".to_string())], 1).unwrap();
        let lines = ["../lidarr-music/X/New/01.flac".to_string(), "../elsewhere/01.flac".to_string()];
        assert_eq!(cat.line_track_ids("jf", &lines).unwrap(), vec![Some("i1".to_string()), None]);
        assert_eq!(cat.line_rows("jf", &lines).unwrap()[0].track_id.as_deref(), Some("i1"));
        // And the other way round: a file's line, its file on the disk, a server id's line.
        assert_eq!(cat.line_of(1, "X/New/01.flac").as_deref(), Some(lines[0].as_str()));
        assert_eq!(cat.line_of(0, "A/01.mp3").as_deref(), Some("A/01.mp3"));
        assert_eq!(cat.line_of(7, "A/01.mp3"), None, "no such folder");
        assert_eq!(cat.path_of_line(&lines[0]), Some(inc.join("X/New/01.flac")));
        assert_eq!(cat.lines_of("jf", &["i1".to_string()]).unwrap(), [lines[0].clone()].into_iter().collect());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn crate_facts_hold_every_folder_in_use_each_file_as_its_line() {
        let root = music();
        let (m, inc) = (root.join("Music"), root.join("lidarr-music"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X/New")).unwrap();
        std::fs::write(m.join("A/01.mp3"), "a").unwrap();
        std::fs::write(inc.join("X/New/01.flac"), "x").unwrap();
        let cat = Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap();
        cat.walk_all();
        let facts = cat.crate_facts().unwrap();
        let mut lines: Vec<(i64, &str)> = facts.iter().map(|f| (f.folder, f.rel.as_str())).collect();
        lines.sort();
        assert_eq!(lines, [(0, "A/01.mp3"), (1, "../lidarr-music/X/New/01.flac")], "Incoming's climb out of Music");
        // Out of use, a folder's files leave the crates.
        cat.set_folders(vec![m.clone()]).unwrap();
        assert_eq!(cat.crate_facts().unwrap().iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), ["A/01.mp3"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_files_credits_are_every_artist_it_names_featured_ones_too() {
        // Decision 0083: its artist and album artist tags and its artists list, each credit split where it joins several;
        // a compilation's album artist is no one.
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let one = "A - B (2020)/01 One.mp3".to_string();
        let db = cat.db();
        let tags = [(0, "artist", "Air feat. Beck"), (1, "albumartist", "Various Artists"), (2, "artists", "Air"), (3, "artists", "Jarvis Cocker")];
        for (n, name, value) in tags {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", params![one, n, name, value]).unwrap();
        }
        drop(db);
        let facts = cat.crate_facts().unwrap();
        let f = facts.iter().find(|f| f.rel == one).unwrap();
        assert_eq!(f.artists, ["Air feat. Beck"], "shown as tagged");
        assert_eq!(f.credits, ["air", "beck", "jarviscocker"]);
        assert_eq!(f.credit_names, ["Air", "Beck", "Jarvis Cocker"], "as the Artist filter offers them");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_files_labels_are_its_label_tags_by_name_or_by_raw_key() {
        // 0.11, quick crates: lofty reads ID3v2 TPUB (and Vorbis PUBLISHER) as the publisher, kept by its raw key alone;
        // Vorbis LABEL as "label". Several in one value are several.
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let (one, two) = ("A - B (2020)/01 One.mp3".to_string(), "A - B (2020)/02 Two.flac".to_string());
        let db = cat.db();
        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 0, 'TPUB', NULL, 'Warp; Bleep')", params![one]).unwrap();
        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 0, 'LABEL', 'label', 'Ninja Tune')", params![two]).unwrap();
        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 1, 'ORGANIZATION', 'label', 'ninja tune')", params![two]).unwrap();
        drop(db);
        let facts = cat.crate_facts().unwrap();
        let labels = |rel: &str| facts.iter().find(|f| f.rel == rel).unwrap().labels.clone();
        assert_eq!(labels(one.as_str()), ["Warp", "Bleep"]);
        assert_eq!(labels(two.as_str()), ["Ninja Tune"], "one label once, in any case");
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
    fn crate_facts_are_kept_until_the_catalogue_changes_and_a_while_has_passed() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let first = cat.crate_facts().unwrap();
        assert!(!first.is_empty());
        assert!(Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "nothing changed: the same facts");
        cat.record_links("jellyfin", "lib", &[("t1".into(), 0, first[0].rel.clone())], 100).unwrap();
        assert!(Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "a change just after: still the same (0.9.7)");
        // As if read longer ago than FACTS_EVERY.
        if let Some(k) = cat.facts.lock().unwrap().as_mut() {
            k.at = std::time::Instant::now() - FACTS_EVERY - Duration::from_secs(1);
        }
        assert!(!Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "a change a while after: read again");
        std::fs::remove_dir_all(&d).ok();
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
    fn a_playlists_rows_come_from_the_catalogue_in_order() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let (one, two) = ("A - B (2020)/01 One.mp3".to_string(), "A - B (2020)/02 Two.flac".to_string());
        let db = cat.db();
        for (n, name, value) in [(0, "title", "One"), (1, "artist", "A"), (2, "artist", "Feat"), (3, "album", "B")] {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", params![one, n, name, value]).unwrap();
        }
        db.execute("UPDATE files SET duration_ms = 61500 WHERE rel = ?1", params![one]).unwrap();
        drop(db);
        cat.record_links("jellyfin", "lib", &[("t1".into(), 0, one.clone())], 100).unwrap();
        let rows = cat.file_rows("jellyfin", 0, &[two.clone(), one.clone(), "gone.mp3".into()]).unwrap();
        assert_eq!(rows.len(), 3, "one row per line, in the playlist's order");
        assert_eq!(rows[0], FileRow::default(), "a file without tags or a link has no facts");
        assert_eq!(
            rows[1],
            FileRow {
                track_id: Some("t1".into()),
                title: Some("One".into()),
                artist: Some("A".into()),
                album: Some("B".into()),
                duration_ms: Some(61500),
                ..FileRow::default()
            },
            "the first value of a tag that has several"
        );
        assert_eq!(rows[2], FileRow::default());
        // Its tempo: the BPM tag, else the analysis' while the file is as analysed, rounded.
        let db = cat.db();
        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 0, 'bpm', 'bpm', '92,5')", params![two]).unwrap();
        db.execute(
            "INSERT INTO analysis (folder, rel, size, mtime, version, bpm, analysed_at) SELECT folder, rel, size, mtime, 1, 136.97, 0 FROM files WHERE rel = ?1",
            params![one],
        )
        .unwrap();
        drop(db);
        let rows = cat.file_rows("jellyfin", 0, &[two.clone(), one.clone()]).unwrap();
        assert_eq!((rows[0].bpm, rows[1].bpm), (Some(93), Some(137)));
        // A whole decode's length where the header said otherwise; an early-stopped decode gives nothing; old rows
        // (complete unknown) keep the header's length.
        let length = |complete: Option<bool>, secs: f64| {
            cat.db().execute("UPDATE analysis SET duration_secs = ?2, complete = ?3 WHERE rel = ?1", params![one, secs, complete]).unwrap();
            let r = cat.file_rows("jellyfin", 0, &[one.clone()]).unwrap().remove(0);
            (r.duration_ms, r.bpm)
        };
        assert_eq!(length(Some(true), 408.2), (Some(408_200), Some(137)));
        assert_eq!(length(Some(false), 7.2), (Some(61_500), None));
        assert_eq!(length(None, 7.2), (Some(61_500), Some(137)));
        cat.db().execute("UPDATE analysis SET mtime = mtime + 1 WHERE rel = ?1", params![one]).unwrap();
        assert_eq!(cat.file_rows("jellyfin", 0, &[one.clone()]).unwrap()[0].bpm, None, "a file changed since its analysis");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn crates_see_a_folders_files_newest_first_and_map_server_ids_to_files() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let (one, two) = ("A - B (2020)/01 One.mp3".to_string(), "A - B (2020)/02 Two.flac".to_string());
        // Two seen later than One.
        let db = cat.db();
        db.execute("INSERT INTO tracks (born) VALUES (100), (200)", []).unwrap();
        db.execute("UPDATE files SET track = (SELECT id FROM tracks WHERE born = 100) WHERE rel = ?1", params![one]).unwrap();
        db.execute("UPDATE files SET track = (SELECT id FROM tracks WHERE born = 200) WHERE rel = ?1", params![two]).unwrap();
        drop(db);
        assert_eq!(cat.files_newest_first().unwrap(), vec![(0, two.clone(), Some(200)), (0, one.clone(), Some(100))]);
        cat.record_links("jellyfin", "lib", &[("t1".into(), 0, one.clone())], 100).unwrap();
        let found = cat.lines_of("jellyfin", &["t1".into(), "unknown".into()]).unwrap();
        assert_eq!(found, [one].into_iter().collect(), "unknown ids are left out");
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

    #[test]
    fn a_wind_s_tracks_are_found_at_once_as_one_by_one() {
        let a = music();
        let cat = Catalogue::open(None, vec![a.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        let two = cat.track_of(0, "A - B (2020)/02 Two.flac").unwrap();
        cat.record_links("jf", "music", &[("j-2".into(), 0, "A - B (2020)/02 Two.flac".into())], 1).unwrap();
        let (one, two) = (crate::serve::ids::song(one), crate::serve::ids::song(two));
        let each = |items: &[(&str, &str)]| -> Vec<(i64, String, Option<u64>)> {
            items
                .iter()
                .map_while(|(s, i)| {
                    let (folder, rel) = cat.file_of(cat.track_of_source(s, i)?)?;
                    let d = cat.duration_of(folder, &rel);
                    Some((folder, rel, d))
                })
                .collect()
        };
        let asked = [("cratarr", two.as_str()), ("jf", "j-2"), ("cratarr", one.as_str()), ("jf", "j-unknown"), ("cratarr", one.as_str())];
        let found = cat.files_of_sources(&asked);
        assert_eq!(found, each(&asked));
        assert_eq!(found.len(), 3, "up to the first it has no file of: {found:?}");
        assert_eq!(found[1], (0, "A - B (2020)/02 Two.flac".to_string(), Some(200_000)), "a server's id, linked");
        assert!(cat.files_of_sources(&[]).is_empty());
        std::fs::remove_dir_all(&a).ok();
    }

    #[test]
    fn a_folder_out_of_use_is_parked_and_comes_back_with_its_track_ids() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(1, "A - B (2020)/01 One.mp3").unwrap();
        let own = crate::serve::ids::song(one);
        assert_eq!(cat.track_of_source("cratarr", &own), Some(one), "Cratarr's own ids need no links");
        assert_eq!(cat.track_of_source("cratarr", "al-x"), None);
        cat.set_meta("full-folders:jf", "as at the last full listing");
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        assert!(cat.meta("full-folders:jf").is_some(), "the same folders: nothing to list again");
        // Out of use (its source switched off): parked, not counted, not forgotten.
        cat.set_folders(vec![a.clone()]).unwrap();
        assert_eq!(cat.status().unwrap().files, 2, "only the folder in use counts");
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), None);
        assert_eq!(cat.track_of_source("cratarr", &own), None, "a parked track does not play");
        // Back, even at another place in the list: the same track id, and the servers list again.
        cat.set_folders(vec![b.clone(), a.clone()]).unwrap();
        assert_eq!(cat.track_of(0, "A - B (2020)/01 One.mp3"), Some(one), "the same id after the break");
        assert_eq!(cat.meta("full-folders:jf"), None, "links checked again after a break");
        assert_eq!(cat.status().unwrap().files, 4);
        // Parked, then other files at that path: forgotten.
        cat.set_folders(vec![a.clone()]).unwrap();
        std::fs::remove_dir_all(&b).unwrap();
        std::fs::create_dir_all(b.join("X")).unwrap();
        std::fs::write(b.join("X/other.mp3"), "x").unwrap();
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), None, "those files are gone: their rows with them");
        assert_eq!(cat.status().unwrap().files, 3);
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }
}
