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
//! their persistent id; the play queue holds song ids as text.

#![allow(dead_code)] // the endpoints that use it come next (decision 0030, plan step 2)

pub mod tags;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
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
";

/// What an annotation is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Song,
    Album,
    Artist,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Song => "song",
            Kind::Album => "album",
            Kind::Artist => "artist",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotation {
    pub play_count: u32,
    /// Epoch seconds.
    pub play_date: Option<i64>,
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
pub struct PlayQueue {
    pub songs: Vec<String>,
    pub current: Option<String>,
    pub position_ms: u64,
    /// Epoch seconds.
    pub changed: i64,
    pub changed_by: Option<String>,
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
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
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
        let conn = match file {
            Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
            None => Connection::open_in_memory()?,
        };
        conn.busy_timeout(Duration::from_secs(15))?;
        conn.execute_batch(SCHEMA).context("creating the user data tables")?;
        conn.execute_batch(tags::SCHEMA).context("creating the album tag table")?;
        Ok(Self { db: Mutex::new(conn) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The annotations of several items of one kind, by item; items without any are left out.
    pub fn get(&self, user: &str, kind: Kind, items: &[String]) -> Result<std::collections::HashMap<String, Annotation>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT play_count, play_date, rating, starred, starred_at FROM annotation WHERE user = ?1 AND kind = ?2 AND item = ?3",
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
        Ok(())
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
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, play_count, play_date) VALUES (?1, ?2, ?3, ?4, 1, ?5)
                 ON CONFLICT (user, item, kind) DO UPDATE SET play_count = annotation.play_count + 1,
                   play_date = max(coalesce(annotation.play_date, 0), excluded.play_date),
                   pid = coalesce(excluded.pid, annotation.pid)",
            )?;
            st.execute(params![user, song.0, Kind::Song.as_str(), song.1, at])?;
            st.execute(params![user, album, Kind::Album.as_str(), Option::<String>::None, at])?;
            for a in artists {
                st.execute(params![user, a, Kind::Artist.as_str(), Option::<String>::None, at])?;
            }
        }
        tx.commit()?;
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
    pub fn apply_import(&self, key: ImportKey, changes: &[ImportChange]) -> Result<()> {
        let user = key.account;
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut plays = tx.prepare_cached(
                "INSERT INTO annotation (user, item, kind, pid, play_count, play_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (user, item, kind) DO UPDATE SET play_count = annotation.play_count + excluded.play_count,
                   play_date = CASE WHEN excluded.play_date IS NULL THEN annotation.play_date
                     ELSE max(coalesce(annotation.play_date, 0), excluded.play_date) END,
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
    pub fn import_choice(&self, source: &str) -> Result<Option<String>> {
        Ok(self.db().query_row("SELECT choice FROM import_choices WHERE source = ?1", [source], |r| r.get(0)).optional()?)
    }

    pub fn set_import_choice(&self, source: &str, choice: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO import_choices (source, choice) VALUES (?1, ?2)", params![source, choice])?;
        Ok(())
    }

    /// Songs' annotations, and what imports brought in for them, follow their persistent ids to new song ids (the
    /// catalogue was rebuilt): `moves` is (persistent id, song id now). Returns how many annotations moved.
    pub fn follow_pids(&self, moves: &[(String, String)]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare_cached("UPDATE OR IGNORE annotation SET item = ?2 WHERE kind = 'song' AND pid = ?1 AND item != ?2")?;
            let mut ledger = tx.prepare_cached("UPDATE OR IGNORE imported SET item = ?2 WHERE kind = 'song' AND pid = ?1 AND item != ?2")?;
            for (pid, item) in moves {
                n += st.execute(params![pid, item])?;
                ledger.execute(params![pid, item])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    pub fn play_queue(&self, user: &str) -> Result<Option<PlayQueue>> {
        let db = self.db();
        db.query_row(
            "SELECT songs, current, position_ms, changed, changed_by FROM play_queue WHERE user = ?1",
            [user],
            |r| {
                let songs: String = r.get(0)?;
                Ok(PlayQueue {
                    songs: songs.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                    current: r.get(1)?,
                    position_ms: r.get::<_, i64>(2)?.max(0) as u64,
                    changed: r.get(3)?,
                    changed_by: r.get(4)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn save_play_queue(&self, user: &str, q: &PlayQueue) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO play_queue (user, songs, current, position_ms, changed, changed_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![user, q.songs.join(","), q.current, q.position_ms as i64, q.changed, q.changed_by],
        )?;
        Ok(())
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

    pub fn added(&self) -> Result<std::collections::HashMap<String, i64>> {
        let db = self.db();
        let mut st = db.prepare("SELECT album, at FROM added")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn data_follows_a_song_to_its_new_id() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "pid1"), "al-x", &[], 100).unwrap();
        assert_eq!(u.follow_pids(&[("pid1".into(), "tr-9".into())]).unwrap(), 1);
        assert_eq!(u.get("me", Kind::Song, &["tr-9".into()]).unwrap()["tr-9"].play_count, 1);
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
        assert_eq!(u.get("me", Kind::Album, &["al-x".into()]).unwrap()["al-x"].play_count, 8);
        assert_eq!(u.get("me", Kind::Artist, &["ar-a".into()]).unwrap()["ar-a"].play_count, 8);
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
    fn the_play_queue_round_trips() {
        let u = UserData::open(None).unwrap();
        let q = PlayQueue { songs: vec!["tr-1".into(), "tr-2".into()], current: Some("tr-2".into()), position_ms: 1500, changed: 9, changed_by: Some("Feishin".into()) };
        u.save_play_queue("me", &q).unwrap();
        assert_eq!(u.play_queue("me").unwrap(), Some(q));
        u.set_added("al-x", 200, "jellyfin").unwrap();
        u.set_added("al-x", 100, "jellyfin").unwrap();
        u.set_added("al-x", 300, "jellyfin").unwrap();
        assert_eq!(u.added().unwrap()["al-x"], 100, "the earliest stays");
        assert_eq!(u.import_choice("jellyfin").unwrap(), None);
        u.set_import_choice("jellyfin", "{}").unwrap();
        assert_eq!(u.import_choice("jellyfin").unwrap().as_deref(), Some("{}"));
    }
}
