//! What user data names by a track id that has gone, moved to the id that took its place (decision 0101: one id per track of
//! a record, so an id never splits between copies). One place for it, as two come here: a copy taking a record's tracks at a
//! build (`Serve::built_now`: mostly fresh ids nothing names, but a retired id can come back with its plays), and every pair
//! at once at the first start of 0.16.
//!
//! A table's rows that meet under one key fold by that table's rule: read, group, fold, delete, insert, as `UPDATE OR IGNORE`
//! would lose what clashes. Safe to repeat: once moved, a row names no old id. The first start moves the artists and the albums
//! by the same rules (`walk`).

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::{Transaction, params, params_from_iter};

use super::albums::{Albums, move_albums};
use super::artists::move_artists;
use super::offline::{ChangeKind, Op, item_id, log};
use super::preferences::move_filters;
use super::{Annotation, UserData, now_ms, put_migration_part};
use crate::catalogue::First;
use crate::moved::{Moved, Report, Table};
use crate::serve::ids;

/// A table's row: its values, in the order of `Columns`.
pub(super) type Row = Vec<Value>;

/// A table's columns, for a rule to find its fields by name.
pub(super) struct Columns(Vec<String>);

impl Columns {
    pub(super) fn at(&self, name: &str) -> usize {
        self.0.iter().position(|c| c == name).unwrap_or_else(|| panic!("no column {name}"))
    }
}

/// A row's number in a column, none where it holds none.
pub(super) fn int(row: &[Value], at: usize) -> Option<i64> {
    match row[at] {
        Value::Integer(v) => Some(v),
        _ => None,
    }
}

fn number(v: Option<i64>) -> Value {
    v.map_or(Value::Null, Value::Integer)
}

/// How rows of a table that meet under one key become one: `into` is the winner's own where it had one, else the first met.
pub(super) type Merge = fn(&Columns, &mut Row, &Row);

/// `into` takes `field` where it holds nothing: a row's own says what it is, and only an empty one takes another's.
fn or_else(c: &Columns, into: &mut Row, from: &Row, field: &str) {
    let at = c.at(field);
    if into[at] == Value::Null {
        into[at] = from[at].clone();
    }
}

/// `combine`'s annotations: plays added up, the later dates, the better rating, the first star. The pid is the winner's.
pub(super) fn annotations(c: &Columns, into: &mut Row, from: &Row) {
    let (count, date, own, own_date, rating, starred, at) =
        (c.at("play_count"), c.at("play_date"), c.at("own_count"), c.at("own_date"), c.at("rating"), c.at("starred"), c.at("starred_at"));
    let read = |r: &Row| Annotation {
        play_count: int(r, count).unwrap_or(0).max(0) as u32,
        play_date: int(r, date),
        own_count: int(r, own).unwrap_or(0).max(0) as u32,
        own_date: int(r, own_date),
        rating: int(r, rating).unwrap_or(0).clamp(0, 5) as u8,
        starred_at: (int(r, starred).unwrap_or(0) != 0).then(|| int(r, at).unwrap_or(0)),
    };
    let both = read(into).combined(&read(from));
    into[count] = Value::Integer(both.play_count.into());
    into[date] = number(both.play_date);
    into[own] = Value::Integer(both.own_count.into());
    into[own_date] = number(both.own_date);
    into[rating] = Value::Integer(both.rating.into());
    into[starred] = Value::Integer(both.starred_at.is_some().into());
    into[at] = number(both.starred_at);
    or_else(c, into, from, "pid");
}

/// What an import brought in, as an annotation's: plays added up, the later date, a star on either, the better rating. The
/// server's item and the pid are the winner's.
pub(super) fn imports(c: &Columns, into: &mut Row, from: &Row) {
    let sum = |into: &mut Row, field: &str| {
        let at = c.at(field);
        into[at] = Value::Integer(int(into, at).unwrap_or(0) + int(from, at).unwrap_or(0));
    };
    let most = |into: &mut Row, field: &str| {
        let at = c.at(field);
        into[at] = number(int(into, at).max(int(from, at)));
    };
    sum(into, "plays");
    most(into, "played");
    most(into, "starred");
    most(into, "rating");
    most(into, "at");
    or_else(c, into, from, "server_item");
    or_else(c, into, from, "pid");
}

/// The row with the later `field` stands, whole, under the winner's key.
pub(super) fn later(c: &Columns, into: &mut Row, from: &Row, field: &str, key: &str) {
    let (at, key) = (c.at(field), c.at(key));
    if int(from, at) > int(into, at) {
        let winner = into[key].clone();
        *into = from.clone();
        into[key] = winner;
    }
}

/// `later` by when a bookmark changed.
fn bookmarks(c: &Columns, into: &mut Row, from: &Row) {
    later(c, into, from, "changed", "item");
}

/// `later` by when a track's words were set.
fn words(c: &Columns, into: &mut Row, from: &Row) {
    later(c, into, from, "at", "track");
}

/// `later` by when a source said a subject's word, or was last asked of it.
pub(super) fn said(c: &Columns, into: &mut Row, from: &Row) {
    later(c, into, from, "at", "subject");
}

/// The last change of an item: the later of the two.
pub(super) fn last_change(c: &Columns, into: &mut Row, from: &Row) {
    let at = c.at("at");
    into[at] = number(int(into, at).max(int(from, at)));
}

/// The ids moved, as stored, in a table a statement can join: one pass over a table with no index on them.
fn hold(tx: &Transaction, moved: &HashMap<String, String>) -> Result<()> {
    tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS moved_ids (old TEXT PRIMARY KEY, id TEXT NOT NULL) WITHOUT ROWID; DELETE FROM temp.moved_ids;")?;
    let mut st = tx.prepare_cached("INSERT INTO temp.moved_ids (old, id) VALUES (?1, ?2)")?;
    for (old, id) in moved {
        st.execute(params![old, id])?;
    }
    Ok(())
}

/// Rows of `table` whose `key` names a moved id, `only` saying which of them name such ids at all, fold onto what it moved to.
/// How many moved. Each table's `moved` is as that table stores the id.
fn fold(tx: &Transaction, table: &str, key: &str, only: &str, moved: &HashMap<String, String>, merge: Merge) -> Result<u64> {
    fold_pairs(tx, table, key, only, &in_order(moved), merge)
}

/// A map as a list, by its old ids: the same each time.
pub(super) fn in_order(moved: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = moved.iter().map(|(old, new)| (old.clone(), new.clone())).collect();
    pairs.sort();
    pairs
}

/// `fold` by a list: rows meet in its order, so the first pair's is the first met, and an old id on two pairs goes to both, whole.
fn fold_pairs(tx: &Transaction, table: &str, key: &str, only: &str, pairs: &[(String, String)], mut merge: impl FnMut(&Columns, &mut Row, &Row)) -> Result<u64> {
    hold(tx, &pairs.iter().cloned().collect())?;
    let info: Vec<(String, bool)> = {
        let mut st = tx.prepare("SELECT name, pk > 0 FROM pragma_table_info(?1) ORDER BY cid")?;
        st.query_map([table], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };
    let columns = Columns(info.iter().map(|(name, _)| name.clone()).collect());
    let names = columns.0.join(", ");
    let keys: Vec<usize> = info.iter().enumerate().filter(|(_, (_, pk))| *pk).map(|(i, _)| i).collect();
    let at_key = keys.iter().enumerate().map(|(n, i)| format!("{} = ?{}", columns.0[*i], n + 1)).collect::<Vec<_>>().join(" AND ");
    let row_of = |r: &rusqlite::Row<'_>| (0..columns.0.len()).map(|i| r.get::<_, Value>(i)).collect::<rusqlite::Result<Row>>();
    let rows: Vec<Row> = {
        let mut st = tx.prepare(&format!("SELECT {names} FROM {table} WHERE {only} AND {key} IN (SELECT old FROM temp.moved_ids)"))?;
        st.query_map([], row_of)?.collect::<rusqlite::Result<_>>()?
    };
    let key_at = columns.at(key);
    let values = |row: &Row| keys.iter().map(|i| row[*i].clone()).collect::<Vec<_>>();
    let mut del = tx.prepare(&format!("DELETE FROM {table} WHERE {at_key}"))?;
    let mut own = tx.prepare(&format!("SELECT {names} FROM {table} WHERE {at_key}"))?;
    let mut held: HashMap<&str, Vec<&Row>> = HashMap::new();
    for row in &rows {
        // A row leaves only once it is known where it goes.
        if let Value::Text(old) = &row[key_at] {
            del.execute(params_from_iter(values(row)))?;
            held.entry(old).or_default().push(row);
        }
    }
    let mut met: BTreeMap<String, Row> = BTreeMap::new();
    for (old, to) in pairs {
        for row in held.get(old.as_str()).into_iter().flatten() {
            let mut row = (*row).clone();
            row[key_at] = Value::Text(to.clone());
            let at = format!("{:?}", values(&row));
            if let Some(into) = met.get_mut(&at) {
                merge(&columns, into, &row);
                continue;
            }
            // The winner's own row first: the fold keeps what is its own.
            let mine = own.query_map(params_from_iter(values(&row)), row_of)?.next().transpose()?;
            met.insert(
                at,
                match mine {
                    Some(mut into) => {
                        merge(&columns, &mut into, &row);
                        into
                    }
                    None => row,
                },
            );
        }
    }
    let marks = vec!["?"; columns.0.len()].join(", ");
    let mut put = tx.prepare(&format!("INSERT OR REPLACE INTO {table} ({names}) VALUES ({marks})"))?;
    for row in met.values() {
        put.execute(params_from_iter(row.iter()))?;
    }
    Ok(rows.len() as u64)
}

/// Where a table holds ids of one kind: what the report calls it, which rows they are, how they say the id, and what becomes
/// of two rows that meet.
#[derive(Clone, Copy)]
pub(super) struct Place {
    pub name: &'static str,
    pub table: &'static str,
    /// The column that names the id.
    pub key: &'static str,
    /// Which of the table's rows hold such ids at all.
    pub only: &'static str,
    /// What comes before the id in `key`.
    pub prefix: &'static str,
    pub merge: Merge,
    /// What two rows can say differently where one goes with its row: the report counts those (`Report::differed`). A row
    /// that says nothing there loses nothing.
    pub differs: &'static [&'static str],
}

/// A place's rows fold by `pairs`, in their order, and the report says what came of each. An id for life has moved already: a
/// folder run on this build wrote under it. One no pair names stays as it is, and is counted.
pub(super) fn walk(tx: &Transaction, place: &Place, pairs: &[(String, String)], report: &mut Report) -> Result<()> {
    let Place { name, table, key, only, prefix, merge, differs } = *place;
    let found: Vec<String> = tx.prepare(&format!("SELECT {key} FROM {table} WHERE {only} ORDER BY {key}"))?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let olds: HashSet<&str> = pairs.iter().map(|(old, _)| old.as_str()).collect();
    let (mut moved, mut unmatched) = (0, Vec::new());
    for id in &found {
        let id = id.strip_prefix(prefix).unwrap_or(id);
        if olds.contains(id) || ids::for_life(id) {
            moved += 1;
        } else {
            unmatched.push(id.to_string());
        }
    }
    let stored: Vec<(String, String)> = pairs.iter().map(|(old, new)| (format!("{prefix}{old}"), format!("{prefix}{new}"))).collect();
    let (mut merged, mut differed) = (0, Vec::new());
    fold_pairs(tx, table, key, only, &stored, |c, into, from| {
        merged += 1;
        merge(c, into, from);
        let lost = |field: &&str| {
            let at = c.at(field);
            from[at] != Value::Null && from[at] != into[at]
        };
        if let (true, Value::Text(id)) = (differs.iter().any(lost), &from[c.at(key)]) {
            differed.push(id.strip_prefix(prefix).unwrap_or(id).to_string());
        }
    })?;
    let after: i64 = tx.query_row(&format!("SELECT count(*) FROM {table} WHERE {only}"), [], |r| r.get(0))?;
    report.tables.push(Table { name: name.to_string(), before: found.len() as u64, after: after as u64, moved, merged });
    report.unmatched(name, unmatched);
    if !differs.is_empty() {
        report.differed(name, differed);
    }
    Ok(())
}

/// A column with no key to meet under: every row names what its id moved to, in one pass.
fn plain(tx: &Transaction, table: &str, column: &str) -> Result<u64> {
    let sql = format!("UPDATE {table} SET {column} = (SELECT id FROM temp.moved_ids WHERE old = {table}.{column}) WHERE {column} IN (SELECT old FROM temp.moved_ids)");
    Ok(tx.execute(&sql, [])? as u64)
}

/// The apps' saved queues: a list each, a place in it staying the place it was.
fn queues(tx: &Transaction, moved: &HashMap<String, String>) -> Result<u64> {
    let saved: Vec<(String, String, String)> = {
        let mut st = tx.prepare("SELECT user, client, songs FROM app_queue")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    let mut n = 0;
    for (user, client, songs) in saved {
        let now = songs.split(',').map(|s| moved.get(s).map_or(s, String::as_str)).collect::<Vec<_>>().join(",");
        if now != songs {
            tx.execute("UPDATE app_queue SET songs = ?3 WHERE user = ?1 AND client = ?2", params![user, client, now])?;
            n += 1;
        }
    }
    Ok(n)
}

/// A device told of a heart on the old id hears it went, and came to the new one, in order. What is no longer on says nothing:
/// the new id's own row says what it is.
fn log_moved(tx: &Transaction, moved: &HashMap<String, String>) -> Result<u64> {
    hold(tx, moved)?;
    let logged: Vec<(String, String, String)> = {
        let mut st = tx.prepare(&format!("SELECT account, kind, item FROM change_log WHERE op = '{}' AND seen_by = '' AND item IN (SELECT old FROM temp.moved_ids)", Op::Changed.as_str()))?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (account, kind, item) in &logged {
        let (Some(kind), Some(to)) = (ChangeKind::of(kind), moved.get(item)) else { continue };
        log(tx, account, kind, item, Op::Deleted, false)?;
        log(tx, account, kind, to, Op::Changed, false)?;
    }
    Ok(logged.len() as u64)
}

/// `UserData::move_tracks` in the caller's transaction, as the first start's is one for all of the user data.
fn move_tracks(tx: &Transaction, moved: &[(i64, i64)]) -> Result<Vec<(&'static str, u64)>> {
    if moved.is_empty() {
        return Ok(Vec::new());
    }
    let song = crate::serve::ids::song;
    let songs: HashMap<String, String> = moved.iter().map(|(old, new)| (song(*old), song(*new))).collect();
    let own = crate::source::cratarr::KIND.id;
    let items: HashMap<String, String> = songs.iter().map(|(old, new)| (item_id("track", own, old), item_id("track", own, new))).collect();
    let mut counts = vec![
        ("annotation", fold(tx, "annotation", "item", "kind = 'song'", &songs, annotations)?),
        ("imported", fold(tx, "imported", "item", "kind = 'song'", &songs, imports)?),
        ("bookmark", fold(tx, "bookmark", "item", "1", &songs, bookmarks)?),
        ("track_words", fold(tx, "track_words", "track", "1", &songs, words)?),
        ("change_at", fold(tx, "change_at", "item", "1", &items, last_change)?),
        ("change_log", log_moved(tx, &items)?),
        ("app_queue", queues(tx, &songs)?),
    ];
    hold(tx, &songs)?;
    for (table, column) in [("plays", "song"), ("scrobble_queue", "song"), ("playlist_item", "track"), ("visual_rating", "track")] {
        counts.push((table, plain(tx, table, column)?));
    }
    counts.retain(|(_, n)| *n > 0);
    Ok(counts)
}

/// Where tracks are named, as the report says each: its name there, `move_tracks`' for it, and which rows.
const TRACK_PLACES: [(&str, &str, &str); 11] = [
    ("annotation: tracks", "annotation", "annotation WHERE kind = 'song'"),
    ("imported: tracks", "imported", "imported WHERE kind = 'song'"),
    ("bookmark", "bookmark", "bookmark"),
    ("track_words", "track_words", "track_words"),
    ("change_at: tracks", "change_at", "change_at WHERE item LIKE 'track:%'"),
    ("change_log: tracks", "change_log", "change_log WHERE item LIKE 'track:%'"),
    ("app_queue", "app_queue", "app_queue"),
    ("plays", "plays", "plays"),
    ("scrobble_queue", "scrobble_queue", "scrobble_queue"),
    ("playlist_item", "playlist_item", "playlist_item"),
    ("visual_rating", "visual_rating", "visual_rating"),
];

/// `move_tracks` as the report reads it: every place's rows before and after, none moved too, so it says it looked.
fn tracks_reported(tx: &Transaction, moved: &[(i64, i64)]) -> Result<Report> {
    let rows = |from: &str| -> Result<u64> { Ok(tx.query_row(&format!("SELECT count(*) FROM {from}"), [], |r| r.get::<_, i64>(0))? as u64) };
    let before: Vec<u64> = TRACK_PLACES.iter().map(|(_, _, from)| rows(from)).collect::<Result<_>>()?;
    let counts: HashMap<&str, u64> = move_tracks(tx, moved)?.into_iter().collect();
    let mut report = Report::default();
    for ((name, key, from), before) in TRACK_PLACES.iter().zip(before) {
        let after = rows(from)?;
        report.tables.push(Table { name: name.to_string(), before, after, moved: counts.get(key).copied().unwrap_or(0), merged: before.saturating_sub(after) });
    }
    Ok(report)
}

/// What the first start's last step moves by.
pub struct Moves<'a> {
    pub albums: &'a Albums,
    /// Each artist's old id to the new, as `Catalogue::artists_moved` gives them.
    pub artists: &'a HashMap<String, String>,
    /// (what went, what took it).
    pub tracks: &'a [(i64, i64)],
    /// What a preference's JSON moves by.
    pub stored: &'a Moved,
}

/// In `meta`, by step: what the first start's steps before the last found. No part of the report can be written until it is
/// whole, and a start that went over them again would report what is done as nothing moved.
const FOUND: &str = "first_start:";

/// The report's part that says the first start is over.
const PART: &str = "records";

impl UserData {
    /// Everything that names one of these tracks, (old, new), moves to the new one, in one transaction: a play or a
    /// favourite on the old id is on the new id after, and none is lost or counted twice. Rows moved by table, the tables
    /// with none left out. Blocking.
    pub fn move_tracks(&self, moved: &[(i64, i64)]) -> Result<Vec<(&'static str, u64)>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let counts = move_tracks(&tx, moved)?;
        tx.commit()?;
        Ok(counts)
    }

    /// Whether the first start is over: its last step's commit says so, and nothing else does. Blocking.
    pub fn first_start_done(&self) -> Result<bool> {
        Ok(self.db().prepare_cached("SELECT 1 FROM migration_report WHERE part = ?1")?.exists([PART])?)
    }

    /// What a step of the first start found at an earlier start. Blocking.
    pub fn step_found(&self, step: &str) -> Result<Option<Report>> {
        Ok(self.meta(&format!("{FOUND}{step}"))?.map(|kept| serde_json::from_str(&kept)).transpose()?)
    }

    /// Keeps what a step found: it is done. Blocking.
    pub fn keep_step_found(&self, step: &str, found: &Report) -> Result<()> {
        self.set_meta(&format!("{FOUND}{step}"), &serde_json::to_string(found)?)
    }

    /// The first start's last step, in one transaction: every album, artist and track the user data names moves, and the
    /// report goes in last, `steps` what the steps before found and `first` the catalogue's. Its commit is the migration's. Blocking.
    pub fn finish_move(&self, moves: &Moves<'_>, steps: Report, first: &First) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let mut report = steps;
        report.extend(move_albums(&tx, moves.albums)?);
        report.extend(move_artists(&tx, moves.artists)?);
        report.extend(tracks_reported(&tx, moves.tracks)?);
        report.extend(move_filters(&tx, moves.stored, now_ms())?);
        tx.execute("DELETE FROM meta WHERE key LIKE ?1", [format!("{FOUND}%")])?;
        let mut part = serde_json::to_value(first)?;
        if let (Some(part), serde_json::Value::Object(found)) = (part.as_object_mut(), serde_json::to_value(&report)?) {
            part.extend(found);
        }
        put_migration_part(&tx, PART, &part)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Heard, Kind};
    use super::*;

    /// A user data with these statements run: what tests below start from.
    fn with(sql: &str) -> UserData {
        let u = UserData::open(None).unwrap();
        u.db().execute_batch(sql).unwrap();
        u
    }

    fn one<T: rusqlite::types::FromSql>(u: &UserData, sql: &str) -> T {
        u.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    const EVERYWHERE: &str = "
      INSERT INTO annotation (user, item, kind, pid, play_count, play_date, rating, starred, starred_at, own_count, own_date) VALUES ('me', 'tr-1', 'song', 'p1', 3, 100, 4, 1, 50, 3, 100);
      INSERT INTO imported (account, source, server_user, kind, item, server_item, pid, plays, played, starred, rating, at) VALUES ('me', 'jf', 'u', 'song', 'tr-1', 'j-1', 'p1', 2, 90, 1, 3, 10);
      INSERT INTO bookmark (user, item, pid, position_ms, created, changed) VALUES ('me', 'tr-1', 'p1', 5000, 1, 1);
      INSERT INTO plays (user, song, pid, at, ended, counted, event) VALUES ('me', 'tr-1', 'p1', 100, 'finished', 1, 'e1');
      INSERT INTO plays (user, song, pid, at, ended, counted, event) VALUES ('you', 'tr-1', 'p1', 101, 'finished', 1, 'e2');
      INSERT INTO scrobble_queue (user, target, song, at) VALUES ('me', 'lastfm', 'tr-1', 100);
      INSERT INTO track_words (track, kind, words, at) VALUES ('tr-1', 'genre', '[\"Dub\"]', 1);
      INSERT INTO playlist (id, owner, name, created, changed) VALUES ('pl-1', '', 'One', 1, 1);
      INSERT INTO playlist_item (playlist, entry, n, line, track) VALUES ('pl-1', 'e', 0, 'A/1.flac', 'tr-1');
      INSERT INTO app_queue (user, client, songs, current, changed) VALUES ('me', 'App', 'tr-7,tr-1,tr-8', 1, 1);
      INSERT INTO visual_rating (user, at, a, b, choice, track) VALUES ('me', 1, '{}', '{}', 'a', 'tr-1');
      INSERT INTO change_at (account, kind, item, at) VALUES ('me', 'favourite', 'track:cratarr/tr-1', 7);
      INSERT INTO change_log (account, kind, item, op, at) VALUES ('me', 'favourite', 'track:cratarr/tr-1', 'changed', 7);
    ";

    /// `data §5`, table by table: nothing names the old id after, and every row came along.
    #[test]
    fn moved_tracks_keep_every_reference() {
        let u = with(EVERYWHERE);
        let counts: HashMap<&str, u64> = u.move_tracks(&[(1, 2)]).unwrap().into_iter().collect();
        let tables = [("annotation", 1), ("imported", 1), ("bookmark", 1), ("plays", 2), ("scrobble_queue", 1), ("track_words", 1), ("playlist_item", 1), ("app_queue", 1), ("visual_rating", 1), ("change_at", 1), ("change_log", 1)];
        assert_eq!(counts, tables.into_iter().collect(), "the report's numbers");
        for (table, column) in [("annotation", "item"), ("imported", "item"), ("bookmark", "item"), ("plays", "song"), ("scrobble_queue", "song"), ("track_words", "track"), ("playlist_item", "track"), ("visual_rating", "track")] {
            assert_eq!(one::<i64>(&u, &format!("SELECT count(*) FROM {table} WHERE {column} = 'tr-1'")), 0, "{table}");
            assert!(one::<i64>(&u, &format!("SELECT count(*) FROM {table} WHERE {column} = 'tr-2'")) > 0, "{table}");
        }
        let a = u.get("me", Kind::Song, &["tr-2".to_string()]).unwrap().remove("tr-2").unwrap();
        assert_eq!((a.play_count, a.own_count, a.rating, a.starred_at), (3, 3, 4, Some(50)));
        assert_eq!(u.app_queues("me").unwrap()[0].songs, ["tr-7", "tr-2", "tr-8"]);
        assert_eq!(u.app_queues("me").unwrap()[0].current, Some(1), "its place is the place it was");
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM plays WHERE event IN ('e1', 'e2') AND song = 'tr-2'"), 2, "a play keeps its event: a report sent again stays one row");
        assert_eq!(one::<i64>(&u, "SELECT at FROM change_at WHERE item = 'track:cratarr/tr-2'"), 7);
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM change_at WHERE item = 'track:cratarr/tr-1'"), 0);
    }

    #[test]
    fn two_annotations_add_up_and_the_winners_pid_stays() {
        let u = with(
            "INSERT INTO annotation (user, item, kind, pid, play_count, play_date, rating, starred, starred_at, own_count, own_date) VALUES
               ('me', 'tr-1', 'song', 'old', 3, 100, 2, 1, 50, 1, 40),
               ('me', 'tr-2', 'song', 'new', 2, 200, 4, 1, 90, 2, 200),
               ('you', 'tr-1', 'song', 'old', 1, 10, 0, 0, NULL, 1, 10);",
        );
        u.move_tracks(&[(1, 2)]).unwrap();
        let a = u.get("me", Kind::Song, &["tr-2".to_string()]).unwrap().remove("tr-2").unwrap();
        assert_eq!((a.play_count, a.play_date, a.own_count, a.own_date, a.rating, a.starred_at), (5, Some(200), 3, Some(200), 4, Some(50)));
        assert_eq!(one::<String>(&u, "SELECT pid FROM annotation WHERE user = 'me'"), "new");
        assert_eq!(one::<i64>(&u, "SELECT play_count FROM annotation WHERE user = 'you' AND item = 'tr-2'"), 1, "each account's own");
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM annotation"), 2);
    }

    /// Two old ids on one track: every copy's plays reach it, whichever row is met first.
    #[test]
    fn several_ids_onto_one_add_up() {
        let u = with("INSERT INTO annotation (user, item, kind, play_count, own_count) VALUES ('me', 'tr-1', 'song', 1, 1), ('me', 'tr-2', 'song', 2, 2), ('me', 'tr-3', 'song', 4, 4);");
        u.move_tracks(&[(1, 3), (2, 3)]).unwrap();
        assert_eq!(one::<i64>(&u, "SELECT play_count FROM annotation WHERE item = 'tr-3'"), 7);
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM annotation"), 1);
        let u = with("INSERT INTO annotation (user, item, kind, play_count, own_count) VALUES ('me', 'tr-1', 'song', 1, 1), ('me', 'tr-2', 'song', 2, 2);");
        u.move_tracks(&[(1, 3), (2, 3)]).unwrap();
        assert_eq!(one::<i64>(&u, "SELECT play_count FROM annotation WHERE item = 'tr-3'"), 3, "the new id with no row of its own");
    }

    /// A star with no date reads as 0, which says nothing of when: of two the earlier stands, and that is the one with a date.
    #[test]
    fn of_two_stars_a_date_stands_over_none() {
        for (first, second) in [("NULL", "90"), ("90", "NULL")] {
            let u = with(&format!("INSERT INTO annotation (user, item, kind, starred, starred_at) VALUES ('me', 'tr-1', 'song', 1, {first}), ('me', 'tr-2', 'song', 1, {second});"));
            u.move_tracks(&[(1, 2)]).unwrap();
            assert_eq!(one::<i64>(&u, "SELECT starred_at FROM annotation WHERE item = 'tr-2'"), 90, "{first}, {second}");
        }
    }

    #[test]
    fn of_two_bookmarks_the_later_stays() {
        let u = with(
            "INSERT INTO bookmark (user, item, pid, position_ms, comment, created, changed) VALUES ('me', 'tr-1', 'p', 5, 'old place', 1, 300), ('me', 'tr-2', 'p', 9, 'new place', 2, 200);",
        );
        u.move_tracks(&[(1, 2)]).unwrap();
        let kept = u.bookmarks("me").unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].item.as_str(), kept[0].position_ms, kept[0].comment.as_str()), ("tr-2", 5, "old place"), "the one last changed, under the id that stays");
    }

    #[test]
    fn two_imports_add_up_and_the_server_item_is_the_winners() {
        let u = with(
            "INSERT INTO imported (account, source, server_user, kind, item, server_item, pid, plays, played, starred, rating, at) VALUES
               ('me', 'jf', 'u', 'song', 'tr-1', 'j-1', 'old', 2, 90, 1, 3, 10),
               ('me', 'jf', 'u', 'song', 'tr-2', 'j-2', 'new', 5, 70, 0, 1, 20);",
        );
        u.move_tracks(&[(1, 2)]).unwrap();
        let row: (String, String, i64, i64, i64, i64) = u
            .db()
            .query_row("SELECT server_item, pid, plays, played, starred, rating FROM imported", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .unwrap();
        assert_eq!(row, ("j-2".to_string(), "new".to_string(), 7, 90, 1, 3));
    }

    #[test]
    fn of_two_tracks_words_the_later_stay() {
        let u = with(
            "INSERT INTO track_words (track, kind, words, at) VALUES ('tr-1', 'genre', '[\"Old\"]', 1), ('tr-2', 'genre', '[\"New\"]', 2), ('tr-1', 'mood', '[\"Warm\"]', 9), ('tr-2', 'mood', '[\"Cold\"]', 3);",
        );
        u.move_tracks(&[(1, 2)]).unwrap();
        let kept: Vec<(String, String, String)> = u.db().prepare("SELECT track, kind, words FROM track_words ORDER BY kind").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(kept, [("tr-2".into(), "genre".into(), "[\"New\"]".into()), ("tr-2".into(), "mood".into(), "[\"Warm\"]".into())]);
    }

    #[test]
    fn of_two_last_changes_the_later_stands() {
        let u = with("INSERT INTO change_at (account, kind, item, at) VALUES ('me', 'favourite', 'track:cratarr/tr-1', 300), ('me', 'favourite', 'track:cratarr/tr-2', 200);");
        u.move_tracks(&[(1, 2)]).unwrap();
        assert_eq!(one::<i64>(&u, "SELECT at FROM change_at"), 300);
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM change_at"), 1);
    }

    /// A device goes by the change log: told of a heart on the old id, it must hear that went, and that the new one has it.
    #[test]
    fn a_device_hears_a_favourite_went_to_the_track_that_took_it() {
        let u = with("");
        u.mark_changed("me", ChangeKind::Favourite, "track:cratarr/tr-1", Op::Changed, 100).unwrap();
        u.mark_changed("me", ChangeKind::Favourite, "track:cratarr/tr-3", Op::Deleted, 100).unwrap();
        let since = u.change_rev().unwrap();
        u.move_tracks(&[(1, 2), (3, 4)]).unwrap();
        let super::super::offline::Since::Changes { changes, .. } = u.changes_since(Some(since), "me", "", &[ChangeKind::Favourite], 10).unwrap() else { panic!("a device up to date reads on") };
        let told: Vec<(&str, Op)> = changes.iter().map(|c| (c.item.as_str(), c.op)).collect();
        assert_eq!(told, [("track:cratarr/tr-1", Op::Deleted), ("track:cratarr/tr-2", Op::Changed)], "a heart taken off says nothing of the new id");
    }

    #[test]
    fn moving_twice_changes_nothing() {
        let dump = |u: &UserData| -> Vec<String> {
            let tables = ["annotation", "imported", "bookmark", "plays", "scrobble_queue", "track_words", "playlist_item", "app_queue", "visual_rating", "change_at", "change_log"];
            let db = u.db();
            tables
                .iter()
                .flat_map(|t| {
                    let mut st = db.prepare(&format!("SELECT * FROM {t}")).unwrap();
                    let n = st.column_count();
                    let rows: Vec<String> = st.query_map([], |r| Ok(format!("{t} {:?}", (0..n).map(|i| r.get::<_, Value>(i).unwrap()).collect::<Vec<_>>()))).unwrap().map(Result::unwrap).collect();
                    rows
                })
                .collect()
        };
        let u = with(EVERYWHERE);
        u.db().execute_batch("INSERT INTO annotation (user, item, kind, play_count, own_count) VALUES ('me', 'tr-2', 'song', 5, 5);").unwrap();
        assert!(!u.move_tracks(&[(1, 2)]).unwrap().is_empty());
        let once = dump(&u);
        assert_eq!(u.move_tracks(&[(1, 2)]).unwrap(), [], "nothing left to move");
        assert_eq!(dump(&u), once);
        assert_eq!(one::<i64>(&u, "SELECT play_count FROM annotation WHERE item = 'tr-2'"), 8, "counted once");
    }

    /// What is not the old id stays, and a kind of row that is no song is not a track's.
    #[test]
    fn nothing_else_moves() {
        let u = with(EVERYWHERE);
        u.record("me", ("tr-9", "p9"), "al-1", &[], &Heard::counted(5)).unwrap();
        u.db().execute_batch("INSERT INTO annotation (user, item, kind, play_count) VALUES ('me', 'tr-1', 'album', 2);").unwrap();
        u.move_tracks(&[(1, 2)]).unwrap();
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM annotation WHERE item = 'tr-1' AND kind = 'album'"), 1);
        assert_eq!(one::<i64>(&u, "SELECT count(*) FROM plays WHERE song = 'tr-9'"), 1);
        assert_eq!(one::<i64>(&u, "SELECT play_count FROM annotation WHERE item = 'tr-9'"), 1);
    }
}
