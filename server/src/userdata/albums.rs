//! What the user data holds of an album, moved from the hashes it was known by to its record's id for life (0.16,
//! docs/records-and-copies.md). An album had two old ids and every table goes by one of them: a copy's own (`albums.id`: what
//! was played, starred or heard), or the Music-form one (`albums.base_id`: tags, hand facts, words, so they followed an album
//! kept from Incoming). For a Music album the two are one string, and an Incoming album's base can be a Music album's id, so
//! the two lists stay apart all the way. What a table's rows do when they meet on one record is that table's own rule.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::{Transaction, params};

use super::moved::{Columns, Merge, Place, Row, annotations, imports, int, last_change, later, said, walk};
use super::offline::ChangeKind;
use super::{NO_FOLDER, UserData};
use crate::moved::{Report, Walk};
use crate::serve::ids;
use crate::serve::review::JOB;

/// A copy's old id, and the record it is a copy of.
#[derive(Debug, Clone)]
pub struct OldCopy {
    pub id: String,
    pub record: String,
    /// The catalogue folder of the copy it names: a row of 0.15 says which copy it is of by the id alone. For an id worked out, where
    /// that copy lies now, not the place the id was worked out for.
    pub folder: i64,
}

/// Where the old ids went, as the first start's records found. Both lists run in the order of the libraries, main ones first:
/// of two rows that cannot both stand, the first met does.
#[derive(Debug, Clone, Default)]
pub struct Albums {
    /// `albums.id`: many to one.
    pub copies: Vec<OldCopy>,
    /// `albums.base_id` and a record: many to many. Tracks that differ make two records of one base.
    pub bases: Vec<(String, String)>,
}

/// The first met stands: the main library's, as the lists run main libraries first.
fn first(_: &Columns, _: &mut Row, _: &Row) {}

/// The earlier stands, whole: when a record came, and where from; when a tag was first set.
fn earlier(c: &Columns, into: &mut Row, from: &Row) {
    let at = c.at("at");
    if int(from, at) < int(into, at) {
        *into = from.clone();
    }
}

/// The one said last stands: a copy holds one.
fn last_said(c: &Columns, into: &mut Row, from: &Row) {
    later(c, into, from, "at", "album");
}

/// One that found a release before one that found none, whichever library's.
fn found(c: &Columns, into: &mut Row, from: &Row) {
    let at = c.at("found");
    if into[at] == Value::Null && from[at] != Value::Null {
        *into = from.clone();
    }
}

/// Facts before none, then the later.
fn facts(c: &Columns, into: &mut Row, from: &Row) {
    let at = c.at("facts");
    match (into[at] == Value::Null, from[at] == Value::Null) {
        (true, false) => *into = from.clone(),
        (false, true) => {}
        _ => later(c, into, from, "at", "subject"),
    }
}

/// One with genres before one asked in vain (`''`).
fn genres(c: &Columns, into: &mut Row, from: &Row) {
    let at = c.at("genres");
    if into[at] == Value::Text(String::new()) {
        *into = from.clone();
    }
}

/// The higher rule, as what an older one found is asked again; then one with a type.
fn types(c: &Columns, into: &mut Row, from: &Row) {
    let (rule, kind) = (c.at("rule"), c.at("primary_type"));
    if int(from, rule) > int(into, rule) || (int(from, rule) == int(into, rule) && into[kind] == Value::Null) {
        *into = from.clone();
    }
}

/// A table that is all albums, by its `album`.
const fn whole(table: &'static str, merge: Merge, differs: &'static [&'static str]) -> Place {
    Place { name: table, table, key: "album", only: "1", prefix: "", merge, differs }
}

/// By a copy's own id.
const BY_ID: [Place; 10] = [
    Place { name: "annotation: albums", table: "annotation", key: "item", only: "kind = 'album'", prefix: "", merge: annotations, differs: &[] },
    whole("added", earlier, &[]),
    Place { name: "imported: albums", table: "imported", key: "item", only: "kind = 'album'", prefix: "", merge: imports, differs: &[] },
    whole("album_matches", found, &["found"]),
    whole("album_complete", first, &["found", "discs"]),
    whole("record_references", first, &["chosen"]),
    whole("discogs_genres", genres, &[]),
    Place { name: "release_types: albums", table: "release_types", key: "key", only: "key LIKE 'dc:%'", prefix: "dc:", merge: types, differs: &[] },
    whole("renumber_matched", first, &[]),
    Place { name: "change_at: albums", table: "change_at", key: "item", only: "item LIKE 'album:cratarr/%'", prefix: ITEM, merge: last_change, differs: &[] },
];

/// By the Music-form id. A base on two records goes to both: each shows what it showed.
const BY_BASE: [Place; 5] = [
    whole("album_tags", earlier, &[]),
    whole("record_overrides", first, &["value"]),
    Place { name: "words: albums", table: "words", key: "subject", only: "subject LIKE 'album:%'", prefix: "album:", merge: said, differs: &[] },
    Place { name: "words_asked: albums", table: "words_asked", key: "subject", only: "subject LIKE 'album:%'", prefix: "album:", merge: said, differs: &[] },
    Place { name: "discogs_facts: albums", table: "discogs_facts", key: "subject", only: "subject LIKE 'album:%'", prefix: "album:", merge: facts, differs: &[] },
];

/// A copy's, not a record's: kept by (record, folder), so a new copy of a kept record is not "promoted already".
const REVIEWS: Place = whole("album_review", last_said, &[]);
const HEARD: Place = whole("intake_heard", last_said, &[]);

/// An album of Cratarr's own as the change log and `change_at` name it (`item_id`).
const ITEM: &str = "album:cratarr/";

/// What names an album and stays as it is, counted: the classical marks are emptied at every open, and a kept answer is a
/// device's retry, gone in 30 days.
const LEFT: [&str; 2] = ["classical_marks", "kept_answer"];

/// Rows from before copies had folders take the folder of the copy their old id names: two copies' rows stay two on one record.
/// What has a folder keeps it.
fn onto_copies(tx: &Transaction, table: &str, moved: &Albums) -> Result<()> {
    let mut st = tx.prepare(&format!("UPDATE {table} SET folder = ?2 WHERE album = ?1 AND folder = {NO_FOLDER}"))?;
    for copy in &moved.copies {
        st.execute(params![copy.id, copy.folder])?;
    }
    Ok(())
}

/// A Keep queued for conversion remembers its job by the job's album (`job:<id>`), which `convert-queue.json` moves by the same
/// ids: both go, or Undo no longer finds its job. Another server's album is that server's.
fn undos(tx: &Transaction, copies: &[(String, String)], report: &mut Report) -> Result<()> {
    let records: HashMap<&str, &str> = copies.iter().map(|(id, record)| (id.as_str(), record.as_str())).collect();
    let rows: Vec<(String, i64, String)> = tx.prepare(&format!("SELECT album, folder, undo FROM album_review WHERE undo LIKE '{JOB}%'"))?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut walked = Walk::default();
    for (album, folder, undo) in &rows {
        let id = undo.strip_prefix(JOB).unwrap_or(undo);
        if let Some(record) = records.get(id) {
            tx.execute("UPDATE album_review SET undo = ?3 WHERE album = ?1 AND folder = ?2", params![album, folder, format!("{JOB}{record}")])?;
            walked.moved += 1;
        } else if ids::for_life(id) {
            walked.moved += 1;
        } else if id.starts_with(ids::ALBUM) {
            walked.unmatched.push(id.to_string());
        }
    }
    report.walked("album_review.undo", rows.len() as u64, walked);
    Ok(())
}

/// The reviews, each with its names, which stay with it if it matches nothing. One whose id has gone (a Keep left it under
/// Incoming's) is found by its base, as it was read: on the first record of that base, the main library's, where a Keep went,
/// and for any copy, as nothing says which it was. Once moved, `base` is the record's id too, until it goes.
fn reviews(tx: &Transaction, moved: &Albums, copies: &[(String, String)], report: &mut Report) -> Result<()> {
    undos(tx, copies, report)?;
    onto_copies(tx, REVIEWS.table, moved)?;
    let known: HashSet<&str> = copies.iter().map(|(id, _)| id.as_str()).collect();
    let mut first_of: HashMap<&str, &str> = HashMap::new();
    for (base, record) in &moved.bases {
        first_of.entry(base).or_insert(record);
    }
    let rows: Vec<(String, String)> = tx.prepare("SELECT DISTINCT album, base FROM album_review")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut pairs = copies.to_vec();
    for (album, base) in &rows {
        if let Some(record) = first_of.get(base.as_str()).filter(|_| !known.contains(album.as_str()) && !ids::for_life(album)) {
            pairs.push((album.clone(), record.to_string()));
        }
    }
    walk(tx, &REVIEWS, &pairs, report)?;
    let albums: Vec<String> = tx.prepare("SELECT DISTINCT album FROM album_review")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    for album in albums.iter().filter(|album| ids::for_life(album)) {
        tx.execute("UPDATE album_review SET base = ?1 WHERE album = ?1", [album])?;
    }
    Ok(())
}

/// The change log named albums by their old ids. As the playlists' move left it: those rows go, and a device asking from
/// before them reads everything again (each starts fresh at 0.16 anyway, decision 0101).
fn change_log(tx: &Transaction, report: &mut Report) -> Result<()> {
    let last: Option<i64> = tx.query_row("SELECT max(rev) FROM change_log", [], |r| r.get(0))?;
    let kinds = (ChangeKind::Favourite.as_str(), ChangeKind::Rating.as_str());
    let gone = tx.execute(&format!("DELETE FROM change_log WHERE kind IN ('{}', '{}') AND item LIKE '{ITEM}%'", kinds.0, kinds.1), [])? as u64;
    if let (true, Some(last)) = (gone > 0, last) {
        tx.execute("INSERT INTO change_log_floor (one, rev) VALUES (1, ?1) ON CONFLICT (one) DO UPDATE SET rev = max(rev, excluded.rev)", [last])?;
    }
    report.table("change_log: albums", gone, 0, 0);
    Ok(())
}

/// The albums moved by `moved`, in the caller's transaction: the user data's move is one, its report written last in it. Safe
/// to repeat: a row moved is under an id no list holds. A row no list matches stays as it is, and is counted: a tag's hash from
/// before a retag cannot be read back.
pub(crate) fn move_albums(tx: &Transaction, moved: &Albums) -> Result<Report> {
    let mut report = Report::default();
    let copies: Vec<(String, String)> = moved.copies.iter().map(|c| (c.id.clone(), c.record.clone())).collect();
    for place in &BY_ID {
        walk(tx, place, &copies, &mut report)?;
    }
    for place in &BY_BASE {
        walk(tx, place, &moved.bases, &mut report)?;
    }
    reviews(tx, moved, &copies, &mut report)?;
    onto_copies(tx, HEARD.table, moved)?;
    walk(tx, &HEARD, &copies, &mut report)?;
    change_log(tx, &mut report)?;
    for table in LEFT {
        let rows: i64 = tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        report.table(table, rows as u64, rows as u64, 0);
    }
    Ok(report)
}

impl UserData {
    /// `move_albums` in a transaction of its own.
    pub fn move_albums(&self, moved: &Albums) -> Result<Report> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let report = move_albums(&tx, moved)?;
        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use rusqlite::types::FromSql;

    use super::super::offline::{Since, item_id};
    use super::super::{ImportKey, Kind, put_migration_part};
    use super::*;
    use crate::source::cratarr::KIND;

    /// The main library's, listed first and sorting last: what a list's order decides, no key's order could.
    const MAIN: &str = "al-zzzzzzzzzzzzzzzzzzzzzz";
    const OTHER: &str = "al-aaaaaaaaaaaaaaaaaaaaaa";
    const GONE: &str = "al-gonegonegonegonegonego";
    const RECORD: &str = "al-7";

    /// Where each form's rows are, and how each says the id.
    const ID_FORM: [(&str, &str, &str); 12] = [
        ("annotation", "item", ""),
        ("added", "album", ""),
        ("imported", "item", ""),
        ("album_review", "album", ""),
        ("album_matches", "album", ""),
        ("album_complete", "album", ""),
        ("intake_heard", "album", ""),
        ("record_references", "album", ""),
        ("discogs_genres", "album", ""),
        ("release_types", "key", "dc:"),
        ("renumber_matched", "album", ""),
        ("change_at", "item", "album:cratarr/"),
    ];
    const BASE_FORM: [(&str, &str, &str); 5] = [
        ("album_tags", "album", ""),
        ("record_overrides", "album", ""),
        ("words", "subject", "album:"),
        ("words_asked", "subject", "album:"),
        ("discogs_facts", "subject", "album:"),
    ];

    /// A user data with these statements run: what tests below start from.
    fn with(sql: &str) -> UserData {
        let u = UserData::open(None).unwrap();
        u.db().execute_batch(sql).unwrap();
        u
    }

    fn one<T: FromSql>(u: &UserData, sql: &str) -> T {
        u.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn count(u: &UserData, sql: &str) -> i64 {
        one(u, &format!("SELECT count(*) FROM {sql}"))
    }

    fn copy(id: &str, record: &str, folder: i64) -> OldCopy {
        OldCopy { id: id.to_string(), record: record.to_string(), folder }
    }

    fn base(id: &str, record: &str) -> (String, String) {
        (id.to_string(), record.to_string())
    }

    /// These ids on one record, in both forms, as a Music album's are, each copy in a folder of its own.
    fn on_record(ids: &[&str]) -> Albums {
        Albums { copies: ids.iter().zip(0..).map(|(id, folder)| copy(id, RECORD, folder)).collect(), bases: ids.iter().map(|id| base(id, RECORD)).collect() }
    }

    /// A row under `id` in every place that holds an album's id.
    fn every_place(id: &str) -> String {
        format!(
            "INSERT INTO annotation (user, item, kind, play_count, rating) VALUES ('ann', '{id}', 'album', 1, 2);
             INSERT INTO added (album, at, origin) VALUES ('{id}', 10, 'jf');
             INSERT INTO imported (account, source, server_user, kind, item, plays, at) VALUES ('ann', 'jf', 'u1', 'album', '{id}', 1, 10);
             INSERT INTO album_review (album, base, state, at, name, album_artist) VALUES ('{id}', '{id}', 'promoted', 10, 'N', 'A');
             INSERT INTO album_tags (album, tag, at) VALUES ('{id}', 'evening', 10);
             INSERT INTO record_overrides (album, field, value, at) VALUES ('{id}', 'year', '2018', 10);
             INSERT INTO words (subject, source, word, kind, weight, at) VALUES ('album:{id}', 'lastfm', 'dub', 'tag', 0.8, 10);
             INSERT INTO words_asked (subject, source, at, ok) VALUES ('album:{id}', 'lastfm', 10, 1);
             INSERT INTO discogs_facts (subject, at, facts) VALUES ('album:{id}', 10, '{{}}');
             INSERT INTO album_matches (album, at, found) VALUES ('{id}', 10, '{{}}');
             INSERT INTO album_complete (album, at, found, discs, rule) VALUES ('{id}', 10, NULL, 1, 1);
             INSERT INTO intake_heard (album, at, heard) VALUES ('{id}', 10, '{{}}');
             INSERT INTO record_references (album, chosen) VALUES ('{id}', '{{}}');
             INSERT INTO discogs_genres (album, genres, at) VALUES ('{id}', 'Dub', 10);
             INSERT INTO release_types (key, primary_type, secondary, at, rule) VALUES ('dc:{id}', 'album', '', 10, 2);
             INSERT INTO renumber_matched (album, at) VALUES ('{id}', 10);
             INSERT INTO change_at (account, kind, item, at) VALUES ('ann', 'favourite', 'album:cratarr/{id}', 10);"
        )
    }

    /// Every row of every place, to compare.
    fn all_rows(u: &UserData) -> Vec<String> {
        let db = u.db();
        let mut rows = Vec::new();
        for (table, _, _) in ID_FORM.iter().chain(&BASE_FORM) {
            let mut st = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let columns = st.column_count();
            let read = st.query_map([], |r| Ok(format!("{table} {:?}", (0..columns).map(|i| r.get::<_, Value>(i).unwrap()).collect::<Vec<_>>()))).unwrap();
            rows.extend(read.map(Result::unwrap));
        }
        rows.sort();
        rows
    }

    #[test]
    fn two_ids_hearts_and_plays_fold_as_any_two_annotations() {
        let u = with(&format!(
            "INSERT INTO annotation (user, item, kind, play_count, play_date, rating, starred, starred_at) VALUES
               ('ann', '{MAIN}', 'album', 3, 100, 2, 0, NULL), ('ann', '{OTHER}', 'album', 4, 50, 5, 1, 70), ('bob', '{OTHER}', 'album', 1, 9, 0, 0, NULL),
               ('ann', '{OTHER}', 'song', 8, 9, 0, 0, NULL);"
        ));
        let report = u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        let ann = u.get("ann", Kind::Album, &[RECORD.to_string()]).unwrap().remove(RECORD).unwrap();
        assert_eq!((ann.play_count, ann.play_date, ann.rating, ann.starred_at), (7, Some(100), 5, Some(70)), "plays added, the later date, the better rating, the star");
        assert_eq!(u.get("bob", Kind::Album, &[RECORD.to_string()]).unwrap()[RECORD].play_count, 1, "each account's own");
        assert_eq!(count(&u, &format!("annotation WHERE kind = 'album' AND item <> '{RECORD}'")), 0, "no row left behind");
        assert_eq!(u.get("ann", Kind::Song, &[OTHER.to_string()]).unwrap()[OTHER].play_count, 8, "what is not an album's is not moved");
        let t = report.table_named("annotation: albums").unwrap();
        assert_eq!((t.before, t.after, t.moved, t.merged), (3, 2, 3, 1));
    }

    #[test]
    fn of_two_added_the_earlier_stands_with_where_it_came_from() {
        let u = with(&format!("INSERT INTO added (album, at, origin) VALUES ('{MAIN}', 200, 'main'), ('{OTHER}', 100, 'other');"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<i64>(&u, &format!("SELECT at FROM added WHERE album = '{RECORD}'")), 100);
        assert_eq!(one::<String>(&u, "SELECT origin FROM added"), "other");
        assert_eq!(count(&u, "added"), 1);
    }

    /// The next import reads this ledger by the new id: left behind, it counts all of it as new.
    #[test]
    fn what_two_ids_imported_adds_up_under_the_record() {
        let u = with(&format!(
            "INSERT INTO imported (account, source, server_user, kind, item, server_item, plays, played, starred, rating, at) VALUES
               ('ann', 'jf', 'u1', 'album', '{MAIN}', 'a1', 3, 10, 0, 1, 5), ('ann', 'jf', 'u1', 'album', '{OTHER}', 'a2', 2, 20, 1, 4, 6);"
        ));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        let key = ImportKey { account: "ann", source: "jf", server_user: "u1" };
        assert_eq!(u.imported(key, Kind::Album, &[RECORD.to_string()]).unwrap()[RECORD].server_item.as_deref(), Some("a1"), "the server's item stays the first's");
        let row: (i64, i64, i64, i64) = u.db().query_row("SELECT plays, played, starred, rating FROM imported", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!(row, (5, 20, 1, 4));
        assert_eq!(count(&u, "imported"), 1);
    }

    /// Two copies of one record are reviewed apart: read by record alone, a new copy of a kept record is "promoted already".
    #[test]
    fn a_review_stays_with_its_copy() {
        let u = with(&format!(
            "INSERT INTO album_review (album, base, state, at, name, album_artist) VALUES
               ('{MAIN}', '{MAIN}', 'promoted', 5, 'First', 'A'), ('{OTHER}', '{MAIN}', 'rejected', 6, 'Second', 'A');"
        ));
        u.move_albums(&Albums { copies: vec![copy(MAIN, RECORD, 1), copy(OTHER, RECORD, 3)], bases: Vec::new() }).unwrap();
        let (one_copy, another) = (u.review(RECORD, Some(1)).unwrap().unwrap(), u.review(RECORD, Some(3)).unwrap().unwrap());
        assert_eq!((one_copy.state.as_str(), one_copy.name.as_str(), one_copy.folder), ("promoted", "First", 1));
        assert_eq!((another.state.as_str(), another.name.as_str(), another.folder), ("rejected", "Second", 3));
        assert!(u.review(RECORD, Some(4)).unwrap().is_none(), "a copy nobody reviewed");
        assert_eq!(one::<String>(&u, &format!("SELECT DISTINCT base FROM album_review WHERE album = '{RECORD}'")), RECORD, "the record is what a base was");
    }

    #[test]
    fn of_two_reviews_of_one_copy_the_later_stands() {
        let u = with(&format!(
            "INSERT INTO album_review (album, base, state, at, name, album_artist) VALUES
               ('{MAIN}', '{MAIN}', 'promoted', 5, 'First', 'A'), ('{OTHER}', '{MAIN}', 'rejected', 6, 'Second', 'A');"
        ));
        u.move_albums(&Albums { copies: vec![copy(MAIN, RECORD, 1), copy(OTHER, RECORD, 1)], bases: Vec::new() }).unwrap();
        assert_eq!(count(&u, "album_review"), 1);
        assert_eq!(u.review(RECORD, Some(1)).unwrap().unwrap().state, "rejected");
    }

    /// A Keep moved the album, so its Incoming id names nothing now: 0.15 read the review by the base, and so must the record.
    #[test]
    fn a_review_a_keep_left_behind_follows_its_base_to_one_record() {
        let u = with(&format!("INSERT INTO album_review (album, base, state, at, undo, name, album_artist) VALUES ('{OTHER}', '{MAIN}', 'promoted', 5, 'key', 'N', 'A');"));
        let report = u.move_albums(&Albums { copies: Vec::new(), bases: vec![base(MAIN, "al-1"), base(MAIN, "al-2")] }).unwrap();
        let r = u.review("al-1", None).unwrap().unwrap();
        assert_eq!((r.album.as_str(), r.folder, r.undo.as_deref()), ("al-1", NO_FOLDER, Some("key")));
        assert!(u.review("al-2", None).unwrap().is_none(), "a copy's, so never copied: the main library's, where a Keep went");
        assert_eq!(report.unmatched.iter().find(|x| x.what == "album_review").unwrap().count, 0);
    }

    /// Undo finds a queued Keep's job by its album, and `convert-queue.json` moves that album by the same ids.
    #[test]
    fn a_queued_keeps_undo_moves_with_its_job() {
        let u = with(&format!(
            "INSERT INTO album_review (album, base, state, at, undo, name, album_artist) VALUES
               ('{MAIN}', '{MAIN}', 'promoted', 5, 'job:{MAIN}', 'N', 'A'),
               ('{OTHER}', '{OTHER}', 'promoted', 5, 'job:{GONE}', 'N', 'A'),
               ('al-1', 'al-1', 'promoted', 5, 'job:0123456789abcdef', 'N', 'A'),
               ('al-2', 'al-2', 'promoted', 5, 'a-key', 'N', 'A');"
        ));
        let report = u.move_albums(&on_record(&[MAIN])).unwrap();
        assert_eq!(u.review(RECORD, None).unwrap().unwrap().undo, Some(format!("job:{RECORD}")));
        assert_eq!(u.review(OTHER, None).unwrap().unwrap().undo, Some(format!("job:{GONE}")), "its job is as lost as it");
        assert_eq!(u.review("al-1", None).unwrap().unwrap().undo.as_deref(), Some("job:0123456789abcdef"), "another server's album is that server's");
        assert_eq!(u.review("al-2", None).unwrap().unwrap().undo.as_deref(), Some("a-key"));
        let t = report.table_named("album_review.undo").unwrap();
        assert_eq!((t.before, t.moved), (3, 1));
        let lost = report.unmatched.iter().find(|x| x.what == "album_review.undo").unwrap();
        assert_eq!((lost.count, lost.sample.as_slice()), (1, [GONE.to_string()].as_slice()));
    }

    #[test]
    fn of_two_tags_the_earlier_stands() {
        let u = with(&format!("INSERT INTO album_tags (album, tag, at) VALUES ('{MAIN}', 'evening', 9), ('{OTHER}', 'evening', 3), ('{OTHER}', 'dub', 4);"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(u.tags_of(RECORD).unwrap(), ["evening", "dub"]);
        assert_eq!(one::<i64>(&u, "SELECT at FROM album_tags WHERE tag = 'evening'"), 3);
        assert_eq!(count(&u, "album_tags"), 2);
    }

    #[test]
    fn the_main_librarys_hand_fact_stands_and_the_others_join_it() {
        let u = with(&format!("INSERT INTO record_overrides (album, field, value, at) VALUES ('{MAIN}', 'year', '2018', 5), ('{OTHER}', 'year', '1999', 9), ('{OTHER}', 'genres', '[]', 9);"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, &format!("SELECT value FROM record_overrides WHERE album = '{RECORD}' AND field = 'year'")), "2018");
        assert_eq!(count(&u, &format!("record_overrides WHERE album = '{RECORD}'")), 2, "what only one said stays");
    }

    /// What is gone with a row where two rows said different things can only be read in the report. Where they agree, nothing
    /// is lost, and one that said nothing loses nothing.
    #[test]
    fn two_rows_that_cannot_both_stand_are_counted() {
        let u = with(&format!(
            "INSERT INTO record_overrides (album, field, value, at) VALUES ('{MAIN}', 'year', '2018', 5), ('{OTHER}', 'year', '1999', 9), ('{MAIN}', 'genres', '[]', 5), ('{OTHER}', 'genres', '[]', 9);
             INSERT INTO album_matches (album, at, found) VALUES ('{MAIN}', 5, '1'), ('{OTHER}', 6, '2'), ('{GONE}', 7, NULL);
             INSERT INTO album_complete (album, at, found, discs, rule) VALUES ('{MAIN}', 5, '1', 1, 1), ('{OTHER}', 6, '2', 1, 1);
             INSERT INTO record_references (album, chosen) VALUES ('{MAIN}', '1'), ('{OTHER}', '2');"
        ));
        let report = u.move_albums(&on_record(&[MAIN, OTHER, GONE])).unwrap();
        let counted: Vec<(&str, u64, &[String])> = report.differed.iter().map(|d| (d.what.as_str(), d.count, d.sample.as_slice())).collect();
        let on_it = [RECORD.to_string()];
        assert_eq!(counted, [("album_matches", 1, on_it.as_slice()), ("album_complete", 1, on_it.as_slice()), ("record_references", 1, on_it.as_slice()), ("record_overrides", 1, on_it.as_slice())]);
    }

    #[test]
    fn of_two_words_the_later_stands() {
        let u = with(&format!(
            "INSERT INTO words (subject, source, word, kind, weight, at) VALUES
               ('album:{MAIN}', 'lastfm', 'dub', 'tag', 0.8, 10), ('album:{OTHER}', 'lastfm', 'dub', 'genre', 0.1, 20), ('album:{OTHER}', 'lastfm', 'old', 'tag', 0.5, 5),
               ('artist:ar-1', 'lastfm', 'dub', 'tag', 0.3, 5);"
        ));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        let row: (String, f64) = u.db().query_row(&format!("SELECT kind, weight FROM words WHERE subject = 'album:{RECORD}' AND word = 'dub'"), [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(row, ("genre".to_string(), 0.1));
        assert_eq!(count(&u, &format!("words WHERE subject = 'album:{RECORD}'")), 2);
        assert_eq!(count(&u, "words WHERE subject = 'artist:ar-1'"), 1, "an artist's words are the artists' step");
    }

    /// Left behind, every album is asked of every source again.
    #[test]
    fn of_two_asks_the_later_stands() {
        let u = with(&format!(
            "INSERT INTO words_asked (subject, source, at, ok) VALUES ('album:{MAIN}', 'lastfm', 10, 1), ('album:{OTHER}', 'lastfm', 20, 0), ('album:{OTHER}', 'discogs', 5, 1);"
        ));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        let row: (i64, i64) = u.db().query_row(&format!("SELECT at, ok FROM words_asked WHERE subject = 'album:{RECORD}' AND source = 'lastfm'"), [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(row, (20, 0));
        assert_eq!(count(&u, "words_asked"), 2);
    }

    #[test]
    fn of_two_discogs_facts_those_with_facts_then_the_later_stand() {
        let (a, b, c, d) = ("al-aaaaaaaaaaaaaaaaaaaaab", "al-bbbbbbbbbbbbbbbbbbbbba", "al-cccccccccccccccccccccc", "al-dddddddddddddddddddddd");
        let u = with(&format!(
            "INSERT INTO discogs_facts (subject, at, facts) VALUES
               ('album:{MAIN}', 5, 'early'), ('album:{OTHER}', 9, 'late'), ('album:{a}', 5, 'said'), ('album:{b}', 9, NULL), ('album:{c}', 9, NULL), ('album:{d}', 5, 'said'),
               ('artist:a loose name', 5, 'theirs'), ('rg:0001', 5, NULL);"
        ));
        let moved = Albums { copies: Vec::new(), bases: vec![base(MAIN, "al-1"), base(OTHER, "al-1"), base(a, "al-2"), base(b, "al-2"), base(c, "al-3"), base(d, "al-3")] };
        u.move_albums(&moved).unwrap();
        let facts = |record: &str| one::<Option<String>>(&u, &format!("SELECT facts FROM discogs_facts WHERE subject = 'album:{record}'"));
        assert_eq!(facts("al-1").as_deref(), Some("late"));
        assert_eq!(facts("al-2").as_deref(), Some("said"), "the later said nothing");
        assert_eq!(facts("al-3").as_deref(), Some("said"), "and nor did the first met");
        assert_eq!(count(&u, "discogs_facts"), 5, "an artist's and a release group's stay as they are");
    }

    #[test]
    fn a_match_found_stands_over_none() {
        let u = with(&format!("INSERT INTO album_matches (album, at, found) VALUES ('{MAIN}', 5, NULL), ('{OTHER}', 6, 'a release');"));
        let report = u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, "SELECT found FROM album_matches"), "a release");
        assert_eq!(report.differed.iter().find(|d| d.what == "album_matches").unwrap().count, 0, "none is nothing lost");
    }

    #[test]
    fn the_main_librarys_match_stands_over_anothers() {
        let u = with(&format!("INSERT INTO album_matches (album, at, found) VALUES ('{MAIN}', 5, 'this'), ('{OTHER}', 6, 'that');"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, &format!("SELECT found FROM album_matches WHERE album = '{RECORD}'")), "this");
    }

    #[test]
    fn the_main_librarys_look_at_what_it_lacks_stands() {
        let u = with(&format!("INSERT INTO album_complete (album, at, found, discs, rule) VALUES ('{MAIN}', 5, 'this', 1, 1), ('{OTHER}', 6, 'that', 2, 1);"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, &format!("SELECT found FROM album_complete WHERE album = '{RECORD}'")), "this");
        assert_eq!(count(&u, "album_complete"), 1);
    }

    #[test]
    fn the_main_librarys_references_stand() {
        let u = with(&format!("INSERT INTO record_references (album, chosen) VALUES ('{MAIN}', 'this'), ('{OTHER}', 'that');"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, &format!("SELECT chosen FROM record_references WHERE album = '{RECORD}'")), "this");
        assert_eq!(count(&u, "record_references"), 1);
    }

    #[test]
    fn what_was_heard_stays_with_its_copy() {
        let u = with(&format!("INSERT INTO intake_heard (album, at, heard) VALUES ('{MAIN}', 5, 'mp3s'), ('{OTHER}', 6, 'flacs');"));
        u.move_albums(&Albums { copies: vec![copy(MAIN, RECORD, 1), copy(OTHER, RECORD, 3)], bases: Vec::new() }).unwrap();
        assert_eq!(u.intake_heard(RECORD, Some(1)).unwrap(), Some((5, "mp3s".to_string())));
        assert_eq!(u.intake_heard(RECORD, Some(3)).unwrap(), Some((6, "flacs".to_string())));
        assert_eq!(u.intake_heard(RECORD, Some(4)).unwrap(), None, "a copy nobody heard");
        assert_eq!(u.intake_heard(RECORD, None).unwrap(), Some((6, "flacs".to_string())), "with none named, the last heard");
    }

    #[test]
    fn of_two_hearings_of_one_copy_the_later_stands() {
        let u = with(&format!("INSERT INTO intake_heard (album, at, heard) VALUES ('{MAIN}', 5, 'mp3s'), ('{OTHER}', 6, 'flacs');"));
        u.move_albums(&Albums { copies: vec![copy(MAIN, RECORD, 1), copy(OTHER, RECORD, 1)], bases: Vec::new() }).unwrap();
        assert_eq!(count(&u, "intake_heard"), 1);
        assert_eq!(u.intake_heard(RECORD, Some(1)).unwrap(), Some((6, "flacs".to_string())));
    }

    #[test]
    fn genres_stand_over_a_search_that_found_none() {
        let u = with(&format!("INSERT INTO discogs_genres (album, genres, at) VALUES ('{MAIN}', '', 5), ('{OTHER}', 'Dub', 6);"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<String>(&u, &format!("SELECT genres FROM discogs_genres WHERE album = '{RECORD}'")), "Dub");
        assert_eq!(count(&u, "discogs_genres"), 1);
    }

    #[test]
    fn of_two_release_types_the_higher_rule_then_one_with_a_type_stands() {
        let (a, b) = ("al-aaaaaaaaaaaaaaaaaaaaab", "al-bbbbbbbbbbbbbbbbbbbbba");
        let u = with(&format!(
            "INSERT INTO release_types (key, primary_type, secondary, at, rule) VALUES
               ('dc:{MAIN}', 'album', '', 5, 1), ('dc:{OTHER}', 'ep', '', 5, 2), ('dc:{a}', NULL, '', 5, 2), ('dc:{b}', 'single', '', 5, 2), ('mb:0001', 'album', '', 5, 2);"
        ));
        let copies = vec![copy(MAIN, "al-1", 0), copy(OTHER, "al-1", 1), copy(a, "al-2", 0), copy(b, "al-2", 1)];
        u.move_albums(&Albums { copies, bases: Vec::new() }).unwrap();
        let kind = |key: &str| one::<Option<String>>(&u, &format!("SELECT primary_type FROM release_types WHERE key = '{key}'"));
        assert_eq!(kind("dc:al-1").as_deref(), Some("ep"));
        assert_eq!(kind("dc:al-2").as_deref(), Some("single"));
        assert_eq!(count(&u, "release_types"), 3, "a MusicBrainz release's stays");
    }

    #[test]
    fn a_renumber_match_is_kept_once() {
        let u = with(&format!("INSERT INTO renumber_matched (album, at) VALUES ('{MAIN}', 5), ('{OTHER}', 6);"));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(count(&u, &format!("renumber_matched WHERE album = '{RECORD}'")), 1);
        assert_eq!(count(&u, "renumber_matched"), 1);
    }

    /// A write older than an item's last change changes nothing: left behind, an old write wins over a newer heart.
    #[test]
    fn of_two_last_changes_the_later_stands() {
        let (main, other, moved) = (item_id("album", KIND.id, MAIN), item_id("album", KIND.id, OTHER), item_id("album", KIND.id, RECORD));
        let u = with(&format!(
            "INSERT INTO change_at (account, kind, item, at) VALUES
               ('ann', 'favourite', '{main}', 200), ('ann', 'favourite', '{other}', 300), ('ann', 'favourite', 'album:jf/1', 7), ('ann', 'favourite', 'track:cratarr/tr-1', 7);"
        ));
        u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(one::<i64>(&u, &format!("SELECT at FROM change_at WHERE item = '{moved}'")), 300);
        assert_eq!(count(&u, "change_at"), 3, "another server's album and a track stay");
    }

    /// Rows naming old ids can only mislead a device, so they go, and one that asks from before them is told to read it all.
    #[test]
    fn the_change_log_forgets_old_albums_and_a_device_reads_everything_again() {
        let u = with(&format!(
            "INSERT INTO change_log (rev, account, kind, item, op, at) VALUES
               (1, 'ann', 'favourite', 'album:cratarr/{MAIN}', 'changed', 1), (2, 'ann', 'rating', 'album:cratarr/{MAIN}', 'changed', 2),
               (3, 'ann', 'favourite', 'track:cratarr/tr-1', 'changed', 3), (4, 'ann', 'favourite', 'album:jf/1', 'changed', 4);"
        ));
        let report = u.move_albums(&on_record(&[MAIN])).unwrap();
        assert_eq!(count(&u, "change_log"), 2, "a track's and another server's stay");
        assert_eq!(report.table_named("change_log: albums").unwrap().before, 2);
        assert!(matches!(u.changes_since(Some(1), "ann", "", &ChangeKind::ALL, 10).unwrap(), Since::Reset { .. }));
        assert!(matches!(u.changes_since(Some(4), "ann", "", &ChangeKind::ALL, 10).unwrap(), Since::Changes { .. }), "what a device has read all of stays as it is");
    }

    /// Tags, hand facts and words show on both today, each album finding them by the base.
    #[test]
    fn one_base_on_two_records_is_copied_to_both() {
        let u = with(&every_place(MAIN));
        let report = u.move_albums(&Albums { copies: Vec::new(), bases: vec![base(MAIN, "al-1"), base(MAIN, "al-2")] }).unwrap();
        for (table, column, prefix) in BASE_FORM {
            for record in ["al-1", "al-2"] {
                assert_eq!(count(&u, &format!("{table} WHERE {column} = '{prefix}{record}'")), 1, "{table} on {record}");
            }
            assert_eq!(count(&u, &format!("{table} WHERE {column} = '{prefix}{MAIN}'")), 0, "{table} left behind");
        }
        let t = report.table_named("album_tags").unwrap();
        assert_eq!((t.before, t.after, t.moved), (1, 2, 1));
    }

    /// One string, two albums: as an id a Music album's, as a base an Incoming one's. A single map hands one the other's plays
    /// or tags.
    #[test]
    fn the_two_forms_are_kept_apart() {
        let u = with(&every_place(MAIN));
        u.move_albums(&Albums { copies: vec![copy(MAIN, "al-1", 0)], bases: vec![base(MAIN, "al-2")] }).unwrap();
        for (table, column, prefix) in BASE_FORM {
            assert_eq!(count(&u, &format!("{table} WHERE {column} = '{prefix}al-2'")), 1, "{table} goes by the base");
        }
        for (table, column, prefix) in ID_FORM {
            assert_eq!(count(&u, &format!("{table} WHERE {column} = '{prefix}al-1'")), 1, "{table} goes by a copy's id");
        }
    }

    /// An album's hash from before a retag cannot be read back: its rows wait under it, each counted and its id named once, and
    /// what has names keeps them.
    #[test]
    fn rows_that_match_nothing_are_kept_and_counted() {
        let u = with(&format!("{} INSERT INTO annotation (user, item, kind, play_count) VALUES ('bob', '{GONE}', 'album', 1);", every_place(GONE)));
        let before = all_rows(&u);
        let report = u.move_albums(&on_record(&[MAIN])).unwrap();
        assert_eq!(all_rows(&u), before, "nothing moved, nothing lost");
        assert_eq!(report.unmatched_count(), ID_FORM.len() as u64 + BASE_FORM.len() as u64 + 1);
        let hearts = report.unmatched.iter().find(|x| x.what == "annotation: albums").unwrap();
        assert_eq!((hearts.count, hearts.sample.as_slice()), (2, [GONE.to_string()].as_slice()), "two rows, one id");
        let words = report.unmatched.iter().find(|x| x.what == "words: albums").unwrap();
        assert_eq!(words.sample, [GONE.to_string()], "the id, not how the table says it");
        let t = report.table_named("record_overrides").unwrap();
        assert_eq!((t.before, t.after, t.moved), (1, 1, 0));
        assert_eq!(u.review(GONE, None).unwrap().unwrap().name, "N");
    }

    /// A folder run on this build wrote under the record, as nothing else was served: no row of it is lost track of.
    #[test]
    fn an_id_for_life_counts_as_moved() {
        let u = with(&every_place("al-12"));
        let before = all_rows(&u);
        let report = u.move_albums(&on_record(&[MAIN])).unwrap();
        assert_eq!(report.unmatched_count(), 0, "{:?}", report.unmatched);
        assert_eq!(all_rows(&u), before);
        for t in &report.tables {
            assert_eq!(t.moved, t.before, "{}", t.name);
        }
        assert_eq!(report.table_named("album_tags").unwrap().moved, 1);
    }

    #[test]
    fn moving_albums_twice_changes_nothing() {
        let u = with(&format!("{}{}{}", every_place(MAIN), every_place(OTHER), every_place(GONE)));
        u.db().execute_batch(&format!("UPDATE album_review SET undo = 'job:{MAIN}' WHERE album = '{MAIN}';")).unwrap();
        let first_run = u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        let once = all_rows(&u);
        let again = u.move_albums(&on_record(&[MAIN, OTHER])).unwrap();
        assert_eq!(all_rows(&u), once);
        assert_eq!(again.tables.iter().map(|t| t.merged).sum::<u64>(), 0, "nothing left to meet");
        assert_eq!(again.unmatched, first_run.unmatched, "and what matched nothing still does");
    }

    /// The first start writes this into its part, last in its transaction, and `/api/admin/migration` reads it as any other.
    #[test]
    fn the_report_reads_as_every_part_of_the_migrations_does() {
        let u = with(&every_place(GONE));
        let report = u.move_albums(&Albums::default()).unwrap();
        put_migration_part(&u.db(), "records", &serde_json::to_value(&report).unwrap()).unwrap();
        let (_, part) = u.migration_report().unwrap().remove("records").unwrap();
        let table = part["tables"].as_array().unwrap().iter().find(|t| t["name"] == "added").unwrap();
        assert_eq!((table["before"].as_u64(), table["after"].as_u64()), (Some(1), Some(1)));
        let row = part["unmatched"].as_array().unwrap().iter().find(|t| t["what"] == "added").unwrap();
        assert_eq!((row["count"].as_u64(), &row["sample"]), (Some(1), &serde_json::json!([GONE])));
    }

    #[test]
    fn what_stays_as_it_is_is_counted() {
        let u = with(&format!(
            "INSERT INTO classical_marks (album, mark, at) VALUES ('{MAIN}', 'classical', 5);
             INSERT INTO kept_answer (user, key, asked, status, kind, body, at) VALUES ('ann', 'k', 'POST /x', 200, NULL, x'00', 5);"
        ));
        let report = u.move_albums(&on_record(&[MAIN])).unwrap();
        for table in ["classical_marks", "kept_answer"] {
            let t = report.table_named(table).unwrap();
            assert_eq!((t.before, t.after, t.moved), (1, 1, 0), "{table}");
        }
        assert_eq!(count(&u, &format!("classical_marks WHERE album = '{MAIN}'")), 1);
    }

    /// On what 0.15 really left (`tests/fixtures/0.15`). The map is by hand, the fixture's ids being the tag's: Both Shores is
    /// in Music and in both Incomings, one record, Just Arrived waits in Incoming under a base of its own, and the rest is what an
    /// id no album has now finds, each by the copy where it lies.
    #[test]
    fn the_data_of_0_15_leaves_no_old_album_id_where_each_has_a_record() {
        let d = std::env::temp_dir().join(format!("cratarr-albums-0-15-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("userdata.sqlite");
        Connection::open(&file).unwrap().execute_batch(include_str!("../../tests/fixtures/0.15/userdata.sql")).unwrap();
        let u = UserData::open(Some(&file)).unwrap();
        let (shores, arrived, bank) = ("al-2", "al-6", "al-7");
        let mut copies = vec![
            copy("al-4FwVmWLNdYpZZSEtkan6Uy", "al-1", 0),
            copy("al-4Jx9dQszNKgnbo4mZtic7y", shores, 0),
            copy("al-0K3QLxUBwKPLshc0VyNQJ7", shores, 1),
            copy("al-7ATEPHL21gflZtRCnZNzGM", "al-3", 0),
            copy("al-6CJXFC1mh9bVtzcdfQX8rc", "al-4", 0),
            copy("al-2c5BBHDvIsL51C4yFB1p80", "al-5", 0),
            copy("al-5CBklbfbpDtbwWYNnJiHHl", arrived, 1),
        ];
        let mut bases: Vec<(String, String)> = copies.iter().filter(|c| c.folder == 0).map(|c| base(&c.id, &c.record)).collect();
        bases.extend([base("al-6D7A0468643kEoStkiZtlE", arrived), base("al-3QRfi6o7OjZJyfmLivEPJF", bank), base("al-24DIhgl7Uwd1I5YdD7vYgT", "al-10")]);
        copies.extend([
            copy("al-4atTH2lV1wmvNeyF1zn8Rr", shores, 2),
            copy("al-2NkuHxIRsSvRXBHij5Jj7H", "al-3", 0),
            copy("al-6D7A0468643kEoStkiZtlE", arrived, 1),
            copy("al-5kyy3QkYKUCRrEIvEzhxHQ", arrived, 1),
            copy("al-4iYsod5k8p0gnwEbQmevTB", bank, 2),
            copy("al-16xHJujSyQP4kXgPnlrD5a", bank, 2),
            copy("al-4I3hKdUhPvMN5OjdbmOVtr", "al-8", 2),
            copy("al-1CS3f2lrsGgYjTeBz843i8", "al-9", 0),
            copy("al-3UHzSSlNKDLqekltbS5Jq9", "al-10", 1),
            copy("al-2LUOgL2YVVkp2U1DSUnrEZ", "al-11", -1),
        ]);
        let report = u.move_albums(&Albums { copies, bases }).unwrap();
        assert_eq!(report.unmatched_count(), 0, "{:?}", report.unmatched);
        let hash = |text: &str| text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').any(|w| w.len() == 25 && w.starts_with("al-"));
        let held = [("album_review", "base"), ("album_review", "undo"), ("change_log", "item")];
        for (table, column) in ID_FORM.iter().chain(&BASE_FORM).map(|(table, column, _)| (*table, *column)).chain(held) {
            let db = u.db();
            let mut st = db.prepare(&format!("SELECT {column} FROM {table} WHERE {column} IS NOT NULL")).unwrap();
            let left: Vec<String> = st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).filter(|text| hash(text)).collect();
            assert!(left.is_empty(), "{table}.{column}: {left:?}");
        }
        let ann = u.get("ann", Kind::Album, &[shores.to_string()]).unwrap().remove(shores).unwrap();
        assert_eq!(ann.play_count, 3, "both copies' plays");
        assert_eq!(count(&u, &format!("annotation WHERE kind = 'album' AND user = 'ann' AND item = '{shores}'")), 1);
        assert_eq!(one::<i64>(&u, &format!("SELECT at FROM added WHERE album = '{shores}'")), 1760920000, "the earlier");
        let kept = u.review(shores, Some(1)).unwrap().unwrap();
        assert_eq!((kept.state.as_str(), kept.folder), ("promoted", 1));
        assert_eq!(u.review(arrived, Some(1)).unwrap().unwrap().undo, Some(format!("job:{arrived}")));
        assert!(u.intake_heard(arrived, Some(1)).unwrap().is_some());
        assert_eq!(u.review(bank, Some(2)).unwrap().unwrap().state, "rejected", "from before its library changed place");
        assert!(u.intake_heard(bank, Some(2)).unwrap().is_some());
        assert_eq!(one::<String>(&u, "SELECT value FROM record_overrides WHERE album = 'al-4' AND field = 'classical'"), "true", "the marks moved at open, then with the facts");
        assert_eq!(count(&u, "change_log WHERE kind = 'favourite' AND item LIKE 'album:%'"), 0);
        drop(u);
        std::fs::remove_dir_all(&d).ok();
    }
}
