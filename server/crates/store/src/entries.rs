//! The one table of Changes (SCENARIOS-GF §0; DOMAIN-CORE §11): every command on the one path keeps its Change here, in the
//! journal's file beside RN's job rows and DI01's, on its one connection. A row is kept for good; what undoing it needs
//! (`data`) goes with its kind's window (T-GF-11). Its id is never handed out again (AUTOINCREMENT), and its token is `j<id>`.
//!
//! - **Write-ahead** (`intend_change`): written before its command applies, as `waiting` (its job not started) or `running`;
//!   then `change_applied` with the rev its own change-log rows took (CP9), or `forget_change`: it never happened.
//! - **One outcome at a time**, each a compare-and-set on its `state`: a job's `start_change` against an Undo's
//!   `cancel_change` (CP8), two Undos' `claim_undo` (CP3); the loser changes nothing.
//! - **A resend finds its Change** by its account and key (`change_by_key`; `intend_change` answers the one already kept).
//! - Named apart from the album events' `intend`, `applied` and `forget` (`journal.rs`), which GF's rest folds into this table.

use anyhow::{Result, anyhow};
use cratarr_domain::change::{Change, ChangeId, ChangeKind, Step, UndoState};
use rusqlite::{OptionalExtension, Row, params};

use crate::journal::Journal;

/// The table, made when missing: safe at every open.
pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entries (
  id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, account TEXT, at INTEGER NOT NULL, object TEXT NOT NULL,
  name TEXT NOT NULL, data TEXT NOT NULL, steps TEXT NOT NULL DEFAULT '[]', state TEXT NOT NULL, key TEXT, rev INTEGER,
  answer TEXT, undone_at INTEGER, undone_by TEXT, undo_rev INTEGER);
CREATE UNIQUE INDEX IF NOT EXISTS entries_key ON entries (coalesce(account, ''), key) WHERE key IS NOT NULL;
";

/// A Change as kept, with what its command answered (a resend gets it again) and the rev its Undo took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub change: Change,
    /// Its command's result, as answered (JSON); none before it applied.
    pub answer: Option<String>,
    pub undo_rev: Option<i64>,
}

const COLUMNS: &str = "id, kind, account, at, object, name, data, steps, state, key, rev, answer, undone_at, undone_by, undo_rev";

fn state_of(c: &Change) -> &'static str {
    match c.state {
        UndoState::Waiting => "waiting",
        UndoState::Running => "running",
        UndoState::Open => "open",
        UndoState::Undoing { .. } => "undoing",
        UndoState::Undone { .. } => "undone",
        UndoState::NoLonger => "no_longer",
    }
}

/// A row read back; `None` for a kind this build doesn't know (it can't be undone here).
fn kept(r: &Row) -> rusqlite::Result<Option<Kept>> {
    let kind: String = r.get(1)?;
    let Some(kind) = ChangeKind::of(&kind) else { return Ok(None) };
    let (state, undone_at, by): (String, Option<i64>, Option<String>) = (r.get(8)?, r.get(12)?, r.get(13)?);
    let state = match state.as_str() {
        "waiting" => UndoState::Waiting,
        "running" => UndoState::Running,
        "open" => UndoState::Open,
        "undoing" => UndoState::Undoing { by },
        "undone" => UndoState::Undone { at: undone_at.unwrap_or_default(), by },
        _ => UndoState::NoLonger,
    };
    let steps: String = r.get(7)?;
    let change = Change {
        id: ChangeId(r.get(0)?),
        kind,
        who: r.get(2)?,
        at: r.get(3)?,
        object: r.get(4)?,
        name: r.get(5)?,
        data: r.get(6)?,
        steps: serde_json::from_str::<Vec<Step>>(&steps).unwrap_or_default(),
        state,
        key: r.get(9)?,
        rev: r.get(10)?,
    };
    Ok(Some(Kept { change, answer: r.get(11)?, undo_rev: r.get(14)? }))
}

impl Journal {
    /// Write-ahead (§0's `intend`): `c` kept before its command applies, `waiting` or `running` as its state says, its id
    /// given now (the token the answer carries at once). A Change already kept with its account's key is that one: the id
    /// and `false`.
    pub fn intend_change(&self, c: &Change) -> Result<(ChangeId, bool)> {
        if !matches!(c.state, UndoState::Waiting | UndoState::Running) {
            return Err(anyhow!("a Change is intended waiting or running"));
        }
        let db = self.db();
        let made = db.execute(
            "INSERT OR IGNORE INTO entries (kind, account, at, object, name, data, steps, state, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![c.kind.name(), c.who, c.at, c.object, c.name, c.data, serde_json::to_string(&c.steps)?, state_of(c), c.key],
        )? == 1;
        if made {
            return Ok((ChangeId(db.last_insert_rowid()), true));
        }
        let id: i64 = db.query_row(
            "SELECT id FROM entries WHERE coalesce(account, '') = coalesce(?1, '') AND key = ?2",
            params![c.who, c.key],
            |r| r.get(0),
        )?;
        Ok((ChangeId(id), false))
    }

    /// Its job starts (`waiting` to `running`): false when an Undo cancelled it first, and the job doesn't run (CP8).
    pub fn start_change(&self, id: ChangeId) -> Result<bool> {
        Ok(self.db().execute("UPDATE entries SET state = 'running' WHERE id = ?1 AND state = 'waiting'", [id.0])? == 1)
    }

    /// It applied: open for its Undo, at the rev its own change-log rows took, its command's answer kept. False when it was
    /// not running (cancelled, or settled already).
    pub fn change_applied(&self, id: ChangeId, rev: i64, answer: &str) -> Result<bool> {
        Ok(self.db().execute("UPDATE entries SET state = 'open', rev = ?2, answer = ?3 WHERE id = ?1 AND state = 'running'", params![id.0, rev, answer])?
            == 1)
    }

    /// It never happened (refused or failed before its store's write, or a stop the start found unapplied): gone.
    pub fn forget_change(&self, id: ChangeId) -> Result<()> {
        self.db().execute("DELETE FROM entries WHERE id = ?1 AND state IN ('waiting', 'running')", [id.0])?;
        Ok(())
    }

    /// An Undo of it before its job started (`waiting` to `undone`): nothing was applied. False when the job started first.
    pub fn cancel_change(&self, id: ChangeId, at: i64, by: Option<&str>) -> Result<bool> {
        Ok(self.db().execute(
            "UPDATE entries SET state = 'undone', undone_at = ?2, undone_by = ?3 WHERE id = ?1 AND state = 'waiting'",
            params![id.0, at, by],
        )? == 1)
    }

    /// Take it for undoing (`open` to `undoing`): true for the one Undo that goes on; any other changes nothing (CP3).
    pub fn claim_undo(&self, id: ChangeId, by: Option<&str>) -> Result<bool> {
        Ok(self.db().execute("UPDATE entries SET state = 'undoing', undone_by = ?2 WHERE id = ?1 AND state = 'open'", params![id.0, by])? == 1)
    }

    /// Hand a claim back (its Undo was refused or failed): it can be undone again.
    pub fn release_undo(&self, id: ChangeId) -> Result<()> {
        self.db().execute("UPDATE entries SET state = 'open', undone_by = NULL WHERE id = ?1 AND state = 'undoing'", [id.0])?;
        Ok(())
    }

    /// Its Undo done, at the rev it took: undone by whoever claimed it.
    pub fn change_undone(&self, id: ChangeId, at: i64, rev: i64) -> Result<bool> {
        Ok(self.db().execute("UPDATE entries SET state = 'undone', undone_at = ?2, undo_rev = ?3 WHERE id = ?1 AND state = 'undoing'", params![id.0, at, rev])?
            == 1)
    }

    /// A Change as kept; none when it isn't (never was, forgotten, or a kind this build doesn't know).
    pub fn change_entry(&self, id: ChangeId) -> Result<Option<Kept>> {
        let db = self.db();
        let row = db.query_row(&format!("SELECT {COLUMNS} FROM entries WHERE id = ?1"), [id.0], kept).optional()?;
        Ok(row.flatten())
    }

    /// The Change `account` sent with `key` (an `Idempotency-Key`), if one was: a resend finds it (ID2).
    pub fn change_by_key(&self, account: Option<&str>, key: &str) -> Result<Option<ChangeId>> {
        let db = self.db();
        let id: Option<i64> = db
            .query_row("SELECT id FROM entries WHERE coalesce(account, '') = coalesce(?1, '') AND key = ?2", params![account, key], |r| r.get(0))
            .optional()?;
        Ok(id.map(ChangeId))
    }

    /// The Changes a stop left between their intent and their end (`running`), or in their Undo (`undoing`), oldest first: for
    /// the start to settle by what their stores show.
    pub fn unsettled(&self) -> Result<Vec<Kept>> {
        let db = self.db();
        let mut st = db.prepare(&format!("SELECT {COLUMNS} FROM entries WHERE state IN ('running', 'undoing') ORDER BY id"))?;
        let rows = st.query_map([], kept)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(state: UndoState, key: Option<&str>) -> Change {
        Change {
            id: ChangeId(0),
            kind: ChangeKind::ArtistOut,
            who: Some("alice".into()),
            at: 1_800_000_000,
            object: "artist:glass harbour".into(),
            name: "Glass Harbour excluded everywhere".into(),
            data: r#"{"out":true}"#.into(),
            steps: Vec::new(),
            state,
            key: key.map(str::to_string),
            rev: None,
        }
    }

    /// §0's store: write-ahead, then applied at its rev with its answer; a claim taken once (CP3); a start against a cancel, one
    /// outcome (CP8); a resend by its key finds its Change; an id never given twice. Red first: new (no table of Changes; today's
    /// `claim_change` holds the playlists' alone).
    #[test]
    fn a_change_is_kept_claimed_once_and_found_by_its_key() {
        let j = Journal::open(None).unwrap();
        let (id, new) = j.intend_change(&change(UndoState::Running, Some("K"))).unwrap();
        assert!(new);
        assert_eq!(j.intend_change(&change(UndoState::Running, Some("K"))).unwrap(), (id, false), "a resend: the one kept");
        assert_eq!(j.change_by_key(Some("alice"), "K").unwrap(), Some(id));
        assert_eq!(j.change_by_key(Some("bob"), "K").unwrap(), None, "a key is its account's");
        let (bobs, _) = j.intend_change(&Change { who: Some("bob".into()), ..change(UndoState::Running, Some("K")) }).unwrap();
        assert_ne!(bobs, id, "a key is its account's");
        assert_eq!(j.change_entry(id).unwrap().unwrap().change.state, UndoState::Running);
        assert!(j.change_applied(id, 41, r#"{"key":"glass harbour"}"#).unwrap());
        assert!(!j.change_applied(id, 42, "{}").unwrap(), "applied once");
        let kept = j.change_entry(id).unwrap().unwrap();
        assert_eq!((kept.change.state.clone(), kept.change.rev, kept.answer.as_deref()), (UndoState::Open, Some(41), Some(r#"{"key":"glass harbour"}"#)));
        // Two Undos: one claims, the other changes nothing; a claim handed back can be claimed again.
        assert!(j.claim_undo(id, Some("alice")).unwrap());
        assert!(!j.claim_undo(id, Some("alice")).unwrap());
        assert_eq!(j.change_entry(id).unwrap().unwrap().change.state, UndoState::Undoing { by: Some("alice".into()) });
        j.release_undo(id).unwrap();
        assert!(j.claim_undo(id, Some("alice")).unwrap());
        assert!(j.change_undone(id, 1_800_000_100, 43).unwrap());
        assert!(!j.claim_undo(id, Some("alice")).unwrap(), "undone: no second Undo");
        let kept = j.change_entry(id).unwrap().unwrap();
        assert_eq!((kept.change.state, kept.undo_rev), (UndoState::Undone { at: 1_800_000_100, by: Some("alice".into()) }, Some(43)));
        // A queued Change: its start and an Undo's cancel, one outcome whichever comes first.
        let (queued, _) = j.intend_change(&change(UndoState::Waiting, None)).unwrap();
        assert!(j.cancel_change(queued, 1_800_000_200, Some("alice")).unwrap());
        assert!(!j.start_change(queued).unwrap(), "cancelled: its job never runs");
        let (started, _) = j.intend_change(&change(UndoState::Waiting, None)).unwrap();
        assert!(j.start_change(started).unwrap());
        assert!(!j.cancel_change(started, 1_800_000_200, Some("alice")).unwrap(), "started: no cancel");
        let unsettled: Vec<ChangeId> = j.unsettled().unwrap().iter().map(|k| k.change.id).collect();
        assert_eq!(unsettled, [bobs, started], "running (bob's never applied, and the started one): for the start to settle");
        // Forgotten: gone, and its id never given again.
        j.forget_change(started).unwrap();
        assert!(j.change_entry(started).unwrap().is_none());
        let (next, _) = j.intend_change(&change(UndoState::Running, None)).unwrap();
        assert!(next.0 > started.0, "{next:?} after {started:?}");
        // A kept Change is never forgotten.
        j.forget_change(id).unwrap();
        assert!(j.change_entry(id).unwrap().is_some());
    }
}
