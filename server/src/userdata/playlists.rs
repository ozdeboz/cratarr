//! The playlists (0.16, decision 0101 point 12): each one a row of its own, its entries rows of theirs. Until 0.15 they
//! were `.m3u` files in Music's playlist folder, with their entry ids, the entries' dates, their sharing and their
//! Subsonic dates in tables beside them; `import_playlists` brings them in once, those tables folded in. The store
//! (`crate::playlists::Store`) writes a `.m3u` copy out after each change and never reads one back.
//!
//! A playlist is known by its id everywhere outside its store (the API, Subsonic, the change log, favourites, its
//! spoken names), so a rename changes nothing there; its store addresses it by its owner and its name, unique per
//! owner whatever the case. The keys 0.15 knew it by (its name, `owner/name` another's) lead to its id
//! (`playlist_moved`). Every write matches the new lines to the entries kept (crate::playlist_ops: by order and
//! content), so an entry keeps its id and when it came, and is logged (decision 0087) in the same transaction. An
//! entry is the record's track (decision 0101 point 11), its line where its file was last seen: one whose file went is
//! kept, missing, with its track, and leads to the track's file again where it is found (`relink_playlists`).

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
-- Its entries by their entry ids (crate::playlist_ops): the place (`n`, none once removed), the line (Music-relative:
-- where its file was last seen), its track (the record's track id as served, `tr-N`, decision 0101: whichever copy
-- plays; none while no file of it was ever found), when it came (epoch s; none for one held before the dates were kept), when it was last placed (an add or a move,
-- epoch ms; 0 for a line given its id by Cratarr), and once removed when (`gone`, epoch ms) and the entry before it
-- then (`after`).
CREATE TABLE IF NOT EXISTS playlist_item (
  playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER, line TEXT NOT NULL, track TEXT, added INTEGER,
  placed INTEGER NOT NULL DEFAULT 0, gone INTEGER, after TEXT, PRIMARY KEY (playlist, entry)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS playlist_item_gone ON playlist_item (gone);
-- The keys 0.15 knew a playlist by (its name; `owner/name` another account's; a Subsonic id's name from before a
-- rename), each leading to its id.
CREATE TABLE IF NOT EXISTS playlist_moved (old TEXT PRIMARY KEY, id TEXT NOT NULL) WITHOUT ROWID;
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

/// The key 0.15 knew a playlist by: its name in the first administrator's playlists, `owner/name` in another's.
fn old_key(owner: &str, name: &str) -> String {
    if owner.is_empty() { name.to_string() } else { format!("{owner}/{name}") }
}

/// Whether `id` has the shape of a playlist's id (`new_id`).
pub fn is_id(id: &str) -> bool {
    id.strip_prefix("pl-").is_some_and(|h| h.len() == 12 && h.bytes().all(|b| b.is_ascii_hexdigit()))
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

/// A playlist by its id, there now or set aside.
fn by_id(conn: &Connection, id: &str) -> Result<Option<(Playlist, bool)>> {
    let mut st = conn.prepare_cached(&format!("SELECT {COLUMNS}, gone IS NOT NULL FROM playlist WHERE id = ?1"))?;
    Ok(st.query_row([id], |r| Ok((playlist_row(r)?, r.get::<_, bool>(6)?))).optional()?)
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

/// What a playlist's rows keep beside its entries, by entry id: when each came, and its track.
#[derive(Default)]
struct Kept {
    added: HashMap<String, Option<i64>>,
    tracks: HashMap<String, Option<String>>,
}

/// The tracks lines name now (`Store`'s catalogue's answer), by line; a line not in it names none.
pub type Tracks = HashMap<String, Option<String>>;

/// A playlist's entries as kept, and when each came and its track.
fn entries_of(conn: &Connection, id: &str) -> Result<(Entries, Kept)> {
    let mut st = conn.prepare_cached("SELECT entry, n, line, added, placed, gone, after, track FROM playlist_item WHERE playlist = ?1")?;
    let rows = st.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    })?;
    let (mut live, mut e, mut kept) = (Vec::new(), Entries::default(), Kept::default());
    for row in rows {
        let (entry, n, line, came, placed, gone, after, track) = row?;
        kept.added.insert(entry.clone(), came);
        kept.tracks.insert(entry.clone(), track);
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
    Ok((e, kept))
}

/// A playlist's entries kept as `e` has them: its rows replaced. An entry new to it came now (`at`, epoch s), one it
/// kept keeps its date. Its track is the one its line names now (`tracks`), else the one it had (its file gone, it is
/// that track's still).
fn keep(tx: &Transaction<'_>, id: &str, e: &Entries, kept: &Kept, tracks: &Tracks, at: i64) -> Result<()> {
    tx.execute("DELETE FROM playlist_item WHERE playlist = ?1", [id])?;
    let mut st = tx.prepare_cached(
        "INSERT INTO playlist_item (playlist, entry, n, line, track, added, placed, gone, after) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    let had = |entry: &str| kept.tracks.get(entry).cloned().flatten();
    for (n, x) in e.live.iter().enumerate() {
        let came = kept.added.get(&x.id).copied().unwrap_or(Some(at));
        let placed = e.placed.get(&x.id).copied().unwrap_or(0);
        let track = tracks.get(&x.line).cloned().flatten().or_else(|| had(&x.id));
        st.execute(params![id, x.id, n as i64, x.line, track, came, placed, Option::<i64>::None, Option::<String>::None])?;
    }
    for (entry, g) in &e.gone {
        let came = kept.added.get(entry).copied().flatten();
        st.execute(params![id, entry, Option::<i64>::None, g.line, had(entry), came, 0i64, g.at, g.after])?;
    }
    Ok(())
}

/// Whose a playlist is (by its id): an account's name, '' the first administrator's.
pub(super) fn owner_of(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(by_id(conn, id)?.map(|(p, _)| p.owner))
}

/// A playlist's change logged by its id (decision 0087), under its owner: seen by others while it is shared, or went
/// shared (`shared`: its sharing changed, so others see that).
pub(super) fn log_playlist(conn: &Connection, id: &str, op: Op, shared: bool) -> Result<()> {
    let Some((p, _)) = by_id(conn, id)? else { return Ok(()) };
    offline::log(conn, &p.owner, offline::ChangeKind::Playlist, id, op, shared || p.shared)
}

/// The lines of a playlist written into `tx`, its entries matched (or given: `given`), while it holds `expected`.
fn write(
    tx: &Transaction<'_>,
    owner: &str,
    name: &str,
    expected: Option<&[String]>,
    lines: &[String],
    given: Option<&Entries>,
    tracks: &Tracks,
) -> Result<Written> {
    let Some(p) = named(tx, owner, name)? else { return Ok(Written::Unknown) };
    let (mut e, kept) = entries_of(tx, &p.id)?;
    let before = e.lines();
    if expected.is_some_and(|x| x != before.as_slice()) {
        return Ok(Written::Stale);
    }
    match given {
        Some(g) => e = g.clone(),
        None => e.matched(lines, now_ms(), crate::playlist_ops::new_id),
    }
    let at = now();
    keep(tx, &p.id, &e, &kept, tracks, at)?;
    if e.lines() != before {
        tx.execute("UPDATE playlist SET changed = ?2 WHERE id = ?1", params![p.id, at])?;
        log_playlist(tx, &p.id, Op::Changed, false)?;
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

    /// A playlist by its id, while it is there.
    pub fn playlist_by_id(&self, id: &str) -> Result<Option<Playlist>> {
        Ok(by_id(&self.db(), id)?.filter(|(_, gone)| !gone).map(|(p, _)| p))
    }

    /// The id of the owner's playlist of that name: the one there now, else the last one of that name set aside.
    pub fn playlist_id(&self, owner: &str, name: &str) -> Result<Option<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id FROM playlist WHERE owner = ?1 AND name = ?2 ORDER BY gone IS NOT NULL, gone DESC LIMIT 1")?;
        Ok(st.query_row(params![owner, name], |r| r.get(0)).optional()?)
    }

    /// The id a key 0.15 knew a playlist by leads to (`playlist_moved`).
    pub fn moved_playlist(&self, old: &str) -> Result<Option<String>> {
        Ok(self.db().prepare_cached("SELECT id FROM playlist_moved WHERE old = ?1")?.query_row([old], |r| r.get(0)).optional()?)
    }

    /// Every key 0.15 knew a playlist by and the id it leads to: for what a page kept of them.
    pub fn playlists_moved(&self) -> Result<HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT old, id FROM playlist_moved")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// The owner's playlist of that name and its lines.
    pub fn playlist(&self, owner: &str, name: &str) -> Result<Option<(Playlist, Vec<String>)>> {
        let db = self.db();
        let Some(p) = named(&db, owner, name)? else { return Ok(None) };
        let lines = lines_of(&db, &p.id)?;
        Ok(Some((p, lines)))
    }

    /// The tracks of a playlist's entries there now, in order (decision 0101 point 11; none for one whose line never
    /// named a track's file).
    pub fn playlist_tracks(&self, owner: &str, name: &str) -> Result<Option<Vec<Option<String>>>> {
        let db = self.db();
        let Some(p) = named(&db, owner, name)? else { return Ok(None) };
        let mut st = db.prepare_cached("SELECT track FROM playlist_item WHERE playlist = ?1 AND n IS NOT NULL ORDER BY n")?;
        let rows = st.query_map([&p.id], |r| r.get::<_, Option<String>>(0))?;
        Ok(Some(rows.collect::<rusqlite::Result<_>>()?))
    }

    /// A playlist's entries by their ids (decision 0087), and when each one there came (epoch s; none for one held
    /// before the dates were kept), in order.
    pub fn playlist_entries(&self, owner: &str, name: &str) -> Result<Option<(Entries, Vec<Option<i64>>)>> {
        let db = self.db();
        let Some(p) = named(&db, owner, name)? else { return Ok(None) };
        let (e, kept) = entries_of(&db, &p.id)?;
        let dates = e.live.iter().map(|x| kept.added.get(&x.id).copied().flatten()).collect();
        Ok(Some((e, dates)))
    }

    /// The playlist's lines become `lines` (their tracks `tracks`), only while it holds `expected` (`None`: whatever it
    /// holds).
    pub fn write_playlist(&self, owner: &str, name: &str, expected: Option<&[String]>, lines: &[String], tracks: &Tracks) -> Result<Written> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let w = write(&tx, owner, name, expected, lines, None, tracks)?;
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
        tracks: &Tracks,
    ) -> Result<Written> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let w = write(&tx, owner, name, Some(expected), &e.lines(), Some(e), tracks)?;
        if matches!(w, Written::Done { .. }) {
            let mut st = tx.prepare_cached("INSERT OR IGNORE INTO playlist_op (account, op_id, at, why) VALUES (?1, ?2, ?3, ?4)")?;
            for (op, why) in ops {
                st.execute(params![account, op, now_ms(), why])?;
            }
        }
        tx.commit()?;
        Ok(w)
    }

    /// A new playlist of the owner's holding `lines` (their tracks `tracks`): its id; none when another has the name
    /// (whatever the case).
    pub fn create_playlist(&self, owner: &str, name: &str, lines: &[String], tracks: &Tracks) -> Result<Option<String>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if taken(&tx, owner, name, None)? {
            return Ok(None);
        }
        let (id, at) = (new_id(), now());
        tx.execute(
            "INSERT INTO playlist (id, owner, name, shared, created, changed) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![id, owner, name, at],
        )?;
        let mut e = Entries::default();
        e.matched(lines, now_ms(), crate::playlist_ops::new_id);
        keep(&tx, &id, &e, &Kept::default(), tracks, at)?;
        tx.commit()?;
        Ok(Some(id))
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

    /// The playlists their owners share with everyone (decision 0051), by their ids.
    pub fn shared_playlists(&self) -> Result<HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT id FROM playlist WHERE shared = 1 AND gone IS NULL")?;
        Ok(st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Shares a playlist (by its id, there or set aside: unshared as it goes) or stops; whether that changed anything.
    /// Logged (decision 0087): shared, everyone sees it come; no longer, its owner sees it changed (the others are told it
    /// is gone: `playlist_gone_for`). A playlist deleted is not logged back.
    pub fn set_playlist_shared(&self, id: &str, on: bool) -> Result<bool> {
        let db = self.db();
        let n = db.execute("UPDATE playlist SET shared = ?2 WHERE id = ?1 AND shared <> ?2", params![id, on])?;
        let deleted = db
            .prepare_cached("SELECT 1 FROM change_log WHERE kind = 'playlist' AND item = ?1 AND op = 'deleted' AND seen_by = ''")?
            .exists([id])?;
        if n > 0 && !deleted {
            log_playlist(&db, id, Op::Changed, on)?;
        }
        Ok(n > 0)
    }

    /// Whether a playlist (by its id) is shared, or went shared.
    pub fn is_playlist_shared(&self, id: &str) -> Result<bool> {
        Ok(by_id(&self.db(), id)?.is_some_and(|(p, _)| p.shared))
    }

    /// Whose a playlist is (by its id): an account's name, '' the first administrator's.
    pub fn playlist_owner(&self, id: &str) -> Result<Option<String>> {
        owner_of(&self.db(), id)
    }

    /// A playlist came (`Op::Changed`: made, renamed, brought back) or went (deleted), by its id; `shared`: others saw
    /// it.
    pub fn playlist_changed(&self, id: &str, op: Op, shared: bool) -> Result<()> {
        log_playlist(&self.db(), id, op, shared)
    }

    /// Whether the `.m3u` files were brought in already.
    pub fn playlists_imported(&self) -> Result<bool> {
        Ok(self.db().prepare_cached("SELECT 1 FROM playlist_import")?.exists([])?)
    }

    /// The `.m3u` files brought in, once (0.16): each owner's playlists by name, with their lines and when their file
    /// last changed (epoch s). What was kept beside the files goes with them: each line's entry id, when it came, the
    /// sharing, and the Subsonic dates. Those tables go. Returns how many playlists came; none when they were brought
    /// in already.
    pub fn import_playlists(&self, found: &[(String, String, Vec<String>, i64)], tracks: &Tracks) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if tx.prepare_cached("SELECT 1 FROM playlist_import")?.exists([])? {
            return Ok(0);
        }
        let has = |table: &str| -> Result<bool> {
            Ok(tx.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?.exists([table])?)
        };
        let (ids, dates, shares, seen) = (has("playlist_entry_id")?, has("playlist_entry")?, has("playlist_shared")?, has("playlist_seen")?);
        let aliases = has("playlist_alias")?;
        // The migration report's part (decision 0101): every table's rows before and after, what found no match.
        let rows = |table: &str, exists: bool| -> Result<i64> {
            Ok(if exists { tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))? } else { 0 })
        };
        let old: Vec<(&str, i64)> = vec![
            ("playlist_entry_id", rows("playlist_entry_id", ids)?),
            ("playlist_entry", rows("playlist_entry", dates)?),
            ("playlist_shared", rows("playlist_shared", shares)?),
            ("playlist_seen", rows("playlist_seen", seen)?),
            ("playlist_alias", rows("playlist_alias", aliases)?),
        ];
        let logged: i64 = tx.query_row("SELECT count(*) FROM change_log WHERE kind = 'playlist'", [], |r| r.get(0))?;
        let hearts = |db: &Transaction<'_>| -> Result<i64> { Ok(db.query_row("SELECT count(*) FROM annotation WHERE kind = 'playlist'", [], |r| r.get(0))?) };
        let hearts_before = hearts(&tx)?;
        let (mut came, mut lines_in, mut tracked, mut moved_hearts) = (0usize, 0usize, 0usize, 0usize);
        let (mut no_track, mut left_out): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        let mut per_owner: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for (owner, wanted, lines, stamp) in found {
            if taken(&tx, owner, wanted, None)? {
                tracing::warn!("playlist {wanted} of {owner:?}: another has its name but for the case; left out");
                left_out.push(old_key(owner, wanted));
                continue;
            }
            lines_in += lines.len();
            for line in lines {
                if tracks.get(line).cloned().flatten().is_some() {
                    tracked += 1;
                } else {
                    no_track.push(line.clone());
                }
            }
            let key = old_key(owner, wanted);
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
            keep(&tx, &id, &e, &Kept { added, tracks: HashMap::new() }, tracks, now())?;
            // The key it was known by, and a Subsonic id's names from before its renames, lead to it; a favourite of it
            // is of its id.
            tx.execute("INSERT OR REPLACE INTO playlist_moved (old, id) VALUES (?1, ?2)", params![key, id])?;
            if aliases {
                tx.execute("INSERT OR IGNORE INTO playlist_moved (old, id) SELECT old, ?2 FROM playlist_alias WHERE name = ?1", params![key, id])?;
            }
            moved_hearts += tx.execute("UPDATE OR IGNORE annotation SET item = ?2 WHERE kind = 'playlist' AND item = ?1", params![key, id])?;
            *per_owner.entry(owner.clone()).or_default() += 1;
            came += 1;
        }
        // The change log named playlists by their old keys: what devices kept of them is read again (each starts fresh
        // at 0.16 anyway, decision 0101).
        let last: Option<i64> = tx.query_row("SELECT max(rev) FROM change_log", [], |r| r.get(0))?;
        tx.execute("DELETE FROM change_log WHERE kind = 'playlist'", [])?;
        if let Some(last) = last {
            tx.execute(
                "INSERT INTO change_log_floor (one, rev) VALUES (1, ?1) ON CONFLICT (one) DO UPDATE SET rev = max(rev, excluded.rev)",
                [last],
            )?;
        }
        tx.execute_batch(
            "DROP TABLE IF EXISTS playlist_entry_id;
             DROP TABLE IF EXISTS playlist_entry;
             DROP TABLE IF EXISTS playlist_shared;
             DROP TABLE IF EXISTS playlist_seen;
             DROP TABLE IF EXISTS playlist_alias;",
        )?;
        tx.execute("INSERT INTO playlist_import (one, at, found) VALUES (1, ?1, ?2)", params![now(), came as i64])?;
        // The report's part, last in the import's transaction.
        let count = |table: &str| -> Result<i64> { Ok(tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?) };
        let mut tables = vec![
            serde_json::json!({ "name": "playlist", "before": 0, "after": count("playlist")? }),
            serde_json::json!({ "name": "playlist_item", "before": 0, "after": count("playlist_item")? }),
            serde_json::json!({ "name": "playlist_moved", "before": 0, "after": count("playlist_moved")? }),
        ];
        tables.extend(old.iter().map(|(name, before)| serde_json::json!({ "name": name, "before": before, "after": 0 })));
        tables.push(serde_json::json!({ "name": "change_log (playlists)", "before": logged, "after": 0 }));
        tables.push(serde_json::json!({ "name": "annotation (playlist favourites)", "before": hearts_before, "after": hearts(&tx)? }));
        let unmatched = |what: &str, all: &[String]| serde_json::json!({ "what": what, "count": all.len(), "sample": all.iter().take(10).collect::<Vec<_>>() });
        let stray: Vec<String> = {
            let mut st = tx.prepare("SELECT item FROM annotation WHERE kind = 'playlist' AND item NOT LIKE 'pl-%'")?;
            st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?
        };
        let report = serde_json::json!({
            "tables": tables,
            "unmatched": [
                unmatched("lines naming no track's file (kept, missing)", &no_track),
                unmatched("playlists left out (another has the name but for the case)", &left_out),
                unmatched("favourites of no playlist brought in", &stray),
            ],
            "playlists": came,
            "per_owner": per_owner,
            "lines": lines_in,
            "lines_with_a_track": tracked,
            "favourites_moved": moved_hearts,
        });
        super::put_migration_part(&tx, "playlists", &report)?;
        tx.commit()?;
        Ok(came)
    }
}

impl UserData {
    /// The owner's entries there now that have a track, by playlist: (playlist name, entry, line, track). What the
    /// minute pass looks at where a line's file went (`relink_playlists`).
    pub fn tracked_entries(&self, owner: &str) -> Result<Vec<(String, String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT p.name, i.entry, i.line, i.track FROM playlist_item i JOIN playlist p ON p.id = i.playlist
             WHERE p.owner = ?1 AND p.gone IS NULL AND i.n IS NOT NULL AND i.track IS NOT NULL",
        )?;
        let rows = st.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The owner's entries there now without a track, by playlist: (playlist name, entry, line). What the minute pass
    /// gives a track once the catalogue knows its line's file (one written before the catalogue read it, at the
    /// import of a library not read yet).
    pub fn untracked_entries(&self, owner: &str) -> Result<Vec<(String, String, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached(
            "SELECT p.name, i.entry, i.line FROM playlist_item i JOIN playlist p ON p.id = i.playlist
             WHERE p.owner = ?1 AND p.gone IS NULL AND i.n IS NOT NULL AND i.track IS NULL",
        )?;
        let rows = st.query_map([owner], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Entries given the tracks their lines name now (`found`: playlist name, entry, track). Their lines stay as they
    /// are, so nothing about the playlist changes for anyone.
    pub fn give_tracks(&self, owner: &str, found: &[(String, String, String)]) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut n = 0;
        for (name, entry, track) in found {
            let Some(p) = named(&tx, owner, name)? else { continue };
            n += tx.execute("UPDATE playlist_item SET track = ?3 WHERE playlist = ?1 AND entry = ?2 AND track IS NULL", params![p.id, entry, track])?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// Entries led to where their track's file is now (`relinks`: playlist name, entry, the new line), each playlist
    /// changed and logged once. Returns the names of the playlists changed, with their lines now.
    pub fn relink_playlists(&self, owner: &str, relinks: &[(String, String, String)]) -> Result<Vec<(String, Vec<String>)>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut changed: Vec<String> = Vec::new();
        for (name, entry, line) in relinks {
            let Some(p) = named(&tx, owner, name)? else { continue };
            let n = tx.execute("UPDATE playlist_item SET line = ?3 WHERE playlist = ?1 AND entry = ?2 AND n IS NOT NULL", params![p.id, entry, line])?;
            if n > 0 && !changed.contains(&p.id) {
                tx.execute("UPDATE playlist SET changed = ?2 WHERE id = ?1", params![p.id, now()])?;
                log_playlist(&tx, &p.id, Op::Changed, false)?;
                changed.push(p.id.clone());
            }
        }
        let mut out = Vec::new();
        for id in changed {
            let name: String = tx.query_row("SELECT name FROM playlist WHERE id = ?1", [&id], |r| r.get(0))?;
            out.push((name, lines_of(&tx, &id)?));
        }
        tx.commit()?;
        Ok(out)
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

    /// Lines that name no track.
    fn none() -> Tracks {
        Tracks::new()
    }

    #[test]
    fn an_entry_is_its_track_kept_when_its_file_goes_and_led_to_where_it_is_now() {
        let u = UserData::open(None).unwrap();
        let tracks: Tracks = [("A/1.mp3".to_string(), Some("tr-1".to_string()))].into();
        u.create_playlist("", "Evening", &v(&["A/1.mp3", "B/2.mp3"]), &tracks).unwrap();
        let tracked = |u: &UserData| u.tracked_entries("").unwrap().into_iter().map(|(n, _, l, t)| (n, l, t)).collect::<Vec<_>>();
        assert_eq!(tracked(&u), [("Evening".to_string(), "A/1.mp3".to_string(), "tr-1".to_string())], "a line with no track's file is none's");
        // Its file gone (its line names no track now): still that track's.
        u.write_playlist("", "Evening", None, &v(&["A/1.mp3", "B/2.mp3", "C/3.mp3"]), &none()).unwrap();
        let kept = u.tracked_entries("").unwrap();
        assert_eq!(kept.len(), 1, "it keeps its track");
        // Its track's file found elsewhere: the entry leads there, its playlist changed once.
        let changed = u.relink_playlists("", &[("Evening".into(), kept[0].1.clone(), "A2/1.mp3".into())]).unwrap();
        assert_eq!(changed, [("Evening".to_string(), v(&["A2/1.mp3", "B/2.mp3", "C/3.mp3"]))]);
        assert_eq!(tracked(&u)[0].1, "A2/1.mp3");
    }

    #[test]
    fn a_write_keeps_each_entrys_id_and_date_and_is_logged() {
        let u = UserData::open(None).unwrap();
        let id = u.create_playlist("", "Evening", &v(&["a", "b"]), &none()).unwrap().unwrap();
        assert!(is_id(&id), "{id}");
        assert!(u.create_playlist("", "evening", &[], &none()).unwrap().is_none(), "one name per owner, whatever the case");
        assert!(u.create_playlist("ann", "Evening", &[], &none()).unwrap().is_some(), "another's is another");
        let (first, dates) = u.playlist_entries("", "Evening").unwrap().unwrap();
        assert_eq!((first.lines(), dates.iter().all(Option::is_some)), (v(&["a", "b"]), true));
        // b first, a gone, c new: b keeps its id and its date.
        assert_eq!(u.write_playlist("", "Evening", Some(&v(&["a", "b"])), &v(&["b", "c"]), &none()).unwrap(), Written::Done { before: v(&["a", "b"]) });
        let (again, _) = u.playlist_entries("", "Evening").unwrap().unwrap();
        assert_eq!(again.live[0].id, first.live[1].id);
        assert!(again.gone.contains_key(&first.live[0].id), "a is remembered as removed");
        assert_eq!(u.write_playlist("", "Evening", Some(&v(&["a"])), &[], &none()).unwrap(), Written::Stale, "not over an edit it has not seen");
        assert_eq!(u.write_playlist("", "Morning", None, &[], &none()).unwrap(), Written::Unknown);
        let logged: i64 = u.db().query_row("SELECT count(*) FROM change_log WHERE item = ?1 AND account = ''", [&id], |r| r.get(0)).unwrap();
        assert_eq!(logged, 1, "its change logged by its id, once, under its owner");
    }

    #[test]
    fn renamed_deleted_and_brought_back_it_keeps_its_id_and_entries() {
        let u = UserData::open(None).unwrap();
        u.create_playlist("", "Evening", &v(&["a"]), &none()).unwrap();
        u.create_playlist("", "Morning", &[], &none()).unwrap();
        let id = u.playlist("", "Evening").unwrap().unwrap().0.id;
        assert_eq!(u.rename_playlist("", "Evening", "MORNING").unwrap(), Named::Taken);
        assert_eq!(u.rename_playlist("", "Evening", "EVENING").unwrap(), Named::Done(v(&["a"])), "its own name in other capitals");
        assert_eq!(u.playlist("", "EVENING").unwrap().unwrap().0.id, id);
        assert!(u.set_playlist_shared(&id, true).unwrap());
        assert_eq!(u.shared_playlists().unwrap(), [id.clone()].into());
        assert_eq!(u.delete_playlist("", "EVENING").unwrap(), Some((v(&["a"]), true)));
        assert!(u.is_playlist_shared(&id).unwrap(), "it went shared");
        assert!(u.playlist("", "EVENING").unwrap().is_none());
        assert!(u.playlist_by_id(&id).unwrap().is_none(), "set aside");
        assert_eq!(u.playlist_id("", "EVENING").unwrap().as_deref(), Some(id.as_str()), "the one set aside");
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
                 INSERT INTO playlist_seen VALUES ('ann/Evening', '', 10, 20);
                 CREATE TABLE playlist_alias (old TEXT PRIMARY KEY, name TEXT NOT NULL, at INTEGER NOT NULL);
                 INSERT INTO playlist_alias VALUES ('Calm', 'Chill', 1);
                 INSERT INTO annotation (user, item, kind, starred) VALUES ('me', 'Chill', 'playlist', 1);
                 INSERT INTO change_log (account, kind, item, op, at) VALUES ('', 'playlist', 'Chill', 'changed', 1);",
            )
            .unwrap();
        }
        let found = vec![
            ("ann".to_string(), "Evening".to_string(), v(&["A/1.mp3", "B/2.mp3"]), 50),
            (String::new(), "Chill".to_string(), v(&["A/1.mp3"]), 60),
        ];
        assert_eq!(u.import_playlists(&found, &none()).unwrap(), 2);
        assert_eq!(u.import_playlists(&found, &none()).unwrap(), 0, "once");
        let (p, lines) = u.playlist("ann", "Evening").unwrap().unwrap();
        assert_eq!((p.shared, p.created, p.changed, lines), (true, 10, 20, v(&["A/1.mp3", "B/2.mp3"])));
        let (e, dates) = u.playlist_entries("ann", "Evening").unwrap().unwrap();
        assert_eq!((e.live[0].id.as_str(), e.placed["e-a"]), ("e-a", 5), "its entry id kept");
        assert_eq!(e.gone["e-x"].after.as_deref(), Some("e-a"), "and the one removed");
        assert_eq!(dates[0], Some(100), "when it came");
        assert_eq!(dates[1], None, "a line its file gained elsewhere: no date, as before");
        // The migration report's part (decision 0101): the tables before and after, what found no match.
        let report = u.migration_report().unwrap().remove("playlists").expect("its part").1;
        assert_eq!((report["playlists"].as_u64(), report["lines"].as_u64(), report["favourites_moved"].as_u64()), (Some(2), Some(3), Some(1)));
        let table = |name: &str| report["tables"].as_array().unwrap().iter().find(|t| t["name"] == name).cloned().unwrap();
        assert_eq!((table("playlist_entry_id")["before"].as_i64(), table("playlist_entry_id")["after"].as_i64()), (Some(2), Some(0)));
        assert_eq!(table("playlist")["after"].as_i64(), Some(2));
        assert_eq!(report["unmatched"][0]["count"].as_u64(), Some(3), "no line names a track's file here");
        assert_eq!(report["per_owner"]["ann"].as_u64(), Some(1));
        let (chill, _) = u.playlist("", "Chill").unwrap().unwrap();
        assert_eq!((chill.created, chill.shared), (60, false), "no dates kept: its file's");
        // What knew them by their keys knows them by their ids.
        let moved = u.playlists_moved().unwrap();
        assert_eq!((moved["Chill"].as_str(), moved["Calm"].as_str(), moved["ann/Evening"].as_str()), (chill.id.as_str(), chill.id.as_str(), p.id.as_str()));
        assert_eq!(u.starred("me", super::super::Kind::Playlist).unwrap(), [chill.id.clone()], "a favourite of its id");
        let logged: i64 = u.db().query_row("SELECT count(*) FROM change_log WHERE kind = 'playlist'", [], |r| r.get(0)).unwrap();
        assert_eq!(logged, 0, "the change log's old keys gone: devices read the playlists again");
        assert!(!u.db().prepare("SELECT 1 FROM sqlite_master WHERE name = 'playlist_entry_id'").unwrap().exists([]).unwrap(), "the old tables go");
    }
}
