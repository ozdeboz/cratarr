//! The playlists (0.16, decision 0101 point 12): each one a row of its own, its entries rows of theirs. Until 0.15 they
//! were `.m3u` files in Music's playlist folder, with their entry ids, the entries' dates, their sharing and their
//! Subsonic dates in tables beside them; `import_playlists` brings them in once, those tables folded in. The store
//! (`crate::playlists::Store`) writes a `.m3u` copy out after each change and never reads one back.
//!
//! A playlist is addressed by its owner and its name, unique per owner whatever the case. Every write matches the new
//! lines to the entries kept (crate::playlist_ops: by order and content), so an entry keeps its id and when it came,
//! and is logged (decision 0087) in the same transaction.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::offline::{self, Op};
use super::{UserData, now, now_ms};
use crate::playlist_ops::{Entries, Entry, Gone};

pub const SCHEMA: &str = "
-- A playlist: its own id (`pl-` and 12 hex digits; a rename keeps it), whose (an account's name, '' the first
-- administrator's), its name, shared with everyone (decision 0051), when it was made and last changed (epoch s), and
-- once deleted when (epoch ms): kept so its Undo brings it back, for as long as removed entries are.
CREATE TABLE IF NOT EXISTS playlist (
  id TEXT PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, shared INTEGER NOT NULL DEFAULT 0,
  created INTEGER NOT NULL, changed INTEGER NOT NULL, gone INTEGER) WITHOUT ROWID;
CREATE UNIQUE INDEX IF NOT EXISTS playlist_named ON playlist (owner, name) WHERE gone IS NULL;
-- Its entries by their entry ids (crate::playlist_ops): the place (`n`, none once removed), the line (Music-relative),
-- when it came (epoch s; none for one held before the dates were kept), when it was last placed (an add or a move,
-- epoch ms; 0 for a line given its id by Cratarr), and once removed when (`gone`, epoch ms) and the entry before it
-- then (`after`).
CREATE TABLE IF NOT EXISTS playlist_item (
  playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER, line TEXT NOT NULL, added INTEGER,
  placed INTEGER NOT NULL DEFAULT 0, gone INTEGER, after TEXT, PRIMARY KEY (playlist, entry)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS playlist_item_gone ON playlist_item (gone);
-- When the `.m3u` files were brought in (epoch s), and how many playlists came: once.
CREATE TABLE IF NOT EXISTS playlist_import (one INTEGER PRIMARY KEY CHECK (one = 1), at INTEGER NOT NULL, found INTEGER NOT NULL);
";

/// A playlist as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub shared: bool,
    /// Epoch seconds.
    pub created: i64,
    pub changed: i64,
}

/// A playlist's key in Cratarr's own records (`Store::key`): its name in the first administrator's playlists,
/// `owner/name` in another's.
pub fn key_of(owner: &str, name: &str) -> String {
    if owner.is_empty() { name.to_string() } else { format!("{owner}/{name}") }
}

/// The owner and the name a key is of (`key_of`'s inverse): neither an account's name nor a playlist's holds a "/".
pub fn of_key(key: &str) -> (&str, &str) {
    key.split_once('/').unwrap_or(("", key))
}

/// A playlist's id: `pl-` and 12 hex digits from the system's random source.
pub fn new_id() -> String {
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).expect("the operating system's random source");
    format!("pl-{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

/// What a write found.
#[derive(Debug, PartialEq, Eq)]
pub enum Written {
    /// No such playlist.
    Unknown,
    /// It no longer held what the write expected: nothing was written.
    Stale,
    /// Written (or already so): the lines it held before.
    Done { before: Vec<String> },
}

/// What a rename or a playlist brought back found.
#[derive(Debug, PartialEq, Eq)]
pub enum Named {
    Unknown,
    /// Another of the owner's playlists has the name (whatever the case).
    Taken,
    /// Done: the playlist's lines.
    Done(Vec<String>),
}

const COLUMNS: &str = "id, owner, name, shared, created, changed";

fn playlist_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
    Ok(Playlist { id: r.get(0)?, owner: r.get(1)?, name: r.get(2)?, shared: r.get::<_, i64>(3)? != 0, created: r.get(4)?, changed: r.get(5)? })
}

/// The owner's playlist of that name, there now.
fn named(conn: &Connection, owner: &str, name: &str) -> Result<Option<Playlist>> {
    let mut st = conn.prepare_cached(&format!("SELECT {COLUMNS} FROM playlist WHERE owner = ?1 AND name = ?2 AND gone IS NULL"))?;
    Ok(st.query_row(params![owner, name], playlist_row).optional()?)
}

/// Whether another of the owner's playlists (not `except`) has `name`, whatever the case.
fn taken(conn: &Connection, owner: &str, name: &str, except: Option<&str>) -> Result<bool> {
    let mut st = conn.prepare_cached("SELECT id, name FROM playlist WHERE owner = ?1 AND gone IS NULL")?;
    let wanted = name.to_lowercase();
    let rows = st.query_map([owner], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, n) = row?;
        if Some(id.as_str()) != except && n.to_lowercase() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A playlist's lines in order.
fn lines_of(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut st = conn.prepare_cached("SELECT line FROM playlist_item WHERE playlist = ?1 AND n IS NOT NULL ORDER BY n")?;
    let rows = st.query_map([id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A playlist's entries as kept, and when each came.
fn entries_of(conn: &Connection, id: &str) -> Result<(Entries, HashMap<String, Option<i64>>)> {
    let mut st = conn.prepare_cached("SELECT entry, n, line, added, placed, gone, after FROM playlist_item WHERE playlist = ?1")?;
    let rows = st.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;
    let (mut live, mut e, mut added) = (Vec::new(), Entries::default(), HashMap::new());
    for row in rows {
        let (entry, n, line, came, placed, gone, after) = row?;
        added.insert(entry.clone(), came);
        match (n, gone) {
            (_, Some(at)) => {
                e.gone.insert(entry, Gone { line, after, at });
            }
            (Some(n), None) => {
                e.placed.insert(entry.clone(), placed);
                live.push((n, Entry { id: entry, line }));
            }
            (None, None) => {}
        }
    }
    live.sort_by_key(|(n, _)| *n);
    e.live = live.into_iter().map(|(_, x)| x).collect();
    Ok((e, added))
}

/// A playlist's entries kept as `e` has them: its rows replaced. An entry new to it came now (`at`, epoch s), one it
/// kept keeps its date.
fn keep(tx: &Transaction<'_>, id: &str, e: &Entries, added: &HashMap<String, Option<i64>>, at: i64) -> Result<()> {
    tx.execute("DELETE FROM playlist_item WHERE playlist = ?1", [id])?;
    let mut st = tx.prepare_cached(
        "INSERT INTO playlist_item (playlist, entry, n, line, added, placed, gone, after) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for (n, x) in e.live.iter().enumerate() {
        let came = added.get(&x.id).copied().unwrap_or(Some(at));
        let placed = e.placed.get(&x.id).copied().unwrap_or(0);
        st.execute(params![id, x.id, n as i64, x.line, came, placed, Option::<i64>::None, Option::<String>::None])?;
    }
    for (entry, g) in &e.gone {
        let came = added.get(entry).copied().flatten();
        st.execute(params![id, entry, Option::<i64>::None, g.line, came, 0i64, g.at, g.after])?;
    }
    Ok(())
}

/// Whether the playlist a key is of is shared: the one there now, else the last one of that key gone (its going is
/// logged as it was).
fn shared_key(conn: &Connection, key: &str) -> Result<bool> {
    let (owner, name) = of_key(key);
    let mut st =
        conn.prepare_cached("SELECT shared FROM playlist WHERE owner = ?1 AND name = ?2 ORDER BY gone IS NOT NULL, gone DESC LIMIT 1")?;
    Ok(st.query_row(params![owner, name], |r| r.get::<_, i64>(0)).optional()?.is_some_and(|s| s != 0))
}

/// A playlist's change logged (decision 0087): seen by others while it is shared (`shared`: its sharing changed, so
/// others see that).
pub(super) fn log_playlist(conn: &Connection, key: &str, op: Op, shared: bool) -> Result<()> {
    let shared = shared || shared_key(conn, key)?;
    offline::log(conn, of_key(key).0, offline::ChangeKind::Playlist, key, op, shared)
}

/// The lines of a playlist written into `tx`, its entries matched (or given: `given`), while it holds `expected`.
fn write(
    tx: &Transaction<'_>,
    owner: &str,
    name: &str,
    expected: Option<&[String]>,
    lines: &[String],
    given: Option<&Entries>,
) -> Result<Written> {
    let Some(p) = named(tx, owner, name)? else { return Ok(Written::Unknown) };
    let (mut e, added) = entries_of(tx, &p.id)?;
    let before = e.lines();
    if expected.is_some_and(|x| x != before.as_slice()) {
        return Ok(Written::Stale);
    }
    match given {
        Some(g) => e = g.clone(),
        None => e.matched(lines, now_ms(), crate::playlist_ops::new_id),
    }
    let at = now();
    keep(tx, &p.id, &e, &added, at)?;
    if e.lines() != before {
        tx.execute("UPDATE playlist SET changed = ?2 WHERE id = ?1", params![p.id, at])?;
        log_playlist(tx, &key_of(owner, name), Op::Changed, false)?;
    }
    Ok(Written::Done { before })
}

impl UserData {
    /// One owner's playlists ('' the first administrator's), with their lines.
    pub fn playlists(&self, owner: &str) -> Result<Vec<(Playlist, Vec<String>)>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!("SELECT {COLUMNS} FROM playlist WHERE owner = ?1 AND gone IS NULL"))?;
        let found: Vec<Playlist> = st.query_map([owner], playlist_row)?.collect::<rusqlite::Result<_>>()?;
        found
            .into_iter()
            .map(|p| {
                let lines = lines_of(&db, &p.id)?;
                Ok((p, lines))
            })
            .collect()
    }

    /// The owners who have playlists, the first administrator ('') among them when they do.
    pub fn playlist_owners(&self) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT DISTINCT owner FROM playlist WHERE gone IS NULL ORDER BY owner")?;
        Ok(st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// The owner's playlist of that name and its lines.
    pub fn playlist(&self, owner: &str, name: &str) -> Result<Option<(Playlist, Vec<String>)>> {
        let db = self.db();
        let Some(p) = named(&db, owner, name)? else { return Ok(None) };
        let lines = lines_of(&db, &p.id)?;
        Ok(Some((p, lines)))
    }

    /// A playlist's entries by their ids (decision 0087), and when each one there came (epoch s; none for one held
    /// before the dates were kept), in order.
    pub fn playlist_entries(&self, owner: &str, name: &str) -> Result<Option<(Entries, Vec<Option<i64>>)>> {
        let db = self.db();
        let Some(p) = named(&db, owner, name)? else { return Ok(None) };
        let (e, added) = entries_of(&db, &p.id)?;
        let dates = e.live.iter().map(|x| added.get(&x.id).copied().flatten()).collect();
        Ok(Some((e, dates)))
    }

    /// The playlist's lines become `lines`, only while it holds `expected` (`None`: whatever it holds).
    pub fn write_playlist(&self, owner: &str, name: &str, expected: Option<&[String]>, lines: &[String]) -> Result<Written> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let w = write(&tx, owner, name, expected, lines, None)?;
        tx.commit()?;
        Ok(w)
    }

    /// The playlist's entries become `e` (its operations applied, decision 0087), only while it holds `expected`; the
    /// operations' ids kept (`account`'s) and, for one refused, why: one sent again is not applied again, and a refused
    /// one is refused again.
    pub fn write_playlist_entries(
        &self,
        owner: &str,
        name: &str,
        expected: &[String],
        e: &Entries,
        account: &str,
        ops: &[(String, Option<String>)],
    ) -> Result<Written> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let w = write(&tx, owner, name, Some(expected), &e.lines(), Some(e))?;
        if matches!(w, Written::Done { .. }) {
            let mut st = tx.prepare_cached("INSERT OR IGNORE INTO playlist_op (account, op_id, at, why) VALUES (?1, ?2, ?3, ?4)")?;
            for (op, why) in ops {
                st.execute(params![account, op, now_ms(), why])?;
            }
        }
        tx.commit()?;
        Ok(w)
    }

    /// A new playlist of the owner's holding `lines`; false when another has the name (whatever the case).
    pub fn create_playlist(&self, owner: &str, name: &str, lines: &[String]) -> Result<bool> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if taken(&tx, owner, name, None)? {
            return Ok(false);
        }
        let (id, at) = (new_id(), now());
        tx.execute(
            "INSERT INTO playlist (id, owner, name, shared, created, changed) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![id, owner, name, at],
        )?;
        let mut e = Entries::default();
        e.matched(lines, now_ms(), crate::playlist_ops::new_id);
        keep(&tx, &id, &e, &HashMap::new(), at)?;
        tx.commit()?;
        Ok(true)
    }

    /// The owner's playlist `from` named `to` (a rename keeps everything else).
    pub fn rename_playlist(&self, owner: &str, from: &str, to: &str) -> Result<Named> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let Some(p) = named(&tx, owner, from)? else { return Ok(Named::Unknown) };
        if taken(&tx, owner, to, Some(&p.id))? {
            return Ok(Named::Taken);
        }
        tx.execute("UPDATE playlist SET name = ?2, changed = ?3 WHERE id = ?1", params![p.id, to, now()])?;
        let lines = lines_of(&tx, &p.id)?;
        tx.commit()?;
        Ok(Named::Done(lines))
    }

    /// The owner's playlist set aside (its Undo brings it back): the lines it held, and whether it was shared; none
    /// when there is no such playlist.
    pub fn delete_playlist(&self, owner: &str, name: &str) -> Result<Option<(Vec<String>, bool)>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let Some(p) = named(&tx, owner, name)? else { return Ok(None) };
        tx.execute("UPDATE playlist SET gone = ?2 WHERE id = ?1", params![p.id, now_ms()])?;
        let lines = lines_of(&tx, &p.id)?;
        tx.commit()?;
        Ok(Some((lines, p.shared)))
    }

    /// The owner's playlist of that name last set aside, brought back; `Unknown` when none was, `Taken` when another
    /// has the name now.
    pub fn restore_playlist(&self, owner: &str, name: &str) -> Result<Named> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if taken(&tx, owner, name, None)? {
            return Ok(Named::Taken);
        }
        let gone: Option<String> = tx
            .query_row(
                "SELECT id FROM playlist WHERE owner = ?1 AND name = ?2 AND gone IS NOT NULL ORDER BY gone DESC LIMIT 1",
                params![owner, name],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = gone else { return Ok(Named::Unknown) };
        tx.execute("UPDATE playlist SET gone = NULL, changed = ?2 WHERE id = ?1", params![id, now()])?;
        let lines = lines_of(&tx, &id)?;
        tx.commit()?;
        Ok(Named::Done(lines))
    }

    /// The playlists their owners share with everyone (decision 0051), by their keys (`Store::key`).
    pub fn shared_playlists(&self) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT owner, name FROM playlist WHERE shared = 1 AND gone IS NULL")?;
        let rows = st.query_map([], |r| Ok(key_of(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Shares a playlist (by its key) or stops; whether that changed anything. Logged (decision 0087): shared, everyone
    /// sees it come; no longer, its owner sees it changed (the others are told it is gone: `playlist_gone_for`). A
    /// playlist deleted (unshared as it goes) is not logged back.
    pub fn set_playlist_shared(&self, key: &str, on: bool) -> Result<bool> {
        let db = self.db();
        let (owner, name) = of_key(key);
        // The one there now, else the last one gone by that name (unshared as it goes).
        let id: Option<String> = db
            .query_row(
                "SELECT id FROM playlist WHERE owner = ?1 AND name = ?2 ORDER BY gone IS NOT NULL, gone DESC LIMIT 1",
                params![owner, name],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(false) };
        let n = db.execute("UPDATE playlist SET shared = ?2 WHERE id = ?1 AND shared <> ?2", params![id, on])?;
        let deleted = db
            .prepare_cached("SELECT 1 FROM change_log WHERE kind = 'playlist' AND item = ?1 AND op = 'deleted' AND seen_by = ''")?
            .exists([key])?;
        if n > 0 && !deleted {
            log_playlist(&db, key, Op::Changed, on)?;
        }
        Ok(n > 0)
    }

    /// Whether a playlist (by its key) is shared.
    pub fn is_playlist_shared(&self, key: &str) -> Result<bool> {
        shared_key(&self.db(), key)
    }

    /// A playlist came (`Op::Changed`: made, renamed to it, brought back) or went (deleted, renamed away), by its key;
    /// `shared`: others saw it.
    pub fn playlist_changed(&self, key: &str, op: Op, shared: bool) -> Result<()> {
        log_playlist(&self.db(), key, op, shared)
    }

    /// Whether the `.m3u` files were brought in already.
    pub fn playlists_imported(&self) -> Result<bool> {
        Ok(self.db().prepare_cached("SELECT 1 FROM playlist_import")?.exists([])?)
    }

    /// The `.m3u` files brought in, once (0.16): each owner's playlists by name, with their lines and when their file
    /// last changed (epoch s). What was kept beside the files goes with them: each line's entry id, when it came, the
    /// sharing, and the Subsonic dates. Those tables go. Returns how many playlists came; none when they were brought
    /// in already.
    pub fn import_playlists(&self, found: &[(String, String, Vec<String>, i64)]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if tx.prepare_cached("SELECT 1 FROM playlist_import")?.exists([])? {
            return Ok(0);
        }
        let has = |table: &str| -> Result<bool> {
            Ok(tx.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?.exists([table])?)
        };
        let (ids, dates, shares, seen) = (has("playlist_entry_id")?, has("playlist_entry")?, has("playlist_shared")?, has("playlist_seen")?);
        let mut came = 0;
        for (owner, wanted, lines, stamp) in found {
            if taken(&tx, owner, wanted, None)? {
                tracing::warn!("playlist {wanted} of {owner:?}: another has its name but for the case; left out");
                continue;
            }
            let key = key_of(owner, wanted);
            let (created, changed) = if seen {
                tx.query_row("SELECT created, changed FROM playlist_seen WHERE name = ?1", [&key], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
                    .unwrap_or((*stamp, *stamp))
            } else {
                (*stamp, *stamp)
            };
            let shared = shares && tx.prepare("SELECT 1 FROM playlist_shared WHERE playlist = ?1")?.exists([&key])?;
            let id = new_id();
            tx.execute(
                "INSERT INTO playlist (id, owner, name, shared, created, changed) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, owner, wanted, shared, created, changed],
            )?;
            // Its entries by the ids kept for them, matched to the file as it is now.
            let mut e = Entries::default();
            if ids {
                let mut st = tx.prepare("SELECT entry, n, line, placed, gone, after FROM playlist_entry_id WHERE playlist = ?1")?;
                let rows = st.query_map([&key], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<String>>(5)?))
                })?;
                let mut live = Vec::new();
                for row in rows {
                    let (entry, n, line, placed, gone, after) = row?;
                    match (n, gone) {
                        (_, Some(at)) => {
                            e.gone.insert(entry, Gone { line, after, at });
                        }
                        (Some(n), None) => {
                            e.placed.insert(entry.clone(), placed);
                            live.push((n, Entry { id: entry, line }));
                        }
                        (None, None) => {}
                    }
                }
                live.sort_by_key(|(n, _)| *n);
                e.live = live.into_iter().map(|(_, x)| x).collect();
            }
            e.matched(lines, now_ms(), crate::playlist_ops::new_id);
            // When each came: kept by the n-th copy of its line.
            let mut added: HashMap<String, Option<i64>> = HashMap::new();
            let mut copies: HashMap<&str, i64> = HashMap::new();
            for x in &e.live {
                let n = copies.entry(x.line.as_str()).or_default();
                let at = if dates {
                    tx.prepare_cached("SELECT added FROM playlist_entry WHERE playlist = ?1 AND entry = ?2 AND n = ?3")?
                        .query_row(params![key, x.line, *n], |r| r.get::<_, i64>(0))
                        .optional()?
                } else {
                    None
                };
                *n += 1;
                added.insert(x.id.clone(), at);
            }
            keep(&tx, &id, &e, &added, now())?;
            came += 1;
        }
        tx.execute_batch(
            "DROP TABLE IF EXISTS playlist_entry_id;
             DROP TABLE IF EXISTS playlist_entry;
             DROP TABLE IF EXISTS playlist_shared;
             DROP TABLE IF EXISTS playlist_seen;",
        )?;
        tx.execute("INSERT INTO playlist_import (one, at, found) VALUES (1, ?1, ?2)", params![now(), came as i64])?;
        tx.commit()?;
        Ok(came)
    }
}

/// Removed entries forgotten past their time (`cut`, epoch ms), and playlists set aside then.
pub(super) fn forget_old(tx: &Transaction<'_>, cut_ms: i64) -> Result<usize> {
    let mut n = tx.execute("DELETE FROM playlist_item WHERE gone < ?1", [cut_ms])?;
    n += tx.execute("DELETE FROM playlist_item WHERE playlist IN (SELECT id FROM playlist WHERE gone < ?1)", [cut_ms])?;
    n += tx.execute("DELETE FROM playlist WHERE gone < ?1", [cut_ms])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_write_keeps_each_entrys_id_and_date_and_is_logged() {
        let u = UserData::open(None).unwrap();
        assert!(u.create_playlist("", "Evening", &v(&["a", "b"])).unwrap());
        assert!(!u.create_playlist("", "evening", &[]).unwrap(), "one name per owner, whatever the case");
        assert!(u.create_playlist("ann", "Evening", &[]).unwrap(), "another's is another");
        let (first, dates) = u.playlist_entries("", "Evening").unwrap().unwrap();
        assert_eq!((first.lines(), dates.iter().all(Option::is_some)), (v(&["a", "b"]), true));
        // b first, a gone, c new: b keeps its id and its date.
        assert_eq!(u.write_playlist("", "Evening", Some(&v(&["a", "b"])), &v(&["b", "c"])).unwrap(), Written::Done { before: v(&["a", "b"]) });
        let (again, _) = u.playlist_entries("", "Evening").unwrap().unwrap();
        assert_eq!(again.live[0].id, first.live[1].id);
        assert!(again.gone.contains_key(&first.live[0].id), "a is remembered as removed");
        assert_eq!(u.write_playlist("", "Evening", Some(&v(&["a"])), &[]).unwrap(), Written::Stale, "not over an edit it has not seen");
        assert_eq!(u.write_playlist("", "Morning", None, &[]).unwrap(), Written::Unknown);
        let logged: i64 = u.db().query_row("SELECT count(*) FROM change_log WHERE item = 'Evening'", [], |r| r.get(0)).unwrap();
        assert_eq!(logged, 1, "its change logged, once per item");
    }

    #[test]
    fn renamed_deleted_and_brought_back_it_keeps_its_id_and_entries() {
        let u = UserData::open(None).unwrap();
        u.create_playlist("", "Evening", &v(&["a"])).unwrap();
        u.create_playlist("", "Morning", &[]).unwrap();
        let id = u.playlist("", "Evening").unwrap().unwrap().0.id;
        assert_eq!(u.rename_playlist("", "Evening", "MORNING").unwrap(), Named::Taken);
        assert_eq!(u.rename_playlist("", "Evening", "EVENING").unwrap(), Named::Done(v(&["a"])), "its own name in other capitals");
        assert_eq!(u.playlist("", "EVENING").unwrap().unwrap().0.id, id);
        assert!(u.set_playlist_shared("EVENING", true).unwrap());
        assert_eq!(u.shared_playlists().unwrap(), ["EVENING".to_string()].into());
        assert_eq!(u.delete_playlist("", "EVENING").unwrap(), Some((v(&["a"]), true)));
        assert!(u.is_playlist_shared("EVENING").unwrap(), "it went shared");
        assert!(u.playlist("", "EVENING").unwrap().is_none());
        assert_eq!(u.restore_playlist("", "EVENING").unwrap(), Named::Done(v(&["a"])));
        assert_eq!(u.playlist("", "EVENING").unwrap().unwrap().0.id, id, "the same playlist back");
        assert_eq!(u.restore_playlist("", "EVENING").unwrap(), Named::Taken, "there again: none brought back over it");
        assert_eq!(u.restore_playlist("", "Never").unwrap(), Named::Unknown, "nothing set aside by that name");
    }

    #[test]
    fn the_files_and_what_was_kept_beside_them_are_brought_in_once() {
        let u = UserData::open(None).unwrap();
        {
            let db = u.db();
            db.execute_batch(
                "CREATE TABLE playlist_entry_id (playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER, line TEXT NOT NULL,
                   placed INTEGER NOT NULL DEFAULT 0, gone INTEGER, after TEXT, PRIMARY KEY (playlist, entry));
                 CREATE TABLE playlist_entry (playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER NOT NULL, added INTEGER NOT NULL,
                   PRIMARY KEY (playlist, entry, n));
                 CREATE TABLE playlist_shared (playlist TEXT PRIMARY KEY, at INTEGER NOT NULL);
                 CREATE TABLE playlist_seen (name TEXT PRIMARY KEY, print TEXT NOT NULL, created INTEGER NOT NULL, changed INTEGER NOT NULL);
                 INSERT INTO playlist_entry_id VALUES ('ann/Evening', 'e-a', 0, 'A/1.mp3', 5, NULL, NULL);
                 INSERT INTO playlist_entry_id VALUES ('ann/Evening', 'e-x', NULL, 'X/1.mp3', 0, 7, 'e-a');
                 INSERT INTO playlist_entry VALUES ('ann/Evening', 'A/1.mp3', 0, 100);
                 INSERT INTO playlist_shared VALUES ('ann/Evening', 1);
                 INSERT INTO playlist_seen VALUES ('ann/Evening', '', 10, 20);",
            )
            .unwrap();
        }
        let found = vec![
            ("ann".to_string(), "Evening".to_string(), v(&["A/1.mp3", "B/2.mp3"]), 50),
            (String::new(), "Chill".to_string(), v(&["A/1.mp3"]), 60),
        ];
        assert_eq!(u.import_playlists(&found).unwrap(), 2);
        assert_eq!(u.import_playlists(&found).unwrap(), 0, "once");
        let (p, lines) = u.playlist("ann", "Evening").unwrap().unwrap();
        assert_eq!((p.shared, p.created, p.changed, lines), (true, 10, 20, v(&["A/1.mp3", "B/2.mp3"])));
        let (e, dates) = u.playlist_entries("ann", "Evening").unwrap().unwrap();
        assert_eq!((e.live[0].id.as_str(), e.placed["e-a"]), ("e-a", 5), "its entry id kept");
        assert_eq!(e.gone["e-x"].after.as_deref(), Some("e-a"), "and the one removed");
        assert_eq!(dates[0], Some(100), "when it came");
        assert_eq!(dates[1], None, "a line its file gained elsewhere: no date, as before");
        let (chill, _) = u.playlist("", "Chill").unwrap().unwrap();
        assert_eq!((chill.created, chill.shared), (60, false), "no dates kept: its file's");
        assert!(!u.db().prepare("SELECT 1 FROM sqlite_master WHERE name = 'playlist_entry_id'").unwrap().exists([]).unwrap(), "the old tables go");
    }
}
