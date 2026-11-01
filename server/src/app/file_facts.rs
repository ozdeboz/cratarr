//! Types, years and Classical into the files (0.13, the owner, 2026-10-31; decision 0092 and its amendment of 0089).
//! With the switch on, Cratarr's own records' files follow what Cratarr holds, found or set by hand:
//!
//! - **The release type and the extra types** ("album", "compilation"…), each a value of Vorbis `RELEASETYPE` (and
//!   Lidarr's `MUSICBRAINZ_ALBUMTYPE`, `retag`) or ID3v2's `TXXX:MusicBrainz Album Type`, as Picard writes them. What
//!   the files say now is MusicBrainz's word, never read as the type.
//! - **The album's year** (`DATE`) in a track that has none, and in every track where the year was set by hand.
//! - **"Classical"** among a classical record's genres: added, never taken away (no mark says whose a "Classical" is;
//!   the owner's choice).
//!
//! Nothing is written by itself (the owner: always asked): Health offers the records whose files differ with Write, a
//! run on the one path (`file_runs`). Off unless switched on (a server choice, `CHOICE`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;

use super::file_runs::{self, FilePlan, flat};

/// The server choice that switches it on ("1").
pub const CHOICE: &str = "write:file-facts";
/// What its runs are called.
pub const WHAT: &str = "file-facts";
/// The fields it writes, by the tag reader's names.
const FIELDS: [&str; 3] = ["releasetype", "date", "genre"];

/// What the Settings row and Health's line show.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub on: bool,
    /// How many records' files differ from what Cratarr holds (none counted while off).
    pub differ: usize,
    /// Its last run since the server started.
    pub run: Option<file_runs::Status>,
}

/// What Cratarr holds of a record, as written into its files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    pub id: String,
    /// The release type and the extra types, as kept ("album", "compilation").
    pub types: Vec<String>,
    pub year: Option<u32>,
    /// Its year set by hand: every track gets it.
    pub year_set: bool,
    pub classical: bool,
}

pub fn is_on(serve: &crate::serve::Serve) -> bool {
    serve.userdata.import_choice(CHOICE).ok().flatten().is_some_and(|c| c == "1")
}

pub fn set_on(serve: &crate::serve::Serve, on: bool) -> Result<()> {
    serve.userdata.set_import_choice(CHOICE, if on { "1" } else { "" })
}

/// A year as a date's first four digits.
fn year_of(date: &str) -> Option<u32> {
    date.trim().get(..4).and_then(|y| y.parse().ok())
}

/// What one track of a record gets: each field that differs, its values before (as the file has them) and after.
pub fn track_plan(r: &Held, has: &HashMap<String, Vec<String>>) -> (Vec<(String, Vec<String>)>, Vec<(String, Vec<String>)>) {
    let now = |n: &str| has.get(n).cloned().unwrap_or_default();
    let (mut before, mut after) = (Vec::new(), Vec::new());
    if !r.types.is_empty() {
        let (mut want, mut got) = (flat(&r.types), flat(&now("releasetype")));
        want.sort();
        got.sort();
        if want != got {
            before.push(("releasetype".to_string(), now("releasetype")));
            after.push(("releasetype".to_string(), r.types.clone()));
        }
    }
    if let Some(y) = r.year {
        let date = now("date");
        let wrong = match date.first() {
            None => true,
            Some(d) => r.year_set && year_of(d) != Some(y),
        };
        if wrong {
            before.push(("date".to_string(), date));
            after.push(("date".to_string(), vec![y.to_string()]));
        }
    }
    if r.classical {
        let genres = now("genre");
        if !flat(&genres).iter().any(|g| g == "classical") {
            let mut more = genres.clone();
            more.push("Classical".to_string());
            before.push(("genre".to_string(), genres));
            after.push(("genre".to_string(), more));
        }
    }
    (before, after)
}

/// Every file of Cratarr's own records that differs from what Cratarr holds, with what it gets. Blocking.
pub fn plans(serve: &crate::serve::Serve) -> Result<Vec<FilePlan>> {
    let classical = super::classical::index(serve)?;
    let cat = serve.catalogue();
    let roots = cat.folders();
    let values = cat.named_values(&FIELDS)?;
    let mut held: Vec<(Held, Vec<(i64, String)>)> = Vec::new();
    {
        let db = serve.library.db();
        let mut st = db.prepare(
            "SELECT a.id, a.release_type, a.secondary, a.year, a.set_fields, s.folder, s.rel FROM albums a
             JOIN songs s ON s.album_id = a.id ORDER BY a.n, s.disc_no, s.track_no, s.rel",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        for row in rows {
            let (id, kind, secondary, year, set, folder, rel) = row?;
            if held.last().is_none_or(|(h, _)| h.id != id) {
                let types = kind.into_iter().chain(secondary.split('\u{1f}').filter(|s| !s.is_empty()).map(str::to_string)).collect();
                let classical = classical.records.get(&id).is_some_and(|r| r.classical);
                let year_set = set.split('\u{1f}').any(|f| f == "year");
                held.push((Held { id, types, year: year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0), year_set, classical }, Vec::new()));
            }
            if let Some((_, files)) = held.last_mut() {
                files.push((folder, rel));
            }
        }
    }
    let none = HashMap::new();
    let mut out = Vec::new();
    for (h, files) in held {
        for (folder, rel) in files {
            let Some(root) = usize::try_from(folder).ok().and_then(|i| roots.get(i)) else { continue };
            let (before, after) = track_plan(&h, values.get(&(folder, rel.clone())).unwrap_or(&none));
            if !after.is_empty() {
                out.push(FilePlan { album: h.id.clone(), path: root.join(&rel), before, after });
            }
        }
    }
    Ok(out)
}

/// The Settings row's and Health's view: whether it is on, how many records differ, its last run.
pub async fn view() -> Result<View> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    tokio::task::spawn_blocking(move || {
        let on = is_on(&served);
        let differ = if on {
            let mut albums: Vec<String> = plans(&served)?.into_iter().map(|p| p.album).collect();
            albums.dedup();
            albums.len()
        } else {
            0
        };
        Ok(View { on, differ, run: file_runs::last(WHAT) })
    })
    .await?
}

/// Write (an administrator's tap): every record whose files differ, as a run on the one path.
pub async fn write(app: &Arc<super::App>) -> Result<file_runs::Status> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    if file_runs::busy(WHAT) {
        anyhow::bail!("Already writing");
    }
    let plans = tokio::task::spawn_blocking(move || -> Result<Vec<FilePlan>> {
        anyhow::ensure!(is_on(&served), "Writing into files is switched off");
        plans(&served)
    })
    .await??;
    anyhow::ensure!(!plans.is_empty(), "Nothing differs");
    Ok(file_runs::start(app, WHAT, plans))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn has(pairs: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        pairs.iter().map(|(k, v)| (k.to_string(), s(v))).collect()
    }
    fn held(types: &[&str], year: Option<u32>, year_set: bool, classical: bool) -> Held {
        Held { id: "al-1".into(), types: s(types), year, year_set, classical }
    }

    #[test]
    fn a_track_gets_what_differs_only() {
        // MusicBrainz's word in the file, Cratarr's two layers: written.
        let (b, a) = track_plan(&held(&["album", "compilation"], None, false, false), &has(&[("releasetype", &["album"])]));
        assert_eq!((b, a), (vec![("releasetype".into(), s(&["album"]))], vec![("releasetype".into(), s(&["album", "compilation"]))]));
        // The same, whatever the case or separator: nothing.
        let (_, a) = track_plan(&held(&["album", "compilation"], None, false, false), &has(&[("releasetype", &["Compilation\0Album"])]));
        assert!(a.is_empty());
        // A year where there is none; a year found never overwrites one there; a year set by hand does.
        let (_, a) = track_plan(&held(&[], Some(2010), false, false), &has(&[]));
        assert_eq!(a, vec![("date".into(), s(&["2010"]))]);
        let (_, a) = track_plan(&held(&[], Some(2010), false, false), &has(&[("date", &["2011-04-02"])]));
        assert!(a.is_empty());
        let (_, a) = track_plan(&held(&[], Some(2010), true, false), &has(&[("date", &["2011-04-02"])]));
        assert_eq!(a, vec![("date".into(), s(&["2010"]))]);
        let (_, a) = track_plan(&held(&[], Some(2010), true, false), &has(&[("date", &["2010-04-02"])]));
        assert!(a.is_empty(), "the full date of the year set is kept");
        // Classical added to the genres there, never twice.
        let (b, a) = track_plan(&held(&[], None, false, true), &has(&[("genre", &["Baroque"])]));
        assert_eq!((b, a), (vec![("genre".into(), s(&["Baroque"]))], vec![("genre".into(), s(&["Baroque", "Classical"]))]));
        let (_, a) = track_plan(&held(&[], None, false, true), &has(&[("genre", &["Baroque; classical"])]));
        assert!(a.is_empty());
        // Not classical: its genres untouched, a "Classical" there kept.
        let (_, a) = track_plan(&held(&[], None, false, false), &has(&[("genre", &["Classical"])]));
        assert!(a.is_empty());
    }
}
