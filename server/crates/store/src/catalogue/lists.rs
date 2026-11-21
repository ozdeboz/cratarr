//! A record's tracklists, one row per reference (SCENARIOS-VR01 VR7 (b), H5; OQ-VR-5 (b), the owner 2026-11-20: "one list per
//! release, agreeing ones folded"), and the administrator's pick among them, named by its reference (VR5, RV-4).
//!
//! - **A list's key** is the record, and the reference it was read for (source, kind, value): a release (or a Bandcamp page)
//!   read directly; a release read through a group or a master where the record holds that release (`how` "group"), else the
//!   group or master itself. A re-read of one reference replaces that row alone (VR6 (c)); another reference of the source
//!   leaves it be (VR23). 0.17's rows, one per source, come keyed by none (kind and value empty) until 0.18's step keys them
//!   (VR42).
//! - **A guess's list** is one whose reference was found by a name search alone and never confirmed (`how` "search",
//!   `confirmed` 0: VR8, H1). Nothing stored says a guess is sure: today's one pairing confirms it at each read (the holding's,
//!   RV-22).
//! - **The pick** is the hand's alone (VR10): `records.counting` names its source and `counting_ref` its kind and value.
//!   Nothing works a count out.

use anyhow::Result;
pub use cratarr_domain::facts::{List, Reference};
use cratarr_domain::facts::Present;
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;
use super::identity::RefTrack;
use crate::serve::ids;

/// A pick's kind and value in `records.counting_ref`, apart by U+001F (a Bandcamp page holds every other character).
const APART: char = '\u{1f}';

/// Before the records' schema: 0.17's `reference_tracks` (one row per source) set aside to be carried, and the facts table of an
/// earlier 0.18 build (one row per source, its value never none) let go of, as nothing wrote it yet (H3, H4; no backwards
/// compatibility). Whether the lists were set aside.
pub(super) fn reshape_before(conn: &Connection) -> Result<bool> {
    let has = |table: &str, column: &str| -> Result<bool> {
        Ok(conn.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?.exists([column])?)
    };
    let exists = |table: &str| -> Result<bool> { Ok(conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?.exists([table])?) };
    if exists("facts")? && !has("facts", "ref")? {
        conn.execute_batch("DROP TABLE facts;")?;
    }
    if exists("reference_tracks")? && !has("reference_tracks", "kind")? {
        conn.execute_batch("ALTER TABLE reference_tracks RENAME TO reference_tracks_017;")?;
        return Ok(true);
    }
    Ok(false)
}

/// After the records' schema: 0.17's lists carried whole into the table keyed by reference, each keyed by none until 0.18's step
/// keys it (VR42), its tracks and its `at` as they were.
pub(super) fn reshape_after(conn: &Connection, set_aside: bool) -> Result<()> {
    if set_aside {
        conn.execute_batch(
            "INSERT OR IGNORE INTO reference_tracks (record, source, kind, value, tracks, at) SELECT record, source, '', '', tracks, at FROM reference_tracks_017;
             DROP TABLE reference_tracks_017;",
        )?;
    }
    Ok(())
}

/// A record's kept lists, each with its reference and whether that reference is a guess (VR8). In the caller's transaction.
pub fn lists_in(db: &Connection, record: i64) -> Result<Vec<List>> {
    let mut st = db.prepare_cached(
        "SELECT k.source, k.kind, k.value, k.tracks, k.at,
                EXISTS (SELECT 1 FROM \"references\" r WHERE r.subject = ?2 AND r.source = k.source AND r.kind = k.kind AND r.value = k.value
                          AND r.how = 'search' AND r.confirmed = 0)
           FROM reference_tracks k WHERE k.record = ?1 ORDER BY k.source, k.kind, k.value",
    )?;
    let rows: Vec<(String, String, String, String, i64, bool)> =
        st.query_map(params![record, ids::record(record)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(source, kind, value, tracks, at, guessed)| {
            let tracks: Vec<RefTrack> = serde_json::from_str(&tracks).ok()?;
            Some(List { reference: Reference { source, kind, value }, tracks, at, guessed })
        })
        .collect())
}

/// Whether a kept list speaks for a record's files (RI.1's clause, HS01's, landed with VR01): a list that is no guess agreeing
/// with some copy's present files by VR2's one rule (`agree`: in place order, disc then number, each title and length the list's
/// track's, as many as it has); or a guess's list that today's pairing confirms (RV-22, the owner's 02:32): a copy in a main
/// library or own music (`sure`, by its folder; never an Incoming arrival not yet added, C15) whose files and the list pair one
/// for one, both ways. In the caller's transaction.
pub fn files_agree_in(db: &Connection, record: i64, sure: &dyn Fn(i64) -> bool) -> Result<bool> {
    let lists: Vec<List> = lists_in(db, record)?.into_iter().filter(|l| !l.tracks.is_empty()).collect();
    if lists.is_empty() {
        return Ok(false);
    }
    let files: Vec<(i64, String, Option<i64>)> = db
        .prepare_cached(&format!(
            "SELECT f.folder, f.rel, f.duration_ms FROM files f JOIN tracks t ON t.id = f.track WHERE t.record = ?1 AND t.retired_at IS NULL AND {} ORDER BY f.folder, f.rel",
            super::PRESENT_F
        ))?
        .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut tags = db.prepare_cached("SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IS NOT NULL ORDER BY n")?;
    let mut copies: std::collections::BTreeMap<i64, Vec<Present>> = std::collections::BTreeMap::new();
    for (folder, rel, ms) in files {
        let read: Vec<(String, String)> = tags.query_map(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let facts = crate::incoming::TrackFacts::of(&crate::naming::Tags(read), None);
        copies.entry(folder).or_default().push(Present { disc: facts.disc, number: facts.number, key: rel, title: facts.title, ms: ms.filter(|m| *m > 0) });
    }
    Ok(copies.into_iter().any(|(folder, mut copy)| {
        copy.sort_by_key(|f| (f.disc.unwrap_or(1), f.number.unwrap_or(0)));
        let placed: Vec<RefTrack> = copy
            .iter()
            .map(|f| RefTrack { disc: f.disc.unwrap_or(1), position: f.number.unwrap_or(0), title: f.title.clone(), length_ms: f.ms, recording: None })
            .collect();
        lists.iter().any(|l| if l.guessed { sure(folder) && cratarr_domain::facts::confirms(&l.tracks, &copy) } else { cratarr_domain::facts::agree(&l.tracks, &placed) })
    }))
}

/// A record's pick, named by its reference (VR5): the hand's alone. In the caller's transaction.
pub fn pick_in(db: &Connection, record: i64) -> Result<Option<Reference>> {
    let row: Option<(Option<String>, Option<String>, bool)> = db
        .prepare_cached("SELECT counting, counting_ref, counted_by_hand FROM records WHERE id = ?1")?
        .query_row([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    Ok(row.and_then(|(source, of, by_hand)| {
        let source = source.filter(|_| by_hand)?;
        let (kind, value) = of.as_deref().and_then(|o| o.split_once(APART)).unwrap_or(("", ""));
        Some(Reference { source, kind: kind.to_string(), value: value.to_string() })
    }))
}

/// A list kept for a record under its reference, in place of what that reference gave before (one row a reference, VR6 (c)),
/// on the record it is now (F1.10). In the caller's transaction.
pub fn keep_list_in(db: &Connection, record: i64, r: &Reference, tracks: &[RefTrack], at: i64) -> Result<()> {
    let record = super::records::merged_end(db, record)?.unwrap_or(record);
    db.prepare_cached("INSERT OR REPLACE INTO reference_tracks (record, source, kind, value, tracks, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?
        .execute(params![record, r.source, r.kind, r.value, serde_json::to_string(tracks)?, at])?;
    Ok(())
}

impl Catalogue {
    /// A list kept for `record` under the reference it was read for (VR7 (b)), at `at`. Blocking.
    pub fn keep_list(&self, record: i64, r: &Reference, tracks: &[RefTrack], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        keep_list_in(&tx, record, r, tracks, at)?;
        tx.commit()?;
        Ok(())
    }

    /// A record's kept lists, by reference, each marked a guess's where it is (VR8). On a reader. Blocking.
    pub fn lists_kept(&self, record: i64) -> Result<Vec<List>> {
        match self.reader() {
            Some(c) => lists_in(&c, record),
            None => lists_in(&self.db(), record),
        }
    }

    /// One reference's list let go of (VR23 (b), (c)): another reference's of the same source stays. Blocking.
    pub fn forget_list(&self, record: i64, r: &Reference) -> Result<()> {
        self.db().execute(
            "DELETE FROM reference_tracks WHERE record = ?1 AND source = ?2 AND kind = ?3 AND value = ?4",
            params![record, r.source, r.kind, r.value],
        )?;
        Ok(())
    }

    /// An administrator's pick, named by its reference (VR5, VR14; FA6), or Reset (none): chosen none, nothing worked out in its
    /// place (VR10). Whether it changed. Blocking.
    pub fn pick_list(&self, record: i64, r: Option<&Reference>) -> Result<bool> {
        let (source, of) = match r {
            Some(r) => (Some(r.source.as_str()), Some(format!("{}{APART}{}", r.kind, r.value))),
            None => (None, None),
        };
        let changed = self.db().execute(
            "UPDATE records SET counting = ?2, counting_ref = ?3, counted_by_hand = ?4
              WHERE id = ?1 AND (counting IS NOT ?2 OR counting_ref IS NOT ?3 OR counted_by_hand IS NOT ?4)",
            params![record, source, of, r.is_some()],
        )?;
        Ok(changed > 0)
    }

    /// The record due its read on the schedule (RV12 (d)), one at a time, oldest first: not had, not merged or retired, holding a
    /// reference that is no guess (a guess's list is never read: VR8) and no Bandcamp page (no job reads Bandcamp: decision 0109;
    /// a record whose only reference is Bandcamp's is read at its next open), whose newest kept list was read before `since` or none was,
    /// and whose last scheduled read (`facts`' `read`) was before `since` too, so a record whose sources answer nothing is not
    /// picked every tick. None: nothing is due. Blocking.
    pub fn due_for_read(&self, since: i64) -> Result<Option<i64>> {
        Ok(self
            .db()
            .query_row(
                "SELECT n.id FROM records n
                  WHERE n.merged_into IS NULL AND n.retired_at IS NULL
                    AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = n.id AND t.retired_at IS NULL)
                    AND EXISTS (SELECT 1 FROM \"references\" r WHERE r.subject = 'al-' || n.id
                                  AND r.source <> 'bandcamp' AND r.kind IN ('release', 'release-group', 'master')
                                  AND (r.how <> 'search' OR r.confirmed = 1))
                    AND COALESCE((SELECT max(k.at) FROM reference_tracks k WHERE k.record = n.id), 0) < ?1
                    AND COALESCE((SELECT max(f.at) FROM facts f WHERE f.record = n.id AND f.fact = 'read'), 0) < ?1
                  ORDER BY n.id LIMIT 1",
                [since],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// A record's scheduled read done at `at` (RV12 (d)): kept as a fact with no value, so the schedule waits its time again.
    pub fn read_at(&self, record: i64, at: i64) -> Result<()> {
        self.said(record, "read", "schedule", "", None, at)
    }

    /// A record's pick, named by its reference: the hand's alone (VR5). On a reader. Blocking.
    pub fn picked(&self, record: i64) -> Result<Option<Reference>> {
        match self.reader() {
            Some(c) => pick_in(&c, record),
            None => pick_in(&self.db(), record),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(position: u32, title: &str, secs: i64) -> RefTrack {
        RefTrack { disc: 1, position, title: title.to_string(), length_ms: Some(secs * 1000), recording: None }
    }

    fn salt(n: u32) -> Vec<RefTrack> {
        (1..=n).map(|i| t(i, &format!("S{i}"), 200 + i64::from(i) * 7)).collect()
    }

    /// VR7 (b) and VR40 (H5): one list per reference, two releases of one source both kept; a re-read of one replaces it alone, and
    /// another reference's of the same source stays (VR6 (c), VR23 (b)). Red first: `reference_tracks` keyed (record, source), a
    /// later read of a source's other release replacing the first unsaid.
    #[test]
    fn two_releases_of_one_source_are_two_lists() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let salt_lines = cat.record_for_tests("Salt Lines", 0);
        let (d70, d70v) = (Reference::new("discogs", "release", "D70"), Reference::new("discogs", "release", "D70v"));
        cat.keep_list(salt_lines, &d70, &salt(12), 1).unwrap();
        cat.keep_list(salt_lines, &d70v, &salt(10), 2).unwrap();
        let kept = |cat: &Catalogue| cat.lists_kept(salt_lines).unwrap().into_iter().map(|l| (l.reference.value, l.tracks.len(), l.at)).collect::<Vec<_>>();
        assert_eq!(kept(&cat), [("D70".to_string(), 12, 1), ("D70v".to_string(), 10, 2)], "two Discogs versions, neither replaced unsaid");
        cat.keep_list(salt_lines, &d70v, &salt(9), 3).unwrap();
        assert_eq!(kept(&cat), [("D70".to_string(), 12, 1), ("D70v".to_string(), 9, 3)], "a re-read replaces its reference's row alone");
        let rows: i64 = cat.db().query_row("SELECT count(*) FROM reference_tracks WHERE record = ?1 AND value = 'D70v'", [salt_lines], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "never two rows of one reference");
        // VR23 (b): a pasted link to another release lets go of no other reference's list.
        cat.forget_list(salt_lines, &d70v).unwrap();
        assert_eq!(kept(&cat), [("D70".to_string(), 12, 1)]);
    }

    /// VR8's store half: a list's reference found by a name search alone, never confirmed, is a guess's; a link's, the files', a
    /// group's read release and a confirmed one are not. Red first: no reference on a list, so no list could be told a guess.
    #[test]
    fn a_guess_is_told_by_its_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let r = cat.record_for_tests("Salt Lines", 0);
        let subject = ids::record(r);
        for (value, how) in [("P70s", "search"), ("R70", "group"), ("P70", "link"), ("P70c", "search")] {
            let reference = Reference::new("bandcamp", "album", value);
            cat.db()
                .execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'bandcamp', 'album', ?2, 1, ?3)", params![subject, value, how])
                .unwrap();
            cat.keep_list(r, &reference, &salt(7), 1).unwrap();
        }
        cat.db().execute("UPDATE \"references\" SET confirmed = 1 WHERE value = 'P70c'", []).unwrap();
        let guessed: Vec<(String, bool)> = cat.lists_kept(r).unwrap().into_iter().map(|l| (l.reference.value, l.guessed)).collect();
        assert_eq!(guessed, [("P70".into(), false), ("P70c".into(), false), ("P70s".into(), true), ("R70".into(), false)]);
    }

    /// VR5 (the store's half): a pick names its reference, never a source alone; Reset chooses none, nothing worked out in its
    /// place. Red first: `counting` a source name.
    #[test]
    fn a_pick_names_its_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let r = cat.record_for_tests("Answered", 0);
        let d7 = Reference::new("discogs", "release", "D7");
        assert!(cat.pick_list(r, Some(&d7)).unwrap());
        assert_eq!(cat.picked(r).unwrap(), Some(d7.clone()));
        let row = cat.record(r).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("discogs"), true));
        assert!(!cat.pick_list(r, Some(&d7)).unwrap(), "the same pick again changes nothing");
        assert!(cat.pick_list(r, None).unwrap());
        assert_eq!(cat.picked(r).unwrap(), None);
        let row = cat.record(r).unwrap().unwrap();
        assert_eq!((row.counting, row.counted_by_hand), (None, false), "Reset: none, and nothing worked out");
    }
}
