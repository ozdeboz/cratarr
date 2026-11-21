//! 0.18's step, its part `facts` (SCENARIOS-VR01 VR42, VR43, VR48; DC1's FA15): what 0.17 kept of a record's lists, counts and
//! years carried onto 0.18's shapes once, at the first start, in one IMMEDIATE transaction whose last write is its mark.
//!
//! - **The lists keyed by reference** (H5): each 0.17 row (one per source, keyed by none) given its source's main reference for the
//!   record (`main_ref`: a link by hand, then a release before a group or master, then the surest), after the relabels below; a row
//!   whose source holds no reference on the record is listed and not carried, one of two references of one rank keyed by the pick and
//!   listed.
//! - **The relabels** (H1, N2, X1, X2): a MusicBrainz group a request was asked with by its id (`album_key` that group) is "asked";
//!   a MusicBrainz release found by a search, unconfirmed, on a record whose group is itself no guess (judged after "asked"), and no
//!   carried match's release, is "group". Every other "search" row stays a guess, re-judged by today's pairing at each read (the
//!   owner's 02:32): nothing stored decides it.
//! - **The counts** (M7): one by hand stays the record's pick, named as its source's main reference; one by hand naming a source that
//!   keeps no list (0.16's AcoustID) listed, no pick made; one worked out (`counted_by_hand` 0) set to none, its lists staying.
//! - **The years on their references** (H4, RV-5): Discogs' year of each album on its Discogs main reference (none: "asked, none"
//!   with its time; with no Discogs reference, for the record, counted apart); MusicBrainz's from `album_matches` on the release its
//!   match names (none found: "asked, none" for the record). `mb_releases` is not read for years.
//! - **The report** gives, per part, the rows read and written, every row not carried with why, the re-keys, relabels and counts
//!   set to none; and the carried name-found matches, per source, how many today's pairing confirms and how many stay guesses, each
//!   listed with why (VR48).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::Catalogue;
use super::records::{Ref, ReferenceRow};
use crate::serve::ids;

/// The part's mark in `meta`, holding its report.
pub const FACTS: &str = "018:facts";
/// The part `fold`'s (FA22): the merges it made, each "al-<from> into al-<into>".
pub const FOLD: &str = "018:fold";

/// What the user data kept that the part reads (the app reads it and hands it here).
#[derive(Debug, Clone, Default)]
pub struct FactsCarry {
    /// Discogs' answer for each record (`discogs_facts`): its year, or none (asked, none), and when.
    pub discogs: Vec<(i64, Option<i32>, i64)>,
    /// MusicBrainz's match of each record (`album_matches`): the release it names (none: none found), its year, and when.
    pub matches: Vec<(i64, Option<String>, Option<i32>, i64)>,
    /// The groups requests were asked with by their id (`album_key` the group itself, N2): (record, group).
    pub asked: Vec<(i64, String)>,
}

/// The part's report: counts by line, and the rows listed by why.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FactsReport {
    pub counts: BTreeMap<String, u64>,
    pub listed: BTreeMap<String, Vec<String>>,
}

impl FactsReport {
    fn count(&mut self, line: &str, n: u64) {
        *self.counts.entry(line.to_string()).or_default() += n;
    }

    fn list(&mut self, why: &str, row: String) {
        self.listed.entry(why.to_string()).or_default().push(row);
    }
}

/// The one reference of `source` that speaks for a record: a link pasted by hand, else its release (or album page) before its
/// release group or master, else the surest found (the References panel's and the step's one rule).
pub fn main_ref<'k>(refs: &'k [(Ref, ReferenceRow)], source: &str) -> Option<&'k (Ref, ReferenceRow)> {
    let kind = |r: &Ref| match r.kind.as_str() {
        "release" | "album" => 0,
        "release-group" | "master" => 1,
        _ => 2,
    };
    let how = |h: &str| match h {
        "hand" => 0,
        "files" => 1,
        "link" => 2,
        _ => 3,
    };
    refs.iter().filter(|(r, _)| r.source == source).min_by_key(|(r, row)| (row.how != "hand", kind(r), how(&row.how)))
}

fn refs_of(tx: &Connection, record: i64) -> Result<Vec<(Ref, ReferenceRow)>> {
    let mut st = tx.prepare_cached("SELECT source, kind, value, at, how, confirmed, read_at FROM \"references\" WHERE subject = ?1 ORDER BY source, kind, value")?;
    let rows = st.query_map([ids::record(record)], |r| {
        Ok((Ref { source: r.get(0)?, kind: r.get(1)?, value: r.get(2)? }, ReferenceRow { at: r.get(3)?, how: r.get(4)?, confirmed: r.get(5)?, read_at: r.get(6)? }))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Whether two references of `source` share the main one's rank (an unclear key, listed).
fn unclear(refs: &[(Ref, ReferenceRow)], source: &str) -> bool {
    let rank = |(r, row): &(Ref, ReferenceRow)| (row.how != "hand", matches!(r.kind.as_str(), "release-group" | "master"), row.how.clone());
    let mut ranks: Vec<_> = refs.iter().filter(|(r, _)| r.source == source).map(rank).collect();
    ranks.sort();
    ranks.len() > 1 && ranks[0] == ranks[1]
}

impl Catalogue {
    /// The part `facts` (VR42), once: done before (its mark kept), its report; else run now, in one IMMEDIATE transaction whose last
    /// write is its mark, `after` heard after its writes and before its mark (what a test cuts it by: VR43). Blocking.
    pub fn step_018_facts(&self, carry: &FactsCarry, after: &dyn Fn() -> Result<()>) -> Result<FactsReport> {
        let mut db = self.db();
        if let Some(kept) = db.query_row("SELECT value FROM meta WHERE key = ?1", [FACTS], |r| r.get::<_, String>(0)).optional()? {
            return Ok(serde_json::from_str(&kept).unwrap_or_default());
        }
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut report = FactsReport::default();
        // "asked" first: a group a request was asked with by its id (N2).
        for (record, group) in &carry.asked {
            let n = tx.execute(
                "UPDATE \"references\" SET how = 'asked' WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'release-group' AND value = ?2 AND how = 'search' AND confirmed = 0",
                params![ids::record(*record), group],
            )?;
            report.count("relabelled asked", n as u64);
        }
        // "group": a release found by a search on a record whose group is no guess, and no carried match's release (X1, X2).
        let matched: BTreeSet<(i64, String)> = carry.matches.iter().filter_map(|(r, release, ..)| Some((*r, release.clone()?))).collect();
        let candidates: Vec<(String, String)> = tx
            .prepare(
                "SELECT r.subject, r.value FROM \"references\" r WHERE r.source = 'musicbrainz' AND r.kind = 'release' AND r.how = 'search' AND r.confirmed = 0
                   AND EXISTS (SELECT 1 FROM \"references\" g WHERE g.subject = r.subject AND g.source = 'musicbrainz' AND g.kind = 'release-group'
                                 AND (g.how <> 'search' OR g.confirmed = 1))",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (subject, release) in candidates {
            let Some(record) = ids::record_number(&subject) else { continue };
            if matched.contains(&(record, release.clone())) {
                report.list("a carried match's release: stays a guess, re-judged by the pairing", format!("{subject} {release}"));
                continue;
            }
            tx.execute(
                "UPDATE \"references\" SET how = 'group' WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'release' AND value = ?2",
                params![subject, release],
            )?;
            report.count("relabelled group", 1);
        }
        // The lists keyed by reference (H5).
        let unkeyed: Vec<(i64, String, String, i64)> = tx
            .prepare("SELECT record, source, tracks, at FROM reference_tracks WHERE kind = '' ORDER BY record, source")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        report.count("lists read", unkeyed.len() as u64);
        for (record, source, tracks, at) in &unkeyed {
            let refs = refs_of(&tx, *record)?;
            tx.execute("DELETE FROM reference_tracks WHERE record = ?1 AND source = ?2 AND kind = ''", params![record, source])?;
            let Some((r, _)) = main_ref(&refs, source) else {
                report.list("lists: no reference of its source on the record, not carried", format!("al-{record} {source}"));
                continue;
            };
            if unclear(&refs, source) {
                report.list("lists: two references of one rank, keyed by the first", format!("al-{record} {source} {}:{}", r.kind, r.value));
            }
            tx.execute(
                "INSERT INTO reference_tracks (record, source, kind, value, tracks, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (record, source, kind, value) DO UPDATE SET tracks = excluded.tracks, at = excluded.at WHERE excluded.at > reference_tracks.at",
                params![record, source, r.kind, r.value, tracks, at],
            )?;
            report.count("lists keyed", 1);
        }
        // The counts (M7): by hand, the pick by its reference; worked out, none.
        let counts: Vec<(i64, String, bool)> = tx
            .prepare("SELECT id, counting, counted_by_hand FROM records WHERE counting IS NOT NULL")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (record, source, by_hand) in counts {
            if !by_hand {
                tx.execute("UPDATE records SET counting = NULL, counting_ref = NULL WHERE id = ?1", [record])?;
                report.count("counts worked out, set to none", 1);
                continue;
            }
            let kept: Option<(String, String)> = tx
                .query_row("SELECT kind, value FROM reference_tracks WHERE record = ?1 AND source = ?2 ORDER BY at DESC LIMIT 1", params![record, source], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            match kept {
                Some((kind, value)) => {
                    tx.execute("UPDATE records SET counting_ref = ?2 WHERE id = ?1", params![record, format!("{kind}\u{1f}{value}")])?;
                    report.count("picks by hand, named by their reference", 1);
                }
                None => {
                    tx.execute("UPDATE records SET counting = NULL, counting_ref = NULL, counted_by_hand = 0 WHERE id = ?1", [record])?;
                    report.list("picks by hand: a source that keeps no list of it, no pick made", format!("al-{record} {source}"));
                }
            }
        }
        // The years on their references (H4).
        report.count("discogs_facts read", carry.discogs.len() as u64);
        for (record, year, at) in &carry.discogs {
            let refs = refs_of(&tx, *record)?;
            let of = main_ref(&refs, "discogs").map(|(r, _)| super::facts::of_ref(&r.kind, &r.value));
            if of.is_none() {
                report.count("discogs: asked, none, with no reference", 1);
            }
            super::facts::said_in(&tx, *record, "year", "discogs", of.as_deref().unwrap_or(""), year.map(|y| y.to_string()).as_deref(), *at)?;
            report.count("years written: discogs", 1);
        }
        report.count("album_matches read", carry.matches.len() as u64);
        for (record, release, year, at) in &carry.matches {
            let of = release.as_deref().map(|r| super::facts::of_ref("release", r)).unwrap_or_default();
            super::facts::said_in(&tx, *record, "year", "musicbrainz", &of, year.map(|y| y.to_string()).as_deref(), *at)?;
            report.count("years written: musicbrainz", 1);
        }
        after()?;
        // Its last write: its mark, holding its report.
        tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![FACTS, serde_json::to_string(&report)?])?;
        tx.commit()?;
        Ok(report)
    }

    /// The part `fold` (FA22, F40; RG2-wide, FA18): every two records of one tracklist joined once, in one IMMEDIATE transaction
    /// whose last write is its mark, listing each merge ("al-2899 into al-1790") for the HA session; a merged id answers as the record
    /// it went into on every route (RV3). Done before: none. `after` is heard before the mark (what a test cuts it by). Blocking.
    pub fn fold_018(&self, kinds: &[(i64, crate::settings::LibraryKind)], after: &dyn Fn() -> Result<()>) -> Result<Option<Vec<(i64, i64)>>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.query_row("SELECT 1 FROM meta WHERE key = ?1", [FOLD], |_| Ok(())).optional()?.is_some() {
            return Ok(None);
        }
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let folded = super::records::fold_in(&tx, at, kinds)?;
        after()?;
        let lines: Vec<String> = folded.iter().map(|(from, into)| format!("{} into {}", ids::record(*from), ids::record(*into))).collect();
        tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![FOLD, serde_json::json!({ "folded": lines }).to_string()])?;
        tx.commit()?;
        Ok(Some(folded))
    }

    /// The carried name-found matches today's pairing confirms, and those that stay guesses with why (VR48's counts, the rehearsal's
    /// line): each `search` reference, unconfirmed, whose list is kept, held against the record's copies in a main library or own
    /// music (C15) by the holding's one pairing. By source. Blocking.
    pub fn matches_confirmed(&self, library: &dyn Fn(i64) -> cratarr_rules::holding::Library) -> Result<BTreeMap<String, (u64, Vec<String>)>> {
        let rows: Vec<(i64, String, String, String)> = {
            let db = self.db();
            let mut st = db.prepare(
                "SELECT CAST(substr(r.subject, 4) AS INTEGER), r.source, r.kind, r.value FROM \"references\" r
                  WHERE r.subject GLOB 'al-[0-9]*' AND r.how = 'search' AND r.confirmed = 0 AND r.kind IN ('release', 'album')",
            )?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?
        };
        let mut out: BTreeMap<String, (u64, Vec<String>)> = BTreeMap::new();
        // Every such record's copies in one read (the copy reader read once for the whole list).
        let records: Vec<i64> = rows.iter().map(|(r, ..)| *r).collect::<BTreeSet<i64>>().into_iter().collect();
        let held = self.copies_of(&records, library)?;
        for (record, source, kind, value) in rows {
            let lists = self.lists_kept(record)?;
            let entry = out.entry(source.clone()).or_default();
            let Some(list) = lists.iter().find(|l| l.reference.source == source && l.reference.kind == kind && l.reference.value == value) else {
                entry.1.push(format!("al-{record} {value}: its list not read"));
                continue;
            };
            let copies = held.get(&record).cloned().unwrap_or_default();
            if !list.guessed || copies.confirmed.contains(&list.reference) {
                entry.0 += 1;
                continue;
            }
            let why = if copies.copies.iter().all(|c| c.present == 0 || !matches!(library(c.folder), cratarr_rules::holding::Library::Main | cratarr_rules::holding::Library::Own(_))) {
                "no copy in a main library or own music"
            } else {
                "a track missing or mismatched"
            };
            entry.1.push(format!("al-{record} {value}: {why}"));
        }
        Ok(out)
    }
}
