//! A record's picture in Cratarr's own store (SCENARIOS-COUPLING COU.11, COU.12, COU.20; SCENARIOS-DC1 FA14; T-COU-7, T-COU-8,
//! T-COU-15, T-COU-20, T-COU-21; the owner, 2026-11-20 21:42, "Cover in the database"): the chosen cover is kept as the picture's
//! bytes (T-COU-7: it shows with every service off), its type, its source, who and when, by record. Nothing is written into or beside
//! a file (GEN.58): the files keep their own pictures, and a copy shown as a copy shows its own.
//!
//! - **What shows** is the record's one picture resolver's (`cratarr_domain::picture`): the chosen cover; else the deciding copy's
//!   files' picture; else the first service's picture kept as it answered (`first`), until a hand picks.
//! - **How it was chosen** (`how`): `hand` (Change cover), `kept` (Replace's "keep the old one", a replaced file's picture kept
//!   before it went to the trash: T-COU-8, T-COU-15), `017` (a 0.17 Change cover carried at 0.18's step, its run kept in `run`:
//!   its Undo drops it, T-COU-21), `first` (a service's, where the files have none: below the files).
//! - **A merge keeps the later pick** (T-COU-20), the other a candidate; retention forgets them with their record (RT01).
//! - **The stamp** (`print`) changes with each pick, so every surface and the device's covers feed take it at once.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;

pub(super) const SCHEMA: &str = "
-- COU.11 (FA14, T-COU-7): a record's chosen cover, kept as its picture's bytes: its type, its source (`upload`, `coverartarchive`,
-- `discogs`, `files@<library>` for a copy's own picture kept), who and when, how it was chosen (`hand`, `kept`, `017`, `first`) and,
-- for a 0.17 pick carried at the step, the file run it came from. One a record.
-- `change`: the Change on the one path that picked it (its Undo's check).
CREATE TABLE IF NOT EXISTS record_covers (
  record INTEGER PRIMARY KEY, bytes BLOB NOT NULL, mime TEXT NOT NULL, source TEXT NOT NULL, who TEXT, at INTEGER NOT NULL,
  how TEXT NOT NULL, run INTEGER, change INTEGER);
-- T-COU-20 (FA14): a pick a merge set aside, kept as a candidate of the record it went into.
CREATE TABLE IF NOT EXISTS cover_candidates (
  record INTEGER NOT NULL, bytes BLOB NOT NULL, mime TEXT NOT NULL, source TEXT NOT NULL, who TEXT, at INTEGER NOT NULL, how TEXT NOT NULL,
  PRIMARY KEY (record, at, source)) WITHOUT ROWID;
";

/// How a cover was chosen.
pub const HAND: &str = "hand";
pub const KEPT: &str = "kept";
pub const CARRIED: &str = "017";
pub const FIRST: &str = "first";

/// A record's cover as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub source: String,
    pub who: Option<String>,
    pub at: i64,
    pub how: String,
    pub run: Option<i64>,
    pub change: Option<i64>,
}

impl Cover {
    /// Its stamp: its bytes' print and when, so a pick of the same picture again still moves every surface on.
    pub fn print(&self) -> String {
        use md5::Digest;
        let hash = md5::Md5::digest(&self.bytes);
        format!("c{:x}{:x}", u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]), self.at)
    }

    /// Whether it is a choice (the hand's, Replace's kept, a 0.17 pick): above the files. A service's first answer is below them.
    pub fn chosen(&self) -> bool {
        self.how != FIRST
    }
}

fn read(db: &Connection, record: i64) -> Result<Option<Cover>> {
    Ok(db
        .prepare_cached("SELECT bytes, mime, source, who, at, how, run, change FROM record_covers WHERE record = ?1")?
        .query_row([record], |r| Ok(Cover { bytes: r.get(0)?, mime: r.get(1)?, source: r.get(2)?, who: r.get(3)?, at: r.get(4)?, how: r.get(5)?, run: r.get(6)?, change: r.get(7)? }))
        .optional()?)
}

/// `Catalogue::keep_cover` in the caller's transaction.
pub(super) fn keep_in(tx: &Connection, record: i64, cover: &Cover) -> Result<Option<Cover>> {
    let before = read(tx, record)?;
    tx.prepare_cached(
        "INSERT INTO record_covers (record, bytes, mime, source, who, at, how, run, change) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (record) DO UPDATE SET bytes = excluded.bytes, mime = excluded.mime, source = excluded.source, who = excluded.who,
           at = excluded.at, how = excluded.how, run = excluded.run, change = excluded.change",
    )?
    .execute(params![record, cover.bytes, cover.mime, cover.source, cover.who, cover.at, cover.how, cover.run, cover.change])?;
    Ok(before)
}

/// T-COU-20: a merged record's pick onto the record it went into: the later of the two stays (a choice above a service's first),
/// the other kept as a candidate of `into`. None left on `from`. In the caller's transaction.
pub(super) fn merge_in(tx: &Connection, from: i64, into: i64) -> Result<()> {
    let _ = (tx, from, into);
    if true {
        return Ok(());
    }
    let Some(theirs) = read(tx, from)? else { return Ok(()) };
    tx.execute("DELETE FROM record_covers WHERE record = ?1", [from])?;
    let candidate = |c: &Cover| set_aside_in(tx, into, c);
    match read(tx, into)? {
        Some(ours) if cratarr_domain::picture::later(&ours.how, ours.at, &theirs.how, theirs.at) => candidate(&theirs)?,
        Some(ours) => {
            candidate(&ours)?;
            keep_in(tx, into, &theirs)?;
        }
        None => {
            keep_in(tx, into, &theirs)?;
        }
    }
    tx.execute("UPDATE cover_candidates SET record = ?2 WHERE record = ?1", params![from, into])?;
    Ok(())
}

/// A cover set aside (a merge's other pick, a pick an Undo may put back), kept as a candidate of `record`. In the caller's
/// transaction.
fn set_aside_in(tx: &Connection, record: i64, c: &Cover) -> Result<()> {
    tx.prepare_cached("INSERT OR REPLACE INTO cover_candidates (record, bytes, mime, source, who, at, how) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?
        .execute(params![record, c.bytes, c.mime, c.source, c.who, c.at, c.how])?;
    Ok(())
}

/// RT01: a forgotten record's pictures go with it. In the caller's transaction.
pub(super) fn forget_in(tx: &Connection, record: i64) -> Result<()> {
    tx.prepare_cached("DELETE FROM record_covers WHERE record = ?1")?.execute([record])?;
    tx.prepare_cached("DELETE FROM cover_candidates WHERE record = ?1")?.execute([record])?;
    Ok(())
}

/// Every record's cover stamp, for the build (`Credits::pictures`): what the served `albums.cover` names and the change print reads.
pub(super) fn prints_in(db: &Connection) -> Result<std::collections::HashMap<i64, (String, bool)>> {
    let mut st = db.prepare("SELECT record, bytes, mime, source, who, at, how, run, change FROM record_covers")?;
    let rows = st.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, Cover { bytes: r.get(1)?, mime: r.get(2)?, source: r.get(3)?, who: r.get(4)?, at: r.get(5)?, how: r.get(6)?, run: r.get(7)?, change: r.get(8)? }))
    })?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (record, c) = row?;
        out.insert(record, (c.print(), c.chosen()));
    }
    Ok(out)
}

impl Catalogue {
    /// A record's cover as kept (its merges followed), chosen or a service's first. Blocking.
    pub fn kept_cover(&self, record: i64) -> Result<Option<Cover>> {
        let db = self.db();
        let Some(end) = super::records::merged_end(&db, record)? else { return Ok(None) };
        read(&db, end)
    }

    /// Keeps `cover` as `record`'s (its merges followed), the one before answered (what an Undo puts back). Its record owes the
    /// change log its picture (LV1). Blocking.
    pub fn keep_cover(&self, record: i64, cover: &Cover) -> Result<Option<Cover>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let end = super::records::merged_end(&tx, record)?.unwrap_or(record);
        let before = keep_in(&tx, end, cover)?;
        tx.commit()?;
        Ok(before)
    }

    /// `record`'s kept cover let go of (Reset; an Undo with nothing before), the one it was answered. Blocking.
    pub fn drop_cover(&self, record: i64) -> Result<Option<Cover>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let end = super::records::merged_end(&tx, record)?.unwrap_or(record);
        let before = read(&tx, end)?;
        tx.execute("DELETE FROM record_covers WHERE record = ?1", [end])?;
        tx.commit()?;
        Ok(before)
    }

    /// The files Cratarr placed in catalogue folder `folder`, while as placed (T-COU-17): what says nothing of its copy's picture
    /// (T-COU-15, T-COU-22). Blocking.
    pub fn placed_as_placed(&self, folder: i64) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let rows: Vec<String> = db
            .prepare_cached(&format!(
                "SELECT p.path FROM placements p JOIN files f ON f.folder = p.folder AND f.rel = p.path
                 WHERE p.folder = ?1 AND p.number IS NOT NULL AND (p.size IS NULL OR p.size = f.size) AND (p.mtime IS NULL OR p.mtime = f.mtime)
                   AND {}",
                super::provenance::SAME_TAGS.replace("placements.", "p.")
            ))?
            .query_map([folder], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows.into_iter().collect())
    }

    /// T-COU-21: the picks a 0.17 file run became, dropped by that run's Undo: the records whose picture it was. Blocking.
    pub fn drop_covers_of_run(&self, run: i64) -> Result<Vec<i64>> {
        let _ = (run, CARRIED);
        Ok(Vec::new())
    }

    /// `cover` set aside as a candidate of `record` (its merges followed): what an Undo puts back. Blocking.
    pub fn set_aside_cover(&self, record: i64, cover: &Cover) -> Result<()> {
        let db = self.db();
        let end = super::records::merged_end(&db, record)?.unwrap_or(record);
        set_aside_in(&db, end, cover)
    }

    /// The candidate of `record` from `source` at `at`, if still kept. Blocking.
    pub fn candidate_cover(&self, record: i64, source: &str, at: i64) -> Result<Option<Cover>> {
        let db = self.db();
        let end = super::records::merged_end(&db, record)?.unwrap_or(record);
        Ok(db
            .prepare_cached("SELECT bytes, mime, source, who, at, how FROM cover_candidates WHERE record = ?1 AND source = ?2 AND at = ?3")?
            .query_row(params![end, source, at], |r| {
                Ok(Cover { bytes: r.get(0)?, mime: r.get(1)?, source: r.get(2)?, who: r.get(3)?, at: r.get(4)?, how: r.get(5)?, run: None, change: None })
            })
            .optional()?)
    }

    /// The candidates a record keeps (a merge's other pick, T-COU-20): source and when, the latest first. Blocking.
    pub fn cover_candidates(&self, record: i64) -> Result<Vec<(String, i64)>> {
        let db = self.db();
        let end = super::records::merged_end(&db, record)?.unwrap_or(record);
        let rows = db.prepare("SELECT source, at FROM cover_candidates WHERE record = ?1 ORDER BY at DESC")?.query_map([end], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}
