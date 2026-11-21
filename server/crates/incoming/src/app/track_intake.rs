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
//!
//! From 0.18 (RN's DI01, DI05) the queue is kept on rows in the journal's file (`bulk` items, each with its steps: search, fetch,
//! check), each item's row written as it changes (never the whole queue, never a percent), its next time and its tries in its
//! step's columns alone (R5). A restart keeps candidates and ticks, follows a fetch under way again by its batch, and sends back
//! only a search cut off. A passed file's Add and its row's Added are one write with its run (R2). 0.17's `track-intake.json` is
//! read once, by 0.18's step (`step_018`), never again.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use super::incoming_host::IncomingHost;
use super::soulseek::{CHECK_AGAIN, SLSKD_AGAIN};
use super::track_fetch::{Fetching, Searching};
use super::turn::{Failure, class};
use crate::intake::{ItemRow, StepRow};
use crate::journal::Journal;
use crate::slskd::Candidate;

/// Between two searches: gentle with Soulseek.
const PACE: Duration = Duration::from_secs(3);
/// A search is followed this long at most, a fetch this long (slskd's queues can be slow).
const SEARCH_AT_MOST: Duration = Duration::from_secs(120);
const FETCH_AT_MOST: Duration = Duration::from_secs(45 * 60);
/// Candidates an item shows, best first.
const SHOWN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
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
    /// Who asked (decision 0051): the administrator whose route queued it; none where 0.17 kept none.
    #[serde(skip)]
    pub(crate) asked_by: Option<String>,
    /// Held for its source (TN.1): the class of what it met, `paused` or `away`, while it holds.
    #[serde(skip)]
    pub(crate) failure: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct View {
    pub running: bool,
    pub items: Vec<Item>,
}

#[derive(Default, Serialize, serde::Deserialize)]
pub(crate) struct Queue {
    #[serde(skip)]
    running: bool,
    pub(crate) next: u64,
    pub(crate) items: Vec<Item>,
    /// Each item's rows as last written: an item is written again only where they differ.
    #[serde(skip)]
    kept: std::collections::HashMap<String, (ItemRow, Vec<StepRow>)>,
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);

/// The bulk queue's items on rows (DI01).
pub const KIND: &str = "bulk";

#[cfg(test)]
thread_local! {
    // A test's own journal (`tests::start_in`): the queue read from and kept in it on the test's thread.
    static AT: std::cell::RefCell<Option<std::sync::Arc<Journal>>> = const { std::cell::RefCell::new(None) };
}

/// Where the queue is kept: the server's journal (none before the server is open).
fn journal() -> Option<std::sync::Arc<Journal>> {
    #[cfg(test)]
    if let Some(at) = AT.with(|a| a.borrow().clone()) {
        return Some(at);
    }
    crate::journal::instance()
}

/// What a bulk item keeps beside its state and its steps.
#[derive(Serialize, serde::Deserialize)]
struct Fields {
    album: AlbumRef,
    disc: u32,
    position: u32,
    title: String,
    recording: String,
    #[serde(default)]
    candidates: Vec<Candidate>,
    /// Each candidate's file as offered, in its order.
    #[serde(default)]
    offered: Vec<crate::slskd::Offered>,
    chosen: Option<usize>,
    why: Option<String>,
    undo: Option<String>,
    #[serde(default)]
    failed: Vec<String>,
    #[serde(default)]
    set_aside: Vec<String>,
    search: Option<String>,
    fetch: Option<String>,
}

/// The step an item is at, by its state: the one its next time is kept on.
fn step_at(s: State) -> &'static str {
    match s {
        State::Queued | State::Searching | State::Found | State::None | State::Failed => "search",
        State::Approved | State::Fetching => "fetch",
        State::Checking | State::Added | State::Refused => "check",
    }
}

/// A step's state, by the item's.
fn step_state(step: &str, s: State) -> &'static str {
    let order = |n: &str| ["search", "fetch", "check"].iter().position(|x| *x == n).unwrap_or(0);
    let (at, me) = (order(step_at(s)), order(step));
    match s {
        _ if me < at => "done",
        State::Searching | State::Fetching | State::Checking if me == at => "running",
        State::Found if me == at => "done",
        State::Failed | State::Refused if me == at => "failed",
        State::Added => "done",
        _ => "to_do",
    }
}

/// An item as its rows: its state and who asked, its own fields, and its steps up to the one it is at, that one holding its next
/// time; the search its misses (R5: one column each).
pub(crate) fn rows_of(i: &Item, seq: i64) -> (ItemRow, Vec<StepRow>) {
    let fields = Fields {
        album: i.album.clone(),
        disc: i.disc,
        position: i.position,
        title: i.title.clone(),
        recording: i.recording.clone(),
        candidates: i.candidates.clone(),
        offered: i.candidates.iter().map(|c| c.offered.clone()).collect(),
        chosen: i.chosen,
        why: i.why.clone(),
        undo: i.undo.clone(),
        failed: i.failed.clone(),
        set_aside: i.set_aside.clone(),
        search: i.search.clone(),
        fetch: i.fetch.clone(),
    };
    let at = step_at(i.state);
    let steps = ["search", "fetch", "check"]
        .into_iter()
        .take_while(|s| *s != at)
        .chain([at])
        .map(|step| {
            let here = step == at;
            // Held for its source (TN.7): at its step, its class kept with it, no try counted.
            let held = here && i.due > 0 && i.why.is_some() && i.failure.is_some();
            StepRow {
                step: step.into(),
                state: if held { "held".into() } else { step_state(step, i.state).into() },
                // A search's misses; a fetch's users failed; a check's files set aside: a hold is none of them.
                tries: match step {
                    "search" => i.misses,
                    "fetch" => i.failed.len() as u32,
                    _ => i.set_aside.len() as u32,
                },
                failure: if held { i.failure.clone() } else { None },
                why: None,
                next_at: if here { i.due } else { 0 },
                ..Default::default()
            }
        })
        .collect();
    let item = ItemRow {
        id: i.id.clone(),
        kind: KIND.into(),
        seq,
        state: serde_json::to_value(i.state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
        asked_by: i.asked_by.clone(),
        fields: serde_json::to_value(fields).unwrap_or_default(),
    };
    (item, steps)
}

/// An item read from its rows. None where its fields cannot be read.
pub(crate) fn item_of(row: &ItemRow, steps: &[StepRow]) -> Option<Item> {
    let f: Fields = serde_json::from_value(row.fields.clone()).ok()?;
    let state: State = serde_json::from_value(serde_json::Value::String(row.state.clone())).ok()?;
    let at = steps.iter().find(|s| s.step == step_at(state));
    let candidates = f.candidates.into_iter().zip(f.offered.into_iter().chain(std::iter::repeat_with(Default::default))).map(|(c, offered)| Candidate { offered, ..c }).collect();
    Some(Item {
        id: row.id.clone(),
        album: f.album,
        disc: f.disc,
        position: f.position,
        title: f.title,
        recording: f.recording,
        state,
        candidates,
        chosen: f.chosen,
        why: f.why,
        undo: f.undo,
        failed: f.failed,
        misses: steps.iter().find(|s| s.step == "search").map_or(0, |s| s.tries),
        due: at.map_or(0, |s| s.next_at),
        set_aside: f.set_aside,
        search: f.search,
        fetch: f.fetch,
        asked_by: row.asked_by.clone(),
        failure: at.and_then(|s| s.failure.clone()),
    })
}

/// What writes an item's rows as Added in its run's own transaction (R2), given the run's Undo; and where those rows are left once
/// written (none: the transaction was not kept).
type Written = std::sync::Arc<Mutex<Option<(ItemRow, Vec<StepRow>)>>>;

fn added_also(item: Item) -> (crate::intake::Also, Written) {
    let rows: Written = std::sync::Arc::new(Mutex::new(None));
    let r = rows.clone();
    let also: crate::intake::Also = Box::new(move |tx, undo| {
        let done = Item { state: State::Added, undo: Some(undo.to_string()), fetch: None, ..item };
        let written = rows_of(&done, seq_of(&done.id));
        tx.keep_items(KIND, None, std::slice::from_ref(&written), &[])?;
        *r.lock().unwrap_or_else(|e| e.into_inner()) = Some(written);
        Ok(())
    });
    (also, rows)
}

/// The id's number: an item's place in the queue.
pub(crate) fn seq_of(id: &str) -> i64 {
    id.rsplit('-').next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// The queue as a start reads it from its rows (DI05): candidates and ticks kept, a fetch under way followed again by its batch; only
/// a search cut off goes back to Queued (its slskd search went with the stop), at its time where it was held (TN.9).
fn load(journal: Option<&Journal>) -> Queue {
    let mut q = Queue::default();
    let Some(j) = journal else { return q };
    match j.intake(KIND) {
        Ok((next, items)) => {
            q.next = next;
            for (row, steps) in items {
                match item_of(&row, &steps) {
                    Some(mut i) => {
                        if i.state == State::Searching {
                            i.state = State::Queued;
                        }
                        q.kept.insert(i.id.clone(), (row, steps));
                        q.items.push(i);
                    }
                    None => tracing::warn!(target: "library", item = %row.id, "a bulk item's row cannot be read: left as it is"),
                }
            }
        }
        Err(e) => tracing::warn!(target: "library", "the track intake's queue cannot be read: {e:#}"),
    }
    q
}

/// The items whose rows differ from those last written, and those gone: written in one transaction (each item's row as it changes,
/// never the whole queue). On a failure nothing is taken as written, so the next change writes it again.
fn write(q: &mut Queue) {
    let Some(j) = journal() else { return };
    let rows: Vec<(ItemRow, Vec<StepRow>)> = q.items.iter().map(|i| rows_of(i, seq_of(&i.id))).collect();
    let changed: Vec<(ItemRow, Vec<StepRow>)> = rows.iter().filter(|r| q.kept.get(&r.0.id) != Some(*r)).cloned().collect();
    let gone: Vec<String> = q.kept.keys().filter(|id| !q.items.iter().any(|i| &i.id == *id)).cloned().collect();
    let next_kept = j.mark(&format!("next:{KIND}")).ok().flatten().and_then(|n| n.parse::<u64>().ok());
    if changed.is_empty() && gone.is_empty() && next_kept == Some(q.next) {
        return;
    }
    match j.write(|tx| tx.keep_items(KIND, Some(q.next), &changed, &gone)) {
        Ok(()) => {
            for id in &gone {
                q.kept.remove(id);
            }
            for r in changed {
                q.kept.insert(r.0.id.clone(), r);
            }
        }
        Err(e) => tracing::warn!(target: "library", "the track intake's queue could not be kept: {e:#}"),
    }
}

fn queue<T>(f: impl FnOnce(&mut Queue) -> T) -> T {
    // Its jobs are the runner's, read from the queue when Activity is (`activity`).
    static PROVIDED: std::sync::Once = std::sync::Once::new();
    PROVIDED.call_once(|| crate::runner::runner().provide(KIND, std::sync::Arc::new(activity)));
    let mut g = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let q = g.get_or_insert_with(|| load(journal().as_deref()));
    let out = f(q);
    write(q);
    out
}

/// An item's rows as one write made them (its Add with its run, R2): taken as written, nothing written again.
fn written(id: &str, rows: (ItemRow, Vec<StepRow>), f: impl FnOnce(&mut Item)) {
    let mut g = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let q = g.get_or_insert_with(|| load(journal().as_deref()));
    if let Some(i) = q.items.iter_mut().find(|i| i.id == id) {
        f(i);
    }
    q.kept.insert(id.to_string(), rows);
    write(q);
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

/// The bulk queue's work as the runner's jobs (Health › Activity, RN's DS6 and AC8): each item searching, fetching or checking, or
/// waiting for its time or its source, read from the same fields its rows hold (its step's next time and tries), so the item, its
/// rows and Activity say one thing. One found waits for a tick and one finished is no job.
pub fn activity() -> Vec<crate::runner::Row> {
    let now = now();
    queue(|q| q.items.iter().filter_map(|i| job_of(i, now)).collect())
}

/// An item as a job, where it is one.
fn job_of(i: &Item, now: i64) -> Option<crate::runner::Row> {
    use crate::runner::{State as Job, Wait};
    let step = step_at(i.state);
    let held = i.due > now && i.why.is_some() && i.failure.is_some();
    let state = match i.state {
        State::Found | State::Failed | State::Added | State::Refused => return None,
        _ if held => Job::Waiting,
        State::Searching | State::Fetching | State::Checking => Job::Running,
        _ if i.due > now => Job::Scheduled,
        _ => Job::Waiting,
    };
    let tries = match step {
        "search" => i.misses,
        "fetch" => i.failed.len() as u32,
        _ => i.set_aside.len() as u32,
    };
    let what = match i.state {
        State::Searching => "Searching",
        State::Fetching => "Fetching",
        State::Checking => "Checking",
        State::Approved => "Waits to be fetched",
        _ => "Waits to be searched",
    };
    Some(crate::runner::Row {
        id: format!("intake:{}", i.id),
        kind: format!("intake:{step}"),
        state,
        what: what.into(),
        detail: Some(format!("{} – {} · {}. {}", i.album.artist, i.album.title, i.position, i.title)),
        done: None,
        of: None,
        unit: None,
        waits: held.then(|| Wait { what: i.why.clone().unwrap_or_default(), until: Some(i.due) }),
        at: (i.due > 0).then_some(i.due),
        tries: Some(tries),
        whose: i.asked_by.clone(),
        started: 0,
        finished: None,
    })
}

/// What the worker does next: an approved file first (what the administrator is waiting for), or one a restart found fetching
/// (followed again by its batch), else the next search; neither before its time where it holds for its source (TN.7).
#[derive(Debug, PartialEq)]
enum Next {
    Fetch(String),
    Search(String),
}

fn next_of(items: &[Item], now: i64) -> Option<Next> {
    let due = |i: &&Item| i.due <= now;
    // Approved, or held at its check: its tick taken up.
    if let Some(i) = items.iter().filter(due).find(|i| matches!(i.state, State::Approved | State::Checking | State::Fetching)) {
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
        .filter(|i| matches!(i.state, State::None | State::Queued | State::Approved | State::Checking | State::Fetching))
        .map(|i| (i.due - now).clamp(1, 60))
        .min()
        .map(|secs| Duration::from_secs(secs as u64))
}

/// At a start (DI05): the queue read from its rows and its worker started where anything waits (a fetch under way followed again by
/// its batch, a tick taken up, a search at its time).
pub fn resume<H: IncomingHost>(app: &std::sync::Arc<H>) {
    let host: std::sync::Arc<dyn IncomingHost> = app.clone();
    let _ = APP.set(host);
    if queue(|q| q.items.iter().any(|i| !i.state.finished() && i.state != State::Found)) {
        run();
    }
}

/// A track searched now, whatever its wait.
pub fn search_now<H: IncomingHost>(app: &std::sync::Arc<H>, id: &str) -> View {
    search_now_on(app.clone(), id)
}

/// `search_now`, on its host.
fn search_now_on(app: std::sync::Arc<dyn IncomingHost>, id: &str) -> View {
    let _ = APP.set(app);
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
#[cfg(test)]
fn on_no_verdict(i: &mut Item, step: State, why: String, due: i64) {
    on_hold(i, step, why, due, "away");
}

/// `on_no_verdict`, its class (`paused` or `away`) kept with its step (RN's DQ2).
fn on_hold(i: &mut Item, step: State, why: String, due: i64, class: &str) {
    i.failure = Some(class.to_string());
    (i.state, i.why, i.due) = (step, Some(why), due);
    if step != State::Checking {
        i.fetch = None;
    }
}

/// A hold's class as its step keeps it.
fn class_word(f: &Failure) -> &'static str {
    match f {
        Failure::Paused { .. } => "paused",
        _ => "away",
    }
}

/// A search that met a failure of class `f`: held Queued where its source could not answer (slskd or MusicBrainz away, or paused),
/// no try counted (TN.7, TN.13); failed with why otherwise, as before.
fn search_failed(i: &mut Item, f: &Failure, why: Option<String>) {
    match held_for(f, now(), SLSKD_AGAIN) {
        Some(due) => on_hold(i, State::Queued, why.unwrap_or_default(), due, class_word(f)),
        None => (i.state, i.why) = (State::Failed, why),
    }
}

/// The albums' missing tracks (those of their release no file holds) queued, each once; the queue as it is now.
pub async fn add_albums<H: IncomingHost>(app: &std::sync::Arc<H>, albums: &[String]) -> Result<View> {
    add_albums_on(app.clone(), albums).await
}

/// `add_albums`, on its host.
async fn add_albums_on(app: std::sync::Arc<dyn IncomingHost>, albums: &[String]) -> Result<View> {
    let _ = APP.set(app);
    let own = crate::source::cratarr::KIND.id;
    for album in albums {
        let release = super::track_fetch::release(album).await?;
        let asked_by = crate::serve::who::current().map(|w| w.user);
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
                    asked_by: asked_by.clone(),
                    failure: None,
                });
            }
        });
    }
    run();
    Ok(view())
}

/// These found items approved (with the candidate given, else the one preselected); the queue as it is now.
pub fn approve<H: IncomingHost>(app: &std::sync::Arc<H>, picked: &[(String, Option<usize>)]) -> View {
    approve_on(app.clone(), picked)
}

/// `approve`, on its host.
fn approve_on(app: std::sync::Arc<dyn IncomingHost>, picked: &[(String, Option<usize>)]) -> View {
    let _ = APP.set(app);
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
static APP: std::sync::OnceLock<std::sync::Arc<dyn IncomingHost>> = std::sync::OnceLock::new();

/// What the bulk queue asks of single-track intake (`track_fetch`) that needs the running server: a track's search started, a
/// candidate fetched, a check that had no verdict asked again, a file set aside, a passed file added. `Live` is the server's; the
/// tests' stands in, with fakes of slskd and AcoustID behind it.
trait Intake: Sync {
    fn search(&self, album: String, recording: String) -> impl Future<Output = Result<String>> + Send;
    fn fetch(&self, search: String, candidate: usize) -> impl Future<Output = Result<String>> + Send;
    /// A fetch whose check had no verdict checked again on its staged file: whether there was one.
    fn check_again(&self, fetch: &str) -> bool;
    fn set_aside(&self, fetch: &str, why: &str) -> Option<String>;
    /// The file added to its album: its run's Undo. `also` writes in the run's own transaction, given its Undo (R2).
    fn add(&self, fetch: String, also: crate::intake::Also) -> impl Future<Output = Result<Option<String>>> + Send;
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

    fn add(&self, fetch: String, also: crate::intake::Also) -> impl Future<Output = Result<Option<String>>> + Send {
        async move { Ok(APP.get().cloned().ok_or_else(|| anyhow::anyhow!("Cratarr is starting"))?.add_fetched(fetch, Some(also)).await?.undo) }
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
                i.failure = None;
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
    let Some((sid, candidate, held, under_way)) = queue(|q| {
        q.items.iter().find(|i| i.id == id).and_then(|i| {
            let under_way = i.fetch.clone().filter(|f| i.state == State::Fetching && super::track_fetch::fetch_view(f).is_some());
            Some((i.search.clone()?, i.chosen?, i.fetch.clone().filter(|_| i.state == State::Checking), under_way))
        })
    }) else {
        return with_item(id, |i| (i.state, i.why) = (State::Failed, Some("Its search is gone; queue it again.".into())));
    };
    // Held at its check (TN.7): its staged file gated again, nothing fetched again. One gone meanwhile is fetched again. One a restart
    // found fetching is followed by its batch (DQ3): no new POST.
    let again = held.as_deref().is_some_and(|f| intake.check_again(f));
    let fid = match held {
        _ if under_way.is_some() => under_way.unwrap_or_default(),
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
                    let f = class(&e);
                    return with_item(id, |i| match held_for(&f, now(), SLSKD_AGAIN) {
                        // slskd away or paused: nothing known of the user. Held at its tick, the same candidate taken up again later.
                        Some(due) => on_hold(i, State::Approved, why, due, class_word(&f)),
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
            let word = v.failure.as_ref().map_or("away", class_word);
            if staged {
                return with_item(id, |i| on_hold(i, State::Checking, why, due, word));
            }
            super::track_fetch::drop_fetch(&fid);
            return with_item(id, |i| on_hold(i, State::Approved, why, due, word));
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
        Some(v) if v.state == Fetching::Passed => {
            // The item as it is once added, its rows written in its run's own transaction (R2): both, or neither.
            let Some(added) = queue(|q| q.items.iter().find(|i| i.id == id).cloned()) else { return };
            let (also, rows) = added_also(added);
            match intake.add(fid.clone(), also).await {
                Ok(undo) => match rows.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    Some(r) => written(id, r, |i| (i.state, i.undo, i.fetch) = (State::Added, undo, None)),
                    None => with_item(id, |i| (i.state, i.undo, i.fetch) = (State::Added, undo, None)),
                },
                Err(e) => with_item(id, |i| (i.state, i.why) = (State::Refused, Some(format!("{e:#}")))),
            }
        }
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
            asked_by: None,
            failure: None,
        }
    }

    /// What waits finds its album by its id when its turn comes.
    #[test]
    fn a_track_waiting_keeps_its_album_after_the_move() {
        let d = std::env::temp_dir().join(format!("cratarr-intake-moved-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let q = Queue { running: false, next: 2, items: vec![item("a", State::Queued)], ..Default::default() };
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

    use anyhow::Context as _;
    use serde_json::{Value, json};

    use crate::app::track_fetch::ReleaseTrack;
    pub(crate) use crate::app::track_intake_base::testing::{Clock, FakeAcoustid, FakeSlskd, T0, slskd_at, tone};
    use crate::app::turn::{ACOUSTID, Turn};

    /// A folder of the test's own, empty.
    fn fresh(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-intake-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The server started on `dir`: the queue kept in its journal's file (`journal.sqlite`, on this thread), read anew from its rows.
    pub(crate) fn start_in(dir: &Path) {
        let journal = Journal::open(Some(&dir.join("journal.sqlite"))).unwrap();
        AT.with(|a| *a.borrow_mut() = Some(Arc::new(journal)));
        stop();
    }

    /// The journal the queue is kept in on this thread.
    pub(crate) fn kept_in() -> Arc<Journal> {
        AT.with(|a| a.borrow().clone()).expect("started")
    }

    /// The server stopped: the queue's memory gone, its store left on disk.
    pub(crate) fn stop() {
        *QUEUE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The test done: the queue kept nowhere.
    pub(crate) fn done(dir: &Path) {
        stop();
        AT.with(|a| *a.borrow_mut() = None);
        std::fs::remove_dir_all(dir).ok();
    }

    /// The queue as its rows hold it (read through a connection of its own), its items by id.
    pub(crate) fn stored(dir: &Path) -> HashMap<String, Item> {
        let j = Journal::open(Some(&dir.join("journal.sqlite"))).unwrap();
        j.intake(KIND).unwrap().1.iter().map(|(r, s)| item_of(r, s).unwrap()).map(|i| (i.id.clone(), i)).collect()
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

        fn add(&self, fetch: String, also: crate::intake::Also) -> impl Future<Output = Result<Option<String>>> + Send {
            self.added.lock().unwrap().push(fetch);
            // Its Added written in the stand-in run's own transaction, as the server's add writes it (R2).
            let kept = match AT.with(|a| a.borrow().clone()) {
                Some(j) => j.write(|tx| also(tx, "added-run")),
                None => Ok(()),
            };
            std::future::ready(kept.map(|()| Some("added-run".to_string())))
        }
    }

    /// Tests elsewhere (TN.5): the bulk queue started on `dir` (`start_in`) with these items on its rows, nothing run.
    pub(crate) fn start_with(dir: &Path, items: Vec<Item>) {
        let next = items.len() as u64;
        start_with_next(dir, items, next);
    }

    /// `start_with`, its next id as given.
    pub(crate) fn start_with_next(dir: &Path, items: Vec<Item>, next: u64) {
        let j = Journal::open(Some(&dir.join("journal.sqlite"))).unwrap();
        let rows: Vec<_> = items.iter().map(|i| rows_of(i, seq_of(&i.id))).collect();
        j.write(|tx| tx.keep_items(KIND, Some(next), &rows, &[])).unwrap();
        drop(j);
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
        let _one = crate::app::switches::TEST_STATE.write().await;
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
        start_with(&d, vec![waiting("ti-1", &tracks[0]), Item { misses: 2, ..waiting("ti-2", &tracks[1]) }, waiting("ti-3", &tracks[2])]);
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
    /// t0+120: ti-2 reads back Queued with its misses and `failed`, ti-1 at its tick with its candidate (changed by RN's DQ4: a restart
    /// keeps ticks; 0.17 sent it back to Queued), each with its time; nothing is asked before t0+300, not even a release; then ti-1 is
    /// the next to fetch and ti-2 is searched. Never a held item failed by the restart, nor its deadline lengthened.
    #[tokio::test(start_paused = true)]
    async fn a_restart_mid_pause_asks_nothing_early() {
        let _one = crate::app::switches::TEST_STATE.write().await;
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
        start_with(&d, vec![waiting("ti-1", &tracks[0]), Item { misses: 2, failed: vec!["peer-b".into()], ..waiting("ti-2", &tracks[1]) }]);
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
        assert_eq!((back[0].id.as_str(), back[0].state, back[0].chosen, back[0].due), ("ti-1", State::Approved, Some(0), T0 + 300), "at its tick, with its time");
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

        // At t0+300 ti-1's tick is taken up again (the next to fetch), and ti-2 is searched, slskd asked then.
        clock.to(300).await;
        assert_eq!(next_of(&view().items, T0 + 300), Some(Next::Fetch("ti-1".into())));
        queue(|q| q.items.retain(|i| i.id != "ti-1"));
        assert_eq!(next_of(&view().items, T0 + 300), Some(Next::Search("ti-2".into())));
        search_with("ti-2", &intake).await;
        let after = stored(&d);
        assert_eq!((after["ti-2"].state, after["ti-2"].failed.clone()), (State::Found, vec!["peer-b".to_string()]));
        assert!(slskd.searches_asked().iter().skip(2).all(|(_, at)| *at >= T0 + 300), "{:?}", slskd.searches_asked());
        assert!(after.values().all(|i| !matches!(i.state, State::Failed | State::Refused)), "nothing failed by the restart");
        assert!(slskd.violations.lock().unwrap().is_empty(), "{:?}", slskd.violations.lock().unwrap());
        done(&d);
    }

    /// TN.16, the bulk queue's half: 0.15's `track-intake.json` reads after the turns as it did (each item's state, misses, due,
    /// failed, set_aside and why), a round leaves it be (its track refused: finished, nothing searched or dropped), and the store
    /// written again holds every field it had, as it had it, and none for a pause (the deadline is `due`; `paused_until` a view).
    /// Moved onto the rows with its assertions (RN): the file read as 0.18's step reads it, its items on rows, read back from them.
    #[tokio::test]
    async fn the_queues_read_back_unchanged_after_the_turns() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("015");
        let raw = include_str!("../../../../tests/fixtures/0.15/track-intake.json");
        let given: Queue = serde_json::from_str(raw).unwrap();
        start_with_next(&d, given.items, given.next);
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
        let back = stored(&d);
        let written: Vec<Value> = had.iter().map(|f| serde_json::to_value(&back[f["id"].as_str().unwrap()]).unwrap()).collect();
        assert_eq!(kept_in().intake(KIND).unwrap().0, fixture["next"].as_u64().unwrap());
        const FIELDS: [&str; 15] = ["id", "album", "disc", "position", "title", "recording", "state", "candidates", "chosen", "why", "undo", "failed", "misses", "due", "set_aside"];
        for (w, f) in written.iter().zip(had) {
            for (k, v) in f.as_object().unwrap() {
                assert_eq!(&w[k], v, "{k} as it was");
            }
            let other: Vec<&String> = w.as_object().unwrap().keys().filter(|k| !FIELDS.contains(&k.as_str())).collect();
            assert!(other.is_empty(), "no field for the pause: {other:?}");
        }
        for (row, steps) in kept_in().intake(KIND).unwrap().1 {
            assert!(!row.fields.as_object().unwrap().keys().any(|k| k.contains("pause")), "no field for the pause in its row");
            assert_eq!(steps.iter().filter(|s| s.next_at != 0).count(), usize::from(back[&row.id].due != 0), "its due in one step's column");
        }
        done(&d);
    }

    // ---- RN: the bulk queue on rows (SCENARIOS-RN DS4-DS7, DQ2, DQ4)

    /// A file run as the gate's add keeps one: `run:<id>`, written in `tx` with what `also` writes (R2).
    fn a_run(tx: &crate::intake::Tx, also: crate::intake::Also) -> Result<String> {
        let run = crate::journal::FileRun { what: "track".into(), title: "Missing tracks · Tide Tables".into(), records: Vec::new(), files: Vec::new(), failed: Vec::new(), planned: Vec::new() };
        let id = tx.record_run(now(), &run, Some("root"), 30 * 86_400)?;
        let undo = format!("run:{id}");
        also(tx, &undo)?;
        Ok(undo)
    }

    /// DS5 (kind 3): ti-3 passed its gate; its Add is a file run of one file into its record, and ti-3's Added row is written in that
    /// run's own transaction. (a) Cut inside it (a stop before its commit): no run and ti-3 not Added after the start, and its Add goes
    /// once at its next turn; (b) kept: one run, and ti-3 Added with that run's Undo, after the start too. Never an Added item whose Undo
    /// names no run, a run whose item is not Added, nor two runs for one item.
    #[tokio::test]
    async fn an_add_and_its_run_are_one_write() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("ds5");
        let t = tide_tables();
        start_with(&d, vec![Item { state: State::Checking, fetch: Some("f-3".into()), search: Some("s-3".into()), chosen: Some(0), ..waiting("ti-3", &t[2]) }]);
        let journal = kept_in();
        let runs = |j: &Journal| j.file_runs_since(0).unwrap().into_iter().map(|r| r.0).collect::<Vec<_>>();
        let item = || queue(|q| q.items.iter().find(|i| i.id == "ti-3").cloned()).unwrap();

        // (a) Cut inside the transaction that ends the run and marks ti-3 Added.
        let (also, rows) = added_also(item());
        crate::intake::cut_next_write();
        assert!(journal.write(|tx| a_run(tx, also)).is_err(), "cut off before its commit");
        assert!(rows.lock().unwrap().is_some(), "written inside, not kept");
        stop();
        let back = stored(&d);
        assert_eq!((back["ti-3"].state, back["ti-3"].undo.clone()), (State::Checking, None), "not Added");
        assert!(runs(&journal).is_empty(), "no run");
        // Its next turn: added once.
        let (also, rows) = added_also(item());
        let undo = journal.write(|tx| a_run(tx, also)).unwrap();
        written("ti-3", rows.lock().unwrap().take().unwrap(), |i| (i.state, i.undo, i.fetch) = (State::Added, Some(undo.clone()), None));

        // (b) Kept: after the start, one run and ti-3 Added with that run's Undo.
        stop();
        let back = stored(&d);
        let kept = runs(&journal);
        assert_eq!(kept.len(), 1, "one run for one item");
        assert_eq!((back["ti-3"].state, back["ti-3"].undo.clone()), (State::Added, Some(format!("run:{}", kept[0]))), "Added with its run's Undo");
        assert_eq!(view().items[0].state, State::Added, "read back so");
        done(&d);
    }

    /// DS6: ti-4's search held by MusicBrainz's 503 until t0+40 (no try counted), ti-5's third search found nothing (misses 3, due
    /// a day on). The rows, the queue and the runner's view of the same work say the same next time and tries; and the journal's file
    /// has one column each for a step's next time and its tries. Never two columns holding a step's next time, nor a hold counted as a
    /// try.
    #[tokio::test]
    async fn a_steps_time_and_tries_are_kept_once() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (_pinned, _clock) = Clock::at_t0();
        let d = fresh("ds6");
        let t = tide_tables();
        let mut ti4 = Item { misses: 1, ..waiting("ti-4", &t[0]) };
        on_hold(&mut ti4, State::Queued, "MusicBrainz asked Cratarr to wait".into(), T0 + 40, "paused");
        let ti5 = Item { state: State::None, misses: 3, due: T0 + super::super::soulseek::wait_after(3), ..waiting("ti-5", &t[1]) };
        start_with(&d, vec![ti4, ti5]);
        let rows: HashMap<String, Vec<crate::intake::StepRow>> = kept_in().intake(KIND).unwrap().1.into_iter().map(|(r, s)| (r.id, s)).collect();
        let search = |id: &str| rows[id].iter().find(|s| s.step == "search").cloned().unwrap();
        assert_eq!((search("ti-4").next_at, search("ti-4").tries, search("ti-4").failure.as_deref()), (T0 + 40, 1, Some("paused")), "held: its time, no try counted");
        assert_eq!((search("ti-5").next_at, search("ti-5").tries), (T0 + 86_400, 3));
        let jobs = activity();
        let job = |id: &str| jobs.iter().find(|r| r.id == format!("intake:{id}")).cloned().unwrap();
        assert_eq!((job("ti-4").at, job("ti-4").tries), (Some(T0 + 40), Some(1)), "the job reads the step");
        assert_eq!((job("ti-5").at, job("ti-5").tries), (Some(T0 + 86_400), Some(3)));
        let queued = view().items;
        assert_eq!(queued.iter().map(|i| (i.due, i.misses)).collect::<Vec<_>>(), [(T0 + 40, 1), (T0 + 86_400, 3)], "the queue reads the step");
        // The schema: a step's next time and its tries, one column each in the journal's file.
        let db = rusqlite::Connection::open(d.join("journal.sqlite")).unwrap();
        let mut timing = Vec::new();
        for table in ["intake_items", "intake_steps", "jobs", "source_pauses", "intake_marks"] {
            let cols: Vec<String> = db.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
            for c in cols.iter().filter(|c| ["next", "due", "tries", "misses", "attempt", "run_at", "retry"].iter().any(|w| c.contains(w))) {
                timing.push(format!("{table}.{c}"));
            }
        }
        assert_eq!(timing, ["intake_steps.tries", "intake_steps.next_at"], "one column each");
        let fields = kept_in().intake(KIND).unwrap().1.into_iter().flat_map(|(r, _)| r.fields.as_object().unwrap().keys().cloned().collect::<Vec<_>>()).collect::<Vec<_>>();
        assert!(!fields.iter().any(|k| k == "due" || k == "misses"), "none in an item's own fields: {fields:?}");
        done(&d);
    }

    /// DS7 (kind 4): the worker marking ti-6 Checking while root ticks ti-7, drops ti-8 and adds a track, all at once from four threads:
    /// each change is in the rows and none lost; no page's call meets "database is locked". A pin of 0.17's one lock, on the rows.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_worker_and_the_routes_write_at_once() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("ds7");
        let t = tide_tables();
        let candidates = vec![candidate(0, "peer-a"), candidate(1, "peer-b")];
        let found = |id: &str, n: usize| Item { state: State::Found, candidates: candidates.clone(), chosen: Some(0), search: Some(format!("s-{id}")), ..waiting(id, &t[n]) };
        start_with(&d, vec![Item { state: State::Fetching, fetch: Some("f-6".into()), ..found("ti-6", 0) }, found("ti-7", 1), found("ti-8", 2)]);
        let journal = kept_in();
        let first = t[0].clone();
        let at = |f: Box<dyn FnOnce() + Send>| {
            let j = journal.clone();
            std::thread::spawn(move || {
                AT.with(|a| *a.borrow_mut() = Some(j));
                for _ in 0..50 {
                    std::thread::yield_now();
                }
                f();
            })
        };
        let threads = [
            at(Box::new(|| with_item("ti-6", |i| i.state = State::Checking))),
            at(Box::new(|| queue(|q| tick(q, &[("ti-7".into(), Some(1))])))),
            at(Box::new(|| drop_item("ti-8"))),
            at(Box::new(move || {
                queue(|q| {
                    q.next += 1;
                    let id = format!("ti-{}", q.next + 6);
                    q.items.push(Item { id, ..waiting("ti-x", &first) });
                })
            })),
        ];
        for th in threads {
            th.join().expect("no page met a locked database");
        }
        let back = stored(&d);
        assert_eq!(back["ti-6"].state, State::Checking, "the worker's change kept");
        assert_eq!((back["ti-7"].state, back["ti-7"].chosen), (State::Approved, Some(1)), "the tick kept");
        assert!(!back.contains_key("ti-8"), "the dropped item gone");
        assert_eq!(back.len(), 3, "the added one kept: {:?}", back.keys().collect::<Vec<_>>());
        stop();
        assert_eq!(view().items.len(), 3, "read back as written");
        done(&d);
    }

    /// DQ2 (TN.7 on the rows): ti-11's check (one file set aside before) meets AcoustID's code 14 with Retry-After 120; ti-12's fetch
    /// meets slskd away. Each is held at its step: ti-11 at its check, its tries as before, its class paused, its next time AcoustID's
    /// deadline; ti-12 at its tick, away, its next time `SLSKD_AGAIN` on. Nobody failed, nothing set aside, no other candidate fetched.
    /// Never a try counted, nor a hold kept as a user's or a file's failure.
    #[tokio::test]
    async fn a_hold_keeps_its_step_and_counts_no_try() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (_pinned, _clock) = Clock::at_t0();
        let d = fresh("dq2");
        let t = tide_tables();
        let candidates = vec![candidate(0, "peer-a"), candidate(1, "peer-b")];
        let mut ti11 = Item { state: State::Checking, candidates: candidates.clone(), chosen: Some(0), fetch: Some("f-11".into()), set_aside: vec!["an earlier file".into()], ..waiting("ti-11", &t[0]) };
        let mut ti12 = Item { state: State::Approved, candidates, chosen: Some(0), ..waiting("ti-12", &t[1]) };
        on_hold(&mut ti11, State::Checking, "AcoustID asked Cratarr to wait".into(), T0 + 120, class_word(&Failure::Paused { until: T0 + 120 }));
        on_hold(&mut ti12, State::Approved, "slskd could not be reached".into(), T0 + SLSKD_AGAIN, class_word(&Failure::Away(String::new())));
        start_with(&d, vec![ti11, ti12]);
        let rows: HashMap<String, Vec<crate::intake::StepRow>> = kept_in().intake(KIND).unwrap().1.into_iter().map(|(r, s)| (r.id, s)).collect();
        let at = |id: &str, step: &str| rows[id].iter().find(|s| s.step == step).cloned().unwrap();
        let check = at("ti-11", "check");
        assert_eq!((check.state.as_str(), check.tries, check.failure.as_deref(), check.next_at), ("held", 1, Some("paused"), T0 + 120), "at its check, no try counted");
        let fetch = at("ti-12", "fetch");
        assert_eq!((fetch.state.as_str(), fetch.tries, fetch.failure.as_deref(), fetch.next_at), ("held", 0, Some("away"), T0 + SLSKD_AGAIN), "at its tick");
        let back = stored(&d);
        for i in back.values() {
            assert!(i.failed.is_empty() && !matches!(i.state, State::Failed | State::Refused), "{}: nobody failed", i.id);
            assert_eq!(i.chosen, Some(0), "{}: no other candidate", i.id);
        }
        assert_eq!(back["ti-11"].set_aside.len(), 1, "nothing more set aside");
        let paused: Vec<String> = kept_in().intake(KIND).unwrap().1.iter().flat_map(|(r, _)| r.fields.as_object().unwrap().keys().filter(|k| k.contains("until") || k.contains("pause")).cloned().collect::<Vec<_>>()).collect();
        assert!(paused.is_empty(), "no second deadline beside the step's: {paused:?}");
        done(&d);
    }

    /// DQ4: ti-14 Found with three candidates (peer-x's preselected), ti-15 approved with candidate 2. After a restart both read back
    /// with their candidates and choices: ti-15 is the next to fetch, from candidate 2; root ticks ti-14, which takes peer-x's. Never
    /// "Its search is gone; queue it again.", a Found item back to Queued, nor a failed user's candidate read back.
    #[tokio::test]
    async fn candidates_and_ticks_survive_a_restart() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("dq4");
        let t = tide_tables();
        let three = vec![candidate(0, "peer-x"), candidate(1, "peer-y"), candidate(2, "peer-z")];
        start_with(&d, vec![
            Item { state: State::Found, candidates: three.clone(), chosen: Some(0), search: Some("s-14".into()), failed: vec!["peer-w".into()], ..waiting("ti-14", &t[0]) },
            Item { state: State::Approved, candidates: three.clone(), chosen: Some(2), search: Some("s-15".into()), ..waiting("ti-15", &t[1]) },
        ]);
        // The server stops and starts.
        stop();
        let back = view().items;
        assert_eq!((back[0].state, back[0].chosen, back[0].candidates.len()), (State::Found, Some(0), 3), "Found, its candidates and choice");
        assert_eq!((back[1].state, back[1].chosen), (State::Approved, Some(2)), "its tick");
        assert!(back.iter().all(|i| i.candidates.iter().all(|c| c.user != "peer-w")), "no failed user's candidate");
        assert_eq!(back[0].candidates[0].offered.filename, three[0].offered.filename, "each candidate's file as offered");
        assert_eq!(next_of(&back, now()), Some(Next::Fetch("ti-15".into())), "ti-15 fetched from its tick");
        queue(|q| tick(q, &[("ti-14".into(), None)]));
        let ticked = view().items;
        assert_eq!((ticked[0].state, ticked[0].chosen), (State::Approved, Some(0)), "peer-x's");
        assert!(ticked.iter().all(|i| i.why.as_deref() != Some("Its search is gone; queue it again.")));
        done(&d);
    }

    /// A Soulseek user's copy of "Neap" as the scripted slskd answers a search.
    fn neap_answer(user: &str) -> Value {
        json!({ "username": user, "hasFreeUploadSlot": true, "queueLength": 0, "uploadSpeed": 100,
            "files": [{ "filename": "Glass Harbour\\Tide Tables\\02 Neap.flac", "size": 1000, "extension": "flac", "length": 10 }] })
    }

    /// DS4 (R7): ti-2 fetching from peer-z, the scripted slskd moving its percent from 2 to 100 over a hundred reads, then done; the
    /// gate passing. The journal's file changes by the item's state changes alone (fetching, checking, added, and its run), never per
    /// read; the percent reaches whoever listens (the fetch's live patch) and is read from memory. Never a write per percent, nor the
    /// whole queue written for one item's change.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_percent_writes_nothing_and_a_change_writes_its_row() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("ds4");
        let t = tide_tables();
        let slskd = crate::slskd::scripted::Scripted::new().answers(vec![neap_answer("peer-z")]).hold_all(1.0).writes_to(&d.join("slskd"), Some(&tone(&d)));
        let heard = Arc::new(FakeAcoustid::default());
        let intake = Through::new(&d, slskd.client(), &heard);
        start_with(&d, vec![waiting("ti-2", &t[1]), waiting("ti-9", &t[0])]);
        search_with("ti-2", &intake).await;
        queue(|q| tick(q, &[("ti-2".into(), Some(0))]));
        let j = kept_in();
        let first_ti9 = j.intake(KIND).unwrap().1.into_iter().find(|(r, _)| r.id == "ti-9").unwrap();
        let mut told = crate::app::track_fetch::subscribe();
        let (counts, ()) = tokio::join!(
            async {
                let batch = loop {
                    if let Some(b) = slskd.going().first().cloned() {
                        break b;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                };
                tokio::time::sleep(Duration::from_millis(100)).await;
                let at_start = j.total_changes();
                for p in 2..=100 {
                    slskd.at(&batch, p as f64);
                    tokio::time::sleep(Duration::from_millis(12)).await;
                }
                let at_end = j.total_changes();
                slskd.release(&batch);
                (at_start, at_end)
            },
            fetch_with("ti-2", &intake),
        );
        assert_eq!(counts.0, counts.1, "a hundred reads of its percent wrote nothing");
        let mut percents = Vec::new();
        loop {
            match told.try_recv() {
                Ok(t) => percents.push(t.view.percent),
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
        assert!(percents.iter().any(|p| *p > 50.0 && *p < 100.0), "the percent reached whoever listens: {percents:?}");
        let rows = j.intake(KIND).unwrap().1;
        assert_eq!(rows.iter().find(|(r, _)| r.id == "ti-2").map(|(r, _)| r.state.as_str()), Some("added"));
        assert!(rows.iter().all(|(r, _)| r.fields.get("percent").is_none()), "no percent kept");
        let ti9 = rows.into_iter().find(|(r, _)| r.id == "ti-9").unwrap();
        assert_eq!(ti9, first_ti9, "the other item's row untouched: never the whole queue written for one item");
        done(&d);
    }

    /// DQ3 (kind 3): ti-13 fetching from peer-z, the scripted slskd holding its batch at 40 %. The server restarts (the queue read
    /// anew from its rows); the fake then completes it. ti-13 reads back Fetching with its fetch, which is followed by its batch: no
    /// new POST; once done it is gated and added once. Never ti-13 back to Queued, a second batch of peer-z for it, nor its tick lost.
    /// (The fetch's own row followed again by its batch after a restart: track_fetch's `a_single_search_and_fetch_survive_a_restart`.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_fetch_under_way_is_followed_after_a_restart() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let d = fresh("dq3");
        let t = tide_tables();
        let slskd = crate::slskd::scripted::Scripted::new().answers(vec![neap_answer("peer-z")]).hold_all(40.0).writes_to(&d.join("slskd"), Some(&tone(&d)));
        let heard = Arc::new(FakeAcoustid::default());
        let intake = Through::new(&d, slskd.client(), &heard);
        start_with(&d, vec![waiting("ti-13", &t[1])]);
        search_with("ti-13", &intake).await;
        queue(|q| tick(q, &[("ti-13".into(), Some(0))]));
        // The fetch begun, then the stop while slskd holds it at 40 %.
        let _ = tokio::time::timeout(Duration::from_millis(1500), fetch_with("ti-13", &intake)).await;
        let posts = slskd.posts().len();
        assert_eq!(posts, 1);
        stop();
        let back = view().items;
        assert_eq!((back[0].state, back[0].chosen, back[0].fetch.is_some()), (State::Fetching, Some(0), true), "Fetching with its fetch and tick");
        assert_eq!(next_of(&back, now()), Some(Next::Fetch("ti-13".into())), "taken up by the worker");
        for b in slskd.going() {
            slskd.release(&b);
        }
        fetch_with("ti-13", &intake).await;
        let after = stored(&d);
        assert_eq!(after["ti-13"].state, State::Added, "gated and added");
        assert_eq!(*intake.added.lock().unwrap(), [back[0].fetch.clone().unwrap()], "added once, its own fetch");
        assert_eq!(slskd.posts().len(), posts, "no new POST: followed by its batch");
        done(&d);
    }

    /// AC8 (kind 6): a bulk item held for AcoustID until t0+300: its view (`/api/track-intake`), its rows and Activity say the same
    /// state, the same why and the same until. Never two answers for one job's state.
    #[tokio::test]
    async fn activity_and_the_item_say_the_same() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (_pinned, _clock) = Clock::at_t0();
        let d = fresh("ac8");
        let t = tide_tables();
        let mut held = Item { state: State::Checking, chosen: Some(0), fetch: Some("f-1".into()), ..waiting("ti-11", &t[2]) };
        on_hold(&mut held, State::Checking, "AcoustID asked Cratarr to wait".into(), T0 + 300, "paused");
        start_with(&d, vec![held]);
        let item = view().items[0].clone();
        let row = activity().into_iter().find(|r| r.id == "intake:ti-11").unwrap();
        assert_eq!((item.due, item.why.clone()), (T0 + 300, Some("AcoustID asked Cratarr to wait".to_string())));
        let waits = row.waits.clone().unwrap();
        assert_eq!((row.state, waits.until, waits.what), (crate::runner::State::Waiting, Some(item.due), item.why.clone().unwrap()), "one until, one why");
        let step = kept_in().intake(KIND).unwrap().1[0].1.iter().find(|s| s.step == "check").cloned().unwrap();
        assert_eq!((step.state.as_str(), step.next_at), ("held", item.due), "the rows say it too");
        done(&d);
    }
}
