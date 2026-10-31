//! Bulk intake of missing tracks (0.13, the owner, 2026-10-31: "Bulk search, you approve"; decision 0080's amendment):
//! the albums named have each missing track searched in turn, the best file preselected; the administrator approves
//! many at once in one list; each approved file is fetched, gated and added in turn. A queue over single-track intake
//! (`track_fetch`): the same search, the same gate (decoded whole, the recording's length, AcoustID), the same Add to
//! album and Undo. Nothing goes in without a tick, so March 2026's unattended batches cannot happen again; one slskd
//! search at a time, paced.
//!
//! Kept in memory, as single-track intake's searches and fetches are (a restart forgets the queue).

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    /// Whether nothing more happens to it.
    fn finished(self) -> bool {
        matches!(self, State::None | State::Failed | State::Added | State::Refused)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumRef {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: String,
    pub album: AlbumRef,
    pub disc: u32,
    pub position: u32,
    pub title: String,
    pub recording: String,
    pub state: State,
    pub candidates: Vec<Candidate>,
    /// The candidate preselected (the best), or the one approved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Added: its undo token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
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

#[derive(Default)]
struct Queue {
    running: bool,
    next: u64,
    items: Vec<Item>,
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);

fn queue<T>(f: impl FnOnce(&mut Queue) -> T) -> T {
    f(QUEUE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Queue::default))
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

fn next_of(items: &[Item]) -> Option<Next> {
    if let Some(i) = items.iter().find(|i| i.state == State::Approved) {
        return Some(Next::Fetch(i.id.clone()));
    }
    items.iter().find(|i| i.state == State::Queued).map(|i| Next::Search(i.id.clone()))
}

/// The albums' missing tracks (those of their release no file holds) queued, each once; the queue as it is now.
pub async fn add_albums(albums: &[String]) -> Result<View> {
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
pub fn approve(picked: &[(String, Option<usize>)]) -> View {
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

/// The worker started, unless it runs.
fn run() {
    let start = queue(|q| !std::mem::replace(&mut q.running, true));
    if !start {
        return;
    }
    tokio::spawn(async {
        loop {
            let next = queue(|q| {
                let n = next_of(&q.items);
                if n.is_none() {
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
        match seen {
            Some(v) if v.state == Searching::Failed => (i.state, i.why) = (State::Failed, v.why),
            Some(v) if !v.candidates.is_empty() => {
                i.candidates = v.candidates.into_iter().take(SHOWN).collect();
                (i.state, i.chosen) = (State::Found, i.candidates.first().map(|c| c.id));
            }
            Some(_) => i.state = State::None,
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
        Err(e) => return with_item(id, |i| (i.state, i.why) = (State::Refused, Some(format!("{e:#}")))),
    };
    with_item(id, |i| i.fetch = Some(fid.clone()));
    let started = std::time::Instant::now();
    let done = loop {
        match super::track_fetch::fetch_view(&fid) {
            Some(v) if matches!(v.state, Fetching::Passed | Fetching::Refused) => break Some(v),
            Some(v) if started.elapsed() < FETCH_AT_MOST => {
                if v.state == Fetching::Checking {
                    with_item(id, |i| i.state = State::Checking);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            _ => break None,
        }
    };
    match done {
        Some(v) if v.state == Fetching::Passed => match super::track_fetch::add(&fid).await {
            Ok(undo) => with_item(id, |i| (i.state, i.undo, i.fetch) = (State::Added, Some(undo), None)),
            Err(e) => with_item(id, |i| (i.state, i.why) = (State::Refused, Some(format!("{e:#}")))),
        },
        Some(v) => with_item(id, |i| (i.state, i.why) = (State::Refused, v.why)),
        None => with_item(id, |i| (i.state, i.why) = (State::Refused, Some("The fetch did not finish in time.".into()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            search: None,
            fetch: None,
        }
    }

    #[test]
    fn an_approved_file_goes_before_the_next_search_and_finished_ones_wait() {
        let items = [item("a", State::Added), item("b", State::Queued), item("c", State::Approved), item("d", State::Queued)];
        assert_eq!(next_of(&items), Some(Next::Fetch("c".into())), "what the administrator approved first");
        assert_eq!(next_of(&items[..2]), Some(Next::Search("b".into())));
        assert_eq!(next_of(&[item("a", State::Found), item("b", State::Refused)]), None, "found waits for a tick: nothing goes in without one");
        assert!(State::None.finished() && State::Added.finished() && !State::Found.finished());
    }
}
