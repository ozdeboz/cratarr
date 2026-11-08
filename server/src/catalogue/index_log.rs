//! The library's change log (0101's point 16, D08): one counter, `index_rev`, and the rows of a device's index that were
//! written since a device last looked, so it asks for those and not for the whole index again.
//!
//! - **Triggers write it**, on the tables the index reads, in the catalogue's own file. A write through any connection logs
//!   what it touched, in its own transaction, and no writer has to remember, as `count_facts_writes` found for the facts.
//! - **A trigger compares what it is handed.** A pass goes over rows it may leave as they were, and what changed no column the
//!   index reads logs nothing, so a quiet library's rev stands still.
//! - **An item's latest change only**, as the user data's change log has it: its earlier row goes, so the log holds what
//!   changed, however often it was written. The counter never goes back (AUTOINCREMENT), not even once rows are pruned.
//! - **Rows are named by what the index names them by:** a track, a record, an artist by their ids for life, a library by its
//!   catalogue folder. What a row says is read when it is sent, so all the log says is which.
//! - **A row can change with no write here at all:** a word of the household's, a mood whose bar moved with AudioMuse's new rows, a
//!   year set by hand, what a build makes of the tags that were read a minute ago. So what each row last said is kept as a print,
//!   and `index_settle` logs every row that says something else now, however it came to. A device is told what is logged, so
//!   it comes to what a fresh index says whatever moved it, and no writer or source has to be listed here.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::Catalogue;

pub(super) const SCHEMA: &str = "
-- What was written that a device's index reads: `kind` is `track`, `record`, `artist`, `library`, `sound` or `cover`, `item` its id.
CREATE TABLE IF NOT EXISTS index_log (rev INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, item INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS index_log_one ON index_log (kind, item);
-- What each row of the index last said, as `index::Prints` makes it a number: one that says something else has changed.
CREATE TABLE IF NOT EXISTS index_prints (
  kind TEXT NOT NULL, item INTEGER NOT NULL, print INTEGER NOT NULL, PRIMARY KEY (kind, item)) WITHOUT ROWID;
-- The lowest rev a device can go on from: asked from before it, it fetches the index whole.
CREATE TABLE IF NOT EXISTS index_floor (one INTEGER PRIMARY KEY CHECK (one = 1), rev INTEGER NOT NULL);
";

/// The most rows the log keeps. It holds a row an item, so this is a library's every row more than once, and passing it takes
/// a change of nearly everything: a device behind that fetches whole, as rows would cost it as much.
const LOG_MOST: i64 = 200_000;

/// `kind`'s items that `select` names (as `item`), logged: each one's earlier row gone, a new rev handed out. Not `INSERT OR
/// REPLACE`: inside a trigger the conflict handling is the statement's that fired it, and its `OR IGNORE` would keep the old rev,
/// so a device that had read that one would miss this change.
fn logged(kind: &str, select: &str) -> String {
    format!(
        "DELETE FROM index_log WHERE kind = '{kind}' AND item IN (SELECT item FROM ({select}));
         INSERT INTO index_log (kind, item) SELECT DISTINCT '{kind}', item FROM ({select}) WHERE item IS NOT NULL;"
    )
}

/// A track of the row `at` (`NEW` or `OLD`) of a table that holds files by folder and path.
fn file_track(at: &str) -> String {
    logged("track", &format!("SELECT track AS item FROM files WHERE folder = {at}.folder AND rel = {at}.rel"))
}

/// Whether any of these columns differ: what an update of them has to say.
fn differ(columns: &[&str]) -> String {
    columns.iter().map(|c| format!("OLD.{c} IS NOT NEW.{c}")).collect::<Vec<_>>().join(" OR ")
}

/// `name` on `table`, `on` being what fires it.
fn trigger(name: &str, on: &str, table: &str, body: &str) -> String {
    format!("DROP TRIGGER IF EXISTS index_{name}; CREATE TRIGGER index_{name} AFTER {on} ON {table} BEGIN {body} END;")
}

/// `name` on `table`, for an update that changes one of `columns`.
fn on_change(name: &str, table: &str, columns: &[&str], body: &str) -> String {
    format!("DROP TRIGGER IF EXISTS index_{name}; CREATE TRIGGER index_{name} AFTER UPDATE ON {table} WHEN {} BEGIN {body} END;", differ(columns))
}

/// Every trigger, as a batch. Written again at each opening, so what an older one logged cannot stay on.
fn triggers() -> String {
    let track = |id: &str| logged("track", &format!("SELECT {id} AS item"));
    let record = |id: &str| logged("record", &format!("SELECT {id} AS item"));
    let artist = |id: &str| logged("artist", &format!("SELECT {id} AS item"));
    let sound = |id: &str| logged("sound", &format!("SELECT {id} AS item"));
    // What an update moves off a row and onto one, a rev for each: most often they are one.
    let both = |kind: &str, id: &str| logged(kind, &format!("SELECT OLD.{id} AS item UNION SELECT NEW.{id}"));
    // A folder parked has a negative id: it is no library, and its rows count for nothing.
    let library = |at: &str| logged("library", &format!("SELECT {at}.id AS item WHERE {at}.id >= 0"));
    // Of a file: what a row of the index reads, whether it is there at all, and which track it is.
    const FILE: [&str; 13] = ["folder", "rel", "track", "state", "format", "bitrate", "duration_ms", "gone_at", "trashed", "bits", "rate", "came", "size"];
    // Of a track: what a row says, and the record that lists it.
    const TRACK: [&str; 5] = ["born", "record", "disc", "number", "retired_at"];
    const RECORD: [&str; 5] = ["retired_at", "title", "artist", "year", "kind"];
    const ARTIST: [&str; 3] = ["name", "sort", "key"];
    const ANALYSIS: [&str; 5] = ["bpm", "four", "complete", "size", "mtime"];
    const BEATS: [&str; 3] = ["meter", "size", "mtime"];
    const MUSE: [&str; 6] = ["track", "energy", "key", "style", "moods", "embedding"];
    let muse = |at: &str| format!("{} {}", track(&format!("{at}.track")), sound(&format!("{at}.track")));
    [
        trigger("files_insert", "INSERT", "files", &track("NEW.track")),
        trigger("files_delete", "DELETE", "files", &track("OLD.track")),
        on_change("files_update", "files", &FILE, &both("track", "track")),
        // A file's tags are read whole: all that says is whose. Moved, they follow their file, which logs it.
        trigger("tags_insert", "INSERT", "tags", &file_track("NEW")),
        trigger("tags_delete", "DELETE", "tags", &file_track("OLD")),
        on_change("tags_update", "tags", &["name", "value"], &file_track("NEW")),
        on_change("tracks_update", "tracks", &TRACK, &format!("{} {}", track("NEW.id"), both("record", "record"))),
        trigger("records_insert", "INSERT", "records", &record("NEW.id")),
        on_change("records_update", "records", &RECORD, &record("NEW.id")),
        trigger("artists_insert", "INSERT", "artists", &artist("NEW.id")),
        on_change("artists_update", "artists", &ARTIST, &artist("NEW.id")),
        trigger("analysis_insert", "INSERT", "analysis", &file_track("NEW")),
        trigger("analysis_delete", "DELETE", "analysis", &file_track("OLD")),
        on_change("analysis_update", "analysis", &ANALYSIS, &file_track("NEW")),
        trigger("beats_insert", "INSERT", "beat_this", &file_track("NEW")),
        trigger("beats_delete", "DELETE", "beat_this", &file_track("OLD")),
        on_change("beats_update", "beat_this", &BEATS, &file_track("NEW")),
        trigger("muse_insert", "INSERT", "audiomuse", &muse("NEW")),
        trigger("muse_delete", "DELETE", "audiomuse", &muse("OLD")),
        on_change("muse_update", "audiomuse", &MUSE, &format!("{} {}", both("track", "track"), both("sound", "track"))),
        trigger("folders_insert", "INSERT", "folders", &library("NEW")),
        trigger("folders_delete", "DELETE", "folders", &library("OLD")),
        on_change("folders_update", "folders", &["id", "path"], &format!("{} {}", library("OLD"), library("NEW"))),
    ]
    .join("\n")
}

/// The log's tables and triggers, on a catalogue whose tables are all there.
pub(super) fn start(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(&format!("BEGIN; {} COMMIT;", triggers()))?;
    Ok(())
}

/// Where the log stands: the rev of its last change, none being 0.
fn rev_in(db: &Connection) -> Result<i64> {
    Ok(db.query_row("SELECT seq FROM sqlite_sequence WHERE name = 'index_log'", [], |r| r.get(0)).optional()?.unwrap_or(0))
}

/// What a device can go on from.
fn floor_in(db: &Connection) -> Result<i64> {
    Ok(db.query_row("SELECT rev FROM index_floor WHERE one = 1", [], |r| r.get(0)).optional()?.unwrap_or(0))
}

fn raise_floor(db: &Connection, rev: i64) -> Result<()> {
    db.execute("INSERT INTO index_floor (one, rev) VALUES (1, ?1) ON CONFLICT (one) DO UPDATE SET rev = max(rev, excluded.rev)", [rev])?;
    Ok(())
}

/// A row logged by what is no trigger, as a trigger logs one.
fn log_one(db: &Connection, kind: &str, item: i64) -> Result<()> {
    db.prepare_cached("DELETE FROM index_log WHERE kind = ?1 AND item = ?2")?.execute(params![kind, item])?;
    db.prepare_cached("INSERT INTO index_log (kind, item) VALUES (?1, ?2)")?.execute(params![kind, item])?;
    Ok(())
}

/// The oldest rows past `LOG_MOST` gone, and the floor raised past them.
fn prune(db: &Connection) -> Result<()> {
    let upto: Option<i64> = db.query_row("SELECT rev FROM index_log ORDER BY rev DESC LIMIT 1 OFFSET ?1", [LOG_MOST], |r| r.get(0)).optional()?;
    if let Some(upto) = upto {
        db.execute("DELETE FROM index_log WHERE rev <= ?1", [upto])?;
        raise_floor(db, upto)?;
    }
    Ok(())
}

/// What the log holds after `after`, up to and with `upto`: the items of each kind.
fn logged_in(db: &Connection, after: i64, upto: i64) -> Result<BTreeMap<String, Vec<i64>>> {
    let mut st = db.prepare_cached("SELECT kind, item FROM index_log WHERE rev > ?1 AND rev <= ?2 ORDER BY rev")?;
    let mut out: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for row in st.query_map(params![after, upto], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (kind, item) = row?;
        out.entry(kind).or_default().push(item);
    }
    Ok(out)
}

impl Catalogue {
    /// `f` on a connection of its own, beside a writer in the middle of something long; in memory, the catalogue's own. Blocking.
    fn beside<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        match self.reader() {
            Some(c) => f(&c),
            None => f(&self.db()),
        }
    }

    /// Where the library's change log stands. Blocking.
    pub fn index_rev(&self) -> Result<i64> {
        self.beside(rev_in)
    }

    /// The items of each kind written after `after`, up to and with `upto`. Blocking.
    pub fn index_logged(&self, after: i64, upto: i64) -> Result<BTreeMap<String, Vec<i64>>> {
        self.beside(|db| logged_in(db, after, upto))
    }

    /// `kind`'s `item` changed, in what has no trigger to say so: a small cover made. Blocking.
    pub fn index_note(&self, kind: &str, item: i64) -> Result<()> {
        log_one(&self.db(), kind, item)
    }

    /// Whether the log names anything but covers after `after`, up to and with `upto`. Covers are no rows and have a feed of their
    /// own, and a library's worth of them takes hours: at each a device would ask for a whole build, to hear nothing. Blocking.
    pub fn index_moved(&self, after: i64, upto: i64) -> Result<bool> {
        self.beside(|db| Ok(db.query_row("SELECT EXISTS (SELECT 1 FROM index_log WHERE rev > ?1 AND rev <= ?2 AND kind <> 'cover')", [after, upto], |r| r.get(0))?))
    }

    /// The lowest rev a device can go on from. Blocking.
    pub fn index_floor(&self) -> Result<i64> {
        self.beside(floor_in)
    }

    /// The index as it stands now, each kind's rows as (id, print): every row that says something else than when last told, or
    /// has left, or is new, joins the log. `read_at` is where the log stood before these were read. Where it stood so still, the
    /// rows hold all that is logged, and the rev answered is the log's, past what this logged, so a device given these rows is not
    /// sent them again. Where something was written meanwhile, `read_at`: what the rows may lack comes by the feed. The first time
    /// there is nothing to compare with and no device told anything, so no row is logged, and none goes on from before. Blocking.
    pub fn index_settle(&self, read_at: i64, rows: &[(&str, &[(i64, u64)])]) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = rev_in(&tx)?;
        let first = !tx.prepare_cached("SELECT 1 FROM index_prints LIMIT 1")?.exists([])?;
        for (kind, now) in rows {
            let mut was: HashMap<i64, i64> = {
                let mut st = tx.prepare_cached("SELECT item, print FROM index_prints WHERE kind = ?1")?;
                st.query_map([kind], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
            };
            let mut put = tx.prepare_cached("INSERT OR REPLACE INTO index_prints (kind, item, print) VALUES (?1, ?2, ?3)")?;
            for (item, print) in *now {
                let print = *print as i64;
                if was.remove(item) != Some(print) {
                    put.execute(params![kind, item, print])?;
                    if !first {
                        log_one(&tx, kind, *item)?;
                    }
                }
            }
            let mut left = tx.prepare_cached("DELETE FROM index_prints WHERE kind = ?1 AND item = ?2")?;
            for item in was.into_keys() {
                left.execute(params![kind, item])?;
                log_one(&tx, kind, item)?;
            }
        }
        let after = rev_in(&tx)?;
        if first {
            raise_floor(&tx, after)?;
        }
        prune(&tx)?;
        tx.commit()?;
        Ok(if before == read_at { after } else { read_at })
    }

    /// Tests elsewhere: the crates' facts read again at the next asking. With no server behind them a build does not tell them.
    #[cfg(test)]
    pub fn facts_forgotten_for_tests(&self) {
        *self.facts.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::catalogue::tests::copy;
    use crate::catalogue::{AlbumCopy, AudioMuseRow, BeatThisRow, TAG_BATCH};

    /// Two folders on the disk, the first with `files` files of one album in it, read and identified, each on its record.
    struct Library {
        roots: [PathBuf; 2],
        cat: Catalogue,
    }

    impl Drop for Library {
        fn drop(&mut self) {
            self.roots.iter().for_each(|r| {
                std::fs::remove_dir_all(r).ok();
            });
        }
    }

    fn library(name: &str, file: Option<&str>, files: usize) -> Library {
        let root = |n: u8| std::env::temp_dir().join(format!("cratarr-index-log-{name}-{n}-{}", std::process::id()));
        let roots = [root(0), root(1)];
        for r in &roots {
            std::fs::remove_dir_all(r).ok();
            std::fs::create_dir_all(r.join("A")).unwrap();
        }
        for n in 0..files {
            std::fs::write(roots[0].join(format!("A/{n:02}.flac")), format!("file {n}")).unwrap();
        }
        let cat = Catalogue::open(file.map(|f| roots[1].join(f)).as_deref(), roots.to_vec()).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let held = Library { cat, roots };
        held.cat.assign_records(&[held.album()]).unwrap();
        held
    }

    impl Library {
        /// The album its first folder holds, as a build hands it over.
        fn album(&self) -> AlbumCopy {
            let db = self.cat.db();
            let mut st = db.prepare("SELECT track, rel, duration_ms FROM files WHERE folder = 0 ORDER BY rel").unwrap();
            let rows: Vec<(i64, String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
            let listed: Vec<(i64, &str, i64)> = rows.iter().map(|(t, rel, ms)| (*t, rel.as_str(), *ms)).collect();
            AlbumCopy { files: rows.iter().map(|(_, rel, _)| (0, rel.clone())).collect(), ..copy(None, "A", "B", &listed) }
        }

        fn track(&self, n: usize) -> i64 {
            self.cat.track_of(0, &format!("A/{n:02}.flac")).unwrap()
        }

        fn record(&self, n: usize) -> i64 {
            let track = self.track(n);
            self.cat.db().query_row("SELECT record FROM tracks WHERE id = ?1", [track], |r| r.get(0)).unwrap()
        }

        /// What `f` logged, by kind.
        fn logged_by(&self, f: impl FnOnce(&Catalogue)) -> BTreeMap<String, Vec<i64>> {
            let before = self.cat.index_rev().unwrap();
            f(&self.cat);
            self.cat.index_logged(before, i64::MAX).unwrap()
        }
    }

    fn just(kinds: &[(&str, &[i64])]) -> BTreeMap<String, Vec<i64>> {
        kinds.iter().map(|(k, items)| (k.to_string(), items.to_vec())).collect()
    }

    /// A retag reads a file's tags whole: every tag row of it written, one track logged.
    #[test]
    fn a_retag_logs_the_track_it_touched_and_no_other() {
        let l = library("retag", None, 3);
        let one = l.track(1);
        let logged = l.logged_by(|cat| cat.tag_for_tests(0, "A/01.flac", &[("title", "New"), ("artist", "Other"), ("genre", "Dub")]));
        assert_eq!(logged, just(&[("track", &[one])]));
    }

    /// Keep moves an album's rows into another library with their track ids.
    #[test]
    fn a_keep_logs_every_track_it_moves() {
        let l = library("keep", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        let logged = l.logged_by(|cat| {
            cat.move_files(0, "A", 1, "B").unwrap();
        });
        let mut tracks = logged["track"].clone();
        tracks.sort_unstable();
        assert_eq!(tracks, [a.min(b), a.max(b)]);
        assert_eq!(logged.len(), 1, "{logged:?}");
    }

    /// A delete goes to the trash and retires when it is emptied: its tracks log both, and what retires names its record too.
    #[test]
    fn a_delete_logs_its_tracks_and_their_record_as_they_retire() {
        let l = library("delete", None, 2);
        let (a, record) = (l.track(0), l.record(0));
        let logged = l.logged_by(|cat| {
            cat.mark_trashed(0, "A/00.flac", "token").unwrap();
        });
        assert_eq!(logged, just(&[("track", &[a])]), "in the trash it leaves the index");
        let logged = l.logged_by(|cat| {
            cat.db().execute("UPDATE tracks SET retired_at = 1, record = NULL WHERE id = ?1", [a]).unwrap();
        });
        assert_eq!(logged, just(&[("record", &[record]), ("track", &[a])]), "the record it left is another row now");
        let logged = l.logged_by(|cat| {
            cat.db().execute("DELETE FROM files WHERE track = ?1", [a]).unwrap();
        });
        assert_eq!(logged, just(&[("track", &[a])]));
    }

    /// The owner's proof for D08: a pass over a library as it was, to its end, logs nothing. What it does write is its links, its
    /// time and a first play's shape, none of which a row of the index says.
    #[test]
    fn a_pass_that_changes_nothing_logs_nothing() {
        let l = library("pass", None, 3);
        let album = l.album();
        let logged = l.logged_by(|cat| {
            assert!(cat.walk_all().is_empty());
            assert_eq!(cat.read_tags_batch(TAG_BATCH).unwrap(), 0);
            cat.identify().unwrap();
            cat.assign_records(std::slice::from_ref(&album)).unwrap();
            cat.record_links("jellyfin", "lib", &[("t1".into(), 0, "A/00.flac".into())], 100).unwrap();
            cat.set_meta("pass:cratarr", "1");
            cat.db().execute("INSERT INTO analysis_shape (folder, rel, spectrum) VALUES (0, 'A/00.flac', x'00')", []).unwrap();
        });
        assert_eq!(logged, BTreeMap::new());
    }

    #[test]
    fn a_write_that_changes_nothing_the_index_reads_logs_nothing() {
        let l = library("nothing", None, 1);
        let same = [
            "UPDATE files SET bitrate = bitrate",
            "UPDATE files SET issue = 'broken', issue_detail = 'x', error = 'y', mtime = 5",
            "UPDATE tracks SET was_title = 'x', back = NULL",
            "UPDATE records SET counting = 'x'",
        ];
        for sql in same {
            let logged = l.logged_by(|cat| {
                cat.db().execute(sql, []).unwrap();
            });
            assert_eq!(logged, BTreeMap::new(), "{sql}");
        }
    }

    /// Each table the index reads, each way a write reaches it: a table left out would part a device from a fresh index with
    /// nothing to say so.
    #[test]
    fn every_table_the_index_reads_logs_a_write() {
        let l = library("tables", None, 1);
        let (track, record) = (l.track(0), l.record(0));
        let file = "A/00.flac";
        let (cat, ours) = (&l.cat, &[track][..]);
        let in_file = just(&[("track", ours)]);
        let muse = just(&[("sound", ours), ("track", ours)]);
        let held = |sql: &str| {
            cat.db().execute(sql, []).unwrap();
        };
        let cases: Vec<(String, BTreeMap<String, Vec<i64>>)> = vec![
            ("UPDATE files SET bitrate = 320".into(), in_file.clone()),
            ("UPDATE files SET duration_ms = 1".into(), in_file.clone()),
            ("UPDATE files SET bits = 24".into(), in_file.clone()),
            (format!("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, '{file}', 0, 'GENRE', 'genre', 'Jazz')"), in_file.clone()),
            ("UPDATE tags SET value = 'Dub'".into(), in_file.clone()),
            ("DELETE FROM tags".into(), in_file.clone()),
            (format!("UPDATE tracks SET born = 1 WHERE id = {track}"), just(&[("record", &[record]), ("track", ours)])),
            (format!("INSERT INTO analysis (folder, rel, size, mtime, version, analysed_at) VALUES (0, '{file}', 1, 1, 1, 1)"), in_file.clone()),
            ("UPDATE analysis SET bpm = 120".into(), in_file.clone()),
            ("UPDATE analysis SET four = 0.5".into(), in_file.clone()),
            ("DELETE FROM analysis".into(), in_file.clone()),
            (format!("INSERT INTO beat_this (folder, rel, size, mtime, beats, downbeats, at) VALUES (0, '{file}', 1, 1, x'00', x'00', 1)"), in_file.clone()),
            ("UPDATE beat_this SET meter = 3".into(), in_file.clone()),
            ("DELETE FROM beat_this".into(), in_file.clone()),
            (format!("INSERT INTO audiomuse (track, energy) VALUES ({track}, 0.5)"), muse.clone()),
            ("UPDATE audiomuse SET embedding = x'00000000'".into(), muse.clone()),
            ("DELETE FROM audiomuse".into(), muse.clone()),
            (format!("UPDATE records SET title = 'New' WHERE id = {record}"), just(&[("record", &[record])])),
            ("INSERT INTO artists (born, name, key) VALUES (1, 'A', 'a')".into(), just(&[("artist", &[1])])),
            ("UPDATE artists SET name = 'B'".into(), just(&[("artist", &[1])])),
            ("UPDATE folders SET path = '/elsewhere' WHERE id = 1".into(), just(&[("library", &[1])])),
        ];
        for (sql, wanted) in cases {
            let logged = l.logged_by(|_| held(&sql));
            let sorted: BTreeMap<String, Vec<i64>> = logged.into_iter().map(|(k, mut v)| (k, { v.sort_unstable(); v })).collect();
            assert_eq!(sorted, wanted, "{sql}");
        }
    }

    /// AudioMuse is taken in whole, and SQLite would empty a table with no trigger on it in one step, row by row or not.
    #[test]
    fn audiomuses_rows_taken_in_whole_log_each_track() {
        let l = library("muse", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        let row = |track: i64| AudioMuseRow { track, energy: Some(0.5), ..AudioMuseRow::default() };
        l.cat.replace_audiomuse(&[row(a), row(b)]).unwrap();
        let logged = l.logged_by(|cat| cat.replace_audiomuse(&[row(a)]).unwrap());
        let mut tracks = logged["track"].clone();
        tracks.sort_unstable();
        assert_eq!(tracks, [a.min(b), a.max(b)], "the one left too: it was deleted and written again");
        assert_eq!(logged["sound"].len(), 2);
    }

    /// The beats written by what the PC heard go by file, and through a statement of their own.
    #[test]
    fn beats_taken_in_log_their_file_as_it_stands() {
        let l = library("beats", None, 1);
        let (size, mtime) = l.cat.file_stamp(0, "A/00.flac").unwrap();
        let row = BeatThisRow { folder: 0, rel: "A/00.flac".into(), size, mtime, beats: vec![500, 1000, 1500, 2000], downbeats: vec![500, 2000] };
        let logged = l.logged_by(|cat| cat.store_beat_this(std::slice::from_ref(&row)).unwrap());
        assert_eq!(logged, just(&[("track", &[l.track(0)])]));
    }

    #[test]
    fn an_item_keeps_only_its_latest_change_and_the_rev_goes_on() {
        let l = library("latest", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        let before = l.cat.index_rev().unwrap();
        let set = |rel: &str, kbps: i64| {
            l.cat.db().execute("UPDATE files SET bitrate = ?2 WHERE rel = ?1", params![rel, kbps]).unwrap();
        };
        set("A/00.flac", 1);
        set("A/01.flac", 2);
        set("A/00.flac", 3);
        let now = l.cat.index_rev().unwrap();
        assert_eq!(now, before + 3, "each change a rev of its own");
        assert_eq!(l.cat.index_logged(before, now).unwrap(), just(&[("track", &[b, a])]), "in the order of their last change, once each");
        assert_eq!(l.cat.index_logged(now - 1, now).unwrap(), just(&[("track", &[a])]), "what came after a rev alone");
        assert_eq!(l.cat.index_logged(before, before + 1).unwrap(), BTreeMap::new(), "the row that held that rev went on");
    }

    /// `UPDATE OR IGNORE` hands its conflict handling to its triggers: an `OR REPLACE` in one of them would keep the old rev, and a
    /// device past that rev would never hear of it.
    #[test]
    fn a_statement_that_ignores_conflicts_still_moves_its_item_on() {
        let l = library("ignore", None, 1);
        let track = l.track(0);
        l.cat.db().execute("UPDATE files SET bitrate = 1", []).unwrap();
        let first = l.cat.index_rev().unwrap();
        l.cat.db().execute("UPDATE OR IGNORE files SET bitrate = 2", []).unwrap();
        assert_eq!(l.cat.index_logged(first, i64::MAX).unwrap(), just(&[("track", &[track])]));
    }

    /// Triggers on the file, not on one connection: whoever writes, and whenever it was opened last.
    #[test]
    fn the_log_holds_across_a_restart_and_a_write_from_any_connection() {
        let mut l = library("restart", Some("catalogue.sqlite"), 1);
        let track = l.track(0);
        let file = l.roots[1].join("catalogue.sqlite");
        let before = l.cat.index_rev().unwrap();
        l.cat.db().execute("UPDATE files SET bitrate = 1", []).unwrap();
        assert_eq!(l.cat.reader().map(|c| rev_in(&c).unwrap()), Some(before + 1), "beside the writer");
        l.cat = Catalogue::open(Some(&file), l.roots.to_vec()).unwrap();
        assert_eq!(l.cat.index_rev().unwrap(), before + 1, "a restart keeps where it stands");
        let other = Connection::open(&file).unwrap();
        other.execute("UPDATE files SET bitrate = 2", []).unwrap();
        drop(other);
        assert_eq!(l.cat.index_logged(before + 1, i64::MAX).unwrap(), just(&[("track", &[track])]));
        l.cat.db().execute("DELETE FROM index_log", []).unwrap();
        l.cat.db().execute("UPDATE files SET bitrate = 3", []).unwrap();
        assert_eq!(l.cat.index_rev().unwrap(), before + 3, "a rev is never handed out again, however much is pruned");
    }

    /// What a row said, as a settle is handed it.
    fn said(l: &Library, read_at: i64, tracks: &[(i64, u64)]) -> i64 {
        l.cat.index_settle(read_at, &[("track", tracks)]).unwrap()
    }

    fn settled(l: &Library, tracks: &[(i64, u64)]) -> i64 {
        said(l, l.cat.index_rev().unwrap(), tracks)
    }

    /// The first settle tells no one anything: no device holds a row to go on from.
    #[test]
    fn the_first_prints_log_nothing_and_no_rev_before_them_goes_on() {
        let l = library("first", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        let before = l.cat.index_rev().unwrap();
        let rev = settled(&l, &[(a, 1), (b, 2)]);
        assert_eq!(rev, before, "nothing logged");
        assert_eq!(l.cat.index_floor().unwrap(), rev);
    }

    #[test]
    fn a_row_that_says_something_else_joins_the_log_with_no_write_to_it() {
        let l = library("moved", None, 3);
        let (a, b, c) = (l.track(0), l.track(1), l.track(2));
        let first = settled(&l, &[(a, 1), (b, 2), (c, 3)]);
        assert_eq!(settled(&l, &[(a, 1), (b, 2), (c, 3)]), first, "as it was: nothing");
        let rev = settled(&l, &[(a, 1), (b, 20), (c, 3)]);
        assert_eq!(rev, first + 1);
        assert_eq!(l.cat.index_logged(first, rev).unwrap(), just(&[("track", &[b])]));
        assert_eq!(settled(&l, &[(a, 1), (b, 20), (c, 3)]), rev, "and it is not told twice");
    }

    #[test]
    fn a_row_gone_or_new_joins_the_log() {
        let l = library("gone", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        let first = settled(&l, &[(a, 1)]);
        let rev = settled(&l, &[(b, 2)]);
        let mut logged = l.cat.index_logged(first, rev).unwrap().remove("track").unwrap();
        logged.sort_unstable();
        assert_eq!(logged, [a.min(b), a.max(b)]);
    }

    /// A kind not handed over is no kind that went: each is compared with itself alone.
    #[test]
    fn kinds_are_held_apart() {
        let l = library("kinds", None, 1);
        let a = l.track(0);
        l.cat.index_settle(l.cat.index_rev().unwrap(), &[("track", &[(a, 1)]), ("record", &[(a + 100, 1)])]).unwrap();
        let before = l.cat.index_rev().unwrap();
        let rev = l.cat.index_settle(before, &[("record", &[(a + 100, 2)])]).unwrap();
        assert_eq!(l.cat.index_logged(before, rev).unwrap(), just(&[("record", &[a + 100])]), "and the track's row is no record that left");
    }

    /// Rows read before a write lack it, and a device told the log's rev would never be sent it.
    #[test]
    fn what_was_written_while_the_rows_were_read_comes_again() {
        let l = library("meanwhile", None, 2);
        let (a, b) = (l.track(0), l.track(1));
        settled(&l, &[(a, 1), (b, 2)]);
        let read_at = l.cat.index_rev().unwrap();
        l.cat.db().execute("UPDATE files SET bitrate = 9 WHERE track = ?1", [b]).unwrap();
        assert_eq!(said(&l, read_at, &[(a, 10), (b, 2)]), read_at, "where the rows were read");
        let logged = l.cat.index_logged(read_at, i64::MAX).unwrap().remove("track").unwrap();
        assert_eq!(logged, [b, a], "both to come, what this logged too");
    }

    #[test]
    fn a_log_past_its_most_goes_from_its_oldest_and_a_device_from_before_starts_again() {
        let l = library("most", None, 1);
        let a = l.track(0);
        settled(&l, &[(a, 1)]);
        let at = l.cat.index_rev().unwrap();
        {
            let db = l.cat.db();
            let tx = db.unchecked_transaction().unwrap();
            for item in 0..LOG_MOST + 3 {
                log_one(&tx, "artist", item).unwrap();
            }
            tx.commit().unwrap();
        }
        settled(&l, &[(a, 1)]);
        let floor = l.cat.index_floor().unwrap();
        assert!(floor > at, "{floor} over {at}");
        assert_eq!(l.cat.db().query_row("SELECT count(*) FROM index_log", [], |r| r.get::<_, i64>(0)).unwrap(), LOG_MOST);
        assert_eq!(l.cat.index_logged(at, floor).unwrap(), BTreeMap::new(), "nothing is left from before it");
    }

    #[test]
    fn a_folder_parked_is_no_library_until_it_is_back() {
        let l = library("parked", None, 1);
        let track = l.track(0);
        let root: &Path = &l.roots[0];
        let logged = l.logged_by(|cat| cat.set_folders(vec![l.roots[1].clone()]).unwrap());
        assert_eq!(logged["library"], [0], "its id for life, and no negative one");
        assert_eq!(logged["track"], [track], "its files went with it");
        let logged = l.logged_by(|cat| cat.set_folders(vec![root.to_path_buf(), l.roots[1].clone()]).unwrap());
        assert_eq!(logged["library"], [0]);
        assert_eq!(logged["track"], [track]);
    }
}
