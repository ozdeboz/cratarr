//! A record's facts with no home (SCENARIOS-DC1 FA15; DOMAIN-CORE §10; RECONCILE X6): one row per record, fact and source, the
//! year by source first. Each source's value is kept, whatever another says or whether that source is switched on now: what a
//! fact resolves to is `resolve()`'s (VR01, DC1a). The hand's stay in the user data's `record_overrides`, read as the hand's;
//! tracklists stay on `reference_tracks`. A write here owes its record to the change log (`record_log`, LV1).

use anyhow::Result;
use rusqlite::{Connection, params};

use super::Catalogue;

pub(super) const SCHEMA: &str = "
-- FA15: a record's facts with no home, one row per source (`fact`: `year` first; `source`: musicbrainz, discogs, bandcamp, …;
-- `at`: when that source said it, epoch seconds).
CREATE TABLE IF NOT EXISTS facts (
  record INTEGER NOT NULL, fact TEXT NOT NULL, source TEXT NOT NULL, value TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (record, fact, source)) WITHOUT ROWID;
";

/// One source's value of a record's fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub fact: String,
    pub source: String,
    pub value: String,
    pub at: i64,
}

/// `Catalogue::said` in the caller's transaction.
pub(super) fn said_in(tx: &Connection, record: i64, fact: &str, source: &str, value: &str, at: i64) -> Result<()> {
    tx.prepare_cached(
        "INSERT INTO facts (record, fact, source, value, at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (record, fact, source) DO UPDATE SET value = excluded.value, at = excluded.at
         WHERE value IS NOT excluded.value OR at IS NOT excluded.at",
    )?
    .execute(params![record, fact, source, value, at])?;
    Ok(())
}

impl Catalogue {
    /// What `source` says of `record`'s `fact` now, read at `at`: its row, its earlier value replaced (one row a source). Blocking.
    pub fn said(&self, record: i64, fact: &str, source: &str, value: &str, at: i64) -> Result<()> {
        said_in(&self.db(), record, fact, source, value, at)
    }

    /// Every source's value of a record's facts, by fact then source. On a reader. Blocking.
    pub fn facts_said(&self, record: i64) -> Result<Vec<Said>> {
        let read = |db: &Connection| -> Result<Vec<Said>> {
            Ok(db
                .prepare_cached("SELECT fact, source, value, at FROM facts WHERE record = ?1 ORDER BY fact, source")?
                .query_map([record], |r| Ok(Said { fact: r.get(0)?, source: r.get(1)?, value: r.get(2)?, at: r.get(3)? }))?
                .collect::<rusqlite::Result<_>>()?)
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T1: i64 = 1_772_539_200;
    const T2: i64 = T1 + 86_400;

    fn said(fact: &str, source: &str, value: &str, at: i64) -> Said {
        Said { fact: fact.into(), source: source.into(), value: value.into(), at }
    }

    /// FA15 (the store's data commit): a re-read replaces its source's value; another source's row stays, with its `at`, through a
    /// switch; the hand's stay in `record_overrides`, read as the hand's; each change is owed to the change log (LV1). Red first:
    /// no table for a fact by source (today a record keeps its winner only, `records.year`).
    #[test]
    fn a_reread_replaces_its_sources_value() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let ud = crate::userdata::UserData::open(None).unwrap();
        let night_ferry = cat.record_for_tests("Night Ferry", 10);
        let owed = || cat.owed_records(10).unwrap().iter().filter(|o| o.record == night_ferry).count();
        cat.owed_moved(&cat.owed_records(100).unwrap()).unwrap();
        // MusicBrainz's year, 2014 at t1; Discogs' 2014.
        cat.said(night_ferry, "year", "musicbrainz", "2014", T1).unwrap();
        assert_eq!(owed(), 1, "logged");
        cat.owed_moved(&cat.owed_records(100).unwrap()).unwrap();
        cat.said(night_ferry, "year", "discogs", "2014", T1).unwrap();
        assert_eq!(owed(), 1, "logged");
        cat.owed_moved(&cat.owed_records(100).unwrap()).unwrap();
        // A re-read at t2 gives 2015.
        cat.said(night_ferry, "year", "musicbrainz", "2015", T2).unwrap();
        assert_eq!(owed(), 1, "logged");
        cat.owed_moved(&cat.owed_records(100).unwrap()).unwrap();
        // The same again changes nothing, and owes nothing.
        cat.said(night_ferry, "year", "musicbrainz", "2015", T2).unwrap();
        assert_eq!(owed(), 0, "nothing changed, nothing owed");
        // Then Discogs is switched off: nothing here reads the switch, and no row goes.
        assert_eq!(cat.facts_said(night_ferry).unwrap(), [said("year", "discogs", "2014", T1), said("year", "musicbrainz", "2015", T2)]);
        let rows: i64 = cat.db().query_row("SELECT count(*) FROM facts WHERE record = ?1 AND fact = 'year' AND source = 'musicbrainz'", [night_ferry], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "never two rows of one source for one fact");
        // The hand's year stays the user data's, read as the hand's: none of it here.
        let album = crate::serve::ids::record(night_ferry);
        ud.set_overrides(std::slice::from_ref(&album), &[("year".into(), Some(serde_json::json!(2013)))], T2).unwrap();
        assert!(ud.overrides().unwrap().contains_key(&album), "the hand's in record_overrides");
        assert!(cat.facts_said(night_ferry).unwrap().iter().all(|s| s.source != "hand"), "never a hand row here");
    }
}
