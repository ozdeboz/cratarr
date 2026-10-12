//! The journal (decision 0011 §6): every event is written here before anything reacts to it. An entry holds the
//! event, when it happened, which handlers are done (with what they returned, so their work can be undone), and
//! whether it was undone. That makes each handler safe to run twice, lets handling resume after a restart, keeps
//! Undo working across restarts, and answers "what happened here". One small SQLite file in the data folder (in
//! memory without one).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub type EventId = i64;

/// What happened. An album action is one event carrying everything needed to react to it and to undo it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// An Incoming album was deleted: moved into the trash inside its own folder.
    AlbumRejected {
        source: String,
        album_id: String,
        title: String,
        /// Catalogue folder id, and the album folder relative to it.
        folder: i64,
        dir: String,
        /// Restores it from the trash.
        token: String,
        release_group: Option<String>,
    },
    /// Keep (decision 0013): an Incoming album moved into Music, renamed per the library's template. Recorded after
    /// the catalogue rows moved with the files (track ids kept).
    AlbumAdopted {
        source: String,
        album_id: String,
        title: String,
        /// The Incoming album folder and its new Music folder: (catalogue folder, relative path).
        from_dir: (i64, String),
        to_dir: (i64, String),
        /// Every file, audio and others.
        moves: Vec<TrackMove>,
        release_group: Option<String>,
    },
    /// Replace (decision 0013): an upgrade took the place of an album in Music. Every old track had a match (else
    /// Replace is refused). Recorded after the catalogue rows moved and the ids were handed on.
    AlbumReplaced {
        source: String,
        album_id: String,
        title: String,
        from_dir: (i64, String),
        to_dir: (i64, String),
        /// The new files, Incoming → Music; `track` is the id each inherited from its old track (or a fresh one for a
        /// track the old album did not have).
        moves: Vec<TrackMove>,
        /// The old files, Music → the Incoming trash; `track` is the id each handed on.
        replaced: Vec<TrackMove>,
        /// Puts the old files back on Undo.
        token: String,
        release_group: Option<String>,
    },
}

impl Event {
    /// The album's title and MusicBrainz release group, whatever happened to it.
    pub fn album(&self) -> (&str, Option<&str>) {
        match self {
            Event::AlbumRejected { title, release_group, .. }
            | Event::AlbumAdopted { title, release_group, .. }
            | Event::AlbumReplaced { title, release_group, .. } => (title, release_group.as_deref()),
        }
    }
}

/// One file of an album action: where it was and where it went, as (catalogue folder, relative path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMove {
    pub track: Option<i64>,
    pub from: (i64, String),
    pub to: (i64, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: EventId,
    /// Epoch seconds.
    pub at: i64,
    pub event: Event,
    pub undone: bool,
}

// temp_store: the image has no /tmp.
const SCHEMA: &str = "
PRAGMA temp_store = MEMORY;
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, event TEXT NOT NULL, undone_at INTEGER);
CREATE TABLE IF NOT EXISTS handled (
  event INTEGER NOT NULL, handler TEXT NOT NULL, at INTEGER NOT NULL, result TEXT NOT NULL,
  PRIMARY KEY (event, handler)) WITHOUT ROWID;
";

pub struct Journal {
    db: Mutex<Connection>,
    /// New events, for the handlers listening (`subscribe`).
    tell: tokio::sync::broadcast::Sender<EventId>,
}

impl Journal {
    /// `file`: the journal; `None` keeps it in memory (no data folder).
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = match file {
            Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
            None => Connection::open_in_memory()?,
        };
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA).context("creating the journal")?;
        Ok(Self { db: Mutex::new(conn), tell: tokio::sync::broadcast::channel(64).0 })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Write an event, then tell whoever listens. Returns its id.
    pub fn record(&self, at: i64, event: &Event) -> Result<EventId> {
        let id = {
            let db = self.db();
            db.execute("INSERT INTO events (at, event) VALUES (?1, ?2)", params![at, serde_json::to_string(event)?])?;
            db.last_insert_rowid()
        };
        let _ = self.tell.send(id);
        Ok(id)
    }

    /// New event ids as they are recorded. A listener that falls behind catches up through `pending`.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<EventId> {
        self.tell.subscribe()
    }

    pub fn get(&self, id: EventId) -> Result<Option<Entry>> {
        let row: Option<(i64, String, Option<i64>)> = self
            .db()
            .query_row("SELECT at, event, undone_at FROM events WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        row.map(|(at, event, undone)| Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: undone.is_some() })).transpose()
    }

    /// Record that `handler` is done with an event, and what it returned (what its undo needs).
    pub fn done(&self, id: EventId, handler: &str, at: i64, result: &serde_json::Value) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO handled (event, handler, at, result) VALUES (?1, ?2, ?3, ?4)",
            params![id, handler, at, result.to_string()],
        )?;
        Ok(())
    }

    /// What `handler` returned for an event; `None` when it has not handled it.
    pub fn result(&self, id: EventId, handler: &str) -> Result<Option<serde_json::Value>> {
        let text: Option<String> = self
            .db()
            .query_row("SELECT result FROM handled WHERE event = ?1 AND handler = ?2", params![id, handler], |r| r.get(0))
            .optional()?;
        Ok(text.map(|t| serde_json::from_str(&t)).transpose()?)
    }

    /// Events since `since` (epoch seconds) that `handler` has not handled and that were not undone, oldest first.
    pub fn pending(&self, handler: &str, since: i64) -> Result<Vec<Entry>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT id, at, event FROM events e WHERE at >= ?2 AND undone_at IS NULL
             AND NOT EXISTS (SELECT 1 FROM handled h WHERE h.event = e.id AND h.handler = ?1) ORDER BY id",
        )?;
        let rows = st.query_map(params![handler, since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        rows.map(|r| {
            let (id, at, event) = r?;
            Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: false })
        })
        .collect()
    }

    /// Mark an event undone. False when it already was (so an Undo runs once).
    pub fn mark_undone(&self, id: EventId, at: i64) -> Result<bool> {
        Ok(self.db().execute("UPDATE events SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL", params![id, at])? == 1)
    }

    /// Forget events older than `before` (epoch seconds): nothing can undo or handle them any more.
    pub fn prune(&self, before: i64) -> Result<usize> {
        let db = self.db();
        db.execute("DELETE FROM handled WHERE event IN (SELECT id FROM events WHERE at < ?1)", [before])?;
        Ok(db.execute("DELETE FROM events WHERE at < ?1", [before])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(title: &str) -> Event {
        Event::AlbumRejected {
            source: "jellyfin".into(),
            album_id: "a1".into(),
            title: title.into(),
            folder: 1,
            dir: "X - Y".into(),
            token: "100/X - Y".into(),
            release_group: Some("rg".into()),
        }
    }

    #[test]
    fn events_are_kept_with_what_each_handler_did() {
        let j = Journal::open(None).unwrap();
        let mut rx = j.subscribe();
        let id = j.record(100, &rejected("Album")).unwrap();
        assert_eq!(rx.try_recv().unwrap(), id, "listeners hear of it");
        let e = j.get(id).unwrap().unwrap();
        assert_eq!((e.at, e.undone, e.event), (100, false, rejected("Album")));
        assert_eq!(j.pending("acquisition", 0).unwrap().len(), 1);
        j.done(id, "acquisition", 101, &serde_json::json!([7, 8])).unwrap();
        assert!(j.pending("acquisition", 0).unwrap().is_empty(), "handled once");
        assert_eq!(j.pending("playlists", 0).unwrap().len(), 1, "per handler");
        assert_eq!(j.result(id, "acquisition").unwrap(), Some(serde_json::json!([7, 8])));
        assert_eq!(j.result(id, "playlists").unwrap(), None);
    }

    #[test]
    fn an_event_is_undone_once_and_then_no_longer_pending() {
        let j = Journal::open(None).unwrap();
        let id = j.record(100, &rejected("Album")).unwrap();
        assert!(j.mark_undone(id, 200).unwrap());
        assert!(!j.mark_undone(id, 201).unwrap(), "twice: no");
        assert!(j.get(id).unwrap().unwrap().undone);
        assert!(j.pending("acquisition", 0).unwrap().is_empty());
    }

    #[test]
    fn old_events_are_pruned_and_the_file_survives_a_restart() {
        let d = std::env::temp_dir().join(format!("cratarr-journal-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("journal.sqlite");
        let (old, new) = {
            let j = Journal::open(Some(&file)).unwrap();
            (j.record(100, &rejected("Old")).unwrap(), j.record(5_000, &rejected("New")).unwrap())
        };
        let j = Journal::open(Some(&file)).unwrap();
        assert!(j.get(new).unwrap().is_some(), "kept across a restart");
        assert_eq!(j.prune(1_000).unwrap(), 1);
        assert!(j.get(old).unwrap().is_none());
        assert!(j.pending("acquisition", 1_000).unwrap().iter().all(|e| e.id == new));
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }
}
