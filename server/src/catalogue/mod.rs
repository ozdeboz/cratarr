//! The catalogue (decision 0008): every audio file in the playlist folder and the extra folders, its tags as stored,
//! and each server's id for it, in one SQLite file in the data folder (or in memory without one). Its track ids are the
//! truth everything else names (0101): a lost file is restored from the nightly backup, never made anew in passing (a new
//! one gives out no ids until an administrator starts them afresh; `Library::fresh_track_ids`).
//!
//! Passes run in the background (`run`): walk the folders (size and modification time decide what changed), read
//! the tags of new and changed files a batch at a time, then ask every server for its tracks (paged, then only what
//! changed) and link each to a file. A folder that cannot be read, or that walks to nothing where it had files, keeps its
//! rows and reports it: an unmounted folder must never look like "everything was deleted".

mod artists;
mod first;
mod id3;
pub mod identity;
mod index_log;
pub mod normalise;
pub mod tags;
pub mod walk;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub use first::{First, FirstAlbums};
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
-- 0101 point 11: a file Cratarr is putting in an old one's place (a better copy, its Undo) takes that track when it is
-- identified (`Catalogue::hand_over`); forgotten after a day unclaimed.
CREATE TABLE IF NOT EXISTS handovers (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, track INTEGER NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS tracks (id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER);
-- 0101: a track a record's track took (`Catalogue::assign_records`) and what it became, always the last of a line of them.
-- Kept for good: an Undo from before, an export or a queue can name `old` long after, and it must not come back.
-- `followed`: what the user data names of it has moved (`Serve::built_now`).
CREATE TABLE IF NOT EXISTS track_moved (old INTEGER PRIMARY KEY, id INTEGER NOT NULL, followed INTEGER NOT NULL DEFAULT 0);
-- 0100: a record's id for life, as a track's is, owned or not: one with no tracks, not retired, is a record not had.
-- Its facts are here, so a retired one keeps what it was. Its copies are its tracks' files, one folder's each
-- (docs/records-and-copies.md).
CREATE TABLE IF NOT EXISTS records (
  id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER,
  title TEXT, artist TEXT, year INTEGER, kind TEXT, counting TEXT);
-- 0100: an artist's id for life, served as `ar-<id>`, kept when nothing credits them any more (`catalogue::artists`).
-- `key`, `ids::artist_key`: one name, one row, until references say two share it, so no unique index.
CREATE TABLE IF NOT EXISTS artists (
  id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, name TEXT NOT NULL, sort TEXT, key TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS artists_by_key ON artists (key);
-- Where an artist's id from before 0.16 went (`ar-<hash>`): what the migration moves by, kept with what it maps, as a track's.
CREATE TABLE IF NOT EXISTS artist_moved (old TEXT PRIMARY KEY, id INTEGER NOT NULL) WITHOUT ROWID;
-- Where an album's ids from before 0.16 went (`Catalogue::first_records`), kept with what it maps, as a track's. `form` is `id`,
-- what one copy was known by, or `base`, the same in Music's form, which tags and words went by. An id names one copy, so one
-- record, where the same tags on other tracks make a base name two. `folder`: the copy's, for an `id`. `rank`: the order rows
-- that meet on one record go in, main libraries first.
CREATE TABLE IF NOT EXISTS album_moved (
  old TEXT NOT NULL, form TEXT NOT NULL, record INTEGER NOT NULL, folder INTEGER, rank INTEGER NOT NULL,
  PRIMARY KEY (old, form, record)) WITHOUT ROWID;
CREATE UNIQUE INDEX IF NOT EXISTS album_moved_one_record ON album_moved (old) WHERE form = 'id';
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

/// The tables whose ids are for life and held in user data: what a fresh start of lost ids lifts. Labels join when they have
/// ids of their own.
const IDS_FOR_LIFE: [&str; 3] = ["tracks", "records", "artists"];

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

/// How many of a folder's files `set_folders` looks for at a path, to tell whether those rows are that folder's.
const SAMPLE: i64 = 20;

/// The meta key of `Catalogue::generation`.
const GENERATION: &str = "generation";

/// How long a gone file waits to be recognised as moved before its track id is retired (decision 0011 §4).
const GONE_KEEP_SECS: i64 = 24 * 3600;

/// Every table holding a row per file, by folder and path: what a move, a parked folder or a retirement takes along. A table with
/// both goes here or says why not (`every_table_by_folder_and_path_goes_with_its_file`), or it stays at a place its file left.
const FILE_TABLES: [&str; 7] = ["files", "cover_files", "tags", "source_ids", "analysis", "analysis_shape", "beat_this"];

/// What a retiring track was, from its file and tags while they are still there (`tracks`' kept columns).
const REMEMBER: &str = "was_rel = (SELECT rel FROM files WHERE track = tracks.id LIMIT 1),
  was_ms = (SELECT duration_ms FROM files WHERE track = tracks.id LIMIT 1),
  was_title = (SELECT t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel WHERE f.track = tracks.id AND t.name = 'title' ORDER BY t.n LIMIT 1),
  was_artist = (SELECT t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel WHERE f.track = tracks.id AND t.name = 'artist' ORDER BY t.n LIMIT 1),
  was_album = (SELECT t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel WHERE f.track = tracks.id AND t.name = 'album' ORDER BY t.n LIMIT 1),
  was_recording = (SELECT t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel WHERE f.track = tracks.id AND t.name = 'musicbrainz_trackid' ORDER BY t.n LIMIT 1)";

/// Whether a folder shows nothing, or is not there: how a mount that is not up looks, and no sign its files are gone.
fn holds_nothing(path: &str) -> bool {
    std::fs::read_dir(path).map_or(true, |mut d| !d.any(|e| e.is_ok_and(|e| !e.file_name().to_string_lossy().starts_with('.'))))
}

/// An album's files in one folder as the library groups them: what [`Catalogue::assign_records`] gives a record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlbumCopy {
    pub facts: identity::RecordFacts,
    /// As shown, for the record's own facts.
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    /// In the order of `facts.tracks`: (track id, disc, number).
    pub tracks: Vec<(i64, Option<u32>, Option<u32>)>,
    /// Where each of `tracks` lies: (catalogue folder, path there).
    pub files: Vec<Place>,
    /// Worth `assign_records`' reading: a track free to move holds another record or none, or albums of one record hold
    /// different tracks.
    pub split: bool,
    /// Who keeps a record of two copies that differ, the higher first (`copies::keeping`, CR18). All alike: the track count alone.
    pub rank: u8,
    /// The catalogue folder it lies in, and what 0.15 knew it by: its tag hash there and in Music's form, and the persistent
    /// id both are the hash of. What the first start's map is made of (`Catalogue::first_records`).
    pub folder: i64,
    pub old_id: String,
    pub old_base: String,
    pub pid: String,
}

/// What one file of an album says of its record ([`vote_of`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Say {
    pub record: Option<i64>,
    /// Its track has no other file, so it goes where its album goes.
    pub free: bool,
    /// Its track is back from retirement, and not yet held against the album it landed in.
    pub back: bool,
}

/// The record more than half of these hold: one held track says nothing of the other seventeen. A track back from
/// retirement was matched by its recording or its name, which says nothing of the album its file landed in. So where none
/// beside it stayed, it takes more than half of the record's own tracks (`size`) to bring the record back.
fn most_of(said: &[Say], size: &dyn Fn(i64) -> usize) -> Option<i64> {
    let mut votes: BTreeMap<i64, (usize, bool)> = BTreeMap::new();
    for s in said {
        if let Some(record) = s.record {
            let vote = votes.entry(record).or_default();
            vote.0 += 1;
            vote.1 |= !s.back;
        }
    }
    let (record, (n, stayed)) = votes.into_iter().max_by_key(|(r, (n, _))| (*n, std::cmp::Reverse(*r)))?;
    (n * 2 > said.len() && (stayed || n * 2 > size(record))).then_some(record)
}

/// The record an album's files say it is, if they do: the one vote, read by `assign_records` and by the library as it
/// builds, so what a list serves an album as is what the catalogue gave it. Failing all of them, the ones free to move: a
/// track shared with a copy elsewhere stays with its record, so without this an album holding such tracks would never
/// settle, a new record born for it at every pass.
pub fn vote_of(said: &[Say], size: &dyn Fn(i64) -> usize) -> Option<i64> {
    let free: Vec<Say> = said.iter().copied().filter(|s| s.free).collect();
    most_of(said, size).or_else(|| most_of(&free, size))
}

/// A pass's take (docs/records-and-copies.md: a copy takes the record's track ids, track by track): the tracks found to be
/// one, each to the one that stays. Read through `now`, as what took a track can itself be taken by a third copy.
#[derive(Clone, Default)]
struct Take {
    to: HashMap<i64, i64>,
}

impl Take {
    fn now(&self, mut track: i64) -> i64 {
        while let Some(to) = self.to.get(&track) {
            track = *to;
        }
        track
    }

    fn all(&self, tracks: &[i64]) -> std::collections::BTreeSet<i64> {
        tracks.iter().map(|t| self.now(*t)).collect()
    }

    /// `ours` and `theirs` are one track of `record`. The one holding the record stays, the older of two that do, as what is
    /// known by an id should stay known by it. Of two that do not, `theirs`: the album it is held against came first. A track
    /// of another record with a file elsewhere stays out of it: its name says nothing of that file, and a take cannot be undone.
    fn pair(&mut self, held: &Held, record: i64, ours: i64, theirs: i64) {
        let (ours, theirs) = (self.now(ours), self.now(theirs));
        let kept_elsewhere = |t: i64| held.shared.contains(&t) && held.record.get(&t).is_some_and(|r| *r != record);
        if ours == theirs || kept_elsewhere(ours) || kept_elsewhere(theirs) {
            return;
        }
        let holds = |t: i64| held.record.get(&t) == Some(&record);
        let (stays, goes) = match (holds(ours), holds(theirs)) {
            (true, true) => (ours.min(theirs), ours.max(theirs)),
            (true, false) => (ours, theirs),
            _ => (theirs, ours),
        };
        self.to.insert(goes, stays);
    }

    /// Two albums that are the same tracks, place by place, `same_tracks`' order. A track either side holds at another place
    /// stays out of it: pairing it would make two tracks of the record one.
    fn place_by_place(&mut self, held: &Held, record: i64, ours: &[i64], theirs: &[i64]) {
        let (mine, other) = (self.all(ours), self.all(theirs));
        for (ours, theirs) in ours.iter().zip(theirs) {
            let (ours, theirs) = (self.now(*ours), self.now(*theirs));
            if !other.contains(&ours) && !mine.contains(&theirs) {
                self.pair(held, record, ours, theirs);
            }
        }
    }

    /// N3: of two albums not the same tracks as they stand, a file replaced outside Cratarr in one has a fresh id, while its
    /// track has no file there. By name and length, whatever the order, and only where each fits the other alone.
    fn by_name(&mut self, held: &Held, record: i64, ours: (&[i64], &[identity::Listed]), theirs: (&[i64], &[identity::Listed])) {
        let (mine, other) = (self.all(ours.0), self.all(theirs.0));
        let fresh: Vec<(i64, &identity::Listed)> = ours.0.iter().zip(ours.1).map(|(t, l)| (self.now(*t), l)).filter(|(t, _)| !other.contains(t) && !held.shared.contains(t)).collect();
        let lacking: Vec<(i64, &identity::Listed)> = theirs.0.iter().zip(theirs.1).map(|(t, l)| (self.now(*t), l)).filter(|(t, _)| !mine.contains(t)).collect();
        for (ours, name) in &fresh {
            let fits: Vec<i64> = lacking.iter().filter(|(_, l)| name.same_track(l)).map(|(t, _)| *t).collect();
            if let [theirs] = fits[..]
                && fresh.iter().filter(|(_, l)| lacking.iter().any(|(t, other)| *t == theirs && l.same_track(other))).count() == 1
            {
                self.pair(held, record, *ours, theirs);
            }
        }
    }
}

/// What the tracks hold, as an album's are counted ([`Held::vote`]).
#[derive(Default)]
struct Held {
    /// Each track's record.
    record: HashMap<i64, i64>,
    /// How many tracks a record has, retired ones too.
    size: HashMap<i64, usize>,
    /// How many files of each record are here now: what says whether anything but one album holds it.
    files: HashMap<i64, usize>,
    /// How many of them lie in albums of each rank (`ranked`).
    in_rank: HashMap<(i64, u8), usize>,
    /// Tracks with more than one file: they stay with their record, whichever album one of them sits in.
    shared: std::collections::HashSet<i64>,
    /// Tracks back from retirement, not yet held against their albums.
    back: std::collections::HashSet<i64>,
    /// Where each track has a file gone a while, or there and not read, by track so a let-go goes the same way every time.
    gone: BTreeMap<i64, Vec<Place>>,
}

impl Held {
    fn read(db: &Connection) -> Result<Self> {
        let mut held = Held::default();
        let mut st = db.prepare("SELECT id, record, back IS NOT NULL FROM tracks WHERE record IS NOT NULL OR back IS NOT NULL")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, bool>(2)?)))? {
            let (track, record, back) = row?;
            if let Some(record) = record {
                held.record.insert(track, record);
                *held.size.entry(record).or_default() += 1;
            }
            if back {
                held.back.insert(track);
            }
        }
        let mut st = db.prepare("SELECT track FROM files WHERE track IS NOT NULL GROUP BY track HAVING count(*) > 1")?;
        held.shared = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare(&format!("SELECT t.record, count(*) FROM files f JOIN tracks t ON t.id = f.track WHERE t.record IS NOT NULL AND {PRESENT_F} AND {} GROUP BY t.record", served("f.")))?;
        held.files = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare(&format!("SELECT track, folder, rel FROM files WHERE track IS NOT NULL AND (gone_at IS NOT NULL OR ({PRESENT} AND NOT {}))", served("")))?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))? {
            let (track, folder, rel) = row?;
            held.gone.entry(track).or_default().push((folder, rel));
        }
        Ok(held)
    }

    /// Whether `album` lacks these tracks only for now: each has a file gone a while from where the album lies, or there and
    /// not read. A mount that blinks must not split a record: a gone file comes back, or goes for good within the day, and one
    /// that could not be read is read again when it changes, however long that takes. By the library, not the folder: a disc
    /// gone whole leaves no file of the album in its folder.
    fn lacks_for_now(&self, album: &AlbumCopy, tracks: &[i64]) -> bool {
        let lies = |place: &Place| album.files.iter().any(|(folder, _)| *folder == place.0);
        tracks.iter().all(|t| self.gone.get(t).is_some_and(|places| places.iter().any(lies)))
    }

    /// The record nothing but these tracks' album holds: every file of it here now is one of them, and one beside them
    /// stayed, as a track back from retirement says nothing of where it landed. Seven files added outside Cratarr to three of
    /// a record's are no majority for it, and without this it would go, and what names it with it. Of two such, the one it
    /// holds most of, the lowest on a tie. A file each, as `AlbumCopy::tracks` lists them. Files in albums of a lower rank do
    /// not count: a copy that gives way to this one does not stop it being alone (CR18).
    fn alone(&self, tracks: &[i64], rank: u8) -> Option<i64> {
        let mut held: BTreeMap<i64, (usize, bool)> = BTreeMap::new();
        for track in tracks {
            if let Some(record) = self.record.get(track) {
                let vote = held.entry(*record).or_default();
                vote.0 += 1;
                vote.1 |= !self.back.contains(track);
            }
        }
        let here = |record: i64| {
            let below: usize = (0..rank).filter_map(|r| self.in_rank.get(&(record, r))).sum();
            self.files.get(&record).map(|n| n.saturating_sub(below))
        };
        held.into_iter().filter(|(record, (n, stayed))| *stayed && here(*record) == Some(*n)).max_by_key(|(r, (n, _))| (*n, std::cmp::Reverse(*r))).map(|(r, _)| r)
    }

    /// Where these albums' files lie, by each one's rank.
    fn ranked(&mut self, albums: &[AlbumCopy]) {
        for a in albums {
            for record in a.tracks.iter().filter_map(|t| self.record.get(&t.0)) {
                *self.in_rank.entry((*record, a.rank)).or_default() += 1;
            }
        }
    }

    /// The record an album's tracks say it is, if they do ([`vote_of`]). Failing that, the record that has no other album to
    /// be in, as what was added beside it can be more than it had: all but that is the library's vote too, as once this has
    /// settled such an album its tracks are unanimous.
    fn vote(&self, album: &AlbumCopy) -> Option<i64> {
        let tracks: Vec<i64> = album.tracks.iter().map(|t| t.0).collect();
        let said: Vec<Say> = tracks.iter().map(|t| Say { record: self.record.get(t).copied(), free: !self.shared.contains(t), back: self.back.contains(t) }).collect();
        vote_of(&said, &|record| self.size.get(&record).copied().unwrap_or(0)).or_else(|| self.alone(&tracks, album.rank))
    }
}

/// What a track is or was ([`Catalogue::remembered`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remembered {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// MusicBrainz recording id.
    pub recording: Option<String>,
    pub duration_ms: Option<i64>,
    /// Retired: what it was when its file was last seen.
    pub retired: bool,
}

/// What a track id is now: the track that took it (`track_moved`), else itself. An Undo from before a take, an export or a
/// queue names ids that went, and each must reach the record's track, or the record splits again.
fn taken_by(db: &Connection, track: i64) -> rusqlite::Result<i64> {
    Ok(db.prepare_cached("SELECT id FROM track_moved WHERE old = ?1")?.query_row([track], |r| r.get(0)).optional()?.unwrap_or(track))
}

/// A file taking the place of `from` (moved outside Cratarr, or a better copy handed its track) came when that one did:
/// to the copy it is the same file. It keeps its own date where `from` has none.
fn carry_came(db: &Connection, to: (i64, &str), from: (i64, &str)) -> rusqlite::Result<usize> {
    db.execute(
        "UPDATE files SET came = COALESCE((SELECT came FROM files WHERE folder = ?3 AND rel = ?4), came) WHERE folder = ?1 AND rel = ?2",
        params![to.0, to.1, from.0, from.1],
    )
}

/// The retired tracks that share a MusicBrainz recording, a file name, or a title and artist with any of `new`, with
/// what they were as identity's facts (folded).
fn retired_like(db: &Connection, new: &[identity::FileFacts]) -> Result<Vec<(i64, identity::FileFacts)>> {
    let recordings: std::collections::HashSet<&str> = new.iter().filter_map(|f| f.recording.as_deref()).collect();
    let names: std::collections::HashSet<&str> = new.iter().map(|f| f.name.as_str()).filter(|n| !n.is_empty()).collect();
    let titled: std::collections::HashSet<(&str, &str)> =
        new.iter().filter_map(|f| Some((f.title.as_deref()?, f.artist.as_deref()?))).collect();
    // A track taken is the record's track now: back, it would split what the take made one (N2).
    let mut st = db.prepare("SELECT id, was_rel, was_ms, was_recording, was_title, was_artist FROM tracks WHERE retired_at IS NOT NULL AND id NOT IN (SELECT old FROM track_moved)")?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, rel, duration_ms, recording, title, artist) = row?;
        let fold = |v: Option<String>| v.map(|v| identity::fold(&v)).filter(|v| !v.is_empty());
        let f = identity::FileFacts {
            name: rel.as_deref().map(identity::name_of).unwrap_or_default(),
            duration_ms,
            recording: fold(recording),
            title: fold(title),
            artist: fold(artist),
            ..Default::default()
        };
        let shares = f.recording.as_deref().is_some_and(|r| recordings.contains(r))
            || (!f.name.is_empty() && names.contains(f.name.as_str()))
            || matches!((f.title.as_deref(), f.artist.as_deref()), (Some(t), Some(a)) if titled.contains(&(t, a)));
        if shares {
            out.push((id, f));
        }
    }
    Ok(out)
}

/// Columns added after the first schema; added to an existing database when missing.
const ADDED_COLUMNS: [(&str, &str); 30] = [
    // 0100: the record a track is of, and its place there.
    ("tracks", "record INTEGER"),
    ("tracks", "disc INTEGER"),
    ("tracks", "number INTEGER"),
    // A retired track a file took back, until it has been held against the album that file is in (`Held::most`).
    ("tracks", "back INTEGER"),
    // 0101 point 1 (the owner, 2026-11-04): a folder's id for life, kept while it is parked under a negative id, so it
    // comes back as itself (`set_folders`).
    ("folders", "home INTEGER"),
    // The path a library left, until what names it by its path has followed (`Catalogue::paths_to_follow`).
    ("folders", "was TEXT"),
    // 0101 point 11: what a track was, kept as it retires (its tags go with its file): a missing entry shows it, and a
    // file that is it again takes the id back. Its last path, its length, and its title, artist, album and MusicBrainz
    // recording as tagged.
    ("tracks", "was_rel TEXT"),
    ("tracks", "was_ms INTEGER"),
    ("tracks", "was_title TEXT"),
    ("tracks", "was_artist TEXT"),
    ("tracks", "was_album TEXT"),
    ("tracks", "was_recording TEXT"),
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
    // Rhythm (0.15): four on the floor, 0 to 1 (`analysis::rhythm`; NULL: not measured yet).
    ("analysis", "four REAL"),
    // The time signature from Beat this!'s downbeats: beats a bar (0: none sure), and how sure; NULL: not worked out yet.
    ("beat_this", "meter INTEGER"),
    ("beat_this", "meter_sure REAL"),
    // What a file handed a track replaces, in the same folder: the one gone row that lets go of it.
    ("handovers", "replaces TEXT"),
    // A file's bit depth and sample rate (0.16, reader 9): what ranks one lossless copy above another. NULL: lossy, or not
    // read since they were kept, which ranks it last until the reader gets there.
    ("files", "bits INTEGER"),
    ("files", "rate INTEGER"),
    // When the file first came (docs/records-and-copies.md): a copy keeps its own date, where `tracks.born` is the track's.
    ("files", "came INTEGER"),
];

/// The crates' facts as last read (`Catalogue::crate_facts`), and what they were read from.
struct KeptFacts {
    written: u64,
    folders: InUse,
    built: i64,
    words: i64,
    /// The classical marks as read (0.10): a mark reads them again at once.
    marks: i64,
    at: std::time::Instant,
    facts: Arc<Vec<crate::crates::Facts>>,
}

/// The crates' facts are read again for a write to what they are read from or a harvested word at most this often (a
/// build, at once).
const FACTS_EVERY: Duration = Duration::from_secs(300);

/// What the crates' facts take from elsewhere that no build brings them, as one stamp: the library's words (0.9.7,
/// `app::words::stamp`), what Classical is made of (0.10), and Discogs' labels, which were only ever seen because the
/// connection's change count moved all the same.
fn said_stamp(s: &crate::serve::Serve) -> i64 {
    let said = crate::app::words::stamp(s).wrapping_mul(31).wrapping_add(crate::app::classical::stamp(s));
    said.wrapping_mul(31).wrapping_add(s.userdata.discogs_albums_stamp().unwrap_or(0))
}

/// What the crates' facts are read from (`read_crate_facts`): a table read there goes here too, or a write to it shows
/// only at the next build.
const FACTS_TABLES: [&str; 6] = ["files", "tags", "tracks", "analysis", "beat_this", "audiomuse"];

/// Each row written to those tables counted in `written`, whoever writes it through this connection: temporary
/// triggers, so nothing changes on disk and no writer has to remember. The connection's change count could not be the
/// stamp: every pass writes its time and a track first played its shape, so the facts, with every AudioMuse vector in
/// them, were read again every five minutes on a library where nothing had changed.
fn count_facts_writes(conn: &Connection, written: Arc<AtomicU64>) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function("facts_written", 0, FunctionFlags::SQLITE_UTF8, move |_| {
        written.fetch_add(1, Ordering::Relaxed);
        Ok(0)
    })?;
    for table in FACTS_TABLES {
        for on in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TEMP TRIGGER facts_{table}_{on} AFTER {on} ON main.{table} BEGIN SELECT facts_written(); END;"))?;
        }
    }
    Ok(())
}

/// A file's place in the catalogue: its folder and path there.
type Place = (i64, String);

/// What the catalogue knows of and holds no file for, for Health's Missing list: nothing here is forgotten by itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Missing {
    pub parked: Vec<ParkedLibrary>,
    /// Tracks whose every file is gone a while, each by one of them.
    pub gone: Vec<MissingTrack>,
    /// Tracks retired, nothing having taken them, the latest first.
    pub retired: Vec<MissingTrack>,
}

/// A library out of use, its rows kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkedLibrary {
    pub path: String,
    /// Epoch seconds; none for a library parked before that was kept.
    pub since: Option<i64>,
    pub files: u64,
}

/// A track with no file to play.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingTrack {
    pub track: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Where it last lay, in its library.
    pub rel: Option<String>,
    /// Epoch seconds.
    pub since: i64,
}

/// `goes` is `stays` for good, every file of it and all that follows it (`track_moved`). In the caller's transaction, which has
/// retired `goes`.
fn taken_for_good(tx: &Connection, goes: i64, stays: i64) -> Result<()> {
    tx.execute("UPDATE files SET track = ?2 WHERE track = ?1", params![goes, stays])?;
    tx.execute("UPDATE track_moved SET id = ?2 WHERE id = ?1", params![goes, stays])?;
    tx.execute("INSERT OR REPLACE INTO track_moved (old, id, followed) VALUES (?1, ?2, 0)", params![goes, stays])?;
    // A link names one file a track: the next pass makes the one that stays its own.
    tx.execute("DELETE FROM source_ids WHERE source = ?1 AND track_id = ?2", params![crate::source::cratarr::KIND.id, crate::serve::ids::song(goes)])?;
    tx.execute("UPDATE OR IGNORE audiomuse SET track = ?2 WHERE track = ?1", params![goes, stays])?;
    tx.execute("DELETE FROM audiomuse WHERE track = ?1", [goes])?;
    Ok(())
}

/// A file as `identify` reads it, new or gone.
struct Identified {
    folder: i64,
    rel: String,
    duration_ms: Option<i64>,
    track: Option<i64>,
    size: i64,
    mtime: i64,
}

/// The tracks with more than one file present: a record's copies. Few, so reading them first costs next to nothing.
fn tracks_of_several(db: &Connection) -> Result<Vec<i64>> {
    let mut st = db.prepare(&format!("SELECT track FROM files WHERE track IS NOT NULL AND {PRESENT} GROUP BY track HAVING count(*) > 1"))?;
    let tracks = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(tracks)
}

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
    /// Bit depth and sample rate (Hz), once reader 9 has read the file.
    pub bits: Option<u32>,
    pub rate: Option<u32>,
    /// When the file first came, epoch seconds; `born` is the track's.
    pub came: Option<i64>,
    /// The record its track is of (0100), once it has one.
    pub record: Option<i64>,
    /// Its track has another file, here or gone a while: a copy's, so it stays with its record ([`vote_of`]).
    pub shared: bool,
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
    pub const TAGS: [&'static str; 27] = [
        "title",
        "artist",
        "artists",
        "albumartist",
        "albumartists",
        "album",
        "tracknumber",
        // The record's track count its tags say (0.14, Health's missing tracks without a reference).
        "totaltracks",
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

/// The folders in use, by their ids for life (0101 point 1, the owner 2026-11-04): a folder keeps its id while it is
/// used and takes it again when it comes back; a new one takes an id never given before. So nothing that names a folder
/// by its id (served album ids, Subsonic's music folders, the journal, queued conversions) changes when libraries come
/// and go. The lowest id in use is the folder playlist lines are made from (Music's, 0, since the first start).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InUse(std::collections::BTreeMap<i64, PathBuf>);

impl InUse {
    /// A folder by its id.
    pub fn get(&self, id: i64) -> Option<&PathBuf> {
        self.0.get(&id)
    }

    /// Each folder with its id, by id.
    pub fn iter(&self) -> impl Iterator<Item = (i64, &PathBuf)> {
        self.0.iter().map(|(id, p)| (*id, p))
    }

    /// The folders' ids, in order.
    pub fn ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.0.keys().copied()
    }

    /// The folders' paths, by id.
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.0.values()
    }

    /// The id of the folder at `path`.
    pub fn id_of(&self, path: &Path) -> Option<i64> {
        self.0.iter().find(|(_, p)| p.as_path() == path).map(|(id, _)| *id)
    }

    /// The folder playlist lines are made from, and its id: the lowest in use.
    pub fn first(&self) -> Option<(i64, &PathBuf)> {
        self.0.iter().next().map(|(id, p)| (*id, p))
    }
}

impl FromIterator<(i64, PathBuf)> for InUse {
    fn from_iter<T: IntoIterator<Item = (i64, PathBuf)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// The meta key of the highest folder id ever given (`set_folders`).
const FOLDER_TOP: &str = "folder_top";

pub struct Catalogue {
    db: Mutex<Connection>,
    /// The folders in use, by their ids for life.
    folders: RwLock<InUse>,
    progress: Mutex<Progress>,
    /// Asks the background loop for a pass now (settings changed, a playlist was edited…).
    poke: tokio::sync::Notify,
    /// Told when files were read again outside a pass (`reread`): the library is built from them at once.
    reread: tokio::sync::Notify,
    /// How much the passes have done since the server started (files read, folders walked, links made): what shows a
    /// long pass moving (0.9.7: a reader's reread of every file takes hours on the NAS, and Cratarr's own library
    /// looked stopped to the watch on the servers' scans).
    worked: std::sync::atomic::AtomicU64,
    /// The crates' facts, as of what was written to them and the folders in use (see `crate_facts`).
    facts: Mutex<Option<KeptFacts>>,
    /// Rows written to what the crates' facts are read from (`count_facts_writes`).
    facts_written: Arc<AtomicU64>,
    /// Its track ids lost (0101, the owner 2026-11-04: Cratarr stops and says so): a catalogue made anew while the user
    /// data remembers another. No ids are given out meanwhile (`identify`).
    ids_lost: std::sync::atomic::AtomicBool,
    /// How many album actions are moving a copy's files now (`copies_moving`).
    moving: Arc<AtomicU64>,
    /// One was when the albums `assign_records` is about to be given were read (`reading_albums`).
    read_half_way: std::sync::atomic::AtomicBool,
    /// The database file, for readers with their own connection (`reader`); `None` in memory.
    file: Option<PathBuf>,
}

/// An album action moving a copy's files, for as long as one of these lives ([`Catalogue::copies_moving`]). A clone goes
/// with the blocking move: a request dropped half way ends the action's future, not its move.
pub struct CopiesMoving(Arc<AtomicU64>);

impl Clone for CopiesMoving {
    fn clone(&self) -> Self {
        self.0.fetch_add(1, Ordering::SeqCst);
        Self(self.0.clone())
    }
}

impl Drop for CopiesMoving {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
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
        conn.execute_batch("CREATE INDEX IF NOT EXISTS files_by_track ON files (track); CREATE INDEX IF NOT EXISTS tracks_by_record ON tracks (record);")?;
        // A folder in use before ids for life: its place is its id from now on.
        conn.execute_batch("UPDATE folders SET home = id WHERE id >= 0 AND home IS NULL;")?;
        // Once, before any copy takes another's track: a file came when its track was born, the one date there was. After
        // that every row carries its own, and a date from a track would be another copy's.
        let came: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'came'", [], |r| r.get(0)).optional()?;
        if came.is_none() {
            conn.execute("UPDATE files SET came = COALESCE((SELECT born FROM tracks WHERE id = files.track), ?1) WHERE came IS NULL", [now()])?;
            conn.execute_batch("INSERT OR REPLACE INTO meta (key, value) VALUES ('came', 'filled');")?;
        }
        fill_meters(&conn).context("working out the time signatures")?;
        // Once: the tags no query reads go, the unnamed ones (lyrics, pictures' text: 63,000 rows of 455,000 here) and the
        // index by name and value (73 MB of a 200 MB catalogue), so what is read fits in a small container's memory.
        // Their pages are reused by what is written next.
        conn.execute_batch("DROP INDEX IF EXISTS tags_by_name;")?;
        let slim: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'tags'", [], |r| r.get(0)).optional()?;
        if slim.as_deref() != Some("named") {
            conn.execute_batch("DELETE FROM tags WHERE name IS NULL; INSERT OR REPLACE INTO meta (key, value) VALUES ('tags', 'named');")?;
        }
        move_analysis_shapes(&conn)?;
        index_log::start(&conn).context("starting the library's change log")?;
        let facts_written = Arc::new(AtomicU64::new(0));
        count_facts_writes(&conn, facts_written.clone())?;
        let cat = Self {
            db: Mutex::new(conn),
            folders: RwLock::new(InUse::default()),
            progress: Mutex::default(),
            poke: Default::default(),
            reread: Default::default(),
            worked: Default::default(),
            facts: Mutex::default(),
            facts_written,
            ids_lost: Default::default(),
            moving: Default::default(),
            read_half_way: Default::default(),
            file: file.map(Path::to_path_buf),
        };
        // A better reader may read what an older one could not.
        if cat.meta("reader").as_deref() != Some(&tags::READER_VERSION.to_string()) {
            // A row with nothing read has no format to go by, or it is served as read, with no tags, until it is read.
            cat.db().execute("UPDATE files SET state = ?1, format = NULL WHERE state = ?2", params![PENDING, FAILED])?;
            // Reader 5 names ID3v2's TBPM and MP4's tmpo "bpm" as it already named Vorbis's BPM: the tags read before.
            cat.db().execute("UPDATE tags SET name = 'bpm' WHERE raw IN ('TBPM', 'tmpo') AND name IS NULL", [])?;
            // Reader 6 names a file's mood, style and grouping (0.9.7, the library's words), which no reader before kept:
            // every file read before is read again, and served as it was read until then.
            // Reader 7 names the release type (0.13, written into files by decision 0092) and reads the fields lofty has
            // no key for (STYLE, unread since lofty 0.25): every file read again too.
            if cat.meta("reader").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v < 7) {
                cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2", params![PENDING, READ])?;
            }
            // Reader 8 checks a FLAC's seek table (0.14, a broken one ended tracks on a far seek): the FLACs read again.
            if cat.meta("reader").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v < 8) {
                cat.db().execute("UPDATE files SET state = ?1 WHERE state = ?2 AND format = 'FLAC'", params![PENDING, READ])?;
            }
            // Reader 9 keeps bit depth and rate (0.16): only lossless copies rank by them, so only they are read again (an
            // MP4 at a lossless rate is ALAC, as `export::kind_guessed` tells it). Served as read until then.
            if cat.meta("reader").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v < 9) {
                cat.db().execute(
                    "UPDATE files SET state = ?1 WHERE state = ?2 AND (format IN ('FLAC', 'WAV', 'AIFF', 'APE', 'WAVPACK') OR (format = 'MP4' AND bitrate >= 500))",
                    params![PENDING, READ],
                )?;
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

    /// Whether its track ids were lost (`set_ids_lost`): no ids are given out until it is restored or started afresh.
    pub fn ids_lost(&self) -> bool {
        self.ids_lost.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_ids_lost(&self, lost: bool) {
        self.ids_lost.store(lost, std::sync::atomic::Ordering::Relaxed);
    }

    /// The highest id it gave out, of any kind in `IDS_FOR_LIFE`. Blocking.
    pub fn top_id(&self) -> i64 {
        let db = self.db();
        let top = |table: &str| {
            let seq: Option<i64> = db.query_row("SELECT seq FROM sqlite_sequence WHERE name = ?1", [table], |r| r.get(0)).optional().ok().flatten();
            let max: Option<i64> = db.query_row(&format!("SELECT MAX(id) FROM {table}"), [], |r| r.get(0)).ok().flatten();
            seq.unwrap_or(0).max(max.unwrap_or(0))
        };
        IDS_FOR_LIFE.iter().map(|t| top(t)).max().unwrap_or(0)
    }

    /// Its next ids of every kind numbered above `floor` (a fresh start after lost ids: an old id then names something
    /// missing, never something else, and `al-12` in a favourite must not quietly become another album). The maps of ids
    /// that moved are kept in this same file, so a lost catalogue takes them with it: an old id has nowhere left to go,
    /// which is why it must name nothing. Blocking.
    pub fn number_above(&self, floor: i64) -> Result<()> {
        let db = self.db();
        for table in IDS_FOR_LIFE {
            if db.execute("UPDATE sqlite_sequence SET seq = MAX(seq, ?1) WHERE name = ?2", params![floor, table])? == 0 {
                db.execute("INSERT INTO sqlite_sequence (name, seq) VALUES (?1, ?2)", params![table, floor])?;
            }
        }
        Ok(())
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn folders(&self) -> InUse {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// How many tracks each folder in use holds (decision 0044: a folder's row), with its path.
    pub fn tracks_per_folder(&self) -> Result<Vec<(PathBuf, u64)>> {
        let counts: std::collections::HashMap<i64, u64> = {
            let db = self.db();
            let mut q = db.prepare(&format!("SELECT folder, count(*) FROM files WHERE {PRESENT} GROUP BY folder"))?;
            q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64)))?.collect::<rusqlite::Result<_>>()?
        };
        Ok(self.folders().iter().map(|(id, p)| (p.clone(), counts.get(&id).copied().unwrap_or(0))).collect())
    }

    /// Use these folders, each by its id for life (0101 point 1, the owner 2026-11-04): a folder in use keeps its id, one
    /// that comes back takes its own id again, and a new one takes an id never given before (`InUse`). A folder whose
    /// path changed keeps its id and rows when the same files are there under the new path (a mount moved, say `/music`
    /// to `/media/Music`), and nothing is read again: named in the same breath, or after a start found it not there and parked
    /// it.
    ///
    /// A folder that is no longer in use is parked, not forgotten: its rows (files, tags, links to the servers' ids)
    /// move to a negative id and keep their track ids, and nothing counts them while parked. Back in use, with its files
    /// where they were, it picks them up again under its own id: the same track ids, only changed files read again. A
    /// source switched off and on (2026-10-15) cost Incoming its links and its track ids before this. A parked folder is
    /// forgotten only when other files are found at its path, never for its age: what it holds is listeners', and how long a
    /// drive stays in a drawer says nothing of whether it comes back. "The same files" is more than half
    /// of a sample spread over the folder: one album gone while it was away forgets nothing (rows go one by one, once the
    /// walk misses them), and nothing at all there is no other files, but a mount not up (`holds_nothing`).
    pub fn set_folders(&self, folders: Vec<PathBuf>) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut wanted: Vec<String> = Vec::new();
        for p in &folders {
            let p = p.to_string_lossy().into_owned();
            if !wanted.contains(&p) {
                wanted.push(p);
            }
        }
        // Spread, as a folder's first rows are its first album: that album gone must not forget a folder, and that album
        // alike at another path must not hand a folder over. The moved mount's case asks no more than half: a mount moved
        // with a few files lost must keep its ids, and two folders alike in more than half of an even sample are one.
        let same_files = |id: i64, at: &str| -> Result<bool> {
            let mut st = tx.prepare(
                "SELECT rel FROM (SELECT rel, row_number() OVER (ORDER BY rel) - 1 AS n, count(*) OVER () AS total
                                  FROM files WHERE folder = ?1 AND gone_at IS NULL AND trashed IS NULL)
                 WHERE n % ((total + ?2 - 1) / ?2) = 0",
            )?;
            let rels: Vec<String> = st.query_map(params![id, SAMPLE], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            let there = rels.iter().filter(|rel| Path::new(at).join(rel).is_file()).count();
            Ok(there * 2 > rels.len())
        };
        let rekey = |from: i64, to: i64| -> Result<()> {
            for table in FILE_TABLES {
                tx.execute(&format!("UPDATE {table} SET folder = ?2 WHERE folder = ?1"), params![from, to])?;
            }
            tx.execute("UPDATE folders SET id = ?2 WHERE id = ?1", params![from, to])?;
            Ok(())
        };
        // Each folder known: its id now (negative: parked), its path, its id for life.
        let known = |tx: &rusqlite::Transaction| -> Result<Vec<(i64, String, Option<i64>)>> {
            let mut st = tx.prepare("SELECT id, path, home FROM folders")?;
            Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?)
        };
        let now_s = now();
        // The highest id ever given: a new folder's is above it, so no id names two folders over time.
        let given: Option<i64> = tx.query_row("SELECT value FROM meta WHERE key = ?1", [FOLDER_TOP], |r| r.get::<_, String>(0)).optional()?.and_then(|v| v.parse().ok());
        let mut top = known(&tx)?.iter().flat_map(|(id, _, home)| [Some(*id), *home]).flatten().chain(given).max().unwrap_or(-1);
        // 1. Every folder in use that is no longer wanted steps aside (parked), its id for life kept.
        let mut lowest = known(&tx)?.iter().map(|(id, _, _)| *id).min().unwrap_or(0).min(0);
        let mut stepped: Vec<i64> = Vec::new();
        for (id, path, _) in known(&tx)? {
            if id < 0 || wanted.contains(&path) {
                continue;
            }
            lowest -= 1;
            rekey(id, lowest)?;
            stepped.push(lowest);
        }
        // 2. Each wanted folder not in use takes the rows kept under its path (parked a while, or only just stepped
        //    aside), when its files are still there; else the rows of a folder that stepped aside, or was parked with nothing
        //    left at its path, and whose files are here now (a mount that moved). Either comes back under its own id; anything
        //    else is new, with a new id.
        let mut forget: Vec<i64> = Vec::new();
        for path in &wanted {
            let rows = known(&tx)?;
            if rows.iter().any(|(id, p, _)| *id >= 0 && p == path) {
                continue;
            }
            let own = rows.iter().find(|(id, p, _)| *id < 0 && p == path).map(|(id, _, _)| *id);
            let taken = match own {
                Some(p) if same_files(p, path)? || holds_nothing(path) => Some(p),
                Some(p) => {
                    forget.push(p);
                    None
                }
                None => None,
            };
            let taken = match taken {
                Some(p) => Some(p),
                None => {
                    // A mount is changed first: a start finds its library not there and parks it, and Settings names the new
                    // place after. Where the parked folder still holds its files, what is here is a copy of it, a folder of its own.
                    let left = rows.iter().filter(|(id, q, _)| *id < 0 && !stepped.contains(id) && holds_nothing(q)).map(|(id, _, _)| *id);
                    let mut moved = None;
                    for p in stepped.iter().copied().chain(left) {
                        let unclaimed = rows.iter().any(|(id, q, _)| *id == p && !wanted.contains(q));
                        if unclaimed && same_files(p, path)? {
                            moved = Some(p);
                            break;
                        }
                    }
                    moved
                }
            };
            // Its own id again where it has one that no folder in use holds (a folder parked before ids for life has
            // none); else a new one.
            let home = taken
                .and_then(|p| rows.iter().find(|(id, _, _)| *id == p).and_then(|(_, _, home)| *home))
                .filter(|h| *h >= 0 && !rows.iter().any(|(id, _, _)| id == h));
            let id = home.unwrap_or_else(|| {
                top += 1;
                top
            });
            match taken {
                Some(p) => {
                    rekey(p, id)?;
                    // Of two moves before anything followed, what names it still says the first path.
                    let left = rows.iter().find(|(at, q, _)| *at == p && q != path).map(|(_, q, _)| q);
                    tx.execute("UPDATE folders SET path = ?2, home = ?1, was = coalesce(was, ?3) WHERE id = ?1", params![id, path, left])?;
                    tx.execute("DELETE FROM meta WHERE key = ?1", [format!("parked:{p}")])?;
                    stepped.retain(|s| *s != p);
                    tracing::info!("catalogue: {path} is in use again, with its rows");
                    // Links may have gone stale while it was away: every server lists its tracks again.
                    tx.execute("DELETE FROM meta WHERE key LIKE 'full-folders:%'", [])?;
                }
                None => {
                    tx.execute("INSERT INTO folders (id, path, home) VALUES (?1, ?2, ?1)", params![id, path])?;
                }
            }
        }
        // 3. What stepped aside and was not taken is parked, from now on, and stays so: the mark says since when.
        for (p, path, _) in known(&tx)?.into_iter().filter(|(id, _, _)| stepped.contains(id)) {
            tx.execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)", params![format!("parked:{p}"), now_s.to_string()])?;
            tracing::info!("catalogue: {path} is out of use; its rows are kept");
        }
        for p in forget {
            tx.execute(
                &format!(
                    "UPDATE tracks SET retired_at = ?2, {REMEMBER} WHERE id IN (SELECT track FROM files WHERE folder = ?1 AND track IS NOT NULL)
                     AND NOT EXISTS (SELECT 1 FROM files o WHERE o.track = tracks.id AND o.folder != ?1)"
                ),
                params![p, now_s],
            )?;
            for table in FILE_TABLES {
                tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1"), [p])?;
            }
            tx.execute("DELETE FROM folders WHERE id = ?1", [p])?;
            tx.execute("DELETE FROM meta WHERE key = ?1", [format!("parked:{p}")])?;
            // Its links to the servers' tracks went with it: every server lists all its tracks again on the next pass.
            tx.execute("DELETE FROM meta WHERE key LIKE 'full-folders:%'", [])?;
        }
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![FOLDER_TOP, top.to_string()])?;
        let in_use: InUse = known(&tx)?.into_iter().filter(|(id, _, _)| *id >= 0).map(|(id, path, _)| (id, PathBuf::from(path))).collect();
        tx.commit()?;
        drop(db);
        *self.folders.write().unwrap_or_else(|p| p.into_inner()) = in_use;
        self.poke.notify_one();
        Ok(())
    }

    /// The libraries that changed path since what names them by it last followed: (where it was, where it is). A switch on an
    /// account says a library's path, so a rename must reach it, or the library comes back on. Kept here until it has, as
    /// `track_moved` is: a stop between the two databases finishes at the next start. Blocking.
    pub fn paths_to_follow(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT was, path FROM folders WHERE was IS NOT NULL AND was != path")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What `paths_to_follow` gave has been followed. A library that moved again meanwhile waits: it says the path followed from no
    /// longer. Blocking.
    pub fn paths_followed(&self, moved: &[(String, String)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        for (was, path) in moved {
            tx.execute("UPDATE folders SET was = NULL WHERE was = ?1 AND path = ?2", params![was, path])?;
        }
        // Moved there and back: nothing named it elsewhere.
        tx.execute("UPDATE folders SET was = NULL WHERE was = path", [])?;
        tx.commit()?;
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

    /// Whether a folder holds a file that counts, gone and trashed ones not being such: what Keep and Delete leave behind
    /// them is no reason for alarm. Blocking.
    fn has_present(&self, folder: i64) -> Result<bool> {
        Ok(self.db().prepare_cached(&format!("SELECT 1 FROM files WHERE folder = ?1 AND {PRESENT} LIMIT 1"))?.exists([folder])?)
    }

    /// Walk every folder and record what is new, changed or gone. Returns problems (unreadable folders, and those that read
    /// as empty where they had files). Blocking.
    pub fn walk_all(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (id, folder) in self.folders().iter() {
            match walk::walk_with_covers(folder) {
                // A bind mount whose source is not up, an NFS share gone, read as an empty folder: treated as unreadable.
                // So is a library someone really emptied, which stays listed until 0.17's library safety can tell them apart.
                Ok((found, ..)) if found.is_empty() && self.has_present(id).unwrap_or(true) => {
                    problems.push(format!("{} reads empty: is it mounted? Its files stay listed", folder.display()));
                }
                Ok((found, covers, skipped)) => {
                    self.work(1);
                    if let Err(e) = self.record_walk(id, &found).and_then(|()| self.record_covers(id, &covers)) {
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
        let now = now();
        {
            let mut upsert = tx.prepare(
                "INSERT INTO files (folder, rel, size, mtime, state, came) VALUES (?1, ?2, ?3, ?4, 0, ?5)
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
                        upsert.execute(params![folder, f.rel, f.size as i64, f.mtime, now])?;
                    }
                }
            }
        }
        // Missing now: gone, not deleted. It keeps its track id and tags until it is recognised elsewhere (a move made
        // outside Cratarr) or `GONE_KEEP_SECS` pass. Its server links go at once: they point at a file that is not there.
        let present: std::collections::HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
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
            let Some((folder, rel)) = folders.iter().find_map(|(id, root)| {
                let rel = path.strip_prefix(root).ok()?;
                let rel: Vec<&str> = rel.components().filter_map(|c| c.as_os_str().to_str()).collect();
                (!rel.is_empty()).then(|| (id, rel.join("/")))
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
            self.changed_outside_pass();
        }
        Ok(read)
    }

    /// Rows Cratarr changed itself, outside a pass (files read again, a record's files moved, other track ids given):
    /// the library is told to build at once, not at its next check (a record moved and moved back by Undo was listed
    /// where it no longer was for up to a minute, 2026-11-01). Told once a whole change is done (a record's files moved
    /// one by one: after the last), so the library never shows half of one.
    pub fn changed_outside_pass(&self) {
        self.progress.lock().unwrap_or_else(|p| p.into_inner()).last_pass = Some(now());
        self.reread.notify_one();
    }

    /// The catalogue's rows changed outside a pass (`changed_outside_pass`).
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
                let r = match folders.get(folder) {
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
                        "UPDATE files SET state = ?3, format = ?4, bitrate = ?5, duration_ms = ?6, error = NULL, issue = ?7, issue_detail = ?8, bits = ?9, rate = ?10
                         WHERE folder = ?1 AND rel = ?2",
                        params![
                            folder,
                            rel,
                            READ,
                            t.format,
                            t.bitrate_kbps.map(i64::from),
                            t.duration_ms.map(|d| d as i64),
                            issue.as_ref().map(|(i, _)| i.code()),
                            issue.as_ref().map(|(_, d)| d.as_str()),
                            t.bit_depth.map(i64::from),
                            t.sample_rate_hz.map(i64::from)
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
        rels.iter().map(|rel| item_at(&db, source, folder, rel)).collect()
    }

    /// The server's id of the file each playlist line names (`place_of_line`), in order. Blocking.
    pub fn line_track_ids(&self, source: &str, lines: &[String]) -> Result<Vec<Option<String>>> {
        let db = self.db();
        lines
            .iter()
            .map(|l| match self.place_of_line(l) {
                Some((folder, rel)) => item_at(&db, source, folder, &rel),
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
    /// with its folder and what the crates' axes test: genres, year, BPM, AudioMuse's. Kept until what they are read from
    /// or the folders change: reading them means reading every file's tags. Blocking.
    pub fn crate_facts(&self) -> Result<Arc<Vec<crate::crates::Facts>>> {
        // Read again when the library is built again too: its albums' release types (0.9.6) come from there.
        let served = crate::serve::instance();
        let (written, folders, built) = (self.facts_written.load(Ordering::Relaxed), self.folders(), served.as_ref().map_or(0, |s| s.built_at()));
        let words = served.as_ref().map_or(0, |s| said_stamp(s));
        let marks = served.as_ref().map_or(0, |s| s.userdata.marks_stamp().unwrap_or(0));
        let mut kept = self.facts.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(k) = kept.as_ref()
            && k.folders == folders
            && k.built == built
            && k.marks == marks
            // A write to what they are read from or a word harvested reads them again only every so often (0.9.7: a
            // reader's reread of every file writes all along for hours, the harvest a word at a time; each Home read
            // them all again, and the sim went past 768 MB). A build or other folders read them at once.
            && ((k.written == written && k.words == words) || k.at.elapsed() < FACTS_EVERY)
        {
            return Ok(k.facts.clone());
        }
        // The reading before goes first: two at once over every file is what the limit cannot hold. Its address stays
        // taken until this one has its own, as what hangs on a reading knows it by that (`shared_alike`, the crates' sorted
        // lists, `words::search_index`): a new one in the same place would pass for it.
        let was = kept.take().map(|k| Arc::downgrade(&k.facts));
        // Beside the catalogue, not under its one lock: a play starting and every writer would wait out the whole read.
        let reader = self.reader();
        let shared = match &reader {
            Some(c) => tracks_of_several(c)?,
            None => tracks_of_several(&self.db())?,
        };
        // Asked first, as nothing here may wait on the library with the catalogue held.
        let mut several: HashMap<i64, Option<Place>> = shared.iter().map(|t| (*t, None)).collect();
        if let Some(s) = served.as_ref().filter(|_| !shared.is_empty()) {
            let songs: Vec<String> = shared.iter().map(|t| crate::serve::ids::song(*t)).collect();
            for (song, place) in s.library.song_places(&songs)? {
                if let Some(t) = crate::serve::ids::track_of(&song) {
                    several.insert(t, Some(place));
                }
            }
        }
        let mut facts = match &reader {
            Some(c) => Self::read_crate_facts(c, &folders, &several)?,
            None => Self::read_crate_facts(&self.db(), &folders, &several)?,
        };
        drop(reader);
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
            // Each file with its record's words and its own, from the one store (decision 0097): every source.
            if let Err(e) = crate::app::words::build(s, &albums, &mut facts) {
                tracing::warn!("crates: the words not read: {e:#}");
            }
            // And with its album's id (0.11, quick crates: the Album axis, an album seed), one copy for all its
            // tracks; its labels Discogs' for its album where its tags name none, as kept (never asked for here).
            match crate::app::discogs_facts::kept_labels(s) {
                Ok(labels) => {
                    let mut shared: HashMap<&str, Arc<str>> = HashMap::new();
                    for f in &mut facts {
                        let Some(album) = f.track.and_then(|t| albums.get(&crate::serve::ids::song(t))) else { continue };
                        f.album = Some(shared.entry(album.as_str()).or_insert_with(|| Arc::from(album.as_str())).clone());
                        if f.labels.is_empty()
                            && let Some(l) = labels.get(album)
                        {
                            f.labels = l.clone();
                        }
                    }
                }
                Err(e) => tracing::warn!("crates: albums and labels not read: {e:#}"),
            }
        }
        let facts = Arc::new(facts);
        drop(was);
        *kept = Some(KeptFacts { written, folders, built, words, marks, at: std::time::Instant::now(), facts: facts.clone() });
        Ok(facts)
    }

    /// One `Facts` a track: `several` holds each track with more than one file present, by the place its row copy has in the
    /// library (none: the first by folder). The other files lend their lines and folders and are read no further, so a
    /// second copy costs no memory here: this read is what the 768 MB limit is tightest on.
    fn read_crate_facts(db: &Connection, folders: &InUse, several: &HashMap<i64, Option<Place>>) -> Result<Vec<crate::crates::Facts>> {
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
        let mut copies: HashMap<i64, Vec<(Place, String)>> = HashMap::new();
        for (folder, rel, _, track, ..) in &files {
            if let Some(t) = track.filter(|t| several.contains_key(t))
                && let Some(line) = line_in(folders, *folder, rel)
            {
                copies.entry(t).or_default().push(((*folder, rel.clone()), line));
            }
        }
        // What each such track's `Facts` takes from its other files, and those files, left out of every read below.
        let mut lent: HashMap<i64, (Vec<String>, Vec<i64>)> = HashMap::new();
        let mut left: HashSet<Place> = HashSet::new();
        for (track, mut held) in copies {
            held.sort();
            let row = several.get(&track).and_then(Option::as_ref).and_then(|p| held.iter().position(|(h, _)| h == p)).unwrap_or(0);
            let ((row_place, _), mut lines, mut in_folders) = (held.remove(row), Vec::new(), Vec::new());
            in_folders.push(row_place.0);
            for (place, line) in held {
                lines.push(line);
                if !in_folders.contains(&place.0) {
                    in_folders.push(place.0);
                }
                left.insert(place);
            }
            lent.insert(track, (lines, in_folders));
        }
        // The key its tags say (ID3v2 TKEY, Vorbis INITIALKEY or KEY).
        let mut st = db.prepare("SELECT folder, rel, value FROM tags WHERE folder >= 0 AND raw IN ('TKEY', 'INITIALKEY', 'KEY') ORDER BY folder, rel, n")?;
        let mut tag_keys: HashMap<Place, String> = HashMap::new();
        for row in st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?)))? {
            let (place, value) = row?;
            if !left.contains(&place) {
                tag_keys.entry(place).or_insert(value);
            }
        }
        // Its labels (0.11, quick crates): Vorbis LABEL or ORGANIZATION and MP4's LABEL (read as "label"), ID3v2 TPUB and
        // Vorbis PUBLISHER (lofty reads those as the publisher: kept by their raw keys). Every tag is kept, so no file
        // needs reading again for them.
        let mut st = db.prepare("SELECT folder, rel, value FROM tags WHERE folder >= 0 AND (name = 'label' OR raw IN ('TPUB', 'PUBLISHER')) ORDER BY folder, rel, n")?;
        let mut tag_labels: HashMap<Place, Vec<String>> = HashMap::new();
        for row in st.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?)))? {
            let (place, value) = row?;
            if left.contains(&place) {
                continue;
            }
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
            if !left.contains(&place) {
                by.entry(place).or_default().push((name, value));
            }
        }
        // The tempo where the file has no BPM tag: its analysis' when sure enough (decision 0035), while the file is as
        // it was then. The tag comes first. With it, four on the floor (0.15), measured in the same analysis.
        let mut st = db.prepare(
            "SELECT a.folder, a.rel, a.bpm, a.four FROM analysis a JOIN files f ON f.folder = a.folder AND f.rel = a.rel
             WHERE a.folder >= 0 AND (a.bpm IS NOT NULL OR a.four IS NOT NULL) AND a.size = f.size AND a.mtime = f.mtime
               AND (a.complete IS NULL OR a.complete != 0)",
        )?;
        let analysed: HashMap<Place, (Option<f64>, Option<f64>)> = st
            .query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), (r.get::<_, Option<f64>>(2)?, r.get::<_, Option<f64>>(3)?))))?
            .collect::<rusqlite::Result<_>>()?;
        // The time signature, where Beat this! heard the file as it is and was sure enough of one (0.15).
        let mut st = db.prepare(
            "SELECT b.folder, b.rel, b.meter FROM beat_this b JOIN files f ON f.folder = b.folder AND f.rel = b.rel
             WHERE b.folder >= 0 AND b.meter > 0 AND b.size = f.size AND b.mtime = f.mtime",
        )?;
        let mut meters: HashMap<Place, u8> = st
            .query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, String>(1)?), r.get::<_, i64>(2)?.clamp(0, 255) as u8)))?
            .collect::<rusqlite::Result<_>>()?;
        // AudioMuse's (decision 0054), where it heard the track: energy, style tags, CLAP's moods. By track: every file of
        // it is the one sound, and a track of two files must not weigh double in what stands out below.
        type Muse = (Option<f64>, Option<String>, Option<String>, Option<Vec<u8>>, Option<String>);
        let mut st = db.prepare(&format!(
            "SELECT a.track, a.energy, a.style, a.moods, a.embedding, a.key FROM audiomuse a WHERE EXISTS (SELECT 1 FROM files f WHERE f.track = a.track AND {PRESENT_F})"
        ))?;
        let mut muse: HashMap<i64, Muse> =
            st.query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))))?.collect::<rusqlite::Result<_>>()?;
        // A mood a file has: one that stands out against the library's (the owner, 2026-10-23: CLAP scores every mood
        // between about 0.5 and 0.65, so a fixed 0.3 gave every heard file all five, every mood pill the same count).
        let stand_out = crate::app::audiomuse::stand_out(muse.values().filter_map(|m| crate::app::audiomuse::mood_scores(m.2.as_deref())));
        Ok(files
            .into_iter()
            .filter_map(|(folder, rel, born, file_track, duration_ms, format, bitrate)| {
                let place = (folder, rel);
                if left.contains(&place) {
                    return None;
                }
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
                if let Some((e, style, clap, embedding, k)) = file_track.and_then(|t| muse.remove(&t)) {
                    muse_key = k;
                    // Its "sounds like" (decision 0054), of unit length: what a Sounds like condition compares. Made so
                    // twice: `similarity::Library` shares this vector now, and made so again what it was handed when it
                    // kept a copy. Once alone is a last bit off in most vectors, and a near tie would fall the other way.
                    let v: Vec<f32> = embedding.unwrap_or_default().chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    sound = crate::similarity::mean_sound(&[&v]).and_then(|v| crate::similarity::mean_sound(&[&v])).map(std::sync::Arc::from);
                    // Every mood's score, for a list's profile; none until CLAP heard the file (all 0).
                    clap_moods = crate::app::audiomuse::mood_scores(clap.as_deref());
                    moods = crate::app::audiomuse::moods_of(clap_moods, &stand_out).into_iter().map(|(m, _)| m.to_string()).collect();
                    let d = crate::app::audiomuse::danceability(&AudioMuseRow { moods: clap, ..AudioMuseRow::default() });
                    energy = e.map(|e| (e * 100.0).round());
                    danceability = d.map(|d| (d * 100.0).round());
                    styles = crate::app::audiomuse::weighted(style.as_deref()).into_iter().take(crate::app::audiomuse::STYLES).map(|(n, _)| n).collect();
                }
                // The files' STYLE and MOOD words apart from AudioMuse's: the words' store weighs them (decision 0097).
                let file_words = (!file_styles.is_empty() || !file_moods.is_empty()).then(|| Box::new((file_styles, file_moods)));
                let (credits, credit_names) = crate::app::excluded_artists::credited(&artists.iter().chain(&listed).collect::<Vec<_>>());
                let (lines, in_folders) = file_track.and_then(|t| lent.remove(&t)).unwrap_or_else(|| (Vec::new(), vec![place.0]));
                Some(crate::crates::Facts {
                    year: crate::crates::year_of(&first),
                    bpm: first.get("bpm").and_then(|b| crate::crates::bpm_of(b)).or_else(|| analysed.get(&place).and_then(|a| a.0).map(f64::round)),
                    four: analysed.get(&place).and_then(|a| a.1).map(|f| f as f32),
                    meter: meters.remove(&place).map(crate::analysis::rhythm::meter_name),
                    genres,
                    rel: line,
                    lines,
                    folders: in_folders,
                    born,
                    energy,
                    danceability,
                    moods,
                    styles,
                    track: file_track,
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
                    file_words,
                })
            })
            .collect())
    }

    /// The playlist lines of the files these server track ids are, in any folder in use (`line_of`); ids not known are
    /// left out. Cratarr's own id is a track: its one line, as the crates know it (`row_line`). Blocking.
    pub fn lines_of(&self, source: &str, ids: &[String]) -> Result<std::collections::HashSet<String>> {
        if source == crate::source::cratarr::KIND.id {
            return Ok(ids.iter().filter_map(|id| self.row_line(self.track_of_source(source, id)?)).collect());
        }
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

    /// A track's playlist line as the crates' facts have it (`read_crate_facts`): its row copy's. A line written now goes by
    /// this, as the lowest folder can be someone's own music or a copy waiting in Incoming. Blocking.
    pub fn row_line(&self, track: i64) -> Option<String> {
        let files = self.files_of(track);
        let row = crate::serve::instance().filter(|_| files.len() > 1).and_then(|s| s.library.file_of(&crate::serve::ids::song(track)).ok().flatten());
        row_line_of(&self.folders(), &files, row.as_ref())
    }

    /// The file a playlist line names, on the disk (`place_of_line`): Music's, or another folder's for a line that
    /// climbs out of it.
    pub fn path_of_line(&self, line: &str) -> Option<PathBuf> {
        let (folder, rel) = self.place_of_line(line)?;
        Some(self.folders().get(folder)?.join(rel))
    }

    /// Where a playlist line points: (folder, rel). Lines are relative to the Music folder (the first in use,
    /// `InUse::first`); a line for a file in another folder climbs out of it ("../lidarr-music/X/01.flac", an Incoming
    /// track). None for a line that lands in no folder of the catalogue.
    pub fn place_of_line(&self, line: &str) -> Option<(i64, String)> {
        let folders = self.folders();
        let (first, root) = folders.first()?;
        if !line.starts_with("../") {
            return Some((first, line.to_string()));
        }
        let mut target = root.clone();
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
        folders.iter().filter(|(id, _)| *id != first).find_map(|(id, f)| {
            let rel = target.strip_prefix(f).ok()?.to_string_lossy().replace('\\', "/");
            (!rel.is_empty()).then_some((id, rel))
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
                row.track_id = item_at(&db, source, *folder, rel)?;
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

    /// Every present file's values of the tags named in `names`, by (folder, path): what the files hold of what Cratarr
    /// writes into them (decision 0092, `app::file_facts`). Read-only; blocking.
    pub fn named_values(&self, names: &[&str]) -> Result<HashMap<(i64, String), HashMap<String, Vec<String>>>> {
        let db = self.db();
        let list = names.iter().map(|n| format!("'{}'", n.replace('\'', "''"))).collect::<Vec<_>>().join(", ");
        let mut st = db.prepare(&format!(
            "SELECT t.folder, t.rel, t.name, t.value FROM tags t JOIN files f ON f.folder = t.folder AND f.rel = t.rel
             WHERE {PRESENT_F} AND t.name IN ({list}) ORDER BY t.folder, t.rel, t.n"
        ))?;
        let mut out: HashMap<(i64, String), HashMap<String, Vec<String>>> = HashMap::new();
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
            let (folder, rel, name, value) = row?;
            out.entry((folder, rel)).or_default().entry(name).or_default().push(value);
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

    /// Files with a problem, worst first ("broken", then damaged tags, …), at most `each` of every kind: a kind past
    /// the others' files still has its own (2026-11-02: one cap over all kinds left the last kind on Home with none,
    /// and its Health panel said it was no longer a problem). Blocking.
    /// The files with one kind of problem, by folder and line. Blocking.
    pub fn with_issue(&self, kind: &str) -> Result<Vec<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare(&format!("SELECT folder, rel FROM files WHERE issue = ?1 AND {PRESENT} ORDER BY rel"))?;
        let rows = st.query_map([kind], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn problems(&self, each: usize) -> Result<Vec<Problem>> {
        let folders = self.folders();
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT folder, rel, issue, issue_detail FROM (
                 SELECT folder, rel, issue, issue_detail, ROW_NUMBER() OVER (PARTITION BY issue ORDER BY rel) AS n
                 FROM files WHERE issue IS NOT NULL AND {PRESENT})
             WHERE n <= ?1
             ORDER BY CASE issue WHEN 'broken' THEN 0 WHEN 'damaged_tag' THEN 1 WHEN 'no_length' THEN 2 ELSE 3 END, rel"
        ))?;
        let rows = st.query_map([each as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })?;
        rows.map(|r| {
            let (folder, rel, kind, detail) = r?;
            Ok(Problem { folder: folders.get(folder).map(|p| p.display().to_string()).unwrap_or_default(), rel, kind, detail })
        })
        .collect()
    }

    /// The problems of these files of a folder (0.14, a record's own Health): those that have one, in their order.
    pub fn problems_in(&self, folder: i64, rels: &[String]) -> Result<Vec<Problem>> {
        let path = self.folders().get(folder).map(|p| p.display().to_string()).unwrap_or_default();
        let db = self.db();
        let mut st = db.prepare_cached(&format!("SELECT issue, issue_detail FROM files WHERE folder = ?1 AND rel = ?2 AND issue IS NOT NULL AND {PRESENT}"))?;
        let mut out = Vec::new();
        for rel in rels {
            if let Some((kind, detail)) = st.query_row(params![folder, rel], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).optional()? {
                out.push(Problem { folder: path.clone(), rel: rel.clone(), kind, detail });
            }
        }
        Ok(out)
    }

    /// Tests elsewhere: treat every file waiting for its tags as read (test files are not audio).
    #[cfg(test)]
    pub fn pretend_read_for_tests(&self) {
        self.db().execute("UPDATE files SET state = 1, duration_ms = 200000 WHERE state = 0", []).unwrap();
    }

    /// Tests elsewhere: when a folder's files came.
    #[cfg(test)]
    pub fn came_for_tests(&self, folder: i64, at: i64) {
        self.db().execute("UPDATE files SET came = ?2 WHERE folder = ?1", [folder, at]).unwrap();
    }

    /// Tests elsewhere: every gone file gone longer than a gone file waits, for `identify` to retire it.
    #[cfg(test)]
    pub fn gone_long_ago_for_tests(&self) {
        self.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
    }

    /// Tests elsewhere: as a stop between the user data's move and its mark leaves it.
    #[cfg(test)]
    pub fn not_followed_for_tests(&self) {
        self.db().execute("UPDATE track_moved SET followed = 0", []).unwrap();
    }

    /// Tests elsewhere: a file on a track, as the take leaves a record's copies.
    #[cfg(test)]
    pub fn give_track_for_tests(&self, folder: i64, rel: &str, track: i64) {
        self.db().execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, track]).unwrap();
    }

    /// Tests elsewhere: a file found wrong, as a read would leave it.
    #[cfg(test)]
    pub fn issue_for_tests(&self, folder: i64, rel: &str, issue: &str, detail: &str) {
        self.db().execute("UPDATE files SET issue = ?3, issue_detail = ?4 WHERE folder = ?1 AND rel = ?2", params![folder, rel, issue, detail]).unwrap();
    }

    /// Tests elsewhere: a file's tags as a read would leave them.
    #[cfg(test)]
    pub fn tag_for_tests(&self, folder: i64, rel: &str, tags: &[(&str, &str)]) {
        let db = self.db();
        db.execute("DELETE FROM tags WHERE folder = ?1 AND rel = ?2", params![folder, rel]).unwrap();
        for (n, (name, value)) in tags.iter().enumerate() {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, n as i64, name, value]).unwrap();
        }
    }

    // ----- Track identity (decision 0011 §4) -----

    /// Give every identified file a track id: a new file that is a gone one moved (by the rules of
    /// `identity::match_moves`) takes its id, else one that is a retired track takes that id back (0101); any other gets a
    /// new one. Files whose tags are not read yet wait. Gone
    /// files nobody claimed within `GONE_KEEP_SECS`, and trashed files whose trash was emptied, retire their ids.
    /// Returns (moves recognised, new ids). Blocking.
    pub fn identify(&self) -> Result<(usize, usize)> {
        // Its ids lost: nothing given out, nothing retired, until it is restored or started afresh (0101).
        if self.ids_lost() {
            return Ok((0, 0));
        }
        let now = now();
        let folders = self.folders();
        let mut db = self.db();
        let tx = db.transaction()?;
        let facts = |sql: &str| -> Result<Vec<Identified>> {
            let mut st = tx.prepare(sql)?;
            let rows = st.query_map([], |r| Ok(Identified { folder: r.get(0)?, rel: r.get(1)?, duration_ms: r.get(2)?, track: r.get(3)?, size: r.get(4)?, mtime: r.get(5)? }))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        let mut new = facts(&format!(
            "SELECT folder, rel, duration_ms, track, size, mtime FROM files WHERE track IS NULL AND state != {PENDING} AND {PRESENT}"
        ))?;
        let mut moved = 0;
        let mut claimed: std::collections::HashSet<(i64, String)> = std::collections::HashSet::new();
        // Files Cratarr handed a track to (a better copy in its old one's place, or its Undo): theirs before any rule, the
        // gone file it replaces letting go. That file only: any other gone row of the track is another copy, in this
        // folder or elsewhere, away for a moment, that comes back to the same track. Unclaimed for a day: forgotten.
        {
            let mut st = tx.prepare("SELECT folder, rel, track, replaces FROM handovers")?;
            let handed: Vec<(i64, String, i64, Option<String>)> =
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
            drop(st);
            for (folder, rel, track, replaces) in handed {
                if !new.iter().any(|f| f.folder == folder && f.rel == rel) {
                    continue;
                }
                // Handed on before a take, it is the record's track now (CR4).
                let track = taken_by(&tx, track)?;
                let mut st = tx.prepare("SELECT rel FROM files WHERE track = ?1 AND folder = ?2 AND rel = ?3 AND gone_at IS NOT NULL")?;
                let old: Vec<String> = st.query_map(params![track, folder, replaces], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                drop(st);
                for r in old {
                    carry_came(&tx, (folder, &rel), (folder, &r))?;
                    for table in ["files", "tags"] {
                        tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![folder, r])?;
                    }
                }
                tx.execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, track])?;
                tx.execute("UPDATE tracks SET retired_at = NULL WHERE id = ?1", [track])?;
                tx.execute("DELETE FROM handovers WHERE folder = ?1 AND rel = ?2", params![folder, rel])?;
                claimed.insert((folder, rel));
                moved += 1;
            }
            tx.execute("DELETE FROM handovers WHERE at < ?1", [now - GONE_KEEP_SECS])?;
            new.retain(|f| !claimed.contains(&(f.folder, f.rel.clone())));
        }
        let gone = facts("SELECT folder, rel, duration_ms, track, size, mtime FROM files WHERE gone_at IS NOT NULL AND track IS NOT NULL")?;
        let mut taken: std::collections::HashSet<usize> = std::collections::HashSet::new();
        if !new.is_empty() {
            let mut tag = tx.prepare_cached(
                "SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IN ('musicbrainz_trackid', 'title', 'artist') ORDER BY n",
            )?;
            let mut file_facts = |file: &Identified| -> Result<identity::FileFacts> {
                let mut f = identity::FileFacts {
                    folder: file.folder,
                    track: file.track,
                    name: identity::name_of(&file.rel),
                    duration_ms: file.duration_ms,
                    size: Some(file.size),
                    mtime: Some(file.mtime),
                    ..Default::default()
                };
                for row in tag.query_map(params![file.folder, file.rel], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
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
            for (n, g) in identity::match_moves_by_folder(&new_facts, &gone_facts) {
                let (Identified { folder: nf, rel: nr, .. }, Identified { folder: gf, rel: gr, track, .. }) = (&new[n], &gone[g]);
                tx.execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![nf, nr, track])?;
                carry_came(&tx, (*nf, nr), (*gf, gr))?;
                // The same sound under another name: what was worked out of it goes along, and is read while the file is as it was then.
                for table in ["analysis", "analysis_shape", "beat_this"] {
                    tx.execute(&format!("UPDATE OR IGNORE {table} SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2"), params![gf, gr, nf, nr])?;
                }
                for table in FILE_TABLES {
                    tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![gf, gr])?;
                }
                claimed.insert((*nf, nr.clone()));
                taken.insert(g);
                moved += 1;
            }
            // A retired track comes back as itself (0101 point 11: a missing entry relinks by itself): a file moved
            // outside Cratarr after the gone files' day, or fetched again, by the same rules, among the retired tracks
            // that share something with a new file (there may be thousands).
            let free: Vec<usize> = (0..new.len()).filter(|&n| !claimed.contains(&(new[n].folder, new[n].rel.clone()))).collect();
            if !free.is_empty() {
                let free_facts: Vec<identity::FileFacts> = free.iter().map(|&n| new_facts[n].clone()).collect();
                let retired = retired_like(&tx, &free_facts)?;
                let retired_facts: Vec<identity::FileFacts> = retired.iter().map(|(_, f)| f.clone()).collect();
                for (i, r) in identity::match_moves(&free_facts, &retired_facts) {
                    let Identified { folder, rel, .. } = &new[free[i]];
                    // Its old row went as it retired: when the track was born is what is left of when it came.
                    tx.execute(
                        "UPDATE files SET track = ?3, came = (SELECT born FROM tracks WHERE id = ?3) WHERE folder = ?1 AND rel = ?2",
                        params![folder, rel, retired[r].0],
                    )?;
                    tx.execute("UPDATE tracks SET retired_at = NULL, back = 1 WHERE id = ?1", [retired[r].0])?;
                    claimed.insert((*folder, rel.clone()));
                    moved += 1;
                }
            }
        }
        let mut fresh = 0;
        {
            let mut born = tx.prepare_cached("INSERT INTO tracks (born) VALUES (?1)")?;
            let mut give = tx.prepare_cached("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2")?;
            for file in new.iter().filter(|f| !claimed.contains(&(f.folder, f.rel.clone()))) {
                born.execute([now])?;
                give.execute(params![file.folder, file.rel, tx.last_insert_rowid()])?;
                fresh += 1;
            }
        }
        // Retire: gone too long, or trashed and the trash emptied.
        let mut retire: Vec<(i64, String)> = gone
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken.contains(i))
            .map(|(_, f)| (f.folder, f.rel.clone()))
            .collect();
        {
            let mut st = tx.prepare("SELECT folder, rel FROM files WHERE gone_at IS NOT NULL AND gone_at >= ?1")?;
            let recent: std::collections::HashSet<(i64, String)> =
                st.query_map([now - GONE_KEEP_SECS], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            retire.retain(|k| !recent.contains(k));
            let mut st = tx.prepare("SELECT DISTINCT folder, trashed FROM files WHERE trashed IS NOT NULL")?;
            let trashed: Vec<(i64, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            for (folder, token) in trashed {
                let still = folders.get(folder).is_some_and(|f| f.join(crate::trash::TRASH_DIR).join(&token).exists());
                if !still {
                    let mut st = tx.prepare("SELECT rel FROM files WHERE folder = ?1 AND trashed = ?2")?;
                    let rels: Vec<String> = st.query_map(params![folder, token], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                    retire.extend(rels.into_iter().map(|r| (folder, r)));
                }
            }
        }
        // A track lives while any copy holds a file of it, gone a while or in a trash included: the last one retires it.
        for (folder, rel) in &retire {
            tx.execute(
                &format!(
                    "UPDATE tracks SET retired_at = ?3, {REMEMBER} WHERE id = (SELECT track FROM files WHERE folder = ?1 AND rel = ?2)
                     AND NOT EXISTS (SELECT 1 FROM files o WHERE o.track = tracks.id AND (o.folder != ?1 OR o.rel != ?2))"
                ),
                params![folder, rel, now],
            )?;
            for table in FILE_TABLES {
                tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![folder, rel])?;
            }
        }
        tx.commit()?;
        Ok((moved, fresh))
    }

    /// Every track not retired, its files listed or parked with their library. Blocking.
    pub fn tracks_alive(&self) -> Result<Vec<i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id FROM tracks WHERE retired_at IS NULL")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a track in use has no record yet, or is back from retirement and not yet held against its album: only then,
    /// or with a stray in an album (`AlbumCopy::split`), is [`Self::assign_records`] worth its reading. Blocking.
    pub fn records_unsettled(&self) -> bool {
        self.db()
            .prepare_cached("SELECT 1 FROM tracks WHERE (record IS NULL OR back IS NOT NULL) AND retired_at IS NULL LIMIT 1")
            .and_then(|mut st| st.exists([]))
            .unwrap_or(false)
    }

    /// Give these albums their records (0100, docs/records-and-copies.md), every album there is. An album is the record
    /// more than half of its tracks hold, or the one nothing else holds ([`Held::vote`]), where it holds the same tracks as
    /// the largest album that says so; else it is held against the others as a new one is: a copy of the record it is the
    /// same as (`identity::same_record`), or a record of its own. A new track joins its album's record. A track keeps its
    /// record for life, unless its only file now sits in another record's album, where it moves: a retired id handed to a
    /// file of a best-of must not take that album with it.
    ///
    /// A copy takes the record's tracks (0101), place by place: its files go to the record's track, and the id they held
    /// retires for good, with what it became in `track_moved`, for what names it to follow (`Serve::built_now`). Copies of
    /// one record always hold the same tracks (Q8, the owner): one that no longer can leaves the record to the first by kind,
    /// then the largest (CR18), and takes tracks of its own, as what points at them stays with the record. The tracks given a
    /// record, taken or let go of. Blocking.
    pub fn assign_records(&self, albums: &[AlbumCopy]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let read_half_way = self.read_half_way.swap(false, Ordering::SeqCst);
        let (given, _) = Self::assign(&tx, albums, now(), read_half_way || self.copies_are_moving())?;
        tx.commit()?;
        Ok(given)
    }

    /// An album action is moving a copy's files, until this goes. Passes do not stop for one, and `identify` pairs what has
    /// arrived with its gone row, so a pass sees a copy half way with nothing saying it lacks anything for now: no copy leaves
    /// its record meanwhile. What really differs goes at the first build after.
    pub fn copies_moving(&self) -> CopiesMoving {
        self.moving.fetch_add(1, Ordering::SeqCst);
        CopiesMoving(self.moving.clone())
    }

    /// Whether an album action is moving a copy's files now.
    pub fn copies_are_moving(&self) -> bool {
        self.moving.load(Ordering::SeqCst) > 0
    }

    /// A build is about to read the albums it gives [`Self::assign_records`]: what it reads half way is no better for the
    /// action ending before it gets there, and a read takes seconds.
    pub fn reading_albums(&self) {
        self.read_half_way.store(self.copies_are_moving(), Ordering::SeqCst);
    }

    /// `assign_records` in the caller's transaction, as the first start's merge commits with its map (`first_records`): the
    /// tracks given a record, taken or let go of, and each album's record, none where nothing there can take one.
    fn assign_in(tx: &Connection, albums: &[AlbumCopy], now: i64) -> Result<(usize, Vec<Option<i64>>)> {
        Self::assign(tx, albums, now, false)
    }

    /// `assign_in`, `moving` saying these albums may be half way through an action, so none of them leaves.
    fn assign(tx: &Connection, albums: &[AlbumCopy], now: i64, moving: bool) -> Result<(usize, Vec<Option<i64>>)> {
        let mut held = Held::read(tx)?;
        held.ranked(albums);
        // Who to hold a new album against: the albums that have a record, by name and by release, then the new ones as
        // they get theirs.
        let (mut by_name, mut by_release) = (HashMap::new(), HashMap::new());
        type ByName = HashMap<(String, String), Vec<(usize, i64)>>;
        type ByRelease = HashMap<String, Vec<(usize, i64)>>;
        fn note(i: usize, a: &AlbumCopy, record: i64, by_name: &mut ByName, by_release: &mut ByRelease) {
            by_name.entry((a.facts.artist.clone(), a.facts.title.clone())).or_default().push((i, record));
            if let Some(r) = &a.facts.release {
                by_release.entry(r.clone()).or_default().push((i, record));
            }
        }
        let mut records: Vec<Option<i64>> = albums.iter().map(|a| held.vote(a)).collect();
        // Each album's tracks as this pass leaves them.
        let mut tracks: Vec<Vec<i64>> = albums.iter().map(|a| a.tracks.iter().map(|t| t.0).collect()).collect();
        let mut take = Take::default();
        // Those told here to leave: what they join below is a record they are the same as, never one they are short of.
        let mut leaving: HashSet<usize> = HashSet::new();
        // The tracklists must match, always, so albums voting one record hold one set of tracks. Where the ids alone differ, a
        // file replaced outside Cratarr, the track is taken. Where the tracks do, a track added to one or one disc of a double
        // album retagged, the first by kind keeps the record, then the largest, then the first met (CR18: nobody has checked an
        // Incoming copy, and one account's own is no one else's), and the others are held against the rest as a new album is:
        // two albums, as a deluxe edition is its own record. What the ids hold together stays together whatever the tags say,
        // or a length fix in one copy would split it. A file gone for now splits nothing, nor a copy an album action is moving.
        let mut voting: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (i, r) in records.iter().enumerate() {
            if let Some(r) = r {
                voting.entry(*r).or_default().push(i);
            }
        }
        for (record, list) in voting.iter().filter(|(_, l)| l.len() > 1) {
            let Some(keeps) = list.iter().copied().max_by_key(|i| (albums[*i].rank, albums[*i].tracks.len(), std::cmp::Reverse(*i))) else { continue };
            for i in list.iter().copied().filter(|i| *i != keeps) {
                if identity::same_record(&albums[i].facts, &albums[keeps].facts) {
                    take.place_by_place(&held, *record, &tracks[i], &tracks[keeps]);
                    continue;
                }
                // A take cannot be undone, so one by name stands only where it leaves the two one set of tracks, but for what
                // either lacks for now: a title two discs share is two tracks, and those discs go their own ways.
                let mut named = take.clone();
                named.by_name(&held, *record, (&tracks[i], &albums[i].facts.tracks), (&tracks[keeps], &albums[keeps].facts.tracks));
                let (mine, theirs) = (named.all(&tracks[i]), named.all(&tracks[keeps]));
                let (lacking, more): (Vec<i64>, Vec<i64>) = (theirs.difference(&mine).copied().collect(), mine.difference(&theirs).copied().collect());
                // Of the very same release a copy can be short of what the record has (the owner, 2026-11-07), arriving or for good,
                // and its ids hold it there whatever the tags say since. A track new to the record is no such thing: there the
                // other copy grew, and the one that holds all the record ever had leaves as any other, by Q8.
                let of_the_record = |list: &[i64]| list.iter().any(|t| held.record.get(t) == Some(record));
                let short = identity::same_release(&albums[i].facts, &albums[keeps].facts) && ((more.is_empty() && of_the_record(&lacking)) || (lacking.is_empty() && of_the_record(&more)));
                if short || (held.lacks_for_now(&albums[i], &lacking) && held.lacks_for_now(&albums[keeps], &more)) {
                    take = named;
                } else if !moving {
                    records[i] = None;
                    leaving.insert(i);
                }
            }
        }
        // CR19, the owner: a folder arriving file by file is short of a record you hold at every pass, so it is a record of its own
        // until its last file. A record less than a day old, alone in one folder, with no track shared with anything, is held
        // against the others as a new album is. Older ones never merge, so nothing played or favourited moves by this.
        let young: HashSet<i64> = tx.prepare("SELECT id FROM records WHERE born >= ?1")?.query_map([now - GONE_KEEP_SECS], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let alone_in_a_folder = |a: &AlbumCopy, record: i64| {
            let held_here = a.tracks.iter().filter(|t| held.record.contains_key(&t.0)).count();
            held.files.get(&record) == Some(&held_here) && a.tracks.iter().all(|t| !held.shared.contains(&t.0))
        };
        let mut arriving: Vec<usize> = Vec::new();
        for (i, a) in albums.iter().enumerate() {
            match records[i] {
                Some(r) if young.contains(&r) && alone_in_a_folder(a, r) => arriving.push(i),
                Some(r) => note(i, a, r, &mut by_name, &mut by_release),
                None => {}
            }
        }
        // Of two that are one, the first by kind stands for them.
        arriving.sort_by_key(|i| std::cmp::Reverse(albums[*i].rank));
        for i in arriving {
            let (a, Some(r)) = (&albums[i], records[i]) else { continue };
            let named = by_name.get(&(a.facts.artist.clone(), a.facts.title.clone())).into_iter().flatten();
            let released = a.facts.release.iter().flat_map(|r| by_release.get(r)).flatten();
            if named.chain(released).any(|(j, _)| identity::same_record(&a.facts, &albums[*j].facts)) {
                records[i] = None;
            } else {
                note(i, a, r, &mut by_name, &mut by_release);
            }
        }
        // Each track a copy of its record lists: an album going elsewhere cannot take it along, and a file of it there lets go.
        let mut home: HashMap<i64, i64> = HashMap::new();
        for (list, record) in tracks.iter().zip(&records) {
            let Some(record) = record else { continue };
            home.extend(list.iter().map(|t| take.now(*t)).filter(|t| held.record.get(t) == Some(record)).map(|t| (t, *record)));
        }
        let mut given = 0;
        // The records a track moved out of.
        let mut left = std::collections::HashSet::new();
        {
            let mut born = tx.prepare_cached("INSERT INTO records (born, title, artist, year) VALUES (?1, ?2, ?3, ?4)")?;
            let mut give = tx.prepare_cached("UPDATE tracks SET record = ?2, disc = ?3, number = ?4 WHERE id = ?1 AND (record IS NULL OR (?5 AND record != ?2))")?;
            let mut of_its_own = tx.prepare_cached("INSERT INTO tracks (born, disc, number) SELECT born, disc, number FROM tracks WHERE id = ?1")?;
            let mut let_go = tx.prepare_cached("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2")?;
            for (i, a) in albums.iter().enumerate() {
                let record = match records[i] {
                    Some(r) => r,
                    None => {
                        let named = by_name.get(&(a.facts.artist.clone(), a.facts.title.clone())).into_iter().flatten();
                        let released = a.facts.release.iter().flat_map(|r| by_release.get(r)).flatten();
                        let same: Option<(usize, i64)> = named.chain(released).find(|(j, _)| identity::same_record(&a.facts, &albums[*j].facts)).copied();
                        // A folder of the very same release joins at once, short of the record or not yet held by it and more than
                        // it has (the owner, 2026-11-07). Held by it already and longer, it is a copy that grew, which is Q8's. Held against
                        // the longest there is, as a copy short of it says less of what its tracks are.
                        let arrives = |(j, r): &&(usize, i64)| {
                            let other = &albums[*j];
                            !leaving.contains(&i) && identity::short_of(&a.facts, &other.facts) && (a.tracks.len() < other.tracks.len() || !a.tracks.iter().any(|t| held.record.get(&t.0) == Some(r)))
                        };
                        let same = same.or_else(|| a.facts.release.iter().flat_map(|r| by_release.get(r)).flatten().filter(arrives).max_by_key(|(j, _)| albums[*j].tracks.len()).copied());
                        // Nothing here can take a record: every track stays with a file elsewhere, gone a while or in a trash.
                        // One with no record has nowhere to stay, and a row with no record is not served.
                        let stays = |t: &i64| {
                            let t = take.now(*t);
                            held.shared.contains(&t) && held.record.contains_key(&t) && !home.contains_key(&t)
                        };
                        if same.is_none() && tracks[i].iter().all(stays) {
                            continue;
                        }
                        // What another record keeps, its files here let go of, or they would be files of a track of that record in
                        // an album of this one, and never listed. A track of its own for each, as old as the file.
                        let elsewhere = |kept: &i64| home.get(kept).is_some_and(|r| Some(*r) != same.map(|s| s.1));
                        let mut own: HashMap<i64, i64> = HashMap::new();
                        let mut own_of = |kept: i64| -> rusqlite::Result<i64> {
                            if let Some(fresh) = own.get(&kept) {
                                return Ok(*fresh);
                            }
                            of_its_own.execute([kept])?;
                            own.insert(kept, tx.last_insert_rowid());
                            Ok(tx.last_insert_rowid())
                        };
                        for (at, track) in tracks[i].iter_mut().enumerate() {
                            let kept = take.now(*track);
                            if !elsewhere(&kept) {
                                continue;
                            }
                            let Some((folder, rel)) = a.files.get(at) else { continue };
                            let fresh = own_of(kept)?;
                            let_go.execute(params![folder, rel, fresh])?;
                            *track = fresh;
                            given += 1;
                        }
                        // A take goes by every row of a track, and so does this: a file beside these gone a while or not read is in
                        // no list, and left on the track it would come back a file of that record, in an album going by its own
                        // vote, and never be listed.
                        let beside = |(at, rel): &Place| a.files.iter().any(|(folder, file)| folder == at && dir_of(file) == dir_of(rel));
                        for (track, places) in held.gone.iter().filter(|(track, _)| elsewhere(&take.now(**track))) {
                            for (folder, rel) in places.iter().filter(|place| beside(place)) {
                                let_go.execute(params![folder, rel, own_of(take.now(*track))?])?;
                                given += 1;
                            }
                        }
                        let r = match same {
                            Some((j, r)) if identity::same_record(&a.facts, &albums[j].facts) => {
                                take.place_by_place(&held, r, &tracks[i], &tracks[j]);
                                r
                            }
                            // By name and length, as place by place a file would take a track it is not.
                            Some((j, r)) => {
                                take.by_name(&held, r, (&tracks[i], &a.facts.tracks), (&tracks[j], &albums[j].facts.tracks));
                                r
                            }
                            None => {
                                born.execute(params![now, a.title, a.artist, a.year])?;
                                tx.last_insert_rowid()
                            }
                        };
                        // Those it lists are given it below, with their places.
                        for fresh in own.values().filter(|fresh| !tracks[i].contains(fresh)) {
                            tx.execute("UPDATE tracks SET record = ?2 WHERE id = ?1", params![fresh, r])?;
                        }
                        records[i] = Some(r);
                        note(i, a, r, &mut by_name, &mut by_release);
                        r
                    }
                };
                for (track, (_, disc, number)) in tracks[i].iter().zip(&a.tracks) {
                    let track = take.now(*track);
                    let changed = give.execute(params![track, record, disc, number, !held.shared.contains(&track)])?;
                    given += changed;
                    if changed > 0 {
                        left.extend(held.record.get(&track).copied());
                    }
                }
            }
            // Held against their albums now, they count as any other track from here.
            let mut settled = tx.prepare_cached("UPDATE tracks SET back = NULL WHERE id = ?1")?;
            for (track, _, _) in albums.iter().flat_map(|a| &a.tracks).filter(|(track, _, _)| held.back.contains(track)) {
                settled.execute([track])?;
            }
        }
        // What was taken goes for good, all its files to the track that stays, gone ones and those in a trash too. Remembered
        // first: what it was is read from those files. The user data follows at the next `Serve::built_now`. By id: of two
        // taken by one track the first keeps its AudioMuse row, the same one every run.
        let mut taken: Vec<i64> = take.to.keys().copied().collect();
        taken.sort_unstable();
        for goes in taken {
            let stays = take.now(goes);
            tx.execute(&format!("UPDATE tracks SET retired_at = ?2, record = NULL, back = NULL, {REMEMBER} WHERE id = ?1"), params![goes, now])?;
            taken_for_good(tx, goes, stays)?;
            left.extend(held.record.get(&goes).copied());
            given += 1;
        }
        // A record lives while a track of it does (one that never had tracks is a record not had).
        tx.execute_batch(
            "UPDATE records SET retired_at = NULL WHERE retired_at IS NOT NULL
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id AND t.retired_at IS NULL);",
        )?;
        tx.execute(
            "UPDATE records SET retired_at = ?1 WHERE retired_at IS NULL AND EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id)
               AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id AND t.retired_at IS NULL)",
            [now],
        )?;
        // Every track moved on, none retired: nothing in the rows now tells this record from one not had, so it goes here,
        // while the pass still knows it had them.
        for record in &left {
            tx.execute(
                "UPDATE records SET retired_at = ?2 WHERE id = ?1 AND retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id)",
                params![record, now],
            )?;
        }
        Ok((given, records))
    }

    /// What a few folders' albums can settle alone: a track with no record takes the one its album's tracks say. Nothing
    /// is born and nothing moves, as both need every album there is: a new folder held against a few would be a record
    /// of its own for life. The next [`Self::assign_records`] does the rest. The tracks given a record. Blocking.
    pub fn join_records(&self, albums: &[AlbumCopy]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let held = Held::read(&tx)?;
        let mut given = 0;
        {
            let mut join = tx.prepare_cached("UPDATE tracks SET record = ?2, disc = ?3, number = ?4 WHERE id = ?1 AND record IS NULL")?;
            for (a, record) in albums.iter().filter_map(|a| Some((a, held.vote(a)?))) {
                for (track, disc, number) in &a.tracks {
                    given += join.execute(params![track, record, disc, number])?;
                }
            }
        }
        tx.commit()?;
        Ok(given)
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
    /// read again and no playlist entry is lost. The caller tells the library once all of a record's moved
    /// (`changed_outside_pass`). Blocking.
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
                for table in FILE_TABLES {
                    tx.execute(&format!("DELETE FROM {table} WHERE folder = ?1 AND rel = ?2"), params![to_folder, new_rel])?;
                }
            }
            tx.execute("DELETE FROM source_ids WHERE folder = ?1 AND rel = ?2", params![from_folder, rel])?;
            tx.execute(
                "UPDATE files SET folder = ?3, rel = ?4, trashed = ?5, gone_at = NULL WHERE folder = ?1 AND rel = ?2",
                params![from_folder, rel, to_folder, new_rel, trashed],
            )?;
            for table in ["tags", "analysis", "analysis_shape", "beat_this"] {
                tx.execute(&format!("UPDATE {table} SET folder = ?3, rel = ?4 WHERE folder = ?1 AND rel = ?2"), params![from_folder, rel, to_folder, new_rel])?;
            }
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
    /// of it; Undo gives them back). `None` lets go. Absolute values, so safe to repeat. An Undo from before a take names
    /// what the take retired: it lands on the record's track, or it would split the record again (CR4). Blocking.
    pub fn set_tracks(&self, files: &[((i64, String), Option<i64>)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        for ((folder, rel), track) in files {
            let track = track.map(|t| taken_by(&tx, t)).transpose()?;
            tx.execute("UPDATE files SET track = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, track])?;
        }
        tx.commit()?;
        drop(db);
        if !files.is_empty() {
            self.changed_outside_pass();
        }
        Ok(())
    }

    /// What the catalogue knows of and has no file for: libraries out of use, tracks with nothing but files gone a while, and
    /// tracks retired that nothing took. Blocking.
    pub fn missing(&self) -> Result<Missing> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, path, (SELECT count(*) FROM files f WHERE f.folder = folders.id) FROM folders WHERE id < 0 ORDER BY path")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut since = db.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut parked = Vec::with_capacity(rows.len());
        for (id, path, files) in rows {
            let at: Option<String> = since.query_row([format!("parked:{id}")], |r| r.get(0)).optional()?;
            parked.push(ParkedLibrary { path, since: at.and_then(|v| v.parse().ok()), files: files as u64 });
        }
        let tag = |name: &str| format!("(SELECT value FROM tags t WHERE t.folder = f.folder AND t.rel = f.rel AND t.name = '{name}' ORDER BY t.n LIMIT 1)");
        let mut st = db.prepare(&format!(
            "SELECT f.track, {}, {}, {}, f.rel, max(f.gone_at) FROM files f WHERE f.gone_at IS NOT NULL AND f.track IS NOT NULL AND f.folder >= 0
               AND NOT EXISTS (SELECT 1 FROM files o WHERE o.track = f.track AND o.gone_at IS NULL)
             GROUP BY f.track ORDER BY max(f.gone_at) DESC, f.track",
            tag("title"),
            tag("artist"),
            tag("album")
        ))?;
        let row = |r: &rusqlite::Row| -> rusqlite::Result<MissingTrack> { Ok(MissingTrack { track: r.get(0)?, title: r.get(1)?, artist: r.get(2)?, album: r.get(3)?, rel: r.get(4)?, since: r.get(5)? }) };
        let gone = st.query_map([], row)?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare("SELECT id, was_title, was_artist, was_album, was_rel, retired_at FROM tracks WHERE retired_at IS NOT NULL AND id NOT IN (SELECT old FROM track_moved) ORDER BY retired_at DESC, id")?;
        let retired = st.query_map([], row)?.collect::<rusqlite::Result<_>>()?;
        Ok(Missing { parked, gone, retired })
    }

    /// A person says a track on Health's Missing list is `stays` (A5): what no rule could pair, a retag and a rename at once, say. It
    /// goes as a take goes, for good, so what names it follows it at the next check. Refused for what still has a file to play, what
    /// went already, and a track that is itself missing: a person's word is for the track in front of them. Blocking.
    pub fn reconnect(&self, goes: i64, stays: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        anyhow::ensure!(goes != stays, "A track can't be itself");
        let (retired, record): (bool, Option<i64>) = tx.query_row("SELECT retired_at IS NOT NULL, record FROM tracks WHERE id = ?1", [goes], |r| Ok((r.get(0)?, r.get(1)?))).optional()?.context("That track is not known")?;
        anyhow::ensure!(taken_by(&tx, goes)? == goes, "That track was taken already");
        let here = |track: i64| -> Result<bool> { Ok(tx.prepare_cached(&format!("SELECT 1 FROM files WHERE track = ?1 AND {PRESENT}"))?.exists([track])?) };
        anyhow::ensure!(!here(goes)?, "That track has a file");
        anyhow::ensure!(here(stays)?, "That track has no file to take it");
        // What it was stays as it retired: its files went then, and it would be read from nothing.
        if retired {
            tx.execute("UPDATE tracks SET record = NULL, back = NULL WHERE id = ?1", [goes])?;
        } else {
            tx.execute(&format!("UPDATE tracks SET retired_at = ?2, record = NULL, back = NULL, {REMEMBER} WHERE id = ?1"), params![goes, now()])?;
        }
        taken_for_good(&tx, goes, stays)?;
        tx.execute("UPDATE records SET retired_at = ?2 WHERE id = ?1 AND retired_at IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = records.id)", params![record, now()])?;
        tx.commit()?;
        drop(db);
        self.changed_outside_pass();
        Ok(())
    }

    /// The tracks taken whose user data has not followed yet: (what went, what took it). Blocking.
    pub fn tracks_to_follow(&self) -> Result<Vec<(i64, i64)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT old, id FROM track_moved WHERE followed = 0")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What `tracks_to_follow` gave has been followed. One taken again meanwhile has another row now, and waits. Blocking.
    pub fn tracks_followed(&self, moved: &[(i64, i64)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        for (old, id) in moved {
            tx.execute("UPDATE track_moved SET followed = 1 WHERE old = ?1 AND id = ?2", params![old, id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every track taken, and what took it: what other stores' ids go by. Blocking.
    pub fn moved_tracks(&self) -> Result<HashMap<i64, i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT old, id FROM track_moved")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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

    /// A file's stable track id as served (`tr-N`, 0101): the record's track, whichever copy the file is (amended
    /// 2026-11-04), by the library it is in (its root as set) and its path there. None: no file there with a track yet,
    /// or no such library in use. Blocking.
    #[allow(dead_code)] // the playlists' entries as tracks (0101 point 12, 632's stage 2)
    pub fn track_by_path(&self, folder: &str, rel: &str) -> Option<String> {
        let at = self.folders().id_of(std::path::Path::new(folder))?;
        self.track_of(at, rel).map(crate::serve::ids::song)
    }

    /// The stable track id a playlist line names, as served: where the line points ([`Self::place_of_line`]), then that
    /// file's track. Blocking.
    #[allow(dead_code)] // the playlists' entries as tracks (0101 point 12, 632's stage 2)
    pub fn track_of_line(&self, line: &str) -> Option<String> {
        let (folder, rel) = self.place_of_line(line)?;
        self.track_of(folder, &rel).map(crate::serve::ids::song)
    }

    /// What a track is or was (0101 point 11: a missing entry shows it): its title, artist, album, MusicBrainz recording
    /// and length, from its file while it has one (here, or gone a while), else as kept when it retired. None: no such
    /// track. Blocking.
    #[allow(dead_code)] // playlists' missing entries (632's stage 2)
    pub fn remembered(&self, track: i64) -> Option<Remembered> {
        let db = self.db();
        let file: Option<(i64, String, Option<i64>)> = db
            .query_row(
                "SELECT folder, rel, duration_ms FROM files WHERE track = ?1 ORDER BY gone_at IS NOT NULL LIMIT 1",
                [track],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .ok()
            .flatten();
        if let Some((folder, rel, duration_ms)) = file {
            let tag = |name: &str| {
                db.query_row(
                    "SELECT value FROM tags WHERE folder = ?1 AND rel = ?2 AND name = ?3 ORDER BY n LIMIT 1",
                    params![folder, rel, name],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .ok()
                .flatten()
            };
            return Some(Remembered {
                title: tag("title"),
                artist: tag("artist"),
                album: tag("album"),
                recording: tag("musicbrainz_trackid"),
                duration_ms,
                retired: false,
            });
        }
        db.query_row(
            "SELECT was_title, was_artist, was_album, was_recording, was_ms, retired_at IS NOT NULL FROM tracks WHERE id = ?1",
            [track],
            |r| Ok(Remembered { title: r.get(0)?, artist: r.get(1)?, album: r.get(2)?, recording: r.get(3)?, duration_ms: r.get(4)?, retired: r.get(5)? }),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Where a file is in the catalogue's terms: its folder's id and its path there; None outside them.
    pub fn place_of_path(&self, path: &Path) -> Option<(i64, String)> {
        self.folders().iter().find_map(|(i, f)| {
            let rel = path.strip_prefix(f).ok()?.to_string_lossy().replace('\\', "/");
            (!rel.is_empty()).then_some((i, rel))
        })
    }

    /// The file arriving at `path` (not walked yet: a better copy Cratarr is putting in its old one's place, or its Undo)
    /// takes `track` when it is identified, rather than a rule's match or a new id (0101 point 11: a better copy hands
    /// its id over as Replace does); `replaced`, the file that held it there, lets go once gone. Both callers place within
    /// one folder. Forgotten after a day unclaimed. Blocking.
    pub fn hand_over(&self, path: &Path, replaced: &Path, track: i64) -> Result<()> {
        let Some((folder, rel)) = self.place_of_path(path) else {
            anyhow::bail!("{} is in no library", path.display());
        };
        let replaces = self.place_of_path(replaced).filter(|(f, _)| *f == folder).map(|(_, r)| r);
        self.db().execute(
            "INSERT OR REPLACE INTO handovers (folder, rel, track, at, replaces) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![folder, rel, track, now(), replaces],
        )?;
        Ok(())
    }

    /// The track a server's item id stands for (decision 0012), when the item is linked to a file the catalogue
    /// counts. Cratarr's own source (decision 0032) needs no links: its ids are the track ids. Blocking.
    pub fn track_of_source(&self, source: &str, item: &str) -> Option<i64> {
        track_of_source(&self.db(), source, item)
    }

    /// The file a server's item names (decision 0012): for another server its link says one file, whichever copy that is;
    /// Cratarr's own ids name a track, so any present file. Blocking.
    pub fn place_of_source(&self, source: &str, item: &str) -> Option<(i64, String)> {
        place_of_source(&self.db(), source, item)
    }

    /// Every present file of a track, by folder and path: a file in each copy of it. Blocking.
    pub fn files_of(&self, track: i64) -> Vec<(i64, String)> {
        files_of(&self.db(), track)
    }

    /// Each of these server tracks and what may play of it, nearest first: Cratarr's own ids every present file, another
    /// server's the one its item names, each with its length; up to the first it has no file of. Which plays is the
    /// listener's (`Listener::among`). In one hold of the catalogue (0.10.2: the slow answers of 2026-10-30): a wind
    /// through the queue looked up its 64 tracks with three takes of the lock each, 192 waits behind whatever else held
    /// it, before its headers went. Blocking.
    pub fn files_of_sources(&self, items: &[(&str, &str)]) -> Vec<(i64, Vec<(i64, String, Option<u64>)>)> {
        let db = self.db();
        items
            .iter()
            .map_while(|(source, item)| {
                let track = track_of_source(&db, source, item)?;
                let files = if *source == crate::source::cratarr::KIND.id { files_of(&db, track) } else { place_of_source(&db, source, item).into_iter().collect() };
                let files: Vec<_> = files
                    .into_iter()
                    .map(|(folder, rel)| {
                        let duration_ms = duration_of(&db, folder, &rel);
                        (folder, rel, duration_ms)
                    })
                    .collect();
                (!files.is_empty()).then_some((track, files))
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
            if let Some(dir) = folders.get(folder) {
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
    ///
    /// A file already analysed as it is (the same size and mtime, this version or newer) keeps its analysis and only
    /// takes what it lacks (0.15: rhythm, the library measured again on the PC): its beats may be Beat this!'s and its
    /// shapes worked out since, which a whole new row would throw away. Returns how many only took what they lacked.
    pub fn store_analyses(&self, items: &[(i64, String, u64, i64, crate::analysis::Analysis)]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let now = now_secs();
        let mut filled = 0;
        for (folder, rel, size, mtime, a) in items {
            let kept = tx
                .prepare_cached("SELECT 1 FROM analysis WHERE folder = ?1 AND rel = ?2 AND size = ?3 AND mtime = ?4 AND version >= ?5")?
                .exists(params![folder, rel, *size as i64, mtime, a.version])?;
            if kept {
                tx.prepare_cached("UPDATE analysis SET four = coalesce(?3, four) WHERE folder = ?1 AND rel = ?2")?
                    .execute(params![folder, rel, a.rhythm.map(|r| r.four)])?;
                filled += 1;
            } else {
                store_analysis(&tx, *folder, rel, *size, *mtime, a, now)?;
            }
        }
        tx.commit()?;
        Ok(filled)
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
                "INSERT OR REPLACE INTO beat_this (folder, rel, size, mtime, beats, downbeats, at, meter, meter_sure)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for r in rows {
                let (meter, sure) = meter_of(&r.beats, &r.downbeats);
                st.execute(params![r.folder, r.rel, r.size as i64, r.mtime, millis(&r.beats), millis(&r.downbeats), now, meter, sure])?;
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
            if let Some(root) = folders.get(folder) {
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
            let Some(root) = folders.get(folder) else { continue };
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
        let root = self.folders().get(folder).cloned()?;
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
            let Some(root) = folders.get(folder) else { continue };
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
            .query_row("SELECT folder, rel FROM files WHERE track = ?1 ORDER BY gone_at IS NOT NULL, folder, rel LIMIT 1", [track], |r| Ok((r.get(0)?, r.get(1)?)))
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
            .map(|(i, p)| {
                Ok(FolderStatus {
                    path: p.display().to_string(),
                    files: count(&format!("SELECT COUNT(*) FROM files WHERE folder = ?1 AND {PRESENT}"), &[&i])?,
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
fn line_in(folders: &InUse, folder: i64, rel: &str) -> Option<String> {
    let (first, root) = folders.first()?;
    if folder == first {
        return Some(rel.to_string());
    }
    crate::playlists::relative_to(root, &folders.get(folder)?.join(rel))
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
        for folder in cat.folders().ids() {
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
pub(crate) async fn link_source(cat: &Arc<Catalogue>, id: &str, source: &Arc<dyn Source>) -> Result<()> {
    if !source.lists_tracks() {
        return Ok(());
    }
    let libraries = source.libraries().await?;
    let started = now();
    let last_full: i64 = cat.meta(&format!("full:{id}")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let last_pass: i64 = cat.meta(&format!("pass:{id}")).and_then(|v| v.parse().ok()).unwrap_or(0);
    // Also when the folders changed since the last full listing: a folder added later (Lidarr's, 2026-10-14) got
    // only the tracks the server changed since, and its older ones stayed unplayable until the next day's listing.
    let folders = cat.folders().paths().map(|f| f.display().to_string()).collect::<Vec<_>>().join("
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
    rows_in(db, folder, dir, PRESENT)
}

/// `served_rows_in`, of the files `which` says, a condition on `files` as `f`.
fn rows_in(db: &Connection, folder: i64, dir: Option<&str>, which: &str) -> Result<Vec<ServedRow>> {
    let range = rel_range(dir);
    let (lo, hi) = range.clone().unzip();
    let within = |col: &str| if range.is_some() { format!(" AND {col} >= ?2 AND {col} < ?3") } else { " AND ?2 IS NULL AND ?3 IS NULL".to_string() };
    let mut rows: Vec<ServedRow> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut st = db.prepare(&format!(
        "SELECT f.rel, f.track, t.born, f.format, f.bitrate, f.duration_ms, f.size, f.mtime, f.bits, f.rate, f.came, t.record,
           (SELECT count(*) > 1 FROM files o WHERE o.track = f.track)
         FROM files f LEFT JOIN tracks t ON t.id = f.track WHERE f.folder = ?1 AND {which} AND {}{}",
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
            bits: r.get(8)?,
            rate: r.get(9)?,
            came: r.get(10)?,
            record: r.get(11)?,
            shared: r.get(12)?,
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
    "a.version, a.duration_secs, a.loudness_lufs, a.track_gain_db, a.track_peak, s.blocks, a.bpm, a.bpm_confidence, s.waveform, a.complete, a.four";
/// The join `ANALYSIS_COLS` needs, after `analysis a`.
const SHAPE_JOIN: &str = "LEFT JOIN analysis_shape s ON s.folder = a.folder AND s.rel = a.rel";

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The time signature of Beat this!'s beats and downbeats (milliseconds): beats a bar (0: none sure) and how sure.
fn meter_of(beats: &[u32], downbeats: &[u32]) -> (i64, f64) {
    let secs = |v: &[u32]| v.iter().map(|m| f64::from(*m) / 1000.0).collect::<Vec<f64>>();
    crate::analysis::rhythm::meter(&secs(beats), &secs(downbeats)).map_or((0, 0.0), |(n, sure)| (i64::from(n), f64::from(sure)))
}

/// The time signatures of the files Beat this! heard before they were worked out (0.15), once. Safe to repeat.
fn fill_meters(conn: &Connection) -> Result<()> {
    let rows: Vec<(i64, String, Vec<u8>, Vec<u8>)> = conn
        .prepare("SELECT folder, rel, beats, downbeats FROM beat_this WHERE meter IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if rows.is_empty() {
        return Ok(());
    }
    let ms = |b: &[u8]| b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect::<Vec<u32>>();
    let tx = conn.unchecked_transaction()?;
    {
        let mut st = tx.prepare("UPDATE beat_this SET meter = ?3, meter_sure = ?4 WHERE folder = ?1 AND rel = ?2")?;
        for (folder, rel, beats, downbeats) in &rows {
            let (meter, sure) = meter_of(&ms(beats), &ms(downbeats));
            st.execute(params![folder, rel, meter, sure])?;
        }
    }
    tx.commit()?;
    tracing::info!("catalogue: time signatures worked out for {} files", rows.len());
    Ok(())
}

fn store_analysis(db: &Connection, folder: i64, rel: &str, size: u64, mtime: i64, a: &crate::analysis::Analysis, now: i64) -> Result<()> {
    db.prepare_cached(
        "INSERT OR REPLACE INTO analysis (folder, rel, size, mtime, version, duration_secs, loudness_lufs, track_gain_db,
           track_peak, bpm, bpm_confidence, waveform, blocks, analysed_at, complete, four)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12, ?13, ?14)",
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
        a.complete,
        a.rhythm.map(|r| r.four)
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
        rhythm: r.get::<_, Option<f64>>(at + 10)?.map(|f| crate::analysis::rhythm::Rhythm { four: f as f32 }),
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
        // An id a take retired stands for the track that took it: a queue, an export or a crate can name it long after.
        let track = taken_by(db, crate::serve::ids::track_of(item)?).ok()?;
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

/// A file's item on a source. Cratarr's own item is the track, whichever copy the file is: its links come from `songs`, which
/// holds a track's row copy alone, so a playlist line on any other copy would have no id and stop playing.
fn item_at(db: &Connection, source: &str, folder: i64, rel: &str) -> Result<Option<String>> {
    if source == crate::source::cratarr::KIND.id {
        let mut st = db.prepare_cached(&format!("SELECT track FROM files WHERE folder = ?1 AND rel = ?2 AND {PRESENT}"))?;
        return Ok(st.query_row(params![folder, rel], |r| r.get::<_, Option<i64>>(0)).optional()?.flatten().map(crate::serve::ids::song));
    }
    let mut st = db.prepare_cached("SELECT track_id FROM source_ids WHERE source = ?1 AND folder = ?2 AND rel = ?3 LIMIT 1")?;
    Ok(st.query_row(params![source, folder, rel], |r| r.get::<_, String>(0)).optional()?)
}

/// The line of a track's `row` file where that is among its `files` as they are now, else the first that has one.
fn row_line_of(folders: &InUse, files: &[Place], row: Option<&Place>) -> Option<String> {
    row.filter(|r| files.contains(r)).and_then(|(f, rel)| line_in(folders, *f, rel)).or_else(|| files.iter().find_map(|(f, rel)| line_in(folders, *f, rel)))
}

/// See `Catalogue::file_of`.
fn file_of(db: &Connection, track: i64) -> Option<(i64, String)> {
    db.query_row(&format!("SELECT folder, rel FROM files WHERE track = ?1 AND {PRESENT} ORDER BY folder, rel LIMIT 1"), [track], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()
}

/// See `Catalogue::files_of`.
fn files_of(db: &Connection, track: i64) -> Vec<(i64, String)> {
    let Ok(mut st) = db.prepare_cached(&format!("SELECT folder, rel FROM files WHERE track = ?1 AND {PRESENT} ORDER BY folder, rel")) else { return Vec::new() };
    st.query_map([track], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// See `Catalogue::place_of_source`.
fn place_of_source(db: &Connection, source: &str, item: &str) -> Option<(i64, String)> {
    if source == crate::source::cratarr::KIND.id {
        return file_of(db, track_of_source(db, source, item)?);
    }
    db.query_row(
        &format!(
            "SELECT f.folder, f.rel FROM source_ids s JOIN files f ON f.folder = s.folder AND f.rel = s.rel
             WHERE s.source = ?1 AND s.track_id = ?2 AND f.track IS NOT NULL AND {}",
            PRESENT_F
        ),
        params![source, item],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
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
        // Capped by kind: one of each, whatever the first kind holds.
        cat.db().execute("UPDATE files SET issue = 'no_length' WHERE rel = 'A - B (2020)/02 Two.flac'", []).unwrap();
        let kinds: Vec<String> = cat.problems(1).unwrap().into_iter().map(|p| p.kind).collect();
        assert_eq!(kinds, ["broken", "no_length"]);
        cat.db().execute("UPDATE files SET issue = 'broken' WHERE rel = 'A - B (2020)/02 Two.flac'", []).unwrap();
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
            rhythm: None,
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

    /// A folder's first file given an analysis, with its shape, and Beat this!'s beats.
    fn heard(cat: &Catalogue, folder: i64) -> (String, BeatThisRow) {
        let (_, one, _, size, mtime) = cat.unanalysed(10).unwrap().into_iter().find(|t| t.0 == folder && t.1.ends_with("One.mp3")).unwrap();
        cat.store_analysis(folder, &one, size, mtime, &an_analysis(-3.0, 500)).unwrap();
        let row = BeatThisRow { folder, rel: one.clone(), size, mtime, beats: vec![500, 1_000], downbeats: vec![500] };
        cat.store_beat_this(std::slice::from_ref(&row)).unwrap();
        (one, row)
    }

    /// How many rows of a file's shape and beats are kept at a place.
    fn parts_at(cat: &Catalogue, folder: i64, rel: &str) -> (i64, i64) {
        let count = |table: &str| cat.db().query_row(&format!("SELECT COUNT(*) FROM {table} WHERE folder = ?1 AND rel = ?2"), params![folder, rel], |r| r.get(0)).unwrap();
        (count("analysis_shape"), count("beat_this"))
    }

    /// Beats left behind would list the file for hearing again, and a row a walk left in the way would break the shape's key.
    #[test]
    fn a_moved_file_keeps_its_beats_and_shape() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let (one, row) = heard(&cat, 0);
        let moved = one.replace("A - B (2020)", "A - B (2020) moved");
        cat.store_beat_this(&[BeatThisRow { rel: moved.clone(), beats: vec![9], ..row }]).unwrap();
        cat.db().execute("INSERT INTO analysis_shape (folder, rel) VALUES (0, ?1)", [&moved]).unwrap();
        cat.move_files(0, "A - B (2020)", 0, "A - B (2020) moved").unwrap();
        assert_eq!(cat.beat_this_of(0, &moved).map(|(beats, _)| beats), Some(vec![0.5, 1.0]), "its own beats, not what stood there");
        assert_eq!(cat.analysis_of(0, &moved).unwrap().waveform.len(), 512, "its shape came along");
        assert_eq!(parts_at(&cat, 0, &one), (0, 0), "nothing left behind");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_parked_folder_keeps_its_beats_and_shape_and_a_forgotten_one_leaves_none() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        let (one, _) = heard(&cat, 0);
        cat.set_folders(vec![b.clone()]).unwrap();
        assert_eq!(parts_at(&cat, 0, &one), (0, 0), "parked, with its files");
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        assert_eq!(parts_at(&cat, 0, &one), (1, 1), "back, with them");
        assert!(cat.beat_this_of(0, &one).is_some());
        cat.set_folders(vec![b.clone()]).unwrap();
        std::fs::remove_dir_all(a.join("A - B (2020)")).unwrap();
        std::fs::write(a.join("other.mp3"), "x").unwrap();
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        let left: i64 = cat.db().query_row("SELECT (SELECT COUNT(*) FROM analysis_shape) + (SELECT COUNT(*) FROM beat_this)", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "its files gone from its path: forgotten, and all of theirs with them");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
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

    pub(super) fn retired(cat: &Catalogue, track: i64) -> bool {
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

    /// 0101 point 11: a retired track keeps what it was (its tags go with its file), and a file that is it again, days
    /// later and elsewhere, takes its id back rather than a new one.
    #[test]
    fn a_retired_track_keeps_what_it_was_and_comes_back_as_itself() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        for (n, (name, value)) in [("title", "One"), ("artist", "A"), ("album", "B"), ("musicbrainz_trackid", "rec-1")].into_iter().enumerate() {
            cat.db()
                .execute(
                    "INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, 'A - B (2020)/01 One.mp3', ?1, ?2, ?2, ?3)",
                    params![n as i64, name, value],
                )
                .unwrap();
        }
        assert_eq!(cat.remembered(one).unwrap().title.as_deref(), Some("One"), "from its file while it has one");
        std::fs::remove_file(d.join("A - B (2020)/01 One.mp3")).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
        cat.identify().unwrap();
        assert!(retired(&cat, one));
        let r = cat.remembered(one).unwrap();
        assert_eq!(
            (r.title.as_deref(), r.artist.as_deref(), r.album.as_deref(), r.recording.as_deref(), r.duration_ms, r.retired),
            (Some("One"), Some("A"), Some("B"), Some("rec-1"), Some(200_000), true),
            "kept as it retired"
        );
        // The same recording, back elsewhere days later: the old id.
        std::fs::create_dir_all(d.join("Elsewhere")).unwrap();
        std::fs::write(d.join("Elsewhere/1-01 Uno.mp3"), b"again").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.db()
            .execute(
                "INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, 'Elsewhere/1-01 Uno.mp3', 0, 'musicbrainz_trackid', 'musicbrainz_trackid', 'rec-1')",
                [],
            )
            .unwrap();
        assert_eq!(cat.identify().unwrap(), (1, 0), "recognised as the retired track: no new id");
        assert_eq!(cat.track_of(0, "Elsewhere/1-01 Uno.mp3"), Some(one));
        assert!(!retired(&cat, one), "back in use");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A copy keeps its own date (docs/records-and-copies.md): the row Cratarr moves, and a file that takes a gone row's
    /// place, whichever way. A date made anew, or read from the track, would be another copy's once tracks are shared.
    #[test]
    fn a_file_keeps_when_it_came_across_a_move() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let came = |rel: &str| cat.db().query_row("SELECT came FROM files WHERE folder = 0 AND rel = ?1", [rel], |r| r.get::<_, Option<i64>>(0)).unwrap();
        assert!(came("A - B (2020)/01 One.mp3").is_some_and(|at| at > 1_000_000_000), "a new file came now");
        // Long ago, and not when the track was born: a date made anew or read from the track would show.
        cat.db().execute_batch("UPDATE files SET came = 100; UPDATE tracks SET born = 50;").unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        // Renamed outside Cratarr: a new row pairs with the gone one.
        std::fs::rename(d.join("A - B (2020)"), d.join("A - B")).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (2, 0));
        assert_eq!(came("A - B/01 One.mp3"), Some(100), "renamed outside");
        // Moved by Cratarr: the same row.
        std::fs::rename(d.join("A - B"), d.join("Moved")).unwrap();
        cat.move_files(0, "A - B", 0, "Moved").unwrap();
        assert_eq!(came("Moved/01 One.mp3"), Some(100), "moved by Cratarr");
        // A better copy handed the track.
        std::fs::remove_file(d.join("Moved/01 One.mp3")).unwrap();
        std::fs::write(d.join("Moved/01 One.flac"), b"better").unwrap();
        cat.hand_over(&d.join("Moved/01 One.flac"), &d.join("Moved/01 One.mp3"), one).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (1, 0));
        assert_eq!(came("Moved/01 One.flac"), Some(100), "handed its track");
        assert_eq!(cat.served_rows(0).unwrap().iter().find(|r| r.rel == "Moved/01 One.flac").and_then(|r| r.came), Some(100), "and served so");
        // Retired and back days later: its row went, so when the track was born stands in.
        std::fs::remove_file(d.join("Moved/01 One.flac")).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
        cat.identify().unwrap();
        assert!(retired(&cat, one));
        std::fs::create_dir_all(d.join("Back")).unwrap();
        std::fs::write(d.join("Back/01 One.flac"), b"again").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (1, 0), "the retired track");
        assert_eq!(came("Back/01 One.flac"), Some(50), "back from retirement");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A catalogue from before 0.16: its files take their tracks' dates, once. Filled again later, a date would be the
    /// track's, which by then may be another copy's.
    #[test]
    fn a_file_from_before_came_when_its_track_was_born_and_only_once() {
        let d = music();
        let file = d.join("catalogue.sqlite");
        let (one, two) = ("A - B (2020)/01 One.mp3", "A - B (2020)/02 Two.flac");
        let came = |cat: &Catalogue, rel: &str| cat.db().query_row("SELECT came FROM files WHERE folder = 0 AND rel = ?1", [rel], |r| r.get::<_, Option<i64>>(0)).unwrap();
        {
            let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
            cat.walk_all();
            read_as_audio(&cat);
            cat.identify().unwrap();
            cat.db().execute_batch("UPDATE files SET came = NULL; UPDATE tracks SET born = 50; DELETE FROM meta WHERE key = 'came';").unwrap();
            cat.db().execute("UPDATE files SET track = NULL WHERE rel = ?1", [two]).unwrap();
        }
        let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        assert_eq!(came(&cat, one), Some(50));
        assert!(came(&cat, two).is_some_and(|at| at > 1_000_000_000), "no track yet: it comes now");
        cat.db().execute_batch("UPDATE files SET came = NULL;").unwrap();
        drop(cat);
        let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        assert_eq!(came(&cat, one), None, "once");
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    /// N7: only lossless copies rank by bit depth and rate, and reading every file again takes hours on the NAS. What
    /// waits for it is served as it was read.
    #[test]
    fn reader_9_rereads_lossless_files_only() {
        let d = music();
        let file = d.join("catalogue.sqlite");
        {
            let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
            for (rel, format, kbps) in [("a.flac", "FLAC", 900), ("b.mp3", "MP3", 320), ("c.m4a", "MP4", 1000), ("d.m4a", "MP4", 256), ("e.opus", "OPUS", 128), ("f.wav", "WAV", 1411)] {
                cat.db().execute("INSERT INTO files (folder, rel, size, mtime, state, format, bitrate) VALUES (0, ?1, 1, 1, ?2, ?3, ?4)", params![rel, READ, format, kbps]).unwrap();
            }
            cat.set_meta("reader", "8");
        }
        let cat = Catalogue::open(Some(&file), vec![d.clone()]).unwrap();
        let waiting: Vec<String> = cat.db().prepare("SELECT rel FROM files WHERE state = ?1 ORDER BY rel").unwrap().query_map([PENDING], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(waiting, ["a.flac", "c.m4a", "f.wav"]);
        assert_eq!(cat.meta("reader").as_deref(), Some("9"));
        assert_eq!(cat.served_rows(0).unwrap().len(), 6, "served as read until then");
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    /// What `copy_for` ranks lossless copies by comes from the file, where its read lands.
    #[test]
    fn a_read_keeps_a_files_bit_depth_and_rate() {
        let d = music();
        let (rate, block, bits) = (48_000u32, 6u16, 24u16);
        let audio = vec![0u8; rate as usize * usize::from(block) / 2];
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + audio.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        wav.extend_from_slice(&block.to_le_bytes());
        wav.extend_from_slice(&bits.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(audio.len() as u32).to_le_bytes());
        wav.extend_from_slice(&audio);
        std::fs::write(d.join("A - B (2020)/03 Three.wav"), wav).unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.read_tags_batch(10).unwrap();
        let rows = cat.served_rows(0).unwrap();
        let row = rows.iter().find(|r| r.rel.ends_with("Three.wav")).expect("read");
        assert_eq!((row.bits, row.rate), (Some(24), Some(48_000)));
        std::fs::remove_dir_all(&d).ok();
    }

    pub(super) fn copy(release: Option<&str>, artist: &str, title: &str, tracks: &[(i64, &str, i64)]) -> AlbumCopy {
        AlbumCopy {
            facts: identity::RecordFacts {
                release: release.map(str::to_string),
                artist: identity::fold(artist),
                title: identity::fold(title),
                tracks: tracks.iter().map(|(_, t, ms)| identity::Listed::of(t, *ms)).collect(),
            },
            title: title.to_string(),
            artist: artist.to_string(),
            year: Some(2020),
            tracks: tracks.iter().enumerate().map(|(i, (id, _, _))| (*id, None, Some(i as u32 + 1))).collect(),
            files: Vec::new(),
            ..Default::default()
        }
    }

    /// 0100: the same tracks are one record, a deluxe edition its own; a track keeps its record, a new one joins.
    #[test]
    fn albums_get_their_records() {
        let cat = Catalogue::open(None, vec![]).unwrap();
        for _ in 0..7 {
            cat.db().execute("INSERT INTO tracks (born) VALUES (1)", []).unwrap();
        }
        let record = |t: i64| cat.db().query_row("SELECT record FROM tracks WHERE id = ?1", [t], |r| r.get::<_, Option<i64>>(0)).unwrap();
        assert!(cat.records_unsettled());
        let music = copy(None, "A", "B", &[(1, "One", 200_000), (2, "Two", 180_000)]);
        let own = copy(None, "A", "B", &[(3, "One", 201_000), (4, "Two", 180_000)]);
        let deluxe = copy(None, "A", "B", &[(5, "One", 200_000), (6, "Two", 180_000), (7, "Bonus", 90_000)]);
        assert_eq!(cat.assign_records(&[music.clone(), own.clone(), deluxe.clone()]).unwrap(), 7);
        assert!(!cat.records_unsettled());
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(3, 1), (4, 2)]), "the same tracks: one record, on one set of them");
        assert_ne!(record(1), record(5), "a deluxe edition is its own");
        let first = record(1);
        let (title, year): (String, u32) = cat.db().query_row("SELECT title, year FROM records WHERE id = ?1", [first], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((title.as_str(), year), ("B", 2020));
        let own = copy(None, "A", "B", &[(1, "One", 201_000), (2, "Two", 180_000)]);
        assert_eq!(cat.assign_records(&[music, own, deluxe]).unwrap(), 0, "for life: nothing to give again");
        cat.db().execute("INSERT INTO tracks (born) VALUES (1)", []).unwrap();
        let grown = copy(None, "A", "B", &[(1, "One", 200_000), (2, "Two", 180_000), (8, "Three", 100_000)]);
        assert_eq!(cat.assign_records(&[grown]).unwrap(), 1);
        assert_eq!(record(8), record(1), "a new track joins its album's record");
        let gone = record(5).unwrap();
        cat.db().execute("UPDATE tracks SET retired_at = 5 WHERE record = ?1", [gone]).unwrap();
        cat.assign_records(&[]).unwrap();
        let retired = |r: i64| cat.db().query_row("SELECT retired_at IS NOT NULL FROM records WHERE id = ?1", [r], |x| x.get::<_, bool>(0)).unwrap();
        assert!(retired(gone), "its last track went");
        cat.db().execute("UPDATE tracks SET retired_at = NULL WHERE id = 5", []).unwrap();
        cat.assign_records(&[]).unwrap();
        assert!(!retired(gone), "a track came back");
    }

    /// `n` tracks with no files.
    pub(super) fn tracks(n: usize) -> Catalogue {
        let cat = Catalogue::open(None, vec![]).unwrap();
        for _ in 0..n {
            cat.db().execute("INSERT INTO tracks (born) VALUES (1)", []).unwrap();
        }
        cat
    }

    pub(super) fn record_of(cat: &Catalogue, track: i64) -> Option<i64> {
        cat.db().query_row("SELECT record FROM tracks WHERE id = ?1", [track], |r| r.get(0)).unwrap()
    }

    fn record_retired(cat: &Catalogue, record: i64) -> bool {
        cat.db().query_row("SELECT retired_at IS NOT NULL FROM records WHERE id = ?1", [record], |r| r.get(0)).unwrap()
    }

    fn records_made(cat: &Catalogue) -> i64 {
        cat.db().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap()
    }

    /// These tracks retired, as their last files' day passing leaves them.
    fn retire(cat: &Catalogue, ids: &[i64]) {
        for id in ids {
            cat.db().execute("UPDATE tracks SET retired_at = 5 WHERE id = ?1", [id]).unwrap();
        }
    }

    /// These tracks back from retirement, as `identify` leaves them.
    fn come_back(cat: &Catalogue, ids: &[i64]) {
        for id in ids {
            cat.db().execute("UPDATE tracks SET retired_at = NULL, back = 1 WHERE id = ?1", [id]).unwrap();
        }
    }

    /// A file of a track, as `identify` leaves it.
    pub(super) fn file_of_track(cat: &Catalogue, rel: &str, track: i64) {
        cat.db().execute("INSERT INTO files (folder, rel, size, mtime, track, state) VALUES (0, ?1, 1, 1, ?2, ?3)", params![rel, track, READ]).unwrap();
    }

    /// One album of `tracks`, each given a title and length by its id.
    fn album(title: &str, tracks: &[i64]) -> AlbumCopy {
        let names: Vec<String> = tracks.iter().map(|t| format!("Track {t}")).collect();
        let list: Vec<(i64, &str, i64)> = tracks.iter().zip(&names).map(|(t, n)| (*t, n.as_str(), 200_000)).collect();
        copy(None, "A", title, &list)
    }

    /// A file of a best-of takes a retired track's id back by its name. One track of three is no vote: the album is a
    /// record of its own, and neither takes the old record's id, favourites and facts nor brings it back.
    #[test]
    fn one_returned_track_does_not_pull_an_album_into_its_record() {
        let d = music();
        // Another album stays: a folder that walks to nothing keeps its rows.
        std::fs::create_dir_all(d.join("Stays")).unwrap();
        std::fs::write(d.join("Stays/01 Here.mp3"), b"not really an mp3").unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let (one, two) = (cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap(), cat.track_of(0, "A - B (2020)/02 Two.flac").unwrap());
        let here = cat.track_of(0, "Stays/01 Here.mp3").unwrap();
        cat.assign_records(&[copy(None, "A", "B", &[(one, "One", 200_000), (two, "Two", 200_000)]), copy(None, "C", "Stays", &[(here, "Here", 200_000)])]).unwrap();
        let old = record_of(&cat, one).unwrap();
        std::fs::remove_dir_all(d.join("A - B (2020)")).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
        cat.identify().unwrap();
        cat.assign_records(&[]).unwrap();
        assert!(record_retired(&cat, old));
        std::fs::create_dir_all(d.join("Best Of")).unwrap();
        for name in ["01 One.mp3", "02 Another.mp3", "03 Third.mp3"] {
            std::fs::write(d.join("Best Of").join(name), name).unwrap();
        }
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (1, 2), "its name and length: the old id");
        assert_eq!(cat.track_of(0, "Best Of/01 One.mp3"), Some(one));
        let (another, third) = (cat.track_of(0, "Best Of/02 Another.mp3").unwrap(), cat.track_of(0, "Best Of/03 Third.mp3").unwrap());
        let best = copy(None, "Various", "Best Of", &[(one, "One", 200_000), (another, "Another", 200_000), (third, "Third", 200_000)]);
        assert_eq!(cat.assign_records(&[best.clone()]).unwrap(), 3);
        let new = record_of(&cat, another).unwrap();
        assert_ne!(new, old, "a record of its own");
        assert_eq!(record_of(&cat, one), Some(new), "the track's only file is in it, so the track moved");
        assert!(record_retired(&cat, old), "its old record stays gone");
        assert!(!cat.records_unsettled());
        assert_eq!(cat.assign_records(&[best]).unwrap(), 0, "settled");
        std::fs::remove_dir_all(&d).ok();
    }

    /// One track alone is the whole of its album, so it holds the vote; but it came back by its name, and a single with that
    /// recording is not the record, gone or still in use.
    #[test]
    fn a_returned_track_alone_in_an_album_is_not_its_record() {
        let cat = tracks(3);
        cat.assign_records(&[album("Long", &[1, 2, 3])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        retire(&cat, &[1, 2, 3]);
        cat.assign_records(&[]).unwrap();
        come_back(&cat, &[1]);
        assert!(cat.records_unsettled(), "held against its album, though it has a record");
        cat.assign_records(&[album("Single", &[1])]).unwrap();
        assert_ne!(record_of(&cat, 1), Some(old));
        assert!(record_retired(&cat, old), "gone, it stays gone");

        let cat = tracks(3);
        cat.assign_records(&[album("Long", &[1, 2, 3])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        retire(&cat, &[3]);
        come_back(&cat, &[3]);
        cat.assign_records(&[album("Long", &[1, 2]), album("Single", &[3])]).unwrap();
        assert_ne!(record_of(&cat, 3), Some(old), "in use, it takes in no album either");
        assert_eq!(record_of(&cat, 1), Some(old));
    }

    /// Beside a track that stayed, a track back from retirement is simply home, even in an album of two.
    #[test]
    fn a_returned_track_in_its_own_album_keeps_its_record() {
        let cat = tracks(2);
        cat.assign_records(&[album("Two", &[1, 2])]).unwrap();
        let old = record_of(&cat, 1);
        retire(&cat, &[2]);
        come_back(&cat, &[2]);
        assert_eq!(cat.assign_records(&[album("Two", &[1, 2])]).unwrap(), 0);
        assert_eq!(record_of(&cat, 2), old);
        assert!(!cat.records_unsettled(), "it counts as any other track now");
    }

    /// A folder moved back after its day, or fetched again: most of a record's tracks together are the record.
    #[test]
    fn most_of_a_record_coming_back_brings_it_back() {
        let cat = tracks(3);
        cat.assign_records(&[album("Three", &[1, 2, 3])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        retire(&cat, &[1, 2, 3]);
        cat.assign_records(&[]).unwrap();
        assert!(record_retired(&cat, old));
        come_back(&cat, &[1, 2]);
        assert_eq!(cat.assign_records(&[album("Three", &[1, 2])]).unwrap(), 0);
        assert_eq!(record_of(&cat, 1), Some(old));
        assert!(!record_retired(&cat, old));
    }

    /// These tracks each with a file, as the albums of a test lie.
    fn on_files(cat: &Catalogue, tracks: std::ops::RangeInclusive<i64>) {
        tracks.for_each(|t| file_of_track(cat, &format!("A/{t}.flac"), t));
    }

    /// Half of an album holding a record is no vote while another album holds it too: the album is held against the others
    /// as a new one is, and its tracks go with it.
    #[test]
    fn half_of_an_album_holding_a_record_is_no_vote() {
        let cat = tracks(5);
        on_files(&cat, 1..=5);
        cat.assign_records(&[album("Half", &[1, 2, 3])]).unwrap();
        let old = record_of(&cat, 1);
        assert_eq!(cat.assign_records(&[album("Half", &[1, 2, 4, 5]), album("Rest", &[3])]).unwrap(), 4);
        assert_ne!(record_of(&cat, 4), old);
        assert_eq!(record_of(&cat, 1), record_of(&cat, 4));
        assert_eq!(record_of(&cat, 3), old, "the other album keeps it");
    }

    /// m5, the owner: three tracks of a record and seven added outside Cratarr in one pass are no majority, but nothing else
    /// holds it, so it stays, ten tracks, with all that names it.
    #[test]
    fn an_album_keeps_its_id_when_most_of_it_came_in_at_once() {
        let cat = tracks(10);
        on_files(&cat, 1..=10);
        cat.assign_records(&[album("Grown", &[1, 2, 3])]).unwrap();
        let (kept, made) = (record_of(&cat, 1), records_made(&cat));
        let grown = album("Grown", &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(cat.assign_records(&[grown.clone()]).unwrap(), 7);
        assert!((1..=10).all(|t| record_of(&cat, t) == kept));
        assert_eq!(records_made(&cat), made, "no record born for it");
        assert_eq!(cat.assign_records(&[grown]).unwrap(), 0, "settled");

        // A file of it gone a while holds nothing: what is left of the record is what is here.
        let cat = tracks(11);
        on_files(&cat, 1..=11);
        cat.assign_records(&[album("Grown", &[1, 2, 3, 4])]).unwrap();
        let kept = record_of(&cat, 1);
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE track = 4", []).unwrap();
        cat.assign_records(&[album("Grown", &[1, 2, 3, 5, 6, 7, 8, 9, 10, 11])]).unwrap();
        assert_eq!(record_of(&cat, 11), kept);

        // So does one not read: it is no file here for any list.
        let cat = tracks(11);
        on_files(&cat, 1..=11);
        cat.assign_records(&[album("Grown", &[1, 2, 3, 4])]).unwrap();
        let kept = record_of(&cat, 1);
        cat.db().execute("UPDATE files SET state = ?1 WHERE track = 4", [FAILED]).unwrap();
        cat.assign_records(&[album("Grown", &[1, 2, 3, 5, 6, 7, 8, 9, 10, 11])]).unwrap();
        assert_eq!(record_of(&cat, 11), kept);
    }

    /// Where a record has a file in another album it has somewhere to be, and the majority rule stands.
    #[test]
    fn another_album_holding_the_record_leaves_the_majority_to_stand() {
        let cat = tracks(11);
        on_files(&cat, 1..=11);
        cat.assign_records(&[album("Whole", &[1, 2, 3, 4])]).unwrap();
        let kept = record_of(&cat, 1);
        cat.assign_records(&[album("Grown", &[1, 2, 3, 5, 6, 7, 8, 9, 10, 11]), album("Left", &[4])]).unwrap();
        assert_eq!(record_of(&cat, 4), kept);
        assert_ne!(record_of(&cat, 11), kept);
        assert_eq!(record_of(&cat, 1), record_of(&cat, 11), "its free tracks went with it");
    }

    /// What came back by its name says nothing of where it landed, however little is left of its record.
    #[test]
    fn a_returned_track_is_no_album_that_holds_its_record_alone() {
        let cat = tracks(5);
        on_files(&cat, 1..=5);
        cat.assign_records(&[album("Old", &[1, 2])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        retire(&cat, &[1, 2]);
        cat.db().execute("DELETE FROM files WHERE track = 2", []).unwrap();
        cat.assign_records(&[]).unwrap();
        come_back(&cat, &[1]);
        cat.assign_records(&[album("Best Of", &[1, 3, 4, 5])]).unwrap();
        assert_ne!(record_of(&cat, 3), Some(old));
        assert!(record_retired(&cat, old));
    }

    /// No track of it retired, so nothing else says it went; and left with no tracks and no retirement, it would read as a
    /// record never had, offering Want for what is here under another id.
    #[test]
    fn a_record_whose_tracks_all_moved_away_retires() {
        let cat = tracks(10);
        on_files(&cat, 1..=10);
        cat.assign_records(&[album("Whole", &[1, 2, 3, 4])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        cat.assign_records(&[album("One", &[1, 2, 5, 6, 7]), album("Two", &[3, 4, 8, 9, 10])]).unwrap();
        let new = record_of(&cat, 1).unwrap();
        assert_ne!(new, old);
        assert!(record_retired(&cat, old), "born anew: the tracks left it");
        assert!(!record_retired(&cat, new));
        let title: String = cat.db().query_row("SELECT title FROM records WHERE id = ?1", [old], |r| r.get(0)).unwrap();
        assert_eq!(title, "Whole", "it keeps what it was");

        let cat = tracks(4);
        cat.assign_records(&[album("Lone", &[1]), album("Big", &[2, 3, 4])]).unwrap();
        let (lone, big) = (record_of(&cat, 1).unwrap(), record_of(&cat, 2).unwrap());
        cat.assign_records(&[album("Big", &[2, 3, 4, 1])]).unwrap();
        assert_eq!(record_of(&cat, 1), Some(big));
        assert!(record_retired(&cat, lone), "its one track went to a record already there");
    }

    /// Only a record tracks left retires for having none: one that never had any is one not had, whatever else a pass moves.
    #[test]
    fn a_record_that_never_had_tracks_is_not_retired() {
        let cat = tracks(4);
        cat.db().execute("INSERT INTO records (born, title) VALUES (1, 'Wanted')", []).unwrap();
        let wanted = cat.db().last_insert_rowid();
        cat.assign_records(&[album("Half", &[1, 2])]).unwrap();
        cat.assign_records(&[album("Half", &[1, 2, 3, 4])]).unwrap();
        assert!(!record_retired(&cat, wanted));
    }

    /// Gone before its last track moved on, a record stays gone from when it went.
    #[test]
    fn a_retired_record_keeps_its_date_when_a_returned_track_moves_on() {
        let cat = tracks(3);
        cat.assign_records(&[album("Single", &[1])]).unwrap();
        let old = record_of(&cat, 1).unwrap();
        retire(&cat, &[1]);
        cat.assign_records(&[]).unwrap();
        cat.db().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [old]).unwrap();
        come_back(&cat, &[1]);
        cat.assign_records(&[album("Best Of", &[1, 2, 3])]).unwrap();
        assert_ne!(record_of(&cat, 1), Some(old));
        let at: i64 = cat.db().query_row("SELECT retired_at FROM records WHERE id = ?1", [old], |r| r.get(0)).unwrap();
        assert_eq!(at, 5);
    }

    /// `tracks.record` is for life but for this: a track whose only file went to another record's album.
    #[test]
    fn a_track_moves_when_its_only_file_sits_in_another_records_album() {
        let cat = tracks(6);
        cat.assign_records(&[album("First", &[1, 2, 3]), album("Second", &[4, 5, 6])]).unwrap();
        let (first, second) = (record_of(&cat, 1), record_of(&cat, 4));
        assert_ne!(first, second);
        assert_eq!(cat.assign_records(&[album("First", &[1, 2]), album("Second", &[4, 5, 6, 3])]).unwrap(), 1);
        assert_eq!(record_of(&cat, 3), second);
        let number: i64 = cat.db().query_row("SELECT number FROM tracks WHERE id = 3", [], |r| r.get(0)).unwrap();
        assert_eq!(number, 4, "its place is in that record now");
        assert!(!record_retired(&cat, first.unwrap()), "two tracks are left in it");
    }

    /// A shared track has a copy's file in its own record's album: one file elsewhere does not take it away.
    #[test]
    fn a_track_with_a_file_in_its_own_album_stays_with_its_record() {
        let cat = tracks(6);
        cat.assign_records(&[album("First", &[1, 2, 3]), album("Second", &[4, 5, 6])]).unwrap();
        let first = record_of(&cat, 1);
        file_of_track(&cat, "First/03.flac", 3);
        file_of_track(&cat, "Second/03.flac", 3);
        assert_eq!(cat.assign_records(&[album("First", &[1, 2, 3]), album("Second", &[4, 5, 6, 3])]).unwrap(), 0);
        assert_eq!(record_of(&cat, 3), first);
    }

    /// Shared tracks cannot move, so an album of them with no vote must settle all the same, or a record is born for it at
    /// every build.
    #[test]
    fn an_album_of_shared_tracks_settles_once() {
        let cat = tracks(6);
        cat.assign_records(&[album("First", &[1, 2]), album("Second", &[3, 4])]).unwrap();
        for track in [1, 2, 3, 4] {
            file_of_track(&cat, &format!("Elsewhere/{track}.flac"), track);
            file_of_track(&cat, &format!("Together/{track}.flac"), track);
        }
        let made = records_made(&cat);
        let half_new = album("Half new", &[1, 2, 5, 6]);
        assert_eq!(cat.assign_records(&[half_new.clone()]).unwrap(), 2);
        assert_eq!(records_made(&cat), made + 1);
        assert_eq!(record_of(&cat, 5), record_of(&cat, 6));
        assert_eq!(cat.assign_records(&[half_new]).unwrap(), 0);
        assert_eq!(records_made(&cat), made + 1, "born once");
        assert_eq!(cat.assign_records(&[album("Mixed", &[1, 3])]).unwrap(), 0);
        assert_eq!(records_made(&cat), made + 1, "nothing here to give a record to");
    }

    /// What a few folders' albums settle alone is what their tracks say. Nothing moves there, nothing is born, and a vote
    /// counts as it does in a whole pass.
    #[test]
    fn a_join_gives_only_what_a_vote_says() {
        let cat = tracks(9);
        cat.assign_records(&[album("Grown", &[1, 2]), album("Other", &[3, 4, 5])]).unwrap();
        let made = records_made(&cat);
        assert_eq!(cat.join_records(&[album("Grown", &[1, 2, 6])]).unwrap(), 1);
        assert_eq!(record_of(&cat, 6), record_of(&cat, 1));
        assert_eq!(cat.join_records(&[album("New", &[7, 8])]).unwrap(), 0, "no vote: nothing given");
        assert_eq!(cat.join_records(&[album("Other", &[3, 9, 7, 8])]).unwrap(), 0, "one of four is no vote");
        assert_eq!(record_of(&cat, 9), None);
        assert_eq!(cat.join_records(&[album("Other", &[1, 3, 4, 5])]).unwrap(), 0, "a stray stays for the whole pass");
        assert_ne!(record_of(&cat, 1), record_of(&cat, 3));
        assert_eq!(records_made(&cat), made);
    }

    // ---- The take: a copy takes the record's tracks (0101).

    /// A record of three tracks, on these ids.
    fn three(ids: [i64; 3]) -> AlbumCopy {
        copy(None, "A", "Three", &[(ids[0], "One", 200_000), (ids[1], "Two", 200_000), (ids[2], "Three", 200_000)])
    }

    /// `a` with a file of each track in `dir`, as `identify` leaves them.
    fn lying_in(cat: &Catalogue, dir: &str, mut a: AlbumCopy) -> AlbumCopy {
        a.files = a.tracks.iter().map(|(t, _, _)| (0, format!("{dir}/{t}.flac"))).collect();
        for ((track, _, _), (_, rel)) in a.tracks.iter().zip(&a.files) {
            file_of_track(cat, rel, *track);
        }
        a
    }

    /// `a` as a pass after the take lists it: its files where they lay, each on the track it holds now.
    fn as_it_stands(cat: &Catalogue, a: &AlbumCopy) -> AlbumCopy {
        let mut now = a.clone();
        for (place, (track, _, _)) in now.files.iter().zip(&mut now.tracks) {
            *track = cat.track_of(place.0, &place.1).unwrap();
        }
        now
    }

    /// Two copies of record `three`, on tracks 1, 2 and 3, the second taken from tracks 4, 5 and 6: the copies, and the record.
    fn two_copies(cat: &Catalogue) -> (AlbumCopy, AlbumCopy, i64) {
        let kept = lying_in(cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let second = lying_in(cat, "Second", three([4, 5, 6]));
        cat.assign_records(&[kept.clone(), second.clone()]).unwrap();
        let second = as_it_stands(cat, &second);
        (kept, second, record_of(cat, 1).unwrap())
    }

    #[test]
    fn a_second_copy_takes_the_records_tracks() {
        let cat = tracks(6);
        let (kept, second, record) = two_copies(&cat);
        assert_eq!(second.tracks.iter().map(|t| t.0).collect::<Vec<_>>(), [1, 2, 3], "place by place");
        assert_eq!((1..=3).map(|t| cat.files_of(t).len()).collect::<Vec<_>>(), [2, 2, 2], "each track a file in each copy");
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(4, 1), (5, 2), (6, 3)]));
        assert_eq!(cat.tracks_to_follow().unwrap().len(), 3, "what names them is to follow");
        assert!((4..=6).all(|t| retired(&cat, t) && record_of(&cat, t).is_none()), "gone, its row kept, no track of any record");
        assert_eq!(record_of(&cat, 1), Some(record));
        assert_eq!(records_made(&cat), 1);
        assert!(!cat.records_unsettled());
        assert_eq!(cat.assign_records(&[kept, second]).unwrap(), 0, "settled");
    }

    /// Both new at once: the first album met is the record, and the other takes its tracks, whichever folder it lies in.
    #[test]
    fn two_new_copies_are_one_record_on_one_set_of_tracks() {
        let cat = tracks(6);
        let (a, b) = (lying_in(&cat, "A", three([1, 2, 3])), lying_in(&cat, "B", three([4, 5, 6])));
        cat.assign_records(&[a, b]).unwrap();
        assert_eq!(cat.track_of(0, "B/5.flac"), Some(2));
        assert_eq!(records_made(&cat), 1);
        assert_eq!(cat.tracks_to_follow().unwrap().len(), 3);
    }

    /// A third copy takes what the second took, and an id two takes old still leads to the track that stands.
    #[test]
    fn a_line_of_takes_ends_at_the_track_that_stands() {
        let cat = tracks(9);
        let (kept, second, _) = two_copies(&cat);
        let third = lying_in(&cat, "Third", three([7, 8, 9]));
        cat.assign_records(&[kept, second, third]).unwrap();
        assert_eq!(cat.track_of(0, "Third/8.flac"), Some(2));
        let moved = cat.moved_tracks().unwrap();
        assert_eq!((moved[&5], moved[&8]), (2, 2));
        let held = Held::read(&cat.db()).unwrap();
        let mut take = Take::default();
        take.pair(&held, 99, 4, 5);
        take.pair(&held, 99, 5, 6);
        assert_eq!((take.now(4), take.now(5)), (6, 6), "in one pass, read through to the end");
    }

    /// A track that took others can be taken itself, by an older track of the record: `track_moved` holds the end of the line,
    /// so what names an id two takes old finds a track, and it stays marked as what it was.
    #[test]
    fn an_id_taken_twice_leads_to_the_track_that_stands() {
        let cat = tracks(9);
        let kept = lying_in(&cat, "Kept", three([4, 5, 6]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let record = record_of(&cat, 4).unwrap();
        let second = lying_in(&cat, "Second", three([7, 8, 9]));
        cat.assign_records(&[kept.clone(), second.clone()]).unwrap();
        let followed = cat.tracks_to_follow().unwrap();
        cat.tracks_followed(&followed).unwrap();
        cat.db().execute("UPDATE tracks SET record = ?1 WHERE id IN (1, 2, 3)", [record]).unwrap();
        let older = lying_in(&cat, "Older", three([1, 2, 3]));
        cat.assign_records(&[kept, as_it_stands(&cat, &second), older]).unwrap();
        let moved = cat.moved_tracks().unwrap();
        assert_eq!((moved[&4], moved[&7]), (1, 1), "the end of the line, for the id two takes old too");
        assert_eq!(cat.track_of(0, "Second/7.flac"), Some(1));
        let waiting = cat.tracks_to_follow().unwrap();
        assert!(waiting.contains(&(4, 1)) && !waiting.iter().any(|(old, _)| *old == 7), "what named 7 moved to 4 already, and goes on with 4");
    }

    /// N1: an id that comes back and is taken has what it held. AudioMuse's row moves where the track that stays has none, and
    /// that track's own stands; the built-in source's link names one file a track, and goes.
    #[test]
    fn what_a_taken_track_held_goes_with_it() {
        let cat = tracks(6);
        let row = |track: i64, energy: f64| AudioMuseRow { track, energy: Some(energy), ..Default::default() };
        cat.replace_audiomuse(&[row(2, 0.2), row(4, 0.4), row(5, 0.5)]).unwrap();
        let link = "INSERT INTO source_ids (source, track_id, library, folder, rel, seen) VALUES (?1, ?2, 'l', 0, ?3, 1)";
        cat.db().execute(link, params!["cratarr", "tr-4", "Second/4.flac"]).unwrap();
        cat.db().execute(link, params!["jf", "j-4", "Second/4.flac"]).unwrap();
        two_copies(&cat);
        assert_eq!(cat.audiomuse_of(1).and_then(|r| r.energy), Some(0.4), "the track that stays had none");
        assert_eq!(cat.audiomuse_of(2).and_then(|r| r.energy), Some(0.2), "its own stands");
        assert!(cat.audiomuse_of(4).is_none() && cat.audiomuse_of(5).is_none());
        let links: Vec<String> = cat.db().prepare("SELECT track_id FROM source_ids").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(links, ["j-4"], "another server's item is a file, and stands");
    }

    /// A gone or trashed file of the id that goes comes along: back, it must not be a track of its own.
    #[test]
    fn every_file_of_a_taken_track_goes_to_the_track_that_stays() {
        let cat = tracks(6);
        file_of_track(&cat, "Second/old name.flac", 4);
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = 'Second/old name.flac'", []).unwrap();
        two_copies(&cat);
        let held: i64 = cat.db().query_row("SELECT track FROM files WHERE rel = 'Second/old name.flac'", [], |r| r.get(0)).unwrap();
        assert_eq!(held, 1);
    }

    fn two_folders_on_disk(name: &str) -> (PathBuf, PathBuf, Catalogue, i64, i64) {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let rel = "A - B (2020)/01 One.mp3";
        let (stays, goes) = (cat.track_of(0, rel).unwrap(), cat.track_of(1, rel).unwrap());
        let album = |folder: i64, track: i64| AlbumCopy { files: vec![(folder, rel.to_string())], ..copy(None, "A", name, &[(track, "One", 200_000)]) };
        cat.assign_records(&[album(0, stays), album(1, goes)]).unwrap();
        assert_eq!(cat.track_of(1, rel), Some(stays), "taken");
        (a, b, cat, stays, goes)
    }

    /// N2: what fits what the taken id was gets an id of its own. Handed the old one, it would split the record again.
    #[test]
    fn a_taken_id_never_comes_back() {
        let (a, b, cat, stays, goes) = two_folders_on_disk("no-return");
        std::fs::create_dir_all(a.join("Later")).unwrap();
        std::fs::write(a.join("Later/01 One.mp3"), b"its name and its length").unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let later = cat.track_of(0, "Later/01 One.mp3").unwrap();
        assert!(later != goes && later != stays, "{later}");
        assert!(retired(&cat, goes), "it stays gone");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// CR4: an Undo from before the take names the id that went, by a file's place or a hand-over. Either lands on the
    /// record's track, or the record is two sets of tracks again.
    #[test]
    fn undo_from_before_the_move_keeps_the_record() {
        let (a, b, cat, stays, goes) = two_folders_on_disk("undo");
        let rel = "A - B (2020)/01 One.mp3";
        cat.set_tracks(&[((1, rel.to_string()), Some(goes))]).unwrap();
        assert_eq!(cat.track_of(1, rel), Some(stays), "a Replace's Undo");
        cat.set_tracks(&[((1, rel.to_string()), None)]).unwrap();
        assert_eq!(cat.track_of(1, rel), None, "letting go still lets go");
        cat.set_tracks(&[((1, rel.to_string()), Some(stays))]).unwrap();

        let back = b.join("A - B (2020)/01 Back.mp3");
        std::fs::write(&back, b"the file it was").unwrap();
        cat.hand_over(&back, &b.join(rel), goes).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        assert_eq!(cat.track_of(1, "A - B (2020)/01 Back.mp3"), Some(stays), "a better copy's Undo");
        assert!(retired(&cat, goes), "it stays gone");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// A queue, a crate or an export can name an id long after it went: it plays what took it.
    #[test]
    fn a_taken_id_still_stands_for_its_track() {
        let (a, b, cat, stays, goes) = two_folders_on_disk("named");
        let own = crate::source::cratarr::KIND.id;
        assert_eq!(cat.track_of_source(own, &crate::serve::ids::song(goes)), Some(stays));
        let files = cat.files_of_sources(&[(own, crate::serve::ids::song(goes).as_str())]);
        assert_eq!((files[0].0, files[0].1.len()), (stays, 2), "both copies to choose from");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// The followed mark: a stop between the user data's move and the mark goes over it again, and one taken further since
    /// waits for its own turn.
    #[test]
    fn what_was_followed_is_not_followed_again() {
        let cat = tracks(6);
        two_copies(&cat);
        let moved = cat.tracks_to_follow().unwrap();
        assert_eq!(moved.len(), 3);
        cat.tracks_followed(&moved[..1]).unwrap();
        assert_eq!(cat.tracks_to_follow().unwrap().len(), 2);
        cat.tracks_followed(&moved).unwrap();
        assert!(cat.tracks_to_follow().unwrap().is_empty());
        assert_eq!(cat.moved_tracks().unwrap().len(), 3, "kept for good");
    }

    /// N3: a file replaced outside Cratarr in one copy has a fresh id, and without this the record lists a track more.
    #[test]
    fn a_fresh_file_in_a_copy_takes_its_track() {
        let cat = tracks(7);
        let (kept, second, _) = two_copies(&cat);
        let replaced = second.files[2].1.clone();
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&replaced]).unwrap();
        let now = as_it_stands(&cat, &second);
        assert_eq!(cat.assign_records(&[kept, now]).unwrap(), 1);
        assert_eq!(cat.track_of(0, &replaced), Some(3));
        assert_eq!(cat.moved_tracks().unwrap()[&7], 3, "the fresh id goes, not the one that has the plays");
        assert!(retired(&cat, 7));
    }

    /// The same where it joined by its album's vote first: both hold the record, and the older id stays.
    #[test]
    fn a_fresh_file_that_joined_the_record_still_takes_its_track() {
        let cat = tracks(7);
        let (kept, second, record) = two_copies(&cat);
        let replaced = second.files[2].1.clone();
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&replaced]).unwrap();
        cat.db().execute("UPDATE tracks SET record = ?1 WHERE id = 7", [record]).unwrap();
        let now = as_it_stands(&cat, &second);
        cat.assign_records(&[now, kept]).unwrap();
        assert_eq!(cat.track_of(0, &replaced), Some(3), "first on the tie, the fresh file does not win");
        assert_eq!(record_of(&cat, 7), None, "no track of the record now, or it counts as one: a track back would need a vote more");
    }

    /// A fresh file with no number sorts last, so its album is not the same tracks place by place: its name and length say.
    #[test]
    fn a_fresh_file_takes_its_track_whatever_its_place() {
        let cat = tracks(7);
        let (kept, second, _) = two_copies(&cat);
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&second.files[1].1]).unwrap();
        let mut now = copy(None, "A", "Three", &[(1, "One", 200_000), (3, "Three", 200_000), (7, "Two", 200_000)]);
        now.files = vec![second.files[0].clone(), second.files[2].clone(), second.files[1].clone()];
        cat.assign_records(&[kept, now.clone()]).unwrap();
        assert_eq!(cat.track_of(0, &second.files[1].1), Some(2));
        assert_eq!(cat.track_of(0, &second.files[0].1), Some(1), "the rest stay: it is still a copy");
    }

    /// A file of another record's track lying in a copy, where its name fits: that track has its own album, and a take would
    /// take its file there along, for good.
    #[test]
    fn a_track_of_another_record_is_not_taken_by_its_name() {
        let cat = tracks(7);
        let (kept, second, _) = two_copies(&cat);
        let other = lying_in(&cat, "Other", album("Other", &[7]));
        cat.assign_records(&[kept.clone(), second.clone(), other.clone()]).unwrap();
        let elsewhere = record_of(&cat, 7);
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&second.files[2].1]).unwrap();
        let now = as_it_stands(&cat, &second);
        cat.assign_records(&[kept, now, other]).unwrap();
        assert!(!cat.moved_tracks().unwrap().contains_key(&7));
        assert_eq!((record_of(&cat, 7), cat.track_of(0, "Other/7.flac")), (elsewhere, Some(7)));
    }

    /// Two fresh files that fit one track is a guess: neither takes it.
    #[test]
    fn a_fresh_file_that_could_be_two_takes_nothing() {
        let cat = tracks(9);
        let (kept, second, record) = two_copies(&cat);
        cat.db().execute("UPDATE tracks SET record = ?1 WHERE id IN (7, 8)", [record]).unwrap();
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&second.files[1].1]).unwrap();
        cat.db().execute("UPDATE files SET track = 8 WHERE rel = ?1", [&second.files[2].1]).unwrap();
        let mut now = copy(None, "A", "Elsewhere", &[(1, "One", 200_000), (7, "Two", 200_000), (8, "Two", 200_000)]);
        now.files = second.files.clone();
        cat.assign_records(&[kept, now]).unwrap();
        assert!(!cat.moved_tracks().unwrap().contains_key(&7));
    }

    /// Q8, the owner: a track added to one of two copies splits them, as a deluxe edition is its own record, and the copy with
    /// more tracks keeps the record, its id and what points at it. Their tracks are shared by now, so the one that leaves
    /// takes tracks of its own: nothing moves but its files.
    #[test]
    fn a_track_added_to_one_of_two_copies_splits_them() {
        let cat = tracks(7);
        let (_, second, record) = two_copies(&cat);
        let grown = AlbumCopy { files: vec![(0, "Kept/1.flac".into()), (0, "Kept/2.flac".into()), (0, "Kept/3.flac".into()), (0, "Kept/7.flac".into())], ..copy(None, "A", "Three", &[(1, "One", 200_000), (2, "Two", 200_000), (3, "Three", 200_000), (7, "Four", 200_000)]) };
        file_of_track(&cat, "Kept/7.flac", 7);
        let moved = cat.moved_tracks().unwrap();
        // Larger or first, the order of the albums says nothing.
        cat.assign_records(&[second.clone(), grown.clone()]).unwrap();
        assert!((1..=3).chain([7]).all(|t| record_of(&cat, t) == Some(record)), "the copy with more tracks keeps the record");
        assert!((1..=3).all(|t| cat.files_of(t).len() == 1), "and the tracks, on its files alone");
        let left = as_it_stands(&cat, &second);
        let ids: Vec<i64> = left.tracks.iter().map(|t| t.0).collect();
        assert!(ids.iter().all(|t| *t > 7), "tracks of its own: {ids:?}");
        let other = record_of(&cat, ids[0]).unwrap();
        assert_ne!(other, record);
        assert!(ids.iter().all(|t| record_of(&cat, *t) == Some(other)));
        assert_eq!(cat.moved_tracks().unwrap(), moved, "nothing to follow: what points at the record stays");
        let born = |t: i64| cat.db().query_row("SELECT born FROM tracks WHERE id = ?1", [t], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(born(ids[0]), born(1), "as old as the file: it is no new music");
        assert_eq!(cat.assign_records(&[left, grown]).unwrap(), 0, "settled");
    }

    /// On an equal count the first album keeps it, as built, and only what the other shares with it lets go.
    #[test]
    fn of_two_copies_as_long_the_first_keeps_the_record() {
        let cat = tracks(9);
        let (kept, second, record) = two_copies(&cat);
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&kept.files[2].1]).unwrap();
        cat.db().execute("UPDATE files SET track = 8 WHERE rel = ?1", [&second.files[2].1]).unwrap();
        let mut a = copy(None, "A", "Three", &[(1, "One", 200_000), (2, "Two", 200_000), (7, "Other", 200_000)]);
        a.files = kept.files.clone();
        let mut b = copy(None, "A", "Three", &[(1, "One", 200_000), (2, "Two", 200_000), (8, "Else", 200_000)]);
        b.files = second.files.clone();
        cat.assign_records(&[a, b.clone()]).unwrap();
        assert_eq!(record_of(&cat, 7), Some(record));
        let left = as_it_stands(&cat, &b);
        assert_ne!(left.tracks[0].0, 1);
        assert_eq!(left.tracks[2].0, 8, "its own track stays its own");
        assert_ne!(record_of(&cat, 8), Some(record), "and goes with it");
    }

    /// Where the other copy gained more than it held it has no majority, and something else holds the record, so the rule
    /// stands (m5): the record stays with the copy that is still it, though that is the smaller. This is where Q8 and m5 meet.
    #[test]
    fn a_copy_that_gained_more_than_it_held_leaves_the_record_to_the_other() {
        let cat = tracks(14);
        let (kept, second, record) = two_copies(&cat);
        for t in 7..=14 {
            file_of_track(&cat, &format!("Kept/{t}.flac"), t);
        }
        let list: Vec<(i64, String)> = (7..=14).map(|t| (t, format!("New {t}"))).collect();
        let names = [(1, "One".to_string()), (2, "Two".to_string()), (3, "Three".to_string())];
        let all: Vec<(i64, &str, i64)> = names.iter().chain(&list).map(|(t, n)| (*t, n.as_str(), 200_000)).collect();
        let mut grown = copy(None, "A", "Three", &all);
        grown.files = kept.files.iter().cloned().chain((7..=14).map(|t| (0, format!("Kept/{t}.flac")))).collect();
        cat.assign_records(&[grown.clone(), second.clone()]).unwrap();
        assert!((1..=3).all(|t| record_of(&cat, t) == Some(record)));
        assert_eq!(cat.track_of(0, &second.files[0].1), Some(1), "the copy that is still the record");
        let now = as_it_stands(&cat, &grown);
        let new = record_of(&cat, 14).unwrap();
        assert_ne!(new, record);
        assert!(now.tracks.iter().all(|(t, _, _)| record_of(&cat, *t) == Some(new)), "all of it, or three files would never be listed");
        assert_eq!(cat.assign_records(&[now, second]).unwrap(), 0, "settled");
    }

    /// A copy leaving a record goes to the record it is the same as, if there is one, without touching what stays.
    #[test]
    fn a_copy_that_leaves_takes_the_tracks_of_the_record_it_is_the_same_as() {
        let cat = tracks(10);
        let (kept, second, record) = two_copies(&cat);
        let grown = AlbumCopy { files: kept.files.iter().cloned().chain([(0, "Kept/7.flac".to_string())]).collect(), ..copy(None, "A", "Three", &[(1, "One", 200_000), (2, "Two", 200_000), (3, "Three", 200_000), (7, "Four", 200_000)]) };
        file_of_track(&cat, "Kept/7.flac", 7);
        let other = lying_in(&cat, "Other", copy(Some("r"), "A", "Three", &[(8, "One", 200_000), (9, "Two", 200_000), (10, "Three", 200_000)]));
        cat.assign_records(&[other.clone()]).unwrap();
        let elsewhere = record_of(&cat, 8);
        assert_ne!(elsewhere, Some(record));
        cat.assign_records(&[other, grown, second.clone()]).unwrap();
        assert_eq!(as_it_stands(&cat, &second).tracks.iter().map(|t| t.0).collect::<Vec<_>>(), [8, 9, 10]);
        assert_eq!(cat.files_of(1).len(), 1);
        assert!(!retired(&cat, 1), "the track that stays with its record is not taken");
    }

    /// A file gone a while is no copy lacking a track: a mount that blinks must not split a record, nor give a copy new ids.
    /// Gone for good a day later, what is left is not the same tracks, and leaves.
    #[test]
    fn a_file_gone_a_while_splits_nothing() {
        let cat = tracks(6);
        let (kept, second, record) = two_copies(&cat);
        let rel = second.files[2].1.clone();
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = ?1", [&rel]).unwrap();
        let lacking = AlbumCopy { files: second.files[..2].to_vec(), ..copy(None, "A", "Three", &[(1, "One", 200_000), (2, "Two", 200_000)]) };
        assert_eq!(cat.assign_records(&[kept.clone(), lacking.clone()]).unwrap(), 0);
        assert_eq!(cat.track_of(0, &second.files[0].1), Some(1));
        cat.db().execute("DELETE FROM files WHERE rel = ?1", [&rel]).unwrap();
        cat.assign_records(&[kept, lacking.clone()]).unwrap();
        let left = as_it_stands(&cat, &lacking);
        assert_ne!(left.tracks[0].0, 1);
        assert_ne!(record_of(&cat, left.tracks[0].0), Some(record));
        assert_eq!(cat.files_of(3).len(), 1);
    }

    /// `three` as a copy lacking the tracks after the first `n`, its files those of `a`.
    fn the_first(n: usize, a: &AlbumCopy) -> AlbumCopy {
        let names = [(1, "One", 200_000), (2, "Two", 200_000), (3, "Three", 200_000)];
        AlbumCopy { files: a.files[..n].to_vec(), ..copy(None, "A", "Three", &names[..n]) }
    }

    /// A file that cannot be read is no gone file, and nothing reads it again until it changes. A tagger writing as a pass reads,
    /// or a mount dropped during a read, must not split a record: its copy lacks it for now, however long.
    #[test]
    fn a_file_not_read_splits_nothing() {
        let cat = tracks(6);
        let (kept, second, _) = two_copies(&cat);
        cat.db().execute("UPDATE files SET state = ?2 WHERE rel = ?1", params![second.files[2].1, FAILED]).unwrap();
        assert_eq!(cat.assign_records(&[kept, the_first(2, &second)]).unwrap(), 0);
        assert_eq!(cat.track_of(0, &second.files[0].1), Some(1));
    }

    /// A disc gone whole leaves no file of the album in its folder, so the album's other folder has to say where it lay.
    #[test]
    fn a_disc_gone_a_while_splits_nothing() {
        let cat = tracks(6);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let mut second = three([4, 5, 6]);
        second.files = [(4, "Second/CD1/4.flac"), (5, "Second/CD1/5.flac"), (6, "Second/CD2/6.flac")].map(|(t, rel)| {
            file_of_track(&cat, rel, t);
            (0, rel.to_string())
        }).to_vec();
        cat.assign_records(&[kept.clone(), second.clone()]).unwrap();
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = 'Second/CD2/6.flac'", []).unwrap();
        assert_eq!(cat.assign_records(&[kept, the_first(2, &second)]).unwrap(), 0);
        assert_eq!(cat.track_of(0, "Second/CD1/4.flac"), Some(1));
    }

    /// Each lacks what the other has, for now: that is no reason for one to leave.
    #[test]
    fn two_copies_each_lacking_a_file_for_now_stay_one() {
        let cat = tracks(6);
        let (kept, second, _) = two_copies(&cat);
        for rel in [&kept.files[2].1, &second.files[1].1] {
            cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = ?1", [rel]).unwrap();
        }
        let names = [(1, "One", 200_000), (3, "Three", 200_000)];
        let other = AlbumCopy { files: vec![second.files[0].clone(), second.files[2].clone()], ..copy(None, "A", "Three", &names) };
        assert_eq!(cat.assign_records(&[the_first(2, &kept), other]).unwrap(), 0);
    }

    /// `by_name` is for a file replaced in a copy, and a take cannot be undone. Two discs of one record that part hold no such
    /// file: a title they share is two tracks, whose plays and entries stay on each.
    #[test]
    fn a_retagged_disc_keeps_a_title_it_shares_with_the_other_disc() {
        let cat = tracks(4);
        let names = [(1, "One", 200_000), (2, "Interlude", 60_000), (3, "Three", 200_000), (4, "Interlude", 61_500)];
        let whole = lying_in(&cat, "Box", copy(None, "A", "Box", &names));
        cat.assign_records(std::slice::from_ref(&whole)).unwrap();
        let first = AlbumCopy { files: whole.files[..2].to_vec(), ..copy(None, "A", "Box", &names[..2]) };
        let other = AlbumCopy { files: whole.files[2..].to_vec(), ..copy(None, "A", "Box", &names[2..]) };
        cat.assign_records(&[first, other]).unwrap();
        assert!(cat.moved_tracks().unwrap().is_empty());
        assert_ne!(record_of(&cat, 1), record_of(&cat, 3), "two albums, as a retag makes them");
    }

    /// `pair`'s own guard, by the way that has none before it: of two albums not the same, a fresh file whose name fits a track of
    /// another record that lies in the other album. The track stays that record's, and the file its own.
    #[test]
    fn a_track_of_another_record_is_not_taken_by_name_between_albums_that_differ() {
        let cat = tracks(9);
        let (kept, second, _) = two_copies(&cat);
        let other = lying_in(&cat, "Other", album("Other", &[7]));
        cat.assign_records(&[kept.clone(), second.clone(), other.clone()]).unwrap();
        let elsewhere = record_of(&cat, 7);
        cat.db().execute("UPDATE files SET track = 7 WHERE rel = ?1", [&kept.files[2].1]).unwrap();
        cat.db().execute("UPDATE files SET track = 9 WHERE rel = ?1", [&second.files[2].1]).unwrap();
        let mut renamed = as_it_stands(&cat, &second);
        renamed.facts.title = identity::fold("Another Name");
        cat.assign_records(&[as_it_stands(&cat, &kept), renamed, other]).unwrap();
        assert!(!cat.moved_tracks().unwrap().contains_key(&9));
        assert_eq!(record_of(&cat, 7), elsewhere);
    }

    /// CR19, the owner: a folder arriving file by file joins the record you hold. Each pass sees it short of the record, so it
    /// is a record of its own until its last file, and a young one is held against the others as a new folder is.
    #[test]
    fn a_folder_that_comes_in_over_three_passes_becomes_a_copy() {
        let cat = tracks(6);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let record = record_of(&cat, 1);
        let names = [(4, "One", 200_000), (5, "Two", 200_000), (6, "Three", 200_000)];
        let arriving = |n: usize| {
            let mut a = copy(None, "A", "Three", &names[..n]);
            a.files = names[..n].iter().map(|(t, _, _)| (0, format!("Copy/{t}.flac"))).collect();
            a
        };
        for n in 1..=3 {
            file_of_track(&cat, &format!("Copy/{}.flac", n + 3), n as i64 + 3);
            cat.assign_records(&[kept.clone(), arriving(n)]).unwrap();
        }
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(4, 1), (5, 2), (6, 3)]));
        assert_eq!(record_of(&cat, 1), record);
        assert_eq!(cat.assign_records(&[kept, as_it_stands(&cat, &arriving(3))]).unwrap(), 0, "settled");
        let young = cat.db().query_row("SELECT count(*) FROM records WHERE retired_at IS NULL", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(young, 1, "the record it was for a while went with its tracks");
    }

    /// These tracks by name as a folder of the very same release holds them so far.
    fn of_the_release(list: &[(i64, &str)]) -> AlbumCopy {
        let tracks: Vec<(i64, &str, i64)> = list.iter().map(|(t, name)| (*t, *name, 200_000)).collect();
        copy(Some("release"), "A", "Three", &tracks)
    }

    /// Three tracks of the release in `Kept`, on tracks 1, 2 and 3, as the record stands: the copy, and the record.
    fn kept_of_the_release(cat: &Catalogue) -> (AlbumCopy, Option<i64>) {
        let kept = lying_in(cat, "Kept", of_the_release(&[(1, "One"), (2, "Two"), (3, "Three")]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let record = record_of(cat, 1);
        (kept, record)
    }

    /// One folder of the release as a pass reads it: these files, by name, on the tracks they hold now.
    fn as_read(cat: &Catalogue, dir: &str, names: &[&str]) -> AlbumCopy {
        let places: Vec<(i64, String)> = names.iter().map(|name| (0, format!("{dir}/{name}.flac"))).collect();
        let list: Vec<(i64, &str)> = places.iter().zip(names).map(|((folder, rel), name)| (cat.track_of(*folder, rel).unwrap(), *name)).collect();
        AlbumCopy { files: places, ..of_the_release(&list) }
    }

    /// The owner, 2026-11-07: a folder of the very same release is the record from its first file, and each file takes the track it
    /// is: the last track first here, which place by place would take the record's first. No record is born for it on the way.
    #[test]
    fn a_folder_of_the_very_same_release_joins_at_its_first_file() {
        let cat = tracks(6);
        let (kept, record) = kept_of_the_release(&cat);
        file_of_track(&cat, "Copy/Three.flac", 6);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["Three"])]).unwrap();
        assert_eq!(cat.track_of(0, "Copy/Three.flac"), Some(3), "the track it is");
        assert_eq!(cat.files_of(1).len(), 1, "and no track it does not hold");
        assert_eq!(records_made(&cat), 1, "no record of its own");
        file_of_track(&cat, "Copy/One.flac", 4);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Three"])]).unwrap();
        assert_eq!(cat.track_of(0, "Copy/One.flac"), Some(1));
        file_of_track(&cat, "Copy/Two.flac", 5);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(4, 1), (5, 2), (6, 3)]));
        assert_eq!((record_of(&cat, 1), records_made(&cat)), (record, 1));
        assert_eq!(cat.assign_records(&[kept, as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap(), 0, "settled");
    }

    /// While it is short it is settled too: nothing moves at the next build, and nothing of it leaves.
    #[test]
    fn a_folder_short_of_its_release_stays_as_it_is_between_files() {
        let cat = tracks(5);
        let (kept, record) = kept_of_the_release(&cat);
        file_of_track(&cat, "Copy/One.flac", 4);
        file_of_track(&cat, "Copy/Two.flac", 5);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Two"])]).unwrap();
        assert_eq!(cat.assign_records(&[kept, as_read(&cat, "Copy", &["One", "Two"])]).unwrap(), 0);
        assert_eq!((cat.track_of(0, "Copy/Two.flac"), record_of(&cat, 2)), (Some(2), record));
    }

    /// Short of it, the tags no longer say what they did, and what the ids hold together stays together, as with a whole copy.
    #[test]
    fn a_copy_short_of_its_release_stays_whatever_the_tags_say_since() {
        let cat = tracks(4);
        let (kept, _) = kept_of_the_release(&cat);
        file_of_track(&cat, "Copy/One.flac", 4);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One"])]).unwrap();
        let mut retagged = as_read(&cat, "Copy", &["One"]);
        retagged.facts.tracks[0] = identity::Listed::of("Uno", 200_000);
        assert_eq!(cat.assign_records(&[kept, retagged]).unwrap(), 0);
        assert_eq!(cat.track_of(0, "Copy/One.flac"), Some(1));
    }

    /// Without that very release nothing changes: a folder short of a record is a record of its own until its last file (CR19).
    #[test]
    fn a_folder_with_no_release_or_another_waits_for_its_last_file() {
        for release in [None, Some("another")] {
            let cat = tracks(4);
            let (kept, record) = kept_of_the_release(&cat);
            file_of_track(&cat, "Copy/One.flac", 4);
            let first = AlbumCopy { files: vec![(0, "Copy/One.flac".to_string())], ..copy(release, "A", "Three", &[(4, "One", 200_000)]) };
            cat.assign_records(&[kept, first]).unwrap();
            assert!(cat.moved_tracks().unwrap().is_empty(), "{release:?}");
            assert_ne!(record_of(&cat, 4), record, "{release:?}");
        }
    }

    /// A track the record does not list is no track of that release, whatever the folder says: nothing here joins.
    #[test]
    fn a_folder_with_a_track_the_record_does_not_list_stays_out() {
        let cat = tracks(5);
        let (kept, record) = kept_of_the_release(&cat);
        file_of_track(&cat, "Copy/One.flac", 4);
        file_of_track(&cat, "Copy/Bonus.flac", 5);
        let mine = AlbumCopy { files: vec![(0, "Copy/One.flac".to_string()), (0, "Copy/Bonus.flac".to_string())], ..of_the_release(&[(4, "One"), (5, "Bonus")]) };
        cat.assign_records(&[kept, mine]).unwrap();
        assert!(cat.moved_tracks().unwrap().is_empty());
        assert_ne!(record_of(&cat, 4), record);
    }

    /// The other way round: the record is short of the folder, a whole copy landing beside a few tracks you hold. It is that record,
    /// and what it brings are tracks of it. Who keeps the record goes by kind still: Music's stays what it was.
    #[test]
    fn a_whole_folder_joins_a_record_short_of_it() {
        let cat = tracks(5);
        let kept = AlbumCopy { rank: 2, ..lying_in(&cat, "Kept", of_the_release(&[(1, "One"), (2, "Two")])) };
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        cat.db().execute("UPDATE records SET born = born - ?1", [GONE_KEEP_SECS + 1]).unwrap();
        let record = record_of(&cat, 1);
        for (rel, track) in [("Copy/One.flac", 3), ("Copy/Two.flac", 4), ("Copy/Three.flac", 5)] {
            file_of_track(&cat, rel, track);
        }
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(3, 1), (4, 2)]));
        assert_eq!(record_of(&cat, 5), record, "a track of the record now");
        assert_eq!(cat.assign_records(&[kept, as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap(), 0, "settled, the shorter staying");
    }

    /// Q8 stands where it is a copy that grew: a track new to the record parts two whole copies, of one release too, and the one that
    /// left does not come back as a folder short of it.
    #[test]
    fn a_track_new_to_the_record_parts_whole_copies_of_one_release() {
        let cat = tracks(7);
        let (kept, record) = kept_of_the_release(&cat);
        for (rel, track) in [("Copy/One.flac", 4), ("Copy/Two.flac", 5), ("Copy/Three.flac", 6)] {
            file_of_track(&cat, rel, track);
        }
        let second = as_read(&cat, "Copy", &["One", "Two", "Three"]);
        cat.assign_records(&[kept.clone(), second.clone()]).unwrap();
        file_of_track(&cat, "Kept/Four.flac", 7);
        let list = [(1, "One"), (2, "Two"), (3, "Three"), (7, "Four")];
        let grown = AlbumCopy { files: [kept.files.clone(), vec![(0, "Kept/Four.flac".to_string())]].concat(), ..of_the_release(&list) };
        cat.assign_records(&[grown.clone(), as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        let left = as_read(&cat, "Copy", &["One", "Two", "Three"]);
        assert_eq!(record_of(&cat, 1), record, "the copy with more keeps it");
        assert_eq!(record_of(&cat, 7), record);
        assert_ne!(record_of(&cat, left.tracks[0].0), record, "the other leaves");
        assert_eq!(cat.assign_records(&[grown, left.clone()]).unwrap(), 0, "and stays out");
        assert_ne!(record_of(&cat, left.tracks[0].0), record);
    }

    /// A copy of the release that lost a file for good is short of it, as one arriving is: it stays, and its track stays the record's.
    #[test]
    fn a_copy_of_one_release_that_lost_a_file_for_good_stays_short_of_it() {
        let cat = tracks(6);
        let (kept, record) = kept_of_the_release(&cat);
        for (rel, track) in [("Copy/One.flac", 4), ("Copy/Two.flac", 5), ("Copy/Three.flac", 6)] {
            file_of_track(&cat, rel, track);
        }
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        cat.db().execute("DELETE FROM files WHERE rel = 'Copy/Two.flac'", []).unwrap();
        assert_eq!(cat.assign_records(&[kept, as_read(&cat, "Copy", &["One", "Three"])]).unwrap(), 0);
        assert_eq!((cat.track_of(0, "Copy/One.flac"), record_of(&cat, 2)), (Some(1), record));
    }

    /// Two folders arriving at once, the release in neither yet: the first met is the record, the other joins it file by file.
    #[test]
    fn two_folders_of_one_release_arriving_at_once_are_one_record() {
        let cat = tracks(3);
        let a = lying_in(&cat, "A", of_the_release(&[(1, "One"), (2, "Two")]));
        let b = lying_in(&cat, "B", of_the_release(&[(3, "Two")]));
        cat.assign_records(&[a, b]).unwrap();
        assert_eq!((cat.track_of(0, "B/3.flac"), records_made(&cat)), (Some(2), 1));
    }

    /// Gaining more than it held takes a copy past any vote, and it is held against the others as a new folder is. It is no new folder:
    /// the record holds it already, and it grew, so it leaves as Q8 and m5 say, the record staying with the copy that is still it.
    #[test]
    fn a_copy_of_one_release_that_gained_more_than_it_held_still_leaves() {
        let cat = tracks(14);
        let (kept, record) = kept_of_the_release(&cat);
        for (rel, track) in [("Copy/One.flac", 4), ("Copy/Two.flac", 5), ("Copy/Three.flac", 6)] {
            file_of_track(&cat, rel, track);
        }
        cat.assign_records(&[kept.clone(), as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        (7..=14).for_each(|t| file_of_track(&cat, &format!("Kept/{t}.flac"), t));
        let names: Vec<String> = (7..=14).map(|t| format!("New {t}")).collect();
        let list: Vec<(i64, &str)> = [(1, "One"), (2, "Two"), (3, "Three")].into_iter().chain((7..=14).zip(names.iter().map(String::as_str))).collect();
        let grown = AlbumCopy { files: kept.files.iter().cloned().chain((7..=14).map(|t| (0, format!("Kept/{t}.flac")))).collect(), ..of_the_release(&list) };
        cat.assign_records(&[grown, as_read(&cat, "Copy", &["One", "Two", "Three"])]).unwrap();
        assert_eq!(cat.track_of(0, "Copy/One.flac"), Some(1), "the copy that is still the record");
        assert_ne!(record_of(&cat, 14), record, "what grew left with what it brought");
    }

    /// A folder held against a copy short of the record would keep tracks of its own for what the other copy has: it goes by the longest.
    #[test]
    fn a_folder_of_one_release_is_held_against_the_longest_copy_of_it() {
        let cat = tracks(6);
        let (kept, _) = kept_of_the_release(&cat);
        file_of_track(&cat, "Short/One.flac", 4);
        cat.assign_records(&[kept.clone(), as_read(&cat, "Short", &["One"])]).unwrap();
        file_of_track(&cat, "Copy/One.flac", 5);
        file_of_track(&cat, "Copy/Two.flac", 6);
        cat.assign_records(&[as_read(&cat, "Short", &["One"]), kept, as_read(&cat, "Copy", &["One", "Two"])]).unwrap();
        assert_eq!((cat.track_of(0, "Copy/One.flac"), cat.track_of(0, "Copy/Two.flac")), (Some(1), Some(2)), "each on the record's own");
    }

    /// CR19: only the young. Past a day a record has what names it, and nothing merges two records.
    #[test]
    fn a_record_past_its_first_day_joins_nothing() {
        let cat = tracks(6);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let mut older = lying_in(&cat, "Copy", three([4, 5, 6]));
        older.facts.title = identity::fold("For Now");
        cat.assign_records(&[kept.clone(), older.clone()]).unwrap();
        let record = record_of(&cat, 4);
        assert_ne!(record, record_of(&cat, 1));
        older.facts.title = identity::fold("Three");
        cat.db().execute("UPDATE records SET born = born - ?1", [GONE_KEEP_SECS + 1]).unwrap();
        cat.assign_records(&[kept.clone(), older.clone()]).unwrap();
        assert!(cat.moved_tracks().unwrap().is_empty());
        assert_eq!(record_of(&cat, 4), record);

        cat.db().execute("UPDATE records SET born = ?2 WHERE id = ?1", params![record, now()]).unwrap();
        cat.assign_records(&[kept, older]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap().len(), 3, "the same album on its first day");
    }

    /// CR19: a record with a file of it elsewhere is a record of copies, though that file is gone for now.
    #[test]
    fn a_young_record_with_a_track_shared_joins_nothing() {
        let cat = tracks(6);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let mut young = lying_in(&cat, "Copy", three([4, 5, 6]));
        young.facts.title = identity::fold("For Now");
        cat.assign_records(&[kept.clone(), young.clone()]).unwrap();
        file_of_track(&cat, "Beside/4.flac", 4);
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = 'Beside/4.flac'", []).unwrap();
        young.facts.title = identity::fold("Three");
        let record = record_of(&cat, 1);
        cat.db().execute("UPDATE records SET born = born - ?2 WHERE id = ?1", params![record, GONE_KEEP_SECS + 1]).unwrap();
        cat.assign_records(&[kept, young]).unwrap();
        assert!(cat.moved_tracks().unwrap().is_empty());
    }

    /// CR19: nor is a record with a file in another folder a folder arriving: it is alone in none of them.
    #[test]
    fn a_young_record_in_two_folders_joins_nothing() {
        let cat = tracks(7);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let mut young = lying_in(&cat, "Copy", three([4, 5, 6]));
        young.facts.title = identity::fold("For Now");
        cat.assign_records(&[kept.clone(), young.clone()]).unwrap();
        let (record, other) = (record_of(&cat, 4), record_of(&cat, 1));
        let beside = lying_in(&cat, "Beside", album("For Now", &[7]));
        cat.db().execute("UPDATE tracks SET record = ?1 WHERE id = 7", [record]).unwrap();
        young.facts.title = identity::fold("Three");
        cat.db().execute("UPDATE records SET born = born - ?2 WHERE id = ?1", params![other, GONE_KEEP_SECS + 1]).unwrap();
        cat.assign_records(&[kept, young, beside]).unwrap();
        assert!(cat.moved_tracks().unwrap().is_empty());
    }

    /// CR19 goes by CR18's order: whichever came first, Music's stands.
    #[test]
    fn of_two_young_records_that_are_one_the_first_by_kind_stands() {
        let cat = tracks(6);
        let kept = AlbumCopy { rank: 2, ..lying_in(&cat, "Kept", three([1, 2, 3])) };
        let mut waiting = lying_in(&cat, "Copy", three([4, 5, 6]));
        waiting.facts.title = identity::fold("For Now");
        cat.assign_records(&[waiting.clone(), kept.clone()]).unwrap();
        let record = record_of(&cat, 1);
        waiting.facts.title = identity::fold("Three");
        cat.assign_records(&[waiting, kept]).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(4, 1), (5, 2), (6, 3)]));
        assert_eq!(record_of(&cat, 1), record);
    }

    /// The take goes by every row of a track, gone ones too, and a let-go must as well. Left on the record's track, a file back
    /// from a moment away is a file of the other record, in an album that goes by its own vote and never lists it.
    #[test]
    fn a_file_of_a_copy_that_left_comes_back_to_its_copy() {
        let cat = tracks(7);
        let (kept, second, _) = two_copies(&cat);
        let rel = second.files[2].1.clone();
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = ?1", [&rel]).unwrap();
        file_of_track(&cat, "Kept/7.flac", 7);
        let names = [(1, "One", 200_000), (2, "Two", 200_000), (3, "Three", 200_000), (7, "Four", 200_000)];
        let mut more = copy(None, "A", "Three", &names);
        more.files = [kept.files.clone(), vec![(0, "Kept/7.flac".to_string())]].concat();
        let lacking = the_first(2, &second);
        cat.assign_records(&[more.clone(), lacking.clone()]).unwrap();
        let left = as_it_stands(&cat, &lacking);
        assert_ne!(left.tracks[0].0, 1, "the copy left");

        cat.db().execute("UPDATE files SET gone_at = NULL WHERE rel = ?1", [&rel]).unwrap();
        let back = as_it_stands(&cat, &second);
        assert_ne!(back.tracks[2].0, 3, "on no track of the record it left");
        cat.assign_records(&[more, back.clone()]).unwrap();
        assert_eq!(record_of(&cat, back.tracks[2].0), record_of(&cat, left.tracks[0].0), "in its copy's record");
    }

    /// A Keep across a mount lands a file at a time, and `identify` pairs what has arrived with its gone row, so nothing says it
    /// lacks anything for now. While an album action is open no copy leaves: what is half way is no copy that stopped being one.
    #[test]
    fn a_copy_half_way_through_a_move_keeps_its_tracks() {
        let cat = tracks(6);
        let (kept, second, _) = two_copies(&cat);
        cat.db().execute("UPDATE files SET rel = 'Third/4.flac' WHERE rel = ?1", [&second.files[0].1]).unwrap();
        let left = AlbumCopy { files: second.files[1..].to_vec(), ..copy(None, "A", "Three", &[(2, "Two", 200_000), (3, "Three", 200_000)]) };
        let arrived = AlbumCopy { files: vec![(0, "Third/4.flac".to_string())], ..copy(None, "A", "Three", &[(1, "One", 200_000)]) };
        let moving = cat.copies_moving();
        assert_eq!(cat.assign_records(&[kept.clone(), left.clone(), arrived.clone()]).unwrap(), 0);
        assert_eq!((cat.track_of(0, "Third/4.flac"), cat.track_of(0, &second.files[1].1)), (Some(1), Some(2)));
        drop(moving);
        assert!(cat.assign_records(&[kept, left, arrived]).unwrap() > 0, "with none open, what is apart is apart");
    }

    /// A read takes seconds, and an action can end inside it: what was read is half way all the same.
    #[test]
    fn albums_read_half_way_let_nothing_leave_though_the_move_has_ended() {
        let cat = tracks(6);
        let (kept, second, _) = two_copies(&cat);
        let left = the_first(2, &second);
        let moving = cat.copies_moving();
        cat.reading_albums();
        drop(moving);
        assert_eq!(cat.assign_records(&[kept.clone(), left.clone()]).unwrap(), 0);
        cat.reading_albums();
        assert!(cat.assign_records(&[kept, left]).unwrap() > 0, "read with none open, what is apart is apart");
    }


    /// What stays of two copies' analyses must not hang on a map's order.
    #[test]
    fn of_two_copies_taken_at_once_the_older_id_keeps_its_audiomuse_row() {
        let cat = tracks(9);
        let kept = lying_in(&cat, "Kept", three([1, 2, 3]));
        cat.assign_records(std::slice::from_ref(&kept)).unwrap();
        let (second, third) = (lying_in(&cat, "Second", three([4, 5, 6])), lying_in(&cat, "Third", three([7, 8, 9])));
        cat.db().execute_batch("INSERT INTO audiomuse (track, energy) VALUES (4, 0.4), (7, 0.7);").unwrap();
        cat.assign_records(&[kept, second, third]).unwrap();
        let energy: f64 = cat.db().query_row("SELECT energy FROM audiomuse WHERE track = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(energy, 0.4);
    }

    /// Copies are held together by their ids once taken: a retag or a length fix in one is no reason to split them.
    #[test]
    fn copies_on_one_set_of_tracks_stay_copies_whatever_their_tags_say() {
        let cat = tracks(6);
        let (kept, second, record) = two_copies(&cat);
        let mut fixed = second.clone();
        fixed.facts.tracks[1].duration_ms = 90_000;
        fixed.facts.title = "another name".to_string();
        assert_eq!(cat.assign_records(&[kept, fixed]).unwrap(), 0);
        assert_eq!(cat.track_of(0, &second.files[0].1), Some(1));
        assert_eq!(record_of(&cat, 1), Some(record));
    }

    /// m1: the library votes with `vote_of`, so where shared tracks hold no majority both go by the free ones. An album of
    /// `Held`'s reading and a group of rows say the same.
    #[test]
    fn the_one_vote_goes_by_the_free_tracks_where_shared_ones_hold_no_majority() {
        let said = |record: i64, free: bool| Say { record: Some(record), free, back: false };
        assert_eq!(vote_of(&[said(1, false), said(1, false), said(2, true), said(2, true)], &|_| 0), Some(2), "t1, t2 shared on the lower, t3, t4 free");
        assert_eq!(vote_of(&[said(1, false), said(1, false), said(1, true), said(2, true)], &|_| 0), Some(1), "a majority of all comes first");
        assert_eq!(vote_of(&[said(1, false), said(2, false)], &|_| 0), None, "nothing free to go by");
        assert_eq!(vote_of(&[Say { record: Some(1), free: true, back: true }], &|_| 3), None, "one back from retirement stands alone: not enough of its record");
        assert_eq!(vote_of(&[Say { record: Some(1), free: true, back: true }], &|_| 0), Some(1));
    }

    #[test]
    fn a_retired_file_leaves_no_beats() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let (one, _) = heard(&cat, 0);
        assert_eq!(parts_at(&cat, 0, &one), (1, 1));
        std::fs::remove_file(d.join(&one)).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
        cat.identify().unwrap();
        assert_eq!(parts_at(&cat, 0, &one), (0, 0));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A track is the record's (0101): two copies hold its id, and it lives until the last of them goes.
    #[test]
    fn a_track_with_two_copies_retires_with_the_last() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let rel = "A - B (2020)/01 One.mp3";
        let one = cat.track_of(0, rel).unwrap();
        cat.set_tracks(&[((1, rel.to_string()), Some(one))]).unwrap();
        let lost = |dir: &PathBuf| {
            std::fs::remove_file(dir.join(rel)).unwrap();
            cat.walk_all();
            cat.db().execute("UPDATE files SET gone_at = gone_at - ?1 WHERE gone_at IS NOT NULL", [GONE_KEEP_SECS + 1]).unwrap();
            cat.identify().unwrap();
        };
        lost(&b);
        assert!(!retired(&cat, one), "a copy is left");
        assert_eq!(cat.file_of(one), Some((0, rel.to_string())));
        lost(&a);
        assert!(retired(&cat, one), "the last copy went");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// Lost track ids (0101, the owner): nothing is given out while they are lost; started afresh, the new ids come above
    /// the floor, so an old id names no new track.
    #[test]
    fn lost_ids_give_nothing_out_and_a_fresh_start_numbers_above_the_old() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.set_ids_lost(true);
        assert_eq!(cat.identify().unwrap(), (0, 0), "lost: no ids");
        assert_eq!(cat.track_of(0, "A - B (2020)/01 One.mp3"), None);
        cat.number_above(2_000_000).unwrap();
        cat.set_ids_lost(false);
        assert_eq!(cat.identify().unwrap(), (0, 2));
        assert!(cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap() > 2_000_000, "above every old id");
        assert!(cat.top_id() > 2_000_000);
        std::fs::remove_dir_all(&d).ok();
    }

    /// CR3: `records` starts at 1 in a new file, so without this `al-12` in a favourite, a tag or a crate quietly names
    /// another album, where a track's id names a missing one.
    #[test]
    fn a_fresh_start_numbers_records_above_every_old_album_id() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        cat.number_above(3_000_000).unwrap();
        let born = |cat: &Catalogue| {
            cat.db().execute("INSERT INTO records (born, title) VALUES (1, 'New')", []).unwrap();
            cat.db().last_insert_rowid()
        };
        assert_eq!(born(&cat), 3_000_001);
        assert_eq!(cat.top_id(), 3_000_001, "what the user data remembers next");
        cat.number_above(5).unwrap();
        assert_eq!(born(&cat), 3_000_002, "a floor never lowers");
        let fresh = Catalogue::open(None, Vec::new()).unwrap();
        assert_eq!(born(&fresh), 1, "as any new catalogue");
        assert_eq!(fresh.top_id(), 1, "a record counts though no track was given");
    }

    /// A better copy hands its id over (0101 point 11): the file arriving where Cratarr said takes the track, though no
    /// rule would pair it (another format, another length), and the replaced file's gone row lets go.
    #[test]
    fn a_file_handed_a_track_takes_it() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        assert_eq!(cat.place_of_path(&d.join("A - B (2020)/01 One.mp3")), Some((0, "A - B (2020)/01 One.mp3".to_string())));
        std::fs::remove_file(d.join("A - B (2020)/01 One.mp3")).unwrap();
        std::fs::write(d.join("A - B (2020)/01 One.flac"), b"better").unwrap();
        cat.hand_over(&d.join("A - B (2020)/01 One.flac"), &d.join("A - B (2020)/01 One.mp3"), one).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET state = 1, duration_ms = 260000 WHERE state = 0", []).unwrap();
        assert_eq!(cat.identify().unwrap(), (1, 0), "handed over: no new id");
        assert_eq!(cat.track_of(0, "A - B (2020)/01 One.flac"), Some(one));
        let rows: i64 = cat.db().query_row("SELECT COUNT(*) FROM files WHERE track = ?1", [one], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "the replaced file's row let go");
        let nowhere = std::env::temp_dir().join("nowhere/x.flac");
        assert!(cat.hand_over(&nowhere, &nowhere, one).is_err(), "in no library");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Each new file fits the gone rows of both copies: as a whole that is no pair, and fresh ids would split the track.
    #[test]
    fn both_copies_renamed_outside_keep_their_track() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let (one, two) = ("01 One.mp3", "02 Two.flac");
        let track_of = |name: &str| cat.track_of(0, &format!("A - B (2020)/{name}")).unwrap();
        let (track_one, track_two) = (track_of(one), track_of(two));
        cat.set_tracks(&[((1, format!("A - B (2020)/{one}")), Some(track_one)), ((1, format!("A - B (2020)/{two}")), Some(track_two))]).unwrap();
        for d in [&a, &b] {
            std::fs::rename(d.join("A - B (2020)"), d.join("A - B (2020) [FLAC]")).unwrap();
        }
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (4, 0), "each paired in its own folder: no new ids");
        for folder in [0, 1] {
            assert_eq!(cat.track_of(folder, &format!("A - B (2020) [FLAC]/{one}")), Some(track_one));
            assert_eq!(cat.track_of(folder, &format!("A - B (2020) [FLAC]/{two}")), Some(track_two));
        }
        assert!(!retired(&cat, track_one) && !retired(&cat, track_two));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    /// A copy is a record's files in one directory, so two can sit in one library: a rename above both leaves each new file
    /// fitting both gone rows on every rule, in its own folder too. They are one track's rows: fresh ids would retire it.
    #[test]
    fn both_copies_in_one_folder_renamed_outside_keep_their_track() {
        let d = music();
        let (one, two) = ("01 One.mp3", "02 Two.flac");
        std::fs::create_dir_all(d.join("A/B [MP3]")).unwrap();
        std::fs::rename(d.join("A - B (2020)"), d.join("A/B")).unwrap();
        for name in [one, two] {
            std::fs::copy(d.join("A/B").join(name), d.join("A/B [MP3]").join(name)).unwrap();
        }
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let (track_one, track_two) = (cat.track_of(0, &format!("A/B/{one}")).unwrap(), cat.track_of(0, &format!("A/B/{two}")).unwrap());
        cat.set_tracks(&[((0, format!("A/B [MP3]/{one}")), Some(track_one)), ((0, format!("A/B [MP3]/{two}")), Some(track_two))]).unwrap();
        std::fs::rename(d.join("A"), d.join("A again")).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (4, 0), "one track's gone rows are one fit: no new ids");
        for dir in ["A again/B", "A again/B [MP3]"] {
            assert_eq!(cat.track_of(0, &format!("{dir}/{one}")), Some(track_one));
            assert_eq!(cat.track_of(0, &format!("{dir}/{two}")), Some(track_two));
        }
        assert!(!retired(&cat, track_one) && !retired(&cat, track_two));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A better copy in one directory, the record's other copy in the same library away for a moment: only the file it
    /// replaces lets go, or that copy comes back with a fresh id and the track splits.
    #[test]
    fn a_claim_leaves_another_directorys_gone_copy_alone() {
        let d = music();
        let (rel, copy, better) = ("A - B (2020)/01 One.mp3", "A - B (2020) [MP3]/01 One.mp3", "A - B (2020)/01 One.flac");
        std::fs::create_dir_all(d.join("A - B (2020) [MP3]")).unwrap();
        std::fs::copy(d.join(rel), d.join(copy)).unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, rel).unwrap();
        cat.set_tracks(&[((0, copy.to_string()), Some(one))]).unwrap();
        std::fs::remove_file(d.join(rel)).unwrap();
        std::fs::write(d.join(better), b"better").unwrap();
        cat.hand_over(&d.join(better), &d.join(rel), one).unwrap();
        let away = d.join("one.away");
        std::fs::rename(d.join(copy), &away).unwrap();
        cat.walk_all();
        cat.db().execute("UPDATE files SET state = 1, duration_ms = 260000 WHERE state = 0", []).unwrap();
        assert_eq!(cat.identify().unwrap(), (1, 0), "handed over: no new id");
        assert_eq!(cat.track_of(0, better), Some(one));
        let gone: Vec<String> = {
            let db = cat.db();
            let mut st = db.prepare("SELECT rel FROM files WHERE track = ?1 AND gone_at IS NOT NULL").unwrap();
            st.query_map([one], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(gone, vec![copy.to_string()], "the replaced file let go; the other copy's row stayed");
        std::fs::rename(&away, d.join(copy)).unwrap();
        cat.walk_all();
        assert_eq!(cat.track_of(0, copy), Some(one), "back, the same track");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A mount away for a moment leaves a copy's rows gone: a better file claiming the track in another folder must leave
    /// them, or that copy comes back with a fresh id and the track splits.
    #[test]
    fn a_claim_leaves_another_folders_gone_copy_alone() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let rel = "A - B (2020)/01 One.mp3";
        let one = cat.track_of(0, rel).unwrap();
        cat.set_tracks(&[((1, rel.to_string()), Some(one))]).unwrap();
        let better = "A - B (2020)/01 One.flac";
        std::fs::remove_file(a.join(rel)).unwrap();
        std::fs::write(a.join(better), b"better").unwrap();
        cat.hand_over(&a.join(better), &a.join(rel), one).unwrap();
        let away = b.join("one.away");
        std::fs::rename(b.join(rel), &away).unwrap();
        cat.walk_all();
        assert_eq!(cat.file_of_any(one), Some((0, rel.to_string())), "both away: by folder and path");
        cat.db().execute("UPDATE files SET state = 1, duration_ms = 260000 WHERE state = 0", []).unwrap();
        assert_eq!(cat.identify().unwrap(), (1, 0), "handed over: no new id");
        assert_eq!(cat.track_of(0, better), Some(one));
        let rows = |folder: i64| -> i64 { cat.db().query_row("SELECT COUNT(*) FROM files WHERE track = ?1 AND folder = ?2", params![one, folder], |r| r.get(0)).unwrap() };
        assert_eq!((rows(0), rows(1)), (1, 1), "the replaced file let go; the other copy's row stayed");
        std::fs::rename(&away, b.join(rel)).unwrap();
        cat.walk_all();
        assert_eq!(cat.track_of(1, rel), Some(one), "back, the same track");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
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
        assert_eq!(cat.last_pass(), None, "no background pass in a test");
        cat.set_tracks(&[(old.clone(), None), (new.clone(), Some(one))]).unwrap();
        assert!(cat.last_pass().is_some(), "the library is told at once, not at its next check");
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

    /// The stable track ids (0101): a file's, by its library's root and its path there, and a playlist line's, as
    /// served; none for a file with no track, a library not in use, or a line in no folder.
    #[test]
    fn a_file_and_a_line_give_their_stable_track_id() {
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
        let (one, other) = (cat.track_of(0, "A/01.mp3").unwrap(), cat.track_of(1, "X/New/01.flac").unwrap());
        let song = crate::serve::ids::song;
        assert_eq!(cat.track_by_path(&m.to_string_lossy(), "A/01.mp3"), Some(song(one)));
        assert_eq!(cat.track_by_path(&inc.to_string_lossy(), "X/New/01.flac"), Some(song(other)));
        assert_eq!(cat.track_by_path(&m.to_string_lossy(), "A/02.mp3"), None, "no such file");
        assert_eq!(cat.track_by_path(&root.join("elsewhere").to_string_lossy(), "A/01.mp3"), None, "no such library");
        assert_eq!(cat.track_of_line("A/01.mp3"), Some(song(one)));
        assert_eq!(cat.track_of_line("../lidarr-music/X/New/01.flac"), Some(song(other)));
        assert_eq!(cat.track_of_line("../elsewhere/01.flac"), None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn crate_facts_hold_every_folder_in_use_each_track_by_its_line() {
        let root = music();
        let (m, inc) = (root.join("Music"), root.join("lidarr-music"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X/New")).unwrap();
        std::fs::write(m.join("A/01.mp3"), "a").unwrap();
        std::fs::write(inc.join("X/New/01.flac"), "x").unwrap();
        let cat = Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap();
        cat.walk_all();
        let facts = cat.crate_facts().unwrap();
        let mut lines: Vec<(i64, &str)> = facts.iter().map(|f| (f.folders[0], f.rel.as_str())).collect();
        lines.sort();
        assert_eq!(lines, [(0, "A/01.mp3"), (1, "../lidarr-music/X/New/01.flac")], "Incoming's climb out of Music");
        // Out of use, a folder's files leave the crates.
        cat.set_folders(vec![m.clone()]).unwrap();
        assert_eq!(cat.crate_facts().unwrap().iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), ["A/01.mp3"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_heard_track_reads_to_the_bit_as_the_library_made_it_and_is_one_vector_wherever_it_is_held() {
        let root = music();
        for n in 0..24 {
            std::fs::write(root.join(format!("A - B (2020)/{n:02}.mp3")), "a").unwrap();
        }
        let cat = Catalogue::open(None, vec![root.clone()]).unwrap();
        cat.walk_all();
        let heard = |track: i64| -> Vec<f32> {
            let mut r = cratarr_rules::Rng::seeded(track as u64);
            (0..200).map(|_| r.unit() as f32 - 0.5).collect()
        };
        for n in 0..24 {
            cat.db().execute("INSERT INTO tracks (id, born) VALUES (?1, 5)", [100 + n]).unwrap();
            cat.give_track_for_tests(0, &format!("A - B (2020)/{n:02}.mp3"), 100 + n);
        }
        let rows: Vec<AudioMuseRow> =
            (100..124).map(|track| AudioMuseRow { track, embedding: Some(heard(track).iter().flat_map(|x| x.to_le_bytes()).collect()), ..AudioMuseRow::default() }).collect();
        cat.replace_audiomuse(&rows).unwrap();
        let facts = cat.crate_facts().unwrap();
        let held: Vec<&crate::crates::Facts> = facts.iter().filter(|f| f.sound.is_some()).collect();
        assert_eq!(held.len(), 24);

        // Made so as the catalogue did, then as the library did what it was handed, when it kept a copy.
        let made_so = |v: &[f32]| -> Vec<f32> {
            let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / len).collect()
        };
        let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
        for f in &held {
            let once = made_so(&heard(f.track.unwrap()));
            assert_eq!(bits(f.sound.as_ref().unwrap()), bits(&made_so(&once)), "{}", f.rel);
        }
        assert!(held.iter().any(|f| bits(f.sound.as_ref().unwrap()) != bits(&made_so(&heard(f.track.unwrap())))), "once is not twice, or this says nothing");

        let alike = crate::crates::Alike::of(&facts, &HashMap::new(), &HashMap::new());
        let songs = facts.iter().map(|f| cratarr_rules::radio::Song { id: crate::serve::ids::song(f.track.unwrap_or_default()), ..Default::default() }).collect();
        let index = cratarr_rules::radio::Index::new(songs).with_facts(&facts);
        for (place, f) in facts.iter().enumerate() {
            let Some(sound) = &f.sound else { continue };
            assert!(alike.heard(place) && index.like.sound(place).is_some_and(|held| std::ptr::eq(held, &**sound)), "{}", f.rel);
            assert_eq!(std::sync::Arc::strong_count(sound), 3, "{}: the facts and the two libraries", f.rel);
        }
        std::fs::remove_dir_all(&root).ok();
    }

    /// One track with a file in Music and one in own music, told apart by their genres.
    fn one_track_twice() -> (PathBuf, Catalogue) {
        let root = music();
        let (m, own) = (root.join("Music"), root.join("own"));
        for dir in [&m, &own] {
            std::fs::create_dir_all(dir.join("A")).unwrap();
            std::fs::write(dir.join("A/01.mp3"), "a").unwrap();
        }
        let cat = Catalogue::open(None, vec![m, own]).unwrap();
        cat.walk_all();
        cat.db().execute("INSERT INTO tracks (id, born) VALUES (77, 5)", []).unwrap();
        cat.give_track_for_tests(0, "A/01.mp3", 77);
        cat.give_track_for_tests(1, "A/01.mp3", 77);
        cat.tag_for_tests(0, "A/01.mp3", &[("genre", "Dub")]);
        cat.tag_for_tests(1, "A/01.mp3", &[("genre", "Jazz")]);
        (root, cat)
    }

    #[test]
    fn a_track_in_two_folders_is_one_fact() {
        let (root, cat) = one_track_twice();
        cat.db().execute("INSERT INTO audiomuse (track, energy) VALUES (77, 0.5)", []).unwrap();
        let facts = cat.crate_facts().unwrap();
        assert_eq!(facts.len(), 1);
        let f = &facts[0];
        assert_eq!((f.rel.as_str(), f.lines.as_slice(), f.folders.as_slice()), ("A/01.mp3", ["../own/A/01.mp3".to_string()].as_slice(), [0, 1].as_slice()), "with no library: the first by folder");
        assert_eq!((f.track, f.genres.as_slice(), f.energy), (Some(77), ["Dub".to_string()].as_slice(), Some(50.0)), "the facts of the one it is read from");
        // With a library, the row copy's, wherever that lies.
        let row = HashMap::from([(77, Some((1, "A/01.mp3".to_string())))]);
        let read = Catalogue::read_crate_facts(&cat.db(), &cat.folders(), &row).unwrap();
        assert_eq!((read.len(), read[0].rel.as_str(), read[0].lines.as_slice(), read[0].folders.as_slice()), (1, "../own/A/01.mp3", ["A/01.mp3".to_string()].as_slice(), [1, 0].as_slice()));
        assert_eq!(read[0].genres, ["Jazz"]);
        // A row the catalogue no longer has: the first by folder, never nothing.
        let gone = HashMap::from([(77, Some((1, "Gone/01.mp3".to_string())))]);
        assert_eq!(Catalogue::read_crate_facts(&cat.db(), &cat.folders(), &gone).unwrap()[0].rel, "A/01.mp3");
        // Two files of no track are two.
        cat.db().execute("UPDATE files SET track = NULL", []).unwrap();
        assert_eq!(Catalogue::read_crate_facts(&cat.db(), &cat.folders(), &HashMap::new()).unwrap().len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn two_copies_count_once_in_a_crate() {
        let (root, cat) = one_track_twice();
        let facts = cat.crate_facts().unwrap();
        let membership = HashMap::new();
        let w = crate::crates::World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0, fits: None };
        let dub = crate::crates::Condition::new("genre", false, crate::crates::Value::AnyOf { values: vec!["Dub".into()] });
        assert_eq!(crate::crates::select(facts.iter(), &[dub], &w), ["A/01.mp3"]);
        let one: Vec<[&crate::crates::Facts; 1]> = facts.iter().map(|f| [f]).collect();
        let items: Vec<&[&crate::crates::Facts]> = one.iter().map(|f| f.as_slice()).collect();
        let genre = crate::crates::list_axes(&items, &[], None, None, 0).into_iter().find(|a| a.key == "genre").unwrap();
        assert_eq!((genre.values, genre.counts), (vec!["Dub".to_string()], vec![1]));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_line_on_any_copy_has_its_track() {
        let (root, cat) = one_track_twice();
        let own = crate::source::cratarr::KIND.id;
        let song = Some("tr-77".to_string());
        let lines = ["A/01.mp3".to_string(), "../own/A/01.mp3".to_string(), "Nowhere/01.mp3".to_string()];
        assert_eq!(cat.line_track_ids(own, &lines).unwrap(), [song.clone(), song.clone(), None]);
        assert_eq!(cat.line_rows(own, &lines).unwrap().into_iter().map(|r| r.track_id).collect::<Vec<_>>(), [song.clone(), song.clone(), None]);
        assert_eq!(cat.track_ids(own, 1, &["A/01.mp3".to_string()]).unwrap(), [song]);
        // Another server's item is its own link to one file.
        cat.record_links("jf", "lib", &[("i1".to_string(), 0, "A/01.mp3".to_string())], 1).unwrap();
        assert_eq!(cat.line_track_ids("jf", &lines).unwrap(), [Some("i1".to_string()), None, None]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_tracks_line_is_its_row_copys() {
        let (root, cat) = one_track_twice();
        let (folders, files) = (cat.folders(), cat.files_of(77));
        let row = (1, "A/01.mp3".to_string());
        assert_eq!(row_line_of(&folders, &files, Some(&row)).as_deref(), Some("../own/A/01.mp3"));
        assert_eq!(row_line_of(&folders, &files, None).as_deref(), Some("A/01.mp3"), "no library to ask: the first by folder");
        assert_eq!(row_line_of(&folders, &files, Some(&(1, "Gone/01.mp3".to_string()))).as_deref(), Some("A/01.mp3"), "a row from before the file went");
        let own = crate::source::cratarr::KIND.id;
        assert_eq!(cat.lines_of(own, &["tr-77".to_string()]).unwrap(), ["A/01.mp3".to_string()].into_iter().collect(), "one line a track, as the crates know it");
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

    /// As if the facts were read longer ago than FACTS_EVERY.
    fn facts_grown_old(cat: &Catalogue) {
        if let Some(k) = cat.facts.lock().unwrap().as_mut() {
            k.at = std::time::Instant::now() - FACTS_EVERY - Duration::from_secs(1);
        }
    }

    #[test]
    fn crate_facts_are_kept_until_what_they_are_read_from_changes_and_a_while_has_passed() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        let first = cat.crate_facts().unwrap();
        assert!(!first.is_empty());
        assert!(Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "nothing changed: the same facts");
        cat.db().execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 0, 'GENRE', 'genre', 'Jazz')", [&first[0].rel]).unwrap();
        assert!(Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "a change just after: still the same (0.9.7)");
        facts_grown_old(&cat);
        let again = cat.crate_facts().unwrap();
        assert!(!Arc::ptr_eq(&first, &again), "a change a while after: read again");
        assert_eq!(again.iter().find(|f| f.rel == first[0].rel).unwrap().genres, ["Jazz"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn what_a_pass_over_an_unchanged_library_writes_leaves_the_crate_facts_kept() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let first = cat.crate_facts().unwrap();
        // A pass with nothing to do, to its end: `link_source` writes its time every five minutes, whatever it found.
        assert!(cat.walk_all().is_empty());
        assert_eq!(cat.read_tags_batch(TAG_BATCH).unwrap(), 0);
        cat.identify().unwrap();
        cat.set_meta("pass:cratarr", "1");
        // A media server's links, and what a track writes as it is first played: nothing a fact reads.
        cat.record_links("jellyfin", "lib", &[("t1".into(), 0, first[0].rel.clone())], 100).unwrap();
        cat.db().execute("INSERT INTO analysis_shape (folder, rel, spectrum) VALUES (0, ?1, x'00')", [&first[0].rel]).unwrap();
        facts_grown_old(&cat);
        assert!(Arc::ptr_eq(&first, &cat.crate_facts().unwrap()), "nothing they are read from changed");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_write_to_every_table_the_facts_are_read_from_counts() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let written = || cat.facts_written.load(Ordering::Relaxed);
        let rel = cat.crate_facts().unwrap()[0].rel.clone();
        let shape = |table: &str| format!("INSERT INTO {table} (folder, rel, size, mtime, {}) VALUES (0, ?1, 1, 1, {})", if table == "analysis" { "version, analysed_at" } else { "beats, downbeats, at" }, if table == "analysis" { "1, 1" } else { "x'00', x'00', 1" });
        let sql = [
            "UPDATE files SET bitrate = 320 WHERE rel = ?1".to_string(),
            "INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, 0, 'GENRE', 'genre', 'Jazz')".to_string(),
            "DELETE FROM tags WHERE rel = ?1".to_string(),
            "UPDATE tracks SET born = 1 WHERE id = (SELECT track FROM files WHERE rel = ?1)".to_string(),
            shape("analysis"),
            "UPDATE analysis SET bpm = 120 WHERE rel = ?1".to_string(),
            shape("beat_this"),
            "DELETE FROM beat_this WHERE rel = ?1".to_string(),
            "INSERT INTO audiomuse (track, energy) SELECT track, 1 FROM files WHERE rel = ?1".to_string(),
        ];
        for q in &sql {
            let before = written();
            assert_eq!(cat.db().execute(q, [&rel]).unwrap(), 1, "{q}");
            assert!(written() > before, "{q}");
        }
        // AudioMuse is taken in whole: a DELETE with no WHERE, which SQLite would not go through row by row.
        let before = written();
        cat.replace_audiomuse(&[]).unwrap();
        assert!(written() > before, "the whole table emptied");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_label_from_discogs_reads_the_crate_facts_again_as_a_word_does() {
        let s = crate::serve::Serve::open(None, Rules::default(), &|_| None).unwrap();
        let before = said_stamp(&s);
        assert_eq!(said_stamp(&s), before, "nothing said: nothing to read");
        s.userdata.set_discogs_facts("album:a", Some(r#"{"labels":[{"name":"Warp"}]}"#), 1).unwrap();
        assert_ne!(said_stamp(&s), before);
    }

    #[test]
    fn the_crate_facts_are_read_beside_the_catalogue_not_under_its_lock() {
        let d = music();
        let cat = Arc::new(Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![d.clone()]).unwrap());
        cat.walk_all();
        // A writer in the middle of something long, as far as the read can tell.
        let lock = cat.db();
        let (c, (tell, told)) = (cat.clone(), std::sync::mpsc::channel());
        std::thread::spawn(move || tell.send(c.crate_facts().unwrap().len()));
        let read = told.recv_timeout(Duration::from_secs(20));
        drop(lock);
        assert_eq!(read, Ok(2));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_crate_facts_before_are_let_go_before_the_new_ones_are_read() {
        let d = music();
        let cat = Arc::new(Catalogue::open(None, vec![d.clone()]).unwrap());
        cat.walk_all();
        let before = Arc::downgrade(&cat.crate_facts().unwrap());
        assert_eq!(before.strong_count(), 1, "only what is kept holds them");
        cat.db().execute("UPDATE files SET bitrate = 320", []).unwrap();
        facts_grown_old(&cat);
        // In memory the read has only the catalogue's own connection: held here, it stops just before its first row.
        let lock = cat.db();
        let c = cat.clone();
        let reading = std::thread::spawn(move || c.crate_facts().unwrap().len());
        let waited = std::time::Instant::now();
        while before.strong_count() > 0 && waited.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let gone = before.strong_count() == 0;
        drop(lock);
        assert_eq!(reading.join().unwrap(), 2);
        assert!(gone, "two readings at once over every file are what the memory limit cannot hold");
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

    /// 0101 point 1 (the owner, 2026-11-04): a folder keeps its id for life. A folder added, wherever it is asked for in
    /// the list, takes a new id and moves no other; one taken out moves no other either; the ids and the tracks under
    /// them stay put, so what names a folder by its id (album ids, Subsonic's folders, the journal) never points
    /// elsewhere.
    #[test]
    fn a_folder_keeps_its_id_for_life_whatever_comes_and_goes() {
        let (a, b, c) = (music(), music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let in_b = cat.track_of(1, "A - B (2020)/01 One.mp3").unwrap();
        let ids = |cat: &Catalogue| [&a, &b, &c].map(|p| cat.folders().id_of(p));
        // Asked for first, the new one still comes after the others.
        cat.set_folders(vec![c.clone(), a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        assert_eq!(ids(&cat), [Some(0), Some(1), Some(2)]);
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(in_b), "b's tracks where they were");
        // The first taken out: no other moves up into its place.
        cat.set_folders(vec![b.clone(), c.clone()]).unwrap();
        assert_eq!(ids(&cat), [None, Some(1), Some(2)]);
        assert_eq!(cat.folders().first().map(|(id, _)| id), Some(1), "lines start from the first in use");
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(in_b));
        // And back: its own id again.
        cat.set_folders(vec![a.clone(), b.clone(), c.clone()]).unwrap();
        assert_eq!(ids(&cat), [Some(0), Some(1), Some(2)]);
        assert_eq!(cat.status().unwrap().files, 6);
        for d in [&a, &b, &c] {
            std::fs::remove_dir_all(d).ok();
        }
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

    /// A mount is changed first, so a start finds its library not there and parks it, and Settings names the new place afterwards:
    /// taken for a new folder, every file of it read as new and every id it held lost. A copy of a folder is another matter: the one
    /// parked still holds its files, so it stays what it is, to come back.
    #[test]
    fn a_folder_renamed_first_and_named_after_keeps_its_id_and_its_tracks() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(1, "A - B (2020)/01 One.mp3").unwrap();
        let renamed = b.with_extension("renamed");
        std::fs::rename(&b, &renamed).unwrap();
        cat.set_folders(vec![a.clone()]).unwrap();
        cat.set_folders(vec![a.clone(), renamed.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&renamed), Some(1), "its own id");
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(one), "and its rows");

        cat.set_folders(vec![a.clone()]).unwrap();
        let copy = renamed.with_extension("copy");
        std::fs::create_dir_all(copy.join("A - B (2020)")).unwrap();
        for file in ["01 One.mp3", "02 Two.flac"] {
            std::fs::copy(renamed.join("A - B (2020)").join(file), copy.join("A - B (2020)").join(file)).unwrap();
        }
        cat.set_folders(vec![a.clone(), copy.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&copy), Some(2), "a folder of its own");
        cat.set_folders(vec![a.clone(), copy.clone(), renamed.clone()]).unwrap();
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(one), "the one parked comes back as itself");
        for d in [&a, &renamed, &copy] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A1: a place stands in for a file in these tables, so a new one left out of `FILE_TABLES` stays where its file left, as a
    /// waveform once did. `handovers` is the one that is not a file's: it is a promise about a place a file is on its way to, kept
    /// through what lies there now going, and taken along it would be gone before the file came. It goes by the folder's id, which
    /// is for life, so a parked folder finds it on its return, and it ends by itself within the day.
    #[test]
    fn every_table_by_folder_and_path_goes_with_its_file() {
        const OF_NO_FILE: [&str; 1] = ["handovers"];
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let tables: Vec<String> = {
            let db = cat.db();
            let mut st = db
                .prepare(
                    "SELECT m.name FROM sqlite_master m WHERE m.type = 'table'
                       AND EXISTS (SELECT 1 FROM pragma_table_info(m.name) WHERE name = 'folder')
                       AND EXISTS (SELECT 1 FROM pragma_table_info(m.name) WHERE name = 'rel') ORDER BY m.name",
                )
                .unwrap();
            st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        for table in &tables {
            assert!(FILE_TABLES.contains(&table.as_str()) || OF_NO_FILE.contains(&table.as_str()), "{table} goes by folder and path: add it to FILE_TABLES, or say why not");
        }
        let named: Vec<&str> = FILE_TABLES.iter().chain(&OF_NO_FILE).copied().collect();
        assert_eq!(named.len(), tables.len(), "a table named is a table there: {named:?} against {tables:?}");
    }

    /// A library's cover files are parked with it and go when it is forgotten, or they stay for good under an id nothing will hold.
    #[test]
    fn a_librarys_cover_files_go_where_it_goes() {
        let (a, b) = (music(), music());
        std::fs::write(b.join("A - B (2020)/cover.jpg"), b"a picture").unwrap();
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        let print = cat.cover_prints().unwrap();
        assert!(print.contains_key(&(1, "A - B (2020)".to_string())), "{print:?}");
        cat.set_folders(vec![a.clone()]).unwrap();
        assert!(!cat.cover_prints().unwrap().contains_key(&(1, "A - B (2020)".to_string())), "parked with its library");
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        assert_eq!(cat.cover_prints().unwrap(), print, "back with it");

        cat.set_folders(vec![a.clone()]).unwrap();
        std::fs::remove_dir_all(&b).unwrap();
        std::fs::create_dir_all(b.join("Other")).unwrap();
        std::fs::write(b.join("Other/01 Other.mp3"), b"other files at its path").unwrap();
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&b), Some(2), "a new library");
        assert!(cat.cover_prints().unwrap().is_empty(), "nothing left of the one forgotten");
        for d in [&a, &b] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A4: what no file answers for, each with where it lay and since when: a library out of use, a track with nothing but gone files,
    /// and one retired. A file gone beside another of its track is no track missing, and what a take took is not missing at all.
    #[test]
    fn the_missing_list_says_what_has_no_file_and_since_when() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let rel = "A - B (2020)/01 One.mp3";
        let (gone, other) = (cat.track_of(0, rel).unwrap(), cat.track_of(0, "A - B (2020)/02 Two.flac").unwrap());
        cat.tag_for_tests(0, rel, &[("title", "One"), ("artist", "A"), ("album", "B")]);
        std::fs::remove_file(a.join(rel)).unwrap();
        cat.walk_all();
        cat.set_folders(vec![a.clone()]).unwrap();
        let missing = cat.missing().unwrap();
        assert_eq!((missing.parked.len(), missing.parked[0].path.as_str(), missing.parked[0].files), (1, &*b.display().to_string(), 2));
        assert!(missing.parked[0].since.is_some(), "since when");
        assert_eq!(missing.gone.len(), 1);
        assert_eq!((missing.gone[0].track, missing.gone[0].title.as_deref(), missing.gone[0].album.as_deref(), missing.gone[0].rel.as_deref()), (gone, Some("One"), Some("B"), Some(rel)));
        assert!(missing.retired.is_empty());

        cat.gone_long_ago_for_tests();
        cat.identify().unwrap();
        let missing = cat.missing().unwrap();
        assert!(missing.gone.is_empty(), "its day over");
        assert_eq!(missing.retired.iter().map(|t| (t.track, t.title.as_deref())).collect::<Vec<_>>(), [(gone, Some("One"))], "it is remembered");
        cat.reconnect(gone, other).unwrap();
        assert!(cat.missing().unwrap().retired.is_empty(), "what was taken is not missing");
        for d in [&a, &b] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A gone file with another of its track still here plays from that one: nothing is missing.
    #[test]
    fn a_file_gone_beside_another_of_its_track_is_no_track_missing() {
        let cat = tracks(6);
        let (_, second, _) = two_copies(&cat);
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE rel = ?1", [&second.files[0].1]).unwrap();
        assert!(cat.missing().unwrap().gone.is_empty());
    }

    /// A5: a person's word, as a take: the old id leads to the track chosen, for good, and what names it is there to follow.
    #[test]
    fn a_track_reconnected_by_hand_is_taken_by_the_one_chosen() {
        let cat = tracks(2);
        file_of_track(&cat, "A/2.flac", 2);
        cat.db().execute("UPDATE tracks SET retired_at = 5, was_title = 'Gone', record = NULL WHERE id = 1", []).unwrap();
        cat.reconnect(1, 2).unwrap();
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(1, 2)]));
        assert_eq!(cat.tracks_to_follow().unwrap(), [(1, 2)]);
        assert_eq!(cat.track_of_source(crate::source::cratarr::KIND.id, "tr-1"), Some(2), "an old id plays what took it");
        assert_eq!(cat.remembered(1).unwrap().title.as_deref(), Some("Gone"), "what it was stays");
        assert!(retired(&cat, 1));
    }

    /// One gone a while has its file's row still: that goes too, and comes to nothing when its day is over.
    #[test]
    fn a_track_gone_a_while_can_be_reconnected_and_lets_go_of_its_file() {
        let cat = tracks(2);
        file_of_track(&cat, "A/1.flac", 1);
        file_of_track(&cat, "B/2.flac", 2);
        cat.db().execute("UPDATE files SET gone_at = 1 WHERE track = 1", []).unwrap();
        cat.db().execute("UPDATE tracks SET was_title = NULL WHERE id = 1", []).unwrap();
        cat.tag_for_tests(0, "A/1.flac", &[("title", "Was")]);
        cat.reconnect(1, 2).unwrap();
        assert!(retired(&cat, 1));
        assert_eq!(cat.remembered(1).unwrap().title.as_deref(), Some("Was"), "read before its file went to the other");
        assert_eq!(cat.files_of(2).len(), 1, "the one to play");
    }

    #[test]
    fn a_track_is_reconnected_only_from_nothing_to_something() {
        let cat = tracks(4);
        file_of_track(&cat, "A/1.flac", 1);
        file_of_track(&cat, "A/2.flac", 2);
        cat.db().execute("UPDATE tracks SET retired_at = 5 WHERE id IN (3, 4)", []).unwrap();
        assert!(cat.reconnect(1, 2).is_err(), "it has a file");
        assert!(cat.reconnect(3, 3).is_err(), "itself");
        assert!(cat.reconnect(3, 4).is_err(), "onto one that has none");
        assert!(cat.reconnect(99, 2).is_err(), "not known");
        cat.reconnect(3, 2).unwrap();
        assert!(cat.reconnect(3, 1).is_err(), "taken already: a take stands");
        assert_eq!(cat.moved_tracks().unwrap(), HashMap::from([(3, 2)]));
    }

    /// A record of tracks all gone on goes with the last of them, as a take leaves it.
    #[test]
    fn a_record_whose_last_track_was_reconnected_retires() {
        let cat = tracks(2);
        cat.assign_records(&[album("Gone", &[1])]).unwrap();
        let record = record_of(&cat, 1).unwrap();
        cat.db().execute("UPDATE tracks SET retired_at = NULL WHERE id = 1", []).unwrap();
        file_of_track(&cat, "A/2.flac", 2);
        cat.reconnect(1, 2).unwrap();
        assert!(record_retired(&cat, record));
    }

    /// A3: a library out of use is forgotten by nobody's clock: however long it is parked, its rows wait, and it comes back as itself,
    /// the same ids on the same files, with nothing read again.
    #[test]
    fn a_library_parked_however_long_comes_back_as_itself() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(1, "A - B (2020)/01 One.mp3").unwrap();
        cat.set_folders(vec![a.clone()]).unwrap();
        let parked: Vec<(String, String)> = {
            let db = cat.db();
            let mut st = db.prepare("SELECT key, value FROM meta WHERE key LIKE 'parked:%'").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(parked.len(), 1, "since when it is parked is kept");
        let years_ago = now() - 10 * 365 * 24 * 3600;
        cat.db().execute("UPDATE meta SET value = ?2 WHERE key = ?1", params![parked[0].0, years_ago.to_string()]).unwrap();
        // Every start and every change of the libraries looks at it again.
        cat.set_folders(vec![a.clone()]).unwrap();
        let (rows, retired): (i64, i64) = cat.db().query_row("SELECT (SELECT count(*) FROM files WHERE folder < 0), (SELECT count(*) FROM tracks WHERE retired_at IS NOT NULL)", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((rows, retired), (2, 0), "its rows wait, and no track of it retires");
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&b), Some(1), "its own id");
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(one), "its own track");
        for d in [&a, &b] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// What names a library by its path hears where it went, however it came there: renamed in one breath, or parked on the way by a
    /// mount changed first. Until that has followed it says the path that is named, and a library back where it was moved nowhere.
    #[test]
    fn a_library_that_changed_path_says_where_it_was_until_that_has_followed() {
        let (a, b) = (music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        let at = |p: &PathBuf| p.display().to_string();
        let renamed = b.with_extension("renamed");
        std::fs::rename(&b, &renamed).unwrap();
        cat.set_folders(vec![a.clone(), renamed.clone()]).unwrap();
        assert_eq!(cat.paths_to_follow().unwrap(), [(at(&b), at(&renamed))], "in one breath");

        let again = renamed.with_extension("again");
        std::fs::rename(&renamed, &again).unwrap();
        cat.set_folders(vec![a.clone(), again.clone()]).unwrap();
        let moved = cat.paths_to_follow().unwrap();
        assert_eq!(moved, [(at(&b), at(&again))], "moved on first: what names it still says the first path");
        cat.paths_followed(&moved).unwrap();
        assert!(cat.paths_to_follow().unwrap().is_empty(), "told once");

        let third = again.with_extension("third");
        std::fs::rename(&again, &third).unwrap();
        cat.set_folders(vec![a.clone()]).unwrap();
        assert!(cat.paths_to_follow().unwrap().is_empty(), "parked, it has gone nowhere yet");
        cat.set_folders(vec![a.clone(), third.clone()]).unwrap();
        let moved = cat.paths_to_follow().unwrap();
        assert_eq!(moved, [(at(&again), at(&third))], "in two moves");

        std::fs::rename(&third, &again).unwrap();
        cat.set_folders(vec![a.clone(), again.clone()]).unwrap();
        assert!(cat.paths_to_follow().unwrap().is_empty(), "there and back before it followed: nothing to say");
        cat.paths_followed(&moved).unwrap();
        cat.set_folders(vec![a.clone()]).unwrap();
        cat.set_folders(vec![a.clone(), again.clone()]).unwrap();
        assert!(cat.paths_to_follow().unwrap().is_empty(), "out of use and back at its own path");
        for d in [&a, &again] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A2 at the door: a file with no tags to speak of, moved and renamed, keeps its track by its size, time and length. Walked in
    /// two passes, as a `mv` is seen: gone from one place, new at another, with what the disk says of it unchanged.
    #[test]
    fn a_file_moved_and_renamed_with_no_tags_keeps_its_track() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        std::fs::create_dir_all(d.join("Elsewhere")).unwrap();
        std::fs::rename(d.join("A - B (2020)/01 One.mp3"), d.join("Elsewhere/first take.mp3")).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (1, 0), "recognised: no id given");
        assert_eq!(cat.track_of(0, "Elsewhere/first take.mp3"), Some(one));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A file renamed or moved outside Cratarr is the same sound under another name: what was worked out of it goes with it, or its
    /// waveform is out of view until the pass has gone over it again, and what it left stays behind with nothing to say whose it is.
    #[test]
    fn a_file_moved_outside_cratarr_keeps_what_was_worked_out_of_its_sound() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let (one, _) = heard(&cat, 0);
        std::fs::rename(d.join("A - B (2020)"), d.join("A - B (2020) moved")).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        assert_eq!(cat.identify().unwrap(), (2, 0), "recognised");
        let moved = one.replace("A - B (2020)", "A - B (2020) moved");
        assert_eq!(cat.analysis_of(0, &moved).map(|a| a.waveform.len()), Some(crate::analysis::WAVEFORM_BINS), "its analysis, as it is the file it was");
        assert_eq!(parts_at(&cat, 0, &moved), (1, 1), "its shape and beats");
        assert_eq!(parts_at(&cat, 0, &one), (0, 0), "nothing left behind");
        let left: i64 = cat.db().query_row("SELECT count(*) FROM analysis WHERE rel = ?1", [&one], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
        std::fs::remove_dir_all(&d).ok();
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
        let found: Vec<(i64, String, Option<u64>)> = cat.files_of_sources(&asked).into_iter().map(|(_, mut files)| files.remove(0)).collect();
        assert_eq!(found, each(&asked));
        assert_eq!(found.len(), 3, "up to the first it has no file of: {found:?}");
        assert_eq!(found[1], (0, "A - B (2020)/02 Two.flac".to_string(), Some(200_000)), "a server's id, linked");
        assert!(cat.files_of_sources(&[]).is_empty());
        std::fs::remove_dir_all(&a).ok();
    }

    #[test]
    fn a_folder_that_reads_empty_keeps_its_rows_and_says_so_and_nothing_is_lost_when_it_is_back() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let rel = "A - B (2020)/01 One.mp3";
        let one = cat.track_of(0, rel).unwrap();
        cat.record_links("jf", "music", &[("j-1".into(), 0, rel.into())], 1).unwrap();
        let (size, mtime): (i64, i64) = cat.db().query_row("SELECT size, mtime FROM files WHERE rel = ?1", [rel], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        cat.store_analysis(0, rel, size as u64, mtime, &an_analysis(-3.0, 500)).unwrap();
        // The mount not up: a folder there, readable, nothing in it.
        let away = d.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&d, &away).unwrap();
        std::fs::create_dir_all(&d).unwrap();

        let problems = cat.walk_all();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains(&d.display().to_string()), "{problems:?}");
        assert_eq!(cat.status().unwrap().files, 2, "the library lists what it listed");
        assert_eq!(cat.track_of_source("jf", "j-1"), Some(one), "its links to the servers stand");
        let gone: i64 = cat.db().query_row("SELECT COUNT(*) FROM files WHERE gone_at IS NOT NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(gone, 0, "so no day's wait runs out on them");
        cat.identify().unwrap();
        assert_eq!(cat.track_of(0, rel), Some(one));
        assert!(cat.analysis_of(0, rel).is_some());

        std::fs::remove_dir_all(&d).unwrap();
        std::fs::rename(&away, &d).unwrap();
        assert!(cat.walk_all().is_empty(), "back: nothing to say");
        assert_eq!(cat.track_of(0, rel), Some(one));
        assert!(cat.analysis_of(0, rel).is_some(), "nothing to analyse again");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_library_keep_and_delete_emptied_is_no_problem() {
        let (a, b, c) = (music(), music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone(), c.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        // Keep of a's last album: the files and the rows go together.
        std::fs::rename(a.join("A - B (2020)"), b.join("C - D (2021)")).unwrap();
        cat.move_files(0, "A - B (2020)", 1, "C - D (2021)").unwrap();
        // Delete of c's: into its trash, the rows marked.
        let token = crate::trash::trash(&c, "A - B (2020)", 100).unwrap();
        cat.mark_trashed(2, "A - B (2020)", &token).unwrap();
        assert_eq!(cat.walk_all(), Vec::<String>::new());
        for d in [a, b, c] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_folder_that_was_never_anything_but_empty_is_no_problem() {
        let d = std::env::temp_dir().join(format!("cratarr-cat-empty-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        assert!(cat.walk_all().is_empty());
        std::fs::remove_dir_all(d).ok();
    }

    /// A folder of ten albums of ten tracks, `A00` first, read and given track ids.
    fn shelf() -> (PathBuf, Catalogue) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-shelf-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for album in 0..10 {
            shelve(&d, album);
        }
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        (d, cat)
    }

    fn shelve(at: &Path, album: usize) {
        std::fs::create_dir_all(at.join(format!("A{album:02}"))).unwrap();
        for track in 0..10 {
            std::fs::write(at.join(format!("A{album:02}/{track:02}.mp3")), b"not really an mp3").unwrap();
        }
    }

    fn elsewhere(d: &Path) -> PathBuf {
        let e = d.with_extension("elsewhere");
        let _ = std::fs::remove_dir_all(&e);
        std::fs::create_dir_all(&e).unwrap();
        std::fs::write(e.join("kept.mp3"), b"another folder").unwrap();
        e
    }

    #[test]
    fn the_album_that_sorts_first_gone_from_a_parked_folder_forgets_nothing_else() {
        let (d, cat) = shelf();
        let other = elsewhere(&d);
        let track = cat.track_of(0, "A05/03.mp3").unwrap();
        cat.set_folders(vec![other.clone()]).unwrap();
        std::fs::remove_dir_all(d.join("A00")).unwrap();
        cat.set_folders(vec![d.clone(), other.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&d), Some(0), "its own id");
        assert_eq!(cat.track_of(0, "A05/03.mp3"), Some(track), "the rest of its rows came back with it");
        for d in [d, other] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_parked_folder_with_most_of_its_files_gone_is_another_folder() {
        let (d, cat) = shelf();
        let other = elsewhere(&d);
        cat.set_folders(vec![other.clone()]).unwrap();
        for album in 0..7 {
            std::fs::remove_dir_all(d.join(format!("A{album:02}"))).unwrap();
        }
        cat.set_folders(vec![d.clone(), other.clone()]).unwrap();
        assert_ne!(cat.folders().id_of(&d), Some(0));
        for d in [d, other] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_mount_moved_with_an_album_lost_keeps_the_rest_of_its_rows() {
        let (d, cat) = shelf();
        let track = cat.track_of(0, "A05/03.mp3").unwrap();
        let moved = d.with_extension("moved");
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&d, &moved).unwrap();
        std::fs::remove_dir_all(moved.join("A00")).unwrap();
        cat.set_folders(vec![moved.clone()]).unwrap();
        assert_eq!(cat.track_of(0, "A05/03.mp3"), Some(track));
        std::fs::remove_dir_all(moved).ok();
    }

    /// Two libraries made by one template can hold the same album under the same names: what is first in the one is no
    /// proof that the other is the same library.
    #[test]
    fn a_folder_alike_in_its_first_albums_is_not_handed_another_folder_s_rows() {
        let (d, cat) = shelf();
        let alike = d.with_extension("alike");
        let _ = std::fs::remove_dir_all(&alike);
        shelve(&alike, 0);
        shelve(&alike, 1);
        cat.set_folders(vec![alike.clone()]).unwrap();
        assert_eq!(cat.track_of(0, "A00/00.mp3"), None, "those rows stay parked");
        assert_ne!(cat.folders().id_of(&alike), Some(0));
        for d in [d, alike] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_mount_not_up_when_its_library_is_switched_on_forgets_nothing() {
        let (d, cat) = shelf();
        let other = elsewhere(&d);
        let track = cat.track_of(0, "A00/00.mp3").unwrap();
        cat.set_folders(vec![other.clone()]).unwrap();
        // A folder there, as a mount point is, and nothing in it.
        let away = d.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&d, &away).unwrap();
        std::fs::create_dir_all(d.join(".cratarr-trash")).unwrap();
        cat.set_folders(vec![d.clone(), other.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&d), Some(0));
        assert_eq!(cat.track_of(0, "A00/00.mp3"), Some(track), "its rows are its own again");
        assert_eq!(cat.walk_all().len(), 1, "said so");
        assert_eq!(cat.track_of(0, "A00/00.mp3"), Some(track), "and the walk keeps them");
        for d in [d, other, away] {
            std::fs::remove_dir_all(d).ok();
        }
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
        // Back, whatever its place in the list asked: under its own folder id, with the same track id, and the servers
        // list again.
        cat.set_folders(vec![b.clone(), a.clone()]).unwrap();
        assert_eq!(cat.folders().id_of(&b), Some(1), "its own id for life");
        assert_eq!(cat.track_of(1, "A - B (2020)/01 One.mp3"), Some(one), "the same id after the break");
        assert_eq!(cat.meta("full-folders:jf"), None, "links checked again after a break");
        assert_eq!(cat.status().unwrap().files, 4);
        // Parked, then other files at that path: forgotten, and back as a new folder with an id never given before.
        cat.set_folders(vec![a.clone()]).unwrap();
        std::fs::remove_dir_all(&b).unwrap();
        std::fs::create_dir_all(b.join("X")).unwrap();
        std::fs::write(b.join("X/other.mp3"), "x").unwrap();
        cat.set_folders(vec![a.clone(), b.clone()]).unwrap();
        cat.walk_all();
        assert_eq!(cat.folders().id_of(&b), Some(2), "its old id is never given again");
        assert_eq!(cat.track_of(2, "A - B (2020)/01 One.mp3"), None, "those files are gone: their rows with them");
        assert_eq!(cat.status().unwrap().files, 3);
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }
}
