//! Soulseek in Cratarr (0.15, the owner, 2026-11-02: "Cratarr does Soulseek, Lidarr stays"): soularr goes; Cratarr
//! fetches whole records from Soulseek its own way. Lidarr and Soulseek are sibling fetchers, either or both (the
//! owner, 2026-11-03): Lidarr fetches its own Wanted through its own downloaders.
//!
//! - **What is wanted:** Cratarr's approved Requests, and only those (the owner, 2026-11-03: "The whole idea is that
//!   Cratarr's Soulseek doesn't need Lidarr"); read each round. One taken off the list stays off.
//! - **Judged by its list:** each request's record is judged by its counting list (`references::tracklist`, Q31): for a record
//!   not had, its references' in order, MusicBrainz's release (or one of its release group's), Discogs', Bandcamp's album. A
//!   Soulseek folder is a candidate only when every track of the list is one of its files, by title and length (`folders_of`),
//!   never because its name looks right. A list without its tracks' lengths gives way to the next in the order that has them;
//!   where no list with lengths finds a folder in any rung of a search, the same answers are judged by it, by titles and count,
//!   each such folder said to have no lengths to check beside its tick (R33, the owner's: `untimed_folders`), and gated by
//!   that list. A record with no list to judge by is Stuck, saying why; so is one that even its list without lengths finds no
//!   folder for, by titles, in a search every rung of which answered.
//! - **Searched on a ladder** (`ladder`): the record and its year, its title stripped of what is in brackets, the main
//!   artist only, then the artist with a wildcard for the artists Soulseek's server filters; none twice; one search at a
//!   time, paced. Not found, it is searched again later (`wait_after`): at once after none or one miss, four hours after
//!   two, a day after three, a week after four or more, never given up; Search now overrides the wait.
//! - **Fetched only with a tick:** the administrator approves many at once; nothing is fetched without it (March
//!   2026's unattended batches cannot happen again). The folder's tracks come in one batch from its user.
//! - **The next folder when a user fails** (the owner, 2026-11-03): refused, offline, an error, or queued over 30
//!   minutes; the user is remembered as failed for the record, and the next best folder is fetched without asking
//!   again. A read of slskd that fails once is not the end of a fetch, and slskd itself away is no failed user: the
//!   record stays as it is, a fetch under way keeps its batch, and it is taken up again (`hold`).
//! - **Every file gated** (single-track intake's gate, 0080: decoded whole, the recording's length, its sound, AcoustID; a list
//!   with no recordings, Discogs' or Bandcamp's, by its titles and lengths, AcoustID's check skipped: OQ9 (a), Q29).
//!   A folder with a file that fails is set aside whole with why per file (the owner, 2026-11-03), its user remembered,
//!   and the next folder tried. Only a whole, checked record goes on. AcoustID that could not be asked is no failed file:
//!   the record is held at its check with its folder, its tick and its user, and asked about again (`hold`).
//! - **Into its own Incoming library** (the owner, 2026-11-03): the Incoming library whose fetcher is "soulseek"
//!   (decision 0096), as Artist/Record (Year), for the intake card. Never straight into Music.
//! - **Kept on disk** (`soulseek.json` in the data folder), so a restart forgets nothing: a fetch under way is followed
//!   again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::track_fetch::{NotAsked, ReleaseTrack};
use crate::slskd::{Answer, Away, Offered, Slskd};

/// The fetcher's name in a library's setting (`settings::LibrarySetting::fetcher`).
pub const FETCHER: &str = "soulseek";
/// Between two searches: gentle with Soulseek.
const PACE: Duration = Duration::from_secs(if cfg!(test) { 0 } else { 5 });
/// A search is followed this long at most.
const SEARCH_AT_MOST: Duration = Duration::from_secs(120);
/// A fetch nobody's queue starts in this long is given up on that user.
const QUEUED_FOR: Duration = Duration::from_secs(30 * 60);
/// A whole fetch is given up after this long (a slow user).
const FETCH_AT_MOST: Duration = Duration::from_secs(4 * 3600);
/// Reads of slskd that may fail in a row before a fetch is given up.
const READS: u32 = 15;
/// Between two such reads.
const READ_AGAIN: Duration = Duration::from_secs(if cfg!(test) { 0 } else { 4 });
/// A record held for slskd is tried again after this long.
const SLSKD_AGAIN: i64 = 300;
/// Folders a record shows, best first.
const SHOWN: usize = 5;
/// The most approved Requests read.
const REQUESTS_AT_MOST: usize = 1000;
/// The shortest wait between two rounds of searches for one record (the ladder's "at once" still waits this).
const ROUND_AT_LEAST: i64 = 15 * 60;
/// A file's length may be this far from its track's.
const LENGTH_SLACK_MS: i64 = crate::slskd::LENGTH_SLACK_MS;
/// A record held at its check is checked again after this long: each time decodes a file to ask AcoustID about it.
const CHECK_AGAIN: i64 = 15 * 60;

// ---- What is kept

/// A record wanted: the catalogue's (C51b), named as the record is, for the searches and the folder it is put in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// 0 for an item 0.16 kept, which named its release group instead: given its request's record when the requests are read.
    #[serde(default)]
    pub record: i64,
    pub artist: String,
    pub album: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// The approved Request it fetches (decision 0099's amendment, the owner 2026-11-03: Soulseek fetches only Cratarr's
    /// approved Requests; Lidarr its own Wanted, through its own downloaders).
    pub request: i64,
}

/// One of the list's tracks, as a folder's files are judged by: the list's own shape (C51b), a recording only where
/// MusicBrainz's list gives one. A track 0.16 kept reads as one, with its recording.
pub type Track = crate::catalogue::identity::RefTrack;

/// A folder of one user's that holds every track of the release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub user: String,
    /// The folder as the user shares it.
    pub dir: String,
    /// The format most of its tracks are in.
    pub format: String,
    pub lossless: bool,
    pub free_slot: bool,
    pub queue: i64,
    pub speed: i64,
    /// Audio files of the folder that are none of the release's tracks.
    pub extra: usize,
    /// Each track's file, in the release's order.
    pub files: Vec<Offered>,
    /// What could not be checked, said beside its tick: a folder found by titles alone, by a list without lengths, where no list
    /// with them found one (R33, `NO_LENGTHS`). Its files are gated by that list (`judged_by`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchecked: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// To be searched (when due).
    Waiting,
    Searching,
    /// Folders found: waits for the administrator's tick.
    Found,
    /// Approved: to be fetched.
    Approved,
    Fetching,
    Checking,
    /// In its Incoming library.
    Placed,
    /// Something the record cannot get past by itself (no release, nowhere to put it): why says.
    Stuck,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub record: Record,
    pub state: State,
    /// The list folders are judged by, by titles and lengths: the record's first with every track's length (R33).
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// The record's list without lengths, read before that one in the design's order, or where it has none (R33): a search's
    /// answers are judged by it, by titles and count, only where `tracks` finds no folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub untimed: Vec<Track>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// The folder approved (of `folders`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<usize>,
    /// Users who failed this record: never asked for it again.
    #[serde(default)]
    pub failed: Vec<String>,
    /// Rounds of searches that found nothing in a row.
    #[serde(default)]
    pub misses: u32,
    /// When it is searched again (epoch s); 0 or unreadable: due. Held (`hold`), when it is taken up again.
    #[serde(default)]
    pub due: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The fetch under way: slskd's batch, and how far (0 to 100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<String>,
    #[serde(default)]
    pub percent: f64,
    /// Where it was placed, in its Incoming library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placed: Option<String>,
    /// The folders set aside for it, with why per file.
    #[serde(default)]
    pub set_aside: Vec<SetAside>,
}

/// A folder set aside: whose, where it is kept, and why each of its files failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetAside {
    pub user: String,
    pub path: String,
    pub why: BTreeMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    next: u64,
    items: Vec<Item>,
    /// Requests taken off the list: not taken in again.
    #[serde(default)]
    dropped: Vec<i64>,
    /// A file that could not be read nor set aside: nothing is written over it (R24).
    #[serde(skip)]
    frozen: bool,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
/// Where it is kept: the data folder's `soulseek.json`, once known.
static FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn kept<T>(f: impl FnOnce(&mut Kept) -> T) -> T {
    let mut g = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let k = g.get_or_insert_with(|| FILE.get().map(|p| load(p)).unwrap_or_default());
    let out = f(k);
    if let Some(p) = FILE.get()
        && !k.frozen
        && let Ok(b) = serde_json::to_vec(k)
    {
        let part = p.with_extension("json.part");
        if std::fs::write(&part, b).and_then(|_| std::fs::rename(&part, p)).is_err() {
            tracing::warn!(target: "soulseek", file = %p.display(), "the Soulseek queue could not be kept");
        }
    }
    out
}

/// The queue as the data folder keeps it (`soulseek.json`); none yet, an empty one (R24, N4). One 0.16 wrote, its items naming
/// their release group, is copied once to `soulseek.json.016` before 0.17 first writes over it, so a rollback finds it. One that
/// cannot be read is set aside whole (`soulseek.json.unreadable-<time>`) and said in the log as an error, never emptied; where it
/// cannot be set aside either, nothing is written to it.
fn load(p: &Path) -> Kept {
    let bytes = match std::fs::read(p) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Kept::default(),
        Err(e) => return set_aside_unreadable(p, &format!("{e}")),
    };
    match serde_json::from_slice::<Kept>(&bytes) {
        Ok(k) => {
            let of_016 = serde_json::from_slice::<serde_json::Value>(&bytes).ok().is_some_and(|v| v["items"].as_array().is_some_and(|items| items.iter().any(|i| i["record"].get("release_group").is_some())));
            let old = p.with_extension("json.016");
            if of_016
                && !old.exists()
                && let Err(e) = std::fs::copy(p, &old)
            {
                tracing::error!(target: "soulseek", file = %old.display(), reason = %e, "0.16's Soulseek queue could not be kept aside");
            }
            k
        }
        Err(e) => set_aside_unreadable(p, &format!("{e}")),
    }
}

/// An unreadable queue set aside under its time (`load`), a new one begun in its place.
fn set_aside_unreadable(p: &Path, why: &str) -> Kept {
    let aside = p.with_extension(format!("json.unreadable-{}", now()));
    match std::fs::rename(p, &aside) {
        Ok(()) => {
            tracing::error!(target: "soulseek", file = %p.display(), aside = %aside.display(), reason = why, "the Soulseek queue could not be read: set aside, a new one begun");
            Kept::default()
        }
        Err(e) => {
            tracing::error!(target: "soulseek", file = %p.display(), reason = why, error = %e, "the Soulseek queue could not be read nor set aside: nothing is written to it");
            Kept { frozen: true, ..Kept::default() }
        }
    }
}

/// The record an item is placed for, as the list holds it now (R24, N5), not as it was when its fetch began: an item 0.16 kept is
/// given its record by the first round of requests, which can come while its first fetch at the upgrade start runs.
fn record_now(k: &Kept, id: &str, was: &Record) -> Record {
    k.items.iter().find(|i| i.id == id).map_or_else(|| was.clone(), |i| i.record.clone())
}

fn with_item(id: &str, f: impl FnOnce(&mut Item)) {
    kept(|k| {
        if let Some(i) = k.items.iter_mut().find(|i| i.id == id) {
            f(i);
        }
    });
}

fn item(id: &str) -> Option<Item> {
    kept(|k| k.items.iter().find(|i| i.id == id).cloned())
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

// ---- The ladders

/// How long a record not found waits before it is searched again, after `misses` rounds in a row found nothing: none
/// after none or one (but `ROUND_AT_LEAST`), four hours after two, a day after three, a week after four and more.
pub fn wait_after(misses: u32) -> i64 {
    match misses {
        0 | 1 => ROUND_AT_LEAST,
        2 => 4 * 3600,
        3 => 86_400,
        _ => 7 * 86_400,
    }
}

/// Words as Soulseek's search takes them: letters and digits, punctuation only hinders.
fn words(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A title without what is in brackets ("Moon Safari (Deluxe Edition)" → "Moon Safari").
fn stripped(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// The first artist credited ("Air feat. Beck", "Air & Beck", "Air, Beck", "Air / Beck" → "Air").
pub fn main_artist(artist: &str) -> String {
    // ASCII only: a letter that grows when lowered would move the cut off the artist's own bytes.
    let lower = artist.to_ascii_lowercase();
    let cut = [" feat.", " feat ", " ft.", " featuring ", " & ", " and ", ", ", " / ", " x ", " vs. ", " vs ", " with "]
        .iter()
        .filter_map(|s| lower.find(s))
        .min()
        .unwrap_or(artist.len());
    artist[..cut].trim().to_string()
}

/// The searches for a record, in turn, none twice: the record and its year; its title stripped; the main artist only;
/// then a wildcard for the artist's first letter (Soulseek's server leaves some artists' names out of every search).
pub fn ladder(r: &Record) -> Vec<String> {
    let (artist, title) = (words(&r.artist), words(&stripped(&r.album)));
    let main = words(&main_artist(&r.artist));
    let mut out: Vec<String> = Vec::new();
    let mut add = |q: String| {
        if !q.trim().is_empty() && !out.contains(&q) {
            out.push(q);
        }
    };
    add(match r.year {
        Some(y) => format!("{} {} {y}", artist, words(&r.album)),
        None => format!("{} {}", artist, words(&r.album)),
    });
    add(format!("{artist} {title}"));
    add(format!("{main} {title}"));
    let mut chars = main.chars();
    if let Some(first) = chars.next()
        && main.chars().count() > 2
        && first.is_alphanumeric()
    {
        add(format!("*{} {title}", chars.as_str()));
    }
    out
}

// ---- Judging folders

/// A file's title from its name: without the extension, the track number in front, or "Artist - " (each way tried).
fn titles_of(name: &str) -> Vec<String> {
    let base = crate::slskd::base_name(name);
    let base = base.rsplit_once('.').map_or(base, |(b, _)| b);
    let no_number = base.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '-' | '.' | '_' | ' ' | ')')).trim();
    let mut out = vec![base.trim().to_string(), no_number.to_string()];
    for s in [base, no_number] {
        if let Some((_, after)) = s.split_once(" - ") {
            out.push(after.trim().to_string());
        }
        if let Some((_, after)) = s.rsplit_once(" - ") {
            out.push(after.trim().to_string());
        }
    }
    out.dedup();
    out
}

fn format_of(f: &Offered) -> String {
    f.extension.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| crate::slskd::base_name(&f.filename).rsplit('.').next().unwrap_or("").to_string()).to_lowercase()
}

/// The folders among a search's answers that hold every track of the release, each track's file by its title and the
/// closest length (within the slack, where both are known), in the release's order. Files are FLAC or MP3 (the library's
/// formats, which Cratarr writes tags into), never locked; users in `failed` are left out. Best first: lossless, a free
/// slot, a short queue, fewest extra files, the fastest.
pub fn folders_of(answers: &[Answer], tracks: &[Track], failed: &[String]) -> Vec<Folder> {
    let mut out = Vec::new();
    if tracks.is_empty() {
        return out;
    }
    for a in answers.iter().filter(|a| !failed.contains(&a.username)) {
        let mut dirs: BTreeMap<String, Vec<&Offered>> = BTreeMap::new();
        for f in a.files.iter().filter(|f| !f.is_locked && matches!(format_of(f).as_str(), "flac" | "mp3")) {
            let dir = f.filename.rsplit_once(['\\', '/']).map_or("", |(d, _)| d).to_string();
            dirs.entry(dir).or_default().push(f);
        }
        'dir: for (dir, files) in dirs {
            if files.len() < tracks.len() {
                continue;
            }
            let mut used = vec![false; files.len()];
            let mut picked = Vec::with_capacity(tracks.len());
            for t in tracks {
                let best = files
                    .iter()
                    .enumerate()
                    .filter(|(i, f)| {
                        !used[*i]
                            && titles_of(&f.filename).iter().any(|n| super::renumber::same_title(n, &t.title))
                            && match (t.length_ms, f.length) {
                                (Some(want), Some(secs)) => (want - secs * 1000).abs() <= LENGTH_SLACK_MS,
                                _ => true,
                            }
                    })
                    .min_by_key(|(_, f)| match (t.length_ms, f.length) {
                        (Some(want), Some(secs)) => (want - secs * 1000).abs(),
                        _ => i64::MAX,
                    });
                let Some((i, f)) = best else { continue 'dir };
                used[i] = true;
                picked.push((*f).clone());
            }
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for f in &picked {
                *counts.entry(format_of(f)).or_default() += 1;
            }
            let format = counts.into_iter().max_by_key(|(_, n)| *n).map(|(f, _)| f).unwrap_or_default();
            out.push(Folder {
                user: a.username.clone(),
                dir,
                lossless: format == "flac",
                format,
                free_slot: a.has_free_upload_slot,
                queue: a.queue_length,
                speed: a.upload_speed,
                extra: files.len() - picked.len(),
                files: picked,
                unchecked: None,
            });
        }
    }
    out.sort_by(|a, b| {
        b.lossless
            .cmp(&a.lossless)
            .then(b.free_slot.cmp(&a.free_slot))
            .then(a.queue.cmp(&b.queue))
            .then(a.extra.cmp(&b.extra))
            .then(b.speed.cmp(&a.speed))
    });
    out
}

/// What a folder found by titles alone says beside its tick (R33).
const NO_LENGTHS: &str = "No track lengths to check";

/// The folders among a search's answers that hold every track of a list without lengths (R33, the owner's fallback where no list
/// with them finds one): each track's file by its title alone, and no other audio file, so its count is the list's. Each says it
/// has no lengths to check. Those whose files' lengths agree with the most others, track by track, come first (`agreeing`), then
/// as `folders_of` ranks.
pub fn untimed_folders(answers: &[Answer], tracks: &[Track], failed: &[String]) -> Vec<Folder> {
    let titles: Vec<Track> = tracks.iter().map(|t| Track { length_ms: None, ..t.clone() }).collect();
    let found: Vec<Folder> = folders_of(answers, &titles, failed).into_iter().filter(|f| f.extra == 0).map(|f| Folder { unchecked: Some(NO_LENGTHS.into()), ..f }).collect();
    let mut ranked: Vec<(usize, Folder)> = agreeing(&found).into_iter().zip(found).collect();
    ranked.sort_by(|(a, _), (b, _)| b.cmp(a));
    ranked.into_iter().map(|(_, f)| f).collect()
}

/// How many of the other folders each one's files agree with, track by track, in length (within the slack; a file of no known
/// length agrees with none): folders found by titles alone are ranked by it, an edition several users share being likelier the
/// record's than one alone. Pure.
fn agreeing(folders: &[Folder]) -> Vec<usize> {
    let alike = |a: &Folder, b: &Folder| {
        a.files.len() == b.files.len() && a.files.iter().zip(&b.files).all(|(x, y)| matches!((x.length, y.length), (Some(p), Some(q)) if (p - q).abs() * 1000 <= LENGTH_SLACK_MS))
    };
    folders.iter().enumerate().map(|(i, a)| folders.iter().enumerate().filter(|(j, b)| *j != i && alike(a, b)).count()).collect()
}

/// The list a folder's files are gated by: the one it was found by (R33), without lengths for one found by titles alone.
fn judged_by<'a>(it: &'a Item, f: &Folder) -> &'a [Track] {
    if f.unchecked.is_some() { &it.untimed } else { &it.tracks }
}

/// After a folder's user failed (`user`): the record's next folder, approved by the same tick, else searched again at
/// once without that user. Returns whether there is a next folder.
fn on_failed(i: &mut Item, user: &str, why: String) -> bool {
    if !i.failed.iter().any(|u| u == user) {
        i.failed.push(user.to_string());
    }
    i.folders.retain(|f| !i.failed.contains(&f.user));
    (i.batch, i.percent, i.why) = (None, 0.0, Some(why));
    if i.folders.is_empty() {
        (i.state, i.chosen, i.due) = (State::Waiting, None, 0);
        false
    } else {
        (i.state, i.chosen) = (State::Approved, Some(0));
        true
    }
}

/// A record held: what had to be asked could not be, which says nothing of its folder or its user. It stays as it is, its
/// tick, its folders and its batch with it, and is taken up again at `due`; `why` says what it waits for. Not Waiting,
/// which is the search's: searched again it would be Found, asking for a tick, and what it fetched would lie forgotten.
fn hold(i: &mut Item, why: String, wait: i64) {
    (i.why, i.due) = (Some(why), now() + wait);
}

/// Whether its fetch is at work: one held at its check is not, until it is due.
fn fetched(i: &Item, now: i64) -> bool {
    match i.state {
        State::Fetching => true,
        State::Checking => i.due <= now,
        _ => false,
    }
}

// ---- The service

/// What the page shows: every record, the running worker's state.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub items: Vec<Item>,
    /// The library records go into (its folder), where one is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

pub fn view(app: &Arc<super::App>) -> View {
    start(app);
    View { items: kept(|k| k.items.clone()), library: library().ok().map(|p| p.display().to_string()) }
}

/// These records approved, each with the folder given (else the best); the queue as it is now.
pub fn approve(app: &Arc<super::App>, picked: &[(String, Option<usize>)]) -> View {
    kept(|k| {
        for (id, folder) in picked {
            if let Some(i) = k.items.iter_mut().find(|i| i.id == *id && i.state == State::Found) {
                let f = folder.unwrap_or(0);
                if f < i.folders.len() {
                    (i.state, i.chosen, i.why) = (State::Approved, Some(f), None);
                }
            }
        }
    });
    wake();
    view(app)
}

/// A record searched now, whatever its wait (and from the ladder's first rung).
pub fn search_now(app: &Arc<super::App>, id: &str) -> View {
    with_item(id, search_again);
    wake();
    view(app)
}

/// An item searched again by hand (Search now): due now, its word gone, and its list read again, as its record counts now (R25,
/// N7: a list chosen by hand since reaches it; the folders found by the old one go with it). One in flight is left as it is.
fn search_again(i: &mut Item) {
    if matches!(i.state, State::Waiting | State::Found | State::Stuck) {
        (i.state, i.due, i.why, i.chosen) = (State::Waiting, 0, None, None);
        i.tracks.clear();
        i.untimed.clear();
        i.folders.clear();
    }
}

/// A record taken off the list (not while it is fetched): its Request is not taken in again. What it fetched goes with
/// it: nothing else looks in a batch's folder.
pub async fn drop_item(app: &Arc<super::App>, id: &str) -> View {
    if let Some(batch) = dropped(id, now())
        && let Ok((_, folder)) = super::track_fetch::slskd_ready().await
    {
        std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(batch)).ok();
    }
    view(app)
}

/// The record off the list, unless it is fetched; its batch, where it has one.
fn dropped(id: &str, now: i64) -> Option<String> {
    kept(|k| {
        let at = k.items.iter().position(|i| i.id == id && !fetched(i, now))?;
        let i = k.items.remove(at);
        k.dropped.push(i.record.request);
        i.batch
    })
}

/// The placed records cleared from the list.
pub fn clear_placed(app: &Arc<super::App>) -> View {
    kept(|k| k.items.retain(|i| i.state != State::Placed));
    view(app)
}

/// The Incoming library whose fetcher is Soulseek (its folder, as Cratarr sees it).
fn library() -> Result<PathBuf> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let dir = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    let saved = crate::settings::load(&dir.join("settings.json"), &|k: &str| std::env::var(k).ok())?.context("Nothing is set up yet")?;
    library_in(&saved).context("No Incoming library takes Soulseek's records: choose one in its settings")
}

/// `library` in these settings.
fn library_in(saved: &crate::settings::Saved) -> Option<PathBuf> {
    let c = saved.sources.iter().find(|c| c.enabled && c.kind == crate::source::cratarr::KIND.id)?;
    c.libraries.fetched_by(FETCHER).map(PathBuf::from)
}

static WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// The searcher woken (a Request approved: taken in now, not at the next round).
pub fn wake() {
    WAKE.notify_waiters();
}

/// Whether Soulseek fetches here: slskd set up and switched on (Settings › Server), as single-track intake's.
pub async fn connected() -> bool {
    super::track_fetch::slskd_ready().await.is_ok()
}

/// The workers started, once: one searches (one search at a time), one fetches (one record at a time).
pub fn start(_app: &Arc<super::App>) {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let Some(served) = crate::serve::instance() else { return };
    let Some(dir) = served.data_dir.clone() else { return };
    let _ = FILE.set(dir.join("soulseek.json"));
    if STARTED.set(()).is_err() {
        return;
    }
    // What a restart left half done: a search starts again, a fetch is followed again.
    kept(|k| {
        for i in k.items.iter_mut().filter(|i| i.state == State::Searching) {
            i.state = State::Waiting;
        }
    });
    tokio::spawn(async move {
        loop {
            // Nothing searched until an Incoming library takes Soulseek's records (soularr may still be at work).
            if library().is_err() {
                let _ = tokio::time::timeout(Duration::from_secs(600), WAKE.notified()).await;
                continue;
            }
            // The approved Requests, read each round (they are Cratarr's own: no service asked).
            if let Err(e) = read_requests(&served).await {
                tracing::warn!(target: "soulseek", reason = %format!("{e:#}"), "the approved Requests not read");
            }
            match next_search() {
                Some(id) => {
                    search(&id).await;
                    tokio::time::sleep(PACE).await;
                }
                None => {
                    let _ = tokio::time::timeout(Duration::from_secs(60), WAKE.notified()).await;
                }
            }
        }
    });
    tokio::spawn(async move {
        loop {
            match next_fetch() {
                Some(id) => fetch(&id).await,
                None => {
                    let _ = tokio::time::timeout(Duration::from_secs(60), WAKE.notified()).await;
                }
            }
        }
    });
}

/// The approved Requests taken in (decision 0099's amendment: Soulseek never reads Lidarr's Wanted), each by its record
/// (C51b, `requests::records_of`): one whose record is not on the list yet is added, waiting to be searched; one no longer
/// approved (cancelled, declined) goes, unless it is fetched or placed.
async fn read_requests(served: &Arc<crate::serve::Serve>) -> Result<()> {
    let (ud, cat) = (served.userdata.clone(), served.catalogue().clone());
    let (approved, of) = tokio::task::spawn_blocking(move || -> Result<_> {
        let approved = ud.requests(None, Some(super::requests::APPROVED), REQUESTS_AT_MOST)?;
        let of = super::requests::records_of(&cat, &approved)?;
        Ok((approved, of))
    })
    .await??;
    kept(|k| take_requests(k, &approved, &of));
    Ok(())
}

/// [`read_requests`]'s list kept, each request's record as `records_of` read it (`of`, in the requests' order): pure. Every
/// record is taken, with a release group or none: it is judged by its list, whosever that is (0100). An item 0.16 kept, by its
/// release group, is given its request's record (row 9.26) and goes on as it was: a fetch under way keeps its batch, its
/// folder and its tick, and is written back with its record.
fn take_requests(k: &mut Kept, approved: &[crate::userdata::Request], of: &[Option<super::requests::RecordOf>]) {
    let ids: std::collections::HashSet<i64> = approved.iter().map(|r| r.id).collect();
    k.items.retain(|i| ids.contains(&i.record.request) || matches!(i.state, State::Approved | State::Fetching | State::Checking | State::Placed));
    k.dropped.retain(|r| ids.contains(r));
    for i in k.items.iter_mut().filter(|i| i.record.record == 0) {
        if let Some((_, Some(o))) = approved.iter().zip(of).find(|(r, _)| r.id == i.record.request) {
            i.record.record = o.record;
        }
    }
    // A record already had, by its own files or a record holding them (the one had rule, R26, N9), is not searched: an item of it
    // not in flight goes, and none is made for it (Clear placed, or Lidarr's delivery, is no reason to fetch it again).
    let had: std::collections::HashSet<i64> = of.iter().flatten().filter(|o| o.had()).map(|o| o.record).collect();
    k.items.retain(|i| !had.contains(&i.record.record) || matches!(i.state, State::Approved | State::Fetching | State::Checking | State::Placed));
    for (r, o) in approved.iter().zip(of) {
        if k.dropped.contains(&r.id) {
            continue;
        }
        // No record yet (0.17's step gives each one at the start): taken once it has one; a had one, never.
        let Some(o) = o.as_ref().filter(|o| !o.had()) else { continue };
        if k.items.iter().any(|i| i.record.request == r.id || i.record.record == o.record) {
            continue;
        }
        k.next += 1;
        k.items.push(Item {
            id: format!("ss-{}", k.next),
            record: Record { record: o.record, artist: o.artist.clone(), album: o.title.clone(), year: o.year, request: r.id },
            state: State::Waiting,
            tracks: Vec::new(),
            untimed: Vec::new(),
            folders: Vec::new(),
            chosen: None,
            failed: Vec::new(),
            misses: 0,
            due: 0,
            why: None,
            batch: None,
            percent: 0.0,
            placed: None,
            set_aside: Vec::new(),
        });
    }
}

/// The next record to search: waiting and due, the longest waiting first.
fn next_search() -> Option<String> {
    let t = now();
    kept(|k| {
        k.items
            .iter()
            .filter(|i| i.state == State::Waiting && i.due <= t)
            .min_by_key(|i| (i.due, i.id.clone()))
            .map(|i| i.id.clone())
    })
}

/// The next record to fetch: one whose fetch a restart left under way first, then the approved; one held waits for its
/// time.
fn next_fetch() -> Option<String> {
    let t = now();
    kept(|k| next_of(&k.items, t))
}

/// [`next_fetch`]'s choice: pure.
fn next_of(items: &[Item], now: i64) -> Option<String> {
    let due = || items.iter().filter(|i| i.due <= now);
    due()
        .find(|i| matches!(i.state, State::Fetching | State::Checking) && i.batch.is_some())
        .or_else(|| due().find(|i| i.state == State::Approved))
        .map(|i| i.id.clone())
}

/// What a record not searched yet is judged by (C51b).
#[derive(Debug, PartialEq)]
enum Listing {
    /// Its lists: the one with lengths, the one without them (R33); one at least.
    Ready(super::references::Judging),
    /// No list to judge by: why, in few words.
    None(String),
    /// A source out of reach: nothing is known of the record yet, so it waits, as with slskd away: why, and how long (a round;
    /// a day for Bandcamp's bot check, R23).
    Later(String, i64),
}

/// How long an item waits after Bandcamp answered with its bot check (R23, N3): at most once a day, nothing done to get past it.
const CHECKED_AGAIN_SECS: i64 = 86_400;

/// A record's counting list, read through `read` (`references::tracklist`, kept there once read).
async fn listing<R, F>(it: &Item, cat: &Arc<crate::catalogue::Catalogue>, read: R) -> Listing
where
    R: Fn(crate::catalogue::records::Ref) -> F,
    F: std::future::Future<Output = Result<Option<super::references::Read>>>,
{
    if it.record.record == 0 {
        return Listing::None("No record to judge it by".into());
    }
    match super::references::tracklist(cat, it.record.record, read).await {
        Ok(Ok(lists)) => Listing::Ready(lists),
        Ok(Err(why)) => Listing::None(why),
        Err(e) if crate::bandcamp::not_read(&e) => Listing::Later("Bandcamp didn't answer".into(), CHECKED_AGAIN_SECS),
        Err(e) => Listing::Later(format!("{e:#}"), ROUND_AT_LEAST),
    }
}

/// One record searched: its list first (once), then the ladder's rungs in turn until one finds folders.
async fn search(id: &str) {
    let Some(it) = item(id) else { return };
    let Some(served) = crate::serve::instance() else { return };
    with_item(id, |i| i.state = State::Searching);
    if it.tracks.is_empty() && it.untimed.is_empty() {
        match listing(&it, served.catalogue(), super::references::read_reference).await {
            Listing::Ready(lists) => {
                let source = |l: &Option<(String, Vec<Track>)>| l.as_ref().map_or_else(String::new, |(s, _)| s.clone());
                tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), list = %source(&lists.timed), untimed = %source(&lists.untimed), "judged by its list");
                with_item(id, |i| (i.tracks, i.untimed) = (lists.timed.map(|(_, l)| l).unwrap_or_default(), lists.untimed.map(|(_, l)| l).unwrap_or_default()));
            }
            Listing::None(why) => return with_item(id, |i| (i.state, i.why, i.due) = (State::Stuck, Some(why), now() + wait_after(3))),
            Listing::Later(why, wait) => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(why), now() + wait)),
        }
    }
    let slskd = match super::track_fetch::slskd_ready().await {
        Ok((s, _)) => s,
        Err(e) => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(format!("{e:#}")), now() + ROUND_AT_LEAST)),
    };
    search_through(id, &slskd).await;
}

/// The ladder's rungs in turn, through this slskd, until one finds folders.
async fn search_through(id: &str, slskd: &Slskd) {
    search_with(id, |q: String| async move { answers(slskd, &q).await }).await;
}

/// `search_through`, each rung's answers from `ask`: folders by the list with lengths, from the first rung where it finds any;
/// where none does, every rung's answers together by the list without lengths, by titles and count (R33): no search more.
async fn search_with<A, F>(id: &str, ask: A)
where
    A: Fn(String) -> F,
    F: std::future::Future<Output = Result<Vec<Answer>>>,
{
    let Some(it) = item(id) else { return };
    let record = format!("{} - {}", it.record.artist, it.record.album);
    let mut why = None;
    let mut heard: Vec<Answer> = Vec::new();
    for (n, q) in ladder(&it.record).into_iter().enumerate() {
        if n > 0 {
            tokio::time::sleep(PACE).await;
        }
        match ask(q.clone()).await {
            Ok(a) => {
                let found = folders_of(&a, &it.tracks, &it.failed);
                if !found.is_empty() {
                    tracing::info!(target: "soulseek", record = %record, search = %q, folders = found.len(), "found");
                    return with_item(id, |i| {
                        (i.folders, i.state, i.misses, i.why) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None);
                    });
                }
                if !it.untimed.is_empty() {
                    heard.extend(a);
                }
            }
            // slskd away says nothing of the record: no miss, or a week's wait would meet it when slskd came back.
            Err(e) if e.is::<Away>() => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(format!("{e:#}")), now() + ROUND_AT_LEAST)),
            Err(e) => why = Some(format!("{e:#}")),
        }
    }
    let found = untimed_folders(&heard, &it.untimed, &it.failed);
    if !found.is_empty() {
        tracing::info!(target: "soulseek", record = %record, folders = found.len(), "found by titles alone: no list with lengths found a folder");
        return with_item(id, |i| {
            (i.folders, i.state, i.misses, i.why) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None);
        });
    }
    // Not found. Where even its list without lengths finds nothing by titles, every rung having answered, it is Stuck, saying so
    // (R33, the owner's: the fallback was the last way): Search now, or another list, takes it up again. Else searched again later.
    let stuck = !it.untimed.is_empty() && why.is_none();
    with_item(id, |i| {
        i.misses += 1;
        (i.state, i.why, i.due) = if stuck {
            (State::Stuck, Some("No folder holds every track, even by titles".into()), now() + wait_after(3))
        } else {
            (State::Waiting, why.or_else(|| Some("No folder holds every track of its release".into())), now() + wait_after(i.misses))
        };
    });
}

/// A search's answers, once it is over (or after `SEARCH_AT_MOST`).
async fn answers(slskd: &Slskd, q: &str) -> Result<Vec<Answer>> {
    let sid = slskd.search(q).await?;
    let started = std::time::Instant::now();
    while started.elapsed() < SEARCH_AT_MOST && !slskd.searched(&sid).await.unwrap_or(false) {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let a = slskd.answers(&sid).await;
    slskd.forget(&sid).await;
    a
}

/// One approved record fetched (or a fetch a restart left followed again), gated and placed; a failed user's next
/// folder after it.
async fn fetch(id: &str) {
    let (slskd, folder) = match super::track_fetch::slskd_ready().await {
        Ok(s) => s,
        // Waits for slskd: tried again in a while.
        Err(e) => return with_item(id, |i| hold(i, format!("{e:#}"), SLSKD_AGAIN)),
    };
    fetch_from(id, &slskd, &folder, |file, track| async move { super::track_fetch::gate_track(&file, &track).await }).await;
}

/// [`fetch`], through this slskd and its completed-downloads folder, each file gated by `gate` (the tests' stands in).
async fn fetch_from<G, Fut>(id: &str, slskd: &Slskd, folder: &Path, gate: G)
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    let Some(it) = item(id) else { return };
    let Some(f) = it.chosen.and_then(|c| it.folders.get(c)).cloned() else {
        return with_item(id, |i| (i.state, i.chosen, i.due) = (State::Waiting, None, 0));
    };
    let batch = match &it.batch {
        Some(b) => b.clone(),
        None => match slskd.fetch_many(&f.user, &f.files).await {
            Ok(b) => {
                with_item(id, |i| (i.state, i.batch, i.percent, i.why) = (State::Fetching, Some(b.clone()), 0.0, None));
                b
            }
            Err(e) if e.is::<Away>() => return with_item(id, |i| hold(i, format!("{e:#}"), SLSKD_AGAIN)),
            Err(e) => {
                with_item(id, |i| {
                    on_failed(i, &f.user, format!("{e:#}"));
                });
                return;
            }
        },
    };
    // At its check its files are in (one held, or one a restart cut): slskd's list decides nothing, and may have let them
    // go, which `follow` would hold against the user.
    let ts = if it.state == State::Checking {
        slskd.transfers(&batch).await.unwrap_or_default()
    } else {
        match follow(slskd, id, &batch).await {
            Ok(t) => t,
            // Its batch stays: slskd may well finish it, and it is followed again rather than left to run with no one to
            // clean its folder.
            Err(e) if e.is::<Away>() => {
                tracing::warn!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), reason = %format!("{e:#}"), "held for slskd");
                return with_item(id, |i| hold(i, format!("{e:#}"), SLSKD_AGAIN));
            }
            Err(e) => {
                for t in slskd.transfers(&batch).await.unwrap_or_default() {
                    slskd.drop_transfer(&f.user, &t.id).await;
                }
                tracing::info!(target: "soulseek", user = %f.user, reason = %format!("{e:#}"), "a user failed; the next folder");
                return with_item(id, |i| {
                    on_failed(i, &f.user, format!("{e:#}"));
                });
            }
        }
    };
    with_item(id, |i| (i.state, i.percent) = (State::Checking, 100.0));
    let dir = folder.join(crate::slskd::FOLDER).join(&batch);
    let release: Vec<ReleaseTrack> =
        judged_by(&it, &f).iter().map(|t| super::track_fetch::track_for(t.disc, t.position, &t.title, &it.record.artist, t.length_ms, t.recording.as_deref().unwrap_or_default())).collect();
    let checked = match check(&dir, &f, &release, &gate).await {
        Ok(verdict) => verdict,
        Err(why) => {
            tracing::warn!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), reason = %why, "held at its check");
            return with_item(id, |i| hold(i, why, CHECK_AGAIN));
        }
    };
    for t in ts {
        slskd.drop_transfer(&f.user, &t.id).await;
    }
    match checked {
        Ok(files) => match place(&kept(|k| record_now(k, id, &it.record)), &files) {
            Ok(to) => {
                std::fs::remove_dir_all(&dir).ok();
                tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), to = %to.display(), "placed in Incoming");
                with_item(id, |i| (i.state, i.placed, i.batch, i.why) = (State::Placed, Some(to.display().to_string()), None, None));
            }
            Err(e) => with_item(id, |i| (i.state, i.why) = (State::Stuck, Some(format!("{e:#}")))),
        },
        Err(why) => {
            let aside = set_aside(&dir, id, &f.user, &why);
            with_item(id, |i| {
                if let Some(path) = aside {
                    i.set_aside.push(SetAside { user: f.user.clone(), path, why: why.clone() });
                }
                let n = why.len();
                on_failed(i, &f.user, format!("{n} of its files failed the check; set aside"));
            });
        }
    }
}

/// Follows a batch until every transfer is done; refused, failed, queued too long or too slow is an error (its user
/// failed). A read of slskd that fails is tried again, `READS` times in a row.
async fn follow(slskd: &Slskd, id: &str, batch: &str) -> Result<Vec<crate::slskd::Transfer>> {
    let started = std::time::Instant::now();
    let mut failed_reads = 0;
    loop {
        let ts = match slskd.transfers(batch).await {
            Ok(t) if !t.is_empty() => {
                failed_reads = 0;
                t
            }
            other => {
                failed_reads += 1;
                if failed_reads >= READS {
                    return Err(other.map_or_else(|e| e.context("slskd could not be read"), |_| anyhow::anyhow!("slskd could not be read: no transfers")));
                }
                tokio::time::sleep(READ_AGAIN).await;
                continue;
            }
        };
        if let Some(bad) = ts.iter().find(|t| t.done() && !t.succeeded()) {
            bail!("{} was not sent{}", crate::slskd::base_name(&bad.filename), bad.exception.as_ref().map(|e| format!(": {e}")).unwrap_or_default());
        }
        if ts.iter().all(|t| t.done()) {
            return Ok(ts);
        }
        let percent = ts.iter().map(|t| t.percent_complete).sum::<f64>() / ts.len() as f64;
        if percent <= 0.0 && started.elapsed() > QUEUED_FOR {
            bail!("It stayed in the user's queue over {} minutes", QUEUED_FOR.as_secs() / 60);
        }
        if started.elapsed() > FETCH_AT_MOST {
            bail!("It took over {} hours", FETCH_AT_MOST.as_secs() / 3600);
        }
        with_item(id, |i| i.percent = (percent * 10.0).round() / 10.0);
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// What the check of a fetched folder found: its files in the release's order, or why each that failed did (by its name).
type Verdict = std::result::Result<Vec<PathBuf>, BTreeMap<String, String>>;

/// Every file of the fetched folder gated against its track (by `gate`): the verdict, or, where AcoustID could not be
/// asked and no file has failed so far, none: what it said, as nothing is known yet.
async fn check<G, Fut>(dir: &Path, f: &Folder, release: &[ReleaseTrack], gate: &G) -> std::result::Result<Verdict, String>
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    let on_disk = files_under(dir);
    let mut out = Vec::new();
    let mut why = BTreeMap::new();
    for (offered, track) in f.files.iter().zip(release) {
        let name = crate::slskd::base_name(&offered.filename).to_string();
        let Some(path) = on_disk.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == name)).cloned() else {
            why.insert(name, "It is not in slskd's folder".into());
            continue;
        };
        match gate(path.clone(), track.clone()).await {
            Ok(_) => out.push(path),
            // None after it is asked about: AcoustID would not answer them either, and each would be decoded whole to find
            // out. A file failed already settles the folder.
            Err(e) if e.is::<NotAsked>() && why.is_empty() => return Err(format!("{e:#}")),
            Err(e) if e.is::<NotAsked>() => break,
            Err(e) => {
                why.insert(name, format!("{e:#}"));
            }
        }
    }
    Ok(if why.is_empty() { Ok(out) } else { Err(why) })
}

/// The files anywhere under `dir`.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { todo.push(p) } else { out.push(p) }
        }
    }
    out
}

/// A name as one part of a path: what a folder name cannot hold made `_`.
fn safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c }).collect();
    t.trim().trim_matches('.').to_string()
}

/// Where a record may go in its library, in turn, none that is there already: Artist/Record (Year), then with a number added.
fn folders_for(lib: &Path, r: &Record) -> impl Iterator<Item = PathBuf> {
    let name = match r.year {
        Some(y) => format!("{} ({y})", safe(&r.album)),
        None => safe(&r.album),
    };
    let base = lib.join(safe(&r.artist)).join(&name);
    (1..).map(move |n| if n == 1 { base.clone() } else { base.with_file_name(format!("{name} {n}")) }).filter(|to| !to.exists())
}

/// The checked files into the Soulseek library, each copied in under a hidden name and then renamed (no walk sees a
/// half-written track); the record's folder. The catalogue is told first that the album arriving there is the record
/// (`Catalogue::expect_record`, C50), so its tracks join that record rather than make another, and again once the last file
/// has landed, from when the expectation waits its week (F9).
fn place(r: &Record, files: &[PathBuf]) -> Result<PathBuf> {
    let served = crate::serve::instance();
    place_with(&library()?, r, files, |to| match served.as_ref().filter(|_| r.record != 0).and_then(|s| Some((s, s.catalogue().place_of_path(to)?))) {
        Some((s, (folder, dir))) => s.catalogue().expect_record(folder, &dir, r.record),
        None => Ok(true),
    })
}

/// How many folders a record is offered before its placing gives up: each was another record's expectation.
const FOLDERS_AT_MOST: usize = 50;

/// `place`, `expect` telling the catalogue that the album arriving in a folder is the record, and whether it took that. A
/// folder another record is still expected in is passed over for the next free one, numbered as a folder that is there already
/// (R27, N10): an album placed there would join that other record. Said again once the last file has landed; a failure then is
/// said in the log and the files stay placed: they are in Incoming, and fetching them again would not help.
fn place_with(lib: &Path, r: &Record, files: &[PathBuf], expect: impl Fn(&Path) -> Result<bool>) -> Result<PathBuf> {
    let mut to = None;
    for folder in folders_for(lib, r).take(FOLDERS_AT_MOST) {
        if expect(&folder)? {
            to = Some(folder);
            break;
        }
    }
    let to = to.context("Another record is expected in every folder it could go in")?;
    std::fs::create_dir_all(&to).with_context(|| format!("{} cannot be made", to.display()))?;
    for f in files {
        let name = f.file_name().context("no file name")?;
        let hidden = to.join(format!(".{}.part", name.to_string_lossy()));
        if std::fs::rename(f, &hidden).is_err() {
            std::fs::copy(f, &hidden).with_context(|| format!("copying {}", f.display()))?;
        }
        std::fs::rename(&hidden, to.join(name))?;
    }
    if let Err(e) = expect(&to) {
        tracing::warn!(target: "soulseek", folder = %to.display(), reason = %format!("{e:#}"), "placed, but its record could not be told of its folder again");
    }
    Ok(to)
}

/// A fetched folder set aside, with why each file failed beside it (`why.json`), in the data folder's
/// `soulseek-set-aside`; where it went.
fn set_aside(dir: &Path, id: &str, user: &str, why: &BTreeMap<String, String>) -> Option<String> {
    let data = crate::serve::instance()?.data_dir.clone()?;
    let to = data.join("soulseek-set-aside").join(format!("{id} {}", safe(user)));
    std::fs::create_dir_all(to.parent()?).ok()?;
    if std::fs::rename(dir, &to).is_err() {
        std::fs::create_dir_all(&to).ok()?;
        for f in files_under(dir) {
            if let Some(n) = f.file_name() {
                std::fs::copy(&f, to.join(n)).ok();
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }
    std::fs::write(to.join("why.json"), serde_json::to_vec_pretty(why).ok()?).ok()?;
    Some(to.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(artist: &str, album: &str, year: Option<i32>) -> Record {
        Record { record: 1, artist: artist.into(), album: album.into(), year, request: 1 }
    }

    fn track(n: u32, title: &str, secs: i64) -> Track {
        Track { disc: 1, position: n, title: title.into(), length_ms: Some(secs * 1000), recording: Some(format!("rec-{n}")) }
    }

    fn file(path: &str, secs: i64) -> Offered {
        Offered { filename: path.into(), size: 1000, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: Some(secs), is_locked: false }
    }

    fn answer(user: &str, slot: bool, files: Vec<Offered>) -> Answer {
        Answer { username: user.into(), files, has_free_upload_slot: slot, queue_length: 0, upload_speed: 100 }
    }

    #[test]
    fn the_ladder_goes_from_the_record_and_its_year_to_a_wildcard_none_twice() {
        assert_eq!(
            ladder(&record("Air feat. Beck", "Moon Safari (Deluxe Edition)", Some(1998))),
            ["Air feat Beck Moon Safari Deluxe Edition 1998", "Air feat Beck Moon Safari", "Air Moon Safari", "*ir Moon Safari"]
        );
        // Nothing to strip and one artist: two rungs fall together.
        assert_eq!(ladder(&record("Boards of Canada", "Geogaddi", None)), ["Boards of Canada Geogaddi", "*oards of Canada Geogaddi"]);
        assert_eq!(main_artist("Simon & Garfunkel"), "Simon");
        assert_eq!(main_artist("Grouch / Grouch In Dub"), "Grouch");
        assert_eq!(main_artist("AC/DC"), "AC/DC");
        assert_eq!(main_artist("İlhan Mimaroğlu & Freddie Hubbard"), "İlhan Mimaroğlu");
    }

    #[test]
    fn a_record_not_found_waits_longer_each_time_and_is_never_given_up() {
        assert_eq!(wait_after(0), ROUND_AT_LEAST);
        assert_eq!(wait_after(1), ROUND_AT_LEAST);
        assert_eq!(wait_after(2), 4 * 3600);
        assert_eq!(wait_after(3), 86_400);
        assert_eq!(wait_after(4), 7 * 86_400);
        assert_eq!(wait_after(40), 7 * 86_400, "never given up");
    }

    #[test]
    fn a_folder_is_a_candidate_only_when_it_holds_every_track_by_title_and_length() {
        let tracks = [track(1, "La Femme d'argent", 430), track(2, "Sexy Boy", 298), track(3, "All I Need", 268)];
        let answers = vec![
            // Whole, lossless, with a cover and a bonus track.
            answer(
                "whole",
                true,
                vec![
                    file(r"Music\Air\Moon Safari\01 - La Femme d'argent.flac", 431),
                    file(r"Music\Air\Moon Safari\02 Air - Sexy Boy.flac", 298),
                    file(r"Music\Air\Moon Safari\03. All I Need.flac", 267),
                    file(r"Music\Air\Moon Safari\04 Bonus.flac", 200),
                    file(r"Music\Air\Moon Safari\cover.jpg", 0),
                ],
            ),
            // A track missing.
            answer("short", true, vec![file(r"x\01 La Femme d'argent.mp3", 430), file(r"x\02 Sexy Boy.mp3", 298)]),
            // The right titles, one a radio edit (40 s short).
            answer("edit", true, vec![file(r"y\01 La Femme d'argent.mp3", 430), file(r"y\02 Sexy Boy.mp3", 258), file(r"y\03 All I Need.mp3", 268)]),
            // Whole, MP3, a busy user.
            answer("mp3", false, vec![file(r"z\Air - La Femme d'argent.mp3", 430), file(r"z\Air - Sexy Boy.mp3", 298), file(r"z\Air - All I Need.mp3", 268)]),
        ];
        let found = folders_of(&answers, &tracks, &[]);
        let who: Vec<(&str, &str, usize)> = found.iter().map(|f| (f.user.as_str(), f.format.as_str(), f.extra)).collect();
        assert_eq!(who, [("whole", "flac", 1), ("mp3", "mp3", 0)]);
        assert_eq!(crate::slskd::base_name(&found[0].files[1].filename), "02 Air - Sexy Boy.flac", "in the release's order");
        assert_eq!(folders_of(&answers, &tracks, &["whole".into()]).len(), 1, "a failed user is left out");
        assert!(folders_of(&answers, &[], &[]).is_empty(), "no release, nothing judged");
    }

    #[test]
    fn a_failed_user_hands_the_record_to_its_next_folder_or_back_to_the_search() {
        let f = |user: &str| Folder { user: user.into(), dir: "d".into(), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 1, extra: 0, files: Vec::new(), unchecked: None };
        let mut i = Item {
            id: "ss-1".into(),
            record: record("Air", "Moon Safari", Some(1998)),
            state: State::Fetching,
            tracks: Vec::new(),
            untimed: Vec::new(),
            folders: vec![f("a"), f("b")],
            chosen: Some(0),
            failed: Vec::new(),
            misses: 0,
            due: 0,
            why: None,
            batch: Some("b1".into()),
            percent: 40.0,
            placed: None,
            set_aside: Vec::new(),
        };
        assert!(on_failed(&mut i, "a", "a is offline".into()));
        assert_eq!((i.state, i.chosen, i.folders.len(), i.batch.clone(), i.failed.clone()), (State::Approved, Some(0), 1, None, vec!["a".to_string()]));
        assert_eq!(i.folders[0].user, "b", "the next folder, approved by the same tick");
        assert!(!on_failed(&mut i, "b", "queued too long".into()));
        assert_eq!((i.state, i.chosen, i.due), (State::Waiting, None, 0), "none left: searched again at once, without them");
        assert_eq!(i.failed, ["a", "b"]);
    }

    /// A record with two folders and two tracks, put on the list under `id`.
    fn listed(id: &str, state: State, batch: Option<&str>) {
        let f = |user: &str| Folder {
            user: user.into(),
            dir: "d".into(),
            format: "flac".into(),
            lossless: true,
            free_slot: true,
            queue: 0,
            speed: 1,
            extra: 0,
            files: vec![file(r"d\01 One.flac", 100), file(r"d\02 Two.flac", 200)],
            unchecked: None,
        };
        let i = Item {
            id: id.into(),
            record: record("Air", "Moon Safari", Some(1998)),
            state,
            tracks: vec![track(1, "One", 100), track(2, "Two", 200)],
            untimed: Vec::new(),
            folders: vec![f("a"), f("b")],
            chosen: Some(0),
            failed: Vec::new(),
            misses: 0,
            due: 0,
            why: None,
            batch: batch.map(Into::into),
            percent: 0.0,
            placed: None,
            set_aside: Vec::new(),
        };
        kept(|k| k.items.push(i));
    }

    /// slskd at a local address, answering as `app` does.
    async fn slskd_at(app: axum::Router) -> Slskd {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Slskd::new(&url, "key").unwrap()
    }

    /// One that says every batch is in.
    fn all_in() -> axum::Router {
        let done = serde_json::json!({ "transfers": [{ "id": "t1", "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 }] });
        axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(done.clone()))))
    }

    /// slskd's completed-downloads folder, a batch's files in it.
    fn fetched_to(name: &str, batch: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{name}-{}", std::process::id()));
        let at = d.join(crate::slskd::FOLDER).join(batch);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("01 One.flac"), b"x").unwrap();
        std::fs::write(at.join("02 Two.flac"), b"x").unwrap();
        d
    }

    fn acoustid_away() -> anyhow::Error {
        NotAsked("AcoustID could not be reached".into()).into()
    }

    #[tokio::test]
    async fn a_check_acoustid_cannot_answer_holds_the_record_and_holds_it_against_nobody() {
        let (id, batch) = ("ss-held", "b-held");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("held", batch);
        let slskd = slskd_at(all_in()).await;
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let gate = |_, _| {
            asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::future::ready(Err(acoustid_away()))
        };
        fetch_from(id, &slskd, &d, gate).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.as_deref(), i.chosen, i.folders.len()), (State::Checking, Some(batch), Some(0), 2), "it stays where it is, its tick and folders with it");
        assert!(i.failed.is_empty() && i.set_aside.is_empty(), "no user failed, nothing set aside");
        assert_eq!(i.why.as_deref(), Some("AcoustID could not be reached"));
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1, "one file decoded to find out, not the whole folder");
        assert!(d.join(crate::slskd::FOLDER).join(batch).join("01 One.flac").exists(), "what it fetched waits where it is");
        // No next folder under the same tick: nothing until its time, then it is this record again.
        assert!(i.due > now());
        assert_eq!(next_of(std::slice::from_ref(&i), i.due - 1), None);
        assert_eq!(next_of(std::slice::from_ref(&i), i.due), Some(id.to_string()));
        // AcoustID back, slskd having let go of the transfers meanwhile: on from the check, with no user failed.
        let forgot = slskd_at(axum::Router::new()).await;
        fetch_from(id, &forgot, &d, |_, _| std::future::ready(Ok(0.97))).await;
        let i = item(id).unwrap();
        assert!(i.failed.is_empty() && i.set_aside.is_empty());
        assert_eq!(i.state, State::Stuck, "passed: only the library a test has not set up stops it");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_file_that_failed_settles_the_folder_with_acoustid_away() {
        let d = fetched_to("settled", "b");
        let dir = d.join(crate::slskd::FOLDER).join("b");
        let f = Folder { user: "a".into(), dir: "d".into(), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 1, extra: 0, files: vec![file(r"d\01 One.flac", 100), file(r"d\02 Two.flac", 200)], unchecked: None };
        let release: Vec<ReleaseTrack> = [(1, "One"), (2, "Two")].iter().map(|(n, t)| super::super::track_fetch::track_for(1, *n, t, "Air", None, "r")).collect();
        let first = |file: PathBuf, _| std::future::ready(if file.ends_with("01 One.flac") { Err(anyhow::anyhow!("2 frames would not decode")) } else { Err(acoustid_away()) });
        let why = check(&dir, &f, &release, &first).await.unwrap().unwrap_err();
        assert_eq!(why.into_iter().collect::<Vec<_>>(), [("01 One.flac".to_string(), "2 frames would not decode".to_string())]);
        // The other way round nothing has failed yet: no verdict, though a file after it might.
        let second = |file: PathBuf, _| std::future::ready(if file.ends_with("01 One.flac") { Err(acoustid_away()) } else { Err(anyhow::anyhow!("2 frames would not decode")) });
        assert_eq!(check(&dir, &f, &release, &second).await.unwrap_err(), "AcoustID could not be reached");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A slskd nothing is listening at.
    async fn slskd_gone() -> Slskd {
        let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        Slskd::new(&format!("http://{gone}"), "key").unwrap()
    }

    fn never_gated(_: PathBuf, _: ReleaseTrack) -> std::future::Ready<Result<f32>> {
        panic!("nothing was fetched to check")
    }

    #[tokio::test]
    async fn slskd_gone_leaves_an_approved_record_as_it_is_and_blames_no_user() {
        let id = "ss-away";
        listed(id, State::Approved, None);
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-away-{}", std::process::id()));
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.chosen, i.folders.len(), i.batch.clone()), (State::Approved, Some(0), 2, None), "its tick and its folders stay");
        assert!(i.failed.is_empty(), "no user failed");
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("slskd could not be reached")), "{:?}", i.why);
        // Not taken again at once, which is what went through every folder in seconds.
        assert_eq!(next_of(std::slice::from_ref(&i), i.due - 1), None);
        assert_eq!(next_of(std::slice::from_ref(&i), i.due), Some(id.to_string()));
    }

    #[tokio::test]
    async fn a_user_who_refuses_still_hands_the_record_to_the_next_folder() {
        let id = "ss-refused";
        listed(id, State::Approved, None);
        let refused = axum::Router::new().route("/api/v0/transfers/downloads/batches", axum::routing::post(|| std::future::ready(axum::http::StatusCode::NOT_FOUND)));
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-refused-{}", std::process::id()));
        fetch_from(id, &slskd_at(refused).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.chosen, i.failed.clone(), i.folders[0].user.as_str(), i.why.as_deref()), (State::Approved, Some(0), vec!["a".to_string()], "b", Some("a is offline")));
    }

    #[tokio::test]
    async fn a_fetch_slskd_stops_answering_keeps_its_batch_and_is_followed_again() {
        let (id, batch) = ("ss-reads", "b-reads");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("reads", batch);
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.as_deref(), i.chosen, i.folders.len()), (State::Fetching, Some(batch), Some(0), 2), "the batch kept, for slskd may well finish it");
        assert!(i.failed.is_empty());
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("slskd could not be read: slskd could not be reached")), "{:?}", i.why);
        assert_eq!(next_of(std::slice::from_ref(&i), i.due - 1), None);
        // Back, with the batch finished meanwhile: followed from where it was, and on to its check.
        with_item(id, |i| i.due = 0);
        fetch_from(id, &slskd_at(all_in()).await, &d, |_, _| std::future::ready(Err(acoustid_away()))).await;
        assert_eq!(item(id).unwrap().state, State::Checking, "followed to its end");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_batch_slskd_answers_it_does_not_know_is_a_fetch_gone_and_held_for_nothing() {
        let (id, batch) = ("ss-gone", "b-gone");
        listed(id, State::Fetching, Some(batch));
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-gone-{}", std::process::id()));
        fetch_from(id, &slskd_at(axum::Router::new()).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.clone(), i.failed.clone()), (State::Approved, None, vec!["a".to_string()]), "the next folder: holding it would wait for ever");
    }

    #[tokio::test]
    async fn a_search_slskd_could_not_make_is_no_miss() {
        let id = "ss-search";
        listed(id, State::Searching, None);
        with_item(id, |i| i.misses = 3);
        search_through(id, &slskd_gone().await).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.misses), (State::Waiting, 3), "the ladder's waits are for records not found");
        assert!(i.due <= now() + ROUND_AT_LEAST, "searched again soon");
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("slskd could not be reached")), "{:?}", i.why);
    }

    #[test]
    fn a_record_held_at_its_check_can_leave_the_list_and_one_at_work_cannot() {
        listed("ss-work", State::Checking, Some("b-work"));
        assert_eq!(dropped("ss-work", now()), None, "being checked");
        assert!(item("ss-work").is_some());
        with_item("ss-work", |i| hold(i, "AcoustID is not set up".into(), CHECK_AGAIN));
        assert_eq!(dropped("ss-work", now()).as_deref(), Some("b-work"), "its batch, for its folder to go");
        assert!(item("ss-work").is_none());
        listed("ss-coming", State::Fetching, Some("b-coming"));
        with_item("ss-coming", |i| hold(i, "slskd could not be reached".into(), CHECK_AGAIN));
        assert_eq!(dropped("ss-coming", now()), None, "its files may still be coming");
    }

    #[test]
    fn a_record_goes_in_as_artist_and_record_with_its_year_a_number_where_taken() {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let r = record("AC/DC", "Back in Black: Live?", Some(1980));
        let first = folders_for(&d, &r).next().unwrap();
        assert_eq!(first, d.join("AC_DC").join("Back in Black_ Live_ (1980)"));
        std::fs::create_dir_all(&first).unwrap();
        assert_eq!(folders_for(&d, &r).next().unwrap(), d.join("AC_DC").join("Back in Black_ Live_ (1980) 2"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_library_is_the_incoming_one_whose_fetcher_is_soulseek() {
        use crate::settings::{Libraries, LibraryKind, LibrarySetting, Saved, SourceConfig};
        let libraries = Libraries(vec![
            LibrarySetting::new("/music", LibraryKind::Main),
            LibrarySetting::new("/lidarr", LibraryKind::Incoming),
            LibrarySetting::new("/soulseek", LibraryKind::Incoming),
        ]);
        let mut saved = Saved {
            sources: vec![SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values: Default::default() }],
            ..Default::default()
        };
        assert_eq!(library_in(&saved), None, "none chosen: none");
        saved.sources[0].libraries.0[2].fetcher = Some(FETCHER.into());
        assert_eq!(library_in(&saved), Some(PathBuf::from("/soulseek")));
    }

    fn asked(id: i64, group: Option<&str>) -> crate::userdata::Request {
        crate::userdata::Request {
            record: None,
            id,
            user: "u".into(),
            album_key: format!("k{id}"),
            artist_key: "a".into(),
            release_group: group.map(Into::into),
            album: format!("Album {id}"),
            artist: "Artist".into(),
            year: Some(2000),
            cover: None,
            at: 0,
            status: "approved".into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
        }
    }

    /// A request's record as `records_of` reads it: record `n`, named after its request.
    fn of(n: i64) -> Option<super::super::requests::RecordOf> {
        Some(super::super::requests::RecordOf { record: n, groups: Vec::new(), title: format!("Album {n}"), artist: "Artist".into(), year: Some(2000), cover: None, holder: None })
    }

    #[test]
    fn only_approved_requests_are_wanted_and_one_taken_off_stays_off() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, Some("rg1")), asked(2, None), asked(3, Some("rg3")), asked(4, None), asked(5, None)], &[of(101), of(102), of(103), None, of(101)]);
        // Every record, with a release group or none (C51b); one with no record yet waits for one; one record taken once.
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record)).collect::<Vec<_>>(), vec![(1, 101), (2, 102), (3, 103)]);
        assert_eq!(k.items[1].record.album, "Album 102", "named as its record is");
        // Taken off: not taken in again while it stays approved.
        k.dropped.push(3);
        k.items.retain(|i| i.record.request != 3);
        take_requests(&mut k, &[asked(1, Some("rg1")), asked(3, Some("rg3"))], &[of(101), of(103)]);
        assert_eq!(k.items.len(), 1);
        // No longer approved (cancelled): it goes, unless it is being fetched or placed.
        k.items[0].state = State::Found;
        take_requests(&mut k, &[], &[]);
        assert!(k.items.is_empty());
        assert!(k.dropped.is_empty(), "a dropped one no longer approved is forgotten");
    }

    /// A data folder of the test's own.
    fn data_folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A list's track as a list without lengths gives it (R33): no length, no recording.
    fn no_length(n: u32, title: &str) -> Track {
        Track { length_ms: None, recording: None, ..track(n, title, 0) }
    }

    /// An item waiting to be searched under `id`, judged by `timed` (with lengths) and `untimed` (without); its record.
    fn waiting_with(id: &str, timed: Vec<Track>, untimed: Vec<Track>) -> Record {
        let r = record("Air", "Moon Safari", Some(1998));
        let i = Item {
            id: id.into(),
            record: r.clone(),
            state: State::Waiting,
            tracks: timed,
            untimed,
            folders: Vec::new(),
            chosen: None,
            failed: Vec::new(),
            misses: 0,
            due: 0,
            why: None,
            batch: None,
            percent: 0.0,
            placed: None,
            set_aside: Vec::new(),
        };
        kept(|k| k.items.push(i));
        r
    }

    /// Each search's answers in turn as `rungs` gives them (none after), the searches counted in `asked`.
    fn answering(rungs: Vec<Vec<Answer>>, asked: &std::sync::atomic::AtomicUsize) -> impl Fn(String) -> std::future::Ready<Result<Vec<Answer>>> + '_ {
        move |_: String| std::future::ready(Ok(rungs.get(asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst)).cloned().unwrap_or_default()))
    }

    /// A user's folder of the record's two tracks: by title and length, as the list with lengths has them.
    fn exact_folder(user: &str) -> Answer {
        answer(user, true, vec![file(r"x\Air\Moon Safari\01 One.flac", 100), file(r"x\Air\Moon Safari\02 Two.flac", 200)])
    }

    /// A user's folder of the record's two tracks by title, their lengths another edition's.
    fn titled_folder(user: &str) -> Answer {
        answer(user, true, vec![file(r"y\Air\Moon Safari\01 One.flac", 150), file(r"y\Air\Moon Safari\02 Two.flac", 260)])
    }

    /// R33 (N17, as the owner amended it): where the list with lengths finds no folder in any rung of the search, the same answers
    /// are judged by the list without them, by titles and count: a folder is found, and no search more is made for it.
    #[tokio::test]
    async fn a_list_without_lengths_is_used_only_when_lists_with_lengths_find_nothing() {
        let r = waiting_with("ss-r33-only", vec![track(1, "One", 100), track(2, "Two", 200)], vec![no_length(1, "One"), no_length(2, "Two")]);
        let asked = std::sync::atomic::AtomicUsize::new(0);
        search_with("ss-r33-only", answering(vec![vec![titled_folder("near")]], &asked)).await;
        let i = item("ss-r33-only").unwrap();
        assert_eq!((i.state, i.folders.iter().map(|f| f.user.as_str()).collect::<Vec<_>>()), (State::Found, vec!["near"]));
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), ladder(&r).len(), "every rung by the list with lengths first; none more");
    }

    /// R33: a folder found by titles alone says so beside its tick, and its files are gated by the list it was found by.
    #[tokio::test]
    async fn a_fallback_folder_says_it_has_no_lengths_to_check() {
        waiting_with("ss-r33-says", Vec::new(), vec![no_length(1, "One"), no_length(2, "Two")]);
        let asked = std::sync::atomic::AtomicUsize::new(0);
        search_with("ss-r33-says", answering(vec![vec![titled_folder("near")]], &asked)).await;
        let i = item("ss-r33-says").unwrap();
        assert_eq!((i.state, i.folders[0].unchecked.as_deref()), (State::Found, Some("No track lengths to check")));
        let gated = judged_by(&i, &i.folders[0]);
        assert!(gated.len() == 2 && gated.iter().all(|t| t.length_ms.is_none()), "by the list without lengths");
    }

    /// R33: a folder the list with lengths finds, in any rung, is all that is offered: one found by titles alone in an earlier rung
    /// is not.
    #[tokio::test]
    async fn with_lists_with_lengths_finding_a_folder_no_fallback_is_offered() {
        waiting_with("ss-r33-strict", vec![track(1, "One", 100), track(2, "Two", 200)], vec![no_length(1, "One"), no_length(2, "Two")]);
        let asked = std::sync::atomic::AtomicUsize::new(0);
        search_with("ss-r33-strict", answering(vec![vec![titled_folder("near")], vec![exact_folder("exact"), titled_folder("near")]], &asked)).await;
        let i = item("ss-r33-strict").unwrap();
        assert_eq!(i.folders.iter().map(|f| (f.user.as_str(), f.unchecked.is_some())).collect::<Vec<_>>(), [("exact", false)]);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2, "found at the second rung");
    }

    /// R33: where even the titles find no folder (a track none holds; another count of tracks), the item is Stuck, saying so; with
    /// a list with lengths alone, a miss is searched again later, as ever.
    #[tokio::test]
    async fn with_no_list_matching_even_by_titles_the_item_says_so() {
        for (id, list) in [("ss-r33-none", vec![no_length(1, "One"), no_length(2, "Two"), no_length(3, "Three")]), ("ss-r33-count", vec![no_length(1, "One")])] {
            waiting_with(id, Vec::new(), list);
            let asked = std::sync::atomic::AtomicUsize::new(0);
            search_with(id, answering(vec![vec![titled_folder("near")]], &asked)).await;
            let i = item(id).unwrap();
            assert_eq!((i.state, i.why.as_deref(), i.misses, i.folders.len()), (State::Stuck, Some("No folder holds every track, even by titles"), 1, 0), "{id}");
        }
        waiting_with("ss-r33-timed", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        search_with("ss-r33-timed", answering(vec![vec![titled_folder("near")]], &std::sync::atomic::AtomicUsize::new(0))).await;
        let i = item("ss-r33-timed").unwrap();
        assert_eq!((i.state, i.why.as_deref()), (State::Waiting, Some("No folder holds every track of its release")));
    }

    /// R33: of the folders found by titles alone, those whose files agree in length with others, track by track, come first.
    #[test]
    fn fallback_folders_whose_lengths_agree_come_first() {
        let list = [no_length(1, "One"), no_length(2, "Two")];
        let at = |user: &str, one: i64, two: i64| answer(user, true, vec![file(&format!(r"{user}\01 One.flac"), one), file(&format!(r"{user}\02 Two.flac"), two)]);
        let found = untimed_folders(&[at("odd", 300, 50), at("first", 100, 200), at("second", 101, 199)], &list, &[]);
        assert_eq!(found.iter().map(|f| f.user.as_str()).collect::<Vec<_>>(), ["first", "second", "odd"]);
    }

    /// R27 (N10): a folder another record is expected in is passed over: the files go into the next free one, numbered, so their
    /// album cannot join that other record; a failure to say so again once they are placed leaves them placed.
    #[test]
    fn a_refused_expectation_places_the_files_elsewhere() {
        let d = data_folder("refused");
        let (lib, from) = (d.join("soulseek"), d.join("fetched"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("01 Pier.flac"), b"flac").unwrap();
        let r = record("Glass Harbour", "Harbour Lights", Some(2014));
        let taken = lib.join("Glass Harbour").join("Harbour Lights (2014)");
        let asked = Mutex::new(0);
        let expect = |to: &Path| -> Result<bool> {
            let mut n = asked.lock().unwrap();
            *n += 1;
            match *n {
                3 => bail!("the catalogue could not be written"),
                _ => Ok(to != taken.as_path()),
            }
        };
        let to = place_with(&lib, &r, &[from.join("01 Pier.flac")], expect).unwrap();
        assert_eq!(to, lib.join("Glass Harbour").join("Harbour Lights (2014) 2"));
        assert!(to.join("01 Pier.flac").exists() && !taken.exists());
        assert_eq!(*asked.lock().unwrap(), 3, "told again once placed");
        std::fs::remove_dir_all(&d).ok();
    }

    /// R24 (N4): the queue 0.16 kept is copied aside once, before 0.17 writes its own over it: a rollback finds it; a later start,
    /// its queue 0.17's, copies nothing over that copy.
    #[test]
    fn the_016_queue_is_kept_aside_once() {
        let d = data_folder("keep-016");
        let p = d.join("soulseek.json");
        let old = r#"{"next":3,"items":[{"id":"ss-3","record":{"artist":"Air","album":"Moon Safari","release_group":"rg","request":7},"state":"waiting"}]}"#;
        std::fs::write(&p, old).unwrap();
        assert_eq!(load(&p).items.len(), 1);
        assert_eq!(std::fs::read_to_string(d.join("soulseek.json.016")).unwrap(), old);
        std::fs::write(&p, serde_json::to_vec(&Kept::default()).unwrap()).unwrap();
        load(&p);
        assert_eq!(std::fs::read_to_string(d.join("soulseek.json.016")).unwrap(), old, "once");
        std::fs::remove_dir_all(&d).ok();
    }

    /// R24 (N4): a queue that cannot be read is set aside whole and a new one begun, never emptied in its place.
    #[test]
    fn an_unreadable_queue_is_set_aside_not_emptied() {
        let d = data_folder("unreadable");
        let p = d.join("soulseek.json");
        std::fs::write(&p, b"{\"next\": 3, \"items\": [ not json").unwrap();
        assert!(load(&p).items.is_empty());
        let aside: Vec<PathBuf> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).filter(|f| f.to_string_lossy().contains("soulseek.json.unreadable-")).collect();
        assert_eq!(aside.len(), 1, "set aside");
        assert_eq!(std::fs::read(&aside[0]).unwrap(), b"{\"next\": 3, \"items\": [ not json");
        assert!(!p.exists(), "nothing written in its place yet");
        std::fs::remove_dir_all(&d).ok();
    }

    /// R24 (N5): an item 0.16 kept, its fetch begun at the upgrade start before the first round of requests gave it its record, is
    /// placed for its record as the list holds it once the fetch is done.
    #[test]
    fn an_old_item_fetched_first_at_start_is_placed_for_its_record() {
        let raw = r#"{"next":3,"items":[{"id":"ss-3","record":{"artist":"Air","album":"Moon Safari","release_group":"rg","request":7},"state":"fetching","batch":"b-1"}]}"#;
        let mut k: Kept = serde_json::from_str(raw).unwrap();
        let at_start = k.items[0].record.clone();
        take_requests(&mut k, &[asked(7, Some("rg"))], &[of(42)]);
        assert_eq!((at_start.record, record_now(&k, "ss-3", &at_start).record), (0, 42));
        assert_eq!(record_now(&k, "ss-9", &at_start), at_start, "one gone from the list: as it was");
    }

    /// A request's record as `records_of` reads it, had: a record holding its files.
    fn had(n: i64) -> Option<super::super::requests::RecordOf> {
        Some(super::super::requests::RecordOf { holder: Some(n + 1000), ..of(n).unwrap() })
    }

    /// R26 (N9): Soulseek takes no record already had (its own files, or a record holding them), and an item of one not in flight
    /// goes; one being fetched stays as it is.
    #[test]
    fn soulseek_takes_no_record_already_had() {
        let mut k = Kept::default();
        let both = [asked(1, None), asked(2, None)];
        take_requests(&mut k, &both, &[had(101), of(102)]);
        assert_eq!(k.items.iter().map(|i| i.record.record).collect::<Vec<_>>(), [102]);
        k.items[0].state = State::Fetching;
        take_requests(&mut k, &both, &[had(101), had(102)]);
        assert_eq!(k.items.len(), 1, "in flight: as it was");
        k.items[0].state = State::Found;
        take_requests(&mut k, &both, &[had(101), had(102)]);
        assert!(k.items.is_empty(), "found, not fetched: gone, its album had");
    }

    /// R26 (N9): a request still approved after its album was placed and had is not searched again when the administrator clears
    /// the placed ones.
    #[test]
    fn clearing_placed_does_not_search_them_again() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)]);
        k.items[0].state = State::Placed;
        k.items.retain(|i| i.state != State::Placed);
        take_requests(&mut k, &[asked(1, None)], &[had(101)]);
        assert!(k.items.is_empty());
    }

    /// C51b (row 9.26): an item 0.16 kept, by its release group, is given its request's record and goes on as it was: its
    /// state, batch, folder, tick and names kept, no second item made for it, and written back with its record.
    #[test]
    fn an_old_item_in_flight_keeps_its_record() {
        let raw = r#"{"next":3,"dropped":[],"items":[{"id":"ss-3","record":{"artist":"Air","album":"Moon Safari","release_group":"rg-moon","release":"rel-moon","request":7},
            "state":"fetching","tracks":[{"disc":1,"position":1,"title":"La Femme d'argent","length_ms":431000,"recording":"rec-1"}],
            "folders":[{"user":"whole","dir":"Music\\Air\\Moon Safari","format":"flac","lossless":true,"free_slot":true,"queue":0,"speed":100,"extra":0,"files":[]}],
            "chosen":0,"batch":"b-1","percent":40.0}]}"#;
        let mut k: Kept = serde_json::from_str(raw).unwrap();
        assert_eq!((k.items[0].record.record, k.items[0].tracks[0].recording.as_deref()), (0, Some("rec-1")), "read as 0.16 kept it");
        take_requests(&mut k, &[asked(7, Some("rg-moon"))], &[of(42)]);
        assert_eq!(k.items.len(), 1, "no second item for its request");
        let i = &k.items[0];
        assert_eq!((i.record.record, i.state, i.batch.as_deref(), i.chosen, i.percent), (42, State::Fetching, Some("b-1"), Some(0), 40.0));
        assert_eq!((i.record.artist.as_str(), i.record.album.as_str()), ("Air", "Moon Safari"), "its names, and so the folder it goes to, as they were");
        let again: Kept = serde_json::from_str(&serde_json::to_string(&k).unwrap()).unwrap();
        assert_eq!(again.items[0].record.record, 42, "written back with it");
    }

    /// R23 (N3): Bandcamp's bot check is Bandcamp out of reach: the item waits, and is tried again a day later at the soonest,
    /// nothing done to get past the check; it is not Stuck for good.
    #[tokio::test]
    async fn bandcamps_bot_check_is_tried_again_later() {
        let (cat, items, d) = fx01("bot-check");
        let checked = |_| std::future::ready(Err(anyhow::Error::from(crate::bandcamp::NotRead)));
        assert_eq!(listing(of_request(&items, 11), &cat, checked).await, Listing::Later("Bandcamp didn't answer".into(), CHECKED_AGAIN_SECS));
        assert!(CHECKED_AGAIN_SECS >= 86_400);
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    /// FX01's data after 0.17's step, and the Soulseek items its approved requests make, as `read_requests` takes them; the
    /// data folder, to remove once the catalogue is let go.
    fn fx01(name: &str) -> (Arc<crate::catalogue::Catalogue>, Vec<Item>, PathBuf) {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        crate::fixtures::data_0_16(&d).unwrap();
        let cat = Arc::new(crate::catalogue::Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![PathBuf::from("/music")]).unwrap());
        let ud = crate::userdata::UserData::open(Some(&d.join("userdata.sqlite"))).unwrap();
        super::super::step_017::run(&cat, &ud).unwrap();
        let approved = ud.requests(None, Some(super::super::requests::APPROVED), 100).unwrap();
        let of = super::super::requests::records_of(&cat, &approved).unwrap();
        let mut k = Kept::default();
        take_requests(&mut k, &approved, &of);
        (cat, k.items, d)
    }

    fn of_request(items: &[Item], request: i64) -> &Item {
        items.iter().find(|i| i.record.request == request).expect("its item")
    }

    /// A Bandcamp album's list, as a stub reader answers it: two tracks, no recordings.
    fn bandcamp_list(r: crate::catalogue::records::Ref) -> std::future::Ready<Result<Option<crate::app::references::Read>>> {
        assert_eq!((r.source.as_str(), r.value.as_str()), ("bandcamp", "https://paperlantern.bandcamp.com/album/after-hours"));
        let list = [("Night Ferry", 241_000), ("Last Lights", 198_000)].into_iter().zip(1..).map(|((t, ms), n)| Track { disc: 1, position: n, title: t.into(), length_ms: Some(ms), recording: None }).collect();
        std::future::ready(Ok(Some((list, None))))
    }

    /// R25 (N7): Search now reads the record's counting list again, so a list chosen by hand since reaches the item; one in
    /// flight keeps what it has.
    #[test]
    fn search_now_reads_the_counting_list_again() {
        let item = |state: State| Item {
            id: "ss-1".into(),
            record: record("Air", "Moon Safari", Some(1998)),
            state,
            tracks: vec![track(1, "La Femme d'argent", 431)],
            untimed: vec![no_length(1, "La Femme d'argent")],
            folders: Vec::new(),
            chosen: None,
            failed: Vec::new(),
            misses: 2,
            due: 9,
            why: Some("No folder holds every track of its release".into()),
            batch: None,
            percent: 0.0,
            placed: None,
            set_aside: Vec::new(),
        };
        let mut found = item(State::Found);
        search_again(&mut found);
        assert_eq!((found.state, found.tracks.len(), found.untimed.len(), found.due, found.why), (State::Waiting, 0, 0, 0, None));
        let mut fetching = item(State::Fetching);
        search_again(&mut fetching);
        assert_eq!((fetching.state, fetching.tracks.len()), (State::Fetching, 1), "in flight: as it was");
    }

    /// C51b, the owner's production case (FX01's look-alike, "Tape Club - After Hours (Paper Lantern)" asked by the label's own
    /// account, no release group): its record not had holds only its Bandcamp album, and is searched by Bandcamp's list, named
    /// as the record is; each track with no recording (judged by titles and lengths, Q29); a Soulseek folder of those titles and
    /// lengths is a candidate.
    #[tokio::test]
    async fn a_labels_bandcamp_only_request_is_searched_by_its_list() {
        let (cat, items, d) = fx01("bandcamp-list");
        let it = of_request(&items, 11);
        assert_eq!((it.record.artist.as_str(), it.record.album.as_str(), it.state), ("Tape Club", "After Hours", State::Waiting), "taken, though it has no release group");
        let Listing::Ready(lists) = listing(it, &cat, bandcamp_list).await else { panic!("no list") };
        let (source, tracks) = lists.timed.expect("Bandcamp's list has lengths");
        assert_eq!((source.as_str(), tracks.len()), ("bandcamp", 2));
        assert!(tracks.iter().all(|t| t.recording.is_none()), "no recording: AcoustID's check skipped at the gate");
        assert_eq!(ladder(&it.record)[0], "Tape Club After Hours 2020");
        let answers = [answer("bc", true, vec![file(r"Tape Club\After Hours\01 Night Ferry.flac", 241), file(r"Tape Club\After Hours\02 Last Lights.flac", 198)])];
        assert_eq!(folders_of(&answers, &tracks, &[]).len(), 1, "judged by Bandcamp's list");
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    /// C51b: a record with no list to judge by says why, in few words: no reference at all. A source out of reach is no "none":
    /// it waits for a round. A record had (FX01's Tide Tables, its files and no list agreeing) is no Soulseek's (R26, N9): no
    /// item is made for it.
    #[tokio::test]
    async fn a_record_with_no_tracklist_says_why() {
        let (cat, items, d) = fx01("no-list");
        let checked = |_| std::future::ready(Err(anyhow::Error::from(crate::bandcamp::NotRead)));
        assert_eq!(listing(of_request(&items, 8), &cat, checked).await, Listing::None("No list to judge by".into()), "Quiet Lanes holds no reference");
        assert!(items.iter().all(|i| i.record.request != 6), "Tide Tables is had");
        let away = |_| std::future::ready(Err(anyhow::anyhow!("MusicBrainz could not be reached")));
        assert!(matches!(listing(of_request(&items, 2), &cat, away).await, Listing::Later(_, ROUND_AT_LEAST)), "Harbour Lights holds its release group");
        drop(cat);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_kept_item_reads_back_and_a_time_that_cannot_be_read_counts_as_due() {
        let raw = r#"{"next":3,"items":[{"id":"ss-3","record":{"artist":"Air","album":"Moon Safari","release_group":"rg","request":7},"state":"waiting"}]}"#;
        let k: Kept = serde_json::from_str(raw).unwrap();
        assert_eq!((k.items[0].due, k.items[0].misses, k.items[0].record.request), (0, 0, 7), "no time kept: due");
    }
}
