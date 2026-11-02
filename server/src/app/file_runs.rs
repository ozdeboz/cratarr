//! One way to change the library's files (decision 0092), the same for every writer:
//!
//! - **A proposal** (`Proposal`): each file's fields before and after, where each new value comes from, by record. The
//!   page shows it as the review sheet (`PlanView`: per record its fields old → new, a field the same in every file once).
//! - **The gate:** a run starts only from an administrator's apply of the ticked records (the routes are theirs).
//! - **Applied one way:** one album at a time under one lock; each file read again first and set aside where it no
//!   longer holds what was planned ("changed since it was planned"); written through `retag::rewrite` (only what
//!   changes, the audio compared byte for byte); a failure set aside with its reason, the rest still written.
//! - **Recorded once:** one journal entry per run (`Journal::record_run`, kept as long as the trash keeps), its Undo per
//!   run or per album, putting back only files that still hold what the run wrote.
//! - **Told once:** one status for every run (`Status`), and each record's state (`FilesState`), on the live channel.
//!
//! Its writers so far: types, years and Classical into the files (`file_facts`). Health's fixes, Renumber, the tempo,
//! the covers, the length fixes and single-track intake move here in that order (0092).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::journal::{FileRun, FileWrite, RunFailed, RunRecord};

/// What a writer proposes: its kind and title, its records, each file's change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Proposal {
    pub kind: String,
    pub title: String,
    pub records: Vec<RunRecord>,
    pub files: Vec<FileWrite>,
    /// Records set aside before the run (their sheet's stamp no longer matching, `ticked`).
    pub failed: Vec<RunFailed>,
}

/// A record's stamp: what the sheet showed of it (each file's place and fields before and after), so an apply can tell
/// whether that is still what would be written.
pub fn stamp(files: &[FileWrite], album: &str) -> String {
    let said: String = files
        .iter()
        .filter(|f| f.album == album)
        .map(|f| format!("{}\u{1}{:?}\u{1}{:?}\n", f.path.display(), f.before, f.after))
        .collect();
    format!("{:016x}", crate::journal::fingerprint(&said))
}

/// A proposal kept to the records ticked on the sheet, each set aside where its stamp no longer matches what would be
/// written now ("changed since it was planned"); without stamps, the records ticked.
pub fn ticked(mut p: Proposal, albums: &[String], stamps: Option<&HashMap<String, String>>) -> Proposal {
    let stale: Vec<String> = albums
        .iter()
        .filter(|a| stamps.and_then(|s| s.get(*a)).is_some_and(|s| *s != stamp(&p.files, a)))
        .cloned()
        .collect();
    p.files.retain(|f| albums.contains(&f.album) && !stale.contains(&f.album));
    let named = |album: &str| p.records.iter().find(|r| r.album == album).map(|r| (r.title.clone(), r.artist.clone()));
    let set_aside: Vec<RunFailed> = stale
        .iter()
        .map(|album| {
            let n = named(album);
            RunFailed { album: album.clone(), title: n.as_ref().map(|n| n.0.clone()), artist: n.map(|n| n.1), file: None, why: "changed since it was planned".into() }
        })
        .collect();
    p.failed.extend(set_aside);
    p
}

/// The review sheet: per record its changes, old → new and where the new comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanView {
    pub kind: String,
    pub title: String,
    pub records: Vec<RecordView>,
}

/// A record on the sheet (and in a run as written: `written` of `of` files).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordView {
    pub album: String,
    pub title: String,
    pub artist: String,
    /// How many of its files change.
    pub files: usize,
    pub fields: Vec<FieldView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undone: Option<bool>,
    /// On a sheet: what it showed, sent back with the apply (`ticked`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamp: Option<String>,
}

/// A field's change: in every file of the record (no `track`), or in one track.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldView {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub from: String,
}

/// A run's status, the same for every writer (0092).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub id: String,
    pub what: String,
    pub title: String,
    /// waiting, writing or done.
    pub state: &'static str,
    /// Records done of all.
    pub done: usize,
    pub of: usize,
    pub failed: Vec<RunFailed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    pub undone: bool,
    pub albums: usize,
}

/// A record's files' state: differs (offered), waiting, writing (`done` of `of` files), written (`at`, today only) or
/// changed (a file changed since it was planned or written).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct FilesState {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
}

/// What a run's id and undo token start with (a kept run: its journal id).
pub const RUN: char = 'r';
/// A run not kept yet (waiting or writing).
const LIVE: char = 'l';
/// Albums apart, so a run never takes the disk from playback.
const PACE: std::time::Duration = std::time::Duration::from_millis(50);
/// How long a record says "written".
const WRITTEN_SECS: i64 = 24 * 3600;

/// The one lock over every change to the library's files: a second run waits.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Runs waiting or writing, by id.
static LIVE_RUNS: Mutex<Option<HashMap<String, Status>>> = Mutex::new(None);
/// Records' states from runs (waiting, writing, written, changed), by album.
static STATES: Mutex<Option<HashMap<String, FilesState>>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn channel() -> &'static tokio::sync::broadcast::Sender<Status> {
    static C: OnceLock<tokio::sync::broadcast::Sender<Status>> = OnceLock::new();
    C.get_or_init(|| tokio::sync::broadcast::channel(64).0)
}

/// Runs as they go, for the live channel (administrators').
pub fn subscribe() -> tokio::sync::broadcast::Receiver<Status> {
    channel().subscribe()
}

fn tell(s: &Status) {
    let mut live = LIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner());
    let live = live.get_or_insert_with(HashMap::new);
    if s.state == "done" {
        live.remove(&s.id);
    } else {
        live.insert(s.id.clone(), s.clone());
    }
    let _ = channel().send(s.clone());
}

fn set_state(album: &str, state: FilesState) {
    STATES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(album.to_string(), state);
}

/// A record's state from runs, where it has one (a "written" only within a day).
pub fn state_of(album: &str, now: i64) -> Option<FilesState> {
    let states = STATES.lock().unwrap_or_else(|e| e.into_inner());
    let s = states.as_ref()?.get(album)?;
    (s.state != "written" || s.at.is_some_and(|at| now - at < WRITTEN_SECS)).then(|| s.clone())
}

/// Whether a writer has a run waiting or writing.
pub fn busy(what: &str) -> bool {
    LIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|l| l.values().any(|s| s.what == what && s.state != "done"))
}

/// A field's values as compared: each item split where several are kept in one (ID3v2's NUL, a "; "), trimmed, a
/// byte-order mark left off, in lower case, in order.
pub fn flat(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(['\0', ';']))
        .map(|v| v.trim().trim_start_matches('\u{feff}').to_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}

/// The fields of a file named in `names`, each with its values as items (the tag reader's names).
pub fn fields_of(path: &Path, names: &[&str]) -> Result<HashMap<String, Vec<String>>> {
    let (tags, _) = crate::catalogue::tags::read(path)?;
    let mut out: HashMap<String, Vec<String>> = names.iter().map(|n| (n.to_string(), Vec::new())).collect();
    for (_, name, value) in tags.items {
        if let Some(n) = name.filter(|n| names.contains(n)) {
            out.entry(n.to_string()).or_default().push(value);
        }
    }
    Ok(out)
}

/// Whether a file's fields hold `want` (each field's values as compared, `flat`).
fn holds(now: &HashMap<String, Vec<String>>, want: &[(String, Vec<String>)]) -> bool {
    want.iter().all(|(n, v)| flat(now.get(n).map(Vec::as_slice).unwrap_or_default()) == flat(v))
}

/// The key a field is written by.
fn key_of(name: &str) -> Option<lofty::tag::ItemKey> {
    use lofty::tag::ItemKey;
    Some(match name {
        "releasetype" => ItemKey::MusicBrainzReleaseType,
        "date" => ItemKey::RecordingDate,
        "genre" => ItemKey::Genre,
        _ => return None,
    })
}

/// Writes `fields` into a file (each field's values as items; none: the field cleared), read back before it takes
/// the file's place.
fn write(path: &Path, fields: &[(String, Vec<String>)]) -> Result<()> {
    use lofty::tag::{ItemValue, TagItem};
    let keys: Vec<(lofty::tag::ItemKey, &Vec<String>)> =
        fields.iter().map(|(n, v)| Ok((key_of(n).with_context(|| format!("no field {n}"))?, v))).collect::<Result<_>>()?;
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    crate::retag::rewrite(
        path,
        |tag| {
            for (k, values) in &keys {
                tag.remove_key(*k);
                for v in *values {
                    tag.push(TagItem::new(*k, ItemValue::Text(v.clone())));
                }
            }
        },
        |copy| {
            if !holds(&fields_of(copy, &names)?, fields) {
                bail!("the written fields did not read back as written");
            }
            Ok(())
        },
    )
}

/// One file: read again first; set aside where it no longer holds `from`; else `to` written.
fn apply_one(path: &Path, from: &[(String, Vec<String>)], to: &[(String, Vec<String>)], moved: &str) -> Result<()> {
    let names: Vec<&str> = from.iter().chain(to).map(|(n, _)| n.as_str()).collect();
    if !holds(&fields_of(path, &names)?, from) {
        bail!("{moved}");
    }
    write(path, to)
}

/// One file put back: already as it was before (an Undo cut short, a person's own undo): nothing to write, undone all
/// the same (false); holding what the run wrote: `before` written (true); else set aside ("changed since it was
/// written").
fn restore_one(path: &Path, after: &[(String, Vec<String>)], before: &[(String, Vec<String>)]) -> Result<bool> {
    let names: Vec<&str> = after.iter().chain(before).map(|(n, _)| n.as_str()).collect();
    let now = fields_of(path, &names)?;
    if holds(&now, before) {
        return Ok(false);
    }
    if !holds(&now, after) {
        bail!("changed since it was written");
    }
    write(path, before).map(|()| true)
}

/// The review sheet of a proposal (or of a run, `written` per record).
pub fn view(kind: &str, title: &str, records: &[RunRecord], files: &[FileWrite], written: bool) -> PlanView {
    let records = records
        .iter()
        .filter_map(|r| {
            let mine: Vec<&FileWrite> = files.iter().filter(|f| f.album == r.album).collect();
            // On a sheet, a record with nothing to write is left out; a run lists every record it was given, those
            // it wrote nothing of (each set aside) with none written.
            if mine.is_empty() && !written {
                return None;
            }
            let mut fields: Vec<FieldView> = Vec::new();
            let names: Vec<&String> = mine.iter().flat_map(|f| f.after.iter().map(|(n, _)| n)).fold(Vec::new(), |mut v, n| {
                if !v.contains(&n) {
                    v.push(n);
                }
                v
            });
            for name in names {
                let rows: Vec<FieldView> = mine
                    .iter()
                    .filter_map(|f| {
                        let after = f.after.iter().find(|(n, _)| n == name)?.1.clone();
                        let before = f.before.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        let from = f.from.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        Some(FieldView { field: name.clone(), track: f.track.clone(), before, after, from })
                    })
                    .collect();
                // The same change in every file of the record: said once.
                let same = rows.len() == mine.len() && rows.windows(2).all(|w| (&w[0].before, &w[0].after, &w[0].from) == (&w[1].before, &w[1].after, &w[1].from));
                match (same, rows.first()) {
                    (true, Some(first)) => fields.push(FieldView { track: None, ..first.clone() }),
                    _ => fields.extend(rows),
                }
            }
            Some(RecordView {
                album: r.album.clone(),
                title: r.title.clone(),
                artist: r.artist.clone(),
                files: mine.len(),
                fields,
                written: written.then(|| mine.len()),
                undone: written.then(|| mine.iter().all(|f| f.undone)),
                stamp: (!written).then(|| stamp(files, &r.album)),
            })
        })
        .collect();
    PlanView { kind: kind.to_string(), title: title.to_string(), records }
}

/// Starts a run of a proposal's ticked records (an administrator's yes, given by the route): one album at a time under
/// the one lock, each file's failure set aside with its reason, the run kept in the journal. Returns its first status
/// ("waiting" while another run holds the lock).
pub fn start(app: &Arc<super::App>, p: Proposal) -> Status {
    let id = format!("{LIVE}{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let albums = albums_of(&p.files);
    let mut status = Status {
        id: id.clone(),
        what: p.kind.clone(),
        title: p.title.clone(),
        state: "waiting",
        done: 0,
        of: albums.len(),
        failed: p.failed.clone(),
        at: None,
        undo: None,
        undone: false,
        albums: albums.len(),
    };
    for (album, _) in &albums {
        set_state(album, FilesState { state: "waiting".into(), done: None, of: None, run: Some(id.clone()), at: None });
    }
    tell(&status);
    let (app, first) = (app.clone(), status.clone());
    let account = crate::serve::who::current().map(|w| w.user);
    tokio::spawn(async move {
        let _turn = LOCK.lock().await;
        status.state = "writing";
        tell(&status);
        let mut written: Vec<FileWrite> = Vec::new();
        for (album, files) in albums {
            let of = files.len();
            set_state(&album, FilesState { state: "writing".into(), done: Some(0), of: Some(of), run: Some(id.clone()), at: None });
            let done = tokio::task::spawn_blocking(move || {
                let mut ok = Vec::new();
                let mut failed = Vec::new();
                let mut seen = Vec::new();
                for f in files {
                    match apply_one(&f.path, &f.before, &f.after, "changed since it was planned") {
                        Ok(()) => ok.push(f),
                        Err(e) => {
                            failed.push((f.path.file_name().and_then(|n| n.to_str()).map(str::to_string), super::plain(format!("{e:#}"))));
                            seen.push(f.path);
                        }
                    }
                }
                (ok, failed, seen)
            })
            .await;
            let (ok, failed, seen) = done.unwrap_or_else(|e| (Vec::new(), vec![(None, e.to_string())], Vec::new()));
            let record = p.records.iter().find(|r| r.album == album);
            // The written files read again, and those set aside too: the catalogue learns what changed them.
            reread(&ok.iter().map(|f| f.path.clone()).chain(seen).collect::<Vec<_>>()).await;
            let changed = failed.iter().any(|(_, why)| why.contains("changed since"));
            set_state(
                &album,
                FilesState {
                    state: if changed { "changed" } else { "written" }.into(),
                    done: Some(ok.len()),
                    of: Some(of),
                    run: Some(id.clone()),
                    at: Some(super::moment::now_secs()),
                },
            );
            status.failed.extend(failed.into_iter().map(|(file, why)| RunFailed {
                album: album.clone(),
                title: record.map(|r| r.title.clone()),
                artist: record.map(|r| r.artist.clone()),
                file,
                why,
            }));
            written.extend(ok);
            status.done += 1;
            tell(&status);
            tokio::time::sleep(PACE).await;
        }
        let at = super::moment::now_secs();
        if !written.is_empty() || !status.failed.is_empty() {
            let keep = i64::from(app.incoming.trash_days()) * 86_400;
            let run = FileRun { what: p.kind.clone(), title: p.title.clone(), records: p.records.clone(), files: written, failed: status.failed.clone() };
            match app.journal.record_run(at, &run, account.as_deref(), keep) {
                Ok(j) => {
                    let kept = format!("{RUN}{j}");
                    let mut states = STATES.lock().unwrap_or_else(|e| e.into_inner());
                    for s in states.get_or_insert_with(HashMap::new).values_mut().filter(|s| s.run.as_deref() == Some(id.as_str())) {
                        s.run = Some(kept.clone());
                    }
                    status.undo = Some(kept);
                }
                Err(e) => tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run of file changes not kept for Undo"),
            }
        }
        status.state = "done";
        status.at = Some(at);
        tracing::info!(target: "library", what = %status.what, albums = status.of, failed = status.failed.len(), "files changed");
        tell(&status);
        if let Some(kept) = &status.undo {
            // Its id from now on: the kept run's.
            LIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).remove(&id);
            let _ = channel().send(Status { id: kept.clone(), ..status.clone() });
        }
    });
    first
}

/// A kept run's status, from the journal.
fn kept_status(id: i64, at: i64, run: &FileRun, undone: bool) -> Status {
    let albums = albums_of(&run.files).len();
    Status {
        id: format!("{RUN}{id}"),
        what: run.what.clone(),
        title: run.title.clone(),
        state: "done",
        done: run.records.len(),
        of: run.records.len(),
        failed: run.failed.clone(),
        at: Some(at),
        undo: Some(format!("{RUN}{id}")),
        undone,
        albums,
    }
}

/// Every run since `since`: those waiting or writing, then those kept, newest first.
pub fn runs(app: &super::App, since: i64) -> Result<Vec<Status>> {
    let mut out: Vec<Status> = LIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|l| l.values().cloned().collect()).unwrap_or_default();
    out.retain(|s| s.state != "done");
    out.extend(app.journal.file_runs_since(since)?.into_iter().map(|(id, at, run, undone)| kept_status(id, at, &run, undone)));
    Ok(out)
}

/// A kept run's id as the journal's.
fn kept_id(token: &str) -> Result<i64> {
    token.strip_prefix(RUN).and_then(|n| n.parse().ok()).context("Not a kept run")
}

/// A kept run, its records as written.
pub fn run(app: &super::App, token: &str) -> Result<(Status, PlanView)> {
    let id = kept_id(token)?;
    let (run, at, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    Ok((kept_status(id, at, &run, undone), view(&run.what, &run.title, &run.records, &run.files, true)))
}

/// Undoes a run (`r…`), or its records `albums`: each file put back where it still holds what the run wrote, the
/// others set aside ("changed since it was written"). Its status, `undone` once all of it is.
pub async fn undo(app: &Arc<super::App>, token: &str, albums: Option<Vec<String>>) -> Result<Status> {
    let id = kept_id(token)?;
    let (run, at, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    if undone {
        bail!("Already undone");
    }
    let todo: Vec<(usize, FileWrite)> = run
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.undone && albums.as_ref().is_none_or(|a| a.contains(&f.album)))
        .map(|(i, f)| (i, f.clone()))
        .collect();
    if todo.is_empty() {
        bail!("Nothing to undo");
    }
    // As a run writes (0092): in the background under the one lock, an album at a time, told as it goes; the answer is
    // its first status. (Once one blocking batch: with 57 records on a slow disk the page's call waited minutes.)
    let mut by_album: Vec<(String, Vec<(usize, FileWrite)>)> = Vec::new();
    for (i, f) in todo {
        match by_album.iter_mut().find(|(a, _)| *a == f.album) {
            Some((_, v)) => v.push((i, f)),
            None => by_album.push((f.album.clone(), vec![(i, f)])),
        }
    }
    let mut status = kept_status(id, at, &run, false);
    (status.state, status.done, status.of, status.failed) = ("waiting", 0, by_album.len(), Vec::new());
    for (album, _) in &by_album {
        set_state(album, FilesState { state: "waiting".into(), done: None, of: None, run: Some(token.to_string()), at: None });
    }
    tell(&status);
    let (app, token, first) = (app.clone(), token.to_string(), status.clone());
    tokio::spawn(async move {
        let _turn = LOCK.lock().await;
        // Read again under the lock: another Undo may have taken some of it meanwhile.
        let Ok(Some((mut run, at, _))) = app.journal.file_run(id) else { return };
        status.state = "writing";
        tell(&status);
        for (album, files) in by_album {
            let files: Vec<(usize, FileWrite)> = files.into_iter().filter(|(i, _)| run.files.get(*i).is_some_and(|f| !f.undone)).collect();
            let of = files.len();
            set_state(&album, FilesState { state: "writing".into(), done: Some(0), of: Some(of), run: Some(token.clone()), at: None });
            let results = tokio::task::spawn_blocking(move || {
                files.into_iter().map(|(i, f)| (i, f.path.clone(), restore_one(&f.path, &f.after, &f.before))).collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            let mut back = Vec::new();
            for (i, path, r) in results {
                match r {
                    Ok(wrote) => {
                        run.files[i].undone = true;
                        if wrote {
                            back.push(path);
                        }
                    }
                    Err(e) => status.failed.push(failed_of(&run.records, &album, &path, &e)),
                }
            }
            let changed = status.failed.iter().any(|f| f.album == album);
            set_state(
                &album,
                FilesState {
                    state: if changed { "changed" } else { "differs" }.into(),
                    done: None,
                    of: None,
                    run: changed.then(|| token.clone()),
                    at: changed.then(super::moment::now_secs),
                },
            );
            reread(&back).await;
            // Kept after each album, so an Undo cut short (a restart) is not done twice.
            let all = run.files.iter().all(|f| f.undone);
            if let Err(e) = app.journal.update_run(id, &run, all.then(super::moment::now_secs)) {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "an undo's progress not kept");
            }
            status.done += 1;
            tell(&status);
            tokio::time::sleep(PACE).await;
        }
        let all = run.files.iter().all(|f| f.undone);
        let failed = std::mem::take(&mut status.failed);
        status = Status { failed, ..kept_status(id, at, &run, all) };
        tracing::info!(target: "library", what = %status.what, undone = all, failed = status.failed.len(), "file changes undone");
        tell(&status);
    });
    Ok(first)
}

/// A file set aside, with its record's title and artist (a record set aside whole has no files in the run to name it).
fn failed_of(records: &[RunRecord], album: &str, path: &Path, e: &anyhow::Error) -> RunFailed {
    let r = records.iter().find(|r| r.album == album);
    RunFailed {
        album: album.to_string(),
        title: r.map(|r| r.title.clone()),
        artist: r.map(|r| r.artist.clone()),
        file: path.file_name().and_then(|n| n.to_str()).map(str::to_string),
        why: super::plain(format!("{e:#}")),
    }
}

/// The files by album, in order.
fn albums_of(files: &[FileWrite]) -> Vec<(String, Vec<FileWrite>)> {
    let mut out: Vec<(String, Vec<FileWrite>)> = Vec::new();
    for f in files {
        match out.iter_mut().find(|(a, _)| *a == f.album) {
            Some((_, v)) => v.push(f.clone()),
            None => out.push((f.album.clone(), vec![f.clone()])),
        }
    }
    out
}

/// Written files read again at once, their analyses carried over (`Serve::reread`).
async fn reread(paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    let Some(served) = crate::serve::instance() else { return };
    let paths = paths.to_vec();
    let _ = tokio::task::spawn_blocking(move || served.reread(&paths)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retag::fixtures::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn values_compare_as_items_whatever_their_separator() {
        assert_eq!(flat(&s(&["Album\0Compilation"])), ["album", "compilation"]);
        assert_eq!(flat(&s(&["\u{feff}album", " compilation "])), ["album", "compilation"]);
        assert_eq!(flat(&s(&["Electronic; Ambient"])), ["electronic", "ambient"]);
        assert!(flat(&[]).is_empty());
    }

    #[test]
    fn a_file_is_written_only_where_it_holds_the_plan_s_before_and_put_back_only_where_it_holds_the_after() {
        let d = dir("file-runs");
        let f = flac(&d, "one");
        let m = d.join("two.mp3");
        mp3(&m);
        for t in [&f, &m] {
            let none = vec![("releasetype".to_string(), Vec::new()), ("date".to_string(), Vec::new())];
            let to = vec![("releasetype".to_string(), s(&["album", "compilation"])), ("date".to_string(), s(&["2010"]))];
            apply_one(t, &none, &to, "moved").unwrap();
            let now = fields_of(t, &["releasetype", "date"]).unwrap();
            assert!(holds(&now, &to), "{}: {now:?}", t.display());
            assert_eq!(apply_one(t, &none, &to, "changed since").unwrap_err().to_string(), "changed since");            // Undo: back to none; again, nothing to write (already as it was).
            assert!(restore_one(t, &to, &none).unwrap());
            assert!(holds(&fields_of(t, &["releasetype", "date"]).unwrap(), &none));
            assert!(!restore_one(t, &to, &none).unwrap(), "already as it was: undone, not set aside");
        }
        // A FLAC's second release type field follows (lofty has no key for it: read as the file keeps it).
        apply_one(&f, &[("releasetype".to_string(), Vec::new())], &[("releasetype".to_string(), s(&["ep"]))], "moved").unwrap();
        use lofty::prelude::*;
        let vc = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&f).unwrap(), lofty::config::ParseOptions::new()).unwrap().remove_vorbis_comments().unwrap();
        assert_eq!(vc.get_all("MUSICBRAINZ_ALBUMTYPE").collect::<Vec<_>>(), ["ep"]);
        assert_eq!(vc.get_all("RELEASETYPE").collect::<Vec<_>>(), ["ep"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_record_no_longer_as_its_sheet_showed_is_set_aside() {
        let file = |album: &str, before: &[&str]| FileWrite {
            album: album.into(),
            path: format!("/m/{album}.flac").into(),
            track: None,
            before: vec![("releasetype".into(), s(before))],
            after: vec![("releasetype".into(), s(&["ep"]))],
            from: Vec::new(),
            undone: false,
        };
        let shown = vec![file("a", &["album"]), file("b", &["album"])];
        let stamps: HashMap<String, String> = ["a", "b"].iter().map(|x| (x.to_string(), stamp(&shown, x))).collect();
        // Since the sheet, a tagger changed b's file: what would be written there is not what was shown.
        let now = Proposal { kind: "types".into(), files: vec![file("a", &["album"]), file("b", &["single"]), file("c", &["album"])], ..Proposal::default() };
        let p = ticked(now, &s(&["a", "b"]), Some(&stamps));
        assert_eq!(p.files.iter().map(|f| f.album.as_str()).collect::<Vec<_>>(), ["a"], "c was not ticked; b set aside");
        assert_eq!(p.failed, vec![RunFailed { album: "b".into(), title: None, artist: None, file: None, why: "changed since it was planned".into() }]);
    }

    #[test]
    fn the_sheet_says_a_change_once_where_every_file_has_it_and_per_track_where_not() {
        let file = |track: &str, before: &[&str], after: &[&str]| FileWrite {
            album: "al-1".into(),
            path: format!("/m/{track}.flac").into(),
            track: Some(track.into()),
            before: vec![("date".into(), s(before)), ("releasetype".into(), s(&["album"]))],
            after: vec![("date".into(), s(after)), ("releasetype".into(), s(&["album", "compilation"]))],
            from: vec![("date".into(), "the album's year".into()), ("releasetype".into(), "the type rule".into())],
            undone: false,
        };
        let records = [RunRecord { album: "al-1".into(), title: "Skylon".into(), artist: "Ott".into() }];
        let v = view("types", "Types and years", &records, &[file("A", &[], &["2010"]), file("B", &["2011"], &["2010"])], false);
        let r = &v.records[0];
        assert_eq!((r.files, r.written), (2, None));
        let tracks: Vec<(&str, Option<&str>)> = r.fields.iter().map(|f| (f.field.as_str(), f.track.as_deref())).collect();
        assert_eq!(tracks, [("date", Some("A")), ("date", Some("B")), ("releasetype", None)]);
    }
}
