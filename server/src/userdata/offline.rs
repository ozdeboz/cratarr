//! What offline writes need kept (0.12, decision 0087): the plays counted, and the first answer to each change sent
//! with a key, each for 30 days, so a play or a change a device sends again (late, after a restart) is counted or made
//! once; when each favourite and rating last changed, so an older change sent late does not undo a newer one; and the
//! change log, so a device asks what changed since it last looked instead of reading everything again; and the
//! playlist operations applied, for playlist edits made offline (the entries' ids are the playlists' own:
//! `super::playlists`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::UserData;

pub const SCHEMA: &str = "
-- The plays counted (decision 0087): the player's id for each, and when it came (epoch s).
CREATE TABLE IF NOT EXISTS play_seen (id TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS play_seen_at ON play_seen (at);
-- A change's first answer by its `Idempotency-Key` (crate::idempotent): whose key, the change it came with (method and
-- path), the answer's status, content type and body, and when (epoch s).
CREATE TABLE IF NOT EXISTS kept_answer (
  user TEXT NOT NULL, key TEXT NOT NULL, asked TEXT NOT NULL, status INTEGER NOT NULL, kind TEXT, body BLOB NOT NULL,
  at INTEGER NOT NULL, PRIMARY KEY (user, key)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS kept_answer_at ON kept_answer (at);
-- When an account's favourite or rating of an item last changed (epoch ms, the device's time of the change): a write
-- older than it changes nothing (the last change wins). `item` as the change log names it (`track:<source>/<id>`).
CREATE TABLE IF NOT EXISTS change_at (
  account TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (account, kind, item)) WITHOUT ROWID;
-- The change log: one counter per server, never handed out again (AUTOINCREMENT: not even once its newest rows are
-- pruned), and an item's latest change only (its earlier row goes). `account`: whose (a playlist's: its owner's
-- folder, '' the first administrator's); `shared`: a playlist others see (shared then, or its sharing changed); `at`:
-- when it was logged (epoch ms); `seen_by`: one other account's row alone ('' for the usual rows): a shared playlist
-- gone from its sight (unshared, deleted, renamed away). Its unique index is made in `migrate`.
CREATE TABLE IF NOT EXISTS change_log (
  rev INTEGER PRIMARY KEY AUTOINCREMENT, account TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL, op TEXT NOT NULL,
  at INTEGER NOT NULL, shared INTEGER NOT NULL DEFAULT 0, seen_by TEXT NOT NULL DEFAULT '');
-- The highest rev pruned: asked for changes since before it, a device reads everything again.
CREATE TABLE IF NOT EXISTS change_log_floor (one INTEGER PRIMARY KEY CHECK (one = 1), rev INTEGER NOT NULL);
-- The playlist operations applied or refused, by the account and the operation's id: one sent again is not applied
-- again, and a refused one is refused again (`why`, none when applied). Kept as long as the change log (epoch ms).
CREATE TABLE IF NOT EXISTS playlist_op (account TEXT NOT NULL, op_id TEXT NOT NULL, at INTEGER NOT NULL, why TEXT,
  PRIMARY KEY (account, op_id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS playlist_op_at ON playlist_op (at);
";

/// Tables made by an earlier build of 0.12 get what came since: the change log's rows for one account alone
/// (`seen_by`, and its unique index with it), and a refused operation's reason. Safe to repeat.
pub(super) fn migrate(conn: &Connection) -> Result<()> {
    let has = |table: &str, column: &str| -> Result<bool> {
        Ok(conn.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?.exists([column])?)
    };
    if !has("change_log", "seen_by")? {
        conn.execute_batch("ALTER TABLE change_log ADD COLUMN seen_by TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS change_log_item;
         CREATE UNIQUE INDEX IF NOT EXISTS change_log_one ON change_log (account, kind, item, seen_by);",
    )?;
    if !has("playlist_op", "why")? {
        conn.execute_batch("ALTER TABLE playlist_op ADD COLUMN why TEXT")?;
    }
    Ok(())
}

/// The most rows the change log keeps; past them the oldest go, as do those older than `KEEP_SECS`.
pub const LOG_MOST: i64 = 50_000;

/// How long what is kept here is kept (s): a device gives a play or a change up after 30 days.
pub const KEEP_SECS: i64 = 30 * 24 * 3600;

/// A change's first answer, as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptAnswer {
    /// The change it came with: method and path.
    pub asked: String,
    pub status: u16,
    /// Its content type.
    pub kind: Option<String>,
    pub body: Vec<u8>,
}

/// What changed, as the change log names it (decision 0087).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A playlist's lines, name or sharing; its id is its key (`Store::key`).
    Playlist,
    /// A favourite: `track:<source>/<id>`, `album:<source>/<id>`, `artist:<source>/<name>`, `playlist:<id>`.
    Favourite,
    /// A rating: `album:<source>/<id>` (a Subsonic app's on a song or an artist: `track:`, `artist:`).
    Rating,
    /// What is excluded everywhere: `tracks` or `artists`.
    Excluded,
}

impl ChangeKind {
    pub const ALL: [ChangeKind; 4] = [ChangeKind::Playlist, ChangeKind::Favourite, ChangeKind::Rating, ChangeKind::Excluded];

    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Playlist => "playlist",
            ChangeKind::Favourite => "favourite",
            ChangeKind::Rating => "rating",
            ChangeKind::Excluded => "excluded",
        }
    }

    pub fn of(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// How it changed: there (a favourite now, rated, made or edited), or no longer (not a favourite, no stars, deleted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Changed,
    Deleted,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Changed => "changed",
            Op::Deleted => "deleted",
        }
    }

    /// A mark set (`true`: changed) or taken off (deleted).
    pub fn of(on: bool) -> Self {
        if on { Op::Changed } else { Op::Deleted }
    }
}

/// An item's id in the change log: `track:<source>/<id>`.
pub fn item_id(what: &str, source: &str, id: &str) -> String {
    format!("{what}:{source}/{id}")
}

/// One change as the log has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logged {
    pub rev: i64,
    pub kind: ChangeKind,
    /// As the log keeps it (a playlist's: its key, `Store::key`).
    pub item: String,
    pub op: Op,
}

/// What changed since a device last looked (`changes_since`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Since {
    /// Not known from there (asked without a rev, from before what is kept, or from ahead of this server's counter):
    /// read everything again, and go on from `rev`.
    Reset { rev: i64 },
    /// The changes after it, oldest first; `more`: ask again from `rev` (the last one's).
    Changes { rev: i64, changes: Vec<Logged>, more: bool },
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A change logged in `conn` (inside the caller's transaction, where it has one): the item's earlier row replaced, a
/// new rev handed out.
pub(super) fn log(conn: &Connection, account: &str, kind: ChangeKind, item: &str, op: Op, shared: bool) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO change_log (account, kind, item, op, at, shared) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![account, kind.as_str(), item, op.as_str(), super::now_ms(), shared],
    )?;
    Ok(())
}

/// A shared playlist gone from the sight of `others` (unshared, deleted): a `deleted` for each of them alone, by its id,
/// under its owner.
pub(super) fn log_gone_for(conn: &Connection, id: &str, others: &[String]) -> Result<()> {
    let owner = super::playlists::owner_of(conn, id)?.unwrap_or_default();
    let mut st = conn.prepare_cached(
        "INSERT OR REPLACE INTO change_log (account, kind, item, op, at, shared, seen_by) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
    )?;
    for other in others.iter().filter(|o| !o.is_empty()) {
        st.execute(params![owner, ChangeKind::Playlist.as_str(), id, Op::Deleted.as_str(), super::now_ms(), other])?;
    }
    Ok(())
}

impl UserData {
    /// Which of `account`'s playlist operations were applied (`None`) or refused (why), by their ids.
    pub fn ops_applied(&self, account: &str, ops: &[String]) -> Result<HashMap<String, Option<String>>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT why FROM playlist_op WHERE account = ?1 AND op_id = ?2")?;
        let mut out = HashMap::new();
        for op in ops {
            if let Some(why) = st.query_row(params![account, op], |r| r.get::<_, Option<String>>(0)).optional()? {
                out.insert(op.clone(), why);
            }
        }
        Ok(out)
    }

    /// A shared playlist (by its id) gone from the sight of `others`, the accounts other than its owner: each is told
    /// it as deleted (decision 0087).
    pub fn playlist_gone_for(&self, id: &str, others: &[String]) -> Result<()> {
        log_gone_for(&self.db(), id, others)
    }

    /// The change log's counter now: the rev of its last change.
    pub fn change_rev(&self) -> Result<i64> {
        Ok(self
            .db()
            .query_row("SELECT seq FROM sqlite_sequence WHERE name = 'change_log'", [], |r| r.get(0))
            .optional()?
            .unwrap_or(0))
    }

    /// What `user` excluded everywhere changed: `tracks` or `artists`.
    pub fn excluded_changed(&self, user: &str, what: &str) -> Result<()> {
        log(&self.db(), user, ChangeKind::Excluded, what, Op::Changed, false)
    }

    /// The changes after `since` (none: `Reset`) that `user` may see, of these kinds, at most `most`: their own, and
    /// the playlists of `folder` (their playlists' folder, '' the first administrator's) and those shared.
    pub fn changes_since(&self, since: Option<i64>, user: &str, folder: &str, kinds: &[ChangeKind], most: usize) -> Result<Since> {
        let db = self.db();
        let rev: i64 = db
            .query_row("SELECT seq FROM sqlite_sequence WHERE name = 'change_log'", [], |r| r.get(0))
            .optional()?
            .unwrap_or(0);
        let floor: i64 = db.query_row("SELECT rev FROM change_log_floor WHERE one = 1", [], |r| r.get(0)).optional()?.unwrap_or(0);
        let Some(since) = since.filter(|&s| s >= floor && s <= rev) else { return Ok(Since::Reset { rev }) };
        if kinds.is_empty() {
            return Ok(Since::Changes { rev, changes: Vec::new(), more: false });
        }
        // The kinds' names are this module's own, never what was asked: safe in the statement.
        let kinds = kinds.iter().map(|k| format!("'{}'", k.as_str())).collect::<Vec<_>>().join(", ");
        let mut st = db.prepare(&format!(
            "SELECT rev, kind, item, op FROM change_log WHERE rev > ?1 AND kind IN ({kinds})
               AND ((kind = 'playlist' AND ((seen_by = '' AND (account = ?3 OR shared = 1)) OR seen_by = ?2))
                 OR (kind <> 'playlist' AND account = ?2))
             ORDER BY rev LIMIT ?4"
        ))?;
        let rows = st.query_map(params![since, user, folder, most as i64 + 1], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        let mut changes = Vec::new();
        for row in rows {
            let (rev, kind, item, op) = row?;
            let Some(kind) = ChangeKind::of(&kind) else { continue };
            changes.push(Logged { rev, kind, item, op: if op == Op::Deleted.as_str() { Op::Deleted } else { Op::Changed } });
        }
        let more = changes.len() > most;
        changes.truncate(most);
        let rev = match (more, changes.last()) {
            (true, Some(last)) => last.rev,
            _ => rev,
        };
        Ok(Since::Changes { rev, changes, more })
    }

    /// Whether `account`'s `item` changed later than `at` (epoch ms) already: a write made at `at` then changes
    /// nothing. One at the same time is not older (the same write sent again).
    pub fn change_is_older(&self, account: &str, kind: ChangeKind, item: &str, at: i64) -> Result<bool> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT at FROM change_at WHERE account = ?1 AND kind = ?2 AND item = ?3")?;
        let kept: Option<i64> = st.query_row(params![account, kind.as_str(), item], |r| r.get(0)).optional()?;
        Ok(kept.is_some_and(|k| k > at))
    }

    /// `account`'s `item` changed (`op`) at `at` (epoch ms; the device's time, or now): kept as its last change unless
    /// a later one is, and logged.
    pub fn mark_changed(&self, account: &str, kind: ChangeKind, item: &str, op: Op, at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO change_at (account, kind, item, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account, kind, item) DO UPDATE SET at = max(change_at.at, excluded.at)",
            params![account, kind.as_str(), item, at],
        )?;
        log(&tx, account, kind, item, op, false)?;
        tx.commit()?;
        Ok(())
    }

    /// Which of these plays were counted.
    pub fn plays_seen(&self, ids: &[String]) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT 1 FROM play_seen WHERE id = ?1")?;
        let mut out = HashSet::new();
        for id in ids {
            if st.exists([id])? {
                out.insert(id.clone());
            }
        }
        Ok(out)
    }

    /// Plays counted, by id, and when they came (epoch s). Safe to repeat.
    pub fn add_plays_seen(&self, seen: &[(String, i64)]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached("INSERT OR IGNORE INTO play_seen (id, at) VALUES (?1, ?2)")?;
            for (id, at) in seen {
                st.execute(params![id, at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The first answer to `user`'s change with this key, if kept.
    pub fn kept_answer(&self, user: &str, key: &str) -> Result<Option<KeptAnswer>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT asked, status, kind, body FROM kept_answer WHERE user = ?1 AND key = ?2")?;
        Ok(st
            .query_row(params![user, key], |r| {
                Ok(KeptAnswer { asked: r.get(0)?, status: r.get::<_, i64>(1)?.clamp(100, 999) as u16, kind: r.get(2)?, body: r.get(3)? })
            })
            .optional()?)
    }

    /// Keeps the first answer to `user`'s change with this key. A key kept already keeps its first.
    pub fn keep_answer(&self, user: &str, key: &str, a: &KeptAnswer) -> Result<()> {
        self.db().execute(
            "INSERT OR IGNORE INTO kept_answer (user, key, asked, status, kind, body, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![user, key, a.asked, a.status as i64, a.kind, a.body, now()],
        )?;
        Ok(())
    }

    /// Forgets what is kept here past its time (as of `now`, epoch s): older than `KEEP_SECS`, and the change log
    /// past `LOG_MOST` rows, its floor raised to the last rev gone. Returns how many rows went.
    pub fn forget_old_offline(&self, now: i64) -> Result<usize> {
        let cut = now - KEEP_SECS;
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut n = tx.execute("DELETE FROM play_seen WHERE at < ?1", [cut])?;
        n += tx.execute("DELETE FROM kept_answer WHERE at < ?1", [cut])?;
        n += UserData::forget_old_scrobble_seen(&tx, cut)?;
        n += tx.execute("DELETE FROM change_at WHERE at < ?1", [cut * 1000])?;
        n += super::playlists::forget_old(&tx, cut * 1000)?;
        n += tx.execute("DELETE FROM playlist_op WHERE at < ?1", [cut * 1000])?;
        let old: Option<i64> = tx.query_row("SELECT max(rev) FROM change_log WHERE at < ?1", [cut * 1000], |r| r.get(0))?;
        let many: Option<i64> =
            tx.query_row("SELECT rev FROM change_log ORDER BY rev DESC LIMIT 1 OFFSET ?1", [LOG_MOST], |r| r.get(0)).optional()?;
        if let Some(upto) = old.max(many) {
            n += tx.execute("DELETE FROM change_log WHERE rev <= ?1", [upto])?;
            tx.execute(
                "INSERT INTO change_log_floor (one, rev) VALUES (1, ?1) ON CONFLICT (one) DO UPDATE SET rev = max(rev, excluded.rev)",
                [upto],
            )?;
        }
        tx.commit()?;
        Ok(n)
    }
}

/// What offline writes keep, pruned once an hour in the background (the first a minute after the start).
pub fn prune_in_background(ud: Arc<UserData>) {
    tokio::spawn(async move {
        let every = std::time::Duration::from_secs(3600);
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + std::time::Duration::from_secs(60), every);
        loop {
            tick.tick().await;
            let ud = ud.clone();
            match tokio::task::spawn_blocking(move || ud.forget_old_offline(now())).await {
                Ok(Ok(n)) if n > 0 => tracing::debug!("offline writes: {n} kept rows past their time forgotten"),
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::warn!("offline writes: what is kept could not be pruned: {e:#}"),
                Err(e) => tracing::warn!("offline writes: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_counted_are_kept_and_forgotten_after_thirty_days() {
        let u = UserData::open(None).unwrap();
        u.add_plays_seen(&[("p1".into(), 1_000), ("p2".into(), 1_000 + KEEP_SECS)]).unwrap();
        u.add_plays_seen(&[("p1".into(), 5_000)]).unwrap();
        let ids = ["p1".to_string(), "p2".to_string(), "p3".to_string()];
        assert_eq!(u.plays_seen(&ids).unwrap(), ["p1".to_string(), "p2".to_string()].into());
        assert_eq!(u.forget_old_offline(1_001 + KEEP_SECS).unwrap(), 1, "p1 came first, at 1000: it goes");
        assert_eq!(u.plays_seen(&ids).unwrap(), ["p2".to_string()].into());
    }

    #[test]
    fn the_last_change_wins_by_its_time_per_account_and_item() {
        let u = UserData::open(None).unwrap();
        let fav = ChangeKind::Favourite;
        let item = item_id("track", "cratarr", "tr-1");
        assert!(!u.change_is_older("me", fav, &item, 100).unwrap(), "nothing kept: made");
        u.mark_changed("me", fav, &item, Op::Changed, 200).unwrap();
        assert!(u.change_is_older("me", fav, &item, 150).unwrap(), "older than the one kept: not made");
        assert!(!u.change_is_older("me", fav, &item, 200).unwrap(), "the same, sent again: made");
        assert!(!u.change_is_older("me", fav, &item, 250).unwrap());
        // An older one marked does not move the time back.
        u.mark_changed("me", fav, &item, Op::Deleted, 120).unwrap();
        assert!(u.change_is_older("me", fav, &item, 150).unwrap());
        assert!(!u.change_is_older("you", fav, &item, 150).unwrap(), "per account");
        assert!(!u.change_is_older("me", ChangeKind::Rating, &item, 150).unwrap(), "per kind");
        assert_eq!(ChangeKind::of("rating"), Some(ChangeKind::Rating));
        assert_eq!(ChangeKind::of("crate"), None);
    }

    fn changes(s: Since) -> (i64, Vec<(i64, &'static str, String, &'static str)>, bool) {
        match s {
            Since::Changes { rev, changes, more } => (rev, changes.into_iter().map(|c| (c.rev, c.kind.as_str(), c.item, c.op.as_str())).collect(), more),
            Since::Reset { rev } => panic!("reset at {rev}"),
        }
    }

    #[test]
    fn the_change_log_gives_each_account_its_own_changes_and_the_playlists_it_sees() {
        let u = UserData::open(None).unwrap();
        let all = ChangeKind::ALL;
        assert_eq!(u.changes_since(None, "me", "", &all, 10).unwrap(), Since::Reset { rev: 0 }, "asked without a rev: read everything");
        let fav = item_id("track", "cratarr", "tr-1");
        u.mark_changed("me", ChangeKind::Favourite, &fav, Op::Changed, 100).unwrap(); // 1
        u.mark_changed("you", ChangeKind::Favourite, &fav, Op::Changed, 100).unwrap(); // 2
        let evening = u.create_playlist("", "Evening", &[], &Default::default()).unwrap().unwrap();
        let morning = u.create_playlist("you", "Morning", &[], &Default::default()).unwrap().unwrap();
        u.playlist_changed(&evening, Op::Changed, false).unwrap(); // 3: the first administrator's
        u.playlist_changed(&morning, Op::Changed, false).unwrap(); // 4
        u.excluded_changed("me", "tracks").unwrap(); // 5
        let (rev, got, more) = changes(u.changes_since(Some(0), "me", "", &all, 10).unwrap());
        assert_eq!(rev, 5);
        assert!(!more);
        assert_eq!(
            got,
            vec![(1, "favourite", fav.clone(), "changed"), (3, "playlist", evening.clone(), "changed"), (5, "excluded", "tracks".into(), "changed")]
        );
        // Shared, another's playlist is seen; its sharing taken back is seen too, once.
        u.set_playlist_shared(&morning, true).unwrap(); // 6
        let (_, got, _) = changes(u.changes_since(Some(5), "me", "", &all, 10).unwrap());
        assert_eq!(got, vec![(6, "playlist", morning.clone(), "changed")]);
        // Only the latest change of an item is kept: unstarred, the star's row goes.
        u.mark_changed("me", ChangeKind::Favourite, &fav, Op::Deleted, 200).unwrap(); // 7
        let (rev, got, _) = changes(u.changes_since(Some(0), "me", "", &[ChangeKind::Favourite], 10).unwrap());
        assert_eq!((rev, got), (7, vec![(7, "favourite", fav.clone(), "deleted")]));
        // At most `most` at once; the rest from the last one's rev.
        let (rev, got, more) = changes(u.changes_since(Some(0), "me", "", &all, 2).unwrap());
        assert_eq!((rev, got.len(), more), (5, 2, true));
        let (rev, got, more) = changes(u.changes_since(Some(rev), "me", "", &all, 2).unwrap());
        assert_eq!((rev, got.iter().map(|c| c.0).collect::<Vec<_>>(), more), (7, vec![6, 7], false));
        // From ahead of the counter: read everything again.
        assert_eq!(u.changes_since(Some(99), "me", "", &all, 10).unwrap(), Since::Reset { rev: 7 });
    }

    #[test]
    fn a_shared_playlist_gone_from_sight_is_deleted_for_each_other_account_alone() {
        let u = UserData::open(None).unwrap();
        let all = ChangeKind::ALL;
        let morning = u.create_playlist("you", "Morning", &[], &Default::default()).unwrap().unwrap();
        u.set_playlist_shared(&morning, true).unwrap(); // 1
        // Unshared: its owner sees it changed; "me" and "them" each see it deleted, under its own row.
        assert!(u.set_playlist_shared(&morning, false).unwrap()); // 2: the owner's
        u.playlist_gone_for(&morning, &["me".to_string(), "them".to_string()]).unwrap(); // 3, 4
        let (_, got, _) = changes(u.changes_since(Some(0), "me", "", &all, 10).unwrap());
        assert_eq!(got, vec![(3, "playlist", morning.clone(), "deleted")]);
        let (_, got, _) = changes(u.changes_since(Some(0), "you", "you", &all, 10).unwrap());
        assert_eq!(got, vec![(2, "playlist", morning.clone(), "changed")]);
        // Edited since, unshared: the others' deletes stay.
        u.playlist_changed(&morning, Op::Changed, false).unwrap(); // 5
        let (_, got, _) = changes(u.changes_since(Some(0), "them", "them", &all, 10).unwrap());
        assert_eq!(got, vec![(4, "playlist", morning.clone(), "deleted")]);
        assert!(!u.set_playlist_shared(&morning, false).unwrap(), "not shared: nothing to stop");
    }

    #[test]
    fn a_refused_operation_is_kept_with_why() {
        let u = UserData::open(None).unwrap();
        let ops = [("op-1".to_string(), None), ("op-2".to_string(), Some("not found".to_string()))];
        u.create_playlist("", "Evening", &[], &Default::default()).unwrap();
        u.write_playlist_entries("", "Evening", &[], &crate::playlist_ops::Entries::default(), "me", &ops, &Default::default()).unwrap();
        let got = u.ops_applied("me", &["op-1".into(), "op-2".into(), "op-3".into()]).unwrap();
        assert_eq!(got, [("op-1".to_string(), None), ("op-2".to_string(), Some("not found".to_string()))].into());
        assert!(u.ops_applied("you", &["op-1".to_string()]).unwrap().is_empty(), "per account");
    }

    #[test]
    fn the_change_log_is_pruned_by_age_and_a_rev_from_before_it_reads_everything_again() {
        let u = UserData::open(None).unwrap();
        u.excluded_changed("me", "tracks").unwrap(); // 1
        u.excluded_changed("me", "artists").unwrap(); // 2
        // Thirty days and a minute on, both are past their time.
        let later = now() + KEEP_SECS + 60;
        assert!(u.forget_old_offline(later).unwrap() >= 2);
        assert_eq!(u.changes_since(Some(1), "me", "", &ChangeKind::ALL, 10).unwrap(), Since::Reset { rev: 2 });
        assert_eq!(changes(u.changes_since(Some(2), "me", "", &ChangeKind::ALL, 10).unwrap()), (2, Vec::new(), false));
        // The counter goes on, never handing out a rev again.
        u.excluded_changed("me", "tracks").unwrap();
        assert_eq!(changes(u.changes_since(Some(2), "me", "", &ChangeKind::ALL, 10).unwrap()).1[0].0, 3);
    }

    #[test]
    fn a_kept_answer_is_the_first_and_per_account() {
        let u = UserData::open(None).unwrap();
        let first = KeptAnswer { asked: "POST /api/x".into(), status: 200, kind: Some("application/json".into()), body: b"{\"n\":1}".to_vec() };
        u.keep_answer("me", "k1", &first).unwrap();
        u.keep_answer("me", "k1", &KeptAnswer { body: b"{\"n\":2}".to_vec(), ..first.clone() }).unwrap();
        assert_eq!(u.kept_answer("me", "k1").unwrap(), Some(first));
        assert_eq!(u.kept_answer("you", "k1").unwrap(), None);
    }
}
