//! What the user data holds of an artist, moved from the name's hash to the artist's id for life (0.16,
//! docs/records-and-copies.md: "gives each artist an id and rewrites what names the name's hash"): a favourite, plays and a
//! rating (`annotation`); what an import brought (`imported`, which the next import reads by id: left behind, it counts
//! everything as new); the words (`words`, `words_asked`: left behind, every source is asked again); and the library's
//! artists inside `related_artists.found`. What goes by a MusicBrainz id or a loose name stays as it is (Q21, `rest` 8.22
//! to 8.28). Nothing calls this until the first start does.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params};

use super::UserData;
use crate::moved::Report;
use crate::serve::ids;

/// An `annotation` row under its artist's id. Two rows of one artist are read as any two annotations of one item
/// (`combine_user_data`): plays added, the later dates, the better rating, the first star.
const ANNOTATION: &str = "
INSERT INTO annotation (user, item, kind, pid, play_count, play_date, rating, starred, starred_at, own_count, own_date)
  SELECT user, ?3, kind, pid, play_count, play_date, rating, starred, starred_at, own_count, own_date
  FROM annotation WHERE user = ?1 AND item = ?2 AND kind = 'artist'
  ON CONFLICT (user, item, kind) DO UPDATE SET
    play_count = play_count + excluded.play_count,
    play_date = max(coalesce(play_date, excluded.play_date), coalesce(excluded.play_date, play_date)),
    rating = max(rating, excluded.rating),
    starred = max(starred, excluded.starred),
    starred_at = min(coalesce(starred_at, excluded.starred_at), coalesce(excluded.starred_at, starred_at)),
    own_count = own_count + excluded.own_count,
    own_date = max(coalesce(own_date, excluded.own_date), coalesce(excluded.own_date, own_date));
";

/// An `imported` row under its artist's id. Two rows of one artist: plays added, the later play, a star on either, the
/// better rating; the server's item stays the first's, as what it names on the server.
const IMPORTED: &str = "
INSERT INTO imported (account, source, server_user, kind, item, server_item, pid, plays, played, starred, rating, at)
  SELECT account, source, server_user, kind, ?5, server_item, pid, plays, played, starred, rating, at
  FROM imported WHERE account = ?1 AND source = ?2 AND server_user = ?3 AND kind = 'artist' AND item = ?4
  ON CONFLICT (account, source, server_user, kind, item) DO UPDATE SET
    plays = plays + excluded.plays,
    played = max(coalesce(played, excluded.played), coalesce(excluded.played, played)),
    starred = max(starred, excluded.starred),
    rating = max(rating, excluded.rating),
    at = max(at, excluded.at);
";

/// A word's row under its artist's id: of two on one word, the later.
const WORDS: &str = "
INSERT INTO words (subject, source, word, kind, weight, at)
  SELECT ?2, source, word, kind, weight, at FROM words WHERE subject = ?1
  ON CONFLICT (subject, source, word) DO UPDATE SET kind = excluded.kind, weight = excluded.weight, at = excluded.at
  WHERE excluded.at > at;
";

/// When a source was last asked of an artist, under their id: of two, the later.
const WORDS_ASKED: &str = "
INSERT INTO words_asked (subject, source, at, ok)
  SELECT ?2, source, at, ok FROM words_asked WHERE subject = ?1
  ON CONFLICT (subject, source) DO UPDATE SET at = excluded.at, ok = excluded.ok
  WHERE excluded.at > at;
";

/// Whether an id still wants moving: an id for life has moved already, so a step run again says nothing of it.
fn old(id: &str) -> bool {
    !ids::for_life(id)
}

fn annotations(conn: &Connection, moved: &HashMap<String, String>, report: &mut Report) -> Result<()> {
    let rows: Vec<(String, String)> = conn.prepare("SELECT user, item FROM annotation WHERE kind = 'artist'")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let (before, mut done, mut unmatched) = (rows.len() as u64, 0, Vec::new());
    for (user, item) in rows {
        match moved.get(&item) {
            Some(new) => {
                conn.execute(ANNOTATION, params![user, item, new])?;
                conn.execute("DELETE FROM annotation WHERE user = ?1 AND item = ?2 AND kind = 'artist'", params![user, item])?;
                done += 1;
            }
            None if old(&item) => unmatched.push(item),
            None => {}
        }
    }
    let after: i64 = conn.query_row("SELECT count(*) FROM annotation WHERE kind = 'artist'", [], |r| r.get(0))?;
    report.table("annotation: artists", before, after as u64, done);
    report.unmatched("annotation: artists", unmatched);
    Ok(())
}

fn imported(conn: &Connection, moved: &HashMap<String, String>, report: &mut Report) -> Result<()> {
    let mut st = conn.prepare("SELECT account, source, server_user, item FROM imported WHERE kind = 'artist'")?;
    let rows: Vec<(String, String, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    let (before, mut done, mut unmatched) = (rows.len() as u64, 0, Vec::new());
    for (account, source, user, item) in rows {
        match moved.get(&item) {
            Some(new) => {
                conn.execute(IMPORTED, params![account, source, user, item, new])?;
                conn.execute("DELETE FROM imported WHERE account = ?1 AND source = ?2 AND server_user = ?3 AND kind = 'artist' AND item = ?4", params![account, source, user, item])?;
                done += 1;
            }
            None if old(&item) => unmatched.push(item),
            None => {}
        }
    }
    let after: i64 = conn.query_row("SELECT count(*) FROM imported WHERE kind = 'artist'", [], |r| r.get(0))?;
    report.table("imported: artists", before, after as u64, done);
    report.unmatched("imported: artists", unmatched);
    Ok(())
}

/// `artist:<id>` subjects of `table`. `upsert` writes one subject's rows under another, `?1` the old, `?2` the new.
fn subjects(conn: &Connection, table: &str, upsert: &str, moved: &HashMap<String, String>, report: &mut Report) -> Result<()> {
    const ARTIST: &str = "artist:";
    let count = || -> rusqlite::Result<u64> { conn.query_row(&format!("SELECT count(*) FROM {table} WHERE subject LIKE 'artist:%'"), [], |r| r.get::<_, i64>(0)).map(|n| n as u64) };
    let before = count()?;
    let found: Vec<String> = conn.prepare(&format!("SELECT DISTINCT subject FROM {table} WHERE subject LIKE 'artist:%'"))?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let (mut done, mut unmatched) = (0, Vec::new());
    for subject in found {
        let id = subject.strip_prefix(ARTIST).unwrap_or(&subject);
        match moved.get(id) {
            Some(new) => {
                conn.execute(upsert, params![subject, format!("{ARTIST}{new}")])?;
                done += conn.execute(&format!("DELETE FROM {table} WHERE subject = ?1"), [&subject])? as u64;
            }
            None if old(id) => unmatched.push(id.to_string()),
            None => {}
        }
    }
    report.table(&format!("{table}: artists"), before, count()?, done);
    report.unmatched(&format!("{table}: artists"), unmatched);
    Ok(())
}

/// The library's artists inside `related_artists.found`, `artists[].id`: the row's key is a MusicBrainz id, which stays.
/// A related artist the library has no id for is `null` there, and asks for nothing.
fn related(conn: &Connection, moved: &HashMap<String, String>, report: &mut Report) -> Result<()> {
    let rows: Vec<(String, String)> = conn.prepare("SELECT artist, found FROM related_artists")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let (before, mut done, mut unmatched) = (rows.len() as u64, 0, Vec::new());
    for (artist, found) in rows {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&found) else {
            unmatched.push(artist);
            continue;
        };
        let mut changed = false;
        for id in value.get_mut("artists").and_then(|a| a.as_array_mut()).into_iter().flatten().filter_map(|a| a.get_mut("id")) {
            let Some(text) = id.as_str() else { continue };
            match moved.get(text) {
                Some(new) => {
                    *id = new.as_str().into();
                    changed = true;
                }
                None if old(text) => unmatched.push(text.to_string()),
                None => {}
            }
        }
        if changed {
            conn.execute("UPDATE related_artists SET found = ?2 WHERE artist = ?1", params![artist, value.to_string()])?;
            done += 1;
        }
    }
    report.table("related_artists.found", before, before, done);
    report.unmatched("related_artists.found", unmatched);
    Ok(())
}

/// The artists moved by `moved` (old id to new, `Catalogue::artists_moved`), in the caller's transaction: the user data's
/// move is one, its report written last in it. Safe to repeat: a row moved is under an id no map holds. A row whose id no map
/// holds stays as it is, and is counted: the hash of a name nothing credits now cannot be read back, so a heart on an artist
/// whose music left stays under it.
pub(crate) fn move_artists(conn: &Connection, moved: &HashMap<String, String>) -> Result<Report> {
    let mut report = Report::default();
    annotations(conn, moved, &mut report)?;
    imported(conn, moved, &mut report)?;
    subjects(conn, "words", WORDS, moved, &mut report)?;
    subjects(conn, "words_asked", WORDS_ASKED, moved, &mut report)?;
    related(conn, moved, &mut report)?;
    Ok(report)
}

impl UserData {
    /// `move_artists` in a transaction of its own.
    pub fn move_artists(&self, moved: &HashMap<String, String>) -> Result<Report> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let report = move_artists(&tx, moved)?;
        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::{ImportChange, ImportKey, Imported, Kind};
    use super::*;

    const OLD: &str = "ar-5vcUvYmuWJAgyxLXEUmaPz";
    const NEW: &str = "ar-7";

    fn map() -> HashMap<String, String> {
        HashMap::from([(OLD.to_string(), NEW.to_string())])
    }

    fn one(u: &UserData, sql: &str) -> i64 {
        u.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn an_artists_heart_plays_and_rating_move_to_their_id() {
        let u = UserData::open(None).unwrap();
        u.set_starred("ann", Kind::Artist, &[OLD.to_string()], true).unwrap();
        u.set_rating("ann", Kind::Artist, OLD, 4).unwrap();
        u.count_play("bob", ("tr-1", "p"), "al-1", &[OLD.to_string()], 100).unwrap();
        let report = u.move_artists(&map()).unwrap();
        let ann = u.get("ann", Kind::Artist, &[NEW.to_string()]).unwrap().remove(NEW).unwrap();
        assert_eq!((ann.rating, ann.starred_at.is_some()), (4, true));
        assert_eq!(u.get("bob", Kind::Artist, &[NEW.to_string()]).unwrap()[NEW].play_count, 1);
        assert!(u.get("ann", Kind::Artist, &[OLD.to_string()]).unwrap().is_empty(), "no row left behind");
        let t = report.table_named("annotation: artists").unwrap();
        assert_eq!((t.before, t.after, t.moved), (2, 2, 2));
        assert_eq!(u.get("bob", Kind::Song, &["tr-1".to_string()]).unwrap()["tr-1"].play_count, 1, "a song's row is no artist's");
        assert_eq!(u.get("bob", Kind::Album, &["al-1".to_string()]).unwrap()["al-1"].play_count, 1, "nor an album's");
    }

    /// A folder run on this build before the move has rows under both ids: read as two annotations of one artist, or a star
    /// or a play is lost, as `UPDATE OR IGNORE` would lose one.
    #[test]
    fn two_rows_of_one_artist_fold_as_any_two_annotations() {
        let u = UserData::open(None).unwrap();
        u.db()
            .execute_batch(
                "INSERT INTO annotation (user, item, kind, play_count, play_date, rating, starred, starred_at, own_count, own_date)
                   VALUES ('ann', 'ar-5vcUvYmuWJAgyxLXEUmaPz', 'artist', 3, 100, 2, 0, NULL, 2, 90),
                          ('ann', 'ar-7', 'artist', 4, 50, 5, 1, 70, 1, NULL);",
            )
            .unwrap();
        u.move_artists(&map()).unwrap();
        let row: (i64, Option<i64>, i64, i64, Option<i64>, i64, Option<i64>) = u
            .db()
            .query_row("SELECT play_count, play_date, rating, starred, starred_at, own_count, own_date FROM annotation WHERE kind = 'artist'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
            })
            .unwrap();
        assert_eq!(row, (7, Some(100), 5, 1, Some(70), 3, Some(90)), "plays added, the later dates, the better rating, the star");
        assert_eq!(one(&u, "SELECT count(*) FROM annotation"), 1);
    }

    #[test]
    fn what_an_import_brought_moves_or_the_next_import_counts_it_as_new() {
        let u = UserData::open(None).unwrap();
        let key = ImportKey { account: "ann", source: "jf", server_user: "u1" };
        let now = Imported { server_item: Some("r1".into()), starred: true, ..Imported::default() };
        u.apply_import(key, &[ImportChange { kind: Kind::Artist, item: OLD.to_string(), star: Some((true, Some(5))), now, ..ImportChange::default() }]).unwrap();
        let report = u.move_artists(&map()).unwrap();
        let last = u.imported(key, Kind::Artist, &[NEW.to_string()]).unwrap();
        assert_eq!(last[NEW].server_item.as_deref(), Some("r1"), "the next import reads it by the new id");
        assert!(u.imported(key, Kind::Artist, &[OLD.to_string()]).unwrap().is_empty());
        assert_eq!(report.table_named("imported: artists").unwrap().moved, 1);
    }

    #[test]
    fn an_artists_words_and_who_was_asked_move_or_every_source_is_asked_again() {
        let u = UserData::open(None).unwrap();
        u.db()
            .execute_batch(
                "INSERT INTO words (subject, source, word, kind, weight, at) VALUES
                   ('artist:ar-5vcUvYmuWJAgyxLXEUmaPz', 'lastfm', 'dub', 'tag', 0.8, 10),
                   ('artist:ar-5vcUvYmuWJAgyxLXEUmaPz', 'lastfm', 'old', 'tag', 0.8, 10),
                   ('artist:ar-7', 'lastfm', 'old', 'tag', 0.1, 20),
                   ('artist:ar-7', 'lastfm', 'dub', 'tag', 0.1, 5),
                   ('album:al-1', 'lastfm', 'dub', 'tag', 0.8, 10);
                 INSERT INTO words_asked (subject, source, at, ok) VALUES ('artist:ar-5vcUvYmuWJAgyxLXEUmaPz', 'lastfm', 30, 1), ('artist:ar-7', 'musicbrainz', 40, 1);",
            )
            .unwrap();
        let report = u.move_artists(&map()).unwrap();
        assert_eq!(one(&u, "SELECT count(*) FROM words WHERE subject LIKE '%5vcUvY%'"), 0);
        assert_eq!(one(&u, "SELECT count(*) FROM words WHERE subject = 'artist:ar-7'"), 2, "two words, one row each");
        let weight = |word: &str| -> f64 { u.db().query_row("SELECT weight FROM words WHERE subject = 'artist:ar-7' AND word = ?1", [word], |r| r.get(0)).unwrap() };
        assert_eq!((weight("dub"), weight("old")), (0.8, 0.1), "of two the later");
        assert_eq!(one(&u, "SELECT count(*) FROM words_asked WHERE subject = 'artist:ar-7'"), 2);
        assert_eq!(one(&u, "SELECT count(*) FROM words WHERE subject = 'album:al-1'"), 1, "an album's words are no artist's");
        assert_eq!(report.table_named("words: artists").unwrap().before, 4);
    }

    #[test]
    fn a_related_artist_names_the_new_id_inside_its_json() {
        let u = UserData::open(None).unwrap();
        let found = serde_json::json!({ "at": 5, "artists": [
            { "name": "A", "why": [], "id": OLD, "score": 0.5 },
            { "name": "B", "why": [], "id": null, "score": 0.4 },
            { "name": "C", "why": [], "id": "ar-gone00000000000000000", "score": 0.3 },
            { "name": "D", "why": [], "id": "ar-9", "score": 0.2 },
        ] });
        u.set_related_artists("mbid", 5, &found.to_string()).unwrap();
        let report = u.move_artists(&map()).unwrap();
        let (at, kept) = u.related_artists("mbid").unwrap().unwrap();
        let kept: serde_json::Value = serde_json::from_str(&kept).unwrap();
        let ids: Vec<&serde_json::Value> = kept["artists"].as_array().unwrap().iter().map(|a| &a["id"]).collect();
        assert_eq!(ids, [&serde_json::json!(NEW), &serde_json::Value::Null, &serde_json::json!("ar-gone00000000000000000"), &serde_json::json!("ar-9")]);
        assert_eq!(at, 5, "the row is read again when its time comes, as it was");
        let u2 = report.unmatched.iter().find(|x| x.what == "related_artists.found").unwrap();
        assert_eq!((u2.count, u2.sample.as_slice()), (1, ["ar-gone00000000000000000".to_string()].as_slice()), "the new id is no old one");
    }

    /// The hash of a name nothing credits cannot be read back: a heart on an artist whose music left stays, counted, and
    /// its table still says how many it holds.
    #[test]
    fn rows_that_match_no_artist_are_kept_and_counted() {
        let u = UserData::open(None).unwrap();
        u.set_starred("ann", Kind::Artist, &["ar-nobodynobodynobody000".to_string(), "ar-12".to_string()], true).unwrap();
        let report = u.move_artists(&map()).unwrap();
        assert_eq!(u.starred("ann", Kind::Artist).unwrap().len(), 2, "kept");
        let what = report.unmatched.iter().find(|x| x.what == "annotation: artists").unwrap();
        assert_eq!((what.count, what.sample.as_slice()), (1, ["ar-nobodynobodynobody000".to_string()].as_slice()), "an id for life is no old one");
        let t = report.table_named("annotation: artists").unwrap();
        assert_eq!((t.before, t.after, t.moved), (2, 2, 0));
    }

    #[test]
    fn moving_artists_twice_changes_nothing() {
        let u = UserData::open(None).unwrap();
        u.set_starred("ann", Kind::Artist, &[OLD.to_string()], true).unwrap();
        u.set_related_artists("mbid", 5, &serde_json::json!({ "at": 5, "artists": [{ "name": "A", "why": [], "id": OLD, "score": 0.5 }] }).to_string()).unwrap();
        u.move_artists(&map()).unwrap();
        let kept = u.related_artists("mbid").unwrap();
        let again = u.move_artists(&map()).unwrap();
        assert_eq!(again.tables.iter().map(|t| t.moved).sum::<u64>(), 0);
        assert_eq!(again.unmatched_count(), 0);
        assert_eq!(u.related_artists("mbid").unwrap(), kept);
        assert_eq!(u.starred("ann", Kind::Artist).unwrap(), [NEW.to_string()]);
    }

    /// On what 0.15 really left (`tests/fixtures/0.15`): the fixture's hashes are the tag's, so every row of an artist the
    /// files credit has its id in the map the catalogue's table makes, with no name to look up.
    #[test]
    fn the_data_of_0_15_moves_every_artist_its_files_credit() {
        let d = std::env::temp_dir().join(format!("cratarr-artists-0-15-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("userdata.sqlite");
        Connection::open(&file).unwrap().execute_batch(include_str!("../../tests/fixtures/0.15/userdata.sql")).unwrap();
        let u = UserData::open(Some(&file)).unwrap();
        let files = Connection::open_in_memory().unwrap();
        files.execute_batch(include_str!("../../tests/fixtures/0.15/catalogue.sql")).unwrap();
        let names: BTreeSet<String> = files.prepare("SELECT DISTINCT value FROM tags WHERE name IN ('artist', 'albumartist')").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        cat.artists_for(&names).unwrap();
        cat.note_artists_moved().unwrap();
        let moved = cat.artists_moved().unwrap();
        let report = u.move_artists(&moved).unwrap();
        assert_eq!(report.unmatched_count(), 0, "{:?}", report.unmatched);
        let held = |sql: &str| one(&u, sql);
        assert_eq!(report.table_named("annotation: artists").unwrap().moved, 2);
        assert_eq!(report.table_named("imported: artists").unwrap().moved, 1);
        assert_eq!(report.table_named("words: artists").unwrap().moved, 1);
        assert_eq!(report.table_named("words_asked: artists").unwrap().moved, 1);
        assert_eq!(report.table_named("related_artists.found").unwrap().moved, 1);
        assert_eq!(held("SELECT count(*) FROM annotation WHERE kind = 'artist' AND length(item) > 15"), 0);
        assert_eq!(held("SELECT count(*) FROM annotation WHERE kind = 'album' AND length(item) > 15"), 4, "the albums are the migration's other step");
        drop(u);
        std::fs::remove_dir_all(&d).ok();
    }
}
