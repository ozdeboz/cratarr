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
  rg_track_gain REAL, rg_track_peak REAL, rg_album_gain REAL, rg_album_peak REAL, genres TEXT NOT NULL DEFAULT '',
  dir TEXT NOT NULL DEFAULT '', src TEXT NOT NULL DEFAULT '');
CREATE INDEX IF NOT EXISTS songs_by_album ON songs (album_id, disc_no, track_no, rel);
CREATE INDEX IF NOT EXISTS songs_by_dir ON songs (folder, dir);
CREATE INDEX IF NOT EXISTS songs_by_title ON songs (order_title, n);
CREATE INDEX IF NOT EXISTS songs_by_pid ON songs (pid);
CREATE TABLE IF NOT EXISTS song_artists (song_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, PRIMARY KEY (song_id, position)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS albums (
  n INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, name TEXT NOT NULL, artist TEXT NOT NULL, artist_id TEXT NOT NULL,
  year INTEGER, original_year INTEGER, genre TEXT, genres TEXT NOT NULL, song_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL, size INTEGER NOT NULL, discs INTEGER NOT NULL, created INTEGER, mbid TEXT,
  folder INTEGER NOT NULL, dir TEXT NOT NULL, compilation INTEGER NOT NULL, order_name TEXT NOT NULL, base_id TEXT NOT NULL,
  format TEXT, min_kbps INTEGER, cover TEXT, release_type TEXT, type_from TEXT, secondary TEXT NOT NULL DEFAULT '', src TEXT NOT NULL DEFAULT '');
CREATE INDEX IF NOT EXISTS albums_by_base ON albums (base_id);
CREATE INDEX IF NOT EXISTS albums_by_name ON albums (order_name, n);
CREATE INDEX IF NOT EXISTS albums_by_created ON albums (created DESC, n);
CREATE INDEX IF NOT EXISTS albums_by_year ON albums (year, n);
CREATE INDEX IF NOT EXISTS albums_by_artist ON albums (artist_id, year, n);
CREATE TABLE IF NOT EXISTS album_artists (album_id TEXT NOT NULL, artist_id TEXT NOT NULL, name TEXT NOT NULL,
  position INTEGER NOT NULL, mbid TEXT, PRIMARY KEY (album_id, position)) WITHOUT ROWID;
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
-- The indexes' terms (0.9.7): a query's word with typos stands for those it meets (`search::typo_terms`).
CREATE VIRTUAL TABLE IF NOT EXISTS songs_vocab USING fts5vocab(songs_fts, 'row');
CREATE VIRTUAL TABLE IF NOT EXISTS albums_vocab USING fts5vocab(albums_fts, 'row');
CREATE VIRTUAL TABLE IF NOT EXISTS artists_vocab USING fts5vocab(artists_fts, 'row');
";

/// A release's types as kept (0.9.6): its primary type ("album", "ep", "single", "other") and its secondary ones
/// ("compilation", "live", "remix", "soundtrack"…), lower case, from MusicBrainz's names ("Album", "EP",
/// "Broadcast" as other) or Discogs' format descriptions ("LP", "EP", "Single", "Compilation"). Words of neither kind
/// are left out.
pub fn release_words<'a>(words: impl IntoIterator<Item = &'a str>) -> (Option<String>, Vec<String>) {
    const SECONDARY: [&str; 12] =
        ["compilation", "soundtrack", "spokenword", "interview", "audiobook", "audio drama", "live", "remix", "dj-mix", "mixtape/street", "demo", "field recording"];
    let (mut primary, mut secondary): (Option<String>, Vec<String>) = (None, Vec::new());
    for w in words {
        let w = w.trim().to_lowercase();
        let main = match w.as_str() {
            "album" | "lp" => Some("album"),
            "ep" => Some("ep"),
            "single" => Some("single"),
            "other" | "broadcast" => Some("other"),
            _ => None,
        };
        match main {
            Some(m) => {
                primary.get_or_insert_with(|| m.to_string());
            }
            None if SECONDARY.contains(&w.as_str()) && !secondary.contains(&w) => secondary.push(w),
            None => {}
        }
    }
    (primary, secondary)
}

/// A release type as the axes and OpenSubsonic show it ("EP", "DJ-mix", "Compilation").
pub fn type_label(word: &str) -> String {
    match word {
        "ep" => "EP".into(),
        "dj-mix" => "DJ-mix".into(),
        w => {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

/// An album's release type (0.9.6, the owner): its release group's on MusicBrainz, else Discogs' format, else a guess
/// for a short one (three tracks or fewer, under 15 minutes: a single); else none, never an EP by its length.
/// (primary, where from, secondary).
fn release_type_of(
    musicbrainz: Option<&(Option<String>, Vec<String>)>,
    discogs: Option<&(Option<String>, Vec<String>)>,
    tracks: usize,
    duration_ms: u64,
) -> (Option<String>, Option<&'static str>, Vec<String>) {
    let secondary = || musicbrainz.or(discogs).map(|t| t.1.clone()).unwrap_or_default();
    if let Some((Some(p), s)) = musicbrainz {
        return (Some(p.clone()), Some("musicbrainz"), s.clone());
    }
    if let Some((Some(p), s)) = discogs {
        return (Some(p.clone()), Some("discogs"), s.clone());
    }
    if (1..=3).contains(&tracks) && duration_ms > 0 && duration_ms < 15 * 60 * 1000 {
        return (Some("single".into()), Some("guess"), secondary());
    }
    (None, None, secondary())
}

#[cfg(test)]
mod release_type_tests {
    use super::*;

    #[test]
    fn release_types_come_from_musicbrainz_then_discogs_then_a_guess_for_a_single() {
        assert_eq!(release_words(["EP", "Live", "Remastered"]), (Some("ep".into()), vec!["live".into()]));
        assert_eq!(release_words(["LP", "Album", "Compilation"]), (Some("album".into()), vec!["compilation".into()]));
        assert_eq!(release_words(["Broadcast"]).0.as_deref(), Some("other"));
        let mb = (Some("album".to_string()), vec!["live".to_string()]);
        let dc = (Some("ep".to_string()), Vec::new());
        assert_eq!(release_type_of(Some(&mb), Some(&dc), 12, 3_000_000), (Some("album".into()), Some("musicbrainz"), vec!["live".into()]));
        assert_eq!(release_type_of(None, Some(&dc), 4, 1_200_000), (Some("ep".into()), Some("discogs"), Vec::new()));
        assert_eq!(release_type_of(None, None, 2, 7 * 60_000), (Some("single".into()), Some("guess"), Vec::new()));
        assert_eq!(release_type_of(None, None, 5, 22 * 60_000), (None, None, Vec::new()), "never an EP by its length");
        assert_eq!(type_label("ep"), "EP");
        assert_eq!(type_label("compilation"), "Compilation");
    }
}

/// The tables' layout; a library of another layout is built again.
const LAYOUT: i64 = 10;

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
    /// Albums' original years from MusicBrainz (0.9.5), by their MusicBrainz release: for an album whose files carry no
    /// original date.
    originals: Mutex<HashMap<String, u32>>,
    /// The release types found (0.9.6), by `mb:<release>` and `dc:<album id>` (user data `release_types`).
    release_types: Mutex<HashMap<String, (Option<String>, Vec<String>)>>,
    /// Discogs' genres and styles for albums whose files carry none (0.9.5), by album id.
    extra_genres: Mutex<HashMap<String, Vec<String>>>,
    /// One build at a time (0.9.8): a check's build reads every row first, and must not write over the albums Cratarr's
    /// own write has just built again from newer rows.
    building: Mutex<()>,
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

/// The album artist of a file that credits none, not even an artist of its own.
pub const UNKNOWN_ARTIST: &str = "[Unknown Artist]";

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
    let album_artist = row.first("albumartist").map(str::to_string).or_else(|| artist_credit.clone()).unwrap_or_else(|| UNKNOWN_ARTIST.into());
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
        Ok(Self {
            db: Mutex::new(conn),
            added: Mutex::default(),
            originals: Mutex::default(),
            extra_genres: Mutex::default(),
            release_types: Mutex::default(),
            building: Mutex::new(()),
        })
    }

    /// The added dates known better than the catalogue's (by album id); a change builds the library again.
    pub fn set_added(&self, added: HashMap<String, i64>) {
        *self.added.lock().unwrap_or_else(|p| p.into_inner()) = added;
    }

    /// The original years known from MusicBrainz (by MusicBrainz release); a change builds the library again.
    pub fn set_originals(&self, originals: HashMap<String, u32>) {
        *self.originals.lock().unwrap_or_else(|p| p.into_inner()) = originals;
    }

    /// The release types found on MusicBrainz and Discogs (0.9.6); a change builds the library again.
    pub fn set_release_types(&self, types: HashMap<String, (Option<String>, Vec<String>)>) {
        *self.release_types.lock().unwrap_or_else(|p| p.into_inner()) = types;
    }

    /// Discogs' genres for albums whose files carry none (by album id); a change builds the library again.
    pub fn set_extra_genres(&self, genres: HashMap<String, Vec<String>>) {
        *self.extra_genres.lock().unwrap_or_else(|p| p.into_inner()) = genres;
    }

    /// A summary of the original years, part of what the library was built from.
    fn originals_print(&self) -> String {
        let o = self.originals.lock().unwrap_or_else(|p| p.into_inner());
        let g = self.extra_genres.lock().unwrap_or_else(|p| p.into_inner());
        // The release types too (0.9.6): without them here, types that came after a build never built the library
        // again (seen on the sim, 2026-10-26: 21 kept, none shown).
        let t = self.release_types.lock().unwrap_or_else(|p| p.into_inner());
        format!(
            "{}/{} genres {}/{} types {}/{}",
            o.len(),
            o.values().fold(0u64, |a, v| a.wrapping_add(u64::from(*v))),
            g.len(),
            g.values().map(Vec::len).sum::<usize>(),
            t.len(),
            t.values().map(|v| usize::from(v.0.is_some()) + v.1.len()).sum::<usize>()
        )
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
            format!("{} originals {}", self.added_print(), self.originals_print()),
            {
                use md5::Digest;
                md5::Md5::digest(serde_json::to_string(rules)?.as_bytes())
            },
            files
        );
        Ok(change_between(self.built_from().as_deref(), &songs, analyses))
    }

    /// Brings the library up to the catalogue, `print` (from `change`) noted as what it was built from (0.9.8): only the
    /// albums of the files that changed are built again (`update_rows`), unless what it is built from besides its files
    /// changed (`context`: the rules, the folders, a new catalogue), or that fails; then the whole of it. Whether it was
    /// built whole, and how many folders were built again. Blocking.
    pub fn build_from(&self, cat: &crate::catalogue::Catalogue, rules: &Rules, print: &str) -> Result<(bool, usize)> {
        let _one = self.building.lock().unwrap_or_else(|p| p.into_inner());
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
        let covers = match &reader {
            Some(c) => crate::catalogue::cover_prints(c)?,
            None => cat.cover_prints()?,
        };
        let roots: Vec<(i64, String)> = folders.iter().map(|f| (f.folder, f.root.clone())).collect();
        let context = context(&cat.generation(), &roots, rules)?;
        let (whole, dirs) = if self.built_context().as_deref() != Some(context.as_str()) {
            self.build_with(&folders, rules, &covers)?;
            (true, 0)
        } else {
            match self.update_rows(&folders, rules, &covers) {
                Ok(n) => (false, n),
                Err(e) => {
                    tracing::warn!(target: "library", reason = %format!("{e:#}"), "the changed albums could not be built alone: built whole");
                    self.build_with(&folders, rules, &covers)?;
                    (true, 0)
                }
            }
        };
        self.set_built_context(&context)?;
        self.set_built_from(print)?;
        Ok((whole, dirs))
    }

    fn built_from(&self) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [BUILT_FROM], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_built_from(&self, print: &str) -> Result<bool> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![BUILT_FROM, print])?;
        Ok(true)
    }

    /// The kept facts albums are built with besides their files, as they are now.
    fn facts(&self) -> Facts {
        Facts {
            added: self.added.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            originals: self.originals.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            extra_genres: self.extra_genres.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            release_types: self.release_types.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        }
    }

    #[cfg(test)]
    fn build(&self, folders: &[FolderRows], rules: &Rules) -> Result<()> {
        self.build_with(folders, rules, &Covers::new())
    }

    /// Builds the whole library from these files; `covers`, each folder's cover files (`catalogue::cover_prints`).
    fn build_with(&self, folders: &[FolderRows], rules: &Rules, covers: &Covers) -> Result<()> {
        let facts = self.facts();
        let roots: HashMap<i64, &str> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        let rows: Vec<(i64, &ServedRow)> = folders.iter().flat_map(|f| f.rows.iter().map(move |r| (f.folder, r))).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute_batch(
            "DELETE FROM songs; DELETE FROM song_artists; DELETE FROM albums; DELETE FROM album_artists; DELETE FROM artists;
             DELETE FROM genres; DELETE FROM songs_fts; DELETE FROM albums_fts; DELETE FROM artists_fts;",
        )?;
        write_albums(&tx, &rows, &roots, rules, &facts, covers)?;
        artists_again(&tx, None)?;
        genres_again(&tx)?;
        tx.execute_batch("UPDATE songs SET cover = (SELECT a.cover FROM albums a WHERE a.id = songs.album_id);")?;
        // Where each catalogue folder is, so the library can be read before the catalogue is open (decision 0032).
        let roots: Vec<(i64, &str)> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('roots', ?1)", [serde_json::to_string(&roots)?])?;
        tx.commit()?;
        db.execute_batch("ANALYZE;")?;
        Ok(())
    }

    /// Builds again only the albums of the files that changed since they were built (0.9.8, the owner: a fix, a cover,
    /// a Renumber showed only after a whole build, half a minute on the NAS): each song keeps the print of the row it
    /// was built from, each album its kept facts' and cover files'; the folders where any differs, or a file came or
    /// went, are built again, with every folder their albums reach. The number of folders built again. Blocking.
    fn update_rows(&self, folders: &[FolderRows], rules: &Rules, covers: &Covers) -> Result<usize> {
        let facts = self.facts();
        let now: HashMap<(i64, &str), &ServedRow> =
            folders.iter().flat_map(|f| f.rows.iter().filter(|r| r.track.is_some()).map(move |r| ((f.folder, r.rel.as_str()), r))).collect();
        let built = self.built_songs()?;
        let mut dirs: BTreeSet<(i64, String)> = BTreeSet::new();
        for ((folder, rel), r) in &now {
            if built.get(&(*folder, rel.to_string())).is_none_or(|b| b.src != row_src(r)) {
                dirs.insert((*folder, crate::catalogue::dir_of(rel).to_string()));
            }
        }
        for ((folder, rel), b) in &built {
            if !now.contains_key(&(*folder, rel.as_str())) {
                dirs.insert((*folder, b.dir.clone()));
            }
        }
        // Albums whose kept facts or cover files changed.
        for (album, (folder, dir, src, first)) in self.built_albums()? {
            let Some(r) = now.get(&(folder, first.as_str())) else { continue };
            if album_src(&album, folder, &dir, r, &facts, covers) != src {
                dirs.extend(built.values().filter(|b| b.album == album).map(|b| (b.folder, b.dir.clone())));
            }
        }
        if dirs.is_empty() {
            return Ok(0);
        }
        let by_dir: HashMap<(i64, String), Vec<ServedRow>> = folders.iter().fold(HashMap::new(), |mut m, f| {
            for r in &f.rows {
                m.entry((f.folder, crate::catalogue::dir_of(&r.rel).to_string())).or_insert_with(Vec::new).push(r.clone());
            }
            m
        });
        let roots: HashMap<i64, &str> = folders.iter().map(|f| (f.folder, f.root.as_str())).collect();
        self.rebuild_dirs(dirs, &mut |folder, dir| Ok(by_dir.get(&(folder, dir.to_string())).cloned().unwrap_or_default()), &roots, rules, covers)
    }

    /// Builds again the albums of the folders `dirs` holds (of library folders), and of every folder those albums reach,
    /// from their files now (`rows_of`). The number of folders built again. Blocking.
    fn rebuild_dirs(
        &self,
        mut dirs: BTreeSet<(i64, String)>,
        rows_of: &mut dyn FnMut(i64, &str) -> Result<Vec<ServedRow>>,
        roots: &HashMap<i64, &str>,
        rules: &Rules,
        covers: &Covers,
    ) -> Result<usize> {
        let facts = self.facts();
        // The folders' rows, and every folder their albums (as built, and as they would be now) reach.
        let mut rows: Vec<(i64, ServedRow)> = Vec::new();
        let mut read: BTreeSet<(i64, String)> = BTreeSet::new();
        for _ in 0..8 {
            for (folder, dir) in dirs.difference(&read).cloned().collect::<Vec<_>>() {
                rows.extend(rows_of(folder, &dir)?.into_iter().map(|r| (folder, r)));
                read.insert((folder, dir));
            }
            let refs: Vec<(i64, &ServedRow)> = rows.iter().map(|(f, r)| (*f, r)).collect();
            let mut albums: BTreeSet<String> = album_ids(&refs, rules);
            albums.extend(self.built_albums_in(&dirs)?);
            let reach = self.dirs_of(&albums)?;
            if reach.is_subset(&dirs) {
                break;
            }
            dirs.extend(reach);
        }
        if !dirs.is_subset(&read) {
            anyhow::bail!("the albums reach further than they were followed");
        }
        let refs: Vec<(i64, &ServedRow)> = rows.iter().map(|(f, r)| (*f, r)).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        // What was built of them: their songs and albums, and the artists they credit, go.
        let mut touched: BTreeSet<String> = BTreeSet::new();
        let mut gone_albums: BTreeSet<String> = BTreeSet::new();
        {
            let mut songs = tx.prepare("SELECT n, id, album_id FROM songs WHERE folder = ?1 AND dir = ?2")?;
            let mut credits = tx.prepare("SELECT artist_id FROM song_artists WHERE song_id = ?1")?;
            let mut del_fts = tx.prepare("DELETE FROM songs_fts WHERE rowid = ?1")?;
            let mut del_credits = tx.prepare("DELETE FROM song_artists WHERE song_id = ?1")?;
            let mut del_song = tx.prepare("DELETE FROM songs WHERE n = ?1")?;
            for (folder, dir) in &dirs {
                let found: Vec<(i64, String, String)> =
                    songs.query_map(params![folder, dir], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
                for (n, id, album) in found {
                    for a in credits.query_map([&id], |r| r.get::<_, String>(0))? {
                        touched.insert(a?);
                    }
                    del_fts.execute([n])?;
                    del_credits.execute([&id])?;
                    del_song.execute([n])?;
                    gone_albums.insert(album);
                }
            }
            let mut album_n = tx.prepare("SELECT n FROM albums WHERE id = ?1")?;
            let mut album_credits = tx.prepare("SELECT artist_id FROM album_artists WHERE album_id = ?1")?;
            let mut del_album_fts = tx.prepare("DELETE FROM albums_fts WHERE rowid = ?1")?;
            let mut del_album_credits = tx.prepare("DELETE FROM album_artists WHERE album_id = ?1")?;
            let mut del_album = tx.prepare("DELETE FROM albums WHERE id = ?1")?;
            for album in &gone_albums {
                for a in album_credits.query_map([album], |r| r.get::<_, String>(0))? {
                    touched.insert(a?);
                }
                if let Some(n) = album_n.query_row([album], |r| r.get::<_, i64>(0)).optional()? {
                    del_album_fts.execute([n])?;
                }
                del_album_credits.execute([album])?;
                del_album.execute([album])?;
            }
        }
        let (written, credited) = write_albums(&tx, &refs, roots, rules, &facts, covers)?;
        touched.extend(credited);
        artists_again(&tx, Some(&touched))?;
        genres_again(&tx)?;
        {
            let mut cover = tx.prepare("UPDATE songs SET cover = (SELECT a.cover FROM albums a WHERE a.id = songs.album_id) WHERE album_id = ?1")?;
            for album in &written {
                cover.execute([album])?;
            }
        }
        tx.commit()?;
        Ok(dirs.len())
    }

    /// Each song as built: (folder, rel) → its folder, album and the print of its row.
    fn built_songs(&self) -> Result<HashMap<(i64, String), BuiltSong>> {
        let db = self.db();
        let mut st = db.prepare("SELECT folder, rel, dir, album_id, src FROM songs")?;
        let rows = st.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), BuiltSong { folder: r.get(0)?, dir: r.get(2)?, album: r.get(3)?, src: r.get(4)? })))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Each album as built: its folder and folder there, its print, and the file of its first song.
    fn built_albums(&self) -> Result<Vec<(String, (i64, String, String, String))>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT a.id, a.folder, a.dir, a.src, (SELECT s.rel FROM songs s WHERE s.album_id = a.id ORDER BY s.n LIMIT 1) FROM albums a",
        )?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, Option<String>>(4)?))))?;
        Ok(rows.filter_map(|r| r.map(|(a, (f, d, s, first))| first.map(|first| (a, (f, d, s, first)))).transpose()).collect::<rusqlite::Result<_>>()?)
    }

    /// The albums whose songs are in these folders, as built.
    fn built_albums_in(&self, dirs: &BTreeSet<(i64, String)>) -> Result<BTreeSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT DISTINCT album_id FROM songs WHERE folder = ?1 AND dir = ?2")?;
        let mut out = BTreeSet::new();
        for (folder, dir) in dirs {
            for a in st.query_map(params![folder, dir], |r| r.get::<_, String>(0))? {
                out.insert(a?);
            }
        }
        Ok(out)
    }

    /// The folders the songs of these albums are in, as built.
    fn dirs_of(&self, albums: &BTreeSet<String>) -> Result<BTreeSet<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT DISTINCT folder, dir FROM songs WHERE album_id = ?1")?;
        let mut out = BTreeSet::new();
        for a in albums {
            for d in st.query_map([a], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                out.insert(d?);
            }
        }
        Ok(out)
    }

    /// What the library was built from besides its files (the catalogue's generation, its folders, the rules): when it
    /// changes, the whole library is built again.
    fn built_context(&self) -> Option<String> {
        self.db().query_row("SELECT value FROM meta WHERE key = 'built_context'", [], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_built_context(&self, context: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('built_context', ?1)", [context])?;
        Ok(())
    }

    /// Builds again just the albums of files Cratarr wrote itself (0.9.8: a fix, a cover, a Renumber, a tempo, a track
    /// added): read from the catalogue for their folders alone, so they show at once. Nothing where the library is not
    /// built from the catalogue as it is now in all else (its next check builds it). The number of folders built
    /// again. Blocking.
    pub fn update_files(&self, cat: &crate::catalogue::Catalogue, rules: &Rules, paths: &[std::path::PathBuf]) -> Result<usize> {
        let _one = self.building.lock().unwrap_or_else(|p| p.into_inner());
        let roots_now: Vec<(i64, String)> = cat.folders().iter().enumerate().map(|(i, p)| (i as i64, p.display().to_string())).collect();
        if self.built_context().as_deref() != Some(context(&cat.generation(), &roots_now, rules)?.as_str()) {
            return Ok(0);
        }
        let mut dirs = BTreeSet::new();
        for path in paths {
            if let Some((folder, root)) = roots_now.iter().find(|(_, root)| path.starts_with(root))
                && let Ok(rel) = path.strip_prefix(root)
            {
                let rel = rel.components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join("/");
                dirs.insert((*folder, crate::catalogue::dir_of(&rel).to_string()));
            }
        }
        if dirs.is_empty() {
            return Ok(0);
        }
        let covers = cat.cover_prints()?;
        let roots: HashMap<i64, &str> = roots_now.iter().map(|(f, r)| (*f, r.as_str())).collect();
        self.rebuild_dirs(dirs, &mut |folder, dir| cat.served_rows_in(folder, dir), &roots, rules, &covers)
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

    /// Each of these songs' catalogue folder and place in it (its file), those the library has.
    pub fn song_places(&self, ids: &[String]) -> Result<std::collections::HashMap<String, (i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT folder, rel FROM songs WHERE id = ?1")?;
        let mut out = std::collections::HashMap::new();
        for id in ids {
            if let Some(p) = st.query_row([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).optional()? {
                out.insert(id.clone(), p);
            }
        }
        Ok(out)
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

    /// Every album lacking something the library requires (the owner, 2026-10-24): its cover, year, genre, album
    /// artist, a track number, its MusicBrainz id.
    pub fn lacking(&self) -> Result<Vec<Lacks>> {
        self.lacks_where("", None)
    }

    /// One album's lacks (0.9.8: a fix applied looks at its own album, not the whole library); none where it lacks
    /// nothing, or is not there.
    pub fn lacks_of(&self, album: &str) -> Result<Option<Lacks>> {
        Ok(self.lacks_where(" WHERE al.id = ?1", Some(album))?.pop())
    }

    fn lacks_where(&self, filter: &str, album: Option<&str>) -> Result<Vec<Lacks>> {
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT al.id, al.name, al.artist, al.folder,
                    al.cover IS NULL,
                    al.year IS NULL OR al.year = 0,
                    al.genres = '',
                    EXISTS (SELECT 1 FROM songs s WHERE s.album_id = al.id AND (s.album_artist IS NULL OR s.album_artist = '')),
                    EXISTS (SELECT 1 FROM songs s WHERE s.album_id = al.id AND (s.track_no IS NULL OR s.track_no = 0)),
                    al.mbid IS NULL OR al.mbid = ''
             FROM albums al{filter}"
        ))?;
        let params: Vec<&str> = album.into_iter().collect();
        let rows = st
            .query_map(rusqlite::params_from_iter(params), |r| {
                Ok(Lacks {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    artist: r.get(2)?,
                    folder: r.get(3)?,
                    cover: r.get(4)?,
                    year: r.get(5)?,
                    genre: r.get(6)?,
                    album_artist: r.get(7)?,
                    track_no: r.get(8)?,
                    musicbrainz: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter(Lacks::any).collect())
    }

    /// Every artist a track credits (on compilations and as guests too): Discover's "New to you" is for none of them.
    pub fn track_artist_names(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT DISTINCT name FROM song_artists")?;
        let names = st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(names)
    }

    /// The albums without a cover: Home leaves them out (the owner, 2026-10-24).
    pub fn coverless(&self) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id FROM albums WHERE cover IS NULL")?;
        let ids = st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(ids)
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

/// Each folder's cover files, as a print (`catalogue::cover_prints`): by (library folder, folder in it).
pub type Covers = HashMap<(i64, String), String>;

/// The kept facts albums are built with besides their files (`Library::set_added` and the rest).
struct Facts {
    added: HashMap<String, i64>,
    originals: HashMap<String, u32>,
    extra_genres: HashMap<String, Vec<String>>,
    release_types: HashMap<String, (Option<String>, Vec<String>)>,
}

/// A song as built: its folder there, its album and its row's print.
struct BuiltSong {
    folder: i64,
    dir: String,
    album: String,
    src: String,
}

/// The print of the row a song is built from: everything in it (its tags, its file, its analysis); another print, a
/// song to build again.
fn row_src(r: &ServedRow) -> String {
    use md5::Digest;
    format!("{:x}", md5::Md5::digest(format!("{r:?}").as_bytes()))
}

/// The print of what an album is built from besides its songs' rows: its kept facts (added date, original year,
/// Discogs' genres, release types) and its folder's cover files.
fn album_src(album: &str, folder: i64, dir: &str, first: &ServedRow, facts: &Facts, covers: &Covers) -> String {
    use md5::Digest;
    let release = first.first("musicbrainz_albumid");
    let said = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        facts.added.get(album),
        release.and_then(|m| facts.originals.get(m)),
        facts.extra_genres.get(album),
        release.and_then(|m| facts.release_types.get(&format!("mb:{m}"))),
        facts.release_types.get(&format!("dc:{album}")),
        covers.get(&(folder, dir.to_string())),
    );
    format!("{:x}", md5::Md5::digest(said.as_bytes()))
}

/// What the library is built from besides its files: the catalogue's generation, its folders, the rules.
fn context(generation: &str, roots: &[(i64, String)], rules: &Rules) -> Result<String> {
    use md5::Digest;
    Ok(format!("{LAYOUT} {generation} {} {:x}", serde_json::to_string(roots)?, md5::Md5::digest(serde_json::to_string(rules)?.as_bytes())))
}

/// The songs these rows make, each folder one album (`one_album_per_folder`).
fn songs_of<'a>(rows: &[(i64, &'a ServedRow)], rules: &Rules) -> Vec<Song<'a>> {
    let mut songs: Vec<Song> = rows.iter().filter(|(_, r)| r.track.is_some()).map(|(folder, r)| song(r, *folder, rules)).collect();
    one_album_per_folder(&mut songs);
    songs
}

/// The albums these rows make.
fn album_ids(rows: &[(i64, &ServedRow)], rules: &Rules) -> BTreeSet<String> {
    songs_of(rows, rules).into_iter().map(|s| s.album_id).collect()
}

/// Writes the songs and albums these rows make (their artists counted apart, `artists_again`): what a whole build
/// and a build of some folders share. The albums written, and every artist they credit.
fn write_albums(
    tx: &Connection,
    rows: &[(i64, &ServedRow)],
    roots: &HashMap<i64, &str>,
    rules: &Rules,
    facts: &Facts,
    covers: &Covers,
) -> Result<(Vec<String>, BTreeSet<String>)> {
    let songs = songs_of(rows, rules);
    let mut credited: BTreeSet<String> = BTreeSet::new();
    let mut albums: BTreeMap<String, Vec<&Song>> = BTreeMap::new();
    {
        let mut ins_song = tx.prepare_cached(
            "INSERT INTO songs (id, pid, folder, rel, album_id, title, artist, album, album_artist, track_no, disc_no, year,
               genre, bpm, duration_ms, size, suffix, kbps, created, mbid, order_title, path,
               rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak, genres, dir, src)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
               ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
        )?;
        let mut ins_fts = tx.prepare_cached("INSERT INTO songs_fts (rowid, title, artist, album) VALUES (?1, ?2, ?3, ?4)")?;
        let mut ins_artist = tx.prepare_cached("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES (?1, ?2, ?3, ?4)")?;
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
                crate::catalogue::dir_of(&r.rel),
                row_src(r),
            ])?;
            let n = tx.last_insert_rowid();
            ins_fts.execute(params![n, s.title, s.artist_credit.as_deref().unwrap_or(""), s.album])?;
            for (i, name) in s.artists.iter().enumerate() {
                let aid = ids::artist(name);
                ins_artist.execute(params![id, aid, name, i as i64])?;
                credited.insert(aid);
            }
            albums.entry(s.album_id.clone()).or_default().push(s);
        }
    }
    let mut ins_album = tx.prepare_cached(
        "INSERT INTO albums (id, name, artist, artist_id, year, original_year, genre, genres, song_count, duration_ms,
           size, discs, created, mbid, folder, dir, compilation, order_name, base_id, format, min_kbps, cover,
           release_type, type_from, secondary, src)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
           ?23, ?24, ?25, ?26)",
    )?;
    let mut ins_fts = tx.prepare_cached("INSERT INTO albums_fts (rowid, name, artist) VALUES (?1, ?2, ?3)")?;
    let mut ins_aa = tx.prepare_cached("INSERT INTO album_artists (album_id, artist_id, name, position, mbid) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for (album_id, songs) in &albums {
        let first = songs[0];
        let years: Vec<u32> = songs.iter().filter_map(|s| s.row.first("date").and_then(year_of)).collect();
        // Its original year: its files' original date, else MusicBrainz's for its release (0.9.5); shown as its year
        // where it is earlier than its release's (a reissue's), so every list and order has it.
        let original = songs
            .iter()
            .filter_map(|s| s.row.first("originaldate").and_then(year_of))
            .min()
            .or_else(|| first.row.first("musicbrainz_albumid").and_then(|m| facts.originals.get(m)).copied());
        let album_year = match (original, years.iter().max().copied()) {
            (Some(o), Some(d)) => Some(o.min(d)),
            (o, d) => o.or(d),
        };
        let mut genres: Vec<String> = Vec::new();
        for s in songs.iter() {
            for g in &s.genres {
                if !genres.iter().any(|x| x.eq_ignore_ascii_case(g)) {
                    genres.push(g.clone());
                }
            }
        }
        // None in its files: Discogs' genres and styles where they were asked (0.9.5; the files never written).
        if genres.is_empty()
            && let Some(g) = facts.extra_genres.get(album_id)
        {
            genres = g.clone();
        }
        // The album's folder: the one its files share.
        let dirs: Vec<&str> = songs.iter().map(|s| s.row.rel.rsplit_once('/').map_or("", |(d, _)| d)).collect();
        let dir = common_dir(&dirs);
        let discs = songs.iter().filter_map(|s| s.row.first("discnumber").and_then(leading_number)).max().unwrap_or(1);
        let compilation = songs.iter().any(|s| s.row.first("compilation").is_some_and(|c| c == "1"));
        let artist_id = ids::artist(&first.album_artists[0]);
        // The art's id: the album's, with a fingerprint of its picture (see `art`).
        let root = roots.get(&first.folder).copied().unwrap_or("");
        let first_track = songs.iter().map(|s| &s.row.rel).min().map(|r| Path::new(root).join(r));
        let cover = super::art::album_print(&Path::new(root).join(&dir), first_track.as_deref()).map(|p| format!("{album_id}_{p}"));
        // One format for the whole album, else none (mixed).
        let formats: BTreeSet<&str> = songs.iter().filter_map(|s| s.row.format.as_deref()).collect();
        let format = (formats.len() == 1).then(|| formats.iter().next().map(|f| f.to_string())).flatten();
        let duration_ms = songs.iter().filter_map(|s| s.row.duration_ms).sum::<u64>();
        let (kind, type_from, secondary) = release_type_of(
            first.row.first("musicbrainz_albumid").and_then(|m| facts.release_types.get(&format!("mb:{m}"))),
            facts.release_types.get(&format!("dc:{album_id}")),
            songs.len(),
            duration_ms,
        );
        ins_album.execute(params![
            album_id,
            first.album,
            first.album_artist,
            artist_id,
            album_year,
            original,
            genres.first(),
            genres.join("\u{1f}"),
            songs.len() as i64,
            duration_ms as i64,
            songs.iter().map(|s| s.row.size).sum::<u64>() as i64,
            discs,
            songs.iter().filter_map(|s| s.row.born).chain(facts.added.get(album_id).copied()).min(),
            first.row.first("musicbrainz_albumid"),
            first.folder,
            dir,
            compilation,
            order_name(&first.album),
            first.album_base,
            format,
            songs.iter().filter_map(|s| s.row.kbps).min(),
            cover,
            kind,
            type_from,
            secondary.join("\u{1f}"),
            album_src(album_id, first.folder, &dir, first.row, facts, covers),
        ])?;
        ins_fts.execute(params![tx.last_insert_rowid(), first.album, first.album_artist])?;
        for (i, name) in first.album_artists.iter().enumerate() {
            let aid = ids::artist(name);
            let mbid = if i == 0 { first.row.first("musicbrainz_albumartistid") } else { None };
            ins_aa.execute(params![album_id, aid, name, i as i64, mbid])?;
            credited.insert(aid);
        }
    }
    Ok((albums.into_keys().collect(), credited))
}

/// The artists counted again from what the songs and albums credit now: all of them, or `only` these (those no longer
/// credited go). Each one's name is the one it is credited by most (the shorter spelling first among equals), its
/// MusicBrainz id the first album's (by id) that names it first, its counts its albums and its songs.
fn artists_again(tx: &Connection, only: Option<&BTreeSet<String>>) -> Result<()> {
    let wanted = |aid: &str| only.is_none_or(|o| o.contains(aid));
    let mut names: HashMap<String, (String, i64)> = HashMap::new();
    {
        let mut st = tx.prepare(
            "SELECT artist_id, name, count(*) FROM (SELECT artist_id, name FROM song_artists UNION ALL SELECT artist_id, name FROM album_artists)
             GROUP BY artist_id, name",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (aid, name, n) = row?;
            if !wanted(&aid) {
                continue;
            }
            let e = names.entry(aid).or_insert_with(|| (name.clone(), n));
            if n > e.1 || (n == e.1 && name < e.0) {
                *e = (name, n);
            }
        }
    }
    let counts = |sql: &str| -> Result<HashMap<String, i64>> {
        let mut st = tx.prepare(sql)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let songs = counts("SELECT artist_id, count(*) FROM song_artists GROUP BY artist_id")?;
    let albums = counts("SELECT artist_id, count(DISTINCT album_id) FROM album_artists GROUP BY artist_id")?;
    let mut mbids: HashMap<String, String> = HashMap::new();
    {
        let mut st = tx.prepare("SELECT artist_id, mbid FROM album_artists WHERE position = 0 AND mbid IS NOT NULL ORDER BY album_id")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (aid, mbid) = row?;
            mbids.entry(aid).or_insert(mbid);
        }
    }
    let existing: HashMap<String, i64> = {
        let mut st = tx.prepare("SELECT id, n FROM artists")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()?.into_iter().filter(|(a, _)| wanted(a)).collect()
    };
    let mut ids: BTreeSet<&String> = names.keys().collect();
    ids.extend(existing.keys());
    let mut ins = tx.prepare_cached("INSERT INTO artists (id, name, mbid, album_count, song_count, order_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    let mut upd = tx.prepare_cached("UPDATE artists SET name = ?2, mbid = ?3, album_count = ?4, song_count = ?5, order_name = ?6 WHERE id = ?1")?;
    let mut del = tx.prepare_cached("DELETE FROM artists WHERE id = ?1")?;
    let mut ins_fts = tx.prepare_cached("INSERT INTO artists_fts (rowid, name) VALUES (?1, ?2)")?;
    let mut del_fts = tx.prepare_cached("DELETE FROM artists_fts WHERE rowid = ?1")?;
    for aid in ids {
        let had = existing.get(aid).copied();
        let Some((name, _)) = names.get(aid) else {
            if let Some(n) = had {
                del_fts.execute([n])?;
                del.execute([aid])?;
            }
            continue;
        };
        let values = params![aid, name, mbids.get(aid), albums.get(aid).copied().unwrap_or(0), songs.get(aid).copied().unwrap_or(0), order_name(name)];
        let n = match had {
            Some(n) => {
                upd.execute(values)?;
                del_fts.execute([n])?;
                n
            }
            None => {
                ins.execute(values)?;
                tx.last_insert_rowid()
            }
        };
        ins_fts.execute(params![n, name])?;
    }
    Ok(())
}

/// The genres counted again: each genre's songs and albums, named as the first song (by album, then as built) has it.
fn genres_again(tx: &Connection) -> Result<()> {
    let mut genre_songs: BTreeMap<String, (String, u32, BTreeSet<String>)> = BTreeMap::new();
    {
        let mut st = tx.prepare("SELECT album_id, genres FROM songs ORDER BY album_id, n")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (album, genres) = row?;
            for g in genres.split('\u{1f}').filter(|g| !g.is_empty()) {
                let e = genre_songs.entry(g.to_lowercase()).or_insert_with(|| (g.to_string(), 0, BTreeSet::new()));
                e.1 += 1;
                e.2.insert(album.clone());
            }
        }
    }
    tx.execute("DELETE FROM genres", [])?;
    let mut ins = tx.prepare_cached("INSERT OR IGNORE INTO genres (name, song_count, album_count) VALUES (?1, ?2, ?3)")?;
    for (_, (name, songs, albums)) in genre_songs {
        ins.execute(params![name, songs, albums.len() as i64])?;
    }
    Ok(())
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

    // ---- Built again where it changed (0.9.8): always what a whole build gives.

    /// Every table as rows of text, without the row numbers (a build of some folders numbers its rows anew), sorted; and
    /// what each search index finds for a few words, as ids.
    fn dump(lib: &Library) -> Vec<String> {
        let db = lib.db();
        let mut out = Vec::new();
        for (table, skip_n) in
            [("songs", true), ("song_artists", false), ("albums", true), ("album_artists", false), ("artists", true), ("genres", false)]
        {
            let mut st = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let cols = st.column_count();
            let mut rows: Vec<String> = st
                .query_map([], |r| {
                    let mut line = format!("{table}:");
                    for i in usize::from(skip_n)..cols {
                        let v: rusqlite::types::Value = r.get(i)?;
                        line.push_str(&format!(" {v:?}"));
                    }
                    Ok(line)
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            rows.sort();
            out.extend(rows);
        }
        for word in ["björk", "orb", "hunter", "clouds", "homogenic", "new", "move"] {
            for (fts, table) in [("songs_fts", "songs"), ("albums_fts", "albums"), ("artists_fts", "artists")] {
                let mut st = db.prepare(&format!("SELECT t.id FROM {fts} f JOIN {table} t ON t.n = f.rowid WHERE {fts} MATCH ?1")).unwrap();
                let mut ids: Vec<String> = st.query_map([format!("\"{word}\"*")], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
                ids.sort();
                out.push(format!("{fts} {word}: {ids:?}"));
            }
        }
        out
    }

    /// `before` built whole, then brought to `after` where it changed; against `after` built whole. The folders built
    /// again.
    fn same_as_whole(before: &[FolderRows], after: &[FolderRows], facts: impl Fn(&Library)) -> usize {
        let rules = Rules::default();
        let step = Library::open(None).unwrap();
        step.build(before, &rules).unwrap();
        facts(&step);
        let n = step.update_rows(after, &rules, &Covers::new()).unwrap();
        let whole = Library::open(None).unwrap();
        facts(&whole);
        whole.build(after, &rules).unwrap();
        let (a, b) = (dump(&step), dump(&whole));
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x, y, "built again where it changed, against built whole");
        }
        assert_eq!(a.len(), b.len());
        n
    }

    fn with(mut f: Vec<FolderRows>, change: impl FnOnce(&mut Vec<ServedRow>)) -> Vec<FolderRows> {
        change(&mut f[0].rows);
        f
    }

    fn tag(r: &mut ServedRow, name: &str, value: &str) {
        r.tags.retain(|(n, _)| n != name);
        r.tags.push((name.into(), value.into()));
        r.mtime += 1;
    }

    #[test]
    fn nothing_changed_builds_nothing_again() {
        assert_eq!(same_as_whole(&sample(), &sample(), |_| {}), 0);
    }

    #[test]
    fn a_retagged_track_builds_its_album_again_and_nothing_else() {
        let after = with(sample(), |rows| tag(&mut rows[1], "genre", "Electronic; Trip Hop"));
        assert_eq!(same_as_whole(&sample(), &after, |_| {}), 1);
        // A new album artist: a new album id, the old artist gone.
        let after = with(sample(), |rows| {
            tag(&mut rows[2], "artist", "Orbital");
            tag(&mut rows[2], "albumartist", "Orbital");
        });
        assert_eq!(same_as_whole(&sample(), &after, |_| {}), 1);
    }

    #[test]
    fn files_that_come_go_or_move_are_built_as_a_whole_build_would() {
        let new_one = |rows: &mut Vec<ServedRow>| {
            rows.push(row(4, "Björk/Homogenic (1997)/03 Unravel.flac", &[
                ("title", "Unravel"),
                ("artist", "Björk"),
                ("albumartist", "Björk"),
                ("album", "Homogenic"),
                ("tracknumber", "03"),
                ("genre", "Electronic"),
            ]))
        };
        assert_eq!(same_as_whole(&sample(), &with(sample(), new_one), |_| {}), 1);
        assert_eq!(same_as_whole(&sample(), &with(sample(), |rows| drop(rows.remove(0))), |_| {}), 1);
        // The Orb's album moved to another folder: the old one empty, the new one new.
        let moved = with(sample(), |rows| rows[2].rel = "The Orb/Adventures Beyond (1991)/01 Little Fluffy Clouds.mp3".into());
        assert_eq!(same_as_whole(&sample(), &moved, |_| {}), 2);
        // A whole album gone, and its artist with it; a new album by a new artist.
        let swapped = with(sample(), |rows| {
            rows.remove(2);
            rows.push(row(9, "New Order/Movement (1981)/01 Dreams Never End.flac", &[("title", "Dreams Never End"), ("artist", "New Order"), ("album", "Movement")]));
        });
        assert_eq!(same_as_whole(&sample(), &swapped, |_| {}), 2);
    }

    #[test]
    fn an_album_over_two_folders_is_built_again_whole() {
        let two_discs = |disc2: &str| {
            with(sample(), |rows| {
                for (t, d, title) in [(5, "CD1", "Hunter"), (6, "CD2", disc2)] {
                    rows.push(row(t, &format!("Björk/Live Box/{d}/01 {title}.flac"), &[
                        ("title", title),
                        ("artist", "Björk"),
                        ("albumartist", "Björk"),
                        ("album", "Live Box"),
                        ("discnumber", if d == "CD1" { "1" } else { "2" }),
                    ]));
                }
            })
        };
        // One file of the second disc changed: both discs' folders built again (the album's songs are in both).
        assert_eq!(same_as_whole(&two_discs("Jóga"), &two_discs("Jóga (Live)"), |_| {}), 2);
    }

    #[test]
    fn an_albums_kept_facts_build_it_again() {
        let none = with(sample(), |rows| rows[2].tags.retain(|(n, _)| n != "genre"));
        let orb = || {
            let lib = Library::open(None).unwrap();
            lib.build(&none, &Rules::default()).unwrap();
            let db = lib.db();
            db.query_row("SELECT id FROM albums WHERE name LIKE 'Adventures%'", [], |r| r.get::<_, String>(0)).unwrap()
        };
        let id = orb();
        let step = Library::open(None).unwrap();
        step.build(&none, &Rules::default()).unwrap();
        // Discogs' genres for it came, and an earlier added date.
        step.set_extra_genres(HashMap::from([(id.clone(), vec!["Ambient House".to_string()])]));
        step.set_added(HashMap::from([(id.clone(), 1_600_000_000)]));
        assert_eq!(step.update_rows(&none, &Rules::default(), &Covers::new()).unwrap(), 1);
        let whole = Library::open(None).unwrap();
        whole.set_extra_genres(HashMap::from([(id.clone(), vec!["Ambient House".to_string()])]));
        whole.set_added(HashMap::from([(id, 1_600_000_000)]));
        whole.build(&none, &Rules::default()).unwrap();
        assert_eq!(dump(&step), dump(&whole));
    }

    #[test]
    fn new_analyses_build_their_albums_again() {
        let analysed = with(sample(), |rows| {
            rows[0].analysis = Some(crate::catalogue::RowAnalysis { track_gain_db: -7.5, track_peak: 0.98, bpm: Some(92.0), album: Some((-7.0, 0.99)) })
        });
        assert_eq!(same_as_whole(&sample(), &analysed, |_| {}), 1);
    }

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
    fn an_albums_length_is_its_songs_summed_and_unknown_when_one_is() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        let listed = lib.albums_in(0, 10).unwrap();
        let (homogenic, _, _, _, secs) = listed.iter().find(|(a, ..)| a.name == "Homogenic").unwrap();
        assert_eq!(*secs, Some(400.0), "two songs of 200 s");
        assert_eq!(lib.album_facts(&homogenic.id).unwrap().unwrap().3, Some(400.0));
        let untimed = with(sample(), |rows| rows[1].duration_ms = None);
        let lib = Library::open(None).unwrap();
        lib.refresh(&untimed, &Rules::default()).unwrap();
        let listed = lib.albums_in(0, 10).unwrap();
        let (homogenic, _, _, _, secs) = listed.iter().find(|(a, ..)| a.name == "Homogenic").unwrap();
        assert_eq!(*secs, None, "one song's length unknown: the album's is");
        assert_eq!(lib.album_facts(&homogenic.id).unwrap().unwrap().3, None);
        let (_, _, _, _, orb) = listed.iter().find(|(a, ..)| a.name == "Adventures Beyond the Ultraworld").unwrap();
        assert_eq!(*orb, Some(200.0), "another album's is its own");
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
    fn an_album_shows_its_original_year_from_its_files_else_musicbrainz() {
        let album = |t: i64, name: &str, tags: &[(&str, &str)]| {
            let mut all = vec![("title", "A"), ("artist", "Ott"), ("albumartist", "Ott"), ("album", name)];
            all.extend_from_slice(tags);
            row(t, &format!("Ott/{name}/01 A.flac"), &all)
        };
        let folders = || {
            vec![FolderRows {
                folder: 0,
                root: "/music".into(),
                rows: vec![
                    album(1, "Skylon", &[("date", "2012-09-24"), ("musicbrainz_albumid", "rel-skylon")]),
                    album(2, "Mir", &[("date", "2015"), ("originaldate", "2011-03-01"), ("musicbrainz_albumid", "rel-mir")]),
                    album(3, "Odd", &[("date", "2005"), ("musicbrainz_albumid", "rel-odd")]),
                ],
            }]
        };
        let years = |lib: &Library| -> Vec<(String, Option<i64>, Option<i64>)> {
            let db = lib.db();
            let mut st = db.prepare("SELECT name, year, original_year FROM albums ORDER BY name").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
        };
        let lib = Library::open(None).unwrap();
        lib.refresh(&folders(), &Rules::default()).unwrap();
        assert_eq!(years(&lib)[2], ("Skylon".into(), Some(2012), None), "a reissue's year without anything better");
        let lib = Library::open(None).unwrap();
        lib.set_originals([("rel-skylon".into(), 2008), ("rel-mir".into(), 1999), ("rel-odd".into(), 2009)].into());
        lib.refresh(&folders(), &Rules::default()).unwrap();
        assert_eq!(
            years(&lib),
            [("Mir".into(), Some(2011), Some(2011)), ("Odd".into(), Some(2005), Some(2009)), ("Skylon".into(), Some(2008), Some(2008))],
            "the files' original date first; MusicBrainz's where they have none; never later than the release's"
        );
        // Release types that come after a build build it again (0.9.6: on the sim they never showed).
        let before = lib.originals_print();
        lib.set_release_types([("mb:rel-odd".into(), (Some("ep".into()), vec!["live".into()]))].into());
        assert_ne!(lib.originals_print(), before, "new types change what the library was built from (`change`)");
        lib.build(&folders(), &Rules::default()).unwrap();
        let t: (Option<String>, Option<String>, String) = lib
            .db()
            .query_row("SELECT release_type, type_from, secondary FROM albums WHERE name = 'Odd'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(t, (Some("ep".into()), Some("musicbrainz".into()), "live".into()));
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
        drop(db);
        // Each kind counted in all, beyond the one shown: an album by its own name or by a track's title, an artist by
        // their name or an album's.
        let words = |q: &str| crate::search::words(q);
        let f = lib.find(&words("bjork"), 1).unwrap();
        assert_eq!((f.tracks.len(), f.totals.tracks, f.totals.albums, f.totals.artists), (1, 2, 1, 1));
        assert_eq!(lib.find(&words("hunter"), 1).unwrap().totals.albums, 1, "by a track's title");
        assert_eq!(lib.find(&words("homogenic"), 1).unwrap().totals.artists, 1, "by an album's name");
        assert_eq!(lib.find(&words("art"), 1).unwrap().totals.genres, 1);
        // The Library page's Tracks: every track without words, those the words name, none from a folder switched off.
        assert_eq!(lib.track_rows(&[], &[]).unwrap().len(), 3);
        let rows = lib.track_rows(&words("bjork"), &[]).unwrap();
        assert_eq!((rows.len(), rows[0].album.as_str()), (2, "Homogenic"));
        assert!(lib.track_rows(&words("bjork"), &[0]).unwrap().is_empty());
        // Several albums' tracks: the albums in the order asked for, each one's in its own; one asked twice once.
        let rows = lib.track_rows(&[], &[]).unwrap();
        let id_of = |album: &str| rows.iter().find(|r| r.album == album).unwrap().album_id.clone();
        let (homogenic, orb) = (id_of("Homogenic"), id_of("Adventures Beyond the Ultraworld"));
        let songs = lib.songs_of_albums(&[orb.clone(), homogenic.clone(), "nothing".into(), orb]).unwrap();
        assert_eq!(songs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Little Fluffy Clouds", "Hunter", "Jóga"]);
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
        // One album's files written by Cratarr (0.9.8): only its folder built again.
        let (folder, rel): (i64, String) = lib
            .db()
            .query_row("SELECT folder, rel FROM songs WHERE album_id = (SELECT id FROM albums ORDER BY song_count DESC LIMIT 1) LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        let path = cat.folders()[folder as usize].join(&rel);
        let t = std::time::Instant::now();
        let n = lib.update_files(&cat, &Rules::default(), &[path]).unwrap();
        println!("one album's folder built again: {n} folders in {:?}", t.elapsed());
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

/// What an album lacks of what the library requires (`Library::lacking`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lacks {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub folder: i64,
    pub cover: bool,
    pub year: bool,
    pub genre: bool,
    pub album_artist: bool,
    pub track_no: bool,
    pub musicbrainz: bool,
}

impl Lacks {
    pub fn any(&self) -> bool {
        self.cover || self.year || self.genre || self.album_artist || self.track_no || self.musicbrainz
    }

    /// What it lacks, in words (Incoming's marks).
    pub fn words(&self) -> Vec<&'static str> {
        [
            (self.cover, "cover"),
            (self.year, "year"),
            (self.genre, "genre"),
            (self.album_artist, "album artist"),
            (self.track_no, "track numbers"),
            (self.musicbrainz, "MusicBrainz id"),
        ]
        .into_iter()
        .filter_map(|(lacks, word)| lacks.then_some(word))
        .collect()
    }
}
