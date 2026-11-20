//! What a record view shows that a write changed, owed to the user data's change log (decision 0087's `record` kind; SCENARIOS-DC1
//! LV1, LV10, LV11, H1): the change log's one sequence versions every record view.
//!
//! - **Triggers owe it**, on the tables a record view reads (records, tracks, files and their tags, references, reference_tracks,
//!   record_artists, record_labels, facts), in the catalogue's own file, as the library's own log is written (`index_log`): a
//!   write through any connection owes what it touched in its own transaction, and no writer has to remember (LV1). A trigger
//!   compares what it is handed, so a pass that leaves a row as it was owes nothing.
//! - **Owed, then moved** (LV11): a row stays here until the store the views read has the change (the served library's check after
//!   the build it made, `Serve::refresh`), then goes into the change log at a new rev. A row owed again after it was read is
//!   owed anew (`n`), so it is moved again.
//! - **A crash between the two** (LV10): what was committed here is still owed at the next start, and moved then.
//! - **Every record, had or not:** a record not had has a view too (its references, its lists).
//! - **What it is now** is read as it is moved: changed, merged into another (its end), or forgotten (no row).

use anyhow::Result;
use rusqlite::{Connection, params};

use super::Catalogue;

pub(super) const SCHEMA: &str = "
-- LIVE (0087's record kind): a record a write changed, owed to the change log until the store its views read has it. `n` orders
-- the owing: a record owed again after it was read is owed anew.
CREATE TABLE IF NOT EXISTS record_owed (n INTEGER PRIMARY KEY AUTOINCREMENT, record INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS record_owed_one ON record_owed (record);
";

/// The records `select` names (as `item`), owed: each one's earlier row gone, a new `n`. Not `INSERT OR REPLACE`: inside a trigger
/// the conflict handling is the statement's that fired it (`index_log::logged`).
fn owed(select: &str) -> String {
    format!(
        "DELETE FROM record_owed WHERE record IN (SELECT item FROM ({select}));
         INSERT INTO record_owed (record) SELECT DISTINCT item FROM ({select}) WHERE item IS NOT NULL;"
    )
}

/// Whether any of these columns differ: what an update of them has to say.
fn differ(columns: &[&str]) -> String {
    columns.iter().map(|c| format!("OLD.{c} IS NOT NEW.{c}")).collect::<Vec<_>>().join(" OR ")
}

fn trigger(name: &str, on: &str, table: &str, body: &str) -> String {
    format!("DROP TRIGGER IF EXISTS owed_{name}; CREATE TRIGGER owed_{name} AFTER {on} ON {table} BEGIN {body} END;")
}

fn on_change(name: &str, table: &str, columns: &[&str], body: &str) -> String {
    format!("DROP TRIGGER IF EXISTS owed_{name}; CREATE TRIGGER owed_{name} AFTER UPDATE ON {table} WHEN {} BEGIN {body} END;", differ(columns))
}

/// The tables a record view reads, and of each the columns it reads (LV1).
const RECORD: [&str; 12] = ["title", "artist", "year", "kind", "released", "genres", "cover", "counting", "counted_by_hand", "checked_by_hand", "merged_into", "retired_at"];
const TRACK: [&str; 4] = ["record", "disc", "number", "retired_at"];
const FILE: [&str; 9] = ["folder", "rel", "track", "format", "bitrate", "duration_ms", "gone_at", "trashed", "size"];
const REFERENCE: [&str; 4] = ["subject", "value", "how", "confirmed"];

/// Every trigger, as a batch. Written again at each opening, so what an older one owed cannot stay on.
fn triggers() -> String {
    let record = |at: &str, column: &str| owed(&format!("SELECT {at}.{column} AS item"));
    let both = |column: &str| owed(&format!("SELECT OLD.{column} AS item UNION SELECT NEW.{column}"));
    let of_track = |at: &str| owed(&format!("SELECT record AS item FROM tracks WHERE id = {at}.track"));
    let of_subject = |at: &str| owed(&format!("SELECT CAST(substr({at}.subject, 4) AS INTEGER) AS item WHERE {at}.subject LIKE 'al-%'"));
    // A file's tags say its track's title and place: whose record, by its file.
    let of_file = |at: &str| owed(&format!("SELECT t.record AS item FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder = {at}.folder AND f.rel = {at}.rel"));
    let mut out = vec![
        trigger("records_insert", "INSERT", "records", &record("NEW", "id")),
        trigger("records_delete", "DELETE", "records", &record("OLD", "id")),
        on_change("records_update", "records", &RECORD, &record("NEW", "id")),
        trigger("tracks_insert", "INSERT", "tracks", &record("NEW", "record")),
        trigger("tracks_delete", "DELETE", "tracks", &record("OLD", "record")),
        on_change("tracks_update", "tracks", &TRACK, &both("record")),
        trigger("files_insert", "INSERT", "files", &of_track("NEW")),
        trigger("files_delete", "DELETE", "files", &of_track("OLD")),
        on_change("files_update", "files", &FILE, &format!("{} {}", of_track("OLD"), of_track("NEW"))),
        trigger("tags_insert", "INSERT", "tags", &of_file("NEW")),
        trigger("tags_delete", "DELETE", "tags", &of_file("OLD")),
        on_change("tags_update", "tags", &["name", "value"], &of_file("NEW")),
        trigger("references_insert", "INSERT", "\"references\"", &of_subject("NEW")),
        trigger("references_delete", "DELETE", "\"references\"", &of_subject("OLD")),
        on_change("references_update", "\"references\"", &REFERENCE, &format!("{} {}", of_subject("OLD"), of_subject("NEW"))),
    ];
    for (table, columns) in [
        ("reference_tracks", &["source", "tracks"][..]),
        ("record_artists", &["artist", "position", "how"][..]),
        ("record_labels", &["label", "catno", "how"][..]),
        ("facts", &["fact", "source", "value"][..]),
    ] {
        out.push(trigger(&format!("{table}_insert"), "INSERT", table, &record("NEW", "record")));
        out.push(trigger(&format!("{table}_delete"), "DELETE", table, &record("OLD", "record")));
        let mut changed = columns.to_vec();
        changed.push("record");
        out.push(on_change(&format!("{table}_update"), table, &changed, &both("record")));
    }
    out.join("\n")
}

/// The owed rows' table and the triggers, on a catalogue whose tables are all there (the facts' too, `facts::SCHEMA`).
pub(super) fn start(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(&format!("BEGIN; {} COMMIT;", triggers()))?;
    Ok(())
}

/// What an owed record is now, as the change log says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Now {
    Changed,
    /// Merged into another: the record it ends in.
    Merged { into: i64 },
    /// No row any more (retention's forgetting, RT01).
    Forgotten,
}

/// One record owed to the change log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owed {
    pub n: i64,
    pub record: i64,
    pub now: Now,
}

impl Catalogue {
    /// The records owed to the change log, oldest first, at most `most`, each as it is now. On a reader. Blocking.
    pub fn owed_records(&self, most: usize) -> Result<Vec<Owed>> {
        let read = |db: &Connection| -> Result<Vec<Owed>> {
            let rows: Vec<(i64, i64, bool, Option<i64>)> = db
                .prepare_cached("SELECT o.n, o.record, r.id IS NULL, r.merged_into FROM record_owed o LEFT JOIN records r ON r.id = o.record ORDER BY o.n LIMIT ?1")?
                .query_map([most as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let mut out = Vec::with_capacity(rows.len());
            for (n, record, gone, merged) in rows {
                let now = match (gone, merged) {
                    (true, _) => Now::Forgotten,
                    (false, Some(_)) => match super::records::merged_end(db, record)? {
                        Some(end) if end != record => Now::Merged { into: end },
                        _ => Now::Changed,
                    },
                    (false, None) => Now::Changed,
                };
                out.push(Owed { n, record, now });
            }
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// These owed rows are in the change log: each goes, unless it was owed again since it was read. Blocking.
    pub fn owed_moved(&self, moved: &[Owed]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut st = tx.prepare_cached("DELETE FROM record_owed WHERE n = ?1 AND record = ?2")?;
            for o in moved {
                st.execute(params![o.n, o.record])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owed_now(cat: &Catalogue) -> Vec<(i64, Now)> {
        let owed = cat.owed_records(100).unwrap();
        cat.owed_moved(&owed).unwrap();
        owed.into_iter().map(|o| (o.record, o.now)).collect()
    }

    /// LV1 (the catalogue's writers): each change a record view shows owes its record, one row a record (its latest), by the
    /// tables' own triggers, whoever writes; a write that changes nothing owes nothing. Red first: no record kind, nothing owed
    /// (a view's change today is told by area, `changed { area }`, when its writer remembers to).
    #[test]
    fn every_change_a_record_shows_is_logged() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let night_ferry = cat.record_for_tests("Night Ferry", 3);
        let other = cat.record_for_tests("Night Ferry (again)", 3);
        assert_eq!(owed_now(&cat), [(night_ferry, Now::Changed), (other, Now::Changed)], "made: owed");
        // The writers, each as the code writes it.
        type Writer<'a> = (&'a str, Box<dyn Fn(&Catalogue) + 'a>);
        let list = [super::super::identity::RefTrack { disc: 1, position: 1, title: "Pier".into(), length_ms: None, recording: None }];
        let writers: Vec<Writer> = vec![
            ("a copy added by a build", Box::new(|c: &Catalogue| c.copy_for_tests(night_ferry, 0))),
            ("a file gone", Box::new(|c: &Catalogue| c.execute_for_tests(&format!("UPDATE files SET gone_at = 5 WHERE track IN (SELECT id FROM tracks WHERE record = {night_ferry})")))),
            ("a file back", Box::new(|c: &Catalogue| c.execute_for_tests(&format!("UPDATE files SET gone_at = NULL WHERE track IN (SELECT id FROM tracks WHERE record = {night_ferry})")))),
            (
                "its tags read again",
                Box::new(|c: &Catalogue| {
                    c.execute_for_tests(&format!(
                        "INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 0, 'TITLE', 'title', 'Pier' FROM files WHERE track IN (SELECT id FROM tracks WHERE record = {night_ferry})"
                    ))
                }),
            ),
            ("MusicBrainz's year read again", Box::new(|c: &Catalogue| c.said(night_ferry, "year", "musicbrainz", "2015", 9).unwrap())),
            ("a source's list read", Box::new(|c: &Catalogue| c.set_reference_tracks(night_ferry, "discogs", &list).unwrap())),
            ("the list that counts set by hand", Box::new(|c: &Catalogue| assert!(c.set_counting(night_ferry, Some("discogs"), true).unwrap()))),
            (
                "a reference kept",
                Box::new(|c: &Catalogue| {
                    c.execute_for_tests(&format!(
                        "INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES ('al-{night_ferry}', 'bandcamp', 'album', 'https://example.org/a', 1, 'hand')"
                    ))
                }),
            ),
            ("a retirement", Box::new(|c: &Catalogue| c.execute_for_tests(&format!("UPDATE records SET retired_at = 7 WHERE id = {night_ferry}")))),
        ];
        for (what, write) in &writers {
            write(&cat);
            assert_eq!(owed_now(&cat), [(night_ferry, Now::Changed)], "{what}: its record owed, once");
        }
        // A pass that leaves its rows as they were owes nothing.
        cat.execute_for_tests(&format!(
            "UPDATE files SET gone_at = gone_at, size = size WHERE track IN (SELECT id FROM tracks WHERE record = {night_ferry}); UPDATE records SET title = title"
        ));
        assert!(owed_now(&cat).is_empty(), "nothing changed, nothing owed");
        // Many writes before a move: one row a record.
        for year in 2000..2010 {
            cat.said(night_ferry, "year", "discogs", &year.to_string(), year).unwrap();
        }
        assert_eq!(owed_now(&cat), [(night_ferry, Now::Changed)]);
        // The fold merging the other into it: the other merged (where it went), it changed (the tracks it took).
        cat.execute_for_tests(&format!("UPDATE tracks SET record = {night_ferry} WHERE record = {other}; UPDATE records SET merged_into = {night_ferry} WHERE id = {other}"));
        let mut moved = owed_now(&cat);
        moved.sort_by_key(|(r, _)| *r);
        assert_eq!(moved, [(night_ferry, Now::Changed), (other, Now::Merged { into: night_ferry })]);
        // Forgotten (retention, RT01): no row.
        cat.execute_for_tests(&format!("DELETE FROM records WHERE id = {other}"));
        assert_eq!(owed_now(&cat), [(other, Now::Forgotten)]);
    }

    /// LV10 (kind 3): a crash after the catalogue committed a change and before its row was moved: what was owed is owed at the next
    /// start. A row owed again after it was read stays owed when the read rows are moved. Red first: no owed rows.
    #[test]
    fn an_owed_row_is_logged_after_a_crash() {
        let dir = std::env::temp_dir().join(format!("cratarr-owed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("catalogue.sqlite");
        let night_ferry = {
            let cat = Catalogue::open(Some(&file), Vec::new()).unwrap();
            let r = cat.record_for_tests("Night Ferry", 2);
            cat.owed_moved(&cat.owed_records(10).unwrap()).unwrap();
            cat.copy_for_tests(r, 0);
            // The commit made; the move cut off: the server dies here.
            r
        };
        let cat = Catalogue::open(Some(&file), Vec::new()).unwrap();
        let owed = cat.owed_records(10).unwrap();
        assert_eq!(owed.iter().map(|o| o.record).collect::<Vec<_>>(), [night_ferry], "owed still, for the start to move");
        // Read for a move, then written again before the move ends: it stays owed.
        cat.said(night_ferry, "year", "musicbrainz", "2014", 1).unwrap();
        cat.owed_moved(&owed).unwrap();
        assert_eq!(cat.owed_records(10).unwrap().iter().map(|o| o.record).collect::<Vec<_>>(), [night_ferry], "owed anew, moved again");
        drop(cat);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LV1's Never: no writer logs a record by hand. The change log's record rows come from the owed rows' one mover
    /// (`Serve::refresh`) and the user data's own triggers.
    #[test]
    fn no_record_writer_logs_by_hand() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        let mut stack = vec![crates.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap().replace('\r', "");
                    let code = text.split("#[cfg(test)]\nmod tests {").next().unwrap();
                    if code.contains(".log_records(") || code.contains(".owed_moved(") {
                        found.push(path.strip_prefix(&crates).unwrap().display().to_string().replace('\\', "/"));
                    }
                }
            }
        }
        found.sort();
        assert_eq!(found, ["served/src/serve/mod.rs"], "one mover");
    }
}
