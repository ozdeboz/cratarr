//! One way to change the library's files (decision 0092): a plan (each file's fields before and after), an
//! administrator's yes (the route that starts a run is theirs), applied one album at a time under one lock, each file
//! read again first and set aside where it no longer holds what was planned; written through `retag::rewrite` (only
//! what changes; the audio compared); one journal entry per run (`Journal::record_run`) with one Undo, which puts back
//! only files that still hold what the run wrote; one status for every run, told on the live channel.
//!
//! Its writers so far: types, years and Classical into the files (`file_facts`). Health's fixes, Renumber, the tempo,
//! the covers, the length fixes and single-track intake move here in that order (0092).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::journal::{FileRun, FileWrite};

/// What a run writes into one file: by field (the tag reader's name), its values before and after.
pub type FilePlan = FileWrite;

/// A run's status, the same for every writer (0092): what it is, waiting, writing or done, how far, what failed and
/// why, when it ended, its undo token, and whether it was undone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub id: u64,
    pub what: String,
    pub state: &'static str,
    pub done: usize,
    pub of: usize,
    pub failed: Vec<Failed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    pub undone: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failed {
    pub album: String,
    pub why: String,
}

/// What an undo token starts with.
pub const UNDO: char = 'r';
/// Albums apart, so a run never takes the disk from playback.
const PACE: std::time::Duration = std::time::Duration::from_millis(50);

/// The one lock over every change to the library's files: a second run waits.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Each writer's last run, by `what`.
static LAST: Mutex<Option<HashMap<String, Status>>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn channel() -> &'static tokio::sync::broadcast::Sender<Status> {
    static C: OnceLock<tokio::sync::broadcast::Sender<Status>> = OnceLock::new();
    C.get_or_init(|| tokio::sync::broadcast::channel(64).0)
}

/// Runs as they go, for the live channel (administrators').
pub fn subscribe() -> tokio::sync::broadcast::Receiver<Status> {
    channel().subscribe()
}

/// A writer's last run (none since the server started).
pub fn last(what: &str) -> Option<Status> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|l| l.get(what).cloned())
}

fn tell(s: &Status) {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(s.what.clone(), s.clone());
    let _ = channel().send(s.clone());
}

/// Whether a writer is waiting or writing now.
pub fn busy(what: &str) -> bool {
    last(what).is_some_and(|s| s.state != "done")
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

/// One file of a run: read again first; set aside where it no longer holds `from`; else `to` written.
fn apply_one(path: &Path, from: &[(String, Vec<String>)], to: &[(String, Vec<String>)], moved: &str) -> Result<()> {
    let names: Vec<&str> = from.iter().chain(to).map(|(n, _)| n.as_str()).collect();
    if !holds(&fields_of(path, &names)?, from) {
        bail!("{moved}");
    }
    write(path, to)
}

/// Starts a run of `plans` (an administrator's yes given by the route): one album at a time under the one lock, each
/// file's failure set aside with its reason, the run kept in the journal. Returns its first status ("waiting" while
/// another run holds the lock).
pub fn start(app: &Arc<super::App>, what: &str, plans: Vec<FilePlan>) -> Status {
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let albums = albums_of(&plans);
    let mut status = Status { id, what: what.to_string(), state: "waiting", done: 0, of: albums.len(), failed: Vec::new(), at: None, undo: None, undone: false };
    tell(&status);
    let (app, first) = (app.clone(), status.clone());
    let account = crate::serve::who::current().map(|w| w.user);
    tokio::spawn(async move {
        let _turn = LOCK.lock().await;
        status.state = "writing";
        tell(&status);
        let mut written: Vec<FileWrite> = Vec::new();
        for (album, files) in albums {
            let done = tokio::task::spawn_blocking(move || {
                let mut ok = Vec::new();
                let mut failed = None;
                for f in files {
                    match apply_one(&f.path, &f.before, &f.after, "changed since it was planned") {
                        Ok(()) => ok.push(f),
                        Err(e) => failed = Some(format!("{}: {}", f.path.file_name().and_then(|n| n.to_str()).unwrap_or(""), super::plain(format!("{e:#}")))),
                    }
                }
                (ok, failed)
            })
            .await;
            match done {
                Ok((ok, failed)) => {
                    reread(&ok.iter().map(|f| f.path.clone()).collect::<Vec<_>>()).await;
                    written.extend(ok);
                    if let Some(why) = failed {
                        status.failed.push(Failed { album: album.clone(), why });
                    }
                }
                Err(e) => status.failed.push(Failed { album: album.clone(), why: format!("{e}") }),
            }
            status.done += 1;
            tell(&status);
            tokio::time::sleep(PACE).await;
        }
        let at = super::moment::now_secs();
        if !written.is_empty() {
            let keep = i64::from(app.incoming.trash_days()) * 86_400;
            let run = FileRun { what: status.what.clone(), files: written };
            match app.journal.record_run(at, &run, account.as_deref(), keep) {
                Ok(j) => status.undo = Some(format!("{UNDO}{j}")),
                Err(e) => tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run of file changes not kept for Undo"),
            }
        }
        status.state = "done";
        status.at = Some(at);
        tracing::info!(target: "library", what = %status.what, albums = status.of, failed = status.failed.len(), "files changed");
        tell(&status);
    });
    first
}

/// Undoes a run (`r…`): each file put back where it still holds what the run wrote, the others set aside. Told as the
/// run's status, `undone`.
pub async fn undo(app: &Arc<super::App>, token: &str) -> Result<Status> {
    let id: i64 = token.strip_prefix(UNDO).and_then(|n| n.parse().ok()).context("Not a run's undo")?;
    let (run, _, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    if undone || !app.journal.claim_run(id, super::moment::now_secs())? {
        bail!("Already undone");
    }
    let _turn = LOCK.lock().await;
    let albums = albums_of(&run.files);
    let mut status = Status { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), what: run.what.clone(), state: "writing", done: 0, of: albums.len(), failed: Vec::new(), at: None, undo: None, undone: true };
    tell(&status);
    for (album, files) in albums {
        let (back, failed) = tokio::task::spawn_blocking(move || {
            let mut back = Vec::new();
            let mut failed = None;
            for f in files {
                match apply_one(&f.path, &f.after, &f.before, "changed since it was written") {
                    Ok(()) => back.push(f.path),
                    Err(e) => failed = Some(format!("{}: {}", f.path.file_name().and_then(|n| n.to_str()).unwrap_or(""), super::plain(format!("{e:#}")))),
                }
            }
            (back, failed)
        })
        .await?;
        reread(&back).await;
        if let Some(why) = failed {
            status.failed.push(Failed { album, why });
        }
        status.done += 1;
        tell(&status);
    }
    status.state = "done";
    status.at = Some(super::moment::now_secs());
    tell(&status);
    Ok(status)
}

/// The plans by album, in order.
fn albums_of(plans: &[FilePlan]) -> Vec<(String, Vec<FilePlan>)> {
    let mut out: Vec<(String, Vec<FilePlan>)> = Vec::new();
    for p in plans {
        match out.iter_mut().find(|(a, _)| *a == p.album) {
            Some((_, files)) => files.push(p.clone()),
            None => out.push((p.album.clone(), vec![p.clone()])),
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
            // Planned from what it no longer holds: set aside.
            assert_eq!(apply_one(t, &none, &to, "changed since").unwrap_err().to_string(), "changed since");
            // Undo: back to none.
            apply_one(t, &to, &none, "moved").unwrap();
            assert!(holds(&fields_of(t, &["releasetype", "date"]).unwrap(), &none));
        }
        // A FLAC's second release type field follows.
        apply_one(&f, &[("releasetype".to_string(), Vec::new())], &[("releasetype".to_string(), s(&["ep"]))], "moved").unwrap();
        assert_eq!(fields_of(&f, &["musicbrainz_albumtype"]).unwrap()["musicbrainz_albumtype"], ["ep"]);
        std::fs::remove_dir_all(&d).ok();
    }
}
