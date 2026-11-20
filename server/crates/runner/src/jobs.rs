//! The runner's jobs (RN items 1, 2 and 6): the long work a person starts (what `app/tasks.rs` was: an import reading 400 pages of
//! Last.fm starts at once, runs on its own, and its account's pages follow it on the live channel, `task`, or by asking, `GET
//! /api/tasks`), and the work Cratarr does on its own (the analysis pass, Soulseek's checks, the sources' reading), in one list.
//!
//! - **A job says where it is** as a step: what it does ("Reading scrobbles"), how many done and of how many where that is known;
//!   what it waits for and until when (`Wait`) while it waits.
//! - **One of a kind runs at a time for an account** ("It is running already."); identical jobs (a kind and a key) are one, and
//!   the second asker follows the first.
//! - **Rows** in the journal's file (`jobs`): each job's kind, key, whose, state, start and end, error and result, written as it
//!   begins and as it ends; its steps stay in memory (R7). A finished job is kept an hour after it ended (R10), across a restart
//!   too, then pruned; the journal's own entries keep their own prune.
//! - **A restart** (`attach`) cuts off what ran: a kind that keeps checkpoints (`kinds::Kind::resumes`) goes on as a new job from
//!   its own (its row ends cancelled, nothing to say); the others end failed, "It stopped before it was done.", and their account's
//!   pages say so.
//! - **Activity** (`rows`, administrators' only): every job that runs or waits, from the runner alone: its own and those other
//!   parts keep on their rows (`provide`: the saved queues), each patch told on the live channel (`activity`), a step at most four
//!   times a second.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cratarr_store::intake::JobRow;
use cratarr_store::journal::Journal;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::state::State;

/// A finished job is kept this long for the pages that come back to it (R10).
pub const KEPT_SECS: i64 = 3600;
/// A step told at most this often (a page of 200 scrobbles a second is plenty to follow).
const EVERY: Duration = Duration::from_millis(250);
/// A job a stop cut off, as its account's pages are told.
pub const CUT_OFF: &str = "It stopped before it was done.";
/// One of a kind already running for the account.
pub const RUNNING_ALREADY: &str = "It is running already.";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// What it does now, as the page shows it ("Reading scrobbles").
    pub name: String,
    pub done: u64,
    pub total: Option<u64>,
}

/// A person's job as their pages read it (the `task` event, `GET /api/tasks`): tasks.rs' shape, unchanged.
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: u64,
    /// Which work: `import:lastfm`, `import:listenbrainz`, `import:file`, `covers:<album>`, ...
    pub kind: String,
    #[serde(skip)]
    pub account: String,
    pub preview: bool,
    pub started: i64,
    pub finished: Option<i64>,
    pub step: Option<Step>,
    pub error: Option<String>,
    /// What it brought, once finished without an error (the work's own answer); while it runs, what it has found so far where
    /// the work says (a covers look's, 0.9.6).
    pub result: Option<serde_json::Value>,
}

/// What a job waits for (its turn to decode, memory, a source's pause, the background switch), and until when where it is known.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wait {
    pub what: String,
    pub until: Option<i64>,
}

/// A job as Health › Activity shows it: its words, what it does now, how far, and whose.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// `job:<id>`, or the part's own (`intake:ti-5`).
    pub id: String,
    pub kind: String,
    pub state: State,
    /// What it does now ("Band waves", "Checking Glass Harbour – Tide Tables", "Reading scrobbles").
    pub what: String,
    /// What it does it to, and why (the file, "catching up").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<u64>,
    /// What it counts, where it counts something other than steps ("files").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waits: Option<Wait>,
    /// Scheduled or retrying: when.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    /// Its tries so far, where its kind counts them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tries: Option<u32>,
    /// Whose: an account; none for Cratarr's own (decision 0051: the journal says who).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whose: Option<String>,
    pub started: i64,
    /// Ended: it leaves the list (its patch says so once).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<i64>,
}

impl Row {
    /// Its words, through the one table (`words`): what it does, what to, then how far or what it waits for. `when` says a time
    /// as the page does.
    pub fn words(&self, when: &dyn Fn(i64) -> String) -> String {
        use crate::words::{number, say};
        let mut parts = vec![self.what.clone()];
        parts.extend(self.detail.clone());
        let far = match (self.done, self.of) {
            (Some(done), Some(of)) => {
                let (d, o) = (number(done), number(of));
                say(if self.unit.as_deref() == Some("files") { "files" } else { "count" }, &[("done", &d), ("of", &o)])
            }
            _ => None,
        };
        parts.extend(far);
        if let Some(w) = &self.waits {
            parts.push(w.what.clone());
            parts.extend(w.until.and_then(|u| say("paused_until", &[("until", &when(u))])));
        }
        parts.join(" · ")
    }
}

/// A part that keeps its work on rows of its own (the saved queues): its jobs as Activity shows them, read when asked.
pub type Provider = Arc<dyn Fn() -> Vec<Row> + Send + Sync>;

struct Kept {
    task: Task,
    key: Option<String>,
    /// Cratarr's own: no account's pages, and none of their rules.
    own: bool,
    state: State,
    detail: Option<String>,
    unit: Option<String>,
    waits: Option<Wait>,
    /// Ended (epoch s).
    ended: Option<i64>,
    told: Instant,
    /// Asked to stop (`Runner::stop`): the work that looks at it stops before its next step.
    stop: Arc<AtomicBool>,
}

impl Kept {
    fn row(&self) -> Row {
        let step = self.task.step.as_ref();
        Row {
            id: format!("job:{}", self.task.id),
            kind: self.task.kind.clone(),
            state: self.state,
            what: step.map_or_else(|| crate::kinds::named(&self.task.kind), |s| s.name.clone()),
            detail: self.detail.clone(),
            done: step.filter(|s| s.total.is_some()).map(|s| s.done),
            of: step.and_then(|s| s.total),
            unit: self.unit.clone(),
            waits: self.waits.clone(),
            at: None,
            tries: None,
            whose: (!self.own).then(|| self.task.account.clone()),
            started: self.task.started,
            finished: self.task.finished,
        }
    }

    fn job_row(&self) -> JobRow {
        JobRow {
            id: self.task.id as i64,
            kind: self.task.kind.clone(),
            key: self.key.clone(),
            account: (!self.own).then(|| self.task.account.clone()),
            preview: self.task.preview,
            state: self.state.as_str().into(),
            started: self.task.started,
            finished: self.task.finished,
            error: self.task.error.clone(),
            result: self.task.result.as_ref().map(serde_json::Value::to_string),
        }
    }
}

pub struct Runner {
    list: Mutex<Vec<Kept>>,
    tell: broadcast::Sender<Task>,
    activity: broadcast::Sender<Row>,
    next: AtomicU64,
    journal: Mutex<Option<Arc<Journal>>>,
    providers: Mutex<Vec<(&'static str, Provider)>>,
}

/// Cratarr's runner.
pub fn runner() -> &'static Runner {
    static RUNNER: OnceLock<Runner> = OnceLock::new();
    RUNNER.get_or_init(Runner::new)
}

fn now() -> i64 {
    cratarr_store::app::moment::now_secs()
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            list: Mutex::new(Vec::new()),
            tell: broadcast::channel(64).0,
            activity: broadcast::channel(256).0,
            next: AtomicU64::new(0),
            journal: Mutex::new(None),
            providers: Mutex::new(Vec::new()),
        }
    }

    fn list(&self) -> std::sync::MutexGuard<'_, Vec<Kept>> {
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn journal(&self) -> Option<Arc<Journal>> {
        self.journal.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Its jobs kept in the journal's file from now on, those kept before a restart read back first (at the start, before any job
    /// begins): what ran is cut off, each finished job kept for its hour, the rest pruned.
    pub fn attach(&self, journal: Arc<Journal>) -> anyhow::Result<()> {
        self.attach_at(journal, now())
    }

    /// `attach` at `now` (epoch s).
    pub fn attach_at(&self, journal: Arc<Journal>, now: i64) -> anyhow::Result<()> {
        journal.prune_jobs(now - KEPT_SECS)?;
        let rows: Vec<JobRow> = Vec::new();
        let mut list = self.list();
        for mut row in rows {
            self.next.fetch_max(row.id.max(0) as u64, Ordering::Relaxed);
            let state = State::parse(&row.state).unwrap_or(State::Failed);
            if !state.is_final() {
                // Cut off by the stop: a kind with checkpoints goes on from them as a new job; the others say so.
                let resumes = crate::kinds::of(&row.kind).resumes;
                (row.state, row.finished) = (if resumes { State::Cancelled } else { State::Failed }.as_str().into(), Some(now));
                if !resumes {
                    row.error = Some(CUT_OFF.into());
                }
                journal.end_job(&row)?;
            }
            if list.iter().any(|k| k.task.id == row.id as u64) {
                continue;
            }
            let task = Task {
                id: row.id as u64,
                kind: row.kind.clone(),
                account: row.account.clone().unwrap_or_default(),
                preview: row.preview,
                started: row.started,
                finished: row.finished,
                step: None,
                error: row.error.clone(),
                result: row.result.as_deref().and_then(|r| serde_json::from_str(r).ok()),
            };
            list.push(Kept {
                task,
                key: row.key.clone(),
                own: row.account.is_none(),
                state: State::parse(&row.state).unwrap_or(State::Failed),
                detail: None,
                unit: None,
                waits: None,
                ended: row.finished,
                told: Instant::now(),
                stop: Arc::new(AtomicBool::new(false)),
            });
        }
        drop(list);
        *self.journal.lock().unwrap_or_else(|e| e.into_inner()) = Some(journal);
        Ok(())
    }

    /// Every task change, as it happens (the live channel's `task`, to its account's pages).
    pub fn subscribe(&self) -> broadcast::Receiver<Task> {
        self.tell.subscribe()
    }

    /// Every Activity patch, as it happens (the live channel's `activity`, to administrators' pages).
    pub fn subscribe_activity(&self) -> broadcast::Receiver<Row> {
        self.activity.subscribe()
    }

    /// `account`'s tasks: running, or ended within the hour.
    pub fn of(&self, account: &str) -> Vec<Task> {
        self.of_at(account, now())
    }

    /// `of` at `now` (epoch s).
    pub fn of_at(&self, account: &str, now: i64) -> Vec<Task> {
        let mut list = self.list();
        list.retain(|k| k.ended.is_none_or(|e| now - e < KEPT_SECS));
        list.iter().filter(|k| !k.own && k.task.account == account).map(|k| k.task.clone()).collect()
    }

    /// A task begun for `account`; refused while one of its kind runs for it.
    pub fn begin(&'static self, kind: &str, account: &str, preview: bool) -> Result<Handle, String> {
        self.begin_in(kind, None, Some(account), preview)
    }

    /// A job of `kind` and `key` begun for `account` (none: Cratarr's own). Identical jobs are one: while one of this kind and key
    /// runs, the second asker is told the first's task to follow (`Err(Ok(task))`); one of a kind runs at a time for an account
    /// (`Err(Err(words))`).
    pub fn begin_keyed(&'static self, kind: &str, key: &str, account: Option<&str>) -> Result<Handle, Result<Task, String>> {
        if let Some(k) = self.list().iter().find(|k| k.task.kind == kind && k.key.as_deref() == Some(key) && k.ended.is_none() && kind.is_empty()) {
            return Err(Ok(k.task.clone()));
        }
        self.begin_in(kind, Some(key), account, false).map_err(Err)
    }

    fn begin_in(&'static self, kind: &str, key: Option<&str>, account: Option<&str>, preview: bool) -> Result<Handle, String> {
        let mut list = self.list();
        if let Some(account) = account
            && list.iter().any(|k| !k.own && k.task.kind == kind && k.task.account == account && k.ended.is_none())
        {
            return Err(RUNNING_ALREADY.into());
        }
        // The last of a kind is the one kept for an account's pages.
        if let Some(account) = account {
            list.retain(|k| !(!k.own && k.task.kind == kind && k.task.account == account));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let task = Task { id, kind: kind.into(), account: account.unwrap_or_default().into(), preview, started: now(), finished: None, step: None, error: None, result: None };
        let stop = Arc::new(AtomicBool::new(false));
        let kept = Kept {
            task: task.clone(),
            key: key.map(str::to_string),
            own: account.is_none(),
            state: State::Running,
            detail: None,
            unit: None,
            waits: None,
            ended: None,
            told: Instant::now(),
            stop: stop.clone(),
        };
        if let Some(j) = self.journal()
            && let Err(e) = j.begin_job(&kept.job_row())
        {
            tracing::warn!(target: "runner", kind, "a job's row not kept: {e:#}");
        }
        let row = kept.row();
        list.push(kept);
        drop(list);
        if account.is_some() {
            let _ = self.tell.send(task);
        }
        let _ = self.activity.send(row);
        Ok(Handle { runner: self, id, stop })
    }

    /// `account`'s running task `id` asked to stop (0.9.6: a cover picked ends its look). Work that looks (`Handle::stopped`) ends
    /// before its next step, with what it found; other work runs on. Whether it was running.
    pub fn stop(&self, account: &str, id: u64) -> bool {
        let list = self.list();
        let Some(k) = list.iter().find(|k| k.task.id == id && !k.own && k.task.account == account && k.ended.is_none()) else { return false };
        k.stop.store(true, Ordering::Relaxed);
        true
    }

    /// Every running job of `account` asked to stop: its account was deleted (AC6). How many.
    pub fn stop_account(&self, account: &str) -> usize {
        let list = self.list();
        let mine: Vec<&Kept> = list.iter().filter(|k| !k.own && k.task.account == account && k.ended.is_none()).collect();
        for k in &mine {
            k.stop.store(true, Ordering::Relaxed);
        }
        mine.len()
    }

    /// A part's own jobs, read from its rows when Activity is (the saved queues): `name` once.
    pub fn provide(&self, name: &'static str, rows: Provider) {
        let mut p = self.providers.lock().unwrap_or_else(|e| e.into_inner());
        p.retain(|(n, _)| *n != name);
        p.push((name, rows));
    }

    /// Activity: every job that runs or waits, its own and the parts', in the order they began. An ended job is not among them.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self.list().iter().filter(|k| k.ended.is_none()).map(Kept::row).collect();
        let providers: Vec<Provider> = self.providers.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| p.clone()).collect();
        for p in providers {
            rows.extend(p());
        }
        // A decode asked for by no job of its own (a page's waveform): seen while it waits and while it decodes.
        rows.extend(crate::resource::DECODE.rows());
        rows
    }

    /// A part's job changed (one of `provide`'s): its patch told to Activity's pages.
    pub fn told(&self, row: Row) {
        let _ = self.activity.send(row);
    }

    /// The finished jobs older than their hour pruned (R10), in memory and in the journal's file: at the start and each hour.
    /// Running, scheduled and retrying jobs stay; nothing else in the journal is touched. How many rows went.
    pub fn prune(&self, now: i64) -> anyhow::Result<usize> {
        self.list().retain(|k| k.ended.is_none_or(|e| now - e < KEPT_SECS));
        match self.journal() {
            Some(j) => j.prune_jobs(now - KEPT_SECS - 365 * 86_400),
            None => Ok(0),
        }
    }

    /// The pruner, at once and every hour, as long as Cratarr runs.
    pub fn prune_hourly(&'static self) {
        cratarr_store::run::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(3600));
            loop {
                tick.tick().await;
                if let Err(e) = self.prune(now()) {
                    tracing::warn!(target: "runner", "finished jobs not pruned: {e:#}");
                }
            }
        });
    }

    fn change(&self, id: u64, f: impl FnOnce(&mut Kept), at_once: bool) {
        let mut list = self.list();
        let Some(k) = list.iter_mut().find(|k| k.task.id == id) else { return };
        let was = k.ended;
        f(k);
        let ended = was.is_none() && k.task.finished.is_some();
        if ended {
            k.ended = k.task.finished;
            if let Some(j) = self.journal()
                && let Err(e) = j.end_job(&k.job_row())
            {
                tracing::warn!(target: "runner", kind = %k.task.kind, "a job's end not kept: {e:#}");
            }
        }
        if at_once || ended || k.told.elapsed() >= EVERY {
            k.told = Instant::now();
            if !k.own {
                let _ = self.tell.send(k.task.clone());
            }
            let _ = self.activity.send(k.row());
        }
    }
}

/// The running job's own side: where it is, what it waits for, and how it ended. Dropped unfinished (a panic), it ends with an
/// error.
pub struct Handle {
    runner: &'static Runner,
    id: u64,
    stop: Arc<AtomicBool>,
}

impl Handle {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether it was asked to stop.
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// What it has found so far, as its result while it runs (told with its steps, at most four times a second).
    pub fn found<T: Serialize>(&self, so_far: &T) {
        let value = serde_json::to_value(so_far).ok();
        self.runner.change(self.id, |k| k.task.result = value, false);
    }

    /// Where it is now. A new step is told at once, and so is a step done to its last (RY01.13: a page sees "3 of 3" before the
    /// task ends, which takes its step away); the same step's count otherwise at most four times a second. A step means it runs.
    pub fn step(&self, name: &str, done: u64, total: Option<u64>) {
        let mut new = total == Some(done);
        self.runner.change(
            self.id,
            |k| {
                new |= k.task.step.as_ref().is_none_or(|s| s.name != name) || k.waits.is_some();
                k.task.step = Some(Step { name: name.into(), done, total });
                (k.state, k.waits) = (State::Running, None);
            },
            false,
        );
        if new {
            self.runner.change(self.id, |_| {}, true);
        }
    }

    /// What it does its step to, and why ("Glass Harbour/Night Ferry/03 Neap.flac · catching up"), and what it counts.
    pub fn detail(&self, detail: Option<String>, unit: Option<&str>) {
        self.runner.change(self.id, |k| (k.detail, k.unit) = (detail, unit.map(str::to_string)), true);
    }

    /// It waits for `what` (until `until`, where known): Activity says so, at once.
    pub fn wait_for(&self, what: &str, until: Option<i64>) {
        let w = Wait { what: what.into(), until };
        self.runner.change(self.id, |k| (k.state, k.waits) = (State::Waiting, Some(w)), true);
    }

    /// It waits no more: it runs.
    pub fn go(&self) {
        self.runner.change(self.id, |k| (k.state, k.waits) = (State::Running, None), true);
    }

    /// Ended: its result, or what went wrong.
    pub fn finish<T: Serialize>(self, result: anyhow::Result<T>) {
        let (result, error) = match result {
            Ok(r) => (serde_json::to_value(r).ok(), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        self.end(result, error);
    }

    fn end(&self, result: Option<serde_json::Value>, error: Option<String>) {
        let stopped = self.stopped();
        self.runner.change(
            self.id,
            |k| {
                if k.task.finished.is_none() {
                    k.task.finished = Some(now());
                    k.task.step = None;
                    k.task.result = result;
                    k.state = if error.is_some() { State::Failed } else if stopped && k.own { State::Cancelled } else { State::Done };
                    k.task.error = error;
                    k.waits = None;
                }
            },
            true,
        );
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Finished already: nothing changes.
        self.end(None, Some(CUT_OFF.into()));
    }
}

/// Runs `work` as a job of `kind` for `account`: begun now, its handle given to the work, its result kept.
pub fn spawn<T, F>(kind: &str, account: &str, preview: bool, work: impl FnOnce(Arc<Handle>) -> F) -> Result<Task, String>
where
    T: Serialize,
    F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let handle = Arc::new(runner().begin(kind, account, preview)?);
    run(handle, work)
}

/// Runs `work` on a job begun (`Runner::begin`, `begin_keyed`): its handle given to the work, its result kept; the job's task.
pub fn run<T, F>(handle: Arc<Handle>, work: impl FnOnce(Arc<Handle>) -> F) -> Result<Task, String>
where
    T: Serialize,
    F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let task = handle.runner.list().iter().find(|k| k.task.id == handle.id).map(|k| k.task.clone()).ok_or("The task went missing.")?;
    let fut = work(handle.clone());
    // As the account that asked (a playlist an import makes is its own).
    cratarr_store::run::spawn(cratarr_store::serve::who::carried(async move {
        let result = fut.await;
        match Arc::try_unwrap(handle) {
            Ok(h) => h.finish(result),
            // The work kept a copy of its handle: end it through that one's state.
            Err(h) => match result {
                Ok(r) => h.end(serde_json::to_value(r).ok(), None),
                Err(e) => h.end(None, Some(format!("{e:#}"))),
            },
        }
    }));
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A runner of the test's own (a restart is a new one on the same journal), living as long as the tests.
    fn fresh() -> &'static Runner {
        Box::leak(Box::new(Runner::new()))
    }

    // ---- tasks.rs' three, moved with their assertions (SCENARIOS-RN's equivalence pins)

    #[tokio::test]
    async fn a_task_says_where_it_is_and_keeps_its_result() {
        let mut told = runner().subscribe();
        let task = spawn("test:kept", "tasks-test", true, |h| async move {
            h.step("Reading", 1, Some(4));
            h.step("Reading", 4, Some(4));
            Ok::<_, anyhow::Error>(serde_json::json!({ "found": 3 }))
        })
        .unwrap();
        assert!(task.finished.is_none());
        let mut seen = Vec::new();
        while let Ok(Ok(t)) = tokio::time::timeout(Duration::from_secs(2), told.recv()).await {
            if t.id != task.id {
                continue;
            }
            seen.push(t.clone());
            if t.finished.is_some() {
                break;
            }
        }
        let last = seen.last().unwrap();
        assert_eq!(last.result, Some(serde_json::json!({ "found": 3 })));
        assert!(seen.iter().any(|t| t.step.as_ref().is_some_and(|s| s.name == "Reading")), "the step was told");
        // Kept for the account's pages, and for nobody else's.
        assert!(runner().of("tasks-test").iter().any(|t| t.id == task.id && t.finished.is_some()));
        assert!(runner().of("someone else").iter().all(|t| t.id != task.id));
    }

    #[tokio::test]
    async fn one_of_a_kind_runs_at_a_time_for_an_account() {
        let (go, wait) = tokio::sync::oneshot::channel::<()>();
        spawn("test:one", "tasks-test-one", false, |_| async move {
            let _ = wait.await;
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
        assert!(spawn("test:one", "tasks-test-one", false, |_| async { Ok::<_, anyhow::Error>(()) }).is_err());
        assert!(spawn("test:one", "tasks-test-two", false, |_| async { Ok::<_, anyhow::Error>(()) }).is_ok(), "another account's own");
        go.send(()).unwrap();
    }

    #[tokio::test]
    async fn a_failure_is_kept_as_its_error() {
        let task = spawn("test:fails", "tasks-test-fails", false, |_| async { Err::<(), _>(anyhow::anyhow!("Last.fm said no")) }).unwrap();
        for _ in 0..100 {
            if let Some(t) = runner().of("tasks-test-fails").into_iter().find(|t| t.id == task.id && t.finished.is_some()) {
                assert_eq!(t.error.as_deref(), Some("Last.fm said no"));
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("never finished");
    }

    // ---- JR2–JR5

    const T0: i64 = 1_800_000_000;

    fn folder(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-runner-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// JR2 (kind 3): alice's Last.fm import at page 120 of 400 (no checkpoints) and Discogs' typing pass at 30 of 300 (Cratarr's own,
    /// its own reading its checkpoint) run when the server stops. At the start every row reads back; the import ends failed with
    /// tasks' words and alice's pages see that; the pass's old row is no longer running and a new pass begins as one job. Never a
    /// cut-off job shown as running, two of one key at once after the start, nor a person's job resumed as another account's.
    /// (The bulk queue's item and its scheduled search are the queue's rows: `track_intake`'s restart tests.)
    #[tokio::test]
    async fn jobs_read_back_after_a_restart_and_the_cut_off_say_so() {
        let d = folder("jr2");
        let file = d.join("journal.sqlite");
        let (import, typing) = {
            let before = fresh();
            before.attach_at(Arc::new(Journal::open(Some(&file)).unwrap()), T0).unwrap();
            let import = before.begin("import:lastfm", "alice", false).unwrap();
            import.step("Reading scrobbles", 120, Some(400));
            let typing = before.begin_keyed("discogs:type", "discogs:type", None).unwrap();
            typing.step("Typing records", 30, Some(300));
            let ids = (import.id(), typing.id());
            // The stop: nothing ends them.
            std::mem::forget(import);
            std::mem::forget(typing);
            ids
        };
        let journal = Arc::new(Journal::open(Some(&file)).unwrap());
        let rows = journal.jobs().unwrap();
        assert_eq!(rows.len(), 2, "every row reads back");
        let after = fresh();
        after.attach_at(journal.clone(), T0 + 600).unwrap();
        let alices = after.of_at("alice", T0 + 600);
        let cut = alices.iter().find(|t| t.id == import).expect("alice's import on her pages");
        assert_eq!((cut.finished, cut.error.as_deref()), (Some(T0 + 600), Some(CUT_OFF)), "it says it was cut off");
        assert!(after.of_at("bob", T0 + 600).is_empty(), "not another account's");
        let kept = journal.jobs().unwrap();
        assert!(kept.iter().all(|r| r.state != "running"), "no cut-off job running: {kept:?}");
        assert_eq!(kept.iter().find(|r| r.id == typing as i64).map(|r| r.state.as_str()), Some("cancelled"), "the pass goes on as itself");
        assert!(after.rows().is_empty(), "nothing shown as running");
        let again = after.begin_keyed("discogs:type", "discogs:type", None).unwrap();
        assert!(matches!(after.begin_keyed("discogs:type", "discogs:type", None), Err(Ok(t)) if t.id == again.id()), "one pass at once");
        assert!(again.id() > typing, "a new job, its id never given twice");
        drop(again);
        drop(journal);
        std::fs::remove_dir_all(&d).ok();
    }

    /// JR3: alice's second Last.fm import while hers runs is refused with tasks' words; bob's runs; the covers look of `al-42` asked
    /// twice and a background job of key `discogs:type` asked twice are each one job, the second asker following the first. Never
    /// two running jobs of one key, nor one account's job refusing another's.
    #[tokio::test]
    async fn identical_jobs_are_one() {
        let r = fresh();
        let alice = r.begin("import:lastfm", "alice", false).unwrap();
        assert_eq!(r.begin("import:lastfm", "alice", false).err().as_deref(), Some(RUNNING_ALREADY));
        let bob = r.begin("import:lastfm", "bob", false).unwrap();
        let look = r.begin_keyed("covers", "al-42", Some("root")).unwrap();
        let second = r.begin_keyed("covers", "al-42", Some("root"));
        assert!(matches!(&second, Err(Ok(t)) if t.id == look.id()), "the second asker follows the first");
        let typing = r.begin_keyed("discogs:type", "discogs:type", None).unwrap();
        assert!(matches!(r.begin_keyed("discogs:type", "discogs:type", None), Err(Ok(t)) if t.id == typing.id()));
        let running: Vec<Row> = r.rows();
        assert_eq!(running.len(), 4, "alice's, bob's, the look and the pass: {running:?}");
        drop((alice, bob, look, typing));
    }

    /// JR4: alice's import ended at t0; a restart at t0+600; at t0+1800 her pages still see it (the hour counts from its end, kept in
    /// its row), bob's see nothing of hers; bob may not stop root's Renumber. At t0+3600 it is gone.
    #[tokio::test]
    async fn a_finished_task_is_kept_its_hour_across_a_restart() {
        let d = folder("jr4");
        let file = d.join("journal.sqlite");
        let journal = Arc::new(Journal::open(Some(&file)).unwrap());
        journal.begin_job(&JobRow { id: 7, kind: "import:lastfm".into(), key: None, account: Some("alice".into()), preview: false, state: "done".into(), started: T0 - 300, finished: Some(T0), error: None, result: Some("{\"read\":400}".into()) }).unwrap();
        drop(journal);
        let r = fresh();
        r.attach_at(Arc::new(Journal::open(Some(&file)).unwrap()), T0 + 600).unwrap();
        let renumber = r.begin("renumber:al-42", "root", false).unwrap();
        let seen = r.of_at("alice", T0 + 1800);
        assert_eq!(seen.iter().map(|t| (t.id, t.finished, t.result.clone())).collect::<Vec<_>>(), [(7, Some(T0), Some(serde_json::json!({ "read": 400 })))]);
        assert!(r.of_at("bob", T0 + 1800).is_empty());
        assert!(!r.stop("bob", renumber.id()), "not his to stop");
        assert!(r.of_at("alice", T0 + 3600).is_empty(), "its hour is over");
        drop(renumber);
        std::fs::remove_dir_all(&d).ok();
    }

    /// JR5 (R10): 10,000 finished jobs over a month, 4 running; the journal's 8 file runs and 30 changes; an intake item. The pruner
    /// takes the finished rows older than their hour and nothing else. Never a running job pruned, a file run, a change or an intake
    /// item pruned by it.
    #[test]
    fn finished_jobs_are_pruned_and_nothing_else() {
        let journal = Arc::new(Journal::open(None).unwrap());
        journal
            .write(|tx| {
                for n in 0..10_000i64 {
                    let at = T0 - 30 * 86_400 + n * 259;
                    tx.begin_job(&JobRow { id: n + 1, kind: "import:file".into(), key: None, account: Some("alice".into()), preview: false, state: "done".into(), started: at - 10, finished: Some(at), error: None, result: None })?;
                }
                for n in 0..4i64 {
                    tx.begin_job(&JobRow { id: 20_000 + n, kind: "renumber:al-1".into(), key: None, account: Some("root".into()), preview: false, state: "running".into(), started: T0 - 86_400, finished: None, error: None, result: None })?;
                }
                let item = cratarr_store::intake::ItemRow { id: "ti-1".into(), kind: "bulk".into(), seq: 1, state: "queued".into(), asked_by: None, fields: serde_json::json!({}) };
                tx.keep_items("bulk", Some(1), &[(item, Vec::new())], &[])?;
                Ok(())
            })
            .unwrap();
        let run = cratarr_store::journal::FileRun { what: "types".into(), title: "T".into(), records: Vec::new(), files: Vec::new(), failed: Vec::new(), planned: Vec::new() };
        for n in 0..8 {
            journal.record_run(T0 - 20 * 86_400 + n, &run, None, 365 * 86_400).unwrap();
        }
        for n in 0..30 {
            journal.record_change(T0 - 3_600 + n, &cratarr_store::journal::Change::Renamed { from: "a".into(), to: "b".into() }, None).unwrap();
        }
        let r = fresh();
        *r.journal.lock().unwrap() = Some(journal.clone());
        let gone = r.prune(T0).unwrap();
        let left = journal.jobs().unwrap();
        assert!(left.iter().all(|j| j.finished.is_none_or(|f| f >= T0 - KEPT_SECS)), "every finished row past its hour gone");
        assert_eq!(left.iter().filter(|j| j.state == "running").count(), 4, "the running stay");
        assert_eq!(gone + left.len(), 10_004);
        assert_eq!(journal.file_runs_since(0).unwrap().len(), 8, "the file runs untouched");
        assert_eq!((1..=30).filter(|id| journal.change(*id).unwrap().is_some()).count(), 30, "the changes untouched");
        assert_eq!(journal.intake("bulk").unwrap().1.len(), 1, "the intake item untouched");
    }
}
