//! One way to change the library's files (decision 0092), the same for every writer:
//!
//! - **A proposal** (`Proposal`): each file's fields before and after, where each new value comes from, by record. The
//!   page shows it as the review sheet (`PlanView`: per record its fields old → new, a field the same in every file once).
//! - **The gate:** a run starts only from an administrator's apply of the ticked records (the routes are theirs).
//! - **Applied one way:** one album at a time under one lock; each file read again first and set aside where it no
//!   longer holds what was planned ("changed since it was planned"); written through `retag::rewrite` (only what
//!   changes, the audio compared byte for byte); a failure set aside with its reason, the rest still written.
//! - **Recorded once:** one journal entry per run (`Journal::record_run`, kept as long as the trash keeps), in before its
//!   first file and kept up as it goes (`RunEntry`: a stop loses nothing, and what a stop cut off is held to what it wrote
//!   before anything writes again, `settle`), its Undo per run or per album, putting back only files that still hold what
//!   the run wrote.
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

use crate::journal::{EventId, FileRun, FileWrite, Journal, RunFailed, RunRecord};

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
    /// The library it is in, as a record's `copies` name it: only where a track's copies change differently, so it shows once
    /// a copy, and a cover's where its copies hold different ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
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
/// What a run's entry says of a record it has not finished with: what the page has to say of a run cut off.
const CUT: &str = "Cratarr stopped before this was done";
/// How often a run's entry is written again as it goes. It goes in whole, so at every album a long run's cost would grow
/// with its square; and a stop misses only which records were reached, as every file's before is in from the start.
const SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// The one lock over every change to the library's files: a second run waits. Taken by `turn`.
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

/// Fields written by their name, lofty having no key for them (`retag::rewrite_named`): a style.
const NAMED: [&str; 1] = ["style"];

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
        // A record's words (0.15, decision 0097): its moods, its tags as grouping (a style by its name: `NAMED`).
        "mood" => ItemKey::Mood,
        "grouping" => ItemKey::ContentGroup,
        _ => return None,
    })
}

/// Writes `fields` into a file (each field's values as items; none: the field cleared), read back before it takes
/// the file's place.
fn write(path: &Path, fields: &[(String, Vec<String>)]) -> Result<()> {
    use lofty::tag::{ItemKey, ItemValue, TagItem};
    let named: Vec<(String, Vec<String>)> = fields.iter().filter(|(n, _)| NAMED.contains(&n.as_str())).cloned().collect();
    let keys: Vec<(lofty::tag::ItemKey, &Vec<String>)> = fields
        .iter()
        .filter(|(n, _)| !NAMED.contains(&n.as_str()))
        .map(|(n, v)| Ok((key_of(n).with_context(|| format!("no field {n}"))?, v)))
        .collect::<Result<_>>()?;
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    crate::retag::rewrite_named(
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
        &named,
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
/// `keep` first and the change made, beside the file: where that fails the file is as it was, and its copy not needed. Its
/// copy and its print as written kept. `noted` has it with all its Undo needs, before the file changes: what stops this
/// after that finds it in the run's entry.
fn apply_whole(f: &mut FileWrite, keep: &Path, n: usize, noted: impl FnOnce(&FileWrite)) -> Result<()> {
    let Some(w) = f.whole.clone() else { bail!("not a whole file's change") };
    // A cover: its folder's picture and its tracks', from what `covers` kept.
    if w.op == "cover" {
        return super::covers::write_kept(w.kept.as_ref().context("the picture is no longer kept")?, || noted(f));
    }
    if print(&f.path)? != w.planned {
        bail!("changed since it was planned");
    }
    if w.op == "reread" {
        f.whole = Some(crate::journal::Whole { wrote: Some(w.planned.clone()), ..w });
        return Ok(());
    }
    std::fs::create_dir_all(keep)?;
    let kept = keep.join(format!("{n}-{}", f.path.file_name().and_then(|x| x.to_str()).unwrap_or("track")));
    std::fs::copy(&f.path, &kept).with_context(|| format!("keeping {} first", f.path.display()))?;
    let path = f.path.clone();
    let changed = super::length_fixes::write(&w.op, &path, w.ms, |made| {
        f.whole = Some(crate::journal::Whole { kept: Some(kept.clone()), wrote: Some(print(made)?), ..w.clone() });
        noted(f);
        Ok(())
    });
    if changed.is_err() {
        let _ = std::fs::remove_file(&kept);
    }
    changed
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
    // Through a copy beside it: an Undo cut short must not leave half a track, which no Undo would take back.
    crate::moves::replace_through_part(path, |part| {
        std::fs::copy(kept, part).with_context(|| format!("{} not given back", path.display()))?;
        Ok(())
    })?;
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

/// A field's rows that are not the same in every file of the record, by track: what a track's copies change alike is said
/// once, and where they differ each row says its library.
fn by_track(rows: Vec<(FieldView, Option<String>)>) -> Vec<FieldView> {
    let same = |a: &FieldView, b: &FieldView| (&a.track, &a.before, &a.after, &a.from) == (&b.track, &b.before, &b.after, &b.from);
    let mut kept: Vec<(FieldView, Option<String>)> = Vec::new();
    for (row, library) in rows {
        if !kept.iter().any(|(k, _)| same(k, &row)) {
            kept.push((row, library));
        }
    }
    let told: Vec<bool> = kept.iter().map(|(row, library)| kept.iter().any(|(other, theirs)| other.track == row.track && theirs != library)).collect();
    kept.into_iter().zip(told).map(|((row, library), told)| FieldView { library: library.filter(|_| told), ..row }).collect()
}

/// The review sheet of a proposal (or of a run, `written` per record).
pub fn view(kind: &str, title: &str, records: &[RunRecord], files: &[FileWrite], written: bool) -> PlanView {
    let roots: Vec<PathBuf> = crate::serve::instance().filter(|s| s.has_catalogue()).map_or_else(Vec::new, |s| s.catalogue().folders().iter().map(|(_, root)| root.to_path_buf()).collect());
    view_in(kind, title, records, files, written, &roots)
}

/// `view`, its libraries these folders.
fn view_in(kind: &str, title: &str, records: &[RunRecord], files: &[FileWrite], written: bool, roots: &[PathBuf]) -> PlanView {
    let library_of = |path: &Path| roots.iter().find(|r| path.starts_with(r)).map(|r| r.display().to_string());
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
                let rows: Vec<(FieldView, Option<String>)> = mine
                    .iter()
                    .filter_map(|f| {
                        let after = f.after.iter().find(|(n, _)| n == name)?.1.clone();
                        let before = f.before.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        let from = f.from.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
                        Some((FieldView { field: name.clone(), track: f.track.clone(), library: None, before, after, from, certainty: None, options: Vec::new() }, library_of(&f.path)))
                    })
                    .collect();
                // The same change in every file of the record: said once.
                let same = rows.len() == mine.len() && rows.windows(2).all(|w| (&w[0].0.before, &w[0].0.after, &w[0].0.from) == (&w[1].0.before, &w[1].0.after, &w[1].0.from));
                match (same, rows.first()) {
                    (true, Some((first, _))) => fields.push(FieldView { track: None, ..first.clone() }),
                    _ => fields.extend(by_track(rows)),
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

/// A run's entry in the journal while it writes. In before the first file, with every file's before and after, so whatever
/// stops the run nothing is lost: the files then say what was reached, and it is held to that (`settle`). A file written
/// whole comes in on its way, with all its Undo needs and before it changes (`whole_file`):
/// what its Undo needs comes of starting it, and a cover's Undo with its old pictures not kept yet would take the pictures
/// out of its tracks.
struct RunEntry {
    journal: Arc<Journal>,
    id: Option<EventId>,
    run: FileRun,
    /// The records not finished with, as the entry says them.
    cut: Vec<RunFailed>,
    saved: std::time::Instant,
}

impl RunEntry {
    /// `keep`: as `Journal::record_run`. Nothing to write and nothing set aside: no entry.
    fn begin(journal: Arc<Journal>, p: &Proposal, at: i64, account: Option<&str>, keep: i64) -> Self {
        let cut: Vec<RunFailed> = albums_of(&p.files)
            .into_iter()
            .map(|(album, _)| {
                let r = p.records.iter().find(|r| r.album == album);
                RunFailed { title: r.map(|r| r.title.clone()), artist: r.map(|r| r.artist.clone()), album, file: None, why: CUT.into() }
            })
            .collect();
        let files = p.files.iter().filter(|f| f.whole.is_none()).cloned().collect();
        let failed = p.failed.iter().chain(&cut).cloned().collect();
        let planned = cut.iter().map(|c| c.album.clone()).collect();
        let run = FileRun { what: p.kind.clone(), title: p.title.clone(), records: p.records.clone(), files, failed, planned };
        let id = (!p.files.is_empty() || !p.failed.is_empty()).then(|| journal.record_run(at, &run, account, keep)).and_then(|kept| match kept {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run of file changes not kept for Undo");
                None
            }
        });
        if let Some(id) = id {
            journal.run_writing(id);
        }
        Self { journal, id, run, cut, saved: std::time::Instant::now() }
    }

    /// A file written whole, about to change: saved at once, at every file and not by `SAVE_EVERY`, as its Undo is in nothing
    /// else. Its own copy costs far more than this.
    fn whole_file(&mut self, f: &FileWrite) {
        self.run.files.push(f.clone());
        self.save();
    }

    /// An album finished: what it wrote takes the place of what it planned, and `failed` is all the run set aside so far.
    fn album_done(&mut self, album: &str, wrote: Vec<FileWrite>, failed: &[RunFailed]) {
        self.cut.retain(|c| c.album != album);
        self.run.planned.retain(|a| a != album);
        self.run.files.retain(|f| f.album != album);
        self.run.files.extend(wrote);
        self.run.failed = failed.iter().chain(&self.cut).cloned().collect();
        if self.saved.elapsed() >= SAVE_EVERY {
            self.save();
        }
    }

    fn save(&mut self) {
        let Some(id) = self.id else { return };
        if let Err(e) = self.journal.update_run(id, &self.run, None) {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run's progress not kept");
        }
        self.saved = std::time::Instant::now();
    }

    /// The run over, every album done: its Undo's token, where it is kept.
    fn end(&mut self) -> Option<String> {
        self.save();
        let id = self.id?;
        self.journal.run_ended(id);
        if self.run.files.is_empty() && self.run.failed.is_empty() {
            if let Err(e) = self.journal.forget_run(id) {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run that came to nothing still kept");
            }
            return None;
        }
        Some(format!("{RUN}{id}"))
    }
}

/// The one lock, and under it, before anything writes, every run a stop cut off held to what it wrote.
async fn turn(journal: &Arc<Journal>) -> tokio::sync::MutexGuard<'static, ()> {
    let turn = LOCK.lock().await;
    let journal = journal.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || settle(&journal)).await.map_err(anyhow::Error::from).and_then(|r| r) {
        tracing::warn!(target: "library", reason = %format!("{e:#}"), "a run cut off not read");
    }
    turn
}

/// The runs a stop cut off, at start: `turn` alone would leave them for the next run, and what writes outside the one lock
/// (a person's tagger, a fix) could write the same meanwhile.
pub async fn settle_cut_off(journal: Arc<Journal>) {
    drop(turn(&journal).await);
}

/// Each run a stop cut off held to the files it reached. Its entry names every file it planned, and once anything else has
/// written the same into one of them neither its page nor its Undo can tell that file from its own, so what the files say
/// is taken now, before anything writes again. A record with a file that cannot be read is read again next time: a mount
/// not up yet says nothing of what was written, and until then that file stays. Blocking.
fn settle(journal: &Journal) -> Result<()> {
    for (id, _, mut run, undone) in journal.file_runs_since(0)? {
        if undone || run.planned.is_empty() {
            continue;
        }
        let planned = std::mem::take(&mut run.planned);
        let mut unread = Vec::new();
        run.files.retain(|f| {
            if f.undone || !planned.contains(&f.album) {
                return true;
            }
            reached(f).unwrap_or_else(|e| {
                tracing::warn!(target: "library", file = %f.path.display(), reason = %format!("{e:#}"), "a file of a run cut off not read");
                unread.push(f.album.clone());
                true
            })
        });
        run.planned = planned.into_iter().filter(|a| unread.contains(a)).collect();
        journal.update_run(id, &run, None)?;
    }
    Ok(())
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
        let _turn = turn(&app.journal).await;
        status.state = "writing";
        tell(&status);
        let at = super::moment::now_secs();
        // Shared with the blocking pool: a file written whole goes into it from there.
        let entry = Arc::new(Mutex::new(RunEntry::begin(app.journal.clone(), &p, at, account.as_deref(), i64::from(app.incoming.trash_days()) * 86_400)));
        for (album, files) in albums {
            let of = files.len();
            set_state(&album, FilesState { state: "writing".into(), done: Some(0), of: Some(of), run: Some(id.clone()), at: None });
            let (keep, noted) = (kept_dir(&id), entry.clone());
            let done = tokio::task::spawn_blocking(move || {
                let mut ok = Vec::new();
                let mut failed = Vec::new();
                let mut seen = Vec::new();
                for (n, mut f) in files.into_iter().enumerate() {
                    let wrote = match (&f.whole, &keep) {
                        (None, _) => apply_one(&f.path, &f.before, &f.after, "changed since it was planned"),
                        (Some(_), Ok(keep)) => apply_whole(&mut f, keep, n, |f| noted.lock().unwrap_or_else(|e| e.into_inner()).whole_file(f)),
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
            let changed = failed.iter().any(|(_, why)| why.contains("changed since"));
            status.failed.extend(failed.into_iter().map(|(file, why)| RunFailed {
                album: album.clone(),
                title: record.map(|r| r.title.clone()),
                artist: record.map(|r| r.artist.clone()),
                file,
                why,
            }));
            let (wrote, again): (usize, Vec<PathBuf>) = (ok.len(), ok.iter().map(|f| f.path.clone()).chain(seen).collect());
            entry.lock().unwrap_or_else(|e| e.into_inner()).album_done(&album, ok, &status.failed);
            // The written files read again, and those set aside too: the catalogue learns what changed them.
            reread(&again).await;
            set_state(
                &album,
                FilesState {
                    state: if changed { "changed" } else { "written" }.into(),
                    done: Some(wrote),
                    of: Some(of),
                    run: Some(id.clone()),
                    at: Some(super::moment::now_secs()),
                },
            );
            status.done += 1;
            tell(&status);
            tokio::time::sleep(PACE).await;
        }
        let kept = entry.lock().unwrap_or_else(|e| e.into_inner()).end();
        if let Some(kept) = kept {
            let mut states = STATES.lock().unwrap_or_else(|e| e.into_inner());
            for s in states.get_or_insert_with(HashMap::new).values_mut().filter(|s| s.run.as_deref() == Some(id.as_str())) {
                s.run = Some(kept.clone());
            }
            status.undo = Some(kept);
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
    let _turn = turn(&app.journal).await;
    let p = tokio::task::spawn_blocking(work).await??;
    let at = super::moment::now_secs();
    let account = crate::serve::who::current().map(|w| w.user);
    let keep = i64::from(app.incoming.trash_days()) * 86_400;
    let run = FileRun { what: p.kind.clone(), title: p.title.clone(), records: p.records.clone(), files: p.files.clone(), failed: p.failed.clone(), planned: Vec::new() };
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
    kept.extend(app.journal.album_events_since(since)?.iter().filter(|(id, ..)| !out.iter().any(|s| s.id == format!("{MOVE}{id}"))).map(|(id, at, e, undone)| moved_status(app, *id, *at, e, *undone)));
    kept.sort_by(|a, b| b.at.cmp(&a.at));
    out.extend(kept);
    Ok(out)
}

/// What an album action is called on the list, and its title (a keep or a move as Incoming names it as it goes).
fn named(app: &super::App, e: &crate::journal::Event) -> (&'static str, String) {
    use crate::journal::Event;
    match e {
        Event::AlbumAdopted { source, title, from_dir, to_dir, .. } => app.incoming.adopted_named(source, from_dir.0, to_dir.0, title),
        Event::AlbumReplaced { title, .. } => ("replace", format!("Replaced: {title}")),
        Event::AlbumRejected { title, .. } => ("delete", format!("Deleted: {title}")),
    }
}

/// An album action's status, from the journal.
fn moved_status(app: &super::App, id: i64, at: i64, e: &crate::journal::Event, undone: bool) -> Status {
    let (what, title) = named(app, e);
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

/// Whether a file of a run cut off holds what the run was to write: its entry cannot say, a file it never reached being in
/// it too. A file written whole is in it on its way, so one the stop caught before it took the file's place is still as
/// planned; a cover is in it from before its first change, and its Undo is right however far it got. An error says neither.
fn reached(f: &FileWrite) -> Result<bool> {
    if let Some(w) = &f.whole {
        return Ok(w.op == "cover" || print(&f.path)? != w.planned);
    }
    let names: Vec<&str> = f.after.iter().chain(&f.before).map(|(n, _)| n.as_str()).collect();
    let now = fields_of(&f.path, &names)?;
    Ok(holds(&now, &f.after) && !holds(&now, &f.before))
}

/// A kept run, its records as written; an album action, its folder before and after.
pub async fn run(app: &super::App, token: &str) -> Result<(Status, PlanView)> {
    if let Some(id) = token.strip_prefix(MOVE).and_then(|n| n.parse::<i64>().ok()) {
        use crate::journal::Event;
        let entry = app.journal.get(id)?.filter(|e| e.applied).context("That is no longer kept")?;
        let status = moved_status(app, id, entry.at, &entry.event, entry.undone);
        let folders = app.library.catalogue().folders();
        let place = |(f, rel): &(i64, String)| folders.get(*f).map_or_else(|| rel.clone(), |root| root.join(rel).display().to_string());
        let (album, title, before, after, files) = match &entry.event {
            Event::AlbumAdopted { album_id, title, from_dir, to_dir, moves, .. } => (album_id, title, vec![place(from_dir)], vec![place(to_dir)], moves.len()),
            Event::AlbumReplaced { album_id, title, moves, .. } => (album_id, title, Vec::new(), Vec::new(), moves.len()),
            Event::AlbumRejected { album_id, title, folder, dir, token, .. } => {
                // A delete keeps no list of its files: they are those of its folder in the trash, or back in its place once undone.
                let lies = folders.get(*folder).map(|root| if entry.undone { root.join(dir) } else { root.join(crate::trash::TRASH_DIR).join(token) });
                let files = match lies {
                    Some(at) => tokio::task::spawn_blocking(move || super::incoming::files_under(&at).len()).await?,
                    None => 0,
                };
                (album_id, title, vec![place(&(*folder, dir.clone()))], vec!["the trash".to_string()], files)
            }
        };
        let fields = if before.is_empty() { Vec::new() } else { vec![FieldView { field: "folder".into(), track: None, library: None, before, after, from: status.what.clone(), certainty: None, options: Vec::new() }] };
        // Every file went at once, so all of them were written: the page shows a record, and its Undo, by what was written.
        let record = RecordView { album: album.clone(), title: title.clone(), artist: String::new(), files, fields, written: Some(files), undone: Some(entry.undone), stamp: None };
        let sheet = PlanView { kind: status.what.clone(), title: status.title.clone(), records: vec![record], making: false };
        return Ok((status, sheet));
    }
    let id = kept_id(token)?;
    let (run, at, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    let mut sheet = view(&run.what, &run.title, &run.records, &run.files, true);
    // Cut off and not read yet (`settle`): its entry names more than it wrote, so the files say.
    if !run.planned.is_empty() {
        let files: Vec<FileWrite> = run.files.iter().filter(|f| run.planned.contains(&f.album)).cloned().collect();
        let wrote = tokio::task::spawn_blocking(move || files.into_iter().filter(|f| reached(f).unwrap_or(false)).map(|f| f.album).collect::<Vec<_>>()).await?;
        for r in sheet.records.iter_mut().filter(|r| run.planned.contains(&r.album)) {
            r.written = Some(wrote.iter().filter(|a| **a == r.album).count());
        }
    }
    Ok((kept_status(id, at, &run, undone), sheet))
}

/// Undoes a run (`r…`), or its records `albums`: each file put back where it still holds what the run wrote, the
/// others set aside ("changed since it was written"). Its status, `undone` once all of it is.
pub async fn undo(app: &Arc<super::App>, token: &str, albums: Option<Vec<String>>) -> Result<Status> {
    // An album action: its own Undo (Incoming's), then its status as the list has it.
    if let Some(key) = token.strip_prefix(MOVE) {
        app.incoming.undo(key).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let id: i64 = key.parse().context("Not an album action")?;
        let entry = app.journal.get(id)?.context("That is no longer kept")?;
        let status = moved_status(app, id, entry.at, &entry.event, true);
        tell(&status);
        return Ok(status);
    }
    let id = kept_id(token)?;
    let (run, at, undone) = app.journal.file_run(id)?.context("That run is no longer kept")?;
    if undone {
        bail!("Already undone");
    }
    let mut todo: Vec<String> = Vec::new();
    for f in run.files.iter().filter(|f| !f.undone && albums.as_ref().is_none_or(|a| a.contains(&f.album))) {
        if !todo.contains(&f.album) {
            todo.push(f.album.clone());
        }
    }
    if todo.is_empty() {
        bail!("Nothing to undo");
    }
    // As a run writes (0092): in the background under the one lock, an album at a time, told as it goes; the answer is
    // its first status. (Once one blocking batch: with 57 records on a slow disk the page's call waited minutes.)
    let mut status = kept_status(id, at, &run, false);
    (status.state, status.done, status.of, status.failed) = ("waiting", 0, todo.len(), Vec::new());
    for album in &todo {
        set_state(album, FilesState { state: "waiting".into(), done: None, of: None, run: Some(token.to_string()), at: None });
    }
    tell(&status);
    let (app, token, first) = (app.clone(), token.to_string(), status.clone());
    tokio::spawn(async move {
        let _turn = turn(&app.journal).await;
        // Read again under the lock: another Undo may have taken some of it meanwhile, a run cut off been held to what it
        // wrote. Its files by their place in this reading, so an album's are found here and not before.
        let Ok(Some((mut run, at, _))) = app.journal.file_run(id) else { return };
        status.state = "writing";
        tell(&status);
        for album in todo {
            let files: Vec<(usize, FileWrite)> = run.files.iter().enumerate().filter(|(_, f)| f.album == album && !f.undone).map(|(i, f)| (i, f.clone())).collect();
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

    /// An MP3 with no info frame, 50 silent frames: what a length's fix writes a new frame into.
    fn bare_mp3(path: &Path) -> Vec<u8> {
        let mut b = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
        for _ in 0..50 {
            let mut frame = vec![0xFF, 0xFB, 0x90, 0x64];
            frame.resize(417, 0);
            b.extend_from_slice(&frame);
        }
        std::fs::write(path, &b).unwrap();
        b
    }

    fn whole(path: &Path, op: &str) -> FileWrite {
        let w = crate::journal::Whole { op: op.into(), ms: None, kept: None, planned: print(path).unwrap(), wrote: None };
        FileWrite { album: "al-1".into(), path: path.into(), track: None, before: Vec::new(), after: Vec::new(), from: Vec::new(), undone: false, whole: Some(w) }
    }

    fn names_in(d: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// A second name for the file sees what is done to its bytes, and a rename leaves it be: so these two hold the old
    /// file whole for as long as the new one is being made.
    #[test]
    fn a_length_fix_and_its_undo_never_write_into_the_library_s_own_file() {
        let d = dir("whole-beside");
        let (track, keep) = (d.join("track.mp3"), d.join("keep"));
        let before = bare_mp3(&track);
        let seen = d.join("seen");
        std::fs::hard_link(&track, &seen).unwrap();
        let mut f = whole(&track, "length");

        apply_whole(&mut f, &keep, 0, |_| ()).unwrap();
        let after = std::fs::read(&track).unwrap();
        assert!(after.len() > before.len(), "its info frame went in");
        assert_eq!(std::fs::read(&seen).unwrap(), before, "the old file left whole, not written over");
        let w = f.whole.as_ref().unwrap();
        assert_eq!(std::fs::read(w.kept.as_ref().unwrap()).unwrap(), before);
        assert_eq!(w.wrote.as_deref(), Some(print(&track).unwrap().as_str()));

        std::fs::remove_file(&seen).unwrap();
        std::fs::hard_link(&track, &seen).unwrap();
        assert!(restore_whole(&track, w).unwrap());
        assert_eq!(std::fs::read(&track).unwrap(), before);
        assert_eq!(std::fs::read(&seen).unwrap(), after, "an Undo does not write over the file either");
        assert_eq!(names_in(&d), ["keep", "seen", "track.mp3"], "no copy left beside it");
        std::fs::remove_dir_all(&d).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_length_fix_and_its_undo_keep_the_file_s_rights() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("whole-rights");
        let track = d.join("track.mp3");
        bare_mp3(&track);
        std::fs::set_permissions(&track, std::fs::Permissions::from_mode(0o640)).unwrap();
        let mode = || std::fs::metadata(&track).unwrap().permissions().mode() & 0o7777;
        let mut f = whole(&track, "length");
        apply_whole(&mut f, &d.join("keep"), 0, |_| ()).unwrap();
        assert_eq!(mode(), 0o640);
        assert!(restore_whole(&track, f.whole.as_ref().unwrap()).unwrap());
        assert_eq!(mode(), 0o640);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_length_fix_that_fails_leaves_the_file_as_it_was_and_nothing_beside_it() {
        let d = dir("whole-fails");
        let (track, keep) = (d.join("track.mp3"), d.join("keep"));
        std::fs::write(&track, b"no frames here at all").unwrap();
        let mut f = whole(&track, "length");
        apply_whole(&mut f, &keep, 0, |_| panic!("nothing was made to tell of")).unwrap_err();
        assert_eq!(std::fs::read(&track).unwrap(), b"no frames here at all");
        assert_eq!(names_in(&d), ["keep", "track.mp3"]);
        assert!(names_in(&keep).is_empty(), "its copy kept for nothing goes");
        assert_eq!(f.whole.as_ref().unwrap().kept, None);
        std::fs::remove_dir_all(&d).ok();
    }

    fn tag_write(album: &str, path: &Path, after: &[&str]) -> FileWrite {
        FileWrite {
            album: album.into(),
            path: path.into(),
            track: None,
            before: vec![("releasetype".into(), Vec::new())],
            after: vec![("releasetype".into(), s(after))],
            from: Vec::new(),
            undone: false,
            whole: None,
        }
    }

    fn proposal(files: Vec<FileWrite>) -> Proposal {
        let records = ["a", "b"].map(|a| RunRecord { album: a.into(), title: a.to_uppercase(), artist: "X".into() }).to_vec();
        Proposal { kind: "types".into(), title: "Types".into(), records, files, failed: Vec::new() }
    }

    fn entry(p: &Proposal) -> (Arc<Journal>, RunEntry) {
        let j = Arc::new(Journal::open(None).unwrap());
        (j.clone(), RunEntry::begin(j, p, 100, None, 1000))
    }

    fn cut_albums(run: &FileRun) -> Vec<&str> {
        run.failed.iter().filter(|f| f.why == CUT).map(|f| f.album.as_str()).collect()
    }

    /// As a restart finds it: only the journal, nothing `start` still held.
    #[test]
    fn a_run_stopped_before_its_first_album_is_there_with_every_file_s_before() {
        let d = dir("run-entry-first");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let (j, e) = entry(&p);
        let (run, _, undone) = j.file_run(e.id.unwrap()).unwrap().unwrap();
        assert_eq!(run.files, p.files);
        assert_eq!(cut_albums(&run), ["a", "b"]);
        assert_eq!((run.failed[0].title.as_deref(), undone), (Some("A"), false));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_run_stopped_half_way_undoes_what_it_wrote_and_passes_what_it_did_not() {
        let d = dir("run-entry-half");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let (j, mut e) = entry(&p);
        let first = p.files[0].clone();
        apply_one(&first.path, &first.before, &first.after, "moved").unwrap();
        e.saved -= SAVE_EVERY;
        e.album_done("a", vec![first], &[]);

        let (run, ..) = j.file_run(e.id.unwrap()).unwrap().unwrap();
        assert_eq!(cut_albums(&run), ["b"], "the album finished is not said to be cut");
        assert_eq!(run.files.len(), 2, "the one not reached is in it all the same");
        let of = |path: &Path| run.files.iter().find(|f| f.path == path).unwrap();
        assert!(reached(of(&one)).unwrap() && !reached(of(&two)).unwrap(), "its page says what is so");
        assert!(restore_one(&one, &of(&one).after, &of(&one).before).unwrap(), "the file it wrote is put back");
        assert!(!restore_one(&two, &of(&two).after, &of(&two).before).unwrap(), "the one it did not reach is passed");
        assert!(holds(&fields_of(&one, &["releasetype"]).unwrap(), &of(&one).before));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A stop inside the second before the next save: the entry is behind, and the file's before is still there.
    #[test]
    fn an_album_not_saved_yet_can_still_be_undone() {
        let d = dir("run-entry-behind");
        let one = flac(&d, "one");
        let p = proposal(vec![tag_write("a", &one, &["ep"])]);
        let (j, mut e) = entry(&p);
        let first = p.files[0].clone();
        apply_one(&first.path, &first.before, &first.after, "moved").unwrap();
        e.album_done("a", vec![first], &[]);

        let (run, ..) = j.file_run(e.id.unwrap()).unwrap().unwrap();
        assert_eq!(cut_albums(&run), ["a"], "not saved within the second");
        assert!(restore_one(&one, &run.files[0].after, &run.files[0].before).unwrap());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_whole_file_is_in_the_entry_before_it_changes_and_not_before_it_is_reached() {
        let d = dir("run-entry-whole");
        let track = d.join("track.mp3");
        let before = bare_mp3(&track);
        let mut f = whole(&track, "length");
        f.album = "a".into();
        let p = proposal(vec![f.clone()]);
        let (j, mut e) = entry(&p);
        let id = e.id.unwrap();
        assert!(j.file_run(id).unwrap().unwrap().0.files.is_empty(), "not reached: nothing of it to undo");
        apply_whole(&mut f, &d.join("keep"), 0, |noted| {
            e.whole_file(noted);
            assert_eq!(std::fs::read(&track).unwrap(), before, "the entry first, the file after");
        })
        .unwrap();
        assert_eq!(j.file_run(id).unwrap().unwrap().0.files, [f], "saved at once, its kept copy and its print as written named");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A stop inside an album of whole files, as a restart finds it: `album_done` never came, and only the journal is left.
    #[test]
    fn a_stop_inside_an_album_of_whole_files_loses_the_undo_of_none_it_wrote() {
        let d = dir("run-entry-whole-cut");
        let tracks: Vec<PathBuf> = ["one", "two", "three"].map(|n| d.join(format!("{n}.mp3"))).to_vec();
        let originals: Vec<Vec<u8>> = tracks.iter().map(|t| bare_mp3(t)).collect();
        let files: Vec<FileWrite> = tracks.iter().map(|t| FileWrite { album: "a".into(), ..whole(t, "length") }).collect();
        let (j, mut e) = entry(&proposal(files.clone()));
        let id = e.id.unwrap();
        for (n, mut f) in files.into_iter().take(2).enumerate() {
            apply_whole(&mut f, &d.join("keep"), n, |noted| e.whole_file(noted)).unwrap();
        }

        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(cut_albums(&run), ["a"], "its album not finished with");
        assert_eq!(run.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), tracks[..2], "the two it wrote, and not the one it did not reach");
        assert!(run.files.iter().all(|f| reached(f).unwrap()), "its page says so");
        for f in &run.files {
            assert!(restore_whole(&f.path, f.whole.as_ref().unwrap()).unwrap());
        }
        assert_eq!(tracks.iter().map(|t| std::fs::read(t).unwrap()).collect::<Vec<_>>(), originals, "each as it was");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A stop in the moment between the entry and the rename: it names a file that has not changed.
    #[test]
    fn a_whole_file_stopped_before_it_took_the_file_s_place_is_said_not_written_and_its_undo_passes_it() {
        let d = dir("run-entry-whole-ahead");
        let track = d.join("track.mp3");
        bare_mp3(&track);
        let mut f = whole(&track, "length");
        let w = f.whole.as_mut().unwrap();
        (w.kept, w.wrote) = (Some(d.join("kept")), Some("what it would have been".into()));
        assert!(!reached(&f).unwrap());
        assert!(!restore_whole(&track, f.whole.as_ref().unwrap()).unwrap(), "as planned: nothing to put back");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_run_that_ended_says_only_what_it_wrote_and_what_it_set_aside() {
        let d = dir("run-entry-end");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let (j, mut e) = entry(&p);
        let id = e.id.unwrap();
        let aside = [RunFailed { album: "b".into(), title: None, artist: None, file: Some("two.flac".into()), why: "changed since it was planned".into() }];
        e.album_done("a", vec![p.files[0].clone()], &[]);
        e.album_done("b", Vec::new(), &aside);
        assert_eq!(e.end(), Some(format!("{RUN}{id}")));
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files, [p.files[0].clone()]);
        assert_eq!(run.failed, aside);
        assert_eq!(j.file_runs_since(0).unwrap().len(), 1, "on the list now");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_run_that_came_to_nothing_leaves_no_entry() {
        let p = proposal(vec![tag_write("a", Path::new("/m/a.flac"), &["ep"])]);
        let (j, mut e) = entry(&p);
        let id = e.id.unwrap();
        e.album_done("a", Vec::new(), &[]);
        assert_eq!(e.end(), None);
        assert_eq!(j.file_run(id).unwrap(), None);
        let (j, e) = entry(&proposal(Vec::new()));
        assert_eq!((e.id, j.file_runs_since(0).unwrap().len()), (None, 0), "nothing to write, nothing set aside: not even begun");
    }

    /// An `App` with no server, for what asks it: its journal, the list of runs.
    async fn app(tag: &str) -> Arc<crate::app::App> {
        let d = dir(tag);
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
        let saved = crate::settings::Saved { music_folder: Some(d.to_string_lossy().into_owned()), ..Default::default() };
        crate::app::App::new(cfg, saved, crate::settings::Origin::Env, None).await.unwrap()
    }

    #[tokio::test]
    async fn a_run_under_way_is_on_the_list_once_and_a_run_cut_off_is_on_it_with_its_undo() {
        let app = app("run-entry-list").await;
        let p = proposal(vec![tag_write("a", Path::new("/m/a.flac"), &["ep"])]);
        let e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let id = e.id.unwrap();
        assert!(runs(&app, 0).unwrap().iter().all(|s| s.id != format!("{RUN}{id}")), "told as it goes: its entry waits for its end");
        // A restart: nothing is writing, and the entry is all that is left of it.
        app.journal.run_ended(id);
        let cut = runs(&app, 0).unwrap().into_iter().find(|s| s.id == format!("{RUN}{id}")).unwrap();
        assert_eq!(cut.undo, Some(format!("{RUN}{id}")));
        assert_eq!(cut.failed.iter().map(|f| f.why.as_str()).collect::<Vec<_>>(), [CUT]);
    }

    /// Inside the second before the next save, so the entry still says the first album was not finished with.
    #[tokio::test]
    async fn a_run_cut_off_says_on_its_page_what_was_written_whatever_its_entry_got_to() {
        let app = app("run-entry-page").await;
        let d = dir("run-entry-page-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let mut e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let first = p.files[0].clone();
        apply_one(&first.path, &first.before, &first.after, "moved").unwrap();
        e.album_done("a", vec![first], &[]);

        let (_, sheet) = run(&app, &format!("{RUN}{}", e.id.unwrap())).await.unwrap();
        let written: Vec<(&str, Option<usize>)> = sheet.records.iter().map(|r| (r.album.as_str(), r.written)).collect();
        assert_eq!(written, [("a", Some(1)), ("b", Some(0))]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// `a`'s file written, `b`'s not, then a stop inside the second before the next save: as a restart finds it.
    fn cut_after_the_first(j: &Arc<Journal>, p: &Proposal) -> i64 {
        let mut e = RunEntry::begin(j.clone(), p, super::super::moment::now_secs(), None, 1000);
        let first = p.files[0].clone();
        apply_one(&first.path, &first.before, &first.after, "moved").unwrap();
        e.album_done("a", vec![first], &[]);
        let id = e.id.unwrap();
        j.run_ended(id);
        id
    }

    fn ep(path: &Path) -> bool {
        holds(&fields_of(path, &["releasetype"]).unwrap(), &[("releasetype".to_string(), s(&["ep"]))])
    }

    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        for _ in 0..1000 {
            if done() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("waited for {what}");
    }

    #[test]
    fn a_run_cut_off_keeps_to_the_files_it_wrote_and_is_read_once() {
        let d = dir("settle-once");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let j = Arc::new(Journal::open(None).unwrap());
        let id = cut_after_the_first(&j, &p);
        settle(&j).unwrap();
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files, [p.files[0].clone()]);
        assert!(run.planned.is_empty());
        assert_eq!(cut_albums(&run), ["a", "b"], "it still says it was cut off");

        // Read once: whatever comes to hold what it was to write from now on is not its doing.
        apply_one(&two, &p.files[1].before, &p.files[1].after, "moved").unwrap();
        settle(&j).unwrap();
        assert_eq!(j.file_run(id).unwrap().unwrap().0.files, [p.files[0].clone()]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// The finding's case, by the real write and the real Undo: a run stopped, the write tried again for what is left.
    #[tokio::test]
    async fn a_run_cut_off_claims_nothing_a_later_run_wrote_and_each_undoes_its_own() {
        let app = app("settle-later").await;
        let d = dir("settle-later-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let cut = cut_after_the_first(&app.journal, &p);

        start(&app, Proposal { kind: "settle-later".into(), ..proposal(vec![tag_write("b", &two, &["ep"])]) });
        until("the second run", || !busy("settle-later")).await;
        assert!(ep(&one) && ep(&two));
        let later = app.journal.file_runs_since(0).unwrap().into_iter().map(|(id, ..)| id).find(|id| *id != cut).unwrap();

        let (_, sheet) = run(&app, &format!("{RUN}{cut}")).await.unwrap();
        let written: Vec<(&str, Option<usize>)> = sheet.records.iter().map(|r| (r.album.as_str(), r.written)).collect();
        assert_eq!(written, [("a", Some(1)), ("b", Some(0))], "the second run's file is not said to be the first's");

        undo(&app, &format!("{RUN}{cut}"), None).await.unwrap();
        until("the first's Undo", || app.journal.file_run(cut).unwrap().unwrap().2).await;
        assert!(!ep(&one), "what it wrote goes back");
        assert!(ep(&two), "and what it did not write stays");

        undo(&app, &format!("{RUN}{later}"), None).await.unwrap();
        until("the second's Undo", || app.journal.file_run(later).unwrap().unwrap().2).await;
        assert!(!ep(&two), "the second run still undoes its own");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Read under the lock, after the Undo was asked for: its entry is shorter than the one the Undo was started from.
    #[tokio::test]
    async fn an_undo_asked_of_a_run_not_read_yet_puts_back_what_it_wrote() {
        let app = app("settle-undo").await;
        let d = dir("settle-undo-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        // The file it did not reach first: the one it wrote is the second of the entry until the entry is read.
        let p = proposal(vec![tag_write("a", &two, &["ep"]), tag_write("a", &one, &["ep"])]);
        let e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let id = e.id.unwrap();
        apply_one(&one, &p.files[1].before, &p.files[1].after, "moved").unwrap();
        app.journal.run_ended(id);

        undo(&app, &format!("{RUN}{id}"), None).await.unwrap();
        until("its Undo", || app.journal.file_run(id).unwrap().unwrap().2).await;
        assert!(!ep(&one));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A mount not up after a restart reads as files gone, and that says nothing of what the run wrote.
    #[test]
    fn a_file_that_cannot_be_read_stays_and_its_record_is_read_again_next_time() {
        let d = dir("settle-unread");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let j = Arc::new(Journal::open(None).unwrap());
        let id = cut_after_the_first(&j, &p);
        let away = d.join("away");
        std::fs::create_dir_all(&away).unwrap();
        for f in [&one, &two] {
            std::fs::rename(f, away.join(f.file_name().unwrap())).unwrap();
        }

        settle(&j).unwrap();
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files.len(), 2, "neither forgotten");
        assert_eq!(run.planned, ["a", "b"]);

        for f in [&one, &two] {
            std::fs::rename(away.join(f.file_name().unwrap()), f).unwrap();
        }
        settle(&j).unwrap();
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files, [p.files[0].clone()]);
        assert!(run.planned.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_run_finished_or_part_undone_is_not_read_for_what_it_reached() {
        let d = dir("settle-kept");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let j = Arc::new(Journal::open(None).unwrap());

        // Finished with: its entry says what it wrote, whatever the file holds now.
        let p = proposal(vec![tag_write("a", &one, &["ep"])]);
        let mut e = RunEntry::begin(j.clone(), &p, 100, None, 1000);
        e.album_done("a", p.files.clone(), &[]);
        e.end();
        settle(&j).unwrap();
        assert_eq!(j.file_run(e.id.unwrap()).unwrap().unwrap().0.files.len(), 1);

        // A file its Undo took back holds its before: still in the entry, undone.
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let id = cut_after_the_first(&j, &p);
        let (mut run, ..) = j.file_run(id).unwrap().unwrap();
        run.files[0].undone = true;
        j.update_run(id, &run, None).unwrap();
        assert!(restore_one(&one, &p.files[0].after, &p.files[0].before).unwrap());
        settle(&j).unwrap();
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files.iter().map(|f| (f.path.clone(), f.undone)).collect::<Vec<_>>(), [(one.clone(), true)]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_run_of_whole_files_cut_off_keeps_what_changed_and_a_cover_however_far_it_got() {
        let d = dir("settle-whole");
        let (done, ahead) = (d.join("done.mp3"), d.join("ahead.mp3"));
        bare_mp3(&done);
        bare_mp3(&ahead);
        let mut changed = FileWrite { album: "a".into(), ..whole(&done, "length") };
        let f = FileWrite { album: "a".into(), ..whole(&ahead, "length") };
        let cover = FileWrite { album: "b".into(), ..whole(&done, "cover") };
        let (j, mut e) = entry(&proposal(vec![changed.clone(), f.clone(), cover.clone()]));
        apply_whole(&mut changed, &d.join("keep"), 0, |noted| e.whole_file(noted)).unwrap();
        e.whole_file(&f);
        e.whole_file(&cover);
        let id = e.id.unwrap();
        j.run_ended(id);

        settle(&j).unwrap();
        let (run, ..) = j.file_run(id).unwrap().unwrap();
        assert_eq!(run.files, [changed, cover], "not the one still as planned");
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

    /// One track, two files: a change they share is one row, as the sheet shows a track. Where they differ it cannot be, and
    /// each row says its library, or the two read the same.
    #[test]
    fn the_sheet_says_a_track_once_and_names_the_library_where_its_copies_differ() {
        let file = |path: &str, track: Option<&str>, before: &str| FileWrite {
            album: "a".into(),
            path: path.into(),
            track: track.map(str::to_string),
            before: vec![("bpm".into(), s(&[before]))],
            after: vec![("bpm".into(), s(&["60"]))],
            from: vec![("bpm".into(), "halved".into())],
            undone: false,
            whole: None,
        };
        let records = [RunRecord { album: "a".into(), title: "A".into(), artist: "B".into() }];
        let roots = [PathBuf::from("/music"), PathBuf::from("/incoming")];
        let rows = |files: &[FileWrite]| -> Vec<(Option<String>, Option<String>, Vec<String>)> {
            let v = view_in("tempo", "Tempo", &records, files, false, &roots);
            assert_eq!(v.records[0].files, files.len(), "what changes is still every file");
            v.records[0].fields.iter().map(|f| (f.track.clone(), f.library.clone(), f.before.clone())).collect()
        };
        let one = || Some("One".to_string());
        let (music, incoming) = (Some("/music".to_string()), Some("/incoming".to_string()));
        let files = [file("/music/1.flac", Some("One"), "120"), file("/incoming/1.mp3", Some("One"), "120"), file("/music/2.flac", Some("Two"), "100"), file("/incoming/2.mp3", Some("Two"), "100")];
        assert_eq!(rows(&files), [(one(), None, s(&["120"])), (Some("Two".to_string()), None, s(&["100"]))]);
        let files = [file("/music/1.flac", Some("One"), "120"), file("/incoming/1.mp3", Some("One"), "118")];
        assert_eq!(rows(&files), [(one(), music.clone(), s(&["120"])), (one(), incoming.clone(), s(&["118"]))]);
        // A cover, on no track: two copies' folders.
        let files = [file("/music/A", None, "its cover"), file("/incoming/A", None, "none")];
        assert_eq!(rows(&files), [(None, music, s(&["its cover"])), (None, incoming, s(&["none"]))]);
        // Two tracks of one name in one library: no library tells them apart.
        let files = [file("/music/1.flac", Some("One"), "120"), file("/music/9.flac", Some("One"), "118")];
        assert_eq!(rows(&files), [(one(), None, s(&["120"])), (one(), None, s(&["118"]))]);
    }

    /// A stop between two copies: the first is in the entry with its Undo, and the second, not begun, is in neither.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cover_stopped_between_its_copies_undoes_the_copy_it_reached() {
        use super::super::every_copy::{picture, two_copies};
        let c = two_copies("cover-cut", true);
        let p = super::super::covers::proposal(&c.serve, &c.album, &picture(200), "uploaded").unwrap();
        let (_j, mut e) = entry(&p);
        assert!(e.run.files.is_empty(), "a cover is in it on its way, not before");
        let mut first = p.files[0].clone();
        apply_whole(&mut first, Path::new("unused"), 0, |f| e.whole_file(f)).unwrap();
        assert_eq!(e.run.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), [first.path.clone()]);
        assert!(reached(&e.run.files[0]).unwrap());
        assert!(first.path.join("cover.jpg").exists());
        let second = &p.files[1].path;
        assert!(!second.join("cover.jpg").exists(), "the second not begun");
        restore_whole(&first.path, first.whole.as_ref().unwrap()).unwrap();
        assert!(!first.path.join("cover.jpg").exists(), "the copy it reached put back");
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

    /// An album action's page (decision 0092) holds its record with every file it moved, as the page shows a record and
    /// its Undo only for what was written. An add to someone's own music opened empty in 0.16.0: its record said none.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_album_actions_page_holds_its_record_with_every_file_it_moved() {
        use crate::journal::{Event, TrackMove};
        let app = super::super::every_copy::app("album-action-page").await;
        let root = app.library.catalogue().folders().get(0).cloned().expect("the music folder in use");
        let moved = |n: usize| (0..n).map(|i| TrackMove { track: Some(i as i64 + 1), from: (0, format!("in/Drift/{i}.flac")), to: (0, format!("A/Drift/{i}.flac")), was: None }).collect::<Vec<_>>();
        let adopted = Event::AlbumAdopted {
            source: "cratarr".into(),
            album_id: "al-1".into(),
            title: "Drift".into(),
            from_dir: (0, "in/Drift".into()),
            to_dir: (0, "A/Drift".into()),
            moves: moved(3),
            release_group: None,
            converted: Vec::new(),
        };
        let replaced = Event::AlbumReplaced {
            source: "cratarr".into(),
            album_id: "al-2".into(),
            title: "Drift".into(),
            from_dir: (0, "in/Drift".into()),
            to_dir: (0, "A/Drift".into()),
            moves: moved(2),
            replaced: moved(2),
            token: "1/A/Drift".into(),
            release_group: None,
            converted: Vec::new(),
        };
        let gone = root.join("in/Gone");
        std::fs::create_dir_all(gone.join("CD 2")).unwrap();
        for f in ["1.flac", "2.flac", "cover.jpg", "CD 2/1.flac"] {
            std::fs::write(gone.join(f), b"x").unwrap();
        }
        let token = crate::trash::trash(&root, "in/Gone", 1).unwrap();
        let rejected = Event::AlbumRejected { source: "cratarr".into(), album_id: "al-3".into(), title: "Gone".into(), folder: 0, dir: "in/Gone".into(), token, release_group: None };
        for (event, files) in [(adopted, 3), (replaced, 2), (rejected, 4)] {
            let id = app.journal.intend(1, &event).unwrap();
            app.journal.applied(id).unwrap();
            let (status, sheet) = run(&app, &format!("{MOVE}{id}")).await.unwrap();
            assert_eq!(status.undo, Some(format!("{MOVE}{id}")), "{}", status.title);
            let records: Vec<(usize, Option<usize>)> = sheet.records.iter().map(|r| (r.files, r.written)).collect();
            assert_eq!(records, [(files, Some(files))], "{}", status.title);
        }
    }
}
