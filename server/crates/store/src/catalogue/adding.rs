//! What the gate names a copy's files by when it places them in a main library (SCENARIOS-COUPLING COU.15; AMENDMENTS 2026-11-20
//! 23:14): adding an arrival, a Move and Replace name folders and files from the record's facts in the database, after WM's
//! cleaning, never from a raw tag. The move and its names are no tag write: the files' bytes stay as they came.
//!
//! - **The album artist** is the record's credit by the one formatter (NAMES.24), each artist by the name the library serves.
//! - **The album** is the record's title, the hand's first (COU.13 (d)); a site's stamp never names a folder (WM.1).
//! - **The year** is the record's by the one rule (`facts::record_year`): where it knows none (its sources differ, none chosen), the
//!   copy's files' own year read through the rule; with none, the year is asked, as before.
//! - **Each file's number** is its track's place in the record (COU.5); a file with none takes the place it pairs with on the
//!   record's list (the pick, else the lists that are no guess). A file the database can't number has none, and the plan says so.
//! - **Each file's title** is its track's as the record knows it: the deciding copy's (a main library's other copy of the track);
//!   a copy in a main library names its own; an arrival of a record held nowhere else takes its list's title; else its own.
//! - A file the catalogue gives no record yet (not read into one) names itself by its own values through the rule.

use std::collections::HashMap;

use anyhow::Result;
use cratarr_domain::facts::{Present, RefTrack, pairs_with};
use cratarr_rules::holding::Library;
use rusqlite::{Connection, OptionalExtension, params};

use super::{Catalogue, PRESENT_F, fact_value};

/// One file of a copy as the database names it, in the tags a library's template reads (`naming::TAGS`).
fn tags_of(fields: &[(&str, Option<String>)]) -> crate::naming::Tags {
    crate::naming::Tags(fields.iter().filter_map(|(name, value)| value.clone().filter(|v| !v.trim().is_empty()).map(|v| (name.to_string(), v))).collect())
}

/// A file's own value of `name`, through the one tag rule.
fn own(tags: &[(String, String)], name: &str) -> Option<String> {
    tags.iter().find(|(n, v)| n.eq_ignore_ascii_case(name) && !v.trim().is_empty()).and_then(|(n, v)| fact_value(n, v))
}

/// The value most of `values` give, a tie to the first given.
fn most(values: impl Iterator<Item = String>) -> Option<String> {
    let mut counted: Vec<(String, usize)> = Vec::new();
    for v in values {
        match counted.iter_mut().find(|(x, _)| *x == v) {
            Some((_, n)) => *n += 1,
            None => counted.push((v, 1)),
        }
    }
    let top = counted.iter().map(|(_, n)| *n).max()?;
    counted.into_iter().find(|(_, n)| *n == top).map(|(v, _)| v)
}

/// The record a copy's files hold tracks of: the one most of them do.
fn record_of(db: &Connection, folder: i64, rels: &[String]) -> Result<Option<i64>> {
    let mut st = db.prepare_cached("SELECT t.record FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder = ?1 AND f.rel = ?2 AND t.retired_at IS NULL")?;
    let mut records = Vec::new();
    for rel in rels {
        if let Some(Some(r)) = st.query_row(params![folder, rel], |r| r.get::<_, Option<i64>>(0)).optional()? {
            records.push(r.to_string());
        }
    }
    Ok(most(records.into_iter()).and_then(|r| r.parse().ok()))
}

/// What names a record's files, beside the files' own.
struct RecordNames {
    credit: Option<String>,
    title: Option<String>,
    year: Option<i32>,
    lists: Vec<Vec<RefTrack>>,
}

impl Catalogue {
    /// The tags a main library's template names the copy at `dir` in catalogue folder `folder` by (COU.15): each music file's path
    /// inside `dir`, in path order, with its names from the record's facts. `deciding`: the catalogue folders whose copies decide a
    /// record's tracks (the main libraries'); `title`: the record's title set by hand, by its id; `served`: each artist's served name
    /// by id, where the library serves one; `library`: each folder's library, as the holding reads it. Changes nothing. Blocking.
    pub fn names_from_record(
        &self,
        folder: i64,
        dir: &str,
        deciding: &[i64],
        title: &dyn Fn(i64) -> Option<String>,
        served: &dyn Fn(&[i64]) -> HashMap<i64, String>,
        library: &dyn Fn(i64) -> Library,
    ) -> Result<Vec<(String, crate::naming::Tags)>> {
        let files = self.album_tags(folder, dir)?;
        let prefix = format!("{dir}/");
        let inside = |rel: &str| rel.strip_prefix(&prefix).unwrap_or(rel).to_string();
        let rels: Vec<String> = files.iter().map(|(rel, _)| rel.clone()).collect();
        let record = record_of(&self.db(), folder, &rels)?;
        let Some(record) = record else {
            // No record yet: its own values through the rule, never a raw stamp.
            return Ok(files.iter().map(|(rel, tags)| (inside(rel), tags_of(&crate::naming::TAGS.map(|n| (n, own(tags, n)))))).collect());
        };
        let names = self.record_names(record, title, served, library)?;
        let db = self.db();
        let present: Vec<Present> = rels.iter().map(|rel| super::provenance::present_in(&db, folder, rel)).collect::<Result<_>>()?;
        // The place each file pairs with on the record's lists, where its track has none of its own.
        let mut listed: Vec<Option<RefTrack>> = vec![None; rels.len()];
        for list in &names.lists {
            for (t, f) in pairs_with(list, &present) {
                if listed[f].is_none() {
                    listed[f] = Some(list[t].clone());
                }
            }
        }
        let decides = deciding.contains(&folder);
        // The copy's own year where the record knows none: its files' by majority, through the rule.
        let own_year = most(files.iter().filter_map(|(_, tags)| own(tags, "originaldate").or_else(|| own(tags, "date")).and_then(|d| d.get(..4).map(str::to_string))));
        let year = names.year.map(|y| y.to_string()).or(own_year);
        let mut place = db.prepare_cached("SELECT t.id, t.disc, t.number FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder = ?1 AND f.rel = ?2 AND t.retired_at IS NULL")?;
        let mut other = db.prepare_cached(&format!("SELECT f.folder, f.rel FROM files f WHERE f.track = ?1 AND f.folder <> ?2 AND {PRESENT_F} ORDER BY f.folder, f.rel"))?;
        let mut out = Vec::with_capacity(files.len());
        for (i, (rel, tags)) in files.iter().enumerate() {
            let (track, disc, number): (Option<i64>, Option<u32>, Option<u32>) =
                place.query_row(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?.unwrap_or((None, None, None));
            let shown = || {
                let p = &present[i];
                if p.title.is_empty() { p.named.first().cloned() } else { Some(p.title.clone()) }
            };
            let deciding_title = match (track, decides) {
                (Some(t), false) => {
                    let elsewhere: Vec<(i64, String)> = other.query_map(params![t, folder], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
                    match elsewhere.into_iter().find(|(f, _)| deciding.contains(f)) {
                        Some((f, r)) => {
                            let p = super::provenance::present_in(&db, f, &r)?;
                            if p.title.is_empty() { p.named.first().cloned() } else { Some(p.title) }
                        }
                        None => None,
                    }
                }
                _ => None,
            };
            let list = listed[i].as_ref();
            let file_title = match (decides, deciding_title) {
                (true, _) => shown(),
                (false, Some(t)) => Some(t),
                (false, None) => list.map(|t| t.title.clone()).or_else(shown),
            };
            // Its track's place; else the place it pairs with on the record's list; else its own numbers through the rule (a track the
            // record has given no place yet), its name's leading number among them.
            let (disc, number) = match number {
                Some(n) => (disc, Some(n)),
                None => match list {
                    Some(t) => (Some(t.disc), Some(t.position)),
                    None => (present[i].disc.or(disc), present[i].number),
                },
            };
            let fields = [
                ("albumartist", names.credit.clone().or_else(|| own(tags, "albumartist"))),
                ("album", names.title.clone().or_else(|| own(tags, "album"))),
                ("artist", own(tags, "artist").or_else(|| names.credit.clone())),
                ("title", file_title),
                ("tracknumber", number.map(|n| n.to_string())),
                ("discnumber", disc.filter(|_| number.is_some()).map(|d| d.to_string())),
                ("originaldate", year.clone()),
            ];
            out.push((inside(rel), tags_of(&fields)));
        }
        Ok(out)
    }

    /// A record's facts as the gate names its files by: its credit (each artist by the name the library serves, else the
    /// catalogue's), its title (the hand's first), its year by the one rule, and the lists its files pair with (the pick, else every
    /// kept list that is no guess). Blocking.
    fn record_names(&self, record: i64, title: &dyn Fn(i64) -> Option<String>, served: &dyn Fn(&[i64]) -> HashMap<i64, String>, library: &dyn Fn(i64) -> Library) -> Result<RecordNames> {
        let Some(read) = self.record_read(record, library)? else { return Ok(RecordNames { credit: None, title: None, year: None, lists: Vec::new() }) };
        let ids: Vec<i64> = read.artists.iter().map(|(id, _)| *id).collect();
        let named = served(&ids);
        let artists: Vec<&str> = read.artists.iter().map(|(id, name)| named.get(id).map_or(name.as_str(), String::as_str)).collect();
        let credit = Some(cratarr_rules::names::credit_text(&artists, None)).filter(|c| !c.trim().is_empty());
        let stored = Some(read.row.title.clone()).filter(|t| !t.trim().is_empty() && t != "[Unknown Album]").and_then(|t| fact_value("album", &t));
        let lists: Vec<Vec<RefTrack>> = match &read.pick {
            Some(p) => read.lists.iter().filter(|l| &l.reference == p).map(|l| l.tracks.clone()).collect(),
            None => read.lists.iter().filter(|l| !l.guessed && !l.tracks.is_empty()).map(|l| l.tracks.clone()).collect(),
        };
        Ok(RecordNames { credit, title: title(read.row.id).or(stored), year: read.year().value, lists })
    }

    /// The records whose present files lie under `dir` in catalogue folder `folder` (a copy there, T-COU-14). Blocking.
    pub fn records_under(&self, folder: i64, dir: &str) -> Result<Vec<i64>> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let db = self.db();
        let mut st = db.prepare_cached(&format!(
            "SELECT DISTINCT t.record FROM files f JOIN tracks t ON t.id = f.track
              WHERE f.folder = ?1 AND substr(f.rel, 1, ?2) = ?3 AND t.record IS NOT NULL AND t.retired_at IS NULL AND {PRESENT_F} ORDER BY t.record"
        ))?;
        let rows = st.query_map(params![folder, prefix.chars().count() as i64, prefix], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
