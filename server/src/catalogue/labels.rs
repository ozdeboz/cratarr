//! A label's id for life (decision 0100, C46): a number given once per name, as an artist's is, and served as `la-<id>`. A
//! row stays when nothing names the label any more, so it comes back to its id. Its records are `record_labels`, written by
//! the record pass from what the files name, and by a label's listing once one is read.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{Catalogue, now};
use crate::serve::ids;

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
}
