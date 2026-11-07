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

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;

pub(super) const SCHEMA: &str = "
-- What was written that a device's index reads: `kind` is `track`, `record`, `artist`, `library` or `sound`, `item` its id.
CREATE TABLE IF NOT EXISTS index_log (rev INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, item INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS index_log_one ON index_log (kind, item);
";

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
