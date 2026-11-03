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
//! Nothing is written by itself (the owner: always asked): the switch only decides whether Health lists the records
//! whose files differ; the review sheet offers them (`proposal`), a run on the one path (`file_runs`) writes the ticked.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::Serialize;

use super::file_runs::{self, FilesState, Proposal, flat};
use crate::journal::{FileWrite, RunRecord};

/// The server choice that switches it on ("1").
pub const CHOICE: &str = "write:file-facts";
/// Its kind of change.
pub const KIND: &str = "types";
const TITLE: &str = "Types and years";
/// The fields it writes, by the tag reader's names.
const FIELDS: [&str; 3] = ["releasetype", "date", "genre"];

/// The Settings row and Health's line: whether it is on, how many records' files differ (none counted while off).
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub on: bool,
    pub differ: usize,
}

/// What Cratarr holds of a record, as written into its files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// The release type and the extra types, as kept ("album", "compilation").
    pub types: Vec<String>,
    /// Where the type comes from, as the sheet says it.
    pub types_from: String,
    pub year: Option<u32>,
    /// Its year set by hand: every track gets it.
    pub year_set: bool,
    pub classical: bool,
}

/// The records whose files differ, as last counted, and the library build they were counted for.
static DIFFER: Mutex<Option<(i64, HashSet<String>)>> = Mutex::new(None);

/// On unless switched off: type and year write by default (decision 0097, the Catalogue sheet's Write).
pub fn is_on(serve: &crate::serve::Serve) -> bool {
    serve.userdata.import_choice(CHOICE).ok().flatten().is_none_or(|c| c == "1")
}

pub fn set_on(serve: &crate::serve::Serve, on: bool) -> Result<()> {
    serve.userdata.set_import_choice(CHOICE, if on { "1" } else { "0" })
}

/// A year as a date's first four digits.
fn year_of(date: &str) -> Option<u32> {
    date.trim().get(..4).and_then(|y| y.parse().ok())
}

/// What one track of a record gets: each field that differs, its values before (as the file has them) and after, and
/// where the new value comes from.
pub fn track_plan(r: &Held, has: &HashMap<String, Vec<String>>) -> (Vec<(String, Vec<String>)>, Vec<(String, Vec<String>)>, Vec<(String, String)>) {
    let now = |n: &str| has.get(n).cloned().unwrap_or_default();
    let (mut before, mut after, mut from) = (Vec::new(), Vec::new(), Vec::new());
    if !r.types.is_empty() {
        let (mut want, mut got) = (flat(&r.types), flat(&now("releasetype")));
        want.sort();
        got.sort();
        if want != got {
            before.push(("releasetype".to_string(), now("releasetype")));
            after.push(("releasetype".to_string(), r.types.clone()));
            from.push(("releasetype".to_string(), r.types_from.clone()));
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
            from.push(("date".to_string(), if r.year_set { "set by hand" } else { "the album's year" }.to_string()));
        }
    }
    if r.classical {
        let genres = now("genre");
        if !flat(&genres).iter().any(|g| g == "classical") {
            let mut more = genres.clone();
            more.push("Classical".to_string());
            before.push(("genre".to_string(), genres));
            after.push(("genre".to_string(), more));
            from.push(("genre".to_string(), "Classical".to_string()));
        }
    }
    (before, after, from)
}

/// Where a type comes from, as the sheet says it.
fn types_from(type_from: Option<&str>, why: Option<&str>) -> String {
    match (type_from, why) {
        (Some("set"), _) => "set by hand".into(),
        (_, Some(_)) => "the type rule".into(),
        (Some("musicbrainz"), None) => "MusicBrainz".into(),
        (Some("discogs"), None) => "Discogs".into(),
        _ => "the type rule".into(),
    }
}

/// Every file of Cratarr's own records (only `albums` where given) that differs from what Cratarr holds, with what it
/// gets. Blocking.
pub fn proposal(serve: &crate::serve::Serve, albums: Option<&[String]>) -> Result<Proposal> {
    let classical = super::classical::index(serve)?;
    let cat = serve.catalogue();
    let roots = cat.folders();
    let values = cat.named_values(&FIELDS)?;
    let mut held: Vec<(Held, Vec<(i64, String, String)>)> = Vec::new();
    {
        let db = serve.library.db();
        let mut st = db.prepare(
            "SELECT a.id, a.name, a.artist, a.release_type, a.secondary, a.type_from, a.type_why, a.year, a.set_fields,
               s.folder, s.rel, s.title
             FROM albums a JOIN songs s ON s.album_id = a.id ORDER BY a.n, s.disc_no, s.track_no, s.rel",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                (r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?),
                (r.get::<_, Option<i64>>(7)?, r.get::<_, String>(8)?),
                (r.get::<_, i64>(9)?, r.get::<_, String>(10)?, r.get::<_, String>(11)?),
            ))
        })?;
        for row in rows {
            let ((id, title, artist), (kind, secondary, from, why), (year, set), file) = row?;
            if albums.is_some_and(|a| !a.contains(&id)) {
                continue;
            }
            if held.last().is_none_or(|(h, _)| h.id != id) {
                let types = kind.into_iter().chain(secondary.split('\u{1f}').filter(|s| !s.is_empty()).map(str::to_string)).collect();
                let classical = classical.records.get(&id).is_some_and(|r| r.classical);
                let year_set = set.split('\u{1f}').any(|f| f == "year");
                // A year below 1000 is a placeholder (0001-01-01), no year: never written into a file.
                let year = year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y >= 1000);
                let types_from = types_from(from.as_deref(), why.as_deref());
                held.push((Held { id, title, artist, types, types_from, year, year_set, classical }, Vec::new()));
            }
            if let Some((_, files)) = held.last_mut() {
                files.push(file);
            }
        }
    }
    let none = HashMap::new();
    let mut p = Proposal { kind: KIND.into(), title: TITLE.into(), ..Proposal::default() };
    for (h, files) in held {
        let mut any = false;
        for (folder, rel, track) in files {
            let Some(root) = usize::try_from(folder).ok().and_then(|i| roots.get(i)) else { continue };
            let (before, after, from) = track_plan(&h, values.get(&(folder, rel.clone())).unwrap_or(&none));
            if !after.is_empty() {
                any = true;
                p.files.push(FileWrite { album: h.id.clone(), path: root.join(&rel), track: Some(track), before, after, from, undone: false, whole: None });
            }
        }
        if any {
            p.records.push(RunRecord { album: h.id, title: h.title, artist: h.artist });
        }
    }
    if albums.is_none() {
        let differ = p.records.iter().map(|r| r.album.clone()).collect();
        *DIFFER.lock().unwrap_or_else(|e| e.into_inner()) = Some((serve.built_at(), differ));
    }
    Ok(p)
}

/// The Settings row's and Health's view: whether it is on and how many records' files differ.
pub async fn view() -> Result<View> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    tokio::task::spawn_blocking(move || {
        let on = is_on(&served);
        let differ = if on { proposal(&served, None)?.records.len() } else { 0 };
        Ok(View { on, differ })
    })
    .await?
}

/// The review sheet: every record whose files differ (only `albums` where given).
pub async fn plan(albums: Option<Vec<String>>) -> Result<file_runs::PlanView> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let p = tokio::task::spawn_blocking(move || proposal(&served, albums.as_deref())).await??;
    Ok(file_runs::view(&p.kind, &p.title, &p.records, &p.files, false))
}

/// The ticked records written (an administrator's apply on the review sheet), each set aside where it is no longer what
/// the sheet showed (its stamp): a run on the one path.
pub async fn apply(app: &Arc<super::App>, albums: Vec<String>, stamps: Option<HashMap<String, String>>) -> Result<file_runs::Status> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    anyhow::ensure!(!albums.is_empty(), "No records ticked");
    anyhow::ensure!(!file_runs::busy(KIND), "Already writing");
    let p = tokio::task::spawn_blocking(move || -> Result<Proposal> {
        anyhow::ensure!(is_on(&served), "Writing into files is switched off");
        Ok(file_runs::ticked(proposal(&served, Some(&albums))?, &albums, stamps.as_ref()))
    })
    .await??;
    anyhow::ensure!(!p.files.is_empty() || !p.failed.is_empty(), "Nothing differs");
    Ok(file_runs::start(app, p))
}

/// Cratarr's own records said with their files' state: from a run where one touched them, else "differs" where they
/// were last counted so (counted again in the background when the library changed).
pub fn tell(source: &str, albums: &mut [crate::source::Album]) {
    if source != crate::source::cratarr::KIND.id || albums.is_empty() {
        return;
    }
    let Some(served) = crate::serve::instance() else { return };
    if !is_on(&served) {
        return;
    }
    let now = super::moment::now_secs();
    let built = served.built_at();
    let differ = DIFFER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if differ.as_ref().is_none_or(|(at, _)| *at != built) {
        // Counted again for the next answer, not this one.
        let s = served.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = proposal(&s, None) {
                tracing::debug!(target: "library", reason = %format!("{e:#}"), "records whose files differ not counted");
            }
        });
    }
    for a in albums {
        a.release.files_state = file_runs::state_of(&a.id, now)
            .or_else(|| differ.as_ref().filter(|(_, d)| d.contains(&a.id)).map(|_| FilesState { state: "differs".into(), done: None, of: None, run: None, at: None }));
    }
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
        Held { id: "al-1".into(), types: s(types), types_from: "the type rule".into(), year, year_set, classical, ..Held::default() }
    }

    #[test]
    fn a_track_gets_what_differs_only() {
        // MusicBrainz's word in the file, Cratarr's two layers: written, said where from.
        let (b, a, f) = track_plan(&held(&["album", "compilation"], None, false, false), &has(&[("releasetype", &["album"])]));
        assert_eq!((b, a), (vec![("releasetype".into(), s(&["album"]))], vec![("releasetype".into(), s(&["album", "compilation"]))]));
        assert_eq!(f, vec![("releasetype".to_string(), "the type rule".to_string())]);
        // The same, whatever the case or separator: nothing.
        let (_, a, _) = track_plan(&held(&["album", "compilation"], None, false, false), &has(&[("releasetype", &["Compilation\0Album"])]));
        assert!(a.is_empty());
        // A year where there is none; a year found never overwrites one there; a year set by hand does.
        let (_, a, _) = track_plan(&held(&[], Some(2010), false, false), &has(&[]));
        assert_eq!(a, vec![("date".into(), s(&["2010"]))]);
        let (_, a, _) = track_plan(&held(&[], Some(2010), false, false), &has(&[("date", &["2011-04-02"])]));
        assert!(a.is_empty());
        let (_, a, f) = track_plan(&held(&[], Some(2010), true, false), &has(&[("date", &["2011-04-02"])]));
        assert_eq!((a, f), (vec![("date".into(), s(&["2010"]))], vec![("date".to_string(), "set by hand".to_string())]));
        let (_, a, _) = track_plan(&held(&[], Some(2010), true, false), &has(&[("date", &["2010-04-02"])]));
        assert!(a.is_empty(), "the full date of the year set is kept");
        // Classical added to the genres there, never twice.
        let (b, a, _) = track_plan(&held(&[], None, false, true), &has(&[("genre", &["Baroque"])]));
        assert_eq!((b, a), (vec![("genre".into(), s(&["Baroque"]))], vec![("genre".into(), s(&["Baroque", "Classical"]))]));
        let (_, a, _) = track_plan(&held(&[], None, false, true), &has(&[("genre", &["Baroque; classical"])]));
        assert!(a.is_empty());
        // Not classical: its genres untouched, a "Classical" there kept.
        let (_, a, _) = track_plan(&held(&[], None, false, false), &has(&[("genre", &["Classical"])]));
        assert!(a.is_empty());
    }

    #[test]
    fn where_a_type_comes_from() {
        assert_eq!(types_from(Some("set"), None), "set by hand");
        assert_eq!(types_from(Some("musicbrainz"), Some("tracks")), "the type rule");
        assert_eq!(types_from(Some("musicbrainz"), None), "MusicBrainz");
        assert_eq!(types_from(None, None), "the type rule");
    }
}
