//! Long work as tasks (the owner, 2026-10-24: one way to show progress, "no need to reinvent the wheel"): work that
//! takes longer than a page should wait for (an import reading 400 pages of Last.fm) starts at once and runs on its own;
//! its page follows it on the live channel (`task`, to its account's pages only), or by asking (`GET /api/tasks`) while
//! the channel is down. It runs on when the page is left, and its result waits for the account's pages.
//!
//! A task says where it is as a step: what it does ("Reading scrobbles"), how many done and of how many, where that is
//! known. One of a kind runs at a time for an account; the last of each kind is kept for an hour after it ended.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::broadcast;

/// A finished task is kept this long for the pages that come back to it.
const KEPT: Duration = Duration::from_secs(3600);
/// A step told at most this often (a page of 200 scrobbles a second is plenty to follow).
const EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// What it does now, as the page shows it ("Reading scrobbles").
    pub name: String,
    pub done: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: u64,
    /// Which work: `import:lastfm`, `import:listenbrainz`, `import:file`.
    pub kind: String,
    #[serde(skip)]
    pub account: String,
    pub preview: bool,
    pub started: i64,
    pub finished: Option<i64>,
    pub step: Option<Step>,
    pub error: Option<String>,
    /// What it brought, once finished without an error (the work's own answer, as its route gave it before); while it
    /// runs, what it has found so far where the work says (a covers look's, 0.9.6).
    pub result: Option<serde_json::Value>,
}

struct Kept {
    task: Task,
    ended: Option<Instant>,
    told: Instant,
    /// Asked to stop (`Tasks::stop`): the work that looks at it stops before its next step.
    stop: Arc<AtomicBool>,
}

pub struct Tasks {
    list: Mutex<Vec<Kept>>,
    tell: broadcast::Sender<Task>,
    next: std::sync::atomic::AtomicU64,
}

/// Cratarr's tasks.
pub fn tasks() -> &'static Tasks {
    static TASKS: OnceLock<Tasks> = OnceLock::new();
    TASKS.get_or_init(|| Tasks { list: Mutex::new(Vec::new()), tell: broadcast::channel(64).0, next: Default::default() })
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Tasks {
    /// Every change, as it happens.
    pub fn subscribe(&self) -> broadcast::Receiver<Task> {
        self.tell.subscribe()
    }

    /// `account`'s tasks: running, or ended within the hour.
    pub fn of(&self, account: &str) -> Vec<Task> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        list.retain(|k| k.ended.is_none_or(|e| e.elapsed() < KEPT));
        list.iter().filter(|k| k.task.account == account).map(|k| k.task.clone()).collect()
    }

    /// A task begun for `account`; refused while one of its kind runs for it.
    pub fn begin(&'static self, kind: &str, account: &str, preview: bool) -> Result<Handle, String> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        if list.iter().any(|k| k.task.kind == kind && k.task.account == account && k.ended.is_none()) {
            return Err("It is running already.".into());
        }
        list.retain(|k| !(k.task.kind == kind && k.task.account == account));
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let task = Task { id, kind: kind.into(), account: account.into(), preview, started: now(), finished: None, step: None, error: None, result: None };
        let stop = Arc::new(AtomicBool::new(false));
        list.push(Kept { task: task.clone(), ended: None, told: Instant::now(), stop: stop.clone() });
        drop(list);
        let _ = self.tell.send(task);
        Ok(Handle { tasks: self, id, stop })
    }

    /// `account`'s running task `id` asked to stop (0.9.6: a cover picked ends its look). Work that looks
    /// (`Handle::stopped`) ends before its next step, with what it found; other work runs on. Whether it was running.
    pub fn stop(&self, account: &str, id: u64) -> bool {
        let list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        let Some(k) = list.iter().find(|k| k.task.id == id && k.task.account == account && k.ended.is_none()) else { return false };
        k.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }

    fn change(&self, id: u64, f: impl FnOnce(&mut Task), at_once: bool) {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        let Some(k) = list.iter_mut().find(|k| k.task.id == id) else { return };
        f(&mut k.task);
        if k.task.finished.is_some() {
            k.ended = Some(Instant::now());
        }
        if at_once || k.told.elapsed() >= EVERY {
            k.told = Instant::now();
            let _ = self.tell.send(k.task.clone());
        }
    }
}

/// The running task's own side: where it is, and how it ended. Dropped unfinished (a panic), it ends with an error.
pub struct Handle {
    tasks: &'static Tasks,
    id: u64,
    stop: Arc<AtomicBool>,
}

impl Handle {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether it was asked to stop.
    pub fn stopped(&self) -> bool {
        self.stop.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// What it has found so far, as its result while it runs (told with its steps, at most four times a second).
    pub fn found<T: Serialize>(&self, so_far: &T) {
        let value = serde_json::to_value(so_far).ok();
        self.tasks.change(self.id, |t| t.result = value, false);
    }

    /// Where it is now. A new step is told at once; the same step's count at most four times a second.
    pub fn step(&self, name: &str, done: u64, total: Option<u64>) {
        let mut new = false;
        self.tasks.change(
            self.id,
            |t| {
                new = t.step.as_ref().is_none_or(|s| s.name != name);
                t.step = Some(Step { name: name.into(), done, total });
            },
            false,
        );
        if new {
            self.tasks.change(self.id, |_| {}, true);
        }
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
        self.tasks.change(
            self.id,
            |t| {
                if t.finished.is_none() {
                    t.finished = Some(now());
                    t.step = None;
                    t.result = result;
                    t.error = error;
                }
            },
            true,
        );
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Finished already: nothing changes.
        self.end(None, Some("It stopped before it was done.".into()));
    }
}

/// Runs `work` as a task of `kind` for `account`: begun now, its handle given to the work, its result kept.
pub fn spawn<T, F>(kind: &str, account: &str, preview: bool, work: impl FnOnce(Arc<Handle>) -> F) -> Result<Task, String>
where
    T: Serialize,
    F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let handle = Arc::new(tasks().begin(kind, account, preview)?);
    let task = tasks().of(account).into_iter().find(|t| t.id == handle.id()).ok_or("The task went missing.")?;
    let fut = work(handle.clone());
    // As the account that asked (a playlist an import makes is its own).
    tokio::spawn(crate::serve::who::carried(async move {
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

    #[tokio::test]
    async fn a_task_says_where_it_is_and_keeps_its_result() {
        let mut told = tasks().subscribe();
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
        assert!(tasks().of("tasks-test").iter().any(|t| t.id == task.id && t.finished.is_some()));
        assert!(tasks().of("someone else").iter().all(|t| t.id != task.id));
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
            if let Some(t) = tasks().of("tasks-test-fails").into_iter().find(|t| t.id == task.id && t.finished.is_some()) {
                assert_eq!(t.error.as_deref(), Some("Last.fm said no"));
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("never finished");
    }
}
