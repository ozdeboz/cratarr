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
//!
//! The album actions (Keep, a move between libraries, Replace, Delete) are on the one list too (the owner, 2026-11-02:
//! a move "was hard to see … Health activity had no listing of this"): a Keep, a move and a Replace told as they go
//! (`Tracked`: moving, then adding while the library reads it), then from the journal, each with its own Undo (`m…`).

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
    /// Still being made (a match looked for): the sheet asks again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub making: bool,
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
    /// How sure what is proposed is (certain ones come ticked, likely ones not: decision 0092, the owner's choice 2);
    /// none where the sheet ticks per record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certainty: Option<&'static str>,
    /// What may be chosen instead (a genre's words found).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FieldOption>,
}

/// One value a field may be given instead, where it was found and how sure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldOption {
    pub value: String,
    /// genre or style, as the sources have it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    pub from: String,
    pub certainty: &'static str,
}

/// A run's status, the same for every writer (0092).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub id: String,
    pub what: String,
    pub title: String,
    /// waiting, writing, adding (an album action's files moved, its library reading it) or done.
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
/// An album action's (its journal event's id).
pub const MOVE: char = 'm';
/// How long a Keep or a move waits for its library to read the record before it says done anyway.
const ADDING_MOST: std::time::Duration = std::time::Duration::from_secs(600);
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
/// byte-order mark left off, in lower case, in order; a number as its number ("03" is 3, "3/12" is 3).
pub fn flat(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(['\0', ';']))
        .map(|v| v.trim().trim_start_matches('\u{feff}').to_lowercase())
        .filter(|v| !v.is_empty())
        .map(|v| match v.split('/').next().and_then(|n| n.trim().parse::<u32>().ok()) {
            Some(n) if v.chars().all(|c| c.is_ascii_digit() || c == '/' || c == ' ') => n.to_string(),
            _ => v,
        })
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
        "albumartist" => ItemKey::AlbumArtist,
        "musicbrainz_albumid" => ItemKey::MusicBrainzReleaseId,
        "musicbrainz_releasegroupid" => ItemKey::MusicBrainzReleaseGroupId,
        "musicbrainz_albumartistid" => ItemKey::MusicBrainzReleaseArtistId,
        "musicbrainz_artistid" => ItemKey::MusicBrainzArtistId,
        "musicbrainz_trackid" => ItemKey::MusicBrainzRecordingId,
        // A BPM as its tag type keeps it (ID3v2's whole-number TBPM): `write`.
        "bpm" => ItemKey::Bpm,
        "tracknumber" => ItemKey::TrackNumber,
        "totaltracks" => ItemKey::TrackTotal,
        "discnumber" => ItemKey::DiscNumber,
        "totaldiscs" => ItemKey::DiscTotal,
        _ => return None,
    })
}

/// Writes `fields` into a file (each field's values as items; none: the field cleared), read back before it takes
/// the file's place.
fn write(path: &Path, fields: &[(String, Vec<String>)]) -> Result<()> {
    use lofty::tag::{ItemKey, ItemValue, TagItem};
    let keys: Vec<(lofty::tag::ItemKey, &Vec<String>)> =
        fields.iter().map(|(n, v)| Ok((key_of(n).with_context(|| format!("no field {n}"))?, v))).collect::<Result<_>>()?;
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    crate::retag::rewrite(
        path,
        |tag| {
            for (k, values) in &keys {
                tag.remove_key(*k);
                let mut k = *k;
                if k == ItemKey::Bpm {
                    tag.remove_key(ItemKey::IntegerBpm);
                    if tag.tag_type() == lofty::tag::TagType::Id3v2 {
                        k = ItemKey::IntegerBpm;
                    }
                }
                for v in *values {
                    tag.push(TagItem::new(k, ItemValue::Text(v.clone())));
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

/// A file's print: its size and its first and last MiB (a whole file's change touches its header or its end), to tell
/// whether it is still as planned or as written.
pub fn print(path: &Path) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    const PART: u64 = 1 << 20;
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut buf = Vec::new();
    (&mut f).take(PART).read_to_end(&mut buf)?;
    if len > PART {
        f.seek(SeekFrom::Start(len.saturating_sub(PART).max(PART)))?;
        f.read_to_end(&mut buf)?;
    }
    let text: String = buf.iter().map(|b| char::from(*b)).collect();
    Ok(format!("{len}:{:016x}", crate::journal::fingerprint(&text)))
}

/// A file written whole (a length's fix): set aside where it is no longer as planned; else its original copied into
/// `keep` first, the change made, the original back where it fails. Its copy and its print as written kept.
fn apply_whole(f: &mut FileWrite, keep: &Path, n: usize) -> Result<()> {
    let Some(w) = f.whole.as_mut() else { bail!("not a whole file's change") };
    // A cover: its folder's picture and its tracks', from what `covers` kept.
    if w.op == "cover" {
        return super::covers::write_kept(w.kept.as_ref().context("the picture is no longer kept")?);
    }
    if print(&f.path)? != w.planned {
        bail!("changed since it was planned");
    }
    if w.op != "reread" {
        std::fs::create_dir_all(keep)?;
        let kept = keep.join(format!("{n}-{}", f.path.file_name().and_then(|x| x.to_str()).unwrap_or("track")));
        std::fs::copy(&f.path, &kept).with_context(|| format!("keeping {} first", f.path.display()))?;
        if let Err(e) = super::length_fixes::write(&w.op, &f.path, w.ms) {
            let _ = std::fs::copy(&kept, &f.path);
            let _ = std::fs::remove_file(&kept);
            return Err(e);
        }
        w.kept = Some(kept);
    }
    w.wrote = Some(print(&f.path)?);
    Ok(())
}

/// A file written whole put back: as planned already, nothing (false); as written, its copy back (true); else set aside.
fn restore_whole(path: &Path, w: &crate::journal::Whole) -> Result<bool> {
    if w.op == "cover" {
        super::covers::undo_kept(w.kept.as_ref().context("its old cover is no longer kept")?)?;
        return Ok(true);
    }
    // A track added: to the trash, the one it replaced back.
    if w.op == "track" {
        super::track_fetch::undo_at(w.kept.as_ref().context("that can no longer be undone")?)?;
        return Ok(true);
    }
    let now = print(path)?;
    if now == w.planned || w.kept.is_none() {
        return Ok(false);
    }
    if w.wrote.as_deref() != Some(now.as_str()) {
        bail!("changed since it was written");
    }
    let kept = w.kept.as_ref().context("its original is no longer kept")?;
    std::fs::copy(kept, path).with_context(|| format!("{} not given back", path.display()))?;
    Ok(true)
}

/// Where a run keeps the originals of files it writes whole: beside the journal, under Cratarr's data folder.
fn kept_dir(run: &str) -> Result<PathBuf> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let data = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
    Ok(Path::new(&data).join("run-files").join(format!("{at}-{run}")))
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
                        Some(FieldView { field: name.clone(), track: f.track.clone(), before, after, from, certainty: None, options: Vec::new() })
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
    PlanView { kind: kind.to_string(), title: title.to_string(), records, making: false }
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
            let keep = kept_dir(&id);
            let done = tokio::task::spawn_blocking(move || {
                let mut ok = Vec::new();
                let mut failed = Vec::new();
                let mut seen = Vec::new();
                for (n, mut f) in files.into_iter().enumerate() {
                    let wrote = match (&f.whole, &keep) {
                        (None, _) => apply_one(&f.path, &f.before, &f.after, "changed since it was planned"),
                        (Some(_), Ok(keep)) => apply_whole(&mut f, keep, n),
                        (Some(_), Err(e)) => Err(anyhow::anyhow!("{e:#}")),
                    };
                    match wrote {
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

/// A change a writer makes itself (a track added to its record: its fetch's own steps), made under the one lock and
/// kept as a run done (decision 0092's writers' map, 0.14): in Changes to files, its Undo the run's. `work` gives the
/// proposal of what it wrote (its files as written). Returns the run's status, its Undo the kept run's.
pub async fn made(app: &Arc<super::App>, work: impl FnOnce() -> Result<Proposal> + Send + 'static) -> Result<Status> {
    let _turn = LOCK.lock().await;
    let p = tokio::task::spawn_blocking(work).await??;
    let at = super::moment::now_secs();
    let account = crate::serve::who::current().map(|w| w.user);
    let keep = i64::from(app.incoming.trash_days()) * 86_400;
    let run = FileRun { what: p.kind.clone(), title: p.title.clone(), records: p.records.clone(), files: p.files.clone(), failed: p.failed.clone() };
    let j = app.journal.record_run(at, &run, account.as_deref(), keep)?;
    let id = format!("{RUN}{j}");
    for (album, files) in albums_of(&p.files) {
        set_state(&album, FilesState { state: "written".into(), done: Some(files.len()), of: Some(files.len()), run: Some(id.clone()), at: Some(at) });
    }
    let status = kept_status(j, at, &run, false);
    tracing::info!(target: "library", what = %status.what, albums = status.of, "files changed");
    let _ = channel().send(status.clone());
    Ok(status)
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

/// Every run since `since`: those going, then those kept and the album actions, newest first.
pub fn runs(app: &super::App, since: i64) -> Result<Vec<Status>> {
    let mut out: Vec<Status> = LIVE_RUNS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|l| l.values().cloned().collect()).unwrap_or_default();
    out.retain(|s| s.state != "done");
    let mut kept: Vec<Status> = app.journal.file_runs_since(since)?.into_iter().map(|(id, at, run, undone)| kept_status(id, at, &run, undone)).collect();
    kept.extend(app.journal.album_events_since(since)?.iter().filter(|(id, ..)| !out.iter().any(|s| s.id == format!("{MOVE}{id}"))).map(|(id, at, e, undone)| moved_status(*id, *at, e, *undone)));
    kept.sort_by(|a, b| b.at.cmp(&a.at));
    out.extend(kept);
    Ok(out)
}

/// What an album action is called on the list, and its title.
fn named(e: &crate::journal::Event) -> (&'static str, String) {
    use crate::journal::Event;
    match e {
        Event::AlbumAdopted { title, to_dir, .. } if to_dir.0 == 0 => ("keep", format!("Kept into Music: {title}")),
        Event::AlbumAdopted { title, .. } => ("move", format!("Moved: {title}")),
        Event::AlbumReplaced { title, .. } => ("replace", format!("Replaced: {title}")),
        Event::AlbumRejected { title, .. } => ("delete", format!("Deleted: {title}")),
    }
}

/// An album action's status, from the journal.
fn moved_status(id: i64, at: i64, e: &crate::journal::Event, undone: bool) -> Status {
    let (what, title) = named(e);
    Status { id: format!("{MOVE}{id}"), what: what.into(), title, state: "done", done: 1, of: 1, failed: Vec::new(), at: Some(at), undo: Some(format!("{MOVE}{id}")), undone, albums: 1 }
}

/// An album action told on the list as it goes (`track`): moving, then adding until its library has read it, then done
/// with its Undo; or failed, with why.
pub struct Tracked {
    status: Status,
}

/// An album action starting (`what`: keep, move, replace; `title` as the list names it): told moving.
pub fn track(what: &str, title: String) -> Tracked {
    let status = Status {
        id: format!("{LIVE}{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        what: what.into(),
        title,
        state: "writing",
        done: 0,
        of: 1,
        failed: Vec::new(),
        at: None,
        undo: None,
        undone: false,
        albums: 1,
    };
    tell(&status);
    Tracked { status }
}

impl Tracked {
    /// It could not be done: why, and done.
    pub fn failed(mut self, album: &str, why: String) {
        self.status.failed.push(RunFailed { album: album.into(), title: None, artist: None, file: None, why });
        self.end(None);
    }

    /// Its files moved (`undo`: the action's journal id): adding while `added` says its library has not read it yet,
    /// asked every few seconds for at most `ADDING_MOST`, then done.
    pub fn moved<F, Fut>(mut self, undo: Option<String>, added: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = bool> + Send,
    {
        self.status.state = "adding";
        self.status.done = 1;
        tell(&self.status);
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            while started.elapsed() < ADDING_MOST && !added().await {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            self.end(undo);
        });
    }

    fn end(mut self, undo: Option<String>) {
        self.status.state = "done";
        self.status.at = Some(super::moment::now_secs());
        self.status.undo = undo.map(|id| format!("{MOVE}{id}"));
        tell(&self.status);
        // Its id from now on: the journal's, as the list reads it.
        if let Some(kept) = self.status.undo.clone() {
            let _ = channel().send(Status { id: kept, ..self.status.clone() });
        }
    }
}

/// A kept run's id as the journal's.
fn kept_id(token: &str) -> Result<i64> {
    token.strip_prefix(RUN).and_then(|n| n.parse().ok()).context("Not a kept run")
}

/// A kept run, its records as written; an album action, its folder before and after.
pub fn run(app: &super::App, token: &str) -> Result<(Status, PlanView)> {
    if let Some(id) = token.strip_prefix(MOVE).and_then(|n| n.parse::<i64>().ok()) {
        use crate::journal::Event;
        let entry = app.journal.get(id)?.filter(|e| e.applied).context("That is no longer kept")?;
        let status = moved_status(id, entry.at, &entry.event, entry.undone);
        let folders = app.library.catalogue().folders();
        let place = |(f, rel): &(i64, String)| folders.get(*f as usize).map_or_else(|| rel.clone(), |root| root.join(rel).display().to_string());
        let (album, title, before, after) = match &entry.event {
            Event::AlbumAdopted { album_id, title, from_dir, to_dir, .. } => (album_id, title, vec![place(from_dir)], vec![place(to_dir)]),
            Event::AlbumReplaced { album_id, title, .. } => (album_id, title, Vec::new(), Vec::new()),
            Event::AlbumRejected { album_id, title, folder, dir, .. } => (album_id, title, vec![place(&(*folder, dir.clone()))], vec!["the trash".to_string()]),
        };
        let fields = if before.is_empty() { Vec::new() } else { vec![FieldView { field: "folder".into(), track: None, before, after, from: status.what.clone(), certainty: None, options: Vec::new() }] };
        let record = RecordView { album: album.clone(), title: title.clone(), artist: String::new(), files: 0, fields, written: None, undone: Some(entry.undone), stamp: None };
        let sheet = PlanView { kind: status.what.clone(), title: status.title.clone(), records: vec![record], making: false };
        return Ok((status, sheet));
    }
    let id = kept_id(token)?;
    let (run, at, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    Ok((kept_status(id, at, &run, undone), view(&run.what, &run.title, &run.records, &run.files, true)))
}

/// Undoes a run (`r…`), or its records `albums`: each file put back where it still holds what the run wrote, the
/// others set aside ("changed since it was written"). Its status, `undone` once all of it is.
pub async fn undo(app: &Arc<super::App>, token: &str, albums: Option<Vec<String>>) -> Result<Status> {
    // An album action: its own Undo (Incoming's), then its status as the list has it.
    if let Some(key) = token.strip_prefix(MOVE) {
        app.incoming.undo(key).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let id: i64 = key.parse().context("Not an album action")?;
        let entry = app.journal.get(id)?.context("That is no longer kept")?;
        let status = moved_status(id, entry.at, &entry.event, true);
        tell(&status);
        return Ok(status);
    }
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
                files
                    .into_iter()
                    .map(|(i, f)| {
                        let back = match &f.whole {
                            Some(w) => restore_whole(&f.path, w),
                            None => restore_one(&f.path, &f.after, &f.before),
                        };
                        (i, f.path.clone(), back)
                    })
                    .collect::<Vec<_>>()
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
    // Files only: a cover's run names its folder (its tracks read again by `covers`).
    let paths: Vec<PathBuf> = paths.iter().filter(|p| !p.is_dir()).cloned().collect();
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
            whole: None,
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
            whole: None,
        };
        let records = [RunRecord { album: "al-1".into(), title: "Skylon".into(), artist: "Ott".into() }];
        let v = view("types", "Types and years", &records, &[file("A", &[], &["2010"]), file("B", &["2011"], &["2010"])], false);
        let r = &v.records[0];
        assert_eq!((r.files, r.written), (2, None));
        let tracks: Vec<(&str, Option<&str>)> = r.fields.iter().map(|f| (f.field.as_str(), f.track.as_deref())).collect();
        assert_eq!(tracks, [("date", Some("A")), ("date", Some("B")), ("releasetype", None)]);
    }
}
