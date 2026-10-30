//! What each user did with the music (decision 0030): plays, last played, ratings, stars, the saved play queue, and
//! when an album was first added where that is known better than the catalogue does (imported from a media server).
//! Its own SQLite file, never derived from anything: it belongs in backups. Nothing here is removed when a file goes
//! missing; a song comes back to its data by its id, or by its persistent id once the catalogue renumbered its tracks.
//! An import from a media server also keeps what it last brought in per item (`imported`), so the next one adds only
//! what changed on the server since (`serve::import`).
//!
//! The annotation table is Navidrome's (`db/migrations`, `persistence/sql_annotations.go`,
//! https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors), translated: one row per
//! user and item with plays, last played, rating and star, unique on (user, item, kind). Changed: songs also keep
//! their persistent id; the play queue holds song ids as text. Bookmarks (a position kept in a song) are Navidrome's
//! too (`persistence/sql_bookmarks.go`), one per user and song; changed: they follow the song by its persistent id, as
//! its annotation does.

#![allow(dead_code)] // the endpoints that use it come next (decision 0030, plan step 2)

pub mod tags;
pub mod words;
pub mod works;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

// journal_size_limit: as the catalogue's (the log's file cut back after a checkpoint).
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS annotation (
  user TEXT NOT NULL, item TEXT NOT NULL, kind TEXT NOT NULL, pid TEXT,
  play_count INTEGER NOT NULL DEFAULT 0, play_date INTEGER, rating INTEGER NOT NULL DEFAULT 0,
  starred INTEGER NOT NULL DEFAULT 0, starred_at INTEGER,
  PRIMARY KEY (user, item, kind)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS annotation_starred ON annotation (user, kind, starred, starred_at);
CREATE INDEX IF NOT EXISTS annotation_played ON annotation (user, kind, play_date);
CREATE INDEX IF NOT EXISTS annotation_plays ON annotation (user, kind, play_count);
CREATE INDEX IF NOT EXISTS annotation_pid ON annotation (pid);
CREATE TABLE IF NOT EXISTS play_queue (
  user TEXT PRIMARY KEY, songs TEXT NOT NULL, current TEXT, position_ms INTEGER NOT NULL DEFAULT 0,
  changed INTEGER NOT NULL, changed_by TEXT);
CREATE TABLE IF NOT EXISTS app_queue (
  user TEXT NOT NULL, client TEXT NOT NULL, songs TEXT NOT NULL, current INTEGER, position_ms INTEGER NOT NULL DEFAULT 0,
  changed INTEGER NOT NULL, PRIMARY KEY (user, client)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS added (album TEXT PRIMARY KEY, at INTEGER NOT NULL, origin TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS playlist_seen (name TEXT PRIMARY KEY, print TEXT NOT NULL, created INTEGER NOT NULL, changed INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS album_review (album TEXT PRIMARY KEY, base TEXT NOT NULL, state TEXT NOT NULL, at INTEGER NOT NULL, undo TEXT,
  name TEXT NOT NULL, album_artist TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS album_review_by_base ON album_review (base);
CREATE TABLE IF NOT EXISTS imported (
  account TEXT NOT NULL, source TEXT NOT NULL, server_user TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL,
  server_item TEXT, pid TEXT, plays INTEGER NOT NULL DEFAULT 0, played INTEGER, starred INTEGER NOT NULL DEFAULT 0,
  rating INTEGER NOT NULL DEFAULT 0, at INTEGER NOT NULL,
  PRIMARY KEY (account, source, server_user, kind, item)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS imported_pid ON imported (pid);
CREATE TABLE IF NOT EXISTS import_choices (source TEXT PRIMARY KEY, choice TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS bookmark (
  user TEXT NOT NULL, item TEXT NOT NULL, pid TEXT, position_ms INTEGER NOT NULL, comment TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL, changed INTEGER NOT NULL, changed_by TEXT,
  PRIMARY KEY (user, item)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS bookmark_pid ON bookmark (pid);
CREATE TABLE IF NOT EXISTS playlist_alias (old TEXT PRIMARY KEY, name TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS playlist_entry (
  playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER NOT NULL, added INTEGER NOT NULL,
  PRIMARY KEY (playlist, entry, n)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS visual_snapshot (
  user TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, settings TEXT NOT NULL, created INTEGER NOT NULL, picture TEXT,
  PRIMARY KEY (user, id));
CREATE TABLE IF NOT EXISTS visual_rating (
  user TEXT NOT NULL, at INTEGER NOT NULL, a TEXT NOT NULL, b TEXT NOT NULL, choice TEXT NOT NULL, track TEXT,
  context TEXT);
CREATE TABLE IF NOT EXISTS visual_thumb (user TEXT NOT NULL, at INTEGER NOT NULL, thumb TEXT NOT NULL, body TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS visual_limits (
  user TEXT PRIMARY KEY, version INTEGER NOT NULL, limits TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS playlist_shared (playlist TEXT PRIMARY KEY, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS user_settings (user TEXT PRIMARY KEY, version INTEGER NOT NULL, settings TEXT NOT NULL, at INTEGER NOT NULL);
-- Every track heard (decision 0064): only added to, kept forever. `event`: the player's own id for it, so a report sent
-- again is one row.
CREATE TABLE IF NOT EXISTS plays (
  user TEXT NOT NULL, song TEXT NOT NULL, pid TEXT, at INTEGER NOT NULL, heard_secs REAL, length_secs REAL,
  ended TEXT NOT NULL, counted INTEGER NOT NULL, player TEXT, output TEXT, context TEXT, event TEXT);
CREATE INDEX IF NOT EXISTS plays_by_user ON plays (user, at);
CREATE UNIQUE INDEX IF NOT EXISTS plays_by_event ON plays (event) WHERE event IS NOT NULL;
-- The listening services an account sends its plays to (app::scrobble): the service's user, what it needs to sign in
-- (JSON), and why it stopped (its login refused) until connected again; `off`: switched off in Connections (the owner,
-- 2026-10-24), its sign-in kept and nothing queued meanwhile.
CREATE TABLE IF NOT EXISTS scrobble_target (
  user TEXT NOT NULL, target TEXT NOT NULL, name TEXT NOT NULL, secret TEXT NOT NULL, broken TEXT, at INTEGER NOT NULL,
  PRIMARY KEY (user, target)) WITHOUT ROWID;
-- Plays to send, one per service, oldest first; `next` when to try again, `why` the last failure.
CREATE TABLE IF NOT EXISTS scrobble_queue (
  id INTEGER PRIMARY KEY, user TEXT NOT NULL, target TEXT NOT NULL, song TEXT NOT NULL, at INTEGER NOT NULL,
  tries INTEGER NOT NULL DEFAULT 0, next INTEGER NOT NULL DEFAULT 0, why TEXT);
CREATE INDEX IF NOT EXISTS scrobble_queue_due ON scrobble_queue (next, at);
-- Discover (decision 0069): each account's suggestions as last looked up (JSON, app::discover::Found), and the albums
-- it answered: `no` (Not interested: never again, and an artist twice so is left out), `hidden` (Hide: gone from the
-- list, nothing said of the artist; the owner, 2026-10-24) or `wanted` (asked of Lidarr, before 0.9's requests).
CREATE TABLE IF NOT EXISTS discover_found (user TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT NOT NULL) WITHOUT ROWID;
-- Discover's Around artists you play (0.9.7): each account's seeds shown, the group showing and the next (JSON,
-- app::around::State).
CREATE TABLE IF NOT EXISTS discover_around (user TEXT PRIMARY KEY, state TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS discover_answer (
  user TEXT NOT NULL, album TEXT NOT NULL, artist TEXT NOT NULL, answer TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (user, album)) WITHOUT ROWID;
-- Requests (decision 0070): an album asked for, by whom and when, its own status (pending, approved, declined,
-- cancelled) with who decided and why, and what the fetcher said of it (the rights are the account's, serve::users).
CREATE TABLE IF NOT EXISTS request (
  id INTEGER PRIMARY KEY AUTOINCREMENT, user TEXT NOT NULL, album_key TEXT NOT NULL, artist_key TEXT NOT NULL,
  release_group TEXT, album TEXT NOT NULL, artist TEXT NOT NULL, year INTEGER, cover TEXT,
  at INTEGER NOT NULL, status TEXT NOT NULL, reason TEXT, decided_by TEXT, decided_at INTEGER, fetched TEXT);
CREATE INDEX IF NOT EXISTS request_status ON request (status, at);
-- Discover's discographies (decision 0069 amended, 0.9): an artist's albums and EPs as MusicBrainz lists them (JSON),
-- by the artist's MusicBrainz id, and when they were read; the same for every account.
CREATE TABLE IF NOT EXISTS discography (artist TEXT PRIMARY KEY, at INTEGER NOT NULL, groups TEXT NOT NULL) WITHOUT ROWID;
-- Related artists to explore (0.9.2): an artist's (by MusicBrainz id) as last looked up (JSON), the same for every account.
CREATE TABLE IF NOT EXISTS related_artists (artist TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT NOT NULL) WITHOUT ROWID;
-- An album's page on Bandcamp as found when the album was opened (0.9.3; none: not found), by the album's key (its
-- release group, else its artist's and its name's), and when; the same for every account.
CREATE TABLE IF NOT EXISTS bandcamp_found (album TEXT PRIMARY KEY, at INTEGER NOT NULL, page TEXT) WITHOUT ROWID;
-- An album's original year as MusicBrainz gives it (0.9.5): its release's release group's first release year (none:
-- MusicBrainz knew none), by the MusicBrainz release, and when it was asked.
CREATE TABLE IF NOT EXISTS original_years (release TEXT PRIMARY KEY, year INTEGER, at INTEGER NOT NULL) WITHOUT ROWID;
-- An album's release type (0.9.6): its primary type (album, ep, single, other; none: not known there) and its
-- secondary types (joined by U+001F), by `mb:<MusicBrainz release>` (its release group's) or `dc:<album id>` (Discogs'
-- format descriptions), and when it was asked.
CREATE TABLE IF NOT EXISTS release_types (key TEXT PRIMARY KEY, primary_type TEXT, secondary TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- The albums a renumber look matched to a release (0.9.6): their gaps are tracks the library does not hold, not wrong
-- numbers (Health leaves them out), and when.
CREATE TABLE IF NOT EXISTS renumber_matched (album TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;
-- A release group's cover for albums not in the library (0.9.5): the Cover Art Archive's, else Discogs' (none: neither
-- has one), and when it was looked for.
CREATE TABLE IF NOT EXISTS rg_covers (rg TEXT PRIMARY KEY, url TEXT, at INTEGER NOT NULL) WITHOUT ROWID;
-- Discogs' genres and styles for a library album whose files carry none (0.9.5; joined by U+001F, empty: none found),
-- by the album's id, and when they were asked.
CREATE TABLE IF NOT EXISTS discogs_genres (album TEXT PRIMARY KEY, genres TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
-- Sleep is no longer kept (the owner, 2026-10-24: not something that should be in Cratarr at all).
DROP TABLE IF EXISTS asleep;
";

/// Until 2026-10-16 the album panel's heart starred the album's id as a song's, where no Subsonic app looked; it is the
/// album's star now. Moves those (the album ids, `al-`) over, keeping the first star's time. Safe to repeat.
const ALBUM_HEARTS: &str = "
INSERT INTO annotation (user, item, kind, starred, starred_at)
  SELECT user, item, 'album', 1, starred_at FROM annotation WHERE kind = 'song' AND item LIKE 'al-%' AND starred = 1
  ON CONFLICT (user, item, kind) DO UPDATE SET starred = 1, starred_at = coalesce(annotation.starred_at, excluded.starred_at);
DELETE FROM annotation WHERE kind = 'song' AND item LIKE 'al-%';
";

/// How a heard track ended (decision 0064).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ended {
    /// Played to its end (or as a Subsonic app counts it: all it tells).
    Finished,
    /// Left for another track.
    Skipped,
    /// Playing stopped on it.
    Stopped,
}

impl Ended {
    fn as_str(self) -> &'static str {
        match self {
            Ended::Finished => "finished",
            Ended::Skipped => "skipped",
            Ended::Stopped => "stopped",
        }
    }

    fn of(s: &str) -> Self {
        match s {
            "skipped" => Ended::Skipped,
            "stopped" => Ended::Stopped,
            _ => Ended::Finished,
        }
    }
}

/// A track heard, as a player tells it (decision 0064): when it started (epoch seconds), how much of it was heard and
/// how long it is, how it ended, whether it counts as a play (0022: the player says), where it sounded and what
/// started it, and the player's own id for the report.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub at: i64,
    pub heard_secs: Option<f64>,
    pub length_secs: Option<f64>,
    pub ended: Ended,
    pub counts: bool,
    pub player: Option<String>,
    pub output: Option<String>,
    pub context: Option<String>,
    pub event: Option<String>,
    /// Heard while following another listener (the owner, 2026-10-30): in the history, scrobbled and counted in the
    /// library's play counts, but not in one's own plays (`Annotation::own_count`), which the taste, Discover, Home's
    /// picks and the radio read.
    pub followed: bool,
}

impl Heard {
    /// A counted play and nothing more known (a Subsonic app's scrobble, a player from before 0064).
    pub fn counted(at: i64) -> Self {
        Heard { at, heard_secs: None, length_secs: None, ended: Ended::Finished, counts: true, player: None, output: None, context: None, event: None, followed: false }
    }
}

/// A row of the log, as the history reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Logged {
    pub song: String,
    pub at: i64,
    pub heard_secs: Option<f64>,
    pub length_secs: Option<f64>,
    pub ended: Ended,
    pub counted: bool,
    pub context: Option<String>,
    /// Heard while following another listener: the history shows it, the taste leaves it out (the owner, 2026-10-30).
    pub followed: bool,
}

/// The week before a user's log began, from the songs' last plays (the owner, 2026-10-23, for 74.3.1: the log began
/// with 74.3, the week before it was only in `annotation`). One row per song last played in the seven days before the
/// user's first row a player told: finished, counted (it was, in `annotation` already: nothing is counted again),
/// context `backfill:annotation`, its own event so running it again adds nothing. Only the last play of each song is
/// known. Run as the file opens.
const WEEK_BEFORE_THE_LOG: &str = "
INSERT INTO plays (user, song, pid, at, heard_secs, length_secs, ended, counted, player, output, context, event)
  SELECT a.user, a.item, a.pid, a.play_date, NULL, NULL, 'finished', 1, NULL, NULL, 'backfill:annotation',
         'backfill:annotation:' || a.user || ':' || a.item || ':' || a.play_date
  FROM annotation a
  JOIN (SELECT user, min(at) AS first FROM plays
        WHERE coalesce(context, '') NOT IN ('listenbrainz', 'lastfm') AND coalesce(context, '') NOT LIKE 'backfill:%' AND coalesce(context, '') NOT LIKE 'file:%' GROUP BY user) f
    ON f.user = a.user
  WHERE a.kind = 'song' AND a.play_date >= f.first - 604800 AND a.play_date < f.first
  ON CONFLICT DO NOTHING;
";

/// A row of the log as read.
fn logged(r: &rusqlite::Row) -> rusqlite::Result<Logged> {
    Ok(Logged {
        song: r.get(0)?,
        at: r.get(1)?,
        heard_secs: r.get(2)?,
        length_secs: r.get(3)?,
        ended: Ended::of(&r.get::<_, String>(4)?),
        counted: r.get(5)?,
        context: r.get(6)?,
        followed: r.get::<_, Option<bool>>(7)?.unwrap_or(false),
    })
}

/// What an annotation is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Song,
    Album,
    Artist,
    /// A playlist by its name (a favourite playlist, as Finamp's hearts; decision 0023).
    Playlist,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Song => "song",
            Kind::Album => "album",
            Kind::Artist => "artist",
            Kind::Playlist => "playlist",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotation {
    /// Every counted play, followed ones too (the owner, 2026-10-30): what the library shows, sorts by and syncs.
    pub play_count: u32,
    /// Epoch seconds.
    pub play_date: Option<i64>,
    /// One's own plays only, none heard while following another listener: what the picks read (Discover, Home's
    /// looks, the radio). Never more than `play_count`.
    pub own_count: u32,
    /// The last of them, epoch seconds.
    pub own_date: Option<i64>,
    /// 0 (none) to 5.
    pub rating: u8,
    /// Epoch seconds, when starred.
    pub starred_at: Option<i64>,
}

/// What an import last brought in for one item: the server's values then, so the next import adds only what changed
/// since (decision 0030, amended 2026-10-15).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    /// The server's id of the item it came from.
    pub server_item: Option<String>,
    pub plays: u32,
    /// Epoch seconds.
    pub played: Option<i64>,
    pub starred: bool,
    pub rating: u8,
}

/// What an import's undo took back (decision 0067): the plays, the stars and ratings, of how many items it had brought.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
pub struct Undone {
    pub items: usize,
    pub plays: u64,
    pub unstarred: usize,
    pub unrated: usize,
}

/// Whose import: the Cratarr account it goes into, the source (its id in the settings) and the account on the server.
#[derive(Debug, Clone, Copy)]
pub struct ImportKey<'a> {
    pub account: &'a str,
    pub source: &'a str,
    pub server_user: &'a str,
}

/// One item's change from an import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportChange {
    pub kind: Kind,
    pub item: String,
    /// Songs: the persistent id, so the record follows the song when the catalogue renumbers its tracks.
    pub pid: Option<String>,
    /// Plays to add; for a song on its album and artists too, as a play is counted (`count_play`).
    pub add_plays: u32,
    /// Last played, kept when later than the one there.
    pub played: Option<i64>,
    pub album: Option<String>,
    pub artists: Vec<String>,
    /// Star (with when, else now) or unstar.
    pub star: Option<(bool, Option<i64>)>,
    pub rating: Option<u8>,
    /// What the server said, kept for the next import.
    pub now: Imported,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppQueue {
    /// The app that saved it (Subsonic's `c`); every install of an app shares its name.
    pub client: String,
    pub songs: Vec<String>,
    /// The place of the song that plays (0-based): a song can be in the queue twice.
    pub current: Option<usize>,
    pub position_ms: u64,
    /// Epoch seconds.
    pub changed: i64,
}

/// A position kept in a song (OpenSubsonic `createBookmark`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub item: String,
    pub position_ms: u64,
    pub comment: String,
    /// Epoch seconds.
    pub created: i64,
    pub changed: i64,
    pub changed_by: Option<String>,
}

/// A look of the visuals kept to come back to (decision 0046, amended): whatever shaped the picture when it was saved,
/// as the page sent it (its JSON, opaque here), and a small picture of it (a data URL), where the page took one.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub settings: serde_json::Value,
    /// Epoch seconds.
    pub created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// Another account's (the owner's library, the owner: "available to all users"): recalled, never changed here.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shared: bool,
}

/// One of the owner's thumbs on the visuals as they were (decision 0048): up or down, and what the page sent with it
/// (what the visuals did over the 20 seconds before it, the settings, the track, what it changed: opaque here).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Thumb {
    /// Epoch milliseconds, the server's.
    pub at: i64,
    pub thumb: String,
    #[serde(flatten)]
    pub body: serde_json::Map<String, serde_json::Value>,
}

/// What the visuals' governor learned from the thumbs (decision 0048): its limits as the page keeps them (opaque here),
/// as of `at` (the page's epoch milliseconds), and the version they were kept at (the clock's milliseconds or after, so
/// it never goes back across a restart).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Learned {
    pub version: i64,
    pub limits: serde_json::Value,
    pub at: i64,
}

/// Learned limits not kept: the page's version was not the one kept (another device learned meanwhile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stale;

/// One of the owner's pairwise ratings of two behaviours of the visuals (the governor's calibration, decision 0046,
/// amended): what was shown as A and as B (their recipes, opaque here), which was preferred ("a", "b", "same" or
/// "neither"), the track it played on, and anything else the rating page noted.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Rating {
    /// Epoch milliseconds.
    pub at: i64,
    pub a: serde_json::Value,
    pub b: serde_json::Value,
    pub choice: String,
    pub track: Option<String>,
    pub context: serde_json::Value,
}

/// An Incoming album promoted (kept into Music) or rejected (deleted) by its id: what was done, and the key that
/// undoes it (Incoming's own undo, decision 0026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The album id it was asked by (its Incoming id).
    pub album: String,
    /// Its Music-form id (see `serve::ids::album`), the same before and after a Keep.
    pub base: String,
    /// "promoted" or "rejected".
    pub state: String,
    /// Epoch seconds.
    pub at: i64,
    /// `None` while a Keep waits for its conversion.
    pub undo: Option<String>,
    pub name: String,
    pub album_artist: String,
}

pub struct UserData {
    db: Mutex<Connection>,
    /// Every change of favourites, as `set_starred` makes it (`subscribe_starred`).
    starred: tokio::sync::broadcast::Sender<Starred>,
}

/// A change of favourites (stars) as `set_starred` made it, whichever way it came: a page, a Subsonic app, Home
/// Assistant's API (the owner, 2026-10-25: a favourite said to Assist showed in the app only later). The live channel
/// tells every open page of `user` (app/events.rs `favourite`), so each heart shows it at once.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Starred {
    #[serde(skip)]
    pub user: String,
    /// `song`, `album`, `artist` or `playlist`.
    pub kind: &'static str,
    /// Their ids (a playlist's: its name).
    pub items: Vec<String>,
    pub on: bool,
}

/// An album answered in Discover, as kept (`discover_exclusions`).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub key: String,
    pub artist_key: String,
    /// `no` (Not interested) or `hidden`.
    pub answer: String,
    pub at: i64,
    /// Its name and its artist's, its year and cover, where they were kept with the answer (from 0.9.3).
    pub name: Option<String>,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub cover: Option<String>,
}

/// An artist excluded everywhere (decision 0083): their key, their name as it was said, and since when (epoch s).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExcludedArtist {
    pub key: String,
    pub name: String,
    pub at: i64,
}

/// A play waiting to be sent to a listening service (app::scrobble).
#[derive(Debug, Clone, PartialEq)]
pub struct Queued {
    pub id: i64,
    pub user: String,
    pub target: String,
    pub song: String,
    pub at: i64,
    pub tries: i64,
}

/// A listening service an account sends its plays to, as Connections shows it (never its secret).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ScrobbleTarget {
    pub target: String,
    /// The account on the service.
    pub name: String,
    /// Why it stopped sending, until connected again.
    pub broken: Option<String>,
    /// Switched on (sending), or off with its sign-in kept.
    pub on: bool,
    /// Plays waiting to be sent, and why the last try failed.
    pub queued: u64,
    pub waiting: Option<String>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// One queue per account was kept before (`play_queue`); each moves to the app that saved it, the song that played
/// found by its id (the first copy: all that was known).
fn move_old_play_queues(conn: &Connection) -> Result<()> {
    let old: Vec<(String, String, Option<String>, i64, i64, Option<String>)> = conn
        .prepare("SELECT user, songs, current, position_ms, changed, changed_by FROM play_queue")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (user, songs, current, position_ms, changed, by) in old {
        let current = current.and_then(|c| songs.split(',').position(|s| s == c)).map(|i| i as i64);
        conn.execute(
            "INSERT OR IGNORE INTO app_queue (user, client, songs, current, position_ms, changed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![user, by.unwrap_or_default(), songs, current, position_ms, changed],
        )?;
        conn.execute("DELETE FROM play_queue WHERE user = ?1", [&user])?;
    }
    Ok(())
}

/// A `visual_snapshot` row's (id, name, settings, created, picture); settings that no longer read as JSON read as null.
fn snapshot_of(r: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
    let settings: String = r.get(2)?;
    Ok(Snapshot {
        id: r.get(0)?,
        name: r.get(1)?,
        settings: serde_json::from_str(&settings).unwrap_or(serde_json::Value::Null),
        created: r.get(3)?,
        picture: r.get(4)?,
        // Only where the query asks for it (`snapshots`): whether the row is another account's.
        shared: r.get::<_, bool>(5).unwrap_or(false),
    })
}

const SNAPSHOT_COLUMNS: &str = "id, name, settings, created, picture";

/// Listening services from before they could be switched off: on. Safe to repeat.
fn add_scrobble_off(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('scrobble_target') WHERE name = 'off'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE scrobble_target ADD COLUMN off INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

/// Discover's answers from before their names were kept (0.9.3) get the columns, empty. Safe to repeat.
fn add_answer_names(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('discover_answer') WHERE name = 'name'")?.exists([])?;
    if !has {
        conn.execute_batch(
            "ALTER TABLE discover_answer ADD COLUMN name TEXT; ALTER TABLE discover_answer ADD COLUMN artist_name TEXT;
             ALTER TABLE discover_answer ADD COLUMN year INTEGER; ALTER TABLE discover_answer ADD COLUMN cover TEXT;",
        )?;
    }
    Ok(())
}

/// Plays from before following (the owner, 2026-10-30) get the column: none was heard while following. Safe to repeat.
fn add_plays_followed(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('plays') WHERE name = 'followed'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE plays ADD COLUMN followed INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

/// Counts from before one's own plays were kept apart from followed ones (the owner, 2026-10-30) get the columns,
/// filled from the counts: until then every counted play was one's own (a followed play was counted nowhere). Safe to
/// repeat.
fn add_annotation_own(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('annotation') WHERE name = 'own_count'")?.exists([])?;
    if !has {
        conn.execute_batch(
            "ALTER TABLE annotation ADD COLUMN own_count INTEGER NOT NULL DEFAULT 0; ALTER TABLE annotation ADD COLUMN own_date INTEGER;
             UPDATE annotation SET own_count = play_count, own_date = play_date;",
        )?;
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS annotation_own_played ON annotation (user, kind, own_date);")?;
    Ok(())
}

/// The rule release types are found by (0.10: a Discogs release must fit the album's tracks or length; an EP or single
/// far longer than its release group's first release is an album). Rows an older rule wrote that this one could find
/// otherwise are asked again (`release_types_asked`).
pub const RELEASE_TYPES_RULE: i64 = 1;

/// Release types from before their rule was kept get the column: rule 0. Safe to repeat.
fn add_release_types_rule(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('release_types') WHERE name = 'rule'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE release_types ADD COLUMN rule INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

/// The artists an account excluded everywhere (decision 0083, the owner 2026-10-30): by the artist's loose key
/// (`app::excluded_artists::key`), with the name as it was said and when. A user data file from before it gets the
/// table, empty. Safe to repeat.
fn add_excluded_artists(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS excluded_artist (
           user TEXT NOT NULL, key TEXT NOT NULL, name TEXT NOT NULL, at INTEGER NOT NULL,
           PRIMARY KEY (user, key)) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Snapshots made before they had pictures (preview 63) get the column, empty. Safe to repeat.
fn add_snapshot_pictures(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('visual_snapshot') WHERE name = 'picture'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE visual_snapshot ADD COLUMN picture TEXT")?;
    }
    Ok(())
}

/// An `imported` row's (server_item, plays, played, starred, rating).
fn imported_of(r: &rusqlite::Row) -> rusqlite::Result<Imported> {
    Ok(Imported {
        server_item: r.get(0)?,
        plays: r.get::<_, i64>(1)?.max(0) as u32,
        played: r.get(2)?,
        starred: r.get::<_, i64>(3)? != 0,
        rating: r.get::<_, i64>(4)?.clamp(0, 5) as u8,
    })
}

impl UserData {
    /// `file`: the database; `None` keeps it in memory.
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = crate::db::open(file)?;
        conn.busy_timeout(Duration::from_secs(15))?;
        conn.execute_batch(SCHEMA).context("creating the user data tables")?;
        conn.execute_batch(tags::SCHEMA).context("creating the album tag table")?;
        conn.execute_batch(words::SCHEMA).context("creating the words' tables")?;
        conn.execute_batch(works::SCHEMA).context("creating the works' tables")?;
        conn.execute_batch(ALBUM_HEARTS).context("moving the panel's album hearts to the albums' stars")?;
        move_old_play_queues(&conn).context("moving the saved play queues to their apps")?;
        add_snapshot_pictures(&conn).context("giving the visuals' snapshots their pictures")?;
        add_scrobble_off(&conn)?;
        add_answer_names(&conn)?;
        add_plays_followed(&conn)?;
        add_annotation_own(&conn).context("keeping one's own plays apart from followed ones")?;
        add_release_types_rule(&conn)?;
        add_excluded_artists(&conn).context("keeping the artists excluded everywhere")?;
        conn.execute_batch(WEEK_BEFORE_THE_LOG).context("writing the week before the log from the last plays")?;
        Ok(Self { db: Mutex::new(conn), starred: tokio::sync::broadcast::channel(64).0 })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The annotations of several items of one kind, by item; items without any are left out.
    pub fn get(&self, user: &str, kind: Kind, items: &[String]) -> Result<std::collections::HashMap<String, Annotation>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT play_count, play_date, rating, starred, starred_at, own_count, own_date FROM annotation WHERE user = ?1 AND kind = ?2 AND item = ?3",
        )?;
        let mut out = std::collections::HashMap::new();
        for item in items {
            let a = st
                .query_row(params![user, kind.as_str(), item], |r| {
                    Ok(Annotation {
                        play_count: r.get::<_, i64>(0)?.max(0) as u32,
                        play_date: r.get(1)?,
                        rating: r.get::<_, i64>(2)?.clamp(0, 5) as u8,
                        starred_at: if r.get::<_, i64>(3)? != 0 { Some(r.get::<_, Option<i64>>(4)?.unwrap_or(0)) } else { None },
                        own_count: r.get::<_, i64>(5)?.max(0) as u32,
                        own_date: r.get(6)?,
                    })
                })
                .optional()?;
            if let Some(a) = a {
                out.insert(item.clone(), a);
            }
        }
        Ok(out)
    }

    /// Stars or unstars items. Safe to repeat: a star keeps its first time.
    pub fn set_starred(&self, user: &str, kind: Kind, items: &[String], on: bool) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, starred, starred_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (user, item, kind) DO UPDATE SET
                   starred = excluded.starred,
                   starred_at = CASE WHEN excluded.starred = 0 THEN NULL WHEN annotation.starred = 1 THEN annotation.starred_at ELSE excluded.starred_at END",
            )?;
            for item in items {
                st.execute(params![user, item, kind.as_str(), on, on.then(now)])?;
            }
        }
        tx.commit()?;
        drop(db);
        // Told once written; nobody listening is no error.
        let _ = self.starred.send(Starred { user: user.to_string(), kind: kind.as_str(), items: items.to_vec(), on });
        Ok(())
    }

    /// Every change of favourites from now on (`Starred`), whoever made it.
    pub fn subscribe_starred(&self) -> tokio::sync::broadcast::Receiver<Starred> {
        self.starred.subscribe()
    }

    /// Every item of one kind a user played or rated, with its annotation (the Library's orders and filters by plays
    /// and rating; the picks read its own plays): one read for all of them.
    pub fn used(&self, user: &str, kind: Kind) -> Result<std::collections::HashMap<String, Annotation>> {
        // Its play history off (0.9.5): only what it rated, nothing of what it played.
        let keeps = self.tracking(user);
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT item, play_count, play_date, rating, starred, starred_at, own_count, own_date FROM annotation
             WHERE user = ?1 AND kind = ?2 AND (play_count > 0 OR play_date IS NOT NULL OR rating > 0)",
        )?;
        let rows = st.query_map(params![user, kind.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Annotation {
                    play_count: r.get::<_, i64>(1)?.max(0) as u32,
                    play_date: r.get(2)?,
                    rating: r.get::<_, i64>(3)?.clamp(0, 5) as u8,
                    starred_at: if r.get::<_, i64>(4)? != 0 { Some(r.get::<_, Option<i64>>(5)?.unwrap_or(0)) } else { None },
                    own_count: r.get::<_, i64>(6)?.max(0) as u32,
                    own_date: r.get(7)?,
                },
            ))
        })?;
        let all: std::collections::HashMap<String, Annotation> = rows.collect::<rusqlite::Result<_>>()?;
        Ok(match keeps {
            true => all,
            false => all
                .into_iter()
                .filter(|(_, a)| a.rating > 0)
                .map(|(k, a)| (k, Annotation { play_count: 0, play_date: None, own_count: 0, own_date: None, ..a }))
                .collect(),
        })
    }

    /// The items of one kind a user starred, newest star first.
    pub fn starred(&self, user: &str, kind: Kind) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT item FROM annotation WHERE user = ?1 AND kind = ?2 AND starred = 1 ORDER BY starred_at DESC, item",
        )?;
        let rows = st.query_map(params![user, kind.as_str()], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Albums in one of the orders only user data decides (`getAlbumList2`): `starred` (newest star first),
    /// `frequent` (most played), `recent` (last played), `highest` (best rated).
    pub fn album_list(&self, user: &str, order: &str, size: usize, offset: usize) -> Result<Vec<String>> {
        let (filter, by) = match order {
            "starred" => ("starred = 1", "starred_at DESC"),
            "frequent" => ("play_count > 0", "play_count DESC, play_date DESC"),
            "recent" => ("play_date IS NOT NULL", "play_date DESC"),
            _ => ("rating > 0", "rating DESC, starred_at DESC"),
        };
        let db = self.db();
        let mut st = db.prepare(&format!(
            "SELECT item FROM annotation WHERE user = ?1 AND kind = 'album' AND {filter} ORDER BY {by}, item LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = st.query_map(params![user, size as i64, offset as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_rating(&self, user: &str, kind: Kind, item: &str, rating: u8) -> Result<()> {
        self.db().execute(
            "INSERT INTO annotation (user, item, kind, rating) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (user, item, kind) DO UPDATE SET rating = excluded.rating",
            params![user, item, kind.as_str(), rating.min(5)],
        )?;
        Ok(())
    }

    /// One play of a song, counted on the song, its album and its artists in one go, as Navidrome's play tracker does.
    /// `at`: when it was played, epoch seconds.
    pub fn count_play(&self, user: &str, song: (&str, &str), album: &str, artists: &[String], at: i64) -> Result<()> {
        self.record(user, song, album, artists, &Heard::counted(at)).map(|_| ())
    }

    /// A track heard (decision 0064): a row in the log and, when it counts as a play (0022), the play counted on the
    /// song, its album and its artists, together; as one's own too unless heard while following. A report with an
    /// `event` already in the log changes nothing; false then.
    pub fn record(&self, user: &str, song: (&str, &str), album: &str, artists: &[String], h: &Heard) -> Result<bool> {
        // Its play history off (0.9.5): nothing written, whoever brings it (a player, an import).
        if !self.tracking(user) {
            return Ok(false);
        }
        let mut db = self.db();
        let tx = db.transaction()?;
        let added = tx.execute(
            "INSERT INTO plays (user, song, pid, at, heard_secs, length_secs, ended, counted, player, output, context, event, followed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT DO NOTHING",
            params![user, song.0, song.1, h.at, h.heard_secs, h.length_secs, h.ended.as_str(), h.counts, h.player, h.output, h.context, h.event, h.followed],
        )?;
        if added == 0 {
            return Ok(false);
        }
        // A counted play goes to the listening services the user connected (app::scrobble), queued in the same
        // transaction (decision 0068). Imported listens never count, so none of them goes back.
        if h.counts {
            tx.execute(
                "INSERT INTO scrobble_queue (user, target, song, at) SELECT ?1, target, ?2, ?3 FROM scrobble_target WHERE user = ?1 AND broken IS NULL AND off = 0",
                params![user, song.0, h.at],
            )?;
        }
        // Heard while following (the owner, 2026-10-30): counted, but not as one's own, which the picks read.
        if h.counts {
            let (at, own) = (h.at, i64::from(!h.followed));
            let mut st = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, play_count, play_date, own_count, own_date)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, CASE WHEN ?6 > 0 THEN ?5 END)
                 ON CONFLICT (user, item, kind) DO UPDATE SET play_count = annotation.play_count + 1,
                   play_date = max(coalesce(annotation.play_date, 0), excluded.play_date),
                   own_count = annotation.own_count + excluded.own_count,
                   own_date = CASE WHEN excluded.own_date IS NULL THEN annotation.own_date
                     ELSE max(coalesce(annotation.own_date, 0), excluded.own_date) END,
                   pid = coalesce(excluded.pid, annotation.pid)",
            )?;
            st.execute(params![user, song.0, Kind::Song.as_str(), song.1, at, own])?;
            st.execute(params![user, album, Kind::Album.as_str(), Option::<String>::None, at, own])?;
            for a in artists {
                st.execute(params![user, a, Kind::Artist.as_str(), Option::<String>::None, at, own])?;
            }
        }
        tx.commit()?;
        Ok(true)
    }

    /// A listening service connected (or connected again) for `user`: its account there and what it signs in with.
    pub fn set_scrobble_target(&self, user: &str, target: &str, name: &str, secret: &str) -> Result<()> {
        self.db().execute(
            "INSERT INTO scrobble_target (user, target, name, secret, broken, at) VALUES (?1, ?2, ?3, ?4, NULL, ?5)
             ON CONFLICT (user, target) DO UPDATE SET name = excluded.name, secret = excluded.secret, broken = NULL, at = excluded.at",
            params![user, target, name, secret, now()],
        )?;
        // What waited while it was stopped goes now.
        self.db().execute("UPDATE scrobble_queue SET next = 0, tries = 0 WHERE user = ?1 AND target = ?2", params![user, target])?;
        Ok(())
    }

    /// A listening service switched on or off (its sign-in kept): off, nothing more is queued for it, and what waits
    /// stays until it is on again.
    pub fn set_scrobble_on(&self, user: &str, target: &str, on: bool) -> Result<bool> {
        let n = self.db().execute("UPDATE scrobble_target SET off = ?3 WHERE user = ?1 AND target = ?2", params![user, target, i64::from(!on)])?;
        if on {
            self.db().execute("UPDATE scrobble_queue SET next = 0, tries = 0 WHERE user = ?1 AND target = ?2", params![user, target])?;
        }
        Ok(n > 0)
    }

    /// A listening service disconnected: it and its waiting plays go.
    pub fn forget_scrobble_target(&self, user: &str, target: &str) -> Result<()> {
        let db = self.db();
        db.execute("DELETE FROM scrobble_target WHERE user = ?1 AND target = ?2", params![user, target])?;
        db.execute("DELETE FROM scrobble_queue WHERE user = ?1 AND target = ?2", params![user, target])?;
        Ok(())
    }

    /// A user's listening services, each with how many plays wait.
    pub fn scrobble_targets(&self, user: &str) -> Result<Vec<ScrobbleTarget>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT t.target, t.name, t.broken, t.off,
                    (SELECT count(*) FROM scrobble_queue q WHERE q.user = t.user AND q.target = t.target),
                    (SELECT why FROM scrobble_queue q WHERE q.user = t.user AND q.target = t.target AND q.why IS NOT NULL ORDER BY q.at LIMIT 1)
             FROM scrobble_target t WHERE t.user = ?1 ORDER BY t.target",
        )?;
        let rows = st.query_map([user], |r| {
            Ok(ScrobbleTarget {
                target: r.get(0)?,
                name: r.get(1)?,
                broken: r.get(2)?,
                on: r.get::<_, i64>(3)? == 0,
                queued: r.get::<_, i64>(4)?.max(0) as u64,
                waiting: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What a user's service signs in with (JSON), while it is on and not stopped.
    pub fn scrobble_secret(&self, user: &str, target: &str) -> Result<Option<String>> {
        Ok(self
            .db()
            .query_row("SELECT secret FROM scrobble_target WHERE user = ?1 AND target = ?2 AND broken IS NULL AND off = 0", params![user, target], |r| r.get(0))
            .optional()?)
    }

    /// What a user's service signs in with (JSON), stopped or not: to sign in again with what was given before.
    pub fn scrobble_secret_any(&self, user: &str, target: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT secret FROM scrobble_target WHERE user = ?1 AND target = ?2", params![user, target], |r| r.get(0)).optional()?)
    }

    /// Plays due to be sent now, oldest first, of services not stopped.
    pub fn due_scrobbles(&self, now: i64, limit: usize) -> Result<Vec<Queued>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT q.id, q.user, q.target, q.song, q.at, q.tries FROM scrobble_queue q
             JOIN scrobble_target t ON t.user = q.user AND t.target = q.target AND t.broken IS NULL AND t.off = 0
             WHERE q.next <= ?1 ORDER BY q.at LIMIT ?2",
        )?;
        let rows = st.query_map(params![now, limit as i64], |r| {
            Ok(Queued { id: r.get(0)?, user: r.get(1)?, target: r.get(2)?, song: r.get(3)?, at: r.get(4)?, tries: r.get(5)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Plays sent (or gone): out of the queue.
    pub fn scrobbled(&self, ids: &[i64]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached("DELETE FROM scrobble_queue WHERE id = ?1")?;
            for id in ids {
                st.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Plays not sent this time: tried again at `next`, `why` kept.
    pub fn scrobbles_wait(&self, ids: &[i64], next: i64, why: &str) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached("UPDATE scrobble_queue SET tries = tries + 1, next = ?2, why = ?3 WHERE id = ?1")?;
            for id in ids {
                st.execute(params![id, next, why])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A service's login refused: it stops (its plays wait) until connected again.
    pub fn scrobble_broken(&self, user: &str, target: &str, why: &str) -> Result<()> {
        self.db().execute("UPDATE scrobble_target SET broken = ?3 WHERE user = ?1 AND target = ?2", params![user, target, why])?;
        Ok(())
    }

    /// The ids of the listens imported into a user's history from `context` (an import's, decision 0067), to tell
    /// what a preview would add.
    pub fn heard_events(&self, user: &str, context: &str) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT event FROM plays WHERE user = ?1 AND context = ?2 AND event IS NOT NULL")?;
        let rows = st.query_map(params![user, context], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An import from `context` undone (decision 0067): its listens out of the user's history. They never counted, so
    /// the counts stay. How many.
    pub fn forget_heard(&self, user: &str, context: &str) -> Result<usize> {
        Ok(self.db().execute("DELETE FROM plays WHERE user = ?1 AND context = ?2 AND counted = 0", params![user, context])?)
    }

    /// What a user heard from `since` on (epoch seconds), oldest first (decision 0064).
    pub fn heard_since(&self, user: &str, since: i64) -> Result<Vec<Logged>> {
        // Its play history off (0.9.5): Insights' listening, the taste and the rest read nothing.
        if !self.tracking(user) {
            return Ok(Vec::new());
        }
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT song, at, heard_secs, length_secs, ended, counted, context, followed FROM plays WHERE user = ?1 AND at >= ?2 ORDER BY at",
        )?;
        let rows = st.query_map(params![user, since], logged)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// When a user's log starts being told as it happens: their first row's time that a player told (an import from
    /// elsewhere, `listenbrainz` and `lastfm`, and the week written from the last plays, `backfill:…`, leave gaps); none before they
    /// heard anything since it exists.
    pub fn heard_from(&self, user: &str) -> Result<Option<i64>> {
        Ok(self.db().query_row(
            "SELECT min(at) FROM plays WHERE user = ?1 AND coalesce(context, '') NOT IN ('listenbrainz', 'lastfm') AND coalesce(context, '') NOT LIKE 'backfill:%' AND coalesce(context, '') NOT LIKE 'file:%'",
            [user],
            |r| r.get(0),
        )?)
    }

    /// Writes the week before the log from the last plays (`WEEK_BEFORE_THE_LOG`); as the file opens. Safe to repeat.
    pub fn backfill_week(&self) -> Result<()> {
        Ok(self.db().execute_batch(WEEK_BEFORE_THE_LOG)?)
    }

    /// Sets an item's whole annotation for a user (a song taking over another's data, decision 0013), its own plays
    /// never more than all of them. Safe to repeat.
    pub fn set_annotation(&self, user: &str, kind: Kind, item: &str, pid: Option<&str>, a: &Annotation) -> Result<()> {
        self.db().execute(
            "INSERT INTO annotation (user, item, kind, pid, play_count, play_date, rating, starred, starred_at, own_count, own_date)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (user, item, kind) DO UPDATE SET pid = coalesce(excluded.pid, annotation.pid),
               play_count = excluded.play_count, play_date = excluded.play_date, rating = excluded.rating,
               starred = excluded.starred, starred_at = excluded.starred_at,
               own_count = excluded.own_count, own_date = excluded.own_date",
            params![
                user,
                item,
                kind.as_str(),
                pid,
                a.play_count,
                a.play_date,
                a.rating,
                a.starred_at.is_some(),
                a.starred_at,
                a.own_count.min(a.play_count),
                a.own_date
            ],
        )?;
        Ok(())
    }

    /// What earlier imports brought in for these items, by item; items never imported are left out.
    pub fn imported(&self, key: ImportKey, kind: Kind, items: &[String]) -> Result<std::collections::HashMap<String, Imported>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT server_item, plays, played, starred, rating FROM imported
             WHERE account = ?1 AND source = ?2 AND server_user = ?3 AND kind = ?4 AND item = ?5",
        )?;
        let mut out = std::collections::HashMap::new();
        for item in items {
            if let Some(i) = st.query_row(params![key.account, key.source, key.server_user, kind.as_str(), item], imported_of).optional()? {
                out.insert(item.clone(), i);
            }
        }
        Ok(out)
    }

    /// Every item of one kind an earlier import left starred, with what it brought in.
    pub fn imported_starred(&self, key: ImportKey, kind: Kind) -> Result<Vec<(String, Imported)>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT item, server_item, plays, played, starred, rating FROM imported
             WHERE account = ?1 AND source = ?2 AND server_user = ?3 AND kind = ?4 AND starred = 1 ORDER BY item",
        )?;
        let rows = st.query_map(params![key.account, key.source, key.server_user, kind.as_str()], |r| {
            let mut i = Imported { server_item: r.get(1)?, ..Imported::default() };
            i.plays = r.get::<_, i64>(2)?.max(0) as u32;
            i.played = r.get(3)?;
            i.starred = r.get::<_, i64>(4)? != 0;
            i.rating = r.get::<_, i64>(5)?.clamp(0, 5) as u8;
            Ok((r.get::<_, String>(0)?, i))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies an import's changes to `key.account`'s annotations and keeps what the server said, all or nothing.
    /// Annotations keep Navidrome's shape: plays add up, last played is the later one, a star keeps its first time.
    /// Imported plays are one's own.
    pub fn apply_import(&self, key: ImportKey, changes: &[ImportChange]) -> Result<()> {
        let user = key.account;
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut plays = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, play_count, play_date, own_count, own_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5, ?6)
                 ON CONFLICT (user, item, kind) DO UPDATE SET play_count = annotation.play_count + excluded.play_count,
                   play_date = CASE WHEN excluded.play_date IS NULL THEN annotation.play_date
                     ELSE max(coalesce(annotation.play_date, 0), excluded.play_date) END,
                   own_count = annotation.own_count + excluded.own_count,
                   own_date = CASE WHEN excluded.own_date IS NULL THEN annotation.own_date
                     ELSE max(coalesce(annotation.own_date, 0), excluded.own_date) END,
                   pid = coalesce(excluded.pid, annotation.pid)",
            )?;
            let mut star = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, starred, starred_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (user, item, kind) DO UPDATE SET pid = coalesce(excluded.pid, annotation.pid),
                   starred = excluded.starred,
                   starred_at = CASE WHEN excluded.starred = 0 THEN NULL WHEN annotation.starred = 1 THEN annotation.starred_at ELSE excluded.starred_at END",
            )?;
            let mut rate = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, rating) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (user, item, kind) DO UPDATE SET rating = excluded.rating, pid = coalesce(excluded.pid, annotation.pid)",
            )?;
            let mut keep = tx.prepare_cached(
                "INSERT INTO imported (account, source, server_user, kind, item, server_item, pid, plays, played, starred, rating, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (account, source, server_user, kind, item) DO UPDATE SET server_item = excluded.server_item,
                   pid = coalesce(excluded.pid, imported.pid), plays = excluded.plays, played = excluded.played,
                   starred = excluded.starred, rating = excluded.rating, at = excluded.at",
            )?;
            let at = now();
            for c in changes {
                let kind = c.kind;
                if c.add_plays > 0 || c.played.is_some() {
                    plays.execute(params![user, c.item, kind.as_str(), c.pid, c.add_plays, c.played])?;
                    if kind == Kind::Song {
                        let groups = c.album.iter().map(|a| (a, Kind::Album)).chain(c.artists.iter().map(|a| (a, Kind::Artist)));
                        for (item, k) in groups {
                            plays.execute(params![user, item, k.as_str(), Option::<String>::None, c.add_plays, c.played])?;
                        }
                    }
                }
                if let Some((on, when)) = c.star {
                    star.execute(params![user, c.item, kind.as_str(), c.pid, on, on.then(|| when.unwrap_or(at))])?;
                }
                if let Some(r) = c.rating {
                    rate.execute(params![user, c.item, kind.as_str(), c.pid, r.min(5)])?;
                }
                let n = &c.now;
                keep.execute(params![
                    key.account,
                    key.source,
                    key.server_user,
                    kind.as_str(),
                    c.item,
                    n.server_item,
                    c.pid,
                    n.plays,
                    n.played,
                    n.starred,
                    n.rating,
                    at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The choices of the last import from a source (the page's JSON), to offer them again.
    /// An import undone as a whole (decision 0067): the plays it added taken off the song, its album and artists
    /// (`groups` names them), the stars and ratings it set taken away where they are still as it set them, and what it
    /// remembered forgotten, so a next import starts afresh. Last played and added dates stay. All or nothing.
    pub fn undo_import(&self, key: ImportKey, groups: impl Fn(&str) -> Option<(String, Vec<String>)>) -> Result<Undone> {
        let rows: Vec<(String, String, i64, bool, i64)> = {
            let db = self.db();
            let mut st = db.prepare_cached("SELECT kind, item, plays, starred, rating FROM imported WHERE account = ?1 AND source = ?2 AND server_user = ?3")?;
            st.query_map(params![key.account, key.source, key.server_user], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?.max(0), r.get::<_, i64>(3)? != 0, r.get::<_, i64>(4)?))
            })?
            .collect::<rusqlite::Result<_>>()?
        };
        // The songs' albums and artists, read before the database is written (`groups` reads the library).
        let song = Kind::Song.as_str();
        let of: std::collections::HashMap<&str, (String, Vec<String>)> =
            rows.iter().filter(|r| r.0 == song && r.2 > 0).filter_map(|r| groups(&r.1).map(|g| (r.1.as_str(), g))).collect();
        let user = key.account;
        let mut done = Undone { items: rows.len(), ..Undone::default() };
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut less = tx.prepare_cached(
                "UPDATE annotation SET play_count = max(0, play_count - ?4), own_count = max(0, own_count - ?4) WHERE user = ?1 AND item = ?2 AND kind = ?3",
            )?;
            let mut unstar = tx.prepare_cached("UPDATE annotation SET starred = 0, starred_at = NULL WHERE user = ?1 AND item = ?2 AND kind = ?3 AND starred = 1")?;
            let mut unrate = tx.prepare_cached("UPDATE annotation SET rating = 0 WHERE user = ?1 AND item = ?2 AND kind = ?3 AND rating = ?4")?;
            for (kind, item, plays, starred, rating) in &rows {
                if *plays > 0 {
                    less.execute(params![user, item, kind, plays])?;
                    done.plays += *plays as u64;
                    if let Some((album, artists)) = of.get(item.as_str()) {
                        less.execute(params![user, album, Kind::Album.as_str(), plays])?;
                        for a in artists {
                            less.execute(params![user, a, Kind::Artist.as_str(), plays])?;
                        }
                    }
                }
                if *starred {
                    done.unstarred += unstar.execute(params![user, item, kind])?;
                }
                if *rating > 0 {
                    done.unrated += unrate.execute(params![user, item, kind, rating])?;
                }
            }
            tx.execute("DELETE FROM imported WHERE account = ?1 AND source = ?2 AND server_user = ?3", params![key.account, key.source, key.server_user])?;
        }
        tx.commit()?;
        Ok(done)
    }

    /// The import choices kept for sources whose id starts with `prefix` (the Subsonic servers imported from, decision
    /// 0067): (source, choice).
    pub fn import_choices(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT source, choice FROM import_choices WHERE substr(source, 1, length(?1)) = ?1 ORDER BY source")?;
        let rows = st.query_map([prefix], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An artist's related artists as last looked up (JSON, app::related), and when.
    pub fn related_artists(&self, artist: &str) -> Result<Option<(i64, String)>> {
        Ok(self.db().query_row("SELECT at, found FROM related_artists WHERE artist = ?1", [artist], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn set_related_artists(&self, artist: &str, at: i64, found: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO related_artists (artist, at, found) VALUES (?1, ?2, ?3)", params![artist, at, found])?;
        Ok(())
    }

    /// An album's page on Bandcamp as last looked for (`Some(None)`: looked for, not found), and when.
    pub fn bandcamp_found(&self, album: &str) -> Result<Option<(i64, Option<String>)>> {
        Ok(self.db().query_row("SELECT at, page FROM bandcamp_found WHERE album = ?1", [album], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    /// Every album page found on Bandcamp since `since` (epoch seconds), by the album's key: what lists of albums not in
    /// the library show without looking again (0.9.5).
    pub fn bandcamp_pages(&self, since: i64) -> Result<std::collections::HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, page FROM bandcamp_found WHERE at >= ?1 AND page IS NOT NULL")?;
        let rows = st.query_map([since], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_bandcamp_found(&self, album: &str, at: i64, page: Option<&str>) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO bandcamp_found (album, at, page) VALUES (?1, ?2, ?3)", params![album, at, page])?;
        Ok(())
    }

    /// An artist's discography as Discover keeps it (JSON), and when it was read (decision 0069 amended).
    pub fn discography(&self, artist: &str) -> Result<Option<(i64, String)>> {
        Ok(self.db().query_row("SELECT at, groups FROM discography WHERE artist = ?1", [artist], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn set_discography(&self, artist: &str, at: i64, groups: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO discography (artist, at, groups) VALUES (?1, ?2, ?3)", params![artist, at, groups])?;
        Ok(())
    }

    /// Discover's suggestions for `user` as last looked up (JSON), and when (decision 0069).
    pub fn discover_found(&self, user: &str) -> Result<Option<(i64, String)>> {
        Ok(self.db().query_row("SELECT at, found FROM discover_found WHERE user = ?1", [user], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn set_discover_found(&self, user: &str, at: i64, found: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO discover_found (user, at, found) VALUES (?1, ?2, ?3)", params![user, at, found])?;
        Ok(())
    }

    /// Discover's Around artists for `user` (JSON, 0.9.7).
    pub fn discover_around(&self, user: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT state FROM discover_around WHERE user = ?1", [user], |r| r.get(0)).optional()?)
    }

    pub fn set_discover_around(&self, user: &str, state: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO discover_around (user, state) VALUES (?1, ?2)", params![user, state])?;
        Ok(())
    }

    /// An album of Discover answered: `no` or `wanted`, kept by its key and its artist's.
    pub fn answer_discover(&self, user: &str, album: &str, artist: &str, answer: &str) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO discover_answer (user, album, artist, answer, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user, album, artist, answer, now()],
        )?;
        Ok(())
    }

    /// What an answered album is called, kept with the answer (0.9.3: Insights' Discovery exclusions list it).
    pub fn name_discover_answer(&self, user: &str, album: &str, name: &str, artist: &str, year: Option<i32>, cover: Option<&str>) -> Result<()> {
        self.db().execute(
            "UPDATE discover_answer SET name = ?3, artist_name = ?4, year = ?5, cover = ?6 WHERE user = ?1 AND album = ?2",
            params![user, album, name, artist, year, cover],
        )?;
        Ok(())
    }

    /// The albums `user` left out of Discover (Not interested and Hide; not those wanted), the newest first, with what
    /// is kept of them.
    pub fn discover_exclusions(&self, user: &str) -> Result<Vec<Answer>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT album, artist, answer, at, name, artist_name, year, cover FROM discover_answer
             WHERE user = ?1 AND answer IN ('no', 'hidden', 'artist') ORDER BY at DESC",
        )?;
        let rows = st.query_map([user], |r| {
            Ok(Answer {
                key: r.get(0)?,
                artist_key: r.get(1)?,
                answer: r.get(2)?,
                at: r.get(3)?,
                name: r.get(4)?,
                artist: r.get(5)?,
                year: r.get(6)?,
                cover: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every Not interested for an artist taken back: they are suggested again.
    pub fn unanswer_discover_artist(&self, user: &str, artist: &str) -> Result<()> {
        self.db().execute("DELETE FROM discover_answer WHERE user = ?1 AND artist = ?2 AND answer IN ('no', 'artist')", params![user, artist])?;
        Ok(())
    }

    /// Every discography Discover keeps: (the artist's MusicBrainz id, its albums as JSON).
    pub fn discographies(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT artist, groups FROM discography")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An answer taken back (its Undo): the album is suggested again.
    pub fn unanswer_discover(&self, user: &str, album: &str) -> Result<()> {
        self.db().execute("DELETE FROM discover_answer WHERE user = ?1 AND album = ?2", params![user, album])?;
        Ok(())
    }

    /// The albums `user` answered (their keys), and the artists twice said no to, or said no to as artists (theirs).
    pub fn discover_answered(&self, user: &str) -> Result<(std::collections::HashSet<String>, std::collections::HashSet<String>)> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album FROM discover_answer WHERE user = ?1")?;
        let albums = st.query_map([user], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        let mut st = db.prepare_cached(
            "SELECT artist FROM discover_answer WHERE user = ?1 AND answer IN ('no', 'artist') GROUP BY artist HAVING count(*) >= 2 OR max(answer = 'artist') = 1",
        )?;
        let artists = st.query_map([user], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?;
        Ok((albums, artists))
    }

    /// An artist excluded everywhere by `user` (decision 0083), by their key; excluded again, the name is the latest
    /// said and the time the first. Whether it is new.
    pub fn exclude_artist(&self, user: &str, key: &str, name: &str) -> Result<bool> {
        let db = self.db();
        let had: bool = db.prepare_cached("SELECT 1 FROM excluded_artist WHERE user = ?1 AND key = ?2")?.exists(params![user, key])?;
        db.execute(
            "INSERT INTO excluded_artist (user, key, name, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (user, key) DO UPDATE SET name = excluded.name",
            params![user, key, name, now()],
        )?;
        Ok(!had)
    }

    /// An artist taken back off `user`'s list: picked again. Whether they were on it.
    pub fn include_artist(&self, user: &str, key: &str) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM excluded_artist WHERE user = ?1 AND key = ?2", params![user, key])? > 0)
    }

    /// The artists `user` excluded everywhere, the newest first.
    pub fn excluded_artist_list(&self, user: &str) -> Result<Vec<ExcludedArtist>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT key, name, at FROM excluded_artist WHERE user = ?1 ORDER BY at DESC, key")?;
        let rows = st.query_map([user], |r| Ok(ExcludedArtist { key: r.get(0)?, name: r.get(1)?, at: r.get(2)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The keys of the artists `user` excluded everywhere: what every pick reads, once per pick.
    pub fn excluded_artists(&self, user: &str) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT key FROM excluded_artist WHERE user = ?1")?;
        let rows = st.query_map([user], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A request kept (decision 0070): its id.
    pub fn add_request(&self, r: &Request) -> Result<i64> {
        let db = self.db();
        db.execute(
            "INSERT INTO request (user, album_key, artist_key, release_group, album, artist, year, cover, at, status, reason, decided_by, decided_at, fetched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![r.user, r.album_key, r.artist_key, r.release_group, r.album, r.artist, r.year, r.cover, r.at, r.status, r.reason, r.decided_by, r.decided_at, r.fetched],
        )?;
        Ok(db.last_insert_rowid())
    }

    /// Requests, newest first: `user`'s own, or everyone's (`None`); only those of `status` when given.
    pub fn requests(&self, user: Option<&str>, status: Option<&str>, limit: usize) -> Result<Vec<Request>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT {REQUEST_COLUMNS} FROM request WHERE (?1 IS NULL OR user = ?1) AND (?2 IS NULL OR status = ?2) ORDER BY at DESC, id DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(params![user, status, limit as i64], request_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn request(&self, id: i64) -> Result<Option<Request>> {
        Ok(self.db().query_row(&format!("SELECT {REQUEST_COLUMNS} FROM request WHERE id = ?1"), [id], request_row).optional()?)
    }

    /// A request answered or withdrawn: its status, why, by whom (now).
    pub fn set_request_status(&self, id: i64, status: &str, reason: Option<&str>, by: Option<&str>) -> Result<()> {
        self.db().execute(
            "UPDATE request SET status = ?2, reason = ?3, decided_by = ?4, decided_at = ?5 WHERE id = ?1",
            params![id, status, reason, by, now()],
        )?;
        Ok(())
    }

    /// What the fetcher said of a request it was handed.
    pub fn set_request_fetched(&self, id: i64, said: &str) -> Result<()> {
        self.db().execute("UPDATE request SET fetched = ?2 WHERE id = ?1", params![id, said])?;
        Ok(())
    }

    /// How many requests wait for an answer (Health).
    pub fn requests_waiting(&self) -> Result<usize> {
        Ok(self.db().query_row("SELECT count(*) FROM request WHERE status = 'pending'", [], |r| r.get::<_, i64>(0))? as usize)
    }

    pub fn import_choice(&self, source: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT choice FROM import_choices WHERE source = ?1", [source], |r| r.get(0)).optional()?)
    }

    pub fn set_import_choice(&self, source: &str, choice: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO import_choices (source, choice) VALUES (?1, ?2)", params![source, choice])?;
        Ok(())
    }

    /// Songs' annotations, and what imports brought in for them, follow their persistent ids to new song ids (the
    /// catalogue was rebuilt): `moves` is (persistent id, song id now). A persistent id given twice goes to its first
    /// song only: two would move its data there and back every time. Returns how many annotations moved.
    pub fn follow_pids(&self, moves: &[(String, String)]) -> Result<usize> {
        let mut seen = std::collections::HashSet::new();
        let moves: Vec<&(String, String)> = moves.iter().filter(|(pid, _)| seen.insert(pid.as_str())).collect();
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare_cached("UPDATE OR IGNORE annotation SET item = ?2 WHERE kind = 'song' AND pid = ?1 AND item != ?2")?;
            let mut ledger = tx.prepare_cached("UPDATE OR IGNORE imported SET item = ?2 WHERE kind = 'song' AND pid = ?1 AND item != ?2")?;
            let mut marks = tx.prepare_cached("UPDATE OR IGNORE bookmark SET item = ?2 WHERE pid = ?1 AND item != ?2")?;
            for (pid, item) in moves {
                n += st.execute(params![pid, item])?;
                ledger.execute(params![pid, item])?;
                marks.execute(params![pid, item])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// The queues the apps of an account saved (Subsonic `savePlayQueue`, decision 0033 step 5), one per app.
    pub fn app_queues(&self, user: &str) -> Result<Vec<AppQueue>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT client, songs, current, position_ms, changed FROM app_queue WHERE user = ?1")?;
        let rows = st.query_map([user], |r| {
            let songs: String = r.get(1)?;
            Ok(AppQueue {
                client: r.get(0)?,
                songs: songs.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                current: r.get::<_, Option<i64>>(2)?.and_then(|i| usize::try_from(i).ok()),
                position_ms: r.get::<_, i64>(3)?.max(0) as u64,
                changed: r.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// Keeps an app's queue in place of the one it saved before; an empty one is taken away (OpenSubsonic: a save
    /// without songs clears).
    pub fn save_app_queue(&self, user: &str, q: &AppQueue) -> Result<()> {
        let db = self.db();
        if q.songs.is_empty() {
            db.execute("DELETE FROM app_queue WHERE user = ?1 AND client = ?2", params![user, q.client])?;
        } else {
            db.execute(
                "INSERT OR REPLACE INTO app_queue (user, client, songs, current, position_ms, changed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![user, q.client, q.songs.join(","), q.current.map(|i| i as i64), q.position_ms as i64, q.changed],
            )?;
        }
        Ok(())
    }

    /// Keeps a position in a song (`pid`: its persistent id), replacing the one kept before.
    pub fn set_bookmark(&self, user: &str, item: &str, pid: Option<&str>, position_ms: u64, comment: &str, by: Option<&str>) -> Result<()> {
        self.db().execute(
            "INSERT INTO bookmark (user, item, pid, position_ms, comment, created, changed, changed_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)
             ON CONFLICT (user, item) DO UPDATE SET pid = coalesce(excluded.pid, bookmark.pid), position_ms = excluded.position_ms,
               comment = excluded.comment, changed = excluded.changed, changed_by = excluded.changed_by",
            params![user, item, pid, position_ms as i64, comment, now(), by],
        )?;
        Ok(())
    }

    /// A user's bookmarks, the last changed first.
    pub fn bookmarks(&self, user: &str) -> Result<Vec<Bookmark>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT item, position_ms, comment, created, changed, changed_by FROM bookmark WHERE user = ?1 ORDER BY changed DESC, item",
        )?;
        let rows = st.query_map([user], |r| {
            Ok(Bookmark {
                item: r.get(0)?,
                position_ms: r.get::<_, i64>(1)?.max(0) as u64,
                comment: r.get(2)?,
                created: r.get(3)?,
                changed: r.get(4)?,
                changed_by: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_bookmark(&self, user: &str, item: &str) -> Result<()> {
        self.db().execute("DELETE FROM bookmark WHERE user = ?1 AND item = ?2", params![user, item])?;
        Ok(())
    }

    /// A user's snapshots of the visuals and, another's, those of `library` (the owner's: every account's presets),
    /// marked shared; the user's own first, each the newest first.
    pub fn snapshots(&self, user: &str, library: Option<&str>) -> Result<Vec<Snapshot>> {
        let db = self.db();
        let library = library.filter(|l| *l != user).unwrap_or(user);
        let mut st = db.prepare_cached(&format!(
            "SELECT {SNAPSHOT_COLUMNS}, user <> ?1 FROM visual_snapshot WHERE user = ?1 OR user = ?2 ORDER BY user <> ?1, created DESC, rowid DESC"
        ))?;
        let rows = st.query_map(params![user, library], snapshot_of)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A snapshot kept, under a new id.
    pub fn add_snapshot(&self, user: &str, name: &str, settings: &serde_json::Value, picture: Option<&str>) -> Result<Snapshot> {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("random: {e}"))?;
        let id: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let created = now();
        self.db().execute(
            "INSERT INTO visual_snapshot (user, id, name, settings, created, picture) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![user, id, name, settings.to_string(), created, picture],
        )?;
        Ok(Snapshot { id, name: name.to_string(), settings: settings.clone(), created, picture: picture.map(str::to_string), shared: false })
    }

    /// A snapshot renamed, given another picture, or saved over with other settings (whichever are given; its time kept);
    /// `None` where the user has none by that id.
    pub fn change_snapshot(&self, user: &str, id: &str, name: Option<&str>, picture: Option<&str>, settings: Option<&serde_json::Value>) -> Result<Option<Snapshot>> {
        let db = self.db();
        let changed = db.execute(
            "UPDATE visual_snapshot SET name = coalesce(?3, name), picture = coalesce(?4, picture), settings = coalesce(?5, settings)
               WHERE user = ?1 AND id = ?2",
            params![user, id, name, picture, settings.map(|s| s.to_string())],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(db
            .query_row(&format!("SELECT {SNAPSHOT_COLUMNS} FROM visual_snapshot WHERE user = ?1 AND id = ?2"), params![user, id], snapshot_of)
            .optional()?)
    }

    /// A rating kept.
    pub fn add_rating(&self, user: &str, r: &Rating) -> Result<()> {
        self.db().execute(
            "INSERT INTO visual_rating (user, at, a, b, choice, track, context) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![user, r.at, r.a.to_string(), r.b.to_string(), r.choice, r.track, r.context.to_string()],
        )?;
        Ok(())
    }

    /// A user's ratings, the oldest first (as they were given).
    pub fn ratings(&self, user: &str) -> Result<Vec<Rating>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT at, a, b, choice, track, context FROM visual_rating WHERE user = ?1 ORDER BY at, rowid")?;
        let json = |s: String| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
        let rows = st.query_map([user], |r| {
            Ok(Rating { at: r.get(0)?, a: json(r.get(1)?), b: json(r.get(2)?), choice: r.get(3)?, track: r.get(4)?, context: json(r.get(5)?) })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A thumb kept, at the server's time now.
    pub fn add_thumb(&self, user: &str, thumb: &str, body: &serde_json::Map<String, serde_json::Value>) -> Result<Thumb> {
        let at = now_ms();
        self.db().execute(
            "INSERT INTO visual_thumb (user, at, thumb, body) VALUES (?1, ?2, ?3, ?4)",
            params![user, at, thumb, serde_json::Value::Object(body.clone()).to_string()],
        )?;
        Ok(Thumb { at, thumb: thumb.to_string(), body: body.clone() })
    }

    /// A user's thumbs, the oldest first.
    pub fn thumbs(&self, user: &str) -> Result<Vec<Thumb>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT at, thumb, body FROM visual_thumb WHERE user = ?1 ORDER BY at, rowid")?;
        let rows = st.query_map([user], |r| {
            let body: String = r.get(2)?;
            let body = match serde_json::from_str(&body) {
                Ok(serde_json::Value::Object(m)) => m,
                _ => serde_json::Map::new(),
            };
            Ok(Thumb { at: r.get(0)?, thumb: r.get(1)?, body })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What a user's governor learned, if anything.
    pub fn learned(&self, user: &str) -> Result<Option<Learned>> {
        let db = self.db();
        Ok(db
            .query_row("SELECT version, limits, at FROM visual_limits WHERE user = ?1", [user], |r| {
                let limits: String = r.get(1)?;
                Ok(Learned { version: r.get(0)?, limits: serde_json::from_str(&limits).unwrap_or(serde_json::Value::Null), at: r.get(2)? })
            })
            .optional()?)
    }

    /// What was learned, kept over the version `base` (0: where nothing is kept yet); the new version, or `Stale` where
    /// `base` is not the one kept. Read and written under the one connection's lock, so of two at once one wins.
    pub fn put_learned(&self, user: &str, base: i64, limits: &serde_json::Value, at: i64) -> Result<std::result::Result<i64, Stale>> {
        let db = self.db();
        let kept: Option<i64> = db.query_row("SELECT version FROM visual_limits WHERE user = ?1", [user], |r| r.get(0)).optional()?;
        if kept.unwrap_or(0) != base {
            return Ok(Err(Stale));
        }
        let version = now_ms().max(base + 1);
        db.execute(
            "INSERT INTO visual_limits (user, version, limits, at) VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT (user) DO UPDATE SET version = excluded.version, limits = excluded.limits, at = excluded.at",
            params![user, version, limits.to_string(), at],
        )?;
        Ok(Ok(version))
    }

    /// Whether an account keeps its play history (0.9.5, the owner: Personal › Tracking; its settings' `tracking`, on
    /// unless set off). Off: nothing of its listening is kept, sent or learned from; what it said (favourites, ratings,
    /// exclusions) still counts.
    pub fn tracking(&self, user: &str) -> bool {
        self.settings_of(user).map_or(true, |(_, s)| s.get("tracking").and_then(serde_json::Value::as_bool) != Some(false))
    }

    /// "Delete play history" (0.9.5): an account's plays and what was made of them: its log, its counts (its stars and
    /// ratings stay), the plays waiting to be sent, its last Discover look. How many plays went.
    pub fn forget_history(&self, user: &str) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let removed = tx.execute("DELETE FROM plays WHERE user = ?1", [user])?;
        tx.execute("UPDATE annotation SET play_count = 0, play_date = NULL, own_count = 0, own_date = NULL WHERE user = ?1", [user])?;
        tx.execute("DELETE FROM annotation WHERE user = ?1 AND play_count = 0 AND rating = 0 AND starred = 0", [user])?;
        tx.execute("DELETE FROM scrobble_queue WHERE user = ?1", [user])?;
        tx.execute("DELETE FROM discover_found WHERE user = ?1", [user])?;
        tx.execute("DELETE FROM discover_around WHERE user = ?1", [user])?;
        tx.commit()?;
        Ok(removed)
    }

    /// The plays waiting to be sent for an account, taken away (its play history switched off).
    pub fn forget_scrobbles(&self, user: &str) -> Result<usize> {
        Ok(self.db().execute("DELETE FROM scrobble_queue WHERE user = ?1", [user])?)
    }

    /// A user's own settings (decision 0051: Radio's and Autoplay's defaults and the like, as the apps keep them) and
    /// their version; version 0 and none while nothing is kept.
    pub fn settings_of(&self, user: &str) -> Result<(i64, serde_json::Value)> {
        let kept = self
            .db()
            .query_row("SELECT version, settings FROM user_settings WHERE user = ?1", [user], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .optional()?;
        Ok(match kept {
            Some((version, s)) => (version, serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({}))),
            None => (0, serde_json::json!({})),
        })
    }

    /// A user's settings, kept over the version `base`; the new version, or `Stale` where `base` is not the one kept.
    pub fn put_settings(&self, user: &str, base: i64, settings: &serde_json::Value) -> Result<std::result::Result<i64, Stale>> {
        let db = self.db();
        let kept: Option<i64> = db.query_row("SELECT version FROM user_settings WHERE user = ?1", [user], |r| r.get(0)).optional()?;
        if kept.unwrap_or(0) != base {
            return Ok(Err(Stale));
        }
        let version = now_ms().max(base + 1);
        db.execute(
            "INSERT INTO user_settings (user, version, settings, at) VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT (user) DO UPDATE SET version = excluded.version, settings = excluded.settings, at = excluded.at",
            params![user, version, settings.to_string(), now()],
        )?;
        Ok(Ok(version))
    }

    /// What was learned, forgotten: the next keeps over version 0.
    pub fn forget_learned(&self, user: &str) -> Result<()> {
        self.db().execute("DELETE FROM visual_limits WHERE user = ?1", [user])?;
        Ok(())
    }

    /// Whether there was such a snapshot to delete.
    pub fn delete_snapshot(&self, user: &str, id: &str) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM visual_snapshot WHERE user = ?1 AND id = ?2", params![user, id])? > 0)
    }

    /// A playlist was renamed: its old names lead to the new one. A Subsonic playlist's id is `pl-` and its name, and
    /// clients keep the id across a rename (Symfonium asks for it again straight after renaming).
    pub fn playlist_renamed(&self, from: &str, to: &str) -> Result<()> {
        let at = now();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute("UPDATE playlist_alias SET name = ?2, at = ?3 WHERE name = ?1", params![from, to, at])?;
        tx.execute("INSERT OR REPLACE INTO playlist_alias (old, name, at) VALUES (?1, ?2, ?3)", params![from, to, at])?;
        tx.execute("DELETE FROM playlist_alias WHERE old = ?1", [to])?;
        tx.execute("DELETE FROM playlist_entry WHERE playlist = ?1", [to])?;
        tx.execute("UPDATE playlist_entry SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        tx.execute("DELETE FROM playlist_shared WHERE playlist = ?1", [to])?;
        tx.execute("UPDATE playlist_shared SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        tx.commit()?;
        Ok(())
    }

    /// The playlist an old name leads to, if it was renamed.
    pub fn playlist_alias(&self, old: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT name FROM playlist_alias WHERE old = ?1", [old], |r| r.get(0)).optional()?)
    }

    /// A new playlist took a name: it no longer leads elsewhere.
    pub fn playlist_made(&self, name: &str) -> Result<()> {
        self.db().execute("DELETE FROM playlist_alias WHERE old = ?1", [name])?;
        Ok(())
    }

    /// A playlist is gone: none of its old names leads anywhere, and it is no longer shared.
    pub fn playlist_gone(&self, name: &str) -> Result<()> {
        self.db().execute("DELETE FROM playlist_alias WHERE name = ?1 OR old = ?1", [name])?;
        self.db().execute("DELETE FROM playlist_entry WHERE playlist = ?1", [name])?;
        self.db().execute("DELETE FROM playlist_shared WHERE playlist = ?1", [name])?;
        Ok(())
    }

    /// The playlists their owners share with everyone (decision 0051), by their keys (`Store::key`).
    pub fn shared_playlists(&self) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT playlist FROM playlist_shared")?;
        Ok(st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_playlist_shared(&self, playlist: &str, on: bool) -> Result<()> {
        if on {
            self.db().execute("INSERT OR IGNORE INTO playlist_shared (playlist, at) VALUES (?1, ?2)", params![playlist, now()])?;
        } else {
            self.db().execute("DELETE FROM playlist_shared WHERE playlist = ?1", [playlist])?;
        }
        Ok(())
    }

    /// A playlist was written (every write: the store tells it, `playlists::on_write`): an entry that came now dates
    /// from now, one that went is forgotten. Entries are told apart as "the n-th copy of a line", so taking out an
    /// earlier copy hands the later one its date: only repeats are touched by that. Entries a playlist held before
    /// this was kept have no date (Added order puts them first, in file order).
    pub fn entries_written(&self, playlist: &str, before: &[String], after: &[String]) -> Result<()> {
        let (was, now_keys) = (occurrences(before), occurrences(after));
        let at = now();
        let mut db = self.db();
        let tx = db.transaction()?;
        for (entry, n) in was.iter().filter(|k| !now_keys.contains(k)) {
            tx.execute("DELETE FROM playlist_entry WHERE playlist = ?1 AND entry = ?2 AND n = ?3", params![playlist, entry, *n as i64])?;
        }
        for (entry, n) in now_keys.iter().filter(|k| !was.contains(k)) {
            tx.execute(
                "INSERT OR REPLACE INTO playlist_entry (playlist, entry, n, added) VALUES (?1, ?2, ?3, ?4)",
                params![playlist, entry, *n as i64, at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// When each of a playlist's entries came (epoch seconds), in order; `None` for one held before dates were kept.
    pub fn entries_added(&self, playlist: &str, entries: &[String]) -> Result<Vec<Option<i64>>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT added FROM playlist_entry WHERE playlist = ?1 AND entry = ?2 AND n = ?3")?;
        occurrences(entries)
            .iter()
            .map(|(entry, n)| Ok(st.query_row(params![playlist, entry, *n as i64], |r| r.get::<_, i64>(0)).optional()?))
            .collect()
    }

    /// Every song a user has played at least once (a crate's unplayed tracks are the others).
    pub fn played(&self, user: &str) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT item FROM annotation WHERE user = ?1 AND kind = ?2 AND play_count > 0")?;
        let rows = st.query_map(params![user, Kind::Song.as_str()], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every song a user played: its play count and when it was last played (the playlists' table, decision 0042).
    pub fn song_plays(&self, user: &str) -> Result<std::collections::HashMap<String, (u32, Option<i64>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT item, play_count, play_date FROM annotation WHERE user = ?1 AND kind = ?2 AND play_count > 0")?;
        let rows = st.query_map(params![user, Kind::Song.as_str()], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?.max(0) as u32, r.get(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every song a user played as their own (none heard while following another): its own plays and the last of them,
    /// for what picks (Home's New for you and looks back).
    pub fn own_song_plays(&self, user: &str) -> Result<std::collections::HashMap<String, (u32, Option<i64>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT item, own_count, own_date FROM annotation WHERE user = ?1 AND kind = ?2 AND own_count > 0")?;
        let rows = st.query_map(params![user, Kind::Song.as_str()], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?.max(0) as u32, r.get(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What changes when a user plays anything (how many songs, how many plays, the latest): a cache's key.
    pub fn plays_print(&self, user: &str) -> Result<(i64, i64, i64)> {
        let db = self.db();
        Ok(db.query_row(
            "SELECT count(*), total(play_count), COALESCE(max(play_date), 0) FROM annotation WHERE user = ?1 AND kind = ?2 AND play_count > 0",
            params![user, Kind::Song.as_str()],
            |r| Ok((r.get(0)?, r.get::<_, f64>(1)? as i64, r.get(2)?)),
        )?)
    }

    /// Each song's play count and when it was last played, for a user (the panel's orders).
    pub fn play_stats(&self, user: &str, songs: &[String]) -> Result<std::collections::HashMap<String, (u32, Option<i64>)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT play_count, play_date FROM annotation WHERE user = ?1 AND kind = ?2 AND item = ?3")?;
        let mut out = std::collections::HashMap::new();
        for song in songs {
            if out.contains_key(song) {
                continue;
            }
            if let Some(v) = st
                .query_row(params![user, Kind::Song.as_str(), song], |r| Ok((r.get::<_, i64>(0)?.max(0) as u32, r.get::<_, Option<i64>>(1)?)))
                .optional()?
            {
                out.insert(song.clone(), v);
            }
        }
        Ok(out)
    }

    /// When a playlist was first seen and last seen changed (epoch seconds), given what it holds now (`print`): the
    /// files keep no such dates, and Subsonic clients compare `changed` to know when to read a playlist again.
    pub fn playlist_seen(&self, name: &str, print: &str) -> Result<(i64, i64)> {
        let at = now();
        let db = self.db();
        db.execute(
            "INSERT INTO playlist_seen (name, print, created, changed) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (name) DO UPDATE SET changed = CASE WHEN playlist_seen.print = excluded.print THEN playlist_seen.changed ELSE excluded.changed END,
               print = excluded.print",
            params![name, print, at],
        )?;
        Ok(db.query_row("SELECT created, changed FROM playlist_seen WHERE name = ?1", [name], |r| Ok((r.get(0)?, r.get(1)?)))?)
    }

    /// An album's review (promoted or rejected from Incoming) as asked by its id, and the undo key of what was done.
    pub fn review(&self, album: &str) -> Result<Option<Review>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, base, state, at, undo, name, album_artist FROM album_review WHERE album = ?1 OR base = ?1 ORDER BY at DESC LIMIT 1")?;
        Ok(st
            .query_map([album], |r| {
                Ok(Review {
                    album: r.get(0)?,
                    base: r.get(1)?,
                    state: r.get(2)?,
                    at: r.get(3)?,
                    undo: r.get(4)?,
                    name: r.get(5)?,
                    album_artist: r.get(6)?,
                })
            })?
            .next()
            .transpose()?)
    }

    pub fn set_review(&self, r: &Review) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO album_review (album, base, state, at, undo, name, album_artist) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![r.album, r.base, r.state, r.at, r.undo, r.name, r.album_artist],
        )?;
        Ok(())
    }

    /// Back to unreviewed.
    pub fn clear_review(&self, album: &str) -> Result<()> {
        self.db().execute("DELETE FROM album_review WHERE album = ?1", [album])?;
        Ok(())
    }

    /// Answers when the database can be read. For the health check.
    pub fn health(&self) -> Result<()> {
        self.db().query_row("SELECT count(*) FROM annotation", [], |r| r.get::<_, i64>(0))?;
        Ok(())
    }

    /// When albums were first added, where known better than the catalogue (`origin` says from where).
    pub fn set_added(&self, album: &str, at: i64, origin: &str) -> Result<()> {
        self.db().execute(
            "INSERT INTO added (album, at, origin) VALUES (?1, ?2, ?3) ON CONFLICT (album) DO UPDATE SET at = min(added.at, excluded.at)",
            params![album, at, origin],
        )?;
        Ok(())
    }

    /// `set_added` for many albums, all or none.
    pub fn set_added_many(&self, albums: &[(String, i64)], origin: &str) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO added (album, at, origin) VALUES (?1, ?2, ?3) ON CONFLICT (album) DO UPDATE SET at = min(added.at, excluded.at)",
            )?;
            for (album, at) in albums {
                st.execute(params![album, at, origin])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A release group's cover as last looked for (`Some(None)`: neither has one), and when.
    pub fn rg_cover(&self, rg: &str) -> Result<Option<(i64, Option<String>)>> {
        Ok(self.db().query_row("SELECT at, url FROM rg_covers WHERE rg = ?1", [rg], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn set_rg_cover(&self, rg: &str, url: Option<&str>, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO rg_covers (rg, url, at) VALUES (?1, ?2, ?3)", params![rg, url, at])?;
        Ok(())
    }

    /// Discogs' genres and styles found for albums, by album (those found none left out).
    pub fn discogs_genres(&self) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album, genres FROM discogs_genres WHERE genres <> ''")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?.split('\u{1f}').map(str::to_string).collect())))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The albums not to ask Discogs about again: those with genres found, and those asked since `since`.
    pub fn discogs_genres_asked(&self, since: i64) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album FROM discogs_genres WHERE genres <> '' OR at >= ?1")?;
        let rows = st.query_map([since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What Discogs answered for these albums (genres and styles, or none), as one write.
    pub fn set_discogs_genres(&self, found: &[(String, Vec<String>)], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare("INSERT OR REPLACE INTO discogs_genres (album, genres, at) VALUES (?1, ?2, ?3)")?;
            for (album, genres) in found {
                st.execute(params![album, genres.join("\u{1f}"), at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The albums a renumber look matched to a release (0.9.6).
    pub fn renumber_matched(&self) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album FROM renumber_matched")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An album a renumber look matched to a release.
    pub fn set_renumber_matched(&self, album: &str, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO renumber_matched (album, at) VALUES (?1, ?2)", params![album, at])?;
        Ok(())
    }

    /// Every release type found (0.9.6), by `mb:<release>` or `dc:<album id>`: (primary, secondary).
    pub fn release_types(&self) -> Result<std::collections::HashMap<String, (Option<String>, Vec<String>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT key, primary_type, secondary FROM release_types WHERE primary_type IS NOT NULL OR secondary <> ''")?;
        let rows = st.query_map([], |r| {
            let secondary: String = r.get(2)?;
            Ok((r.get::<_, String>(0)?, (r.get::<_, Option<String>>(1)?, secondary.split('\u{1f}').filter(|s| !s.is_empty()).map(str::to_string).collect())))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The keys starting `prefix` not to ask about again: those with a primary type, and those asked since `since`;
    /// without their prefix. Those an older rule found where `RELEASE_TYPES_RULE` changed it (every one of Discogs', an
    /// EP or single of MusicBrainz's) are asked again, kept as they are until then.
    pub fn release_types_asked(&self, prefix: &str, since: i64) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT key FROM release_types WHERE substr(key, 1, length(?1)) = ?1 AND (primary_type IS NOT NULL OR at >= ?2)
             AND NOT (rule < ?3 AND (substr(key, 1, 3) = 'dc:' OR coalesce(primary_type, '') IN ('ep', 'single')))",
        )?;
        let rows = st.query_map(params![prefix, since, RELEASE_TYPES_RULE], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|k| k.ok().map(|k| k[prefix.len()..].to_string())).collect())
    }

    /// What was found for these keys (a primary type or none, the secondary ones), as one write.
    pub fn set_release_types(&self, found: &[(String, Option<String>, Vec<String>)], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare("INSERT OR REPLACE INTO release_types (key, primary_type, secondary, at, rule) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for (key, primary, secondary) in found {
                st.execute(params![key, primary, secondary.join("\u{1f}"), at, RELEASE_TYPES_RULE])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Every original year MusicBrainz gave, by the album's MusicBrainz release (0.9.5).
    pub fn original_years(&self) -> Result<std::collections::HashMap<String, u32>> {
        let db = self.db();
        let mut st = db.prepare("SELECT release, year FROM original_years WHERE year IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.clamp(0, i64::from(u32::MAX)) as u32)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The releases not to ask about again: those with a year, and those asked since `since`.
    pub fn original_years_asked(&self, since: i64) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT release FROM original_years WHERE year IS NOT NULL OR at >= ?1")?;
        let rows = st.query_map([since], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What MusicBrainz answered for these releases (a year, or none), as one write.
    pub fn set_original_years(&self, found: &[(String, Option<u32>)], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare("INSERT OR REPLACE INTO original_years (release, year, at) VALUES (?1, ?2, ?3)")?;
            for (release, year) in found {
                st.execute(params![release, year, at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn added(&self) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album, at FROM added")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Each entry as "the n-th copy of this line" (from 0), in order.
fn occurrences(entries: &[String]) -> Vec<(String, usize)> {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    entries
        .iter()
        .map(|e| {
            let n = seen.entry(e.as_str()).or_default();
            *n += 1;
            (e.clone(), *n - 1)
        })
        .collect()
}

/// An album asked for (decision 0070), as kept.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub id: i64,
    pub user: String,
    /// The suggestion's key and its artist's (app::discover), so it leaves that account's Discover.
    pub album_key: String,
    pub artist_key: String,
    pub release_group: Option<String>,
    pub album: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub at: i64,
    /// `pending`, `approved`, `declined` or `cancelled`.
    pub status: String,
    /// Why it was declined, if a reason was given.
    pub reason: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    /// What the fetcher (Lidarr) said when it was handed the request.
    pub fetched: Option<String>,
}

const REQUEST_COLUMNS: &str =
    "id, user, album_key, artist_key, release_group, album, artist, year, cover, at, status, reason, decided_by, decided_at, fetched";

fn request_row(r: &rusqlite::Row) -> rusqlite::Result<Request> {
    Ok(Request {
        id: r.get(0)?,
        user: r.get(1)?,
        album_key: r.get(2)?,
        artist_key: r.get(3)?,
        release_group: r.get(4)?,
        album: r.get(5)?,
        artist: r.get(6)?,
        year: r.get(7)?,
        cover: r.get(8)?,
        at: r.get(9)?,
        status: r.get(10)?,
        reason: r.get(11)?,
        decided_by: r.get(12)?,
        decided_at: r.get(13)?,
        fetched: r.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discogs_genres_and_covers_are_kept_and_an_album_without_any_waits() {
        let u = UserData::open(None).unwrap();
        u.set_discogs_genres(&[("al-1".into(), vec!["Electronic".into(), "Dub".into()]), ("al-2".into(), Vec::new())], 100).unwrap();
        assert_eq!(u.discogs_genres().unwrap(), [("al-1".to_string(), vec!["Electronic".to_string(), "Dub".to_string()])].into());
        assert_eq!(u.discogs_genres_asked(200).unwrap(), ["al-1".to_string()].into(), "one without genres asked again later");
        u.set_rg_cover("rg-1", None, 5).unwrap();
        assert_eq!(u.rg_cover("rg-1").unwrap(), Some((5, None)));
        assert_eq!(u.rg_cover("rg-2").unwrap(), None);
    }

    #[test]
    fn tracking_off_reads_no_plays_and_deleting_the_history_keeps_what_was_said() {
        let u = UserData::open(None).unwrap();
        assert!(u.tracking("me"), "on unless set off");
        u.count_play("me", ("tr-1", "p1"), "al-1", &[], 100).unwrap();
        u.count_play("me", ("tr-2", "p2"), "al-1", &[], 200).unwrap();
        u.set_starred("me", Kind::Song, &["tr-2".to_string()], true).unwrap();
        assert_eq!(u.heard_since("me", 0).unwrap().len(), 2);
        u.put_settings("me", 0, &serde_json::json!({ "tracking": false })).unwrap().ok().unwrap();
        assert!(!u.tracking("me"));
        assert!(u.heard_since("me", 0).unwrap().is_empty());
        assert!(u.used("me", Kind::Song).unwrap().values().all(|a| a.play_count == 0), "no counts read");
        assert_eq!(u.forget_history("me").unwrap(), 2);
        assert_eq!(u.starred("me", Kind::Song).unwrap(), ["tr-2".to_string()], "the favourite stays");
        let (version, _) = u.settings_of("me").unwrap();
        u.put_settings("me", version, &serde_json::json!({ "tracking": true })).unwrap().ok().unwrap();
        assert!(u.heard_since("me", 0).unwrap().is_empty(), "the history gone");
    }

    #[test]
    fn release_types_are_kept_by_their_key_and_one_without_a_type_waits() {
        let u = UserData::open(None).unwrap();
        u.set_release_types(&[("mb:a".into(), Some("ep".into()), vec!["live".into()]), ("mb:b".into(), None, Vec::new()), ("dc:al-1".into(), Some("single".into()), Vec::new())], 100).unwrap();
        let all = u.release_types().unwrap();
        assert_eq!(all.get("mb:a"), Some(&(Some("ep".to_string()), vec!["live".to_string()])));
        assert!(!all.contains_key("mb:b"), "nothing known");
        assert_eq!(u.release_types_asked("mb:", 200).unwrap(), ["a".to_string()].into(), "b asked again after a while");
        assert_eq!(u.release_types_asked("mb:", 50).unwrap().len(), 2);
        assert_eq!(u.release_types_asked("dc:", 50).unwrap(), ["al-1".to_string()].into());
    }

    #[test]
    fn release_types_an_older_rule_found_are_asked_again_where_the_rule_changed_and_kept_until_then() {
        let u = UserData::open(None).unwrap();
        let found: [(String, Option<String>, Vec<String>); 5] = [
            ("mb:ep".into(), Some("ep".into()), Vec::new()),
            ("mb:none".into(), None, Vec::new()),
            ("mb:single".into(), Some("single".into()), Vec::new()),
            ("mb:album".into(), Some("album".into()), Vec::new()),
            ("dc:al-1".into(), Some("single".into()), Vec::new()),
        ];
        u.set_release_types(&found, 100).unwrap();
        u.db().execute("UPDATE release_types SET rule = 0", []).unwrap();
        assert_eq!(u.release_types_asked("mb:", 0).unwrap(), ["album".to_string(), "none".to_string()].into(), "an EP or single of MusicBrainz's asked again");
        assert!(u.release_types_asked("dc:", 0).unwrap().is_empty(), "every one of Discogs' asked again");
        assert_eq!(u.release_types().unwrap().get("dc:al-1"), Some(&(Some("single".to_string()), Vec::new())), "kept until then");
        u.set_release_types(&found, 200).unwrap();
        assert_eq!(u.release_types_asked("mb:", 0).unwrap().len(), 4, "asked by this rule: done");
        assert_eq!(u.release_types_asked("dc:", 0).unwrap().len(), 1);
    }

    #[test]
    fn original_years_are_kept_and_a_release_without_one_waits() {
        let u = UserData::open(None).unwrap();
        u.set_original_years(&[("rel-a".into(), Some(2008)), ("rel-b".into(), None)], 100).unwrap();
        assert_eq!(u.original_years().unwrap(), [("rel-a".to_string(), 2008)].into());
        let asked = |since| u.original_years_asked(since).unwrap().into_iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(asked(50), ["rel-a".to_string(), "rel-b".to_string()].into());
        assert_eq!(asked(200), ["rel-a".to_string()].into(), "one MusicBrainz knew no year for, asked again later");
    }

    #[test]
    fn entries_date_from_the_write_that_brought_them() {
        let u = UserData::open(None).unwrap();
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // Held before dates were kept: no date.
        assert_eq!(u.entries_added("p", &v(&["a", "b"])).unwrap(), vec![None, None]);
        u.entries_written("p", &v(&["a", "b"]), &v(&["a", "b", "c", "a"])).unwrap();
        let got = u.entries_added("p", &v(&["a", "b", "c", "a"])).unwrap();
        assert_eq!((got[0], got[1]), (None, None), "a and b were there before");
        assert!(got[2].is_some() && got[3].is_some(), "c and the second a came now");
        // A copy taken out is forgotten; renamed, the dates go with the playlist; gone, they go.
        u.entries_written("p", &v(&["a", "b", "c", "a"]), &v(&["a", "b", "c"])).unwrap();
        assert_eq!(u.entries_added("p", &v(&["a", "a"])).unwrap()[1], None, "the second a is gone");
        u.playlist_renamed("p", "q").unwrap();
        assert!(u.entries_added("q", &v(&["c"])).unwrap()[0].is_some());
        u.playlist_gone("q").unwrap();
        assert_eq!(u.entries_added("q", &v(&["c"])).unwrap(), vec![None]);
    }

    #[test]
    fn play_stats_are_a_users_count_and_last_play() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-1", &[], 100).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-1", &[], 200).unwrap();
        let s = u.play_stats("me", &["tr-1".to_string(), "tr-2".to_string()]).unwrap();
        assert_eq!(s.get("tr-1"), Some(&(2, Some(200))));
        assert!(!s.contains_key("tr-2"), "never played");
    }

    #[test]
    fn every_track_heard_is_logged_once_and_only_a_counted_one_counts() {
        let u = UserData::open(None).unwrap();
        assert_eq!(u.heard_from("me").unwrap(), None, "the log starts empty");
        let skip = Heard { ended: Ended::Skipped, counts: false, heard_secs: Some(12.0), length_secs: Some(240.0), event: Some("e1".into()), ..Heard::counted(100) };
        assert!(u.record("me", ("tr-1", "pid1"), "al-1", &[], &skip).unwrap());
        assert!(!u.record("me", ("tr-1", "pid1"), "al-1", &[], &skip).unwrap(), "the same report again: one row");
        u.count_play("me", ("tr-2", "pid2"), "al-1", &[], 200).unwrap();
        let log = u.heard_since("me", 0).unwrap();
        assert_eq!(log.iter().map(|l| (l.song.as_str(), l.ended, l.counted)).collect::<Vec<_>>(), [("tr-1", Ended::Skipped, false), ("tr-2", Ended::Finished, true)]);
        assert_eq!(log[0].heard_secs, Some(12.0));
        let s = u.play_stats("me", &["tr-1".to_string(), "tr-2".to_string()]).unwrap();
        assert!(!s.contains_key("tr-1"), "a skip is no play");
        assert_eq!(s.get("tr-2"), Some(&(1, Some(200))));
        assert_eq!(u.heard_from("me").unwrap(), Some(100));
        let imported = Heard { counts: false, context: Some("listenbrainz".into()), event: Some("lb".into()), ..Heard::counted(10) };
        u.record("me", ("tr-1", "pid1"), "al-1", &[], &imported).unwrap();
        assert_eq!(u.heard_from("me").unwrap(), Some(100), "an import's older rows are no live history");
        assert!(u.heard_since("me", 150).unwrap().len() == 1 && u.heard_since("you", 0).unwrap().is_empty());
    }

    #[test]
    fn a_play_heard_while_following_is_logged_sent_and_counted_but_not_as_ones_own() {
        let u = UserData::open(None).unwrap();
        u.set_scrobble_target("me", "listenbrainz", "me", "{}").unwrap();
        let along = Heard { event: Some("f1".into()), followed: true, ..Heard::counted(500) };
        assert!(u.record("me", ("tr-1", "pid1"), "al-1", &["ar-1".to_string()], &along).unwrap());
        let log = u.heard_since("me", 0).unwrap();
        assert_eq!(log.iter().map(|l| (l.song.as_str(), l.counted, l.followed)).collect::<Vec<_>>(), [("tr-1", true, true)], "in the history");
        assert_eq!(u.due_scrobbles(i64::MAX, 10).unwrap().len(), 1, "scrobbled");
        assert_eq!(u.play_stats("me", &["tr-1".to_string()]).unwrap().get("tr-1"), Some(&(1, Some(500))), "counted on the song");
        let counts = |kind, item: &str| {
            let a = u.get("me", kind, &[item.to_string()]).unwrap().remove(item).unwrap_or_default();
            (a.play_count, a.play_date, a.own_count, a.own_date)
        };
        for (kind, item) in [(Kind::Song, "tr-1"), (Kind::Album, "al-1"), (Kind::Artist, "ar-1")] {
            assert_eq!(counts(kind, item), (1, Some(500), 0, None), "{item}: in the play count, not one's own");
        }
        assert!(u.own_song_plays("me").unwrap().is_empty(), "nothing played as one's own");
        // One's own play adds to both; the followed one stays out of the own count and its date.
        u.count_play("me", ("tr-1", "pid1"), "al-1", &["ar-1".to_string()], 400).unwrap();
        assert_eq!(counts(Kind::Song, "tr-1"), (2, Some(500), 1, Some(400)));
        assert_eq!(counts(Kind::Artist, "ar-1"), (2, Some(500), 1, Some(400)));
        assert_eq!(u.own_song_plays("me").unwrap().get("tr-1"), Some(&(1, Some(400))));
        u.count_play("me", ("tr-2", "pid2"), "al-1", &[], 600).unwrap();
        assert!(!u.heard_since("me", 0).unwrap()[2].followed);
        assert_eq!(counts(Kind::Song, "tr-2"), (1, Some(600), 1, Some(600)), "one's own: both");
        assert_eq!(u.forget_history("me").unwrap(), 3);
        assert!(u.get("me", Kind::Song, &["tr-1".into()]).unwrap().is_empty(), "both counts gone with the history");
    }

    #[test]
    fn counts_from_before_are_ones_own() {
        let dir = std::env::temp_dir().join(format!("cratarr-own-counts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("u.db");
        let _ = std::fs::remove_file(&file);
        // The annotation table as it was before one's own plays were kept apart.
        let old = Connection::open(&file).unwrap();
        old.execute_batch(
            "CREATE TABLE annotation (
               user TEXT NOT NULL, item TEXT NOT NULL, kind TEXT NOT NULL, pid TEXT,
               play_count INTEGER NOT NULL DEFAULT 0, play_date INTEGER, rating INTEGER NOT NULL DEFAULT 0,
               starred INTEGER NOT NULL DEFAULT 0, starred_at INTEGER,
               PRIMARY KEY (user, item, kind)) WITHOUT ROWID;
             INSERT INTO annotation (user, item, kind, pid, play_count, play_date) VALUES ('me', 'tr-1', 'song', 'p1', 5, 300);
             INSERT INTO annotation (user, item, kind, rating) VALUES ('me', 'al-1', 'album', 4);",
        )
        .unwrap();
        drop(old);
        let u = UserData::open(Some(&file)).unwrap();
        let a = &u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"];
        assert_eq!((a.play_count, a.play_date, a.own_count, a.own_date), (5, Some(300), 5, Some(300)), "filled from the counts");
        let b = &u.get("me", Kind::Album, &["al-1".into()]).unwrap()["al-1"];
        assert_eq!((b.own_count, b.own_date, b.rating), (0, None, 4));
        // Opened again after a followed play: nothing filled twice.
        let along = Heard { event: Some("f1".into()), followed: true, ..Heard::counted(400) };
        u.record("me", ("tr-1", "p1"), "al-1", &[], &along).unwrap();
        drop(u);
        let u = UserData::open(Some(&file)).unwrap();
        let a = &u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"];
        assert_eq!((a.play_count, a.play_date, a.own_count, a.own_date), (6, Some(400), 5, Some(300)));
        drop(u);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_week_before_the_log_is_written_once_from_the_last_plays_and_counted_nothing_again() {
        let u = UserData::open(None).unwrap();
        let day = 86_400;
        let first = 30 * day;
        // Last plays: one in the week before the log, one before that, one after the log began (told by a player too).
        u.count_play("me", ("tr-1", "pid1"), "al-1", &[], first - 2 * day).unwrap();
        u.count_play("me", ("tr-2", "pid2"), "al-1", &[], first - 9 * day).unwrap();
        u.db().execute("DELETE FROM plays", []).unwrap();
        let told = Heard { event: Some("e1".into()), ..Heard::counted(first) };
        u.record("me", ("tr-3", "pid3"), "al-1", &[], &told).unwrap();
        u.backfill_week().unwrap();
        u.backfill_week().unwrap();
        let log = u.heard_since("me", 0).unwrap();
        assert_eq!(log.iter().map(|l| (l.song.as_str(), l.at, l.context.as_deref())).collect::<Vec<_>>(), [("tr-1", first - 2 * day, Some("backfill:annotation")), ("tr-3", first, None)], "once, only the week before");
        assert!(log[0].counted && log[0].ended == Ended::Finished);
        assert_eq!(u.heard_from("me").unwrap(), Some(first), "the log as told still begins with the player's row");
        let s = u.play_stats("me", &["tr-1".to_string()]).unwrap();
        assert_eq!(s.get("tr-1"), Some(&(1, Some(first - 2 * day))), "counted nothing again");
    }

    #[test]
    fn the_albums_used_are_those_played_or_rated() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-1", &[], 100).unwrap();
        u.set_rating("me", Kind::Album, "al-2", 4).unwrap();
        let mut told = u.subscribe_starred();
        u.set_starred("me", Kind::Album, &["al-3".to_string()], true).unwrap();
        // Every change told, whoever made it (the live channel passes it on to the account's pages).
        assert_eq!(told.try_recv().unwrap(), Starred { user: "me".into(), kind: "album", items: vec!["al-3".into()], on: true });
        let used = u.used("me", Kind::Album).unwrap();
        assert_eq!((used["al-1"].play_count, used["al-1"].play_date, used["al-1"].rating), (1, Some(100), 0));
        assert_eq!((used["al-2"].play_count, used["al-2"].rating), (0, 4));
        assert!(!used.contains_key("al-3"), "only starred: not used");
        assert!(u.used("you", Kind::Album).unwrap().is_empty(), "per user");
    }

    #[test]
    fn stars_keep_their_first_time_and_go_on_unstar() {
        let u = UserData::open(None).unwrap();
        let ids = vec!["tr-1".to_string()];
        u.set_starred("me", Kind::Song, &ids, true).unwrap();
        let first = u.get("me", Kind::Song, &ids).unwrap()["tr-1"].starred_at;
        assert!(first.is_some());
        u.set_starred("me", Kind::Song, &ids, true).unwrap();
        assert_eq!(u.get("me", Kind::Song, &ids).unwrap()["tr-1"].starred_at, first);
        assert_eq!(u.starred("me", Kind::Song).unwrap(), ids);
        u.set_starred("me", Kind::Song, &ids, false).unwrap();
        assert!(u.starred("me", Kind::Song).unwrap().is_empty());
        assert!(u.get("you", Kind::Song, &ids).unwrap().is_empty(), "per user");
    }

    #[test]
    fn a_play_counts_on_song_album_and_artists() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-x", &["ar-a".into()], 100).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-x", &["ar-a".into()], 50).unwrap();
        let song = &u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"];
        assert_eq!((song.play_count, song.play_date), (2, Some(100)), "the latest play stays the last played");
        assert_eq!(u.get("me", Kind::Album, &["al-x".into()]).unwrap()["al-x"].play_count, 2);
        assert_eq!(u.get("me", Kind::Artist, &["ar-a".into()]).unwrap()["ar-a"].play_count, 2);
    }

    #[test]
    fn the_panels_album_hearts_become_album_stars() {
        let dir = std::env::temp_dir().join(format!("cratarr-hearts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("u.db");
        let _ = std::fs::remove_file(&file);
        let u = UserData::open(Some(&file)).unwrap();
        u.set_starred("me", Kind::Song, &["al-x".into(), "tr-1".into()], true).unwrap();
        u.set_starred("me", Kind::Song, &["al-y".into()], true).unwrap();
        u.set_starred("me", Kind::Album, &["al-y".into()], true).unwrap();
        drop(u);
        let u = UserData::open(Some(&file)).unwrap();
        let mut albums = u.starred("me", Kind::Album).unwrap();
        albums.sort();
        assert_eq!(albums, ["al-x", "al-y"]);
        assert_eq!(u.starred("me", Kind::Song).unwrap(), ["tr-1"]);
        drop(u);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn data_follows_a_song_to_its_new_id() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-x", &[], 100).unwrap();
        assert_eq!(u.follow_pids(&[("pid1".into(), "tr-9".into())]).unwrap(), 1);
        assert_eq!(u.get("me", Kind::Song, &["tr-9".into()]).unwrap()["tr-9"].play_count, 1);
    }

    #[test]
    fn a_track_in_two_folders_keeps_its_data_on_the_first() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-x", &[], 100).unwrap();
        let both = [("pid1".to_string(), "tr-5".to_string()), ("pid1".to_string(), "tr-8".to_string())];
        assert_eq!(u.follow_pids(&both).unwrap(), 1, "moved once, to the first");
        assert_eq!(u.get("me", Kind::Song, &["tr-5".into()]).unwrap()["tr-5"].play_count, 1);
        assert_eq!(u.follow_pids(&both).unwrap(), 0, "and it stays there");
    }

    const KEY: ImportKey = ImportKey { account: "me", source: "jellyfin", server_user: "u1" };

    #[test]
    fn imported_plays_add_to_the_song_its_album_and_artists_and_are_remembered() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "p"), "al-x", &["ar-a".into()], 500).unwrap();
        let change = ImportChange {
            kind: Kind::Song,
            item: "tr-1".into(),
            pid: Some("p".into()),
            add_plays: 7,
            played: Some(100),
            album: Some("al-x".into()),
            artists: vec!["ar-a".into()],
            star: Some((true, Some(5))),
            rating: Some(4),
            now: Imported { server_item: Some("s1".into()), plays: 7, played: Some(100), starred: true, rating: 4 },
        };
        u.apply_import(KEY, &[change.clone()]).unwrap();
        let a = &u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"];
        assert_eq!((a.play_count, a.play_date, a.starred_at, a.rating), (8, Some(500), Some(5), 4), "added to Cratarr's own play");
        assert_eq!((a.own_count, a.own_date), (8, Some(500)), "imported plays are one's own");
        assert_eq!(u.get("me", Kind::Album, &["al-x".into()]).unwrap()["al-x"].play_count, 8);
        let artist = &u.get("me", Kind::Artist, &["ar-a".into()]).unwrap()["ar-a"];
        assert_eq!((artist.play_count, artist.own_count), (8, 8));
        assert_eq!(u.imported(KEY, Kind::Song, &["tr-1".into(), "tr-2".into()]).unwrap(), [("tr-1".to_string(), change.now.clone())].into());
        assert_eq!(u.imported_starred(KEY, Kind::Song).unwrap(), vec![("tr-1".to_string(), change.now.clone())]);
        let other = ImportKey { account: "you", ..KEY };
        assert!(u.imported(other, Kind::Song, &["tr-1".into()]).unwrap().is_empty(), "per Cratarr account");
        // Unstarring takes the star and its time away; a later star gets the time given.
        let off = ImportChange { star: Some((false, None)), add_plays: 0, played: None, rating: None, ..change.clone() };
        u.apply_import(KEY, &[off]).unwrap();
        assert_eq!(u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"].starred_at, None);
        // The record follows the song to a new id with its annotation.
        u.follow_pids(&[("p".into(), "tr-9".into())]).unwrap();
        assert!(u.imported(KEY, Kind::Song, &["tr-9".into()]).unwrap().contains_key("tr-9"));
        assert_eq!(u.get("me", Kind::Song, &["tr-9".into()]).unwrap()["tr-9"].play_count, 8);
    }

    #[test]
    fn an_import_undone_takes_back_its_plays_stars_and_ratings_where_they_are_still_its_own() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "p"), "al-x", &["ar-a".into()], 500).unwrap();
        let change = |item: &str, star: bool, rating: u8| ImportChange {
            kind: Kind::Song,
            item: item.into(),
            pid: None,
            add_plays: 7,
            played: Some(100),
            album: Some("al-x".into()),
            artists: vec!["ar-a".into()],
            star: Some((star, Some(5))),
            rating: Some(rating),
            now: Imported { server_item: Some("s".into()), plays: 7, played: Some(100), starred: star, rating },
        };
        u.apply_import(KEY, &[change("tr-1", true, 4), change("tr-2", true, 3)]).unwrap();
        // Since the import: tr-2's rating changed in Cratarr, so it stays.
        u.apply_import(KEY, &[ImportChange { add_plays: 0, played: None, star: None, rating: Some(5), now: Imported { rating: 3, ..change("tr-2", true, 3).now }, ..change("tr-2", true, 3) }]).unwrap();
        let undone = u.undo_import(KEY, |_| Some(("al-x".into(), vec!["ar-a".into()]))).unwrap();
        assert_eq!(undone, Undone { items: 2, plays: 14, unstarred: 2, unrated: 1 });
        let get = |kind, item: &str| u.get("me", kind, &[item.to_string()]).unwrap().remove(item).unwrap_or_default();
        assert_eq!((get(Kind::Song, "tr-1").play_count, get(Kind::Song, "tr-1").rating), (1, 0), "Cratarr's own play stays");
        assert_eq!(get(Kind::Song, "tr-1").own_count, 1, "and as one's own");
        assert_eq!(get(Kind::Song, "tr-2").rating, 5, "a rating changed since is the user's");
        assert_eq!((get(Kind::Album, "al-x").play_count, get(Kind::Artist, "ar-a").play_count), (1, 1));
        assert_eq!((get(Kind::Album, "al-x").own_count, get(Kind::Artist, "ar-a").own_count), (1, 1));
        assert!(get(Kind::Song, "tr-1").starred_at.is_none());
        assert!(u.imported(KEY, Kind::Song, &["tr-1".into()]).unwrap().is_empty(), "forgotten: the next import starts afresh");
    }

    #[test]
    fn requests_keep_who_asked_their_status_and_who_answered() {
        let u = UserData::open(None).unwrap();
        let r = |user: &str, album: &str, at: i64| Request {
            id: 0,
            user: user.into(),
            album_key: format!("rg-{album}"),
            artist_key: "air".into(),
            release_group: Some(format!("rg-{album}")),
            album: album.into(),
            artist: "Air".into(),
            year: Some(1998),
            cover: None,
            at,
            status: "pending".into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
        };
        let a = u.add_request(&r("me", "Moon Safari", 1)).unwrap();
        let b = u.add_request(&r("you", "Talkie Walkie", 2)).unwrap();
        assert_eq!(u.requests_waiting().unwrap(), 2);
        assert_eq!(u.requests(None, None, 10).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), vec![b, a], "newest first");
        assert_eq!(u.requests(Some("me"), None, 10).unwrap().len(), 1, "each account its own");
        u.set_request_status(b, "declined", Some("Not on Soulseek"), Some("admin")).unwrap();
        let d = u.request(b).unwrap().unwrap();
        assert_eq!((d.status.as_str(), d.reason.as_deref(), d.decided_by.as_deref()), ("declined", Some("Not on Soulseek"), Some("admin")));
        assert!(d.decided_at.is_some());
        assert_eq!(u.requests(None, Some("pending"), 10).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), vec![a]);
        u.set_request_fetched(a, "Lidarr is looking for it").unwrap();
        assert_eq!(u.request(a).unwrap().unwrap().fetched.as_deref(), Some("Lidarr is looking for it"));
        assert!(u.request(99).unwrap().is_none());
    }

    #[test]
    fn discover_keeps_its_answers_and_leaves_out_an_artist_said_no_to_twice() {
        let u = UserData::open(None).unwrap();
        u.answer_discover("me", "rg-1", "bonobo", "no").unwrap();
        u.name_discover_answer("me", "rg-1", "Black Sands", "Bonobo", Some(2010), None).unwrap();
        let kept = u.discover_exclusions("me").unwrap();
        assert_eq!((kept[0].name.as_deref(), kept[0].artist.as_deref(), kept[0].year), (Some("Black Sands"), Some("Bonobo"), Some(2010)), "its names kept with it");
        u.answer_discover("me", "rg-2", "air", "wanted").unwrap();
        let (albums, artists) = u.discover_answered("me").unwrap();
        assert!(albums.contains("rg-1") && albums.contains("rg-2"));
        assert!(artists.is_empty(), "one no is not the artist's");
        u.answer_discover("me", "rg-3", "bonobo", "no").unwrap();
        u.answer_discover("me", "rg-4", "air", "wanted").unwrap();
        let (_, artists) = u.discover_answered("me").unwrap();
        assert_eq!(artists.into_iter().collect::<Vec<_>>(), ["bonobo"], "wanted is no no");
        assert!(u.discover_answered("you").unwrap().0.is_empty(), "each account its own");
        u.set_discover_found("me", 5, "{}").unwrap();
        assert_eq!(u.discover_found("me").unwrap(), Some((5, "{}".into())));
    }

    #[test]
    fn discover_leaves_out_an_artist_said_no_to_as_an_artist_at_once() {
        let u = UserData::open(None).unwrap();
        u.answer_discover("me", "artist:air", "air", "artist").unwrap();
        let (albums, artists) = u.discover_answered("me").unwrap();
        assert!(artists.contains("air"), "once is enough");
        assert!(albums.contains("artist:air"));
        assert_eq!(u.discover_exclusions("me").unwrap().len(), 1, "listed with the exclusions");
        u.unanswer_discover_artist("me", "air").unwrap();
        assert!(u.discover_answered("me").unwrap().1.is_empty(), "taken back");
    }

    #[test]
    fn an_artist_excluded_everywhere_is_kept_once_per_account_apart_from_discovers_no() {
        // Decision 0083.
        let u = UserData::open(None).unwrap();
        assert!(u.exclude_artist("me", "air", "Air").unwrap(), "new");
        assert!(!u.exclude_artist("me", "air", "AIR").unwrap(), "once");
        let listed = u.excluded_artist_list("me").unwrap();
        assert_eq!(listed.iter().map(|a| (a.key.as_str(), a.name.as_str())).collect::<Vec<_>>(), [("air", "AIR")], "the name last said");
        assert!(u.excluded_artists("you").unwrap().is_empty(), "each account its own");
        // Discover's Not interested in an artist stays Discover's, and the everywhere list is not Discover's.
        u.answer_discover("me", "artist:bonobo", "bonobo", "artist").unwrap();
        assert_eq!(u.excluded_artists("me").unwrap().into_iter().collect::<Vec<_>>(), ["air"]);
        assert_eq!(u.discover_answered("me").unwrap().1.into_iter().collect::<Vec<_>>(), ["bonobo"]);
        assert_eq!(u.discover_exclusions("me").unwrap().len(), 1);
        assert!(u.include_artist("me", "air").unwrap(), "taken back");
        assert!(!u.include_artist("me", "air").unwrap(), "gone already");
        assert!(u.excluded_artists("me").unwrap().is_empty());
        assert_eq!(u.discover_answered("me").unwrap().1.len(), 1, "Discover's untouched");
    }

    #[test]
    fn a_user_data_file_from_before_gets_the_everywhere_list() {
        let dir = std::env::temp_dir().join(format!("cratarr-excluded-artists-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("u.db");
        let _ = std::fs::remove_file(&file);
        // A file as one was before the list (the annotation table only).
        let old = Connection::open(&file).unwrap();
        old.execute_batch(
            "CREATE TABLE annotation (
               user TEXT NOT NULL, item TEXT NOT NULL, kind TEXT NOT NULL, pid TEXT,
               play_count INTEGER NOT NULL DEFAULT 0, play_date INTEGER, rating INTEGER NOT NULL DEFAULT 0,
               starred INTEGER NOT NULL DEFAULT 0, starred_at INTEGER,
               PRIMARY KEY (user, item, kind)) WITHOUT ROWID;",
        )
        .unwrap();
        let had = |c: &Connection| c.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'excluded_artist'").unwrap().exists([]).unwrap();
        assert!(!had(&old));
        drop(old);
        let u = UserData::open(Some(&file)).unwrap();
        assert!(had(&u.db()), "the table, empty");
        assert!(u.excluded_artists("me").unwrap().is_empty());
        u.exclude_artist("me", "beck", "Beck").unwrap();
        drop(u);
        // Opened again: kept, nothing made twice.
        let u = UserData::open(Some(&file)).unwrap();
        assert_eq!(u.excluded_artist_list("me").unwrap().len(), 1);
        drop(u);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_counted_play_is_queued_for_each_service_and_an_imported_listen_never() {
        let u = UserData::open(None).unwrap();
        u.set_scrobble_target("me", "listenbrainz", "me-lb", "{}").unwrap();
        u.set_scrobble_target("me", "lastfm", "me-fm", "{}").unwrap();
        u.count_play("me", ("tr-1", "p"), "al-x", &[], 100).unwrap();
        u.count_play("you", ("tr-1", "p"), "al-x", &[], 100).unwrap();
        let imported = Heard { counts: false, context: Some("lastfm".into()), event: Some("e".into()), ..Heard::counted(200) };
        u.record("me", ("tr-2", "p2"), "al-x", &[], &imported).unwrap();
        let due = u.due_scrobbles(1_000, 10).unwrap();
        let mut got: Vec<(String, String)> = due.iter().map(|q| (q.target.clone(), q.song.clone())).collect();
        got.sort();
        assert_eq!(got, [("lastfm".to_string(), "tr-1".to_string()), ("listenbrainz".to_string(), "tr-1".to_string())]);
        assert!(due.iter().all(|q| q.user == "me" && q.song == "tr-1"), "the other user has no service; the import is never sent");
        // Failing waits; a refused login stops the service until it is connected again.
        let fm: Vec<i64> = due.iter().filter(|q| q.target == "lastfm").map(|q| q.id).collect();
        u.scrobbles_wait(&fm, 5_000, "offline").unwrap();
        assert_eq!(u.due_scrobbles(1_000, 10).unwrap().len(), 1, "lastfm waits until 5000");
        u.scrobble_broken("me", "listenbrainz", "token refused").unwrap();
        assert!(u.due_scrobbles(10_000, 10).unwrap().iter().all(|q| q.target == "lastfm"), "a stopped service sends nothing");
        let t = u.scrobble_targets("me").unwrap();
        assert_eq!((t[0].target.as_str(), t[0].queued, t[0].waiting.as_deref()), ("lastfm", 1, Some("offline")));
        assert_eq!(t[1].broken.as_deref(), Some("token refused"));
        u.set_scrobble_target("me", "listenbrainz", "me-lb", "{}").unwrap();
        assert_eq!(u.due_scrobbles(1_000, 10).unwrap().len(), 1, "connected again: what waited goes");
        u.scrobbled(&u.due_scrobbles(10_000, 10).unwrap().iter().map(|q| q.id).collect::<Vec<_>>()).unwrap();
        assert!(u.due_scrobbles(10_000, 10).unwrap().is_empty());
        u.forget_scrobble_target("me", "lastfm").unwrap();
        assert_eq!(u.scrobble_targets("me").unwrap().len(), 1);
    }

    #[test]
    fn bookmarks_are_kept_per_song_and_follow_it() {
        let u = UserData::open(None).unwrap();
        u.set_bookmark("me", "tr-1", Some("p1"), 135_000, "", Some("Symfonium")).unwrap();
        u.set_bookmark("me", "tr-1", None, 140_000, "chorus", None).unwrap();
        u.set_bookmark("you", "tr-2", Some("p2"), 5, "", None).unwrap();
        let mine = u.bookmarks("me").unwrap();
        assert_eq!(mine.len(), 1, "one per song: the second replaced the first");
        assert_eq!((mine[0].position_ms, mine[0].comment.as_str()), (140_000, "chorus"));
        u.follow_pids(&[("p1".into(), "tr-9".into())]).unwrap();
        assert_eq!(u.bookmarks("me").unwrap()[0].item, "tr-9", "kept its persistent id through the update");
        u.delete_bookmark("me", "tr-9").unwrap();
        assert!(u.bookmarks("me").unwrap().is_empty());
        assert_eq!(u.bookmarks("you").unwrap().len(), 1);
    }

    #[test]
    fn a_renamed_playlist_is_found_by_its_old_names() {
        let u = UserData::open(None).unwrap();
        u.playlist_renamed("Test", "Testttt").unwrap();
        u.playlist_renamed("Testttt", "Trip").unwrap();
        assert_eq!(u.playlist_alias("Test").unwrap().as_deref(), Some("Trip"), "a chain leads to the name now");
        assert_eq!(u.playlist_alias("Testttt").unwrap().as_deref(), Some("Trip"));
        u.playlist_renamed("Trip", "Test").unwrap();
        assert_eq!(u.playlist_alias("Test").unwrap(), None, "its own name again");
        assert_eq!(u.playlist_alias("Trip").unwrap().as_deref(), Some("Test"));
        u.playlist_made("Trip").unwrap();
        assert_eq!(u.playlist_alias("Trip").unwrap(), None, "a new playlist takes the name back");
        u.playlist_gone("Test").unwrap();
        assert_eq!(u.playlist_alias("Testttt").unwrap(), None, "gone with it");
    }

    #[test]
    fn a_queue_kept_before_moves_to_its_app() {
        let dir = std::env::temp_dir().join(format!("cratarr-userdata-queue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("userdata.sqlite");
        {
            let conn = Connection::open(&file).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO play_queue (user, songs, current, position_ms, changed, changed_by) VALUES ('me', 'tr-1,tr-2', 'tr-2', 1500, 9, 'Feishin')",
                [],
            )
            .unwrap();
        }
        let u = UserData::open(Some(&file)).unwrap();
        let moved = AppQueue { client: "Feishin".into(), songs: vec!["tr-1".into(), "tr-2".into()], current: Some(1), position_ms: 1500, changed: 9 };
        assert_eq!(u.app_queues("me").unwrap(), vec![moved.clone()]);
        drop(u);
        assert_eq!(UserData::open(Some(&file)).unwrap().app_queues("me").unwrap(), vec![moved], "moved once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_app_keeps_its_own_play_queue() {
        let u = UserData::open(None).unwrap();
        let q = |client: &str, current| AppQueue {
            client: client.into(),
            songs: vec!["tr-1".into(), "tr-2".into(), "tr-1".into()],
            current,
            position_ms: 1500,
            changed: 9,
        };
        u.save_app_queue("me", &q("Feishin", Some(2))).unwrap();
        u.save_app_queue("me", &q("DSub", Some(0))).unwrap();
        u.save_app_queue("me", &q("Feishin", Some(1))).unwrap();
        let mut got = u.app_queues("me").unwrap();
        got.sort_by(|a, b| a.client.cmp(&b.client));
        assert_eq!(got, vec![q("DSub", Some(0)), q("Feishin", Some(1))], "one per app, the last save kept");
        assert!(u.app_queues("you").unwrap().is_empty());
        u.save_app_queue("me", &AppQueue { client: "DSub".into(), ..AppQueue::default() }).unwrap();
        assert_eq!(u.app_queues("me").unwrap(), vec![q("Feishin", Some(1))], "a save without songs clears");
        u.set_added("al-x", 200, "jellyfin").unwrap();
        u.set_added("al-x", 100, "jellyfin").unwrap();
        u.set_added("al-x", 300, "jellyfin").unwrap();
        assert_eq!(u.added().unwrap()["al-x"], 100, "the earliest stays");
        assert_eq!(u.import_choice("jellyfin").unwrap(), None);
        u.set_import_choice("jellyfin", "{}").unwrap();
        assert_eq!(u.import_choice("jellyfin").unwrap().as_deref(), Some("{}"));
    }

    #[test]
    fn snapshots_are_kept_per_user_newest_first() {
        let u = UserData::open(None).unwrap();
        let settings = serde_json::json!({ "v": 1, "flash": 0.5, "routes": { "beat|-|size": 0.08 } });
        let a = u.add_snapshot("me", "Oyster Cove", &settings, Some("data:image/jpeg;base64,AAAA")).unwrap();
        let b = u.add_snapshot("me", "Puds", &serde_json::json!({ "v": 1 }), None).unwrap();
        u.add_snapshot("you", "Theirs", &settings, None).unwrap();
        assert_ne!(a.id, b.id);
        let mine = u.snapshots("me", None).unwrap();
        assert_eq!(mine.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Puds", "Oyster Cove"], "the newest first, only mine");
        assert_eq!(mine[1].settings, settings, "kept as sent");
        assert_eq!(mine[1].picture.as_deref(), Some("data:image/jpeg;base64,AAAA"));
        assert_eq!(mine[0].picture, None);
        let renamed = u.change_snapshot("me", &a.id, Some("Golden hour"), None, None).unwrap().unwrap();
        assert_eq!((renamed.name.as_str(), renamed.created), ("Golden hour", a.created));
        assert_eq!(renamed.picture.as_deref(), Some("data:image/jpeg;base64,AAAA"), "a rename keeps the picture");
        let pictured = u.change_snapshot("me", &b.id, None, Some("data:image/png;base64,BBBB"), None).unwrap().unwrap();
        assert_eq!((pictured.name.as_str(), pictured.picture.as_deref()), ("Puds", Some("data:image/png;base64,BBBB")));
        assert!(u.change_snapshot("you", &a.id, Some("Mine now"), None, None).unwrap().is_none(), "another user's is not theirs to rename");
        assert!(!u.delete_snapshot("you", &a.id).unwrap(), "another user's is not theirs to delete");
        let over = serde_json::json!({ "v": 2, "flash": 1.2 });
        let saved = u.change_snapshot("me", &b.id, None, Some("data:image/png;base64,DDDD"), Some(&over)).unwrap().unwrap();
        assert_eq!((saved.name.as_str(), saved.created, &saved.settings), ("Puds", b.created, &over), "saved over: its name and time kept");
        assert_eq!(saved.picture.as_deref(), Some("data:image/png;base64,DDDD"));
        assert!(u.change_snapshot("you", &b.id, None, None, Some(&over)).unwrap().is_none(), "another user's is not theirs to save over");
        assert!(u.delete_snapshot("me", &a.id).unwrap());
        assert_eq!(u.snapshots("me", None).unwrap().len(), 1);
    }

    #[test]
    fn the_owners_snapshots_are_every_accounts_library() {
        let u = UserData::open(None).unwrap();
        u.add_snapshot("owner", "Preset", &serde_json::json!({ "v": 4 }), None).unwrap();
        u.add_snapshot("guest", "Own", &serde_json::json!({ "v": 4 }), None).unwrap();
        let guest = u.snapshots("guest", Some("owner")).unwrap();
        assert_eq!(guest.iter().map(|s| (s.name.as_str(), s.shared)).collect::<Vec<_>>(), [("Own", false), ("Preset", true)], "its own first, then the owner's, marked shared");
        let owner = u.snapshots("owner", Some("owner")).unwrap();
        assert_eq!(owner.iter().map(|s| (s.name.as_str(), s.shared)).collect::<Vec<_>>(), [("Preset", false)], "the owner's own, not shared to itself");
        assert!(u.change_snapshot("guest", &guest[1].id, Some("Mine now"), None, None).unwrap().is_none(), "the owner's stays the owner's");
        assert!(!serde_json::to_string(&owner[0]).unwrap().contains("shared"), "only a shared one says so");
    }

    #[test]
    fn snapshots_saved_before_pictures_get_the_column() {
        let file = std::env::temp_dir().join(format!("cratarr-snapshots-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&file);
        {
            let c = Connection::open(&file).unwrap();
            c.execute_batch(
                "CREATE TABLE visual_snapshot (user TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, settings TEXT NOT NULL,
                   created INTEGER NOT NULL, PRIMARY KEY (user, id));
                 INSERT INTO visual_snapshot VALUES ('me', 'a1', 'Kartlav', '{\"v\":1}', 5);",
            )
            .unwrap();
        }
        let u = UserData::open(Some(&file)).unwrap();
        let mine = u.snapshots("me", None).unwrap();
        assert_eq!((mine[0].name.as_str(), mine[0].picture.as_deref()), ("Kartlav", None));
        assert!(u.change_snapshot("me", "a1", None, Some("data:image/jpeg;base64,CC"), None).unwrap().unwrap().picture.is_some());
        drop(u);
        UserData::open(Some(&file)).unwrap();
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn thumbs_are_kept_per_user_in_order_as_sent() {
        let u = UserData::open(None).unwrap();
        let body = |n: i64| serde_json::json!({ "relative": { "motion": 0.6 }, "changed": null, "n": n }).as_object().unwrap().clone();
        let first = u.add_thumb("me", "down", &body(1)).unwrap();
        u.add_thumb("you", "up", &body(9)).unwrap();
        u.add_thumb("me", "up", &body(2)).unwrap();
        let mine = u.thumbs("me").unwrap();
        assert_eq!(mine.iter().map(|t| (t.thumb.as_str(), t.body["n"].as_i64().unwrap())).collect::<Vec<_>>(), [("down", 1), ("up", 2)]);
        assert_eq!(mine[0], first);
        assert!(mine[0].at > 1_700_000_000_000, "the server's time, in milliseconds");
        let sent = serde_json::to_value(&mine[0]).unwrap();
        assert_eq!((sent["thumb"].as_str(), sent["n"].as_i64(), sent["relative"]["motion"].as_f64()), (Some("down"), Some(1), Some(0.6)), "flat, as sent");
    }

    #[test]
    fn learned_limits_keep_a_version_and_refuse_a_stale_one() {
        let u = UserData::open(None).unwrap();
        let limits = serde_json::json!({ "motion": { "value": 0.57, "step": 0.1, "dir": -1 } });
        assert_eq!(u.learned("me").unwrap(), None);
        assert_eq!(u.put_learned("me", 5, &limits, 1).unwrap(), Err(Stale), "nothing kept: only base 0 keeps");
        let first = u.put_learned("me", 0, &limits, 1).unwrap().unwrap();
        assert!(first > 1_700_000_000_000, "versions from the clock, so a restart never goes back");
        assert_eq!(u.put_learned("me", 0, &limits, 2).unwrap(), Err(Stale), "another device kept them meanwhile");
        let looser = serde_json::json!({ "motion": { "value": 0.6, "step": 0.05, "dir": 1 } });
        let second = u.put_learned("me", first, &looser, 3).unwrap().unwrap();
        assert!(second > first);
        assert_eq!(u.learned("me").unwrap(), Some(Learned { version: second, limits: looser, at: 3 }));
        assert_eq!(u.learned("you").unwrap(), None, "each user's own");
        u.forget_learned("me").unwrap();
        assert_eq!(u.learned("me").unwrap(), None);
        assert!(u.put_learned("me", 0, &limits, 4).unwrap().is_ok(), "forgotten: base 0 keeps again");
    }

    #[test]
    fn ratings_are_kept_per_user_in_order() {
        let u = UserData::open(None).unwrap();
        let r = |at: i64, choice: &str| Rating {
            at,
            a: serde_json::json!({ "seed": 1 }),
            b: serde_json::json!({ "seed": 2 }),
            choice: choice.to_string(),
            track: Some("cratarr/tr-1".into()),
            context: serde_json::json!({ "secs": 20 }),
        };
        u.add_rating("me", &r(2, "b")).unwrap();
        u.add_rating("me", &r(1, "a")).unwrap();
        u.add_rating("you", &r(3, "same")).unwrap();
        let mine = u.ratings("me").unwrap();
        assert_eq!(mine.iter().map(|x| (x.at, x.choice.as_str())).collect::<Vec<_>>(), [(1, "a"), (2, "b")]);
        assert_eq!(mine[0].a, serde_json::json!({ "seed": 1 }));
        assert_eq!(mine[0].context, serde_json::json!({ "secs": 20 }));
    }
}
