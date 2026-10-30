//! What offline writes need kept (0.12, decision 0087): the plays counted, and the first answer to each change sent
//! with a key, each for 30 days, so a play or a change a device sends again (late, after a restart) is counted or made
//! once; and when each favourite and rating last changed, so an older change sent late does not undo a newer one.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

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
";

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

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl UserData {
    /// Whether `account`'s `item` changed later than `at` (epoch ms) already: a write made at `at` then changes
    /// nothing. One at the same time is not older (the same write sent again).
    pub fn change_is_older(&self, account: &str, kind: ChangeKind, item: &str, at: i64) -> Result<bool> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT at FROM change_at WHERE account = ?1 AND kind = ?2 AND item = ?3")?;
        let kept: Option<i64> = st.query_row(params![account, kind.as_str(), item], |r| r.get(0)).optional()?;
        Ok(kept.is_some_and(|k| k > at))
    }

    /// `account`'s `item` changed (`op`) at `at` (epoch ms; the device's time, or now): kept as its last change unless
    /// a later one is.
    pub fn mark_changed(&self, account: &str, kind: ChangeKind, item: &str, op: Op, at: i64) -> Result<()> {
        let _ = op;
        self.db().execute(
            "INSERT INTO change_at (account, kind, item, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account, kind, item) DO UPDATE SET at = max(change_at.at, excluded.at)",
            params![account, kind.as_str(), item, at],
        )?;
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

    /// Forgets what is kept here past its time (as of `now`, epoch s). Returns how many rows went.
    pub fn forget_old_offline(&self, now: i64) -> Result<usize> {
        let cut = now - KEEP_SECS;
        let db = self.db();
        let mut n = db.execute("DELETE FROM play_seen WHERE at < ?1", [cut])?;
        n += db.execute("DELETE FROM kept_answer WHERE at < ?1", [cut])?;
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
