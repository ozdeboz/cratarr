//! A label's id for life (decision 0100, C46): a number given once per name, as an artist's is, and served as `la-<id>`. A
//! row stays when nothing names the label any more, so it comes back to its id. Its records are `record_labels`, written by
//! the record pass from what the files name, and by a row of its page once opened (C56).
//!
//! A label About shows by its Discogs id or its MBID (F9) is the label holding that reference, else, when it is opened
//! (`open_label`), the one label of its name that holds none of that source's references (PB3: as an outside row joins the record the
//! house has of its names, F11.2), else one made with it: so two labels may share a name, as two artists may (two ids at one source,
//! or X8), and a label known by its name alone is the one of its name only where exactly one has it (`labels_shown`, X8).
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
  also TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (label, source, ref_kind, ref_value)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS label_rows_by_ref ON label_rows (source, ref_kind, ref_value);
-- C56 (OQ10 (a): a hundred at a time): how far a label's list at a source was read: when its first hundred was (`at`), and where
-- the next starts (MusicBrainz's offset, Discogs' page), none at its end or where the source does not know the label. Let go of
-- with the rows, thirty days on.
CREATE TABLE IF NOT EXISTS label_reads (label INTEGER NOT NULL, source TEXT NOT NULL, at INTEGER NOT NULL, next INTEGER, PRIMARY KEY (label, source)) WITHOUT ROWID;
";

/// A row's `also` added to a label's rows kept before it (A2.3: a preview's copy), once, as the catalogue opens.
pub(super) fn add_also(db: &Connection) -> Result<()> {
    if !db.prepare("SELECT 1 FROM pragma_table_info('label_rows') WHERE name = 'also'")?.exists([])? {
        db.execute_batch("ALTER TABLE label_rows ADD COLUMN also TEXT NOT NULL DEFAULT ''")?;
    }
    Ok(())
}

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
    /// What else its source says of it (A2.3, F18.1): the album's references at other sources (MusicBrainz's link to a Discogs
    /// release), and its artist's reference there (MusicBrainz's credit), kind `artist`.
    pub also: Vec<Ref>,
}

/// A row's `also` as kept: JSON [[source, kind, value], ...], '' none.
fn also_text(also: &[Ref]) -> String {
    if also.is_empty() {
        return String::new();
    }
    serde_json::to_string(&also.iter().map(|r| [r.source.as_str(), r.kind.as_str(), r.value.as_str()]).collect::<Vec<_>>()).unwrap_or_default()
}

/// A row's `also` read back.
fn also_of(text: &str) -> Vec<Ref> {
    serde_json::from_str::<Vec<[String; 3]>>(text).map(|v| v.into_iter().map(|[s, k, v]| Ref { source: s, kind: k, value: v }).collect()).unwrap_or_default()
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
const ROW_COLUMNS: &str = "source, ref_kind, ref_value, title, artist, year, catno, kind, cover, also";

fn listed_of(r: &rusqlite::Row) -> rusqlite::Result<LabelListed> {
    Ok(LabelListed {
        reference: Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? },
        title: r.get(3)?,
        artist: r.get(4)?,
        year: r.get(5)?,
        catno: r.get(6)?,
        kind: r.get(7)?,
        cover: r.get(8)?,
        also: also_of(&r.get::<_, String>(9)?),
    })
}

/// A label as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRow {
    pub id: i64,
    pub name: String,
}

/// What makes two names one label (A2.4): its bare name (`bare_key`).
fn key(name: &str) -> String {
    bare_key(name)
}

/// The words a label's name carries or leaves off as a company, which say nothing of which label it is (A2.4).
const COMPANY: [&str; 5] = ["records", "recordings", "ltd", "limited", "inc"];

/// A label's key (A2.4): its name folded as an artist's is (case and typographic quotes), without the company words Discogs and the
/// files add or leave off ("Records", "Recordings", "Ltd", "Limited", "Inc", with or without a dot or a comma) while one is last,
/// keeping at least one word: "Warp Records" is "warp", "Freestyle Records Ltd." is "freestyle", "Records" stays "records". Pure.
pub fn bare_key(name: &str) -> String {
    let folded = ids::artist_key(name.trim());
    let mut words: Vec<&str> = folded.split_whitespace().collect();
    while words.len() > 1 && words.last().is_some_and(|w| COMPANY.contains(&w.trim_end_matches(['.', ',']))) {
        words.pop();
    }
    let mut out = words.join(" ");
    while out.ends_with(',') {
        out.pop();
    }
    out
}

/// A files' label value as the labels it names (A2.4.2), as the artists' are taken apart: split at ",", ";" and "/", each part
/// trimmed; a part that is a company word alone ("Inc.", "Ltd") belongs to the one before it and is dropped; never split at "&"
/// ("Simon & Schuster Audio" is one). Pure.
pub fn apart(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in value.split([',', ';', '/']) {
        let part = part.trim();
        if part.is_empty() || (!out.is_empty() && COMPANY.contains(&part.to_lowercase().trim_end_matches('.'))) {
            continue;
        }
        out.push(part.to_string());
    }
    out
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

/// The one label of a name (X8: exactly one has it; none where none, or several, do). Blocking.
fn one_of_name_in(db: &Connection, name: &str) -> Result<Option<i64>> {
    let of_name: Vec<i64> = db.prepare_cached("SELECT id FROM labels WHERE key = ?1 ORDER BY id LIMIT 2")?.query_map([key(name.trim())], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(match of_name[..] {
        [one] => Some(one),
        _ => None,
    })
}

/// The label a reference no label holds joins when it is opened (PB3, the review of preview 1's fixes): the one label of its name
/// (X8), where it holds none of that source's references (one holding another id there is another label). Blocking.
fn joined_by_name_in(db: &Connection, r: &Ref, name: &str) -> Result<Option<i64>> {
    let Some(one) = one_of_name_in(db, name)? else { return Ok(None) };
    let holds = db.prepare_cached("SELECT 1 FROM \"references\" WHERE subject = ?1 AND source = ?2 AND kind = ?3")?.exists(params![ids::label(one), r.source, r.kind])?;
    Ok((!holds).then_some(one))
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

    /// A record's credited artists with their names, in credit order, on a reader (a record's page never waits on the writer). M8,
    /// M9. Blocking.
    pub fn record_credits_named(&self, record: i64) -> Result<Vec<(i64, String)>> {
        let read = |db: &Connection| -> Result<Vec<(i64, String)>> {
            let mut st = db.prepare_cached("SELECT a.id, a.name FROM record_artists r JOIN artists a ON a.id = r.artist WHERE r.record = ?1 ORDER BY r.position, a.id")?;
            let rows = st.query_map([record], |r| Ok((r.get(0)?, r.get(1)?)))?;
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
            // F11.2: a row no record holds that a record the house has of its artist and title is one album with is that record's row, as
            // an open of it joins it (`records::had_by_names_in`): never shown beside it. RG5: the list's rows judged together (X8).
            let mut named = None;
            let joined: Vec<Option<i64>> = {
                let asked: Vec<(&[Ref], &str, &str, Option<&str>)> =
                    listed.iter().map(|l| (std::slice::from_ref(&l.reference), l.artist.as_str(), l.title.as_str(), l.kind.as_deref())).collect();
                super::records::had_by_names_in(db, &asked, 0, &mut named)?.into_iter().map(|j| j.map(|(had, _)| had)).collect()
            };
            let mut rows = Vec::with_capacity(listed.len());
            for (l, by_names) in listed.into_iter().zip(joined) {
                let found = match super::records::found_by_reference(db, &Listing { refs: vec![l.reference.clone()], ..Default::default() })? {
                    Some(found) => Some(found),
                    None => by_names,
                };
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
                    None => one_of_name_in(db, name)?,
                });
            }
            Ok(out)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The label a label's reference names (F9: one About shows, opened): the one that holds it; else the one label of its name that
    /// holds none of that source's (`joined_by_name_in`, PB3: one real label, one page), given it; else one made now with it, named
    /// `name`. In one transaction, so two opens at once make or join one. Blocking.
    pub fn open_label(&self, r: &Ref, name: &str, by_name: bool) -> Result<i64> {
        let mut db = self.db();
        let tx = db.transaction()?;
        if let Some(id) = holding_in(&tx, r)? {
            return Ok(id);
        }
        let at = now();
        // A1.8.4: a namesake (`by_name` false) joins no label by its name.
        let joined = if by_name { joined_by_name_in(&tx, r, name)? } else { None };
        let id = match joined {
            Some(one) => one,
            None => give_label(&tx, &key(name.trim()), name.trim(), at)?,
        };
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
                "INSERT OR REPLACE INTO label_rows (label, source, ref_kind, ref_value, title, artist, year, catno, kind, cover, at, also)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for r in rows {
                put.execute(params![label, r.reference.source, r.reference.kind, r.reference.value, r.title, r.artist, r.year, r.catno, r.kind, r.cover, at, also_text(&r.also)])?;
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
                        also: also_of(&row.get::<_, String>(10)?),
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
            also: Vec::new(),
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

    /// A2.4.1: files naming "Warp" and "Warp Records", "Big Dada" and "Big Dada Recordings", "Freestyle Records" and "Freestyle Records
    /// Ltd.", and a label "Records": four labels, Warp, Big Dada, Freestyle and Records, each by its bare name (company words off the
    /// end while one is last, one word kept). Two labels of one bare key holding two Discogs ids stay two (X8), and a name alone finds
    /// neither.
    #[test]
    fn a_label_is_keyed_by_its_bare_name() {
        assert_eq!(bare_key("Warp Records"), "warp");
        assert_eq!(bare_key("Freestyle Records Ltd."), "freestyle");
        assert_eq!(bare_key("Big Dada Recordings"), "big dada");
        assert_eq!(bare_key("Merge Records, Inc."), "merge");
        assert_eq!(bare_key("Limited Edition Records Limited"), "limited edition");
        assert_eq!(bare_key("Records"), "records", "at least one word");
        assert_eq!(bare_key("WARP"), "warp", "case folded as an artist's key");
        let cat = with_tracks(7);
        let names = ["Warp", "Warp Records", "Big Dada", "Big Dada Recordings", "Freestyle Records", "Freestyle Records Ltd.", "Records"];
        let albums: Vec<AlbumCopy> = names.iter().enumerate().map(|(i, l)| album(&format!("Album {i}"), &[i as i64 + 1], &["Glass Harbour"], &[], &[(l, "")])).collect();
        cat.assign_records(&albums).unwrap();
        let kept: Vec<String> = {
            let db = cat.db();
            let mut st = db.prepare("SELECT name FROM labels ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(kept, ["Big Dada", "Freestyle Records", "Records", "Warp"], "four labels, each first named as its files first name it");
        let warp = cat.label_for("Warp Records").unwrap();
        assert_eq!(cat.label_records(warp).unwrap().len(), 2, "Warp's two records on one label");
        // Two of one bare key with two Discogs ids: two, and a name alone finds neither.
        let other = {
            let db = cat.db();
            give_label(&db, &bare_key("Warp Recordings"), "Warp Recordings", 1).unwrap()
        };
        assert!(cat.set_reference(&ids::label(warp), &Ref::new("discogs", "label", "23528"), "link").unwrap());
        assert!(cat.set_reference(&ids::label(other), &Ref::new("discogs", "label", "777"), "link").unwrap());
        assert_eq!(cat.labels_shown(&[(None, "Warp".into())]).unwrap(), [None], "a name two labels share names neither (X8)");
    }

    /// A2.4.2: a files' label value naming two labels is two ("," ";" "/"); a part that is a company word alone belongs to the one
    /// before it; never split at "&".
    #[test]
    fn a_files_label_value_is_split_into_its_labels() {
        assert_eq!(apart("Warp Records,Beat Records"), ["Warp Records", "Beat Records"]);
        assert_eq!(apart("Warp; Beat"), ["Warp", "Beat"]);
        assert_eq!(apart("Warp / Beat"), ["Warp", "Beat"]);
        assert_eq!(apart("Merge Records, Inc."), ["Merge Records"]);
        assert_eq!(apart("Simon & Schuster Audio"), ["Simon & Schuster Audio"]);
        let cat = with_tracks(2);
        cat.assign_records(&[album("Tide Tables", &[1, 2], &["Glass Harbour"], &[], &[("Warp Records,Beat Records", "WARP-1")])]).unwrap();
        let record = record_of(&cat, 1);
        let mut tied: Vec<(String, String)> = cat.record_labels(record).unwrap().into_iter().map(|(l, catno)| (l.name, catno)).collect();
        tied.sort();
        assert_eq!(tied, [("Beat Records".to_string(), "WARP-1".to_string()), ("Warp Records".to_string(), "WARP-1".to_string())], "two labels, each with the catalogue number");
    }
}
