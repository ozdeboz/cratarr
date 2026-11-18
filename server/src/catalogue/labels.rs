//! A label's id for life (decision 0100, C46): a number given once per name, as an artist's is, and served as `la-<id>`. A
//! row stays when nothing names the label any more, so it comes back to its id. Its records are `record_labels`, written by
//! the record pass from what the files name, and by a row of its page once opened (C56).
//!
//! A label About shows by its Discogs id or its MBID (F9) is the label holding that reference, else one made with it when it is
//! opened (`open_label`), by the reference alone: so two labels may share a name, as two artists may, and a label known by its
//! name alone is the one of its name only where exactly one has it (`labels_shown`, X8).
//!
//! A label's lists at MusicBrainz and Discogs are a cache, not records (C56, Q33): their rows are kept in `label_rows` thirty days
//! from when they were read, then let go of by RT01's daily pass, and a record is made of one only when it is opened.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::records::{Listing, Ref, Requested};
use super::{Catalogue, now};
use crate::serve::ids;

pub(super) const SCHEMA: &str = "
-- C56 (Q33, the owner): a label's rows as MusicBrainz and Discogs list them, a cache and not records: no record is made of one
-- until it is opened. `source`, `ref_kind`, `ref_value`: the row's reference there (a release group; a master, or a release
-- with none). Kept thirty days from when it was read (`at`), then let go of by RT01's daily pass.
CREATE TABLE IF NOT EXISTS label_rows (
  label INTEGER NOT NULL, source TEXT NOT NULL, ref_kind TEXT NOT NULL, ref_value TEXT NOT NULL,
  title TEXT NOT NULL, artist TEXT NOT NULL DEFAULT '', year INTEGER, catno TEXT NOT NULL DEFAULT '', kind TEXT, cover TEXT, at INTEGER NOT NULL,
  PRIMARY KEY (label, source, ref_kind, ref_value)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS label_rows_by_ref ON label_rows (source, ref_kind, ref_value);
-- C56 (OQ10 (a): a hundred at a time): how far a label's list at a source was read: when its first hundred was (`at`), and where
-- the next starts (MusicBrainz's offset, Discogs' page), none at its end or where the source does not know the label. Let go of
-- with the rows, thirty days on.
CREATE TABLE IF NOT EXISTS label_reads (label INTEGER NOT NULL, source TEXT NOT NULL, at INTEGER NOT NULL, next INTEGER, PRIMARY KEY (label, source)) WITHOUT ROWID;
";

/// How long a label's rows from a source are kept, and its list there counts as read (C56: thirty days, like Bandcamp finds).
pub const ROWS_KEPT: i64 = 30 * 86_400;

/// A row of a label's list at a source (C56), as read and kept: its reference there, what the source says of it, and its
/// catalogue number ('' none).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelListed {
    pub reference: Ref,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub catno: String,
    pub kind: Option<String>,
    pub cover: Option<String>,
}

/// How far a label's list at a source was read (C56): when its first hundred was, and where the next starts (none: its end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelRead {
    pub at: i64,
    pub next: Option<i64>,
}

/// A label as its page reads it from the catalogue (C56), on one reader.
#[derive(Debug, Clone)]
pub struct LabelKept {
    pub label: LabelRow,
    /// The records tied to it (`record_labels`), each as merged now, with its catalogue number there ('' none), in the records'
    /// order.
    pub tied: Vec<(i64, String)>,
    /// Its sources' kept rows, each with the record its reference names now (`records::found_by_reference`, as a row opened
    /// finds it), else the record the house has of its names that an open of it joins (F11.2, `records::had_by_names_in`); none: no
    /// record yet.
    pub rows: Vec<(LabelListed, Option<i64>)>,
    /// Every record above, read as the lists read records (`Requested`: the one had rule's holder), by id.
    pub records: HashMap<i64, Requested>,
    /// Where its list at each source stands, by source.
    pub reads: HashMap<String, LabelRead>,
    /// What each source knows it by.
    pub refs: Vec<Ref>,
}

/// A kept row's columns, as `listed_of` reads them.
const ROW_COLUMNS: &str = "source, ref_kind, ref_value, title, artist, year, catno, kind, cover";

fn listed_of(r: &rusqlite::Row) -> rusqlite::Result<LabelListed> {
    Ok(LabelListed {
        reference: Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? },
        title: r.get(3)?,
        artist: r.get(4)?,
        year: r.get(5)?,
        catno: r.get(6)?,
        kind: r.get(7)?,
        cover: r.get(8)?,
    })
}

/// A label as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRow {
    pub id: i64,
    pub name: String,
}

/// What makes two names one label: as two names are one artist, case and typographic quotes aside.
fn key(name: &str) -> String {
    ids::artist_key(name)
}

fn label_by_key(db: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(db.prepare_cached("SELECT min(id) FROM labels WHERE key = ?1")?.query_row([key], |r| r.get(0))?)
}

fn give_label(db: &Connection, key: &str, name: &str, born: i64) -> Result<i64> {
    db.prepare_cached("INSERT INTO labels (born, name, key) VALUES (?1, ?2, ?3)")?.execute(params![born, name, key])?;
    Ok(db.last_insert_rowid())
}

/// These names' labels by key, in the caller's transaction, each new one given its id in the names' order. Blocking.
pub(super) fn labels_in(tx: &Connection, names: &BTreeSet<String>, born: i64) -> Result<HashMap<String, i64>> {
    let mut known = HashMap::new();
    for name in names {
        let key = key(name);
        if known.contains_key(&key) {
            continue;
        }
        let id = match label_by_key(tx, &key)? {
            Some(id) => id,
            None => give_label(tx, &key, name, born)?,
        };
        known.insert(key, id);
    }
    Ok(known)
}

/// The label a label's reference names (F9): the label kept that holds it; none where none does. Blocking.
fn holding_in(db: &Connection, r: &Ref) -> Result<Option<i64>> {
    let subjects: Vec<String> = db
        .prepare_cached("SELECT subject FROM \"references\" WHERE source = ?1 AND kind = ?2 AND value = ?3 ORDER BY subject")?
        .query_map(params![r.source, r.kind, r.value], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in subjects.iter().filter_map(|s| ids::label_number(s)) {
        if db.prepare_cached("SELECT 1 FROM labels WHERE id = ?1")?.exists([id])? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

impl Catalogue {
    /// The label a name is, given an id if it has none. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a label's listing (C56)
    pub fn label_for(&self, name: &str) -> Result<i64> {
        let key = key(name);
        let db = self.db();
        match label_by_key(&db, &key)? {
            Some(id) => Ok(id),
            None => give_label(&db, &key, name, now()),
        }
    }

    /// A label by its id for life. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a label's page (C56)
    pub fn label(&self, id: i64) -> Result<Option<LabelRow>> {
        Ok(self.db().prepare_cached("SELECT id, name FROM labels WHERE id = ?1")?.query_row([id], |r| Ok(LabelRow { id: r.get(0)?, name: r.get(1)? })).optional()?)
    }

    /// A label's records, each with its catalogue number there ('' none), in the records' order. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // a label's page (C56)
    pub fn label_records(&self, label: i64) -> Result<Vec<(i64, String)>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT record, catno FROM record_labels WHERE label = ?1 ORDER BY record, catno")?;
        let rows = st.query_map([label], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A record's labels (C54: its page), each with its catalogue number there ('' none), in the order they were given ids. On a
    /// reader. Blocking.
    pub fn record_labels(&self, record: i64) -> Result<Vec<(LabelRow, String)>> {
        let read = |db: &Connection| -> Result<Vec<(LabelRow, String)>> {
            let mut st = db.prepare_cached("SELECT l.id, l.name, r.catno FROM record_labels r JOIN labels l ON l.id = r.label WHERE r.record = ?1 ORDER BY l.id, r.catno")?;
            let rows = st.query_map([record], |r| Ok((LabelRow { id: r.get(0)?, name: r.get(1)? }, r.get(2)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// A record's artists, in order. Blocking.
    #[cfg_attr(not(test), allow(dead_code))] // an artist's page and a record's (C54, C55)
    pub fn record_artists(&self, record: i64) -> Result<Vec<i64>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT artist FROM record_artists WHERE record = ?1 ORDER BY position")?;
        let rows = st.query_map([record], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A label as its page reads it (C56, `LabelKept`), on one reader, never the writer's lock: its records, its sources' kept
    /// rows with the record each names now, where its lists stand and what the sources know it by. None: no such label.
    /// Blocking.
    pub fn label_kept(&self, label: i64) -> Result<Option<LabelKept>> {
        let read = |db: &Connection| -> Result<Option<LabelKept>> {
            let Some(row) = db.prepare_cached("SELECT id, name FROM labels WHERE id = ?1")?.query_row([label], |r| Ok(LabelRow { id: r.get(0)?, name: r.get(1)? })).optional()? else {
                return Ok(None);
            };
            let ties: Vec<(i64, String)> =
                db.prepare_cached("SELECT record, catno FROM record_labels WHERE label = ?1 ORDER BY record, catno")?.query_map([label], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            let mut tied = Vec::with_capacity(ties.len());
            for (record, catno) in ties {
                if let Some(end) = super::records::merged_end(db, record)? {
                    tied.push((end, catno));
                }
            }
            let listed: Vec<LabelListed> = db
                .prepare_cached(&format!("SELECT {ROW_COLUMNS} FROM label_rows WHERE label = ?1 ORDER BY source, at, ref_kind, ref_value"))?
                .query_map([label], listed_of)?
                .collect::<rusqlite::Result<_>>()?;
            let mut rows = Vec::with_capacity(listed.len());
            let mut named = None;
            for l in listed {
                let mut found = super::records::found_by_reference(db, &Listing { refs: vec![l.reference.clone()], ..Default::default() })?;
                // F11.2: a row no record holds that a record the house has of its artist and title is one album with is that record's
                // row, as an open of it joins it (`records::had_by_names_in`): never shown beside it.
                if found.is_none() {
                    found = super::records::had_by_names_in(db, &[(std::slice::from_ref(&l.reference), l.artist.as_str(), l.title.as_str(), l.kind.as_deref())], &mut named)?
                        .pop()
                        .flatten()
                        .map(|(had, _)| had);
                }
                rows.push((l, found));
            }
            let wanted: Vec<i64> = tied.iter().map(|(r, _)| *r).chain(rows.iter().filter_map(|(_, r)| *r)).collect::<BTreeSet<_>>().into_iter().collect();
            let mut names = None;
            let records: HashMap<i64, Requested> = super::records::read_records_in(db, &wanted, &mut names)?.into_iter().map(|q| (q.row.id, q)).collect();
            let reads: HashMap<String, LabelRead> = db
                .prepare_cached("SELECT source, at, next FROM label_reads WHERE label = ?1")?
                .query_map([label], |r| Ok((r.get::<_, String>(0)?, LabelRead { at: r.get(1)?, next: r.get(2)? })))?
                .collect::<rusqlite::Result<_>>()?;
            let refs: Vec<Ref> = db
                .prepare_cached("SELECT source, kind, value FROM \"references\" WHERE subject = ?1 ORDER BY source, kind, value")?
                .query_map([ids::label(label)], |r| Ok(Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(Some(LabelKept { label: row, tied, rows, records, reads, refs }))
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The labels About shows, as known (F9), in their order, on one reader: one with a reference, the label that holds it, by it
    /// alone (a label of its name is not it); one with none, the one label of its name where exactly one has it (X8: a name two
    /// share, or none has, is neither; no name, none). Blocking.
    pub fn labels_shown(&self, asked: &[(Option<Ref>, String)]) -> Result<Vec<Option<i64>>> {
        let read = |db: &Connection| -> Result<Vec<Option<i64>>> {
            let mut out = Vec::with_capacity(asked.len());
            for (r, name) in asked {
                out.push(match r {
                    Some(r) => holding_in(db, r)?,
                    None if name.trim().is_empty() => None,
                    None => {
                        let of_name: Vec<i64> =
                            db.prepare_cached("SELECT id FROM labels WHERE key = ?1 ORDER BY id LIMIT 2")?.query_map([key(name.trim())], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
                        match of_name[..] {
                            [one] => Some(one),
                            _ => None,
                        }
                    }
                });
            }
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The label a label's reference names (F9: one About shows, opened): the one that holds it, else one made now with it, named
    /// `name`, by the reference alone, never a label of its name; in one transaction, so two opens at once make one. Blocking.
    pub fn open_label(&self, r: &Ref, name: &str) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if let Some(id) = holding_in(&tx, r)? {
            return Ok(id);
        }
        let at = now();
        let id = give_label(&tx, &key(name.trim()), name.trim(), at)?;
        if !super::records::put_reference(&tx, &ids::label(id), r, "link", at, false)?.kept() {
            anyhow::bail!("Another holds {} {}.", r.source, r.value);
        }
        tx.commit()?;
        Ok(id)
    }

    /// A label's rows read at a source (C56), kept at `at`, each by its reference (found again, as read now); and how far its list
    /// there stands: read from its start where `first` (its time then), and where the next read starts (none: its end, or the
    /// source does not know the label). Blocking.
    pub fn keep_label_rows(&self, label: i64, source: &str, rows: &[LabelListed], next: Option<i64>, first: bool, at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut put = tx.prepare_cached(
                "INSERT OR REPLACE INTO label_rows (label, source, ref_kind, ref_value, title, artist, year, catno, kind, cover, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for r in rows {
                put.execute(params![label, r.reference.source, r.reference.kind, r.reference.value, r.title, r.artist, r.year, r.catno, r.kind, r.cover, at])?;
            }
        }
        if first {
            tx.prepare_cached("INSERT OR REPLACE INTO label_reads (label, source, at, next) VALUES (?1, ?2, ?3, ?4)")?.execute(params![label, source, at, next])?;
        } else if tx.prepare_cached("UPDATE label_reads SET next = ?3 WHERE label = ?1 AND source = ?2")?.execute(params![label, source, next])? == 0 {
            tx.prepare_cached("INSERT INTO label_reads (label, source, at, next) VALUES (?1, ?2, ?3, ?4)")?.execute(params![label, source, at, next])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The kept row a reference names (C56: what a row opened says of its record), preferring `label`'s, then the latest read; and
    /// the label it was kept for. Blocking.
    pub fn label_row_of(&self, r: &Ref, label: Option<i64>) -> Result<Option<(i64, LabelListed)>> {
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT label, {ROW_COLUMNS} FROM label_rows WHERE source = ?1 AND ref_kind = ?2 AND ref_value = ?3 ORDER BY label IS ?4 DESC, at DESC LIMIT 1"
        ))?;
        Ok(st
            .query_row(params![r.source, r.kind, r.value, label], |row| {
                Ok((
                    row.get(0)?,
                    LabelListed {
                        reference: Ref { source: row.get(1)?, kind: row.get(2)?, value: row.get(3)? },
                        title: row.get(4)?,
                        artist: row.get(5)?,
                        year: row.get(6)?,
                        catno: row.get(7)?,
                        kind: row.get(8)?,
                        cover: row.get(9)?,
                    },
                ))
            })
            .optional()?)
    }

    /// A record tied to a label it was opened from (C56), with its catalogue number there, where nothing ties it to that label yet:
    /// a label's list's tie (`listing`), which no build touches. Blocking.
    pub fn tie_label(&self, record: i64, label: i64, catno: &str) -> Result<()> {
        self.db().execute(
            "INSERT OR IGNORE INTO record_labels (record, label, catno, how) SELECT ?1, ?2, ?3, 'listing'
             WHERE EXISTS (SELECT 1 FROM labels WHERE id = ?2) AND NOT EXISTS (SELECT 1 FROM record_labels WHERE record = ?1 AND label = ?2)",
            params![record, label, catno],
        )?;
        Ok(())
    }

    /// The labels' rows and reads kept longer than `ROWS_KEPT` before `now` let go of (C56, RT01's daily pass): a label opened
    /// again reads its lists anew. How many rows. Blocking.
    pub fn forget_label_rows(&self, now: i64) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let gone = tx.execute("DELETE FROM label_rows WHERE at < ?1", [now - ROWS_KEPT])?;
        tx.execute("DELETE FROM label_reads WHERE at < ?1", [now - ROWS_KEPT])?;
        tx.commit()?;
        Ok(gone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::AlbumCopy;
    use crate::catalogue::identity::{Listed, RecordFacts, fold};
    use crate::catalogue::tests::tracks as with_tracks;

    /// An album of these tracks, credited and labelled.
    fn album(title: &str, tracks: &[i64], artists: &[&str], artist_ids: &[&str], labels: &[(&str, &str)]) -> AlbumCopy {
        AlbumCopy {
            facts: RecordFacts { release: None, artist: fold(artists[0]), title: fold(title), tracks: tracks.iter().map(|t| Listed::of(&format!("Track {t}"), 200_000)).collect() },
            title: title.into(),
            artist: artists[0].into(),
            tracks: tracks.iter().enumerate().map(|(i, t)| (*t, None, Some(i as u32 + 1))).collect(),
            artists: artists.iter().map(|a| a.to_string()).collect(),
            artist_ids: artist_ids.iter().map(|a| a.to_string()).collect(),
            labels: labels.iter().map(|(l, c)| (l.to_string(), c.to_string())).collect(),
            ..Default::default()
        }
    }

    fn record_of(cat: &Catalogue, track: i64) -> i64 {
        crate::catalogue::tests::record_of(cat, track).unwrap()
    }

    /// The labels an album's files name are its record's, each with its catalogue number, and the label lists its records.
    #[test]
    fn a_labels_records() {
        let cat = with_tracks(3);
        let tide_tables = album("Tide Tables", &[1, 2], &["Glass Harbour"], &[], &[("Low Tide Records", "LTR-012"), ("Bleep", "")]);
        cat.assign_records(&[tide_tables, album("Signal", &[3], &["Noor Field"], &[], &[("low tide records", "LTR-020")])]).unwrap();
        let (tide, signal) = (record_of(&cat, 1), record_of(&cat, 3));
        let low = cat.label_for("Low Tide Records").unwrap();
        assert_eq!(cat.label_for("LOW TIDE RECORDS").unwrap(), low, "one name, one label");
        assert_eq!(cat.label(low).unwrap().map(|l| l.name), Some("Low Tide Records".into()));
        assert_eq!(ids::label(low), format!("la-{low}"), "served as its number for life");
        assert_eq!(cat.label_records(low).unwrap(), [(tide, "LTR-012".to_string()), (signal, "LTR-020".to_string())]);
        assert_eq!(cat.label_records(cat.label_for("Bleep").unwrap()).unwrap(), [(tide, String::new())]);
        // Retagged: what the files say now.
        cat.assign_records(&[album("Tide Tables", &[1, 2], &["Glass Harbour"], &[], &[]), album("Signal", &[3], &["Noor Field"], &[], &[("Low Tide Records", "LTR-020")])]).unwrap();
        assert_eq!(cat.label_records(low).unwrap(), [(signal, "LTR-020".to_string())]);
        assert_eq!(cat.label(9_999).unwrap(), None);
    }

    /// A record's artists in the order its files credit them, the first its album's; each artist's MusicBrainz id a `files`
    /// reference where the files give one for each.
    #[test]
    fn a_records_artists_in_order() {
        let cat = with_tracks(3);
        let (one, two) = ("00000000-0000-4000-8000-000000000201", "00000000-0000-4000-8000-000000000202");
        cat.assign_records(&[
            album("Together", &[1, 2], &["Noor Field", "Glass Harbour"], &[two, one], &[]),
            album("Alone", &[3], &["Kite Theory"], &[one, two], &[]),
        ])
        .unwrap();
        let (together, alone) = (record_of(&cat, 1), record_of(&cat, 3));
        let id = |name: &str| cat.artist_of(name).unwrap().unwrap();
        assert_eq!(cat.record_artists(together).unwrap(), [id("Noor Field"), id("Glass Harbour")]);
        assert_eq!(cat.record_artists(alone).unwrap(), [id("Kite Theory")]);
        let refs = |name: &str| cat.references_of(&ids::artist(id(name))).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert_eq!(refs("Glass Harbour"), [(one.to_string(), "files".to_string())]);
        assert_eq!(refs("Noor Field"), [(two.to_string(), "files".to_string())]);
        assert!(refs("Kite Theory").is_empty(), "two ids for one artist say nothing of which");
        // The credit turned round: the order follows.
        cat.assign_records(&[album("Together", &[1, 2], &["Glass Harbour", "Noor Field"], &[], &[]), album("Alone", &[3], &["Kite Theory"], &[], &[])]).unwrap();
        assert_eq!(cat.record_artists(together).unwrap(), [id("Glass Harbour"), id("Noor Field")]);
    }

    /// CR3, for labels as for records and artists: in a catalogue made anew, `la-12` named anywhere must not quietly name
    /// another label.
    #[test]
    fn a_lost_catalogue_lifts_labels_too() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        assert_eq!(ids::highest_named(r#"{"label":"la-3000005","of":"al-12"}"#), 3_000_005, "a label's id is read where it is named");
        assert_eq!(ids::highest_named("ola-99 la-7x"), 0, "a word that only ends in it, or runs on, names nothing");
        cat.number_above(3_000_005).unwrap();
        assert_eq!(cat.label_for("New").unwrap(), 3_000_006);
        assert_eq!(cat.top_id(), 3_000_006, "what the user data remembers next");
        let fresh = Catalogue::open(None, Vec::new()).unwrap();
        assert_eq!(fresh.label_for("New").unwrap(), 1);
        assert_eq!(fresh.top_id(), 1, "a label counts though nothing else was given");
    }

    /// F11.2 (the owner's word: an outside album never shows beside a record the house has), a label's page: Discogs' row of "Night
    /// Walks" by Kite Theory, which the house has untagged (credited to them, tied to no label), is that record's row, had; opened
    /// (`keep_listing(None, …)`, as `labels::open_record` keeps it) it is that record, its master added. A name two had records share
    /// (X8): the row stays a row of its own, and opened is a record of its own.
    #[test]
    fn a_labels_row_joins_the_record_the_house_has() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let label = cat.label_for("Low Tide Records").unwrap();
        let artist = cat.artist_for("Kite Theory").unwrap();
        let had_named = |title: &str| {
            let record = cat.record_for_tests(title, 2);
            cat.db().execute("UPDATE records SET artist = 'Kite Theory' WHERE id = ?1", [record]).unwrap();
            cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", params![record, artist]).unwrap();
            record
        };
        let had = had_named("Night Walks");
        let orbital = [had_named("Orbital"), had_named("Orbital")];
        let row = |title: &str, master: &str| LabelListed {
            reference: Ref::new("discogs", "master", master),
            title: title.into(),
            artist: "Kite Theory".into(),
            year: Some(2020),
            catno: "LTR-040".into(),
            kind: Some("ep".into()),
            cover: None,
        };
        let rows = [row("Night Walks", "3001"), row("Orbital", "3002")];
        cat.keep_label_rows(label, "discogs", &rows, None, true, 1).unwrap();
        let kept = cat.label_kept(label).unwrap().expect("a label");
        let named: Vec<(&str, Option<i64>)> = kept.rows.iter().map(|(l, r)| (l.title.as_str(), *r)).collect();
        assert_eq!(named, [("Night Walks", Some(had)), ("Orbital", None)], "the row of the record the house has is its; a name two share stays its own");
        assert_eq!(kept.records.get(&had).and_then(|q| q.holder), Some(had), "had");
        let opened = |l: &LabelListed| {
            let listing = Listing { title: l.title.clone(), artist: l.artist.clone(), kind: l.kind.clone(), refs: vec![l.reference.clone()], ..Listing::default() };
            cat.record(cat.keep_listing(None, &[listing], "link").unwrap()[0]).unwrap().unwrap().id
        };
        assert_eq!(opened(&rows[0]), had, "opened: the record the house has");
        assert!(cat.references_of(&ids::record(had)).unwrap().iter().any(|(r, row)| r.value == "3001" && row.how == "link"), "its master added");
        let own = opened(&rows[1]);
        assert!(!orbital.contains(&own), "a name two had records share: a record of its own");
    }
}
