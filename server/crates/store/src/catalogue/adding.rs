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
use cratarr_rules::holding::Library;
use rusqlite::params;

use super::{Catalogue, PRESENT_F};

impl Catalogue {
    /// The tags a main library's template names the copy at `dir` in catalogue folder `folder` by (COU.15). Red: the files' raw tags.
    pub fn names_from_record(
        &self,
        folder: i64,
        dir: &str,
        _deciding: &[i64],
        _title: &dyn Fn(i64) -> Option<String>,
        _served: &dyn Fn(&[i64]) -> HashMap<i64, String>,
        _library: &dyn Fn(i64) -> Library,
    ) -> Result<Vec<(String, crate::naming::Tags)>> {
        let prefix = format!("{dir}/");
        Ok(self.album_tags(folder, dir)?.into_iter().map(|(rel, tags)| (rel.strip_prefix(&prefix).unwrap_or(&rel).to_string(), crate::naming::Tags(tags))).collect())
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
