//! The conversion queue (decision 0013): an album with hi-res files waits here while they are converted to CD format,
//! one album and one file at a time (the NAS stays free for the media server), then Keep or Replace runs on it.
//!
//! Each file is converted into `<Incoming folder>/.cratarr-convert/<album>/…`. When all are done, one plan of moves
//! puts the originals in the trash and the converted files in their place; then the action runs, its event carrying
//! what was converted, so Undo puts the originals back. A failure puts them back at once.
//!
//! Safe to interrupt: the jobs are kept in the data folder; a restart finishes each where it was (a converted file is
//! not converted again, the swap repeats safely, an intended action is finished or forgotten by the journal).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{IncomingDir, IncomingService, KeepResult, Located};
use crate::app::library::epoch_now;
use crate::convert::STAGING_DIR;
use crate::journal::{Converted, EventId};
use crate::moves::Step;
use crate::source;

/// How long a finished job is kept, for the page to say how it went.
const KEEP_FINISHED_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Keep,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Waiting,
    Converting,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Job {
    id: u64,
    source: String,
    album: source::Album,
    folder: i64,
    dir: String,
    release_group: Option<String>,
    action: Action,
    year: Option<String>,
    /// The music files to convert, inside the album folder.
    files: Vec<String>,
    /// How many are converted.
    done: usize,
    /// How far the one being converted is, 0 to 1 (not written down: it starts again from the file's beginning).
    #[serde(skip)]
    part: f32,
    state: State,
    /// The album's new folder, as planned when it was queued.
    to: String,
    /// The catalogue folder it moves into (decision 0034); jobs from before moves were into Music (0).
    #[serde(default)]
    target: i64,
    /// The swap's trash stamp, set before the swap (so a repeat uses the same paths).
    swap: Option<i64>,
    /// The action's event, set as soon as it is intended.
    event: Option<EventId>,
    undo: Option<String>,
    why: Option<String>,
    finished: Option<i64>,
    /// Who queued it (decision 0051): the journal says who changed the library.
    #[serde(default)]
    by: Option<String>,
}

/// A job as the page sees it.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub id: u64,
    pub source: String,
    pub album_id: String,
    pub title: String,
    pub action: Action,
    pub state: State,
    pub done: usize,
    /// How far the file being converted is, 0 to 1.
    pub part: f32,
    pub total: usize,
    /// The album's folder in Music.
    pub folder: String,
    /// Once done: pass to undo.
    pub undo: Option<String>,
    /// Once failed: why.
    pub why: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Jobs {
    next: u64,
    list: Vec<Job>,
}

pub(super) struct Queue {
    file: Option<PathBuf>,
    jobs: std::sync::Mutex<Jobs>,
    wake: std::sync::Arc<tokio::sync::Notify>,
    /// The whole queue after every change, for pages that follow it (`/api/events`).
    tell: tokio::sync::broadcast::Sender<Vec<JobView>>,
}

/// Where a file is converted to: in the staging folder, named `.flac`.
fn staged(dir: &str, rel: &str) -> String {
    format!("{STAGING_DIR}/{dir}/{}", flac_name(rel))
}

/// A file's name once converted: the same, ending in `.flac`.
fn flac_name(rel: &str) -> String {
    Path::new(rel).with_extension("flac").to_string_lossy().replace('\\', "/")
}

/// What converting the album's files does: each original into the trash under `stamp`, its converted file in its place.
fn conversions(dir: &str, files: &[String], stamp: i64) -> Vec<Converted> {
    files
        .iter()
        .map(|rel| Converted {
            original: format!("{dir}/{rel}"),
            trashed: format!("{}/{stamp}/{dir}/{rel}", crate::trash::TRASH_DIR),
            converted: format!("{dir}/{}", flac_name(rel)),
        })
        .collect()
}

/// The swap as moves: every original out first (a converted file may take its name), then the converted files in.
fn swap_steps(root: &Path, dir: &str, files: &[String], converted: &[Converted]) -> Vec<Step> {
    let at = |rel: &str| (root.to_path_buf(), rel.to_string());
    let mut steps: Vec<Step> = converted.iter().map(|c| Step { from: at(&c.original), to: at(&c.trashed) }).collect();
    steps.extend(files.iter().zip(converted).map(|(rel, c)| Step { from: at(&staged(dir, rel)), to: at(&c.converted) }));
    steps
}

/// Where Undo puts a converted file (`c.converted`): the trash, under the swap's stamp plus one, so it never meets the
/// original (which may have its name) and a repeated Undo uses the same path.
pub(super) fn converted_trash(c: &Converted) -> Option<String> {
    let rest = c.trashed.strip_prefix(&format!("{}/", crate::trash::TRASH_DIR))?;
    let (stamp, _) = rest.split_once('/')?;
    let dir_rel = c.converted.as_str();
    Some(format!("{}/{}/{dir_rel}", crate::trash::TRASH_DIR, stamp.parse::<i64>().ok()? + 1))
}

/// Remove an album's staging folder, and the staging root when that is left empty. Blocking.
fn clear_staging(root: &Path, dir: &str) {
    let staging = root.join(STAGING_DIR);
    let _ = std::fs::remove_dir_all(staging.join(dir));
    let mut d = staging.join(dir).parent().map(Path::to_path_buf);
    while let Some(p) = d {
        if !p.starts_with(&staging) || std::fs::remove_dir(&p).is_err() {
            break;
        }
        d = p.parent().map(Path::to_path_buf);
    }
}

impl Queue {
    /// The jobs kept in `file` (none without one, or when it cannot be read).
    pub(super) fn open(file: Option<PathBuf>) -> Self {
        let jobs = file
            .as_deref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| match serde_json::from_slice::<Jobs>(&b) {
                Ok(j) => Some(j),
                Err(e) => {
                    tracing::warn!("the conversion queue cannot be read, starting empty: {e:#}");
                    None
                }
            })
            .unwrap_or_default();
        Self { file, jobs: std::sync::Mutex::new(jobs), wake: Default::default(), tell: tokio::sync::broadcast::channel(16).0 }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Jobs> {
        self.jobs.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Change the jobs and write them down (atomically: a temporary file renamed into place).
    fn change<T>(&self, f: impl FnOnce(&mut Jobs) -> T) -> T {
        let mut jobs = self.lock();
        let out = f(&mut jobs);
        let now = epoch_now();
        jobs.list.retain(|j| j.finished.is_none_or(|at| now - at < KEEP_FINISHED_SECS));
        if let Some(file) = &self.file {
            let tmp = file.with_extension("json.tmp");
            let written = serde_json::to_vec(&*jobs)
                .map_err(anyhow::Error::from)
                .and_then(|b| Ok(std::fs::write(&tmp, b)?))
                .and_then(|()| Ok(std::fs::rename(&tmp, file)?));
            if let Err(e) = written {
                tracing::warn!("the conversion queue cannot be saved: {e:#}");
            }
        }
        let _ = self.tell.send(views(&jobs, now));
        out
    }

    /// The whole queue after every change. A listener that falls behind reads `views` instead.
    pub(super) fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Vec<JobView>> {
        self.tell.subscribe()
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) {
        self.change(|jobs| {
            if let Some(j) = jobs.list.iter_mut().find(|j| j.id == id) {
                f(j)
            }
        })
    }

    /// How far job `id`'s current file is, told to the pages; not written down (it changes every percent).
    fn part(&self, id: u64, part: f32) {
        let mut jobs = self.lock();
        if let Some(j) = jobs.list.iter_mut().find(|j| j.id == id) {
            j.part = part;
        }
        let _ = self.tell.send(views(&jobs, epoch_now()));
    }

    fn get(&self, id: u64) -> Option<Job> {
        self.lock().list.iter().find(|j| j.id == id).cloned()
    }

    /// The action of job `id` was intended as `event`.
    pub(super) fn intended(&self, id: u64, event: EventId) {
        self.update(id, |j| j.event = Some(event));
    }

    /// Whether an album waits or is being converted.
    pub(super) fn busy(&self, album_id: &str) -> bool {
        self.lock().list.iter().any(|j| j.album.id == album_id && matches!(j.state, State::Waiting | State::Converting))
    }

    /// The next job to work on: one cut short first, else the oldest waiting.
    fn next(&self) -> Option<u64> {
        let jobs = self.lock();
        let open = |s: State| jobs.list.iter().filter(move |j| j.state == s).map(|j| j.id).min();
        open(State::Converting).or_else(|| open(State::Waiting))
    }

    pub(super) fn views(&self) -> Vec<JobView> {
        views(&self.lock(), epoch_now())
    }
}

/// The jobs as the page sees them, finished ones only for a while.
fn views(jobs: &Jobs, now: i64) -> Vec<JobView> {
    jobs.list
        .iter()
        .filter(|j| j.finished.is_none_or(|at| now - at < KEEP_FINISHED_SECS))
        .map(|j| JobView {
            id: j.id,
            source: j.source.clone(),
            album_id: j.album.id.clone(),
            title: j.album.title.clone(),
            action: j.action,
            state: j.state,
            done: j.done,
            part: j.part,
            total: j.files.len(),
            folder: j.to.clone(),
            undo: j.undo.clone(),
            why: j.why.clone(),
        })
        .collect()
}

/// How a job ended.
enum Outcome {
    Done(Option<String>),
    Failed(String),
    /// Cancelled while converting: the job is gone already.
    Cancelled,
}

impl IncomingService {
    /// Queue Keep or Replace of an album whose `files` need converting first. Returns at once.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn enqueue(&self, source_id: &str, at: Located, action: Action, year: Option<&str>, files: Vec<String>, to: String, target: i64) -> KeepResult {
        let title = at.album.title.clone();
        self.queue.change(|jobs| {
            jobs.next += 1;
            jobs.list.push(Job {
                id: jobs.next,
                source: source_id.to_string(),
                album: at.album,
                folder: at.dir.folder,
                dir: at.dir.dir,
                release_group: at.dir.release_group,
                action,
                year: year.map(str::to_string),
                files,
                done: 0,
                part: 0.0,
                state: State::Waiting,
                to: to.clone(),
                target,
                swap: None,
                event: None,
                undo: None,
                why: None,
                finished: None,
                by: crate::serve::who::current().map(|w| w.user),
            });
        });
        self.queue.wake.notify_one();
        KeepResult { undo: None, title, folder: to, queued: true }
    }

    /// The conversion queue as the page sees it.
    pub fn jobs(&self) -> Vec<JobView> {
        self.queue.views()
    }

    /// The whole queue after every change (`jobs` for the first look, and after falling behind).
    pub fn subscribe_jobs(&self) -> tokio::sync::broadcast::Receiver<Vec<JobView>> {
        self.queue.subscribe()
    }

    /// Take a job out of the queue while it waits or converts (a file being converted is finished first, then
    /// dropped). Refused once its files are being swapped in.
    pub async fn cancel(&self, id: u64) -> Result<(), super::ToggleError> {
        let job = self.queue.get(id).ok_or_else(|| super::ToggleError::Refused("that conversion is over".into()))?;
        if job.swap.is_some() || !matches!(job.state, State::Waiting | State::Converting) {
            return Err(super::ToggleError::Refused(format!("{} is already going into Music", job.album.title)));
        }
        self.queue.change(|jobs| jobs.list.retain(|j| j.id != id));
        if job.state == State::Waiting
            && let Some(root) = self.lib.catalogue().folders().get(job.folder as usize).cloned()
        {
            let dir = job.dir.clone();
            let _ = tokio::task::spawn_blocking(move || clear_staging(&root, &dir)).await;
        }
        Ok(())
    }

    /// Work through the queue, one job at a time, for as long as Cratarr runs.
    pub(in crate::app) fn work(self: &std::sync::Arc<Self>) {
        let me = std::sync::Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(svc) = me.upgrade() else { return };
                match svc.queue.next() {
                    Some(id) => svc.run(id).await,
                    None => {
                        // Not holding the service while asleep, so it can go when Cratarr stops.
                        let wake = svc.queue.wake.clone();
                        drop(svc);
                        wake.notified().await;
                    }
                }
            }
        });
    }

    async fn run(&self, id: u64) {
        let Some(job) = self.queue.get(id) else { return };
        let outcome = crate::journal::done_by(job.by.clone(), self.run_job(job)).await;
        let (state, undo, why) = match outcome {
            Outcome::Done(undo) => (State::Done, undo, None),
            Outcome::Failed(why) => (State::Failed, None, Some(why)),
            Outcome::Cancelled => return,
        };
        if let Some(w) = &why {
            tracing::warn!("conversion job {id}: {w}");
        }
        self.queue.update(id, |j| {
            j.state = state;
            j.undo = undo;
            j.why = why;
            j.finished = Some(epoch_now());
        });
    }

    async fn run_job(&self, job: Job) -> Outcome {
        let Some(root) = self.lib.catalogue().folders().get(job.folder as usize).cloned() else {
            return Outcome::Failed("that Incoming folder is not in use any more".into());
        };
        // Cut short after its action was intended: the journal knows whether it happened.
        if let Some(event) = job.event {
            self.reconcile().await;
            if self.journal.get(event).ok().flatten().is_some_and(|e| e.applied) {
                return Outcome::Done(Some(event.to_string()));
            }
            self.queue.update(job.id, |j| j.event = None);
        }
        let stamp = match job.swap {
            Some(s) => s,
            None => {
                self.queue.update(job.id, |j| j.state = State::Converting);
                for (i, rel) in job.files.iter().enumerate().skip(job.done) {
                    if self.queue.get(job.id).is_none() {
                        let (r, d) = (root.clone(), job.dir.clone());
                        let _ = tokio::task::spawn_blocking(move || clear_staging(&r, &d)).await;
                        return Outcome::Cancelled;
                    }
                    let (input, output) = (root.join(&job.dir).join(rel), root.join(staged(&job.dir, rel)));
                    let (tell, mut told) = tokio::sync::watch::channel(0f32);
                    let mut work = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                        // Written whole or not at all, and the same input gives the same bytes: one already there is done.
                        if !output.is_file() {
                            crate::moves::create_dirs_like_parent(output.parent().unwrap_or(Path::new(".")))?;
                            crate::convert::to_cd_flac_told(&input, &output, |p| {
                                let _ = tell.send(p);
                            })?;
                        }
                        Ok(())
                    });
                    // How far it is, to the pages as it goes: a long file (a 40-minute track) would otherwise show
                    // nothing until it is done.
                    let converted = loop {
                        tokio::select! {
                            r = &mut work => break r.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r),
                            Ok(()) = told.changed() => {
                                let p = *told.borrow_and_update();
                                self.queue.part(job.id, p);
                            }
                        }
                    };
                    if let Err(e) = converted {
                        let (r, d) = (root.clone(), job.dir.clone());
                        let _ = tokio::task::spawn_blocking(move || clear_staging(&r, &d)).await;
                        return Outcome::Failed(format!("{rel} could not be converted: {e:#}"));
                    }
                    self.queue.update(job.id, |j| {
                        j.done = i + 1;
                        j.part = 0.0;
                    });
                }
                if self.queue.get(job.id).is_none() {
                    let (r, d) = (root.clone(), job.dir.clone());
                    let _ = tokio::task::spawn_blocking(move || clear_staging(&r, &d)).await;
                    return Outcome::Cancelled;
                }
                let s = epoch_now();
                self.queue.update(job.id, |j| j.swap = Some(s));
                s
            }
        };
        // The swap: originals into the trash, converted files in their place.
        let converted = conversions(&job.dir, &job.files, stamp);
        let steps = swap_steps(&root, &job.dir, &job.files, &converted);
        let files = self.files.clone();
        let swap = steps.clone();
        let swapped = tokio::task::spawn_blocking(move || files.move_all(&swap)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r);
        if let Err(e) = swapped {
            let (r, d) = (root.clone(), job.dir.clone());
            let _ = tokio::task::spawn_blocking(move || clear_staging(&r, &d)).await;
            return Outcome::Failed(format!("the converted files could not take the originals' place: {e:#}"));
        }
        self.rename_rows(job.folder, &converted, false).await;
        *self.owned.lock().await = None;
        // The action, on what is now a CD-format album.
        let at = Located {
            source: job.source.clone(),
            album: job.album.clone(),
            dir: IncomingDir { folder: job.folder, dir: job.dir.clone(), release_group: job.release_group.clone() },
            root: root.clone(),
        };
        let done = match job.action {
            Action::Keep => self.keep_now(&at, job.target, job.year.as_deref(), converted.clone(), Some(job.id)).await,
            Action::Replace => self.replace_now(&at, job.year.as_deref(), converted.clone(), Some(job.id)).await,
        };
        let outcome = match done {
            Ok(r) => Outcome::Done(r.undo),
            Err(e) => {
                // The album stays in Incoming as it was: the originals back, the converted files dropped.
                let files = self.files.clone();
                let back = steps.clone();
                let undone = tokio::task::spawn_blocking(move || files.move_back(&back)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r);
                match undone {
                    Ok(()) => self.rename_rows(job.folder, &converted, true).await,
                    Err(b) => tracing::error!("could not put the originals of {} back: {b:#}", job.album.title),
                }
                Outcome::Failed(e.to_string())
            }
        };
        let (r, d) = (root, job.dir.clone());
        let _ = tokio::task::spawn_blocking(move || clear_staging(&r, &d)).await;
        self.lib.catalogue().poke();
        outcome
    }

    /// The rows of renamed files (a WAV became FLAC) follow the swap, or go back (`back`); track ids kept.
    async fn rename_rows(&self, folder: i64, converted: &[Converted], back: bool) {
        let (c, converted) = (self.lib.catalogue(), converted.to_vec());
        let done = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            for v in converted.iter().filter(|v| v.converted != v.original) {
                let (from, to) = if back { (&v.converted, &v.original) } else { (&v.original, &v.converted) };
                if !c.has_row(folder, to) {
                    c.move_files(folder, from, folder, to)?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|r| r);
        if let Err(e) = done {
            tracing::warn!("catalogue: could not follow the converted files: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swap_moves_the_originals_out_before_the_converted_files_take_their_names() {
        let root = std::env::temp_dir().join(format!("cratarr-queue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |rel: &str, body: &[u8]| {
            std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
            std::fs::write(root.join(rel), body).unwrap();
        };
        put("X/A/01.flac", b"hi-res flac");
        put("X/A/02.wav", b"hi-res wav");
        let files = vec!["01.flac".to_string(), "02.wav".to_string()];
        put(&staged("X/A", "01.flac"), b"cd flac 1");
        put(&staged("X/A", "02.wav"), b"cd flac 2");
        let converted = conversions("X/A", &files, 100);
        assert_eq!(converted[1].converted, "X/A/02.flac");
        assert_eq!(converted[1].trashed, ".cratarr-trash/100/X/A/02.wav");
        let steps = swap_steps(&root, "X/A", &files, &converted);
        crate::moves::apply(&steps).unwrap();
        crate::moves::apply(&steps).unwrap();
        assert_eq!(std::fs::read(root.join("X/A/01.flac")).unwrap(), b"cd flac 1");
        assert_eq!(std::fs::read(root.join("X/A/02.flac")).unwrap(), b"cd flac 2");
        assert!(!root.join("X/A/02.wav").exists());
        assert_eq!(std::fs::read(root.join(".cratarr-trash/100/X/A/01.flac")).unwrap(), b"hi-res flac");
        // Undo (as `undo_steps` does it): converted files to the trash under the next stamp, originals back.
        let at = |rel: &str| (root.clone(), rel.to_string());
        let mut back: Vec<Step> = converted.iter().map(|c| Step { from: at(&c.converted), to: at(&converted_trash(c).unwrap()) }).collect();
        back.extend(converted.iter().map(|c| Step { from: at(&c.trashed), to: at(&c.original) }));
        crate::moves::apply(&back).unwrap();
        crate::moves::apply(&back).unwrap();
        assert_eq!(std::fs::read(root.join("X/A/01.flac")).unwrap(), b"hi-res flac");
        assert_eq!(std::fs::read(root.join("X/A/02.wav")).unwrap(), b"hi-res wav");
        assert_eq!(std::fs::read(root.join(".cratarr-trash/101/X/A/02.flac")).unwrap(), b"cd flac 2");
        clear_staging(&root, "X/A");
        assert!(!root.join(STAGING_DIR).exists(), "an empty staging folder goes");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_queue_survives_a_restart_and_forgets_finished_jobs_after_a_while() {
        let d = std::env::temp_dir().join(format!("cratarr-queue-file-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("convert-queue.json");
        let job = |id, state, finished| Job {
            id,
            source: "jellyfin".into(),
            album: source::Album {
                id: format!("a{id}"),
                title: "T".into(),
                artist: None,
                year: None,
                added: None,
                path: None,
                tracks: 1,
                format: None,
                bitrate_kbps: None,
                duration_secs: None,
                tags: Vec::new(),
                library: None,
                artists: Vec::new(),
                release: Default::default(),
            },
            folder: 1,
            dir: "X".into(),
            release_group: None,
            action: Action::Keep,
            year: None,
            files: vec!["01.flac".into()],
            done: 0,
            part: 0.0,
            state,
            to: "X - T".into(),
            target: 0,
            swap: None,
            event: None,
            undo: None,
            why: None,
            finished,
            by: None,
        };
        {
            let q = Queue::open(Some(file.clone()));
            q.change(|j| {
                j.list.push(job(1, State::Waiting, None));
                j.list.push(job(2, State::Converting, None));
                j.list.push(job(3, State::Done, Some(epoch_now() - KEEP_FINISHED_SECS - 1)));
            });
        }
        let q = Queue::open(Some(file));
        assert_eq!(q.views().iter().map(|v| v.id).collect::<Vec<_>>(), vec![1, 2], "the old finished job is gone");
        assert_eq!(q.next(), Some(2), "a job cut short goes first");
        assert!(q.busy("a1") && q.busy("a2") && !q.busy("a3"));
        let mut rx = q.subscribe();
        q.update(1, |j| j.done = 1);
        assert_eq!(rx.try_recv().unwrap().iter().find(|v| v.id == 1).map(|v| v.done), Some(1), "every change is told, whole");
        std::fs::remove_dir_all(&d).ok();
    }
}
