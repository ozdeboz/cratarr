//! The OpenSubsonic library (decision 0030): songs, albums and artists as the API shows them, derived from the
//! catalogue into their own SQLite file (deleting it costs a rebuild). Patterns from Navidrome
//! (https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors), translated:
//!
//! - an album's aggregates live on its row (songs, length, size, years, discs, genres, added = its oldest song);
//! - search through contentless FTS5 tables (`unicode61 remove_diacritics 2`), each word as `(word OR word*)`, ranked
//!   by bm25 with column weights (`persistence/sql_search_fts.go`);
//! - paging in two phases, the page's row ids first and their details after, over covering indexes
//!   (`persistence/sql_search.go`);
//! - WAL, `synchronous=NORMAL`, a busy timeout, and `ANALYZE` after a rebuild (`db/optimize.go`).
//!
//! Changed from Navidrome: the library is rebuilt whole when the catalogue's files change (24,000 songs take seconds),
//! where Navidrome refreshes the albums a scan touched; one library; no tag tables (the catalogue keeps the tags).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::ids;
use crate::catalogue::{Rules, ServedRow};

// journal_size_limit: as the catalogue's (the log's file cut back after a checkpoint).
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS songs (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, pid TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL, path TEXT NOT NULL,
  album_id TEXT NOT NULL, title TEXT NOT NULL, artist TEXT, album TEXT NOT NULL, album_artist TEXT,
  track_no INTEGER, disc_no INTEGER, year INTEGER, genre TEXT, bpm INTEGER, duration_ms INTEGER, size INTEGER NOT NULL,
  suffix TEXT, kbps INTEGER, created INTEGER, mbid TEXT, order_title TEXT NOT NULL, cover TEXT,
  rg_track_gain REAL, rg_track_peak REAL, rg_album_gain REAL, rg_album_peak REAL, genres TEXT NOT NULL DEFAULT '');
CREATE INDEX IF NOT EXISTS songs_by_album ON songs (album_id, disc_no, track_no, rel);
CREATE INDEX IF NOT EXISTS songs_by_title ON songs (order_title, n);
CREATE INDEX IF NOT EXISTS songs_by_pid ON songs (pid);
CREATE TABLE IF NOT EXISTS song_artists (song_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (song_id, position)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS albums (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, artist TEXT NOT NULL, artist_id TEXT NOT NULL,
  year INTEGER, original_year INTEGER, genre TEXT, genres TEXT NOT NULL, song_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL, size INTEGER NOT NULL, discs INTEGER NOT NULL, created INTEGER, mbid TEXT,
  folder INTEGER NOT NULL, dir TEXT NOT NULL, compilation INTEGER NOT NULL, order_name TEXT NOT NULL, base_id TEXT NOT NULL,
  format TEXT, min_kbps INTEGER, cover TEXT);
CREATE INDEX IF NOT EXISTS albums_by_base ON albums (base_id);
CREATE INDEX IF NOT EXISTS albums_by_name ON albums (order_name, n);
CREATE INDEX IF NOT EXISTS albums_by_created ON albums (created DESC, n);
CREATE INDEX IF NOT EXISTS albums_by_year ON albums (year, n);
CREATE INDEX IF NOT EXISTS albums_by_artist ON albums (artist_id, year, n);
CREATE TABLE IF NOT EXISTS album_artists (album_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (album_id, position)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS album_artists_by_artist ON album_artists (artist_id, album_id);
CREATE INDEX IF NOT EXISTS song_artists_by_artist ON song_artists (artist_id, song_id);
CREATE TABLE IF NOT EXISTS artists (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, mbid TEXT, album_count INTEGER NOT NULL,
  song_count INTEGER NOT NULL, order_name TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS artists_by_name ON artists (order_name, n);
CREATE TABLE IF NOT EXISTS genres (name TEXT PRIMARY KEY, song_count INTEGER NOT NULL, album_count INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS songs_fts USING fts5(title, artist, album, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS albums_fts USING fts5(name, artist, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE IF NOT EXISTS artists_fts USING fts5(name, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2');
";

/// The tables' layout; a library of another layout is built again.
const LAYOUT: i64 = 8;

/// The meta key of the catalogue state the library was built from.
const BUILT_FROM: &str = "built_from";

/// What changed since the library was built: nothing, only the analyses (their gains and tempi are the songs'), or the
/// songs themselves (files, tags, the rules, the added dates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    None,
    Analyses,
    Songs,
}

/// The change from what the library was built from (`was`) to now, and the print to note: the songs' part and the
/// analyses' part, apart. A print from before they were apart reads as the songs changed: built once more.
fn change_between(was: Option<&str>, songs: &str, analyses: &str) -> (Change, String) {
    let print = format!("{songs} | analysis {analyses}");
    let change = match was {
        Some(w) if w == print => Change::None,
        Some(w) if w.split_once(" | analysis ").is_some_and(|(s, _)| s == songs) => Change::Analyses,
        _ => Change::Songs,
    };
    (change, print)
}

/// One folder of the catalogue as the library reads it: its id there and its files.
pub struct FolderRows {
    pub folder: i64,
    /// The folder as Cratarr sees it; a song's `path` is this and its place in it.
    pub root: String,
    pub rows: Vec<ServedRow>,
}

pub struct Library {
    db: Mutex<Connection>,
    /// When albums were added, where known better than the catalogue's first sight of their files (imported from a
    /// media server): an album's `created` is the earlier of the two.
    added: Mutex<HashMap<String, i64>>,
}

/// A number at the start of a tag ("3/12" is 3).
/// A song's ReplayGain (decision 0035): from its analysis once it has one (every track levelled the same way), else
/// from the file's own tags (`REPLAYGAIN_*`; Opus's `R128_*`, which count from -23 LUFS in 1/256 dB, so 5 dB more for
/// ReplayGain's -18). The album's from its tracks' analyses once all have one, else the same way from the tags.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Gains {
    pub track_gain: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
}

/// "-6.53 dB", "+1.2 dB", "0.988": the number at the start.
fn decibels(v: &str) -> Option<f64> {
    let t = v.trim();
    let end = t.find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))).unwrap_or(t.len());
    t[..end].parse::<f64>().ok().filter(|x| x.is_finite())
}

fn r128(v: &str) -> Option<f64> {
    v.trim().parse::<i32>().ok().map(|q| f64::from(q) / 256.0 + 5.0)
}

pub(super) fn gains(r: &ServedRow) -> Gains {
    let tag_track = || r.first("replaygain_track_gain").and_then(decibels).or_else(|| r.first("r128_track_gain").and_then(r128));
    let tag_album = || r.first("replaygain_album_gain").and_then(decibels).or_else(|| r.first("r128_album_gain").and_then(r128));
    let peak = |name: &str| r.first(name).and_then(decibels).filter(|p| *p > 0.0);
    let (track_gain, track_peak) = match &r.analysis {
        Some(a) => (Some(f64::from(a.track_gain_db)), Some(f64::from(a.track_peak))),
        None => (tag_track(), peak("replaygain_track_peak")),
    };
    let (album_gain, album_peak) = match r.analysis.as_ref().and_then(|a| a.album) {
        Some((g, p)) => (Some(f64::from(g)), Some(f64::from(p))),
        None => (tag_album(), peak("replaygain_album_peak")),
    };
    Gains { track_gain, track_peak, album_gain, album_peak }
}

/// A song's tempo (decision 0035): the file's own BPM tag, else its analysis' when that is sure enough.
pub(super) fn bpm_of(r: &ServedRow) -> Option<u32> {
    r.first("bpm")
        .and_then(leading_number)
        .or_else(|| r.analysis.as_ref().and_then(|a| a.bpm).map(|b| b.round()).filter(|b| *b > 0.0).map(|b| b as u32))
}

fn leading_number(v: &str) -> Option<u32> {
    let digits: String = v.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

/// The year in a date tag ("1997-05-12", "1997").
/// A song as a file of a folder knows it ([`Library::songs_by_rel`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RelSong {
    pub song: String,
    pub album: String,
    pub duration_ms: Option<i64>,
    /// When it came (epoch seconds).
    pub created: Option<i64>,
    /// Its genres, as its tags have them: what a playlist's top genres count.
    pub genres: Vec<String>,
}

pub fn year_of(v: &str) -> Option<u32> {
    let y: String = v.trim().chars().take(4).collect();
    (y.len() == 4).then(|| y.parse().ok()).flatten().filter(|y| *y > 0)
}

/// How names sort: lower case, typographic quotes plain, a leading article ("The", "A", "An") moved off, as
/// Navidrome's `OrderName` and nghe's index do.
pub fn order_name(name: &str) -> String {
    let lower = ids::clear(&name.trim().to_lowercase());
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.trim_start().to_string();
        }
    }
    lower
}

/// What a song shows and is grouped by.
struct Song<'a> {
    row: &'a ServedRow,
    folder: i64,
    title: String,
    artists: Vec<String>,
    artist_credit: Option<String>,
    album: String,
    album_artist: String,
    album_artists: Vec<String>,
    album_id: String,
    /// The album's id as it is (or would be) in Music, folder 0: what its tags are kept under, so they follow an
    /// album kept from Incoming.
    album_base: String,
    genres: Vec<String>,
}

fn song<'a>(row: &'a ServedRow, folder: i64, rules: &Rules) -> Song<'a> {
    let file_name = row.rel.rsplit('/').next().unwrap_or(&row.rel);
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    let title = row.first("title").unwrap_or(stem).to_string();
    let list = |list: &str, text: &str| {
        let from = if row.all(list).next().is_some() { list } else { text };
        rules.values(from, &row.all(from).map(str::to_string).collect::<Vec<_>>())
    };
    let artists = list("artists", "artist");
    let artist_credit = row.first("artist").map(str::to_string).or_else(|| (!artists.is_empty()).then(|| artists.join(", ")));
    // The album artist as the file credits it, else the song's artist.
    let album_artist = row.first("albumartist").map(str::to_string).or_else(|| artist_credit.clone()).unwrap_or_else(|| "[Unknown Artist]".into());
    let mut album_artists = list("albumartists", "albumartist");
    if album_artists.is_empty() {
        album_artists = if artists.is_empty() { vec![album_artist.clone()] } else { artists.clone() };
    }
    let album = row.first("album").map(str::to_string).unwrap_or_else(|| "[Unknown Album]".into());
    let album_id = ids::album(row, &album_artist, folder);
    let album_base = ids::album(row, &album_artist, 0);
    let genres = rules.values("genre", &row.all("genre").map(str::to_string).collect::<Vec<_>>());
    Song { row, folder, title, artists, artist_credit, album, album_artist, album_artists, album_id, album_base, genres }
}

/// What these files say, cheaply: when it differs from what the library was built from, it is built again (tests;
/// the server checks the catalogue itself, `change` and `build_from`).
#[cfg(test)]
pub fn fingerprint(folders: &[FolderRows]) -> String {
    let mut h = md5::Md5::default();
    use md5::Digest;
    for f in folders {
        h.update(f.folder.to_le_bytes());
        for r in &f.rows {
            h.update(r.rel.as_bytes());
            h.update(r.track.unwrap_or(0).to_le_bytes());
            h.update(r.mtime.to_le_bytes());
            h.update(r.size.to_le_bytes());
        }
    }
    format!("{:x}", h.finalize())
}

impl Library {
    /// `file`: the database; `None` keeps it in memory.
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = crate::db::open(file)?;
        conn.busy_timeout(Duration::from_secs(15))?;
        // Derived data: a library from an older layout is simply built again.
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != LAYOUT {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%_fts_%'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for t in tables {
                conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{t}\""))?;
            }
        }
        conn.execute_batch(SCHEMA).context("creating the library tables")?;
        conn.execute_batch(&format!("PRAGMA user_version = {LAYOUT}"))?;
        Ok(Self { db: Mutex::new(conn), added: Mutex::default() })
    }

    /// The added dates known better than the catalogue's (by album id); a change builds the library again.
    pub fn set_added(&self, added: HashMap<String, i64>) {
        *self.added.lock().unwrap_or_else(|p| p.into_inner()) = added;
    }

    /// A summary of the added dates, part of what the library was built from.
    fn added_print(&self) -> String {
        let added = self.added.lock().unwrap_or_else(|p| p.into_inner());
        format!("{}/{}", added.len(), added.values().fold(0i64, |a, v| a.wrapping_add(*v)))
    }

    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Builds the library again from these files, unless it was built from them already. Returns whether it was
    /// built. Blocking.
    #[cfg(test)]
    pub fn refresh(&self, folders: &[FolderRows], rules: &Rules) -> Result<bool> {
        let print = fingerprint(folders);
        if self.built_from().as_deref() == Some(print.as_str()) {
            return Ok(false);
        }
        self.build(folders, rules)?;
        self.set_built_from(&print)
    }

    /// Builds the library again from the catalogue when its files changed (a cheap check first), reading through a
    /// connection of its own so the catalogue is not held up meanwhile (a full read takes seconds). Returns whether it
    /// was built. Blocking. The server does it in its two steps (`change`, then `build_from`), to tell new analyses from
    /// changed songs; this is the timing test's.
    #[cfg(test)]
    pub fn refresh_from(&self, cat: &crate::catalogue::Catalogue, rules: &Rules) -> Result<bool> {
        let (change, print) = self.change(cat, rules)?;
        if change == Change::None {
            return Ok(false);
        }
        self.build_from(cat, rules, &print)?;
        Ok(true)
    }

    /// What changed since the library was built (a cheap check: the catalogue's counts and sums), and the print to
    /// build it with. Blocking.
    pub fn change(&self, cat: &crate::catalogue::Catalogue, rules: &Rules) -> Result<(Change, String)> {
        let served = match cat.reader() {
            Some(c) => crate::catalogue::served_fingerprint(&c)?,
            None => cat.served_fingerprint()?,
        };
        // The analyses' part is the fingerprint's tail (`… analysis <count>/<sum of when>`).
        let (files, analyses) = served.split_once(" analysis ").unwrap_or((served.as_str(), ""));
        let songs = format!(
            "catalogue {} {} {} rules {:x} files {}",
            cat.generation(),
            cat.folders().len(),
            self.added_print(),
            {
                use md5::Digest;
                md5::Md5::digest(serde_json::to_string(rules)?.as_bytes())
            },
            files
        );
        Ok(change_between(self.built_from().as_deref(), &songs, analyses))
    }

    /// Builds the library from the catalogue, `print` (from `change`) noted as what it was built from. Blocking.
    pub fn build_from(&self, cat: &crate::catalogue::Catalogue, rules: &Rules, print: &str) -> Result<()> {
        let reader = cat.reader();
        let count = cat.folders().len() as i64;
        let folders = (0..count)
            .map(|folder| {
                let rows = match &reader {
                    Some(c) => crate::catalogue::served_rows(c, folder)?,
                    None => cat.served_rows(folder)?,
                };
                let root = cat.folders().get(folder as usize).map(|p| p.display().to_string()).unwrap_or_default();
                Ok(FolderRows { folder, root, rows })
            })
            .collect::<Result<Vec<_>>>()?;
        self.build(&folders, rules)?;
        self.set_built_from(print)?;
        Ok(())
    }

    fn built_from(&self) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [BUILT_FROM], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_built_from(&self, print: &str) -> Result<bool> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![BUILT_FROM, print])?;
        Ok(true)
    }

    fn build(&self, folders: &[FolderRows], rules: &Rules) -> Result<()> {
        let added = self.added.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut songs: Vec<Song> =
            folders.iter().flat_map(|f| f.rows.iter().filter(|r| r.track.is_some()).map(move |r| (f.folder, r))).map(|(folder, r)| song(r, folder, rules)).collect();
        one_album_per_folder(&mut songs);
        let roots: HashMap<i64, &str> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute_batch(
            "DELETE FROM songs; DELETE FROM song_artists; DELETE FROM albums; DELETE FROM album_artists; DELETE FROM artists;
             DELETE FROM genres; DELETE FROM songs_fts; DELETE FROM albums_fts; DELETE FROM artists_fts;",
        )?;
        // Artists: every name credited on an album or a song, the album artists counted.
        let mut artist_names: BTreeMap<String, String> = BTreeMap::new();
        let mut artist_mbid: HashMap<String, String> = HashMap::new();
        let mut artist_albums: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut artist_songs: HashMap<String, u32> = HashMap::new();
        let mut albums: BTreeMap<String, Vec<&Song>> = BTreeMap::new();
        {
            let mut ins_song = tx.prepare(
                "INSERT INTO songs (id, pid, folder, rel, album_id, title, artist, album, album_artist, track_no, disc_no, year,
                   genre, bpm, duration_ms, size, suffix, kbps, created, mbid, order_title, path,
                   rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak, genres)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
                   ?23, ?24, ?25, ?26, ?27)",
            )?;
            let mut ins_fts = tx.prepare("INSERT INTO songs_fts (rowid, title, artist, album) VALUES (?1, ?2, ?3, ?4)")?;
            let mut ins_artist = tx.prepare("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
            for s in &songs {
                let r = s.row;
                let id = ids::song(r.track.unwrap_or_default());
                let suffix = r.rel.rsplit_once('.').map(|(_, e)| e.to_lowercase());
                let g = gains(r);
                ins_song.execute(params![
                    id,
                    ids::song_pid(r, &s.album_artist),
                    s.folder,
                    r.rel,
                    s.album_id,
                    s.title,
                    s.artist_credit,
                    s.album,
                    s.album_artist,
                    r.first("tracknumber").and_then(leading_number),
                    r.first("discnumber").and_then(leading_number),
                    r.first("date").and_then(year_of),
                    s.genres.first(),
                    bpm_of(r),
                    r.duration_ms.map(|d| d as i64),
                    r.size as i64,
                    suffix,
                    r.kbps,
                    r.born,
                    r.first("musicbrainz_trackid"),
                    order_name(&s.title),
                    format!("{}/{}", roots.get(&s.folder).map_or("", |r| r.trim_end_matches('/')), r.rel),
                    g.track_gain,
                    g.track_peak,
                    g.album_gain,
                    g.album_peak,
                    s.genres.join("\u{1f}"),
                ])?;
                let n = tx.last_insert_rowid();
                ins_fts.execute(params![n, s.title, s.artist_credit.as_deref().unwrap_or(""), s.album])?;
                for (i, name) in s.artists.iter().enumerate() {
                    let aid = ids::artist(name);
                    ins_artist.execute(params![id, aid, name, i as i64])?;
                    artist_names.entry(aid.clone()).or_insert_with(|| name.clone());
                    *artist_songs.entry(aid).or_default() += 1;
                }
                albums.entry(s.album_id.clone()).or_default().push(s);
            }
        }
        {
            let mut ins_album = tx.prepare(
                "INSERT INTO albums (id, name, artist, artist_id, year, original_year, genre, genres, song_count, duration_ms,
                   size, discs, created, mbid, folder, dir, compilation, order_name, base_id, format, min_kbps, cover)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            )?;
            let mut ins_fts = tx.prepare("INSERT INTO albums_fts (rowid, name, artist) VALUES (?1, ?2, ?3)")?;
            let mut ins_aa = tx.prepare("INSERT INTO album_artists (album_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
            let mut genre_songs: BTreeMap<String, (String, u32, BTreeSet<String>)> = BTreeMap::new();
            for (album_id, songs) in &albums {
                let first = songs[0];
                let years: Vec<u32> = songs.iter().filter_map(|s| s.row.first("date").and_then(year_of)).collect();
                let original: Vec<u32> = songs.iter().filter_map(|s| s.row.first("originaldate").and_then(year_of)).collect();
                let mut genres: Vec<String> = Vec::new();
                for s in songs.iter() {
                    for g in &s.genres {
                        if !genres.iter().any(|x| x.eq_ignore_ascii_case(g)) {
                            genres.push(g.clone());
                        }
                        let e = genre_songs.entry(g.to_lowercase()).or_insert_with(|| (g.clone(), 0, BTreeSet::new()));
                        e.1 += 1;
                        e.2.insert(album_id.clone());
                    }
                }
                // The album's folder: the one its files share.
                let dirs: Vec<&str> = songs.iter().map(|s| s.row.rel.rsplit_once('/').map_or("", |(d, _)| d)).collect();
                let dir = common_dir(&dirs);
                let discs = songs.iter().filter_map(|s| s.row.first("discnumber").and_then(leading_number)).max().unwrap_or(1);
                let compilation = songs.iter().any(|s| s.row.first("compilation").is_some_and(|c| c == "1"));
                let artist_id = ids::artist(&first.album_artists[0]);
                // One format for the whole album, else none (mixed).
                // The art's id: the album's, with a fingerprint of its picture (see `art`).
                let root = roots.get(&first.folder).copied().unwrap_or("");
                let first_track = songs.iter().map(|s| &s.row.rel).min().map(|r| Path::new(root).join(r));
                let cover = super::art::album_print(&Path::new(root).join(&dir), first_track.as_deref()).map(|p| format!("{album_id}_{p}"));
                let formats: BTreeSet<&str> = songs.iter().filter_map(|s| s.row.format.as_deref()).collect();
                let format = (formats.len() == 1).then(|| formats.iter().next().map(|f| f.to_string())).flatten();
                ins_album.execute(params![
                    album_id,
                    first.album,
                    first.album_artist,
                    artist_id,
                    years.iter().max(),
                    original.iter().min(),
                    genres.first(),
                    genres.join("\u{1f}"),
                    songs.len() as i64,
                    songs.iter().filter_map(|s| s.row.duration_ms).sum::<u64>() as i64,
                    songs.iter().map(|s| s.row.size).sum::<u64>() as i64,
                    discs,
                    songs.iter().filter_map(|s| s.row.born).chain(added.get(album_id).copied()).min(),
                    first.row.first("musicbrainz_albumid"),
                    first.folder,
                    dir,
                    compilation,
                    order_name(&first.album),
                    first.album_base,
                    format,
                    songs.iter().filter_map(|s| s.row.kbps).min(),
                    cover,
                ])?;
                ins_fts.execute(params![tx.last_insert_rowid(), first.album, first.album_artist])?;
                for (i, name) in first.album_artists.iter().enumerate() {
                    let aid = ids::artist(name);
                    ins_aa.execute(params![album_id, aid, name, i as i64])?;
                    artist_names.entry(aid.clone()).or_insert_with(|| name.clone());
                    artist_albums.entry(aid.clone()).or_default().insert(album_id.clone());
                    if let (0, Some(mb)) = (i, first.row.first("musicbrainz_albumartistid")) {
                        artist_mbid.entry(aid).or_insert_with(|| mb.to_string());
                    }
                }
            }
            let mut ins_genre = tx.prepare("INSERT OR IGNORE INTO genres (name, song_count, album_count) VALUES (?1, ?2, ?3)")?;
            for (_, (name, songs, albums)) in genre_songs {
                ins_genre.execute(params![name, songs, albums.len() as i64])?;
            }
        }
        {
            let mut ins_artist =
                tx.prepare("INSERT INTO artists (id, name, mbid, album_count, song_count, order_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            let mut ins_fts = tx.prepare("INSERT INTO artists_fts (rowid, name) VALUES (?1, ?2)")?;
            for (aid, name) in &artist_names {
                ins_artist.execute(params![
                    aid,
                    name,
                    artist_mbid.get(aid),
                    artist_albums.get(aid).map_or(0, |a| a.len() as i64),
                    artist_songs.get(aid).copied().unwrap_or(0),
                    order_name(name),
                ])?;
                ins_fts.execute(params![tx.last_insert_rowid(), name])?;
            }
        }
        tx.execute_batch("UPDATE songs SET cover = (SELECT a.cover FROM albums a WHERE a.id = songs.album_id);")?;
        // Where each catalogue folder is, so the library can be read before the catalogue is open (decision 0032).
        let roots: Vec<(i64, &str)> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('roots', ?1)", [serde_json::to_string(&roots)?])?;
        tx.commit()?;
        db.execute_batch("ANALYZE;")?;
        Ok(())
    }

    /// Where each catalogue folder was at the last build: (folder, path).
    /// The songs of one folder by their file: their id, album, length and when they came (the crates' cards). When a
    /// song came is its album's when that is earlier: an album's date can be imported from a media server.
    pub fn songs_by_rel(&self, folder: i64) -> Result<HashMap<String, RelSong>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT s.rel, s.id, s.album_id, s.duration_ms, MIN(COALESCE(s.created, a.created), COALESCE(a.created, s.created)), s.genres
             FROM songs s LEFT JOIN albums a ON a.id = s.album_id WHERE s.folder = ?1",
        )?;
        let rows = st.query_map(rusqlite::params![folder], |r| {
            let genres: Option<String> = r.get(5)?;
            Ok((
                r.get::<_, String>(0)?,
                RelSong {
                    song: r.get(1)?,
                    album: r.get(2)?,
                    duration_ms: r.get(3)?,
                    created: r.get(4)?,
                    genres: genres.map(|g| g.split('\u{1f}').filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn roots(&self) -> Vec<(i64, String)> {
        let v: Option<String> = self.db().query_row("SELECT value FROM meta WHERE key = 'roots'", [], |r| r.get(0)).optional().ok().flatten();
        v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
    }

    /// Counts, for the log and tests: (songs, albums, artists).
    /// Every album's id, catalogue folder and when it came (`created`), and every song's id, album, catalogue folder and
    /// place in it: Insights' overview and its period.
    #[allow(clippy::type_complexity)]
    pub fn album_arrivals(&self) -> Result<(Vec<(String, i64, Option<i64>)>, Vec<(String, String, i64, String)>)> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, folder, created FROM albums")?;
        let albums = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare("SELECT id, album_id, folder, rel FROM songs")?;
        let songs = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok((albums, songs))
    }

    /// Every artist with an album, the most albums first, with their MusicBrainz id where known: the artist store's
    /// builder works through them in this order.
    pub fn artist_names(&self) -> Result<Vec<(String, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT name, mbid FROM artists WHERE album_count > 0 ORDER BY album_count DESC, order_name")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// An artist by name, as the library writes it (any case): their id and MusicBrainz id.
    pub fn artist_by_name(&self, name: &str) -> Result<Option<(String, Option<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id, mbid FROM artists WHERE name = ?1 COLLATE NOCASE ORDER BY album_count DESC LIMIT 1")?;
        Ok(st.query_row([name], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    /// Every album artist's albums: (artist id, name, MusicBrainz id, album id, its catalogue folder, its tracks).
    /// Home's Artists to explore.
    #[allow(clippy::type_complexity)]
    pub fn artist_albums(&self) -> Result<Vec<(String, String, Option<String>, String, i64, i64)>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT ar.id, ar.name, ar.mbid, al.id, al.folder, al.song_count FROM album_artists aa
             JOIN albums al ON al.id = aa.album_id JOIN artists ar ON ar.id = aa.artist_id",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Each of these songs' catalogue folder (those the library has).
    pub fn song_folders(&self, ids: &[String]) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder FROM songs WHERE id = ?1")?;
        let mut out = std::collections::HashMap::new();
        for id in ids {
            if let Ok(f) = st.query_row([id], |r| r.get::<_, i64>(0)) {
                out.insert(id.clone(), f);
            }
        }
        Ok(out)
    }

    pub fn counts(&self) -> Result<(u64, u64, u64)> {
        let db = self.db();
        let n = |t: &str| -> Result<u64> { Ok(db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0))? as u64) };
        Ok((n("songs")?, n("albums")?, n("artists")?))
    }
}

/// One album per folder (Cratarr's music layout, decision 0016; gonic's model): songs of one folder whose tags disagree
/// (a date or an album artist on some tracks only) would otherwise split it into several albums. The folder's album is
/// the one most of its songs name, the first in path order on a tie, so its id still comes from the tags and survives
/// a move.
fn one_album_per_folder(songs: &mut [Song]) {
    let dir = |s: &Song| (s.folder, s.row.rel.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string()));
    let mut votes: HashMap<(i64, String), Vec<(usize, &str)>> = HashMap::new();
    let mut order: Vec<usize> = (0..songs.len()).collect();
    order.sort_by(|a, b| songs[*a].row.rel.cmp(&songs[*b].row.rel));
    for &i in &order {
        let s = &songs[i];
        let list = votes.entry(dir(s)).or_default();
        match list.iter_mut().find(|(_, id)| *id == s.album_id) {
            Some(v) => v.0 += 1,
            None => list.push((1, s.album_id.as_str())),
        }
    }
    let chosen: HashMap<(i64, String), (String, usize)> = votes
        .into_iter()
        .map(|(d, list)| {
            let best = list.iter().max_by(|a, b| a.0.cmp(&b.0).then(std::cmp::Ordering::Greater)).map_or("", |v| v.1).to_string();
            (d, (best, list.len()))
        })
        .collect();
    // The album's own name and artist come from a song that named it.
    let mut lead: HashMap<String, (String, String, Vec<String>, String)> = HashMap::new();
    for s in songs.iter() {
        if let Some((id, _)) = chosen.get(&dir(s))
            && *id == s.album_id
        {
            lead.entry(id.clone()).or_insert_with(|| (s.album.clone(), s.album_artist.clone(), s.album_artists.clone(), s.album_base.clone()));
        }
    }
    for s in songs.iter_mut() {
        let Some((id, kinds)) = chosen.get(&dir(s)) else { continue };
        if *kinds > 1
            && let Some((album, artist, artists, base)) = lead.get(id)
        {
            s.album_id = id.clone();
            s.album_base = base.clone();
            s.album = album.clone();
            s.album_artist = artist.clone();
            s.album_artists = artists.clone();
        }
    }
}

/// The folder several paths share ("" when none).
fn common_dir(dirs: &[&str]) -> String {
    let mut common: Option<Vec<&str>> = None;
    for d in dirs {
        let parts: Vec<&str> = d.split('/').filter(|p| !p.is_empty()).collect();
        common = Some(match common {
            None => parts,
            Some(c) => c.iter().zip(&parts).take_while(|(a, b)| a == b).map(|(a, _)| *a).collect(),
        });
    }
    common.unwrap_or_default().join("/")
}

/// An FTS5 query from what a user typed: each word as `(word OR word*)`, as Navidrome's; quotes and FTS operators
/// cannot leak in. `None` when nothing is left to search for.
pub fn fts_query(typed: &str) -> Option<String> {
    let words: Vec<String> = typed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("(\"{w}\" OR \"{w}\"*)"))
        .collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(track: i64, rel: &str, tags: &[(&str, &str)]) -> ServedRow {
        ServedRow {
            rel: rel.into(),
            track: Some(track),
            born: Some(1_700_000_000 + track),
            format: Some("FLAC".into()),
            kbps: Some(900),
            duration_ms: Some(200_000),
            size: 20_000_000,
            mtime: 1,
            tags: tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
            analysis: None,
        }
    }

    fn sample() -> Vec<FolderRows> {
        let homogenic = |t: i64, n: &str, title: &str| {
            row(t, &format!("Björk/Homogenic (1997)/{n} {title}.flac"), &[
                ("title", title),
                ("artist", "Björk"),
                ("albumartist", "Björk"),
                ("album", "Homogenic"),
                ("tracknumber", n),
                ("date", "1997-09-22"),
                ("genre", "Electronic; Art Pop"),
            ])
        };
        vec![FolderRows {
            folder: 0,
            root: "/music".into(),
            rows: vec![
                homogenic(1, "01", "Hunter"),
                homogenic(2, "02", "Jóga"),
                row(3, "The Orb/Adventures (1991)/01 Little Fluffy Clouds.mp3", &[
                    ("title", "Little Fluffy Clouds"),
                    ("artist", "The Orb"),
                    ("album", "Adventures Beyond the Ultraworld"),
                    ("tracknumber", "1/10"),
                    ("discnumber", "2/2"),
                ]),
            ],
        }]
    }

    #[test]
    fn songs_albums_and_artists_come_from_the_tags() {
        let lib = Library::open(None).unwrap();
        assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
        assert_eq!(lib.counts().unwrap(), (3, 2, 2));
        let db = lib.db();
        let (count, genres, dir, year, created): (i64, String, String, i64, i64) = db
            .query_row("SELECT song_count, genres, dir, year, created FROM albums WHERE name = 'Homogenic'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        assert_eq!((count, genres.as_str(), dir.as_str(), year, created), (2, "Electronic\u{1f}Art Pop", "Björk/Homogenic (1997)", 1997, 1_700_000_001));
        let (track, disc): (i64, i64) =
            db.query_row("SELECT track_no, disc_no FROM songs WHERE title = 'Little Fluffy Clouds'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((track, disc), (1, 2));
        let order: String = db.query_row("SELECT order_name FROM artists WHERE name = 'The Orb'", [], |r| r.get(0)).unwrap();
        assert_eq!(order, "orb");
    }

    #[test]
    fn a_songs_album_is_found_by_the_songs_id() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let all: HashMap<String, String> = {
            let db = lib.db();
            let mut st = db.prepare("SELECT id, album_id FROM songs").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(all.values().collect::<std::collections::HashSet<_>>().len(), 2);
        let asked: Vec<String> = all.keys().cloned().chain(["tr-none".to_string()]).collect();
        assert_eq!(lib.albums_of(&asked).unwrap(), all, "each song with its album; one not in the library left out");
    }

    #[test]
    fn gains_come_from_the_analysis_once_there_else_from_the_files_tags() {
        let tagged = row(1, "A/X/01 t.flac", &[("replaygain_track_gain", "-6.53 dB"), ("replaygain_track_peak", "0.988123"), ("replaygain_album_gain", "-7 dB")]);
        assert_eq!(gains(&tagged), Gains { track_gain: Some(-6.53), track_peak: Some(0.988123), album_gain: Some(-7.0), album_peak: None });
        // Opus counts from -23 LUFS in 1/256 dB: 5 dB more for ReplayGain's -18.
        let opus = row(2, "A/X/02 t.opus", &[("r128_track_gain", "-512")]);
        assert_eq!(gains(&opus).track_gain, Some(3.0));
        let mut analysed = tagged.clone();
        analysed.analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -4.0, track_peak: 0.5, bpm: Some(127.6), album: None });
        let g = gains(&analysed);
        assert_eq!((g.track_gain, g.track_peak, g.album_gain), (Some(-4.0), Some(0.5), Some(-7.0)), "the album's waits for all its tracks");
        analysed.analysis.as_mut().unwrap().album = Some((-5.0, 0.9));
        assert_eq!((gains(&analysed).album_gain, gains(&analysed).album_peak), (Some(-5.0), Some(0.8999999761581421)));
        // Tempo: the file's own tag first, then the analysis'.
        assert_eq!(bpm_of(&analysed), Some(128));
        let mut tagged_bpm = analysed.clone();
        tagged_bpm.tags.push(("bpm".into(), "90".into()));
        assert_eq!(bpm_of(&tagged_bpm), Some(90));
    }

    #[test]
    fn a_song_carries_its_replay_gain() {
        let lib = Library::open(None).unwrap();
        let mut r = row(1, "A/X (2001)/01 t.flac", &[("title", "t"), ("artist", "A"), ("album", "X"), ("replaygain_track_gain", "-6.5 dB")]);
        r.analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -4.25, track_peak: 0.75, bpm: Some(120.0), album: Some((-5.0, 0.9)) });
        lib.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: vec![r] }], &Rules::default()).unwrap();
        let id = crate::serve::ids::song(1);
        let song = lib.songs_by_id(&[id]).unwrap().pop().unwrap();
        let rg = song.replay_gain.unwrap();
        assert_eq!((rg.track_gain, rg.track_peak, rg.album_gain), (Some(-4.25), Some(0.75), Some(-5.0)));
        assert_eq!(song.bpm, Some(120));
        let json = serde_json::to_value(lib.songs_by_id(&[crate::serve::ids::song(1)]).unwrap().pop().unwrap()).unwrap();
        assert_eq!(json["replayGain"]["trackGain"], -4.25);
    }

    #[test]
    fn an_artists_work_is_their_albums_newest_first_then_what_they_appear_on() {
        let lib = Library::open(None).unwrap();
        let t = |n: i64, rel: &str, artist: &str, album_artist: &str, album: &str, date: &str| {
            let title = format!("t{n}");
            row(n, rel, &[("title", &title), ("artist", artist), ("albumartist", album_artist), ("album", album), ("tracknumber", &n.to_string()), ("date", date)])
        };
        let folders = vec![FolderRows {
            folder: 0,
            root: "/music".into(),
            rows: vec![
                t(1, "A/Old (2001)/01 t.flac", "A", "A", "Old", "2001"),
                t(2, "A/New (2010)/01 t.flac", "A", "A", "New", "2010"),
                t(3, "A/New (2010)/02 t.flac", "C", "A", "New", "2010"),
                t(4, "B/Mix (2005)/01 t.flac", "B", "B", "Mix", "2005"),
                t(5, "B/Mix (2005)/02 t.flac", "A", "B", "Mix", "2005"),
            ],
        }];
        lib.refresh(&folders, &Rules::default()).unwrap();
        let (own, on, songs) = lib.artist_works(&super::super::ids::artist("a")).unwrap();
        let names = |l: &[crate::serve::api::Album]| l.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&own), ["New", "Old"]);
        assert_eq!(names(&on), ["Mix"], "credited on a track only");
        let titles: Vec<&str> = songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["t2", "t3", "t5", "t1"], "every track of their albums, and theirs on others', album by album");
    }

    #[test]
    fn a_folder_is_one_album_even_when_its_tags_disagree() {
        let lib = Library::open(None).unwrap();
        let t = |n: i64, date: &str| row(n, &format!("A/X (2013)/0{n} t.flac"), &[("title", "t"), ("artist", "A"), ("album", "X"), ("date", date)]);
        let folders = vec![FolderRows { folder: 0, root: "/music".into(), rows: vec![t(1, "2013"), t(2, "2013"), t(3, "2015")] }];
        lib.refresh(&folders, &Rules::default()).unwrap();
        assert_eq!(lib.counts().unwrap().1, 1);
        let year: i64 = lib.db().query_row("SELECT year FROM albums", [], |r| r.get(0)).unwrap();
        assert_eq!(year, 2015, "the album's aggregates still count every song");
    }

    #[test]
    fn an_imported_added_date_comes_first_when_earlier() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let id: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        lib.set_added([(id.clone(), 5)].into());
        lib.build(&sample(), &Rules::default()).unwrap();
        let created: i64 = lib.db().query_row("SELECT created FROM albums WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(created, 5);
        lib.set_added([(id.clone(), 9_999_999_999)].into());
        lib.build(&sample(), &Rules::default()).unwrap();
        let created: i64 = lib.db().query_row("SELECT created FROM albums WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(created, 1_700_000_001, "a later one does not move it");
    }

    #[test]
    fn a_song_has_all_its_genres_and_is_found_by_each() {
        let lib = Library::open(None).unwrap();
        assert!(lib.refresh(&sample(), &Rules::default()).unwrap());
        let found = lib.songs_by_genre("Art Pop", 10, 0, None).unwrap();
        assert_eq!(found.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Hunter", "Jóga"], "found by its second genre too");
        assert_eq!(found[0].genres.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Electronic", "Art Pop"]);
        assert_eq!(found[0].genre.as_deref(), Some("Electronic"), "the one-genre field keeps the first");
        assert_eq!(lib.songs_by_genre("art pop", 10, 0, None).unwrap().len(), 2, "regardless of capitals");
        assert_eq!(lib.random_songs(10, Some("Art Pop"), (None, None), None).unwrap().len(), 2);
        assert!(lib.songs_by_genre("Art", 10, 0, None).unwrap().is_empty(), "a whole genre, not a part of one");
        assert_eq!(lib.songs_by_rel(0).unwrap()["Björk/Homogenic (1997)/01 Hunter.flac"].genres, ["Electronic", "Art Pop"]);
    }

    #[test]
    fn new_analyses_alone_are_told_apart_from_changed_songs() {
        let (first, print) = change_between(None, "catalogue 1 2 a rules f files 10/5", "3/99");
        assert_eq!(first, Change::Songs, "never built");
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 10/5", "3/99").0, Change::None);
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 10/5", "4/120").0, Change::Analyses);
        assert_eq!(change_between(Some(&print), "catalogue 1 2 a rules f files 11/6", "4/120").0, Change::Songs);
        assert_eq!(change_between(Some("catalogue 1 2 a rules f added 10/5 analysis 3/99"), "catalogue 1 2 a rules f files 10/5", "3/99").0, Change::Songs, "a print from before");
    }

    #[test]
    fn built_again_only_when_the_files_change() {
        let lib = Library::open(None).unwrap();
        let mut folders = sample();
        assert!(lib.refresh(&folders, &Rules::default()).unwrap());
        assert!(!lib.refresh(&folders, &Rules::default()).unwrap());
        folders[0].rows[0].mtime = 2;
        assert!(lib.refresh(&folders, &Rules::default()).unwrap());
    }

    #[test]
    fn search_finds_words_and_their_beginnings_without_accents() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let db = lib.db();
        let found = |table: &str, q: &str| -> i64 {
            let q = fts_query(q).unwrap();
            db.query_row(&format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?1"), [q], |r| r.get(0)).unwrap()
        };
        assert_eq!(found("songs_fts", "joga"), 1, "diacritics are ignored");
        assert_eq!(found("songs_fts", "fluff"), 1, "a word's beginning finds it");
        assert_eq!(found("albums_fts", "bjork homo"), 1);
        assert_eq!(found("artists_fts", "orb"), 1);
        assert_eq!(fts_query("\" *"), None, "nothing left to search for");
        assert_eq!(fts_query("a OR b").as_deref(), Some("(\"a\" OR \"a\"*) AND (\"OR\" OR \"OR\"*) AND (\"b\" OR \"b\"*)"), "quoted: a word, not an operator");
    }

    /// A real catalogue, built into a library: `CRATARR_REAL_CATALOGUE=<copy of catalogue.sqlite> cargo test
    /// real_catalogue -- --ignored --nocapture`. Prints the time taken and what came out.
    #[test]
    #[ignore]
    fn real_catalogue() {
        let Ok(file) = std::env::var("CRATARR_REAL_CATALOGUE") else { return };
        let cat = crate::catalogue::Catalogue::open(Some(Path::new(&file)), vec!["/media/Music".into(), "/media/lidarr-music".into()]).unwrap();
        let lib = Library::open(None).unwrap();
        let t = std::time::Instant::now();
        assert!(lib.refresh_from(&cat, &Rules::default()).unwrap());
        let first = t.elapsed();
        assert!(!lib.refresh_from(&cat, &Rules::default()).unwrap(), "nothing changed");
        println!("read and built {:?}, checked again {:?}, (songs, albums, artists) {:?}", first, t.elapsed() - first, lib.counts().unwrap());
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        println!("memory: {}", status.lines().filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS")).collect::<Vec<_>>().join(", "));
        let db = lib.db();
        let per_folder: Vec<(i64, i64)> =
            db.prepare("SELECT folder, count(*) FROM albums GROUP BY folder").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        println!("albums per folder {per_folder:?}");
        let dirs: i64 = db.query_row("SELECT count(DISTINCT folder || '/' || dir) FROM albums", [], |r| r.get(0)).unwrap();
        println!("distinct album folders {dirs}");
        // Albums whose songs lie in more than one folder, and folders split into several albums: grouping by tags vs folders.
        let split: Vec<(String, i64)> = db
            .prepare("SELECT folder || '/' || substr(rel, 1, length(rel) - length(replace(rel, rtrim(rel, replace(rel, '/', '')), ''))) d, count(DISTINCT album_id) n FROM songs GROUP BY d HAVING n > 1 ORDER BY n DESC LIMIT 8")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        println!("folders split over several albums (first 8): {split:#?}");
    }

    #[test]
    fn articles_do_not_decide_the_order() {
        assert_eq!(order_name("The Beatles"), "beatles");
        assert_eq!(order_name("A Tribe Called Quest"), "tribe called quest");
        assert_eq!(order_name("Theatre"), "theatre");
        assert_eq!(year_of("1997-09-22"), Some(1997));
        assert_eq!(leading_number("03/12"), Some(3));
    }
}
