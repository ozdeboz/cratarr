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

/// The part `wm`'s mark (SCENARIOS-WM WM.22): its report, in `FactsReport`'s shape.
pub const WM: &str = "018:wm";

/// How long 0.17's expectation of a folder (C50) still speaks at the step: its own week (`expect_record`).
const EXPECTED_WEEK: i64 = 7 * 86_400;

/// A value as a fact reads it (the one tag rule): what's left, else none.
fn cleaned(value: &str) -> Option<String> {
    use cratarr_rules::hygiene::{Field, value as clean};
    clean(Field::Album, value).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Catalogue {
    /// The part `wm` (WM.22, T-WM-5, T-WM-9), once, before the part `fold`: WM's cleaned values at the upgrade, the raw tags kept.
    /// - **Provenance seeded** from what 0.17 holds alone: its C50 expectations still within their week, and the placings `seeds`
    ///   names (a fetcher's items still placed, listing their folder: the requests crate's to read), each a copy's placement row.
    /// - **A record named by a stamp** takes its clean name: a stamp beside a name is cut; a title that is only a stamp is merged into
    ///   the record a seeded placement names where its copy is that record's (WM.10 (b1): `identity::judge`), else titled as a file
    ///   with no album titles it, and listed as made by a stamp with no placement known (b2). Its artist line the same way.
    /// - **Artists and labels named by a stamp** are listed: their credits follow the files' cleaned names at the first build.
    /// One IMMEDIATE transaction whose last write is its mark (its report); `after` heard before the mark (what a test cuts it by). Done
    /// before: none. The merges, each (the record that went, the one it went into), are said for the user data to follow. Blocking.
    pub fn step_018_wm(&self, seeds: &[(i64, String, i64)], after: &dyn Fn() -> Result<()>) -> Result<Option<(FactsReport, Vec<(i64, i64)>)>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.query_row("SELECT 1 FROM meta WHERE key = ?1", [WM], |_| Ok(())).optional()?.is_some() {
            return Ok(None);
        }
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let mut report = FactsReport::default();
        // Provenance from what 0.17 holds, never invented (T-WM-9).
        let n = tx.execute("INSERT OR IGNORE INTO placements (folder, path, record, at) SELECT folder, dir, record, at FROM expected WHERE at >= ?1", [at - EXPECTED_WEEK])?;
        report.count("placements seeded from expectations", n as u64);
        for (folder, dir, record) in seeds {
            let n = tx.execute("INSERT OR IGNORE INTO placements (folder, path, record, at) VALUES (?1, ?2, ?3, ?4)", params![folder, dir.trim_matches('/'), record, at])?;
            report.count("placements seeded from placed items", n as u64);
        }
        let placed = super::records::placed_in(&tx)?;
        let mut merges: Vec<(i64, i64)> = Vec::new();
        let named: Vec<(i64, String, String)> = tx
            .prepare("SELECT id, COALESCE(title, ''), COALESCE(artist, '') FROM records WHERE merged_into IS NULL AND retired_at IS NULL ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (record, title, artist) in named {
            let (title_clean, artist_clean) = (cleaned(&title), cleaned(&artist));
            let title_stamped = !title.trim().is_empty() && title_clean.as_deref() != Some(title.trim());
            let artist_stamped = !artist.trim().is_empty() && artist_clean.as_deref() != Some(artist.trim());
            if !title_stamped && !artist_stamped {
                continue;
            }
            let subject = ids::record(record);
            if title_stamped && title_clean.is_none() {
                // A title only a stamp: its copy under a placement 0.17's evidence names is that record's, merged (WM.10 (b1)).
                let files: Vec<(i64, String, i64)> = tx
                    .prepare(&format!(
                        "SELECT f.folder, f.rel, f.track FROM files f JOIN tracks t ON t.id = f.track WHERE t.record = ?1 AND t.retired_at IS NULL AND {} ORDER BY f.folder, f.rel",
                        super::PRESENT_F
                    ))?
                    .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                let places: Vec<(i64, String)> = files.iter().map(|(f, r, _)| (*f, r.clone())).collect();
                if let Some(into) = super::provenance::placed_for(&placed, &places).filter(|into| *into != record) {
                    let folded = super::identity::fold(&artist_clean.clone().unwrap_or_default());
                    match super::provenance::judged_in(&tx, &places, None, &folded, into)? {
                        Some(super::provenance::Placed::Copy { takes, .. }) => {
                            for (f, stays) in takes {
                                let goes = files[f].2;
                                if goes != stays {
                                    tx.execute(&format!("UPDATE tracks SET retired_at = ?2, record = NULL, back = NULL, {} WHERE id = ?1", super::REMEMBER), params![goes, at])?;
                                    super::taken_for_good(&tx, goes, stays)?;
                                }
                            }
                            tx.execute("UPDATE tracks SET record = ?2 WHERE record = ?1", params![record, into])?;
                            super::records::merge_records_in(&tx, record, into)?;
                            report.list("merged by its seeded placement", format!("{subject} \"{title}\" into {}", ids::record(into)));
                            merges.push((record, into));
                            continue;
                        }
                        _ => report.list("a placement names another record, its tracks differ: not merged", format!("{subject} \"{title}\" ({})", ids::record(into))),
                    }
                } else {
                    report.list("made by a stamp, no placement known", format!("{subject} \"{title}\""));
                }
            }
            // Its clean name: the stamp cut, or as a file with no album titles it.
            let new_title = if title_stamped { title_clean.unwrap_or_else(|| super::records::UNKNOWN_ALBUM.to_string()) } else { title.clone() };
            let new_artist = if artist_stamped { artist_clean.unwrap_or_else(|| crate::serve::library::UNKNOWN_ARTIST.to_string()) } else { artist.clone() };
            tx.execute("UPDATE records SET title = ?2, artist = ?3 WHERE id = ?1", params![record, new_title, new_artist])?;
            report.list("named again by the tag rule", format!("{subject} \"{title}\" by \"{artist}\" -> \"{new_title}\" by \"{new_artist}\""));
            report.count("records named again", 1);
        }
        for (table, line) in [("artists", "artists named by a stamp (their credits follow the cleaned files at the first build)"), ("labels", "labels named by a stamp (their credits follow the cleaned files at the first build)")] {
            let rows: Vec<(i64, String)> = tx.prepare(&format!("SELECT id, name FROM {table} ORDER BY id"))?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            for (id, name) in rows.into_iter().filter(|(_, name)| cleaned(name).as_deref() != Some(name.trim())) {
                report.list(line, format!("{id} \"{name}\""));
            }
        }
        report.count("merged", merges.len() as u64);
        after()?;
        tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![WM, serde_json::to_string(&report)?])?;
        tx.commit()?;
        Ok(Some((report, merges)))
    }
}

/// The part `covers`' mark (SCENARIOS-COUPLING COU.20, T-COU-21): its report, in `FactsReport`'s shape.
pub const COVERS: &str = "018:covers";

/// A 0.17 Change cover pick as its kept run gives it: the record it was for (`al-<n>`, followed through merges), the picture's bytes
/// read from what the run kept (the file it wrote), who and when (the run's), and the run, whose Undo drops the pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverPick {
    pub record: i64,
    pub bytes: Vec<u8>,
    pub who: Option<String>,
    pub at: i64,
    pub run: i64,
}

impl Catalogue {
    /// The part `covers` (COU.20, T-COU-21), once: each 0.17 Change cover pick still kept and not undone (`picks`, the journal's,
    /// read by the app) becomes its record's chosen cover in Cratarr's store, by the hand, at its run's time; of two picks of one
    /// record the later stays. A record a hand picked since in the store keeps that. `unknown`: picks whose run 0.17 pruned (what the
    /// data folder still holds of them), listed: they can't be known as picks, so their records show their files' picture (C-COU-6).
    /// No file is read but the kept pictures, and none written. One IMMEDIATE transaction whose last write is its mark; `after` heard
    /// before it. Done before: none. Blocking.
    pub fn step_018_covers(&self, picks: &[CoverPick], unknown: &[String], after: &dyn Fn() -> Result<()>) -> Result<Option<FactsReport>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.query_row("SELECT 1 FROM meta WHERE key = ?1", [COVERS], |_| Ok(())).optional()?.is_some() {
            return Ok(None);
        }
        let mut report = FactsReport::default();
        let picks: &[CoverPick] = if true { &[] } else { picks };
        let unknown: &[String] = if true { &[] } else { unknown };
        let mut picks: Vec<&CoverPick> = picks.iter().collect();
        picks.sort_by_key(|p| (p.at, p.run));
        for p in picks {
            let Some(record) = super::records::merged_end(&tx, p.record)? else {
                report.list("a 0.17 cover pick of a record no longer known", ids::record(p.record));
                continue;
            };
            let held: Option<(String, i64)> = tx.query_row("SELECT how, at FROM record_covers WHERE record = ?1", [record], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            if held.as_ref().is_some_and(|(how, at)| how != super::covers::CARRIED && how != super::covers::FIRST && *at > p.at) {
                report.list("a 0.17 cover pick a later hand pick keeps", ids::record(record));
                continue;
            }
            let cover = super::covers::Cover {
                mime: crate::serve::art::sniff(&p.bytes).to_string(),
                bytes: p.bytes.clone(),
                source: "upload".to_string(),
                who: p.who.clone(),
                at: p.at,
                how: super::covers::CARRIED.to_string(),
                run: Some(p.run),
                change: None,
            };
            super::covers::keep_in(&tx, record, &cover)?;
            report.count("0.17 cover picks carried", 1);
        }
        for u in unknown {
            report.list("a 0.17 cover pick whose run was pruned: its files' picture shows", u.clone());
        }
        after()?;
        tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![COVERS, serde_json::to_string(&report)?])?;
        tx.commit()?;
        Ok(Some(report))
    }
}
