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
//!
//! From 0.17 (TN, the owner's OQ-T4 (a)) a track whose source could not answer for it (slskd or AcoustID away, or asking to be
//! left alone until a deadline) holds at its step, under its tick, until then, and says why: no try counted, no user failed,
//! nothing set aside, no other candidate fetched. The time it waits for is its `due`, kept with the queue, so a restart asks
//! nothing early.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use super::soulseek::{CHECK_AGAIN, SLSKD_AGAIN};
use super::track_fetch::{Fetching, Searching};
use super::turn::{Failure, class};
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
    /// Searches in a row that found nothing, and when it is searched again (epoch s; 0 or unreadable: due). One held for its
    /// source (TN) waits for this too, at whatever step it holds.
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

#[cfg(test)]
thread_local! {
    // A test's own store (`tests::start_in`): the queue read from and kept in it on the test's thread.
    static AT: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Where the queue is kept now (none before the server is open).
fn file() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(at) = AT.with(|a| a.borrow().clone()) {
        return at;
    }
    FILE.get_or_init(|| crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| d.join("track-intake.json")).unwrap_or_default()).clone()
}

fn queue<T>(f: impl FnOnce(&mut Queue) -> T) -> T {
    let mut g = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let file = file();
    let q = g.get_or_insert_with(|| {
        let mut q: Queue = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // What a restart cut short starts again: its search (the candidates and the in-flight fetch are gone), at its time where it
        // was held for its source (TN.9: a deadline kept, nothing asked before it).
        for i in q.items.iter_mut().filter(|i| matches!(i.state, State::Searching | State::Found | State::Approved | State::Fetching | State::Checking)) {
            (i.state, i.chosen) = (State::Queued, None);
        }
        q
    });
    let out = f(q);
    if !file.as_os_str().is_empty()
        && let Ok(b) = serde_json::to_vec(q)
    {
        let part = file.with_extension("json.part");
        if std::fs::write(&part, b).and_then(|_| std::fs::rename(&part, &file)).is_err() {
            tracing::warn!(target: "library", file = %file.display(), "the track intake's queue could not be kept");
        }
    }
    out
}

/// Now (epoch s), as the turns read it (`moment::now_secs`): a test moves it.
fn now() -> i64 {
    super::moment::now_secs()
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

/// What the worker does next: an approved file first (what the administrator is waiting for), else the next search; neither
/// before its time where it holds for its source (TN.7).
#[derive(Debug, PartialEq)]
enum Next {
    Fetch(String),
    Search(String),
}

fn next_of(items: &[Item], now: i64) -> Option<Next> {
    let due = |i: &&Item| i.due <= now;
    // Approved, or held at its check: its tick taken up.
    if let Some(i) = items.iter().filter(due).find(|i| matches!(i.state, State::Approved | State::Checking)) {
        return Some(Next::Fetch(i.id.clone()));
    }
    items
        .iter()
        .filter(due)
        .find(|i| i.state == State::Queued)
        .or_else(|| items.iter().filter(due).filter(|i| i.state == State::None).min_by_key(|i| i.due))
        .map(|i| Next::Search(i.id.clone()))
}

/// How long the worker sleeps when nothing is due now: until the first of what waits for its time (a track not found, or one held
/// for its source), a minute at most. None when nothing waits: the worker stops (a route starts it again).
fn wake_in(items: &[Item], now: i64) -> Option<Duration> {
    items
        .iter()
        .filter(|i| matches!(i.state, State::None | State::Queued | State::Approved | State::Checking))
        .map(|i| (i.due - now).clamp(1, 60))
        .min()
        .map(|secs| Duration::from_secs(secs as u64))
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

/// When a track whose source could not answer for it is taken up again, by the failure's class (TN.1): a pause at its deadline
/// (a moment on at least), the source away after `away` seconds. None where the failure says something of the user or the file,
/// or is not understood.
fn held_for(f: &Failure, now: i64, away: i64) -> Option<i64> {
    match f {
        Failure::Paused { until } => Some((*until).max(now + 1)),
        Failure::Away(_) => Some(now + away),
        _ => None,
    }
}

/// AcoustID could not be asked about the file, or slskd for it (away, or paused until a deadline): nothing is known of it or its
/// user, so no user failed, no try counts, and no next candidate is fetched by itself (TN.7, the owner's OQ-T4 (a)). The item
/// holds at its `step` (Queued, its tick's Approved, or Checking with its staged file) until `due`, saying why, and is taken up
/// again then under the same tick.
fn on_no_verdict(i: &mut Item, step: State, why: String, due: i64) {
    (i.state, i.why, i.due) = (step, Some(why), due);
    if step != State::Checking {
        i.fetch = None;
    }
}

/// A search that met a failure of class `f`: held Queued where its source could not answer (slskd or MusicBrainz away, or paused),
/// no try counted (TN.7, TN.13); failed with why otherwise, as before.
fn search_failed(i: &mut Item, f: &Failure, why: Option<String>) {
    match held_for(f, now(), SLSKD_AGAIN) {
        Some(due) => on_no_verdict(i, State::Queued, why.unwrap_or_default(), due),
        None => (i.state, i.why) = (State::Failed, why),
    }
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
    queue(|q| tick(q, picked));
    run();
    view()
}

/// These found items ticked: approved with the candidate given, else the one preselected.
fn tick(q: &mut Queue, picked: &[(String, Option<usize>)]) {
    for (id, candidate) in picked {
        if let Some(i) = q.items.iter_mut().find(|i| i.id == *id && i.state == State::Found) {
            let c = candidate.or(i.chosen).filter(|c| i.candidates.iter().any(|x| x.id == *c));
            if let Some(c) = c {
                (i.state, i.chosen) = (State::Approved, Some(c));
            }
        }
    }
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

/// What the bulk queue asks of single-track intake (`track_fetch`) that needs the running server: a track's search started, a
/// candidate fetched, a check that had no verdict asked again, a file set aside, a passed file added. `Live` is the server's; the
/// tests' stands in, with fakes of slskd and AcoustID behind it.
trait Intake: Sync {
    fn search(&self, album: String, recording: String) -> impl Future<Output = Result<String>> + Send;
    fn fetch(&self, search: String, candidate: usize) -> impl Future<Output = Result<String>> + Send;
    /// A fetch whose check had no verdict checked again on its staged file: whether there was one.
    fn check_again(&self, fetch: &str) -> bool;
    fn set_aside(&self, fetch: &str, why: &str) -> Option<String>;
    /// The file added to its album: its run's Undo.
    fn add(&self, fetch: String) -> impl Future<Output = Result<Option<String>>> + Send;
}

/// Single-track intake as the server runs it.
struct Live;

impl Intake for Live {
    fn search(&self, album: String, recording: String) -> impl Future<Output = Result<String>> + Send {
        async move { super::track_fetch::search(&album, &recording, None).await }
    }

    fn fetch(&self, search: String, candidate: usize) -> impl Future<Output = Result<String>> + Send {
        async move { super::track_fetch::fetch(&search, candidate).await }
    }

    fn check_again(&self, fetch: &str) -> bool {
        super::track_fetch::check_again(fetch)
    }

    fn set_aside(&self, fetch: &str, why: &str) -> Option<String> {
        super::track_fetch::set_aside(fetch, why)
    }

    fn add(&self, fetch: String) -> impl Future<Output = Result<Option<String>>> + Send {
        async move { Ok(super::track_fetch::add(APP.get().ok_or_else(|| anyhow::anyhow!("Cratarr is starting"))?, &fetch).await?.undo) }
    }
}

/// The worker started, unless it runs.
fn run() {
    let start = queue(|q| !std::mem::replace(&mut q.running, true));
    if !start {
        return;
    }
    crate::run::spawn(async {
        loop {
            let (next, wait) = queue(|q| {
                let now = now();
                let (n, wait) = (next_of(&q.items, now), wake_in(&q.items, now));
                // Nothing now and nothing to take up later: the worker stops (a route starts it again).
                if n.is_none() && wait.is_none() {
                    q.running = false;
                }
                (n, wait)
            });
            match (next, wait) {
                (Some(Next::Search(id)), _) => {
                    search(&id).await;
                    tokio::time::sleep(PACE).await;
                }
                (Some(Next::Fetch(id)), _) => fetch(&id).await,
                (None, Some(wait)) => tokio::time::sleep(wait).await,
                (None, None) => return,
            }
        }
    });
}

/// One item searched: its candidates kept, the best preselected.
async fn search(id: &str) {
    search_with(id, &Live).await;
}

/// [`search`], its search started through `intake` (the tests' stands in for the server's).
async fn search_with<I: Intake>(id: &str, intake: &I) {
    let Some((album, recording)) = queue(|q| q.items.iter().find(|i| i.id == id).map(|i| (i.album.id.clone(), i.recording.clone()))) else { return };
    with_item(id, |i| i.state = State::Searching);
    let sid = match intake.search(album, recording).await {
        Ok(s) => s,
        Err(e) => return with_item(id, |i| search_failed(i, &class(&e), Some(format!("{e:#}")))),
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
            Some(v) if v.state == Searching::Failed => search_failed(i, &v.failure.unwrap_or(Failure::Other), v.why),
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

/// One approved item fetched, gated and, passed, added to its album; one held at its check gated again on its staged file.
async fn fetch(id: &str) {
    fetch_with(id, &Live).await;
}

/// [`fetch`], through `intake` (the tests' stands in for the server's).
async fn fetch_with<I: Intake>(id: &str, intake: &I) {
    let Some((sid, candidate, held)) =
        queue(|q| q.items.iter().find(|i| i.id == id).and_then(|i| Some((i.search.clone()?, i.chosen?, i.fetch.clone().filter(|_| i.state == State::Checking)))))
    else {
        return with_item(id, |i| (i.state, i.why) = (State::Failed, Some("Its search is gone; queue it again.".into())));
    };
    // Held at its check (TN.7): its staged file gated again, nothing fetched again. One gone meanwhile is fetched again.
    let again = held.as_deref().is_some_and(|f| intake.check_again(f));
    let fid = match held {
        Some(f) if again => f,
        held => {
            if let Some(f) = held {
                super::track_fetch::drop_fetch(&f);
            }
            with_item(id, |i| (i.state, i.fetch) = (State::Fetching, None));
            match intake.fetch(sid, candidate).await {
                Ok(f) => f,
                Err(e) => {
                    let why = format!("{e:#}");
                    return with_item(id, |i| match held_for(&class(&e), now(), SLSKD_AGAIN) {
                        // slskd away or paused: nothing known of the user. Held at its tick, the same candidate taken up again later.
                        Some(due) => on_no_verdict(i, State::Approved, why, due),
                        // The user refused it or is offline: the next candidate.
                        None => {
                            on_failed(i, why);
                        }
                    });
                }
            }
        }
    };
    with_item(id, |i| i.fetch = Some(fid.clone()));
    let started = std::time::Instant::now();
    let mut checked = again;
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
        // No answer (slskd or AcoustID away, or paused): held at its check with its staged file, else at its tick (OQ-T4 (a)).
        let staged = super::track_fetch::staged(&fid);
        if let Some(due) = v.failure.as_ref().and_then(|f| held_for(f, now(), if staged { CHECK_AGAIN } else { SLSKD_AGAIN })) {
            if staged {
                return with_item(id, |i| on_no_verdict(i, State::Checking, why, due));
            }
            super::track_fetch::drop_fetch(&fid);
            return with_item(id, |i| on_no_verdict(i, State::Approved, why, due));
        }
        let aside = if checked { intake.set_aside(&fid, &why) } else { None };
        super::track_fetch::drop_fetch(&fid);
        return with_item(id, |i| {
            if let Some(a) = aside {
                i.set_aside.push(a);
            }
            on_failed(i, why);
        });
    }
    match done {
        Some(v) if v.state == Fetching::Passed => match intake.add(fid.clone()).await {
            Ok(undo) => with_item(id, |i| (i.state, i.undo, i.fetch) = (State::Added, undo, None)),
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
pub(crate) mod tests {
    use super::*;

    /// The fixture's store is what 0.15 left, or a move proved on it proves nothing: the queue reads it.
    #[test]
    fn the_tracks_waiting_in_0_15_are_read_as_the_queue_reads_them() {
        let queue: Queue = serde_json::from_str(include_str!("../../../../tests/fixtures/0.15/track-intake.json")).unwrap();
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

    /// SK2.4 (kind 6): a Soulseek record, the bulk queue's ti-1 and root's single-track fetch start at once: their three POSTs go
    /// one after another through slskd's one lane, and the 429 one meets is asked again, never refused for it (today's "slskd would
    /// not fetch it: 429 …"). The bulk queue's POST is single-track fetch's (`track_intake::fetch` → `track_fetch::fetch`). The
    /// one-user rule is Soulseek's records' alone (SC02.9; the review of part 2, L2): ti-1, of the record's own peer, starts beside
    /// the record's download.
    #[tokio::test]
    async fn the_bulk_queue_and_single_tracks_wait_their_turn_in_slskds_lane() {
        let slskd = crate::slskd::scripted::Scripted::new().slow(200).busy(1).hold_all(10.0).singles(&[r"z\07 Track.flac", r"z\03 Single.flac"]);
        let d = std::env::temp_dir().join(format!("cratarr-intake-lane-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        super::super::soulseek::picked_for_tests("ss-sk24", 9_024, "peer-a");
        let file = |name: &str| crate::slskd::Offered { filename: format!(r"z\{name}"), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false };
        let client = slskd.client();
        let (record, bulk, single) = tokio::join!(
            super::super::soulseek::start_for_tests("ss-sk24", &client, &d),
            super::super::track_fetch::fetch_through_for_tests(&client, d.clone(), d.join("ti-1"), "peer-a", file("07 Track.flac")),
            super::super::track_fetch::fetch_through_for_tests(&client, d.clone(), d.join("single"), "peer-s", file("03 Single.flac"))
        );
        assert!(record, "the record under way");
        assert!(bulk.is_ok() && single.is_ok(), "neither refused for a 429: {bulk:?} {single:?}");
        assert!(slskd.never_overlapped() && slskd.violations().is_empty(), "{:?}", slskd.violations());
        let mut taken: Vec<String> = slskd.posts().into_iter().filter(|o| o.status == 200).map(|o| o.user).collect();
        taken.sort();
        assert_eq!(taken, ["peer-a", "peer-a", "peer-s"], "three POSTs, one after another");
        let record = super::super::soulseek::item_for_tests("ss-sk24").and_then(|i| i.batch).unwrap();
        assert!(slskd.going().contains(&record), "ti-1 of peer-a taken while the record's batch of peer-a downloads");
        assert_eq!(slskd.posts().iter().filter(|o| o.status == 429).count(), 1, "the one 429 asked again");
        assert!(super::super::soulseek::item_for_tests("ss-sk24").is_some_and(|i| i.why.is_none() && i.failed.is_empty()));
        std::fs::remove_dir_all(&d).ok();
    }

    /// Changed by design (TN.7, the owner's OQ-T4 (a)): a fetch with no verdict (AcoustID could not be asked) still blames no user
    /// and fetches no other candidate, but is no longer failed: it holds at its check, its fetch kept, until its time, and then the
    /// same tick's file is taken up again.
    #[test]
    fn a_fetch_with_no_verdict_blames_no_user_and_fetches_nothing_more() {
        let mut i = item("v", State::Checking);
        i.candidates = vec![candidate(0, "a"), candidate(1, "b")];
        i.chosen = Some(0);
        i.fetch = Some("f1".into());
        on_no_verdict(&mut i, State::Checking, "AcoustID could not be reached".into(), 1_000);
        assert_eq!((i.state, i.why.as_deref(), i.fetch.as_deref(), i.due), (State::Checking, Some("AcoustID could not be reached"), Some("f1"), 1_000));
        assert!(i.failed.is_empty() && i.set_aside.is_empty() && i.candidates.len() == 2, "no user held to it");
        assert_eq!(next_of(std::slice::from_ref(&i), 999), None, "nothing fetched by itself before its time");
        assert_eq!((next_of(std::slice::from_ref(&i), 1_000), i.chosen), (Some(Next::Fetch("v".into())), Some(0)), "then its own tick's file, no other candidate");
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

    // ---- TN: a track held for its source (the owner's OQ-T4 (a)), through slskd and AcoustID fakes on tokio's paused clock

    use std::collections::{HashMap, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use anyhow::Context as _;
    use axum::http::{Method, StatusCode};
    use axum::response::{IntoResponse, Response};
    use serde_json::{Value, json};

    use crate::app::track_fetch::ReleaseTrack;
    use crate::app::turn::{ACOUSTID, Pinned, Turn};

    pub(crate) const T0: i64 = 1_800_000_000;

    /// The epoch clock pinned at t0 and moved on by the test (`moment::PINNED`), tokio's paused clock moved on as far with it.
    pub(crate) struct Clock(std::cell::Cell<i64>);

    impl Clock {
        pub(crate) fn at_t0() -> (Pinned, Self) {
            crate::app::moment::PINNED.store(T0, Ordering::Relaxed);
            (Pinned(T0), Clock(std::cell::Cell::new(0)))
        }

        pub(crate) async fn to(&self, secs: i64) {
            let by = secs - self.0.replace(secs);
            crate::app::moment::PINNED.store(T0 + secs, Ordering::Relaxed);
            tokio::time::advance(Duration::from_secs(by as u64)).await;
        }
    }

    /// A folder of the test's own, empty.
    fn fresh(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-intake-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The server started on `dir`: the queue kept in its `track-intake.json` (on this thread), read anew from what is there.
    fn start_in(dir: &Path) {
        AT.with(|a| *a.borrow_mut() = Some(dir.join("track-intake.json")));
        stop();
    }

    /// The server stopped: the queue's memory gone, its store left on disk.
    fn stop() {
        *QUEUE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The test done: the queue kept nowhere.
    pub(crate) fn done(dir: &Path) {
        stop();
        AT.with(|a| *a.borrow_mut() = None);
        std::fs::remove_dir_all(dir).ok();
    }

    /// `track-intake.json` as written, its items by id.
    pub(crate) fn stored(dir: &Path) -> HashMap<String, Item> {
        let q: Queue = serde_json::from_slice(&std::fs::read(dir.join("track-intake.json")).unwrap()).unwrap();
        q.items.into_iter().map(|i| (i.id.clone(), i)).collect()
    }

    /// "Tide Tables"' tracks by Glass Harbour, each ten seconds long.
    pub(crate) fn tide_tables() -> Vec<ReleaseTrack> {
        [(1, "Slack Water", "rec-slack"), (2, "Neap", "rec-neap"), (3, "Spring Tide", "rec-spring")]
            .into_iter()
            .map(|(n, title, recording)| crate::app::track_fetch::track_for(1, n, title, "Glass Harbour", Some(10_000), recording))
            .collect()
    }

    /// One of "Tide Tables"' tracks queued.
    pub(crate) fn waiting(id: &str, t: &ReleaseTrack) -> Item {
        Item {
            album: AlbumRef { source: "cratarr".into(), id: "al-tide".into(), title: "Tide Tables".into(), artist: "Glass Harbour".into() },
            disc: t.disc,
            position: t.position,
            title: t.title.clone(),
            recording: t.recording.clone(),
            ..item(id, State::Queued)
        }
    }

    /// A Soulseek user's answer: their copy of a track of "Tide Tables", ten seconds long, a slot free or not.
    fn offers(user: &str, n: u32, title: &str, free: bool) -> Value {
        let file = json!({ "filename": format!("Glass Harbour\\Tide Tables\\0{n} {title}.flac"), "size": 1000, "extension": "flac", "length": 10 });
        json!({ "username": user, "files": [file], "hasFreeUploadSlot": free, "queueLength": 0, "uploadSpeed": 100 })
    }

    /// Ten seconds of a tone as FLAC in `dir`, as the gate passes one (`track_fetch`'s old drive's file).
    pub(crate) fn tone(dir: &Path) -> PathBuf {
        let name = format!("intake-tone-{}", dir.file_name().unwrap().to_string_lossy());
        let w = crate::analysis::tests::wav(&name, 10.0, |t| 0.2 * (t * (440.0 + 200.0 * (t * 0.7).sin()) * std::f64::consts::TAU).sin() as f32);
        let out = dir.join("tone.flac");
        crate::convert::to_cd_flac(&w, &out).unwrap();
        std::fs::remove_file(&w).ok();
        out
    }

    /// slskd as the bulk queue and Soulseek meet it, answering within the test (`Slskd::within`). Its searches and batches answer as
    /// scripted (else 200 and 201), each search finding what `found` holds for its words; a batch taken lands its files in slskd's
    /// folder (each `file`), as slskd's would. All asked is noted with the moment (epoch s) it was asked. A search, a batch or a
    /// batch's read while its 429 holds, or anything that is not slskd's API as they ask it, is a violation, answered 418. Until
    /// `away_until` (epoch s) all it is asked is answered 503, slskd not up: 0.17's stand-in for a while slskd holds its askers off,
    /// as slskd has no pause of its own there (round 1.3's lane: a 429 is asked again in it).
    #[derive(Default)]
    pub(crate) struct FakeSlskd {
        pub(crate) away_until: i64,
        pub(crate) searches: Mutex<VecDeque<(u16, Option<&'static str>)>>,
        pub(crate) batches: Mutex<VecDeque<u16>>,
        pub(crate) found: HashMap<&'static str, Value>,
        pub(crate) folder: PathBuf,
        pub(crate) file: PathBuf,
        pub(crate) asked: Mutex<Vec<(String, String, i64, Value)>>,
        pub(crate) held: Mutex<i64>,
        pub(crate) violations: Mutex<Vec<String>>,
    }

    impl FakeSlskd {
        fn answer(&self, method: &Method, path: &str, body: &[u8]) -> Response {
            let now = now();
            let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
            self.asked.lock().unwrap().push((method.to_string(), path.to_string(), now, body.clone()));
            if now < self.away_until {
                return (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable").into_response();
            }
            let held = *self.held.lock().unwrap();
            let parts: Vec<&str> = path.trim_start_matches("/api/v0/").split('/').collect();
            match (method.as_str(), parts.as_slice()) {
                ("POST", ["searches"]) => {
                    if now < held {
                        self.violation(format!("a search at {now}, held until {held}"));
                    }
                    let (status, said) = self.searches.lock().unwrap().pop_front().unwrap_or((200, None));
                    let mut r = (StatusCode::from_u16(status).unwrap(), axum::Json(json!({ "id": body["id"], "searchText": body["searchText"] }))).into_response();
                    if let Some(said) = said {
                        *self.held.lock().unwrap() = now + said.parse::<i64>().unwrap();
                        r.headers_mut().insert(axum::http::header::RETRY_AFTER, said.parse().unwrap());
                    }
                    r
                }
                ("GET", ["searches", _]) => axum::Json(json!({ "isComplete": true })).into_response(),
                ("GET", ["searches", id, "responses"]) => {
                    let asked = self.asked.lock().unwrap();
                    let words = asked.iter().find(|(m, p, _, b)| m == "POST" && p.ends_with("/searches") && b["id"] == *id).map(|(.., b)| b["searchText"].clone());
                    axum::Json(words.and_then(|w| self.found.get(w.as_str()?).cloned()).unwrap_or_else(|| json!([]))).into_response()
                }
                ("DELETE", ["searches", _]) => StatusCode::NO_CONTENT.into_response(),
                ("POST", ["transfers", "downloads", "batches"]) => {
                    if now < held {
                        self.violation(format!("a batch at {now}, held until {held}"));
                    }
                    let status = self.batches.lock().unwrap().pop_front().unwrap_or(201);
                    if status >= 300 {
                        return (StatusCode::from_u16(status).unwrap(), "Service Unavailable").into_response();
                    }
                    let to = self.folder.join(body["options"]["destination"].as_str().unwrap_or_default());
                    std::fs::create_dir_all(&to).unwrap();
                    // Each file of the batch (a track, or a record's folder), as slskd lands it.
                    for f in body["files"].as_array().into_iter().flatten() {
                        std::fs::copy(&self.file, to.join(crate::slskd::base_name(f["filename"].as_str().unwrap_or_default()))).unwrap();
                    }
                    (StatusCode::CREATED, axum::Json(json!({ "batch": { "transfers": [] }, "failures": [] }))).into_response()
                }
                ("GET", ["transfers", "downloads", "batches", id]) => {
                    if now < held {
                        self.violation(format!("a batch read at {now}, held until {held}"));
                    }
                    let done = json!({ "id": format!("t-{id}"), "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 });
                    axum::Json(json!({ "transfers": [done] })).into_response()
                }
                ("DELETE", ["transfers", "downloads", _, _]) => StatusCode::NO_CONTENT.into_response(),
                _ => {
                    self.violation(format!("{method} {path}"));
                    StatusCode::IM_A_TEAPOT.into_response()
                }
            }
        }

        fn violation(&self, v: String) {
            self.violations.lock().unwrap().push(v);
        }

        /// The batches it was asked for: each one's user and file.
        fn batches_asked(&self) -> Vec<(String, String)> {
            let asked = self.asked.lock().unwrap();
            let batches = asked.iter().filter(|(m, p, ..)| m == "POST" && p.ends_with("/batches"));
            batches.map(|(.., b)| (b["username"].as_str().unwrap_or_default().to_string(), b["files"][0]["filename"].as_str().unwrap_or_default().to_string())).collect()
        }

        /// The searches it was asked for: each one's words, and when.
        pub(crate) fn searches_asked(&self) -> Vec<(String, i64)> {
            let asked = self.asked.lock().unwrap();
            let searches = asked.iter().filter(|(m, p, ..)| m == "POST" && p.ends_with("/searches"));
            searches.map(|(_, _, at, b)| (b["searchText"].as_str().unwrap_or_default().to_string(), *at)).collect()
        }
    }

    /// slskd at a test address, answered within the test by `fake`.
    pub(crate) fn slskd_at(url: &str, fake: &Arc<FakeSlskd>) -> crate::slskd::Slskd {
        let fake = fake.clone();
        let router = axum::Router::new().fallback(move |method: Method, uri: axum::http::Uri, body: axum::body::Bytes| {
            let fake = fake.clone();
            async move { fake.answer(&method, uri.path(), &body) }
        });
        crate::slskd::Slskd::within(url, "key", router)
    }

    /// AcoustID as the gate asks it, through `acoustid::lookup_in`'s send (TN-a1's seam): its limit (HTTP 429, code 14, the stand-in
    /// body) where scripted, else ok, knowing the file as each of "Tide Tables"' recordings; each lookup's moment noted (epoch s, and
    /// tokio's). A lookup whose `meta` is not `recordingids` or without the key, one under 333 ms after the one before, or one while
    /// its limit holds (its Retry-After where it says one, `said`, else its own default, a minute), is a violation.
    #[derive(Default)]
    pub(crate) struct FakeAcoustid {
        pub(crate) limits: Mutex<VecDeque<bool>>,
        pub(crate) said: Option<i64>,
        pub(crate) asked: Mutex<Vec<(i64, tokio::time::Instant)>>,
        pub(crate) held: Mutex<i64>,
        pub(crate) violations: Mutex<Vec<String>>,
    }

    impl FakeAcoustid {
        pub(crate) fn answer(&self, form: Vec<(&'static str, String)>) -> std::future::Ready<Result<crate::acoustid::Heard>> {
            let (now, at) = (now(), tokio::time::Instant::now());
            let field = |k: &str| form.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
            let mut violations = self.violations.lock().unwrap();
            if field("meta") != Some("recordingids") || field("client") != Some("key") {
                violations.push(format!("asked {form:?}"));
            }
            let mut asked = self.asked.lock().unwrap();
            if asked.last().is_some_and(|(_, last)| at - *last < Duration::from_millis(333)) {
                violations.push(format!("two lookups under 333 ms apart at {now}"));
            }
            let mut held = self.held.lock().unwrap();
            if now < *held {
                violations.push(format!("a lookup at {now}, held until {held}"));
            }
            asked.push((now, at));
            let mut headers = reqwest::header::HeaderMap::new();
            let (status, body) = if self.limits.lock().unwrap().pop_front().unwrap_or(false) {
                *held = now + self.said.unwrap_or(60);
                if let Some(said) = self.said {
                    headers.insert(reqwest::header::RETRY_AFTER, said.into());
                }
                (429, r#"{"status":"error","error":{"code":14,"message":"rate limit exceeded"}}"#.to_string())
            } else {
                let known = json!([{ "id": "rec-slack" }, { "id": "rec-neap" }, { "id": "rec-spring" }]);
                (200, json!({ "status": "ok", "results": [{ "score": 0.97, "recordings": known }] }).to_string())
            };
            std::future::ready(Ok(crate::acoustid::Heard { status: reqwest::StatusCode::from_u16(status).unwrap(), headers, body }))
        }
    }

    /// Single-track intake as the server runs it, through the fakes: "Tide Tables" as its release, slskd answered within the test,
    /// AcoustID asked in a turn of its own, slskd's folder and staging in the test's folder. Each search started is noted (its
    /// release read), and each file set aside or added (none is written into a library).
    pub(crate) struct Through {
        slskd: crate::slskd::Slskd,
        folder: PathBuf,
        staging: PathBuf,
        acoustid: Arc<Turn>,
        heard: Arc<FakeAcoustid>,
        started: Mutex<Vec<(String, i64)>>,
        set_aside: Mutex<Vec<String>>,
        added: Mutex<Vec<String>>,
    }

    impl Through {
        pub(crate) fn new(dir: &Path, slskd: crate::slskd::Slskd, heard: &Arc<FakeAcoustid>) -> Self {
            Through {
                slskd,
                folder: dir.join("slskd"),
                staging: dir.join("staging"),
                acoustid: Arc::new(Turn::new(ACOUSTID)),
                heard: heard.clone(),
                started: Mutex::default(),
                set_aside: Mutex::default(),
                added: Mutex::default(),
            }
        }

        /// The gate as the server's, AcoustID asked through its fake.
        fn gate(&self) -> impl FnOnce(PathBuf, ReleaseTrack) -> std::pin::Pin<Box<dyn Future<Output = Result<f32>> + Send>> + Send + 'static {
            let (turn, heard) = (self.acoustid.clone(), self.heard.clone());
            move |file, track| {
                Box::pin(async move {
                    let lookup = |print: crate::acoustid::Print| async move { crate::acoustid::lookup_in(&turn, |form| heard.answer(form), "key", &print).await };
                    crate::app::track_fetch::gate_with(&file, &track, lookup).await
                })
            }
        }
    }

    impl Intake for Through {
        fn search(&self, album: String, recording: String) -> impl Future<Output = Result<String>> + Send {
            self.started.lock().unwrap().push((recording.clone(), now()));
            let (track, slskd) = (tide_tables().into_iter().find(|t| t.recording == recording), self.slskd.clone());
            async move { Ok(crate::app::track_fetch::search_through(&album, track.context("That recording is not on the album's release")?, slskd)) }
        }

        fn fetch(&self, search: String, candidate: usize) -> impl Future<Output = Result<String>> + Send {
            let (slskd, folder, staging, gate) = (self.slskd.clone(), self.folder.clone(), self.staging.clone(), self.gate());
            async move { crate::app::track_fetch::fetch_through(&search, candidate, slskd, folder, staging, gate).await }
        }

        fn check_again(&self, fetch: &str) -> bool {
            crate::app::track_fetch::check_again_with(fetch, self.gate())
        }

        fn set_aside(&self, fetch: &str, why: &str) -> Option<String> {
            self.set_aside.lock().unwrap().push(format!("{fetch}: {why}"));
            None
        }

        fn add(&self, fetch: String) -> impl Future<Output = Result<Option<String>>> + Send {
            self.added.lock().unwrap().push(fetch);
            std::future::ready(Ok(Some("added-run".to_string())))
        }
    }

    /// Tests elsewhere (TN.5): the bulk queue started on `dir` (`start_in`) with these items, nothing run.
    pub(crate) fn start_with(dir: &Path, items: Vec<Item>) {
        let given = Queue { running: false, next: items.len() as u64, items };
        std::fs::write(dir.join("track-intake.json"), serde_json::to_vec(&given).unwrap()).unwrap();
        start_in(dir);
    }

    /// Tests elsewhere (TN.5): the bulk worker's next step at `now`, taken as its loop takes it, through `intake`: the item it took.
    pub(crate) async fn worked(intake: &Through, now: i64) -> Option<String> {
        match next_of(&view().items, now)? {
            Next::Search(id) => {
                search_with(&id, intake).await;
                Some(id)
            }
            Next::Fetch(id) => {
                fetch_with(&id, intake).await;
                Some(id)
            }
        }
    }

    /// TN.7's never, in the store as written: no track Failed or Refused, no user held to one, nothing set aside.
    fn nothing_failed(items: &HashMap<String, Item>, intake: &Through) {
        for i in items.values() {
            assert!(!matches!(i.state, State::Failed | State::Refused), "{}: {:?}, {:?}", i.id, i.state, i.why);
            assert!(i.failed.is_empty() && i.set_aside.is_empty(), "{}: no user held to it, nothing set aside", i.id);
        }
        assert!(intake.set_aside.lock().unwrap().is_empty(), "nothing set aside");
    }

    /// TN.7 (the owner's OQ-T4 (a); TN.13's "the bulk queue counts no try"): `track-intake.json` with ti-1 approved (candidate 0,
    /// peer-a's), ti-2 queued (searched twice before, found nothing), ti-3 approved and fetched to its check. slskd answers ti-1's
    /// enqueue 503 (away), AcoustID ti-3's lookup code 14, slskd ti-2's search 503 (away: a 429 is 0.17's round 1.3's, asked again in
    /// slskd's lane, never a hold). Then, on disk: ti-1 stays Approved with candidate 0, held to t0+300 (`SLSKD_AGAIN`) and fetched
    /// under its tick at its time, from peer-a again; ti-2 waits Queued until t0+300, its misses as they were, then is searched; ti-3
    /// waits at its check with its staged file until AcoustID's deadline, then is gated on that file, nothing fetched again; each
    /// says why. Never Failed or Refused, a user in
    /// `failed`, a file set aside, the next candidate (peer-b's) fetched, a question to a source before its deadline.
    #[tokio::test(start_paused = true)]
    async fn slskd_away_holds_the_track_and_fails_nothing() {
        let _one = crate::app::switches::TEST_STATE.lock().await;
        let (_pinned, clock) = Clock::at_t0();
        let d = fresh("held");
        let tracks = tide_tables();
        let slskd = Arc::new(FakeSlskd {
            // ti-1's and ti-3's searches found; ti-2's met by slskd not up (503); found after it.
            searches: Mutex::new(VecDeque::from([(200, None), (200, None), (503, None)])),
            // ti-1's enqueue met by slskd not up (503); ti-3's taken; ti-1's taken at its time.
            batches: Mutex::new(VecDeque::from([503, 201])),
            found: HashMap::from([
                ("Glass Harbour Slack Water", json!([offers("peer-a", 1, "Slack Water", true), offers("peer-b", 1, "Slack Water", false)])),
                ("Glass Harbour Neap", json!([offers("peer-a", 2, "Neap", true)])),
                ("Glass Harbour Spring Tide", json!([offers("peer-b", 3, "Spring Tide", true)])),
            ]),
            folder: d.join("slskd"),
            file: tone(&d),
            ..Default::default()
        });
        let heard = Arc::new(FakeAcoustid { limits: Mutex::new(VecDeque::from([true])), ..Default::default() });
        let intake = Through::new(&d, slskd_at("http://192.168.1.20:5031", &slskd), &heard);
        let (slack_a, spring_b) = ("Glass Harbour\\Tide Tables\\01 Slack Water.flac".to_string(), "Glass Harbour\\Tide Tables\\03 Spring Tide.flac".to_string());

        // Given: ti-1 and ti-3 found and ticked (ti-1 its candidate 0, peer-a's, before peer-b's), ti-2 queued with two misses.
        let given = Queue { running: false, next: 3, items: vec![waiting("ti-1", &tracks[0]), Item { misses: 2, ..waiting("ti-2", &tracks[1]) }, waiting("ti-3", &tracks[2])] };
        std::fs::write(d.join("track-intake.json"), serde_json::to_vec(&given).unwrap()).unwrap();
        start_in(&d);
        search_with("ti-1", &intake).await;
        search_with("ti-3", &intake).await;
        queue(|q| tick(q, &[("ti-1".into(), Some(0)), ("ti-3".into(), None)]));
        let ticked = view().items;
        let users = |i: &Item| i.candidates.iter().map(|c| c.user.clone()).collect::<Vec<_>>();
        assert_eq!((ticked[0].state, ticked[0].chosen, users(&ticked[0])), (State::Approved, Some(0), vec!["peer-a".to_string(), "peer-b".into()]));
        assert_eq!((ticked[2].state, ticked[2].chosen), (State::Approved, Some(0)));

        // When: slskd answers ti-1's enqueue 503, AcoustID ti-3's lookup code 14, slskd ti-2's search 503.
        fetch_with("ti-1", &intake).await;
        fetch_with("ti-3", &intake).await;
        search_with("ti-2", &intake).await;

        // Then, in the store: each held at its step until its source's time, saying why.
        let held = stored(&d);
        let (ti1, ti2, ti3) = (&held["ti-1"], &held["ti-2"], &held["ti-3"]);
        assert_eq!((ti1.state, ti1.chosen, ti1.due), (State::Approved, Some(0), T0 + 300), "ti-1 at its tick, the same candidate");
        assert!(ti1.why.as_deref().is_some_and(|w| w.starts_with("slskd would not fetch it: 503")), "{:?}", ti1.why);
        assert_eq!((ti2.state, ti2.misses, ti2.due), (State::Queued, 2, T0 + 300), "ti-2: no try counted");
        assert!(ti2.why.as_deref().is_some_and(|w| w.starts_with("slskd would not search: 503")), "{:?}", ti2.why);
        assert_eq!((ti3.state, ti3.due, ti3.why.as_deref()), (State::Checking, T0 + 60, Some("AcoustID asked Cratarr to wait")), "ti-3 at its check");
        let fid = queue(|q| q.items[2].fetch.clone()).expect("ti-3's fetch kept");
        assert!(crate::app::track_fetch::staged(&fid) && d.join("staging").join(&fid).join("03 Spring Tide.flac").exists(), "ti-3's staged file kept");
        nothing_failed(&held, &intake);
        let items = view().items;
        assert_eq!((next_of(&items, T0), next_of(&items, T0 + 59)), (None, None), "nothing taken up before its time");
        assert_eq!(wake_in(&items, T0), Some(Duration::from_secs(60)), "the worker waits on for them");
        assert_eq!(slskd.batches_asked(), [("peer-a".to_string(), slack_a.clone()), ("peer-b".into(), spring_b)]);
        assert_eq!(heard.asked.lock().unwrap().len(), 1);

        // AcoustID's deadline: ti-3 gated on the file it staged, nothing fetched again, and added.
        clock.to(60).await;
        assert_eq!(next_of(&view().items, T0 + 60), Some(Next::Fetch("ti-3".into())));
        fetch_with("ti-3", &intake).await;
        assert_eq!((stored(&d)["ti-3"].state, stored(&d)["ti-3"].undo.as_deref()), (State::Added, Some("added-run")));
        assert_eq!(*intake.added.lock().unwrap(), [fid], "the file staged before it held, checked again");
        assert_eq!(slskd.batches_asked().len(), 2, "nothing fetched again");
        assert_eq!(heard.asked.lock().unwrap().iter().map(|(at, _)| *at).collect::<Vec<_>>(), [T0, T0 + 60], "AcoustID asked again at its deadline");

        // slskd's time: ti-1 fetched under its tick, the same candidate (peer-a's), never the next (peer-b's); then ti-2 searched;
        // neither before.
        clock.to(299).await;
        assert_eq!(next_of(&view().items, T0 + 299), None, "neither before slskd's time");
        clock.to(300).await;
        assert_eq!(next_of(&view().items, T0 + 300), Some(Next::Fetch("ti-1".into())));
        fetch_with("ti-1", &intake).await;
        assert_eq!(next_of(&view().items, T0 + 300), Some(Next::Search("ti-2".into())));
        search_with("ti-2", &intake).await;
        assert_eq!(stored(&d)["ti-2"].state, State::Found);
        let neap: Vec<i64> = slskd.searches_asked().into_iter().filter(|(w, _)| w == "Glass Harbour Neap").map(|(_, at)| at).collect();
        assert_eq!(neap, [T0, T0 + 300], "ti-2 asked again at slskd's time");
        let last = stored(&d);
        assert_eq!((last["ti-1"].state, last["ti-1"].chosen), (State::Added, Some(0)));
        let slack: Vec<(String, String)> = slskd.batches_asked().into_iter().filter(|(_, f)| f.contains("Slack Water")).collect();
        assert_eq!(slack, [("peer-a".to_string(), slack_a.clone()), ("peer-a".to_string(), slack_a)], "the same candidate under its tick, never peer-b's");
        nothing_failed(&last, &intake);
        assert!(slskd.violations.lock().unwrap().is_empty(), "{:?}", slskd.violations.lock().unwrap());
        assert!(heard.violations.lock().unwrap().is_empty(), "{:?}", heard.violations.lock().unwrap());
        done(&d);
    }

    /// TN.9 (OQ-T3), the bulk queue's half: slskd held until t0+300 (away at ti-2's search: a 429 is asked again in slskd's lane, 0.17's
    /// round 1.3), ti-2 queued with its two
    /// misses and the user who failed it, ti-1 held at its tick for slskd away until t0+300. The server stops at t0+60 and starts at
    /// t0+120: both read back Queued (a restart sends work back, as today), ti-2 with its misses and `failed`, each with its time;
    /// nothing is asked before t0+300, not even a release; then they are searched. Never a held item failed by the restart, nor its
    /// deadline lengthened.
    #[tokio::test(start_paused = true)]
    async fn a_restart_mid_pause_asks_nothing_early() {
        let _one = crate::app::switches::TEST_STATE.lock().await;
        let (_pinned, clock) = Clock::at_t0();
        let d = fresh("restart");
        let tracks = tide_tables();
        let slskd = Arc::new(FakeSlskd {
            // ti-1's search found; ti-2's met by slskd not up (503); both found after it.
            searches: Mutex::new(VecDeque::from([(200, None), (503, None)])),
            // ti-1's enqueue met by slskd not up.
            batches: Mutex::new(VecDeque::from([503])),
            found: HashMap::from([
                ("Glass Harbour Slack Water", json!([offers("peer-a", 1, "Slack Water", true)])),
                ("Glass Harbour Neap", json!([offers("peer-a", 2, "Neap", true), offers("peer-b", 2, "Neap", true)])),
            ]),
            folder: d.join("slskd"),
            ..Default::default()
        });
        let heard = Arc::new(FakeAcoustid::default());
        let intake = Through::new(&d, slskd_at("http://192.168.1.20:5032", &slskd), &heard);
        let given = Queue { running: false, next: 2, items: vec![waiting("ti-1", &tracks[0]), Item { misses: 2, failed: vec!["peer-b".into()], ..waiting("ti-2", &tracks[1]) }] };
        std::fs::write(d.join("track-intake.json"), serde_json::to_vec(&given).unwrap()).unwrap();
        start_in(&d);
        search_with("ti-1", &intake).await;
        queue(|q| tick(q, &[("ti-1".into(), Some(0))]));
        fetch_with("ti-1", &intake).await;
        search_with("ti-2", &intake).await;
        let before = stored(&d);
        assert_eq!((before["ti-1"].state, before["ti-1"].due), (State::Approved, T0 + 300));
        assert_eq!((before["ti-2"].state, before["ti-2"].misses, before["ti-2"].due), (State::Queued, 2, T0 + 300));

        // The server stops at t0+60 and starts at t0+120: the queue read anew from its store.
        clock.to(60).await;
        stop();
        clock.to(120).await;
        let back = view().items;
        assert_eq!((back[0].id.as_str(), back[0].state, back[0].chosen, back[0].due), ("ti-1", State::Queued, None, T0 + 300), "sent back, with its time");
        let ti2 = &back[1];
        assert_eq!((ti2.state, ti2.misses, ti2.failed.clone(), ti2.due), (State::Queued, 2, vec!["peer-b".to_string()], T0 + 300), "misses, failed and deadline kept");
        assert_eq!(ti2.why, before["ti-2"].why, "still saying why");
        let asked = (intake.started.lock().unwrap().len(), slskd.asked.lock().unwrap().len());
        for at in [120, 200, 299] {
            clock.to(at).await;
            let items = view().items;
            assert_eq!(next_of(&items, T0 + at), None, "nothing taken up at t0+{at}");
            assert!(wake_in(&items, T0 + at).is_some(), "the worker waits on at t0+{at}");
        }
        assert_eq!((intake.started.lock().unwrap().len(), slskd.asked.lock().unwrap().len()), asked, "nothing asked before t0+300, not even a release");

        // At t0+300 both are searched, slskd asked then.
        clock.to(300).await;
        for id in ["ti-1", "ti-2"] {
            assert_eq!(next_of(&view().items, T0 + 300), Some(Next::Search(id.into())));
            search_with(id, &intake).await;
        }
        let after = stored(&d);
        assert_eq!((after["ti-1"].state, after["ti-2"].state, after["ti-2"].failed.clone()), (State::Found, State::Found, vec!["peer-b".to_string()]));
        assert!(slskd.searches_asked().iter().skip(2).all(|(_, at)| *at >= T0 + 300), "{:?}", slskd.searches_asked());
        assert!(after.values().all(|i| !matches!(i.state, State::Failed | State::Refused)), "nothing failed by the restart");
        assert!(slskd.violations.lock().unwrap().is_empty(), "{:?}", slskd.violations.lock().unwrap());
        done(&d);
    }

    /// TN.16, the bulk queue's half: 0.15's `track-intake.json` reads after the turns as it did (each item's state, misses, due,
    /// failed, set_aside and why), a round leaves it be (its track refused: finished, nothing searched or dropped), and the store
    /// written again holds every field it had, as it had it, and none for a pause (the deadline is `due`; `paused_until` a view).
    #[tokio::test]
    async fn the_queues_read_back_unchanged_after_the_turns() {
        let _one = crate::app::switches::TEST_STATE.lock().await;
        let d = fresh("015");
        let raw = include_str!("../../../../tests/fixtures/0.15/track-intake.json");
        std::fs::write(d.join("track-intake.json"), raw).unwrap();
        start_in(&d);
        let fixture: Value = serde_json::from_str(raw).unwrap();
        let had = fixture["items"].as_array().unwrap();
        let read = view().items;
        assert_eq!(read.len(), had.len(), "a 0.15 file read whole");
        let len = |v: &Value| v.as_array().map_or(0, Vec::len);
        for (i, f) in read.iter().zip(had) {
            let state: State = serde_json::from_value(f["state"].clone()).unwrap();
            let was = (f["id"].as_str().unwrap(), state, f["misses"].as_u64().unwrap_or(0) as u32, f["due"].as_i64().unwrap_or(0), len(&f["failed"]), len(&f["set_aside"]), f["why"].as_str());
            assert_eq!((i.id.as_str(), i.state, i.misses, i.due, i.failed.len(), i.set_aside.len(), i.why.as_deref()), was);
        }
        let now = now();
        assert_eq!((next_of(&read, now), wake_in(&read, now)), (None, None), "a round leaves it be");
        queue(|_| ());
        let written: Value = serde_json::from_slice(&std::fs::read(d.join("track-intake.json")).unwrap()).unwrap();
        assert_eq!(written["next"], fixture["next"]);
        const FIELDS: [&str; 15] = ["id", "album", "disc", "position", "title", "recording", "state", "candidates", "chosen", "why", "undo", "failed", "misses", "due", "set_aside"];
        for (w, f) in written["items"].as_array().unwrap().iter().zip(had) {
            for (k, v) in f.as_object().unwrap() {
                assert_eq!(&w[k], v, "{k} as it was");
            }
            let other: Vec<&String> = w.as_object().unwrap().keys().filter(|k| !FIELDS.contains(&k.as_str())).collect();
            assert!(other.is_empty(), "no field for the pause: {other:?}");
        }
        done(&d);
    }
}
