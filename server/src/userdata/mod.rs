//! What each user did with the music (decision 0030): plays, last played, ratings, stars, the saved play queue, and
//! when an album was first added where that is known better than the catalogue does (imported from a media server).
//! Its own SQLite file, never derived from anything: it belongs in backups. Nothing here is removed when a file goes
//! missing; a song comes back to its data by its id, or by its persistent id once the catalogue renumbered its tracks.
//!
//! The annotation table is Navidrome's (`db/migrations`, `persistence/sql_annotations.go`,
//! https://github.com/navidrome/navidrome, GPL-3.0-or-later, Copyright the Navidrome authors), translated: one row per
//! user and item with plays, last played, rating and star, unique on (user, item, kind). Changed: songs also keep
//! their persistent id; the play queue holds song ids as text.

#![allow(dead_code)] // the endpoints that use it come next (decision 0030, plan step 2)

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
";

/// What an annotation is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayQueue {
    pub songs: Vec<String>,
    pub current: Option<String>,
    pub position_ms: u64,
    /// Epoch seconds.
    pub changed: i64,
    pub changed_by: Option<String>,
}

pub struct UserData {
    db: Mutex<Connection>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
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

    /// Sets a song's whole annotation (an import from a media server). The persistent id goes along.
    pub fn import(&self, user: &str, kind: Kind, item: &str, pid: Option<&str>, a: &Annotation) -> Result<()> {
        self.db().execute(
            "INSERT INTO annotation (user, item, kind, pid, play_count, play_date, rating, starred, starred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (user, item, kind) DO UPDATE SET pid = coalesce(excluded.pid, annotation.pid),
               play_count = max(annotation.play_count, excluded.play_count),
               play_date = max(coalesce(annotation.play_date, 0), coalesce(excluded.play_date, 0)),
               rating = CASE WHEN annotation.rating = 0 THEN excluded.rating ELSE annotation.rating END,
               starred = max(annotation.starred, excluded.starred),
               starred_at = coalesce(annotation.starred_at, excluded.starred_at)",
            params![
                user,
                item,
                kind.as_str(),
                pid,
                a.play_count,
                a.play_date,
                a.rating,
                a.starred_at.is_some(),
                a.starred_at
            ],
        )?;
        Ok(())
    }

    /// Songs' annotations follow their persistent ids to new song ids (the catalogue was rebuilt): `moves` is
    /// (persistent id, song id now). Returns how many moved.
    pub fn follow_pids(&self, moves: &[(String, String)]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare_cached("UPDATE OR IGNORE annotation SET item = ?2 WHERE kind = 'song' AND pid = ?1 AND item != ?2")?;
            for (pid, item) in moves {
                n += st.execute(params![pid, item])?;
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

    /// When albums were first added, where known better than the catalogue (`origin` says from where).
    pub fn set_added(&self, album: &str, at: i64, origin: &str) -> Result<()> {
        self.db().execute(
            "INSERT INTO added (album, at, origin) VALUES (?1, ?2, ?3) ON CONFLICT (album) DO UPDATE SET at = min(added.at, excluded.at)",
            params![album, at, origin],
        )?;
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

    #[test]
    fn an_import_never_lowers_what_is_there() {
        let u = UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "p"), "al-x", &[], 500).unwrap();
        u.import("me", Kind::Song, "tr-1", Some("p"), &Annotation { play_count: 7, play_date: Some(100), rating: 0, starred_at: Some(5) }).unwrap();
        let a = &u.get("me", Kind::Song, &["tr-1".into()]).unwrap()["tr-1"];
        assert_eq!((a.play_count, a.play_date, a.starred_at), (7, Some(500), Some(5)));
    }

    #[test]
    fn the_play_queue_round_trips() {
        let u = UserData::open(None).unwrap();
        let q = PlayQueue { songs: vec!["tr-1".into(), "tr-2".into()], current: Some("tr-2".into()), position_ms: 1500, changed: 9, changed_by: Some("Feishin".into()) };
        u.save_play_queue("me", &q).unwrap();
        assert_eq!(u.play_queue("me").unwrap(), Some(q));
        u.set_added("al-x", 200, "jellyfin").unwrap();
        u.set_added("al-x", 100, "jellyfin").unwrap();
        assert_eq!(u.added().unwrap()["al-x"], 100);
    }
}
