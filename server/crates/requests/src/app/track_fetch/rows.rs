//! The page's single searches and fetches on rows in the journal's file (DI-Q3 (a); SCENARIOS-RN DQ5): each search an item of the
//! queue `search` (its track, candidates and the files offered), each fetch one of `fetch` (its track, candidate, staged file and
//! verdict) with its step holding slskd's batch. Kept as each one's state changes, never its percent (R7). A start reads them back
//! (`load`): a search cut off says so, a fetch under way is followed again by its batch (`resume`: no new POST, SK2.3's rule), one
//! checking is heard again on its staged file, one passed still offers Add to album.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{FetchJob, FetchView, Fetching, Jobs, ReleaseTrack, SearchJob, SearchView, Searching, Wanted};
use crate::intake::{ItemRow, StepRow};
use crate::journal::Journal;
use crate::slskd::{Candidate, Offered};

/// The queues they are kept as.
pub(super) const SEARCHES: &str = "search";
pub(super) const FETCHES: &str = "fetch";

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // A test's own journal (`keep_in`), on its thread.
    static AT: std::cell::RefCell<Option<Arc<Journal>>> = const { std::cell::RefCell::new(None) };
}

/// Tests: the searches and fetches kept in `journal` on this thread (none: nowhere).
#[cfg(any(test, feature = "test-support"))]
pub fn keep_in(journal: Option<Arc<Journal>>) {
    AT.with(|a| *a.borrow_mut() = journal);
}

/// Tests: a restart of these searches and fetches alone (the tests beside keep theirs): their memory forgotten, read back from their
/// rows as a start reads them.
#[cfg(any(test, feature = "test-support"))]
pub fn restarted(ids: &[&str]) {
    let mut loaded = load();
    let mut g = super::JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let jobs = g.get_or_insert_with(Jobs::default);
    for id in ids {
        jobs.searches.remove(*id);
        jobs.fetches.remove(*id);
        if let Some(s) = loaded.searches.remove(*id) {
            jobs.searches.insert(id.to_string(), s);
        }
        if let Some(f) = loaded.fetches.remove(*id) {
            jobs.fetches.insert(id.to_string(), f);
        }
    }
}

/// Where they are kept: the server's journal (none without one).
pub(super) fn journal() -> Option<Arc<Journal>> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(j) = AT.with(|a| a.borrow().clone()) {
        return Some(j);
    }
    crate::journal::instance()
}

/// What a search or a fetch is for, as kept: its track whole (what the page is not sent of it too).
#[derive(Serialize, Deserialize)]
struct KeptWanted {
    album: String,
    track: ReleaseTrack,
    track_id: String,
    artist_id: Option<String>,
    path: Option<PathBuf>,
    replace: Option<String>,
}

impl KeptWanted {
    fn of(w: &Wanted) -> Self {
        KeptWanted { album: w.album.clone(), track: w.track.clone(), track_id: w.track.track_id.clone(), artist_id: w.track.artist_id.clone(), path: w.track.path.clone(), replace: w.replace.clone() }
    }

    fn wanted(self) -> Wanted {
        let track = ReleaseTrack { track_id: self.track_id, artist_id: self.artist_id, path: self.path, ..self.track };
        Wanted { album: self.album, track, replace: self.replace }
    }
}

#[derive(Serialize, Deserialize)]
struct KeptSearch {
    wanted: KeptWanted,
    candidates: Vec<Candidate>,
    /// Each candidate's file as offered, in its order (what a fetch of it asks slskd for).
    offered: Vec<Offered>,
    bandcamp: String,
    why: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct KeptFetch {
    wanted: KeptWanted,
    candidate: Candidate,
    offered: Offered,
    staged: Option<PathBuf>,
    why: Option<String>,
    score: Option<f32>,
}

fn word<T: Serialize>(state: &T) -> String {
    serde_json::to_value(state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// A search kept as it is now.
pub(super) fn keep_search(id: &str, s: &SearchJob) {
    let fields = KeptSearch {
        wanted: KeptWanted::of(&s.wanted),
        candidates: s.view.candidates.clone(),
        offered: s.view.candidates.iter().map(|c| c.offered.clone()).collect(),
        bandcamp: s.view.bandcamp.clone(),
        why: s.view.why.clone(),
    };
    keep(SEARCHES, id, &word(&s.view.state), &fields, Vec::new());
}

/// A fetch kept as it is now: its state, its batch on its step; never its percent.
pub(super) fn keep_fetch(id: &str, f: &FetchJob) {
    let Some((candidate, batch)) = f.candidate.clone() else { return };
    let fields = KeptFetch {
        wanted: KeptWanted::of(&f.wanted),
        offered: candidate.offered.clone(),
        candidate,
        staged: f.staged.clone(),
        why: f.view.why.clone(),
        score: f.view.score,
    };
    let state = word(&f.view.state);
    let step = StepRow { step: "fetch".into(), state: state.clone(), batch: Some(batch), ..Default::default() };
    keep(FETCHES, id, &state, &fields, vec![step]);
}

fn keep<T: Serialize>(kind: &str, id: &str, state: &str, fields: &T, steps: Vec<StepRow>) {
    let Some(j) = journal() else { return };
    let row = ItemRow { id: id.to_string(), kind: kind.into(), seq: now_ms(), state: state.into(), asked_by: crate::serve::who::current().map(|w| w.user), fields: serde_json::to_value(fields).unwrap_or_default() };
    let kept = j.write(|tx| {
        // Its place in its queue is the one it came in at.
        let seq = tx.intake_item(id)?.map_or(row.seq, |(r, _)| r.seq);
        tx.keep_items(kind, None, &[(ItemRow { seq, ..row.clone() }, steps)], &[])
    });
    if let Err(e) = kept {
        tracing::warn!(target: "library", id, "a single track's {kind} not kept: {e:#}");
    }
}

/// A search or fetch forgotten.
pub(super) fn forget(kind: &str, id: &str) {
    let Some(j) = journal() else { return };
    if let Err(e) = j.write(|tx| tx.keep_items(kind, None, &[], &[id.to_string()])) {
        tracing::warn!(target: "library", id, "a single track's {kind} not forgotten: {e:#}");
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// The searches and fetches kept, as a start reads them back: a search cut off while it searched says so (its slskd search went with
/// the stop); every fetch as it was kept, its percent 0 until slskd is read again.
pub(super) fn load() -> Jobs {
    let mut jobs = Jobs::default();
    let Some(j) = journal() else { return jobs };
    match j.intake(SEARCHES) {
        Ok((_, items)) => {
            for (row, _) in items {
                let Ok(k) = serde_json::from_value::<KeptSearch>(row.fields) else { continue };
                let candidates = k.candidates.into_iter().zip(k.offered).map(|(c, offered)| Candidate { offered, ..c }).collect();
                let mut view = SearchView { state: Searching::Done, candidates, bandcamp: k.bandcamp, why: k.why, failure: None };
                view.state = match row.state.as_str() {
                    "done" => Searching::Done,
                    "failed" => Searching::Failed,
                    _ => {
                        view.why = Some(CUT_OFF.into());
                        Searching::Failed
                    }
                };
                jobs.searches.insert(row.id, SearchJob { wanted: k.wanted.wanted(), view });
            }
        }
        Err(e) => tracing::warn!(target: "library", "the single searches kept not read: {e:#}"),
    }
    match j.intake(FETCHES) {
        Ok((_, items)) => {
            for (row, steps) in items {
                let Ok(k) = serde_json::from_value::<KeptFetch>(row.fields) else { continue };
                let state: Fetching = serde_json::from_value(serde_json::Value::String(row.state.clone())).unwrap_or(Fetching::Queued);
                let batch = steps.iter().find(|s| s.step == "fetch").and_then(|s| s.batch.clone()).unwrap_or_default();
                let candidate = Candidate { offered: k.offered, ..k.candidate };
                let view = FetchView { state, percent: 0.0, why: k.why, score: k.score, failure: None };
                jobs.fetches.insert(row.id, FetchJob { wanted: k.wanted.wanted(), view, staged: k.staged, candidate: Some((candidate, batch)) });
            }
        }
        Err(e) => tracing::warn!(target: "library", "the single fetches kept not read: {e:#}"),
    }
    jobs
}

/// What a search cut off by a stop says.
pub(super) const CUT_OFF: &str = "The search did not finish before Cratarr stopped; search again.";

/// The fetches a start found under way, to follow again: each one's id, its candidate and batch, its track, and its staged file
/// where it was being checked.
pub(super) fn under_way(jobs: &Jobs) -> Vec<(String, Candidate, String, ReleaseTrack, Option<PathBuf>, Fetching)> {
    jobs.fetches
        .iter()
        .filter(|(_, f)| matches!(f.view.state, Fetching::Queued | Fetching::Downloading | Fetching::Checking))
        .filter_map(|(id, f)| {
            let (c, batch) = f.candidate.clone()?;
            Some((id.clone(), c, batch, f.wanted.track.clone(), f.staged.clone(), f.view.state.clone()))
        })
        .collect()
}

/// Whether a staged folder is one a kept fetch names (the 24-hour tidy leaves it).
pub(super) fn named(jobs: &Jobs, dir: &str) -> bool {
    jobs.fetches.contains_key(dir)
}
