//! Bulk intake of missing tracks (0.13, the owner, 2026-10-31: "Bulk search, you approve"; decision 0080's amendment):
//! the albums named have each missing track searched in turn, the best file preselected; the administrator approves
//! many at once in one list; each approved file is fetched, gated and added in turn. A queue over single-track intake
//! (`track_fetch`): the same search, the same gate (decoded whole, the recording's length, AcoustID), the same Add to
//! album and Undo. Nothing goes in without a tick, so March 2026's unattended batches cannot happen again; one slskd
//! search at a time, paced.
//!
//! From 0.15 (the plans' lessons from SoulSync and DroppedNeedle) the queue is kept on disk (`track-intake.json` in the
//! data folder; a search or fetch a restart cut short starts again); a track not found is searched again on the
//! Soulseek ladder's waits (`soulseek::wait_after`: never given up; Search now overrides); a user who refuses, errs or
//! keeps it queued too long is remembered for the track, and its next candidate fetched by the same tick; a file that
//! fails the gate is set aside with why (`track-intake-set-aside` in the data folder), and the next candidate tried.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use super::track_fetch::{Fetching, Searching};
use crate::slskd::Candidate;

/// Between two searches: gentle with Soulseek.
const PACE: Duration = Duration::from_secs(3);
/// A search is followed this long at most, a fetch this long (slskd's queues can be slow).
const SEARCH_AT_MOST: Duration = Duration::from_secs(120);
const FETCH_AT_MOST: Duration = Duration::from_secs(45 * 60);
/// Candidates an item shows, best first.
const SHOWN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Queued,
    Searching,
    Found,
    None,
    Failed,
    Approved,
    Fetching,
    Checking,
    Added,
    Refused,
}

impl State {
    /// Whether nothing more happens to it (one not found is searched again later: not finished).
    fn finished(self) -> bool {
        matches!(self, State::Failed | State::Added | State::Refused)
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct AlbumRef {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Item {
    pub id: String,
    pub album: AlbumRef,
    pub disc: u32,
    pub position: u32,
    pub title: String,
    pub recording: String,
    pub state: State,
    #[serde(skip_deserializing)]
    pub candidates: Vec<Candidate>,
    /// The candidate preselected (the best), or the one approved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Added: its run's Undo (Changes to files, decision 0092's writers' map: a run per approval).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    /// Users who failed this track: never asked for it again.
    #[serde(default)]
    pub failed: Vec<String>,
    /// Searches in a row that found nothing, and when it is searched again (epoch s; 0 or unreadable: due).
    #[serde(default)]
    pub misses: u32,
    #[serde(default)]
    pub due: i64,
    /// Its files set aside, with why.
    #[serde(default)]
    pub set_aside: Vec<String>,
    #[serde(skip)]
    search: Option<String>,
    #[serde(skip)]
    fetch: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct View {
    pub running: bool,
    pub items: Vec<Item>,
}

#[derive(Default, Serialize, serde::Deserialize)]
struct Queue {
    #[serde(skip)]
    running: bool,
    next: u64,
    items: Vec<Item>,
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
/// Where the queue is kept: the data folder's `track-intake.json`.
static FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

fn queue<T>(f: impl FnOnce(&mut Queue) -> T) -> T {
    let mut g = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let q = g.get_or_insert_with(|| {
        let file = FILE.get_or_init(|| crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| d.join("track-intake.json")).unwrap_or_default());
        let mut q: Queue = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // What a restart cut short starts again: its search (the candidates and the in-flight fetch are gone).
        for i in q.items.iter_mut().filter(|i| matches!(i.state, State::Searching | State::Found | State::Approved | State::Fetching | State::Checking)) {
            (i.state, i.chosen, i.due) = (State::Queued, None, 0);
        }
        q
    });
    let out = f(q);
    if let Some(file) = FILE.get().filter(|f| !f.as_os_str().is_empty())
        && let Ok(b) = serde_json::to_vec(q)
    {
        let part = file.with_extension("json.part");
        if std::fs::write(&part, b).and_then(|_| std::fs::rename(&part, file)).is_err() {
            tracing::warn!(target: "library", file = %file.display(), "the track intake's queue could not be kept");
        }
    }
    out
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn with_item(id: &str, f: impl FnOnce(&mut Item)) {
    queue(|q| {
        if let Some(i) = q.items.iter_mut().find(|i| i.id == id) {
            f(i);
        }
    });
}

pub fn view() -> View {
    queue(|q| View { running: q.running, items: q.items.clone() })
}

/// What the worker does next: an approved file first (what the administrator is waiting for), else the next search.
#[derive(Debug, PartialEq)]
enum Next {
    Fetch(String),
    Search(String),
}

fn next_of(items: &[Item], now: i64) -> Option<Next> {
    if let Some(i) = items.iter().find(|i| i.state == State::Approved) {
        return Some(Next::Fetch(i.id.clone()));
    }
    items
        .iter()
        .find(|i| i.state == State::Queued)
        .or_else(|| items.iter().filter(|i| i.state == State::None && i.due <= now).min_by_key(|i| i.due))
        .map(|i| Next::Search(i.id.clone()))
}

/// A track searched now, whatever its wait.
pub fn search_now(app: &std::sync::Arc<super::App>, id: &str) -> View {
    let _ = APP.set(app.clone());
    with_item(id, |i| {
        if matches!(i.state, State::None | State::Failed | State::Refused) {
            (i.state, i.due, i.why) = (State::Queued, 0, None);
        }
    });
    run();
    view()
}

/// After the chosen candidate's user failed: the item's next candidate of another user, approved by the same tick, else
/// searched again at once without that user. Whether there is a next one.
fn on_failed(i: &mut Item, why: String) -> bool {
    let Some(user) = i.chosen.and_then(|c| i.candidates.iter().find(|x| x.id == c)).map(|c| c.user.clone()) else {
        (i.state, i.why) = (State::Refused, Some(why));
        return false;
    };
    if !i.failed.contains(&user) {
        i.failed.push(user);
    }
    i.candidates.retain(|c| !i.failed.contains(&c.user));
    (i.why, i.fetch) = (Some(why), None);
    match i.candidates.first() {
        Some(c) => {
            (i.state, i.chosen) = (State::Approved, Some(c.id));
            true
        }
        None => {
            (i.state, i.chosen, i.due) = (State::Queued, None, 0);
            false
        }
    }
}

/// AcoustID could not be asked about the file, or slskd for it: nothing is known of it or its user, so no user failed,
/// and no next candidate is fetched by itself. The item stops here and says why; Search now takes it up again. A fetch
/// lives in memory, and fetching the file again on a timer would be just the unattended fetching this queue exists not to
/// do.
fn on_no_verdict(i: &mut Item, why: String) {
    (i.state, i.why, i.fetch) = (State::Failed, Some(why), None);
}

/// The albums' missing tracks (those of their release no file holds) queued, each once; the queue as it is now.
pub async fn add_albums(app: &std::sync::Arc<super::App>, albums: &[String]) -> Result<View> {
    let _ = APP.set(app.clone());
    let own = crate::source::cratarr::KIND.id;
    for album in albums {
        let release = super::track_fetch::release(album).await?;
        queue(|q| {
            for t in release.tracks.iter().filter(|t| t.have.is_none()) {
                let there = q.items.iter().any(|i| i.album.id == *album && i.recording == t.recording && !i.state.finished());
                if there {
                    continue;
                }
                q.next += 1;
                q.items.push(Item {
                    id: format!("ti-{}", q.next),
                    album: AlbumRef { source: own.to_string(), id: album.clone(), title: release.title.clone(), artist: release.artist.clone() },
                    disc: t.disc,
                    position: t.position,
                    title: t.title.clone(),
                    recording: t.recording.clone(),
                    state: State::Queued,
                    candidates: Vec::new(),
                    chosen: None,
                    why: None,
                    undo: None,
                    failed: Vec::new(),
                    misses: 0,
                    due: 0,
                    set_aside: Vec::new(),
                    search: None,
                    fetch: None,
                });
            }
        });
    }
    run();
    Ok(view())
}

/// These found items approved (with the candidate given, else the one preselected); the queue as it is now.
pub fn approve(app: &std::sync::Arc<super::App>, picked: &[(String, Option<usize>)]) -> View {
    let _ = APP.set(app.clone());
    queue(|q| {
        for (id, candidate) in picked {
            if let Some(i) = q.items.iter_mut().find(|i| i.id == *id && i.state == State::Found) {
                let c = candidate.or(i.chosen).filter(|c| i.candidates.iter().any(|x| x.id == *c));
                if let Some(c) = c {
                    (i.state, i.chosen) = (State::Approved, Some(c));
                }
            }
        }
    });
    run();
    view()
}

/// One item dropped (its staged file with it); not while it is being fetched.
pub fn drop_item(id: &str) {
    let gone = queue(|q| {
        let at = q.items.iter().position(|i| i.id == id && !matches!(i.state, State::Fetching | State::Checking | State::Searching))?;
        Some(q.items.remove(at))
    });
    if let Some(f) = gone.and_then(|i| i.fetch) {
        super::track_fetch::drop_fetch(&f);
    }
}

/// The finished items cleared.
pub fn clear_finished() {
    queue(|q| q.items.retain(|i| !i.state.finished()));
}

/// The app its adds are runs of (Changes to files), as the routes that start the worker give it.
static APP: std::sync::OnceLock<std::sync::Arc<super::App>> = std::sync::OnceLock::new();

/// The worker started, unless it runs.
fn run() {
    let start = queue(|q| !std::mem::replace(&mut q.running, true));
    if !start {
        return;
    }
    tokio::spawn(async {
        loop {
            let next = queue(|q| {
                let n = next_of(&q.items, now());
                // Nothing now and nothing to search again later: the worker stops (a route starts it again).
                if n.is_none() && !q.items.iter().any(|i| i.state == State::None) {
                    q.running = false;
                }
                n
            });
            match next {
                Some(Next::Search(id)) => {
                    search(&id).await;
                    tokio::time::sleep(PACE).await;
                }
                Some(Next::Fetch(id)) => fetch(&id).await,
                None if queue(|q| q.running) => tokio::time::sleep(Duration::from_secs(60)).await,
                None => return,
            }
        }
    });
}

/// One item searched: its candidates kept, the best preselected.
async fn search(id: &str) {
    let Some((album, recording)) = queue(|q| q.items.iter().find(|i| i.id == id).map(|i| (i.album.id.clone(), i.recording.clone()))) else { return };
    with_item(id, |i| i.state = State::Searching);
    let sid = match super::track_fetch::search(&album, &recording, None).await {
        Ok(s) => s,
        Err(e) => return with_item(id, |i| (i.state, i.why) = (State::Failed, Some(format!("{e:#}")))),
    };
    let started = std::time::Instant::now();
    let seen = loop {
        match super::track_fetch::search_view(&sid) {
            Some(v) if v.state != Searching::Searching => break Some(v),
            Some(_) if started.elapsed() < SEARCH_AT_MOST => tokio::time::sleep(Duration::from_secs(1)).await,
            _ => break None,
        }
    };
    with_item(id, |i| {
        i.search = Some(sid);
        let fresh: Vec<Candidate> = seen.as_ref().map(|v| v.candidates.iter().filter(|c| !i.failed.contains(&c.user)).take(SHOWN).cloned().collect()).unwrap_or_default();
        match seen {
            Some(v) if v.state == Searching::Failed => (i.state, i.why) = (State::Failed, v.why),
            Some(_) if !fresh.is_empty() => {
                i.candidates = fresh;
                (i.state, i.chosen, i.misses, i.why) = (State::Found, i.candidates.first().map(|c| c.id), 0, None);
            }
            Some(_) => {
                i.misses += 1;
                (i.state, i.due) = (State::None, now() + super::soulseek::wait_after(i.misses));
            }
            None => (i.state, i.why) = (State::Failed, Some("The search did not finish.".into())),
        }
    });
}

/// One approved item fetched, gated and, passed, added to its album.
async fn fetch(id: &str) {
    let Some((sid, candidate)) = queue(|q| q.items.iter().find(|i| i.id == id).and_then(|i| Some((i.search.clone()?, i.chosen?)))) else {
        return with_item(id, |i| (i.state, i.why) = (State::Failed, Some("Its search is gone; queue it again.".into())));
    };
    with_item(id, |i| i.state = State::Fetching);
    let fid = match super::track_fetch::fetch(&sid, candidate).await {
        Ok(f) => f,
        Err(e) if e.is::<crate::slskd::Away>() => return with_item(id, |i| on_no_verdict(i, format!("{e:#}"))),
        // The user refused it or is offline: the next candidate.
        Err(e) => {
            return with_item(id, |i| {
                on_failed(i, format!("{e:#}"));
            });
        }
    };
    with_item(id, |i| i.fetch = Some(fid.clone()));
    let started = std::time::Instant::now();
    let mut checked = false;
    let done = loop {
        match super::track_fetch::fetch_view(&fid) {
            Some(v) if matches!(v.state, Fetching::Passed | Fetching::Refused) => break Some(v),
            Some(v) if started.elapsed() < FETCH_AT_MOST => {
                if v.state == Fetching::Checking {
                    checked = true;
                    with_item(id, |i| i.state = State::Checking);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            _ => break None,
        }
    };
    // Refused before the check: the user failed (refused, an error, queued too long); after it: the file did, and is
    // set aside with why. Either way the next candidate. For want of an answer, neither did.
    if let Some(v) = done.as_ref().filter(|v| v.state == Fetching::Refused) {
        let why = v.why.clone().unwrap_or_else(|| "It failed".into());
        if v.no_verdict {
            super::track_fetch::drop_fetch(&fid);
            return with_item(id, |i| on_no_verdict(i, why));
        }
        let aside = if checked { super::track_fetch::set_aside(&fid, &why) } else { None };
        super::track_fetch::drop_fetch(&fid);
        return with_item(id, |i| {
            if let Some(a) = aside {
                i.set_aside.push(a);
            }
            on_failed(i, why);
        });
    }
    match done {
        Some(v) if v.state == Fetching::Passed => match async { super::track_fetch::add(APP.get().ok_or_else(|| anyhow::anyhow!("Cratarr is starting"))?, &fid).await }.await {
            Ok(run) => with_item(id, |i| (i.state, i.undo, i.fetch) = (State::Added, run.undo, None)),
            Err(e) => with_item(id, |i| (i.state, i.why) = (State::Refused, Some(format!("{e:#}")))),
        },
        Some(v) => with_item(id, |i| (i.state, i.why) = (State::Refused, v.why)),
        None => {
            super::track_fetch::drop_fetch(&fid);
            with_item(id, |i| {
                on_failed(i, "The fetch did not finish in time.".into());
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture's store is what 0.15 left, or a move proved on it proves nothing: the queue reads it.
    #[test]
    fn the_tracks_waiting_in_0_15_are_read_as_the_queue_reads_them() {
        let queue: Queue = serde_json::from_str(include_str!("../../tests/fixtures/0.15/track-intake.json")).unwrap();
        assert_eq!((queue.next, queue.items[0].album.id.as_str(), queue.items[0].state), (2, "al-2c5BBHDvIsL51C4yFB1p80", State::Refused));
    }

    fn item(id: &str, state: State) -> Item {
        Item {
            id: id.into(),
            album: AlbumRef { source: "cratarr".into(), id: "al-1".into(), title: "A".into(), artist: "B".into() },
            disc: 1,
            position: 1,
            title: "T".into(),
            recording: format!("rec-{id}"),
            state,
            candidates: Vec::new(),
            chosen: None,
            why: None,
            undo: None,
            failed: Vec::new(),
            misses: 0,
            due: 0,
            set_aside: Vec::new(),
            search: None,
            fetch: None,
        }
    }

    /// What waits finds its album by its id when its turn comes.
    #[test]
    fn a_track_waiting_keeps_its_album_after_the_move() {
        let d = std::env::temp_dir().join(format!("cratarr-intake-moved-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let q = Queue { running: false, next: 2, items: vec![item("a", State::Queued)] };
        std::fs::write(d.join("track-intake.json"), serde_json::to_vec(&q).unwrap()).unwrap();
        let moved = crate::moved::Moved { ids: [("al-1".to_string(), "al-7".to_string())].into(), ..Default::default() };
        let report = moved.stores(&d).unwrap();
        let back: Queue = serde_json::from_slice(&std::fs::read(d.join("track-intake.json")).unwrap()).unwrap();
        assert_eq!((back.items[0].album.id.as_str(), back.next), ("al-7", 2));
        let t = report.table_named("track-intake.json").unwrap();
        assert_eq!((t.before, t.moved), (1, 1));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_approved_file_goes_before_the_next_search_and_finished_ones_wait() {
        let items = [item("a", State::Added), item("b", State::Queued), item("c", State::Approved), item("d", State::Queued)];
        assert_eq!(next_of(&items, 0), Some(Next::Fetch("c".into())), "what the administrator approved first");
        assert_eq!(next_of(&items[..2], 0), Some(Next::Search("b".into())));
        assert_eq!(next_of(&[item("a", State::Found), item("b", State::Refused)], 0), None, "found waits for a tick: nothing goes in without one");
        assert!(State::Added.finished() && !State::Found.finished() && !State::None.finished(), "not found: searched again later");
    }

    #[test]
    fn a_track_not_found_is_searched_again_when_its_wait_is_over() {
        let mut none = item("n", State::None);
        none.due = 1_000;
        assert_eq!(next_of(std::slice::from_ref(&none), 999), None);
        assert_eq!(next_of(std::slice::from_ref(&none), 1_000), Some(Next::Search("n".into())));
        assert_eq!(next_of(&[none, item("q", State::Queued)], 5_000), Some(Next::Search("q".into())), "the queued first");
    }

    fn candidate(id: usize, user: &str) -> Candidate {
        Candidate {
            id,
            user: user.into(),
            file: "f.flac".into(),
            format: "flac".into(),
            lossless: true,
            bitrate: None,
            sample_rate: None,
            bit_depth: None,
            size: 1,
            length_ms: None,
            free_slot: true,
            queue: 0,
            speed: 1,
            offered: crate::slskd::Offered { filename: "f.flac".into(), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false },
        }
    }

    #[test]
    fn a_fetch_with_no_verdict_blames_no_user_and_fetches_nothing_more() {
        let mut i = item("v", State::Checking);
        i.candidates = vec![candidate(0, "a"), candidate(1, "b")];
        i.chosen = Some(0);
        i.fetch = Some("f1".into());
        on_no_verdict(&mut i, "AcoustID could not be reached".into());
        assert_eq!((i.state, i.why.as_deref(), i.fetch.as_deref()), (State::Failed, Some("AcoustID could not be reached"), None));
        assert!(i.failed.is_empty() && i.set_aside.is_empty() && i.candidates.len() == 2, "no user held to it");
        assert_eq!(next_of(std::slice::from_ref(&i), i64::MAX), None, "nothing fetched by itself");
    }

    #[test]
    fn a_failed_user_hands_the_track_to_its_next_candidate_of_another_user() {
        let mut i = item("a", State::Fetching);
        (i.candidates, i.chosen) = (vec![candidate(0, "x"), candidate(1, "x"), candidate(2, "y")], Some(0));
        assert!(on_failed(&mut i, "x is offline".into()));
        assert_eq!((i.state, i.chosen, i.failed.clone(), i.candidates.len()), (State::Approved, Some(2), vec!["x".to_string()], 1), "x's other file too");
        assert!(!on_failed(&mut i, "queued too long".into()));
        assert_eq!((i.state, i.chosen, i.due), (State::Queued, None, 0));
    }

    #[test]
    fn a_kept_queue_reads_back_and_what_a_restart_cut_short_is_searched_again() {
        let raw = r#"{"next":2,"items":[{"id":"ti-2","album":{"source":"cratarr","id":"al-1","title":"A","artist":"B"},"disc":1,"position":3,"title":"T","recording":"r","state":"found","candidates":[]}]}"#;
        let q: Queue = serde_json::from_str(raw).unwrap();
        assert_eq!((q.next, q.items[0].state, q.items[0].misses), (2, State::Found, 0));
    }
}
