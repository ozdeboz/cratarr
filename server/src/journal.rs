//! The journal (decision 0011 §6): every event is written here before anything reacts to it. An entry holds the
//! event, when it happened, which handlers are done (with what they returned, so their work can be undone), and
//! whether it was undone. That makes each handler safe to run twice, lets handling resume after a restart, keeps
//! Undo working across restarts, and answers "what happened here". One small SQLite file in the data folder (in
//! memory without one).
//!
//! It is the app's one undo journal (decision 0026): besides the album events, the changes people make to playlists
//! and crates are kept here (`Change`, in a table of their own, so the album handlers never see them), each holding
//! what undoing it needs and a fingerprint of what it left.

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
        /// Hi-res files converted to CD format just before (Undo puts the originals back).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        converted: Vec<Converted>,
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
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        converted: Vec<Converted>,
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
    /// Replace: the track id a new file had before it took over an old one's (given back on Undo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<i64>,
}

/// One hi-res file converted to CD format before Keep or Replace (decision 0013), in the album's Incoming folder
/// (`from_dir.0`): paths relative to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Converted {
    /// Where the original was.
    pub original: String,
    /// Where the original is now: in the trash.
    pub trashed: String,
    /// The converted file, before the action moved it (its `from` in `moves`).
    pub converted: String,
}

/// A change to playlists or crates (decision 0026): what undoing it needs, and what it left (so an undo can tell
/// whether anything changed since). Undoing one is recorded as a change of its own, whose undo is the redo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    /// Playlists' lines changed (an edit, repairs, repeats removed): each one's file before, and what it left.
    Texts { changes: Vec<TextChange> },
    /// New playlists, each with the fingerprint of the file it left.
    Created { playlists: Vec<(String, u64)> },
    /// Deleted playlists (the store keeps each aside).
    Deleted { playlists: Vec<String> },
    Renamed { from: String, to: String },
    /// The names a playlist is also spoken as (decision 0015).
    Spoken { playlist: String, before: Vec<String>, after: Vec<String> },
    /// One of one's own crates (decision 0024): its definition before and after (`None`: it did not exist).
    Crate { before: Option<crate::crates::CrateDef>, after: Option<crate::crates::CrateDef> },
    /// A crate's tracks and albums left out by hand (decision 0024, amended 2026-10-19): its list before and after.
    LeftOut { crate_id: String, before: crate::crates::LeftOut, after: crate::crates::LeftOut },
    /// The tracks excluded from every automatic pick (decision 0024, amended 2026-10-19): their lines before and after.
    Excluded { before: Vec<String>, after: Vec<String> },
    /// The conditions an account added to a fixed crate (decision 0083: Not in this crate), by its kept id: before and
    /// after.
    Added { crate_id: String, before: Vec<crate::crates::Condition>, after: Vec<crate::crates::Condition> },
    /// An artist put on an account's everywhere list (`out`) or taken off it (decision 0083): the account, the artist's
    /// key and name.
    ArtistOut { user: String, key: String, name: String, out: bool },
    /// Tracks' tempo adjusted (Adjust BPM, 2026-10-23): each one's BPM tag before and after.
    Tempo { tracks: Vec<crate::app::tempo::TempoChange> },
}

/// One playlist's file before a change, and the fingerprint of the file the change left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextChange {
    pub playlist: String,
    pub before: String,
    pub left: u64,
}

/// Changes are offered for undo only in the toast after them: forgotten after a day.
pub const CHANGE_SECS: i64 = 24 * 3600;

/// A file's fingerprint (FNV-1a, 64 bits): the same text always gives the same number, across restarts and versions.
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: EventId,
    /// Epoch seconds.
    pub at: i64,
    pub event: Event,
    pub undone: bool,
    /// False while the action is only intended (see `intend`): it may or may not have happened.
    pub applied: bool,
}

// journal_size_limit: as the catalogue's. (Temporary storage in memory: crate::db.)
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, event TEXT NOT NULL, undone_at INTEGER);
CREATE TABLE IF NOT EXISTS handled (
  event INTEGER NOT NULL, handler TEXT NOT NULL, at INTEGER NOT NULL, result TEXT NOT NULL,
  PRIMARY KEY (event, handler)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS changes (
  id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, change TEXT NOT NULL, undone_at INTEGER);
";

/// Columns added after the first schema; added to an existing journal when missing.
const ADDED_COLUMNS: [(&str, &str); 4] =
    [("events", "applied INTEGER NOT NULL DEFAULT 1"), ("changes", "account TEXT"), ("events", "account TEXT"), ("events", "undone_by TEXT")];

tokio::task_local! {
    /// Whose a queued job's journal entries are: whoever queued it (it runs on its own, after their request).
    static BY: Option<String>;
}

/// `f`, what it journals done by `account` (decision 0051: the journal says who changed the library).
pub async fn done_by<F: std::future::Future>(account: Option<String>, f: F) -> F::Output {
    BY.scope(account, f).await
}

/// Who does what the running task journals: a queued job's account, else the running request's (none: Cratarr itself).
fn by() -> Option<String> {
    BY.try_with(Clone::clone).ok().flatten().or_else(|| crate::serve::who::current().map(|w| w.user))
}

pub struct Journal {
    db: Mutex<Connection>,
    /// New events, for the handlers listening (`subscribe`).
    tell: tokio::sync::broadcast::Sender<EventId>,
    /// Events just undone, for handlers that put their work back (`subscribe_undone`).
    tell_undone: tokio::sync::broadcast::Sender<EventId>,
}

impl Journal {
    /// `file`: the journal; `None` keeps it in memory (no data folder).
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = crate::db::open(file)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA).context("creating the journal")?;
        for (table, column) in ADDED_COLUMNS {
            let name = column.split_whitespace().next().unwrap_or_default();
            let has: bool = conn
                .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?
                .exists([name])?;
            if !has {
                conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column}")).context("updating the journal")?;
            }
        }
        Ok(Self { db: Mutex::new(conn), tell: tokio::sync::broadcast::channel(64).0, tell_undone: tokio::sync::broadcast::channel(64).0 })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Write-ahead: write what is about to be done, before doing it. Nobody hears of it until `applied`; a crash in
    /// between leaves it in `intents`, to be finished or forgotten (whichever the disk shows). Returns its id.
    pub fn intend(&self, at: i64, event: &Event) -> Result<EventId> {
        let db = self.db();
        db.execute("INSERT INTO events (at, event, applied, account) VALUES (?1, ?2, 0, ?3)", params![at, serde_json::to_string(event)?, by()])?;
        Ok(db.last_insert_rowid())
    }

    /// The intended action happened: tell whoever listens. Safe to call twice.
    pub fn applied(&self, id: EventId) -> Result<()> {
        self.db().execute("UPDATE events SET applied = 1 WHERE id = ?1", [id])?;
        let _ = self.tell.send(id);
        Ok(())
    }

    /// The intended action did not happen: forget it.
    pub fn forget(&self, id: EventId) -> Result<()> {
        let db = self.db();
        db.execute("DELETE FROM handled WHERE event = ?1", [id])?;
        db.execute("DELETE FROM events WHERE id = ?1 AND applied = 0", [id])?;
        Ok(())
    }

    /// Actions intended but not known to have happened (a crash in between), oldest first.
    pub fn intents(&self) -> Result<Vec<Entry>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, at, event FROM events WHERE applied = 0 ORDER BY id")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        rows.map(|r| {
            let (id, at, event) = r?;
            Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: false, applied: false })
        })
        .collect()
    }

    /// Write an event that already happened, then tell whoever listens. Returns its id.
    #[allow(dead_code)] // album actions use intend/applied; kept for events with nothing to apply
    pub fn record(&self, at: i64, event: &Event) -> Result<EventId> {
        let id = {
            let db = self.db();
            db.execute("INSERT INTO events (at, event, account) VALUES (?1, ?2, ?3)", params![at, serde_json::to_string(event)?, by()])?;
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
        let row = self
            .db()
            .query_row("SELECT at, event, undone_at, applied FROM events WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        row.map(|(at, event, undone, applied): (i64, String, Option<i64>, i64)| {
            Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: undone.is_some(), applied: applied == 1 })
        })
        .transpose()
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

    /// Events since `since` (epoch seconds) that happened, that `handler` has not handled and that were not undone,
    /// oldest first.
    pub fn pending(&self, handler: &str, since: i64) -> Result<Vec<Entry>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT id, at, event FROM events e WHERE at >= ?2 AND undone_at IS NULL AND applied = 1
             AND NOT EXISTS (SELECT 1 FROM handled h WHERE h.event = e.id AND h.handler = ?1) ORDER BY id",
        )?;
        let rows = st.query_map(params![handler, since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        rows.map(|r| {
            let (id, at, event) = r?;
            Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: false, applied: true })
        })
        .collect()
    }

    /// The last album action done (and not undone) on a server's album: what became of it, for a page still showing
    /// the album by its old id.
    pub fn album_fate(&self, source: &str, album_id: &str) -> Result<Option<Event>> {
        let db = self.db();
        let event: Option<String> = db
            .query_row(
                "SELECT event FROM events WHERE applied = 1 AND undone_at IS NULL
                 AND json_extract(event, '$.source') = ?1 AND json_extract(event, '$.album_id') = ?2 ORDER BY id DESC LIMIT 1",
                params![source, album_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(event.map(|e| serde_json::from_str(&e)).transpose()?)
    }

    /// Mark an event undone, then tell whoever listens for that. False when it already was (so an Undo runs once).
    pub fn mark_undone(&self, id: EventId, at: i64) -> Result<bool> {
        let marked = self.db().execute("UPDATE events SET undone_at = ?2, undone_by = ?3 WHERE id = ?1 AND undone_at IS NULL", params![id, at, by()])? == 1;
        if marked {
            let _ = self.tell_undone.send(id);
        }
        Ok(marked)
    }

    /// Event ids as they are undone. A listener that falls behind catches up through `undone_since`.
    pub fn subscribe_undone(&self) -> tokio::sync::broadcast::Receiver<EventId> {
        self.tell_undone.subscribe()
    }

    /// Undone events since `since` that `handler` handled and has not put back yet (no `undo_handler` row), oldest
    /// first.
    pub fn undone_since(&self, handler: &str, undo_handler: &str, since: i64) -> Result<Vec<Entry>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT id, at, event FROM events e WHERE at >= ?3 AND undone_at IS NOT NULL
             AND EXISTS (SELECT 1 FROM handled h WHERE h.event = e.id AND h.handler = ?1)
             AND NOT EXISTS (SELECT 1 FROM handled h WHERE h.event = e.id AND h.handler = ?2) ORDER BY id",
        )?;
        let rows = st.query_map(params![handler, undo_handler, since], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        rows.map(|r| {
            let (id, at, event) = r?;
            Ok(Entry { id, at, event: serde_json::from_str(&event)?, undone: true, applied: true })
        })
        .collect()
    }

    /// Keep a change to playlists or crates (decision 0026) of `account`'s (decision 0051; `None`: the first
    /// administrator's); changes older than a day are forgotten on the way.
    pub fn record_change(&self, at: i64, change: &Change, account: Option<&str>) -> Result<EventId> {
        let db = self.db();
        db.execute("DELETE FROM changes WHERE at < ?1", [at - CHANGE_SECS])?;
        db.execute("INSERT INTO changes (at, change, account) VALUES (?1, ?2, ?3)", params![at, serde_json::to_string(change)?, account])?;
        Ok(db.last_insert_rowid())
    }

    /// A change, whether it was undone, and whose it is; `None` when it is not (or no longer) kept.
    pub fn change(&self, id: EventId) -> Result<Option<(Change, bool, Option<String>)>> {
        let row = self
            .db()
            .query_row("SELECT change, undone_at, account FROM changes WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<String>>(2)?))
            })
            .optional()?;
        row.map(|(c, undone, account)| Ok((serde_json::from_str(&c)?, undone.is_some(), account))).transpose()
    }

    /// Take a change for undoing: true for the one caller that may undo it (it was not undone yet).
    pub fn claim_change(&self, id: EventId, at: i64) -> Result<bool> {
        Ok(self.db().execute("UPDATE changes SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL", params![id, at])? == 1)
    }

    /// Hand a claimed change back (its undo was refused or failed): it can be undone again.
    pub fn release_change(&self, id: EventId) -> Result<()> {
        self.db().execute("UPDATE changes SET undone_at = NULL WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Forget events older than `before` (epoch seconds): nothing can undo or handle them any more.
    pub fn prune(&self, before: i64) -> Result<usize> {
        let db = self.db();
        db.execute("DELETE FROM handled WHERE event IN (SELECT id FROM events WHERE at < ?1)", [before])?;
        Ok(db.execute("DELETE FROM events WHERE at < ?1", [before])?)
    }
}

#[cfg(test)]
mod change_tests {
    use super::*;

    #[test]
    fn a_change_is_kept_for_a_day_and_claimed_once() {
        let j = Journal::open(None).unwrap();
        let c = Change::Renamed { from: "a".into(), to: "b".into() };
        let id = j.record_change(1_000_000, &c, Some("ann")).unwrap();
        assert_eq!(j.change(id).unwrap(), Some((c.clone(), false, Some("ann".into()))));
        assert!(j.claim_change(id, 1_000_010).unwrap(), "the first undo takes it");
        assert!(!j.claim_change(id, 1_000_011).unwrap(), "a second, at the same time, does not");
        j.release_change(id).unwrap();
        assert_eq!(j.change(id).unwrap(), Some((c.clone(), false, Some("ann".into()))), "handed back after a refusal");
        j.record_change(1_000_000 + CHANGE_SECS + 1, &c, None).unwrap();
        assert_eq!(j.change(id).unwrap(), None, "forgotten after a day");
        assert_eq!(fingerprint("x"), fingerprint("x"));
        assert_ne!(fingerprint("x"), fingerprint("y"));
        assert_eq!(fingerprint(""), 0xcbf2_9ce4_8422_2325, "FNV-1a's offset basis: stable");
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

    #[tokio::test]
    async fn a_library_change_says_who_made_it_and_who_undid_it() {
        use crate::serve::who::{Scope, Who, acting_as};
        let j = Journal::open(None).unwrap();
        let id = done_by(Some("ann".into()), async { j.intend(100, &rejected("A")).unwrap() }).await;
        let bob = Who { user: "bob".into(), admin: true, scope: Scope::Full, home: true };
        acting_as(bob, async { j.mark_undone(id, 200).unwrap() }).await;
        let who: (Option<String>, Option<String>) =
            j.db().query_row("SELECT account, undone_by FROM events WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(who, (Some("ann".into()), Some("bob".into())), "a queued job's: who queued it; an undo: who asked");
        let own = j.intend(300, &rejected("B")).unwrap();
        let by: Option<String> = j.db().query_row("SELECT account FROM events WHERE id = ?1", [own], |r| r.get(0)).unwrap();
        assert_eq!(by, None, "Cratarr's own");
    }

    #[test]
    fn an_albums_fate_is_its_last_action_not_undone() {
        let j = Journal::open(None).unwrap();
        assert_eq!(j.album_fate("jellyfin", "a1").unwrap(), None);
        let id = j.record(100, &rejected("Album")).unwrap();
        assert_eq!(j.album_fate("jellyfin", "a1").unwrap(), Some(rejected("Album")));
        assert_eq!(j.album_fate("other", "a1").unwrap(), None);
        assert_eq!(j.album_fate("jellyfin", "a2").unwrap(), None);
        j.mark_undone(id, 101).unwrap();
        assert_eq!(j.album_fate("jellyfin", "a1").unwrap(), None);
    }

    #[test]
    fn events_are_kept_with_what_each_handler_did() {
        let j = Journal::open(None).unwrap();
        let mut rx = j.subscribe();
        let id = j.record(100, &rejected("Album")).unwrap();
        assert_eq!(rx.try_recv().unwrap(), id, "listeners hear of it");
        let e = j.get(id).unwrap().unwrap();
        assert_eq!((e.at, e.undone, e.applied, e.event), (100, false, true, rejected("Album")));
        assert_eq!(j.pending("acquisition", 0).unwrap().len(), 1);
        j.done(id, "acquisition", 101, &serde_json::json!([7, 8])).unwrap();
        assert!(j.pending("acquisition", 0).unwrap().is_empty(), "handled once");
        assert_eq!(j.pending("playlists", 0).unwrap().len(), 1, "per handler");
        assert_eq!(j.result(id, "acquisition").unwrap(), Some(serde_json::json!([7, 8])));
        assert_eq!(j.result(id, "playlists").unwrap(), None);
    }

    #[test]
    fn an_intent_is_heard_of_only_once_applied_and_can_be_forgotten() {
        let j = Journal::open(None).unwrap();
        let mut rx = j.subscribe();
        let id = j.intend(100, &rejected("Album")).unwrap();
        assert!(rx.try_recv().is_err(), "nobody hears of an intent");
        assert!(j.pending("acquisition", 0).unwrap().is_empty(), "not handled before it happened");
        assert_eq!(j.intents().unwrap().iter().map(|e| e.id).collect::<Vec<_>>(), vec![id]);
        assert!(!j.get(id).unwrap().unwrap().applied);
        j.applied(id).unwrap();
        j.applied(id).unwrap();
        assert_eq!(rx.try_recv().unwrap(), id);
        assert!(j.intents().unwrap().is_empty());
        assert_eq!(j.pending("acquisition", 0).unwrap().len(), 1);
        let other = j.intend(100, &rejected("Never")).unwrap();
        j.forget(other).unwrap();
        assert!(j.get(other).unwrap().is_none());
        j.forget(id).unwrap();
        assert!(j.get(id).unwrap().is_some(), "an applied event is never forgotten");
    }

    #[test]
    fn a_journal_from_before_intents_reads_as_applied() {
        let d = std::env::temp_dir().join(format!("cratarr-journal-old-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("journal.sqlite");
        {
            let c = Connection::open(&file).unwrap();
            c.execute_batch(
                "CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, event TEXT NOT NULL, undone_at INTEGER);",
            )
            .unwrap();
            c.execute("INSERT INTO events (at, event) VALUES (100, ?1)", [serde_json::to_string(&rejected("Old")).unwrap()]).unwrap();
        }
        let j = Journal::open(Some(&file)).unwrap();
        assert!(j.get(1).unwrap().unwrap().applied);
        assert_eq!(j.pending("acquisition", 0).unwrap().len(), 1);
        drop(j);
        std::fs::remove_dir_all(&d).ok();
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
    fn an_undo_is_heard_of_and_listed_until_put_back() {
        let j = Journal::open(None).unwrap();
        let mut rx = j.subscribe_undone();
        let (a, b) = (j.record(100, &rejected("A")).unwrap(), j.record(100, &rejected("B")).unwrap());
        j.done(a, "playlists", 101, &serde_json::json!(1)).unwrap();
        j.mark_undone(a, 200).unwrap();
        j.mark_undone(b, 200).unwrap();
        j.mark_undone(a, 201).unwrap();
        assert_eq!((rx.try_recv().unwrap(), rx.try_recv().unwrap()), (a, b));
        assert!(rx.try_recv().is_err(), "heard of once");
        let ids = |v: Vec<Entry>| v.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(j.undone_since("playlists", "playlists-undone", 0).unwrap()), vec![a], "only what the handler did");
        j.done(a, "playlists-undone", 202, &serde_json::json!(1)).unwrap();
        assert!(j.undone_since("playlists", "playlists-undone", 0).unwrap().is_empty());
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
