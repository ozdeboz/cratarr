//! Soulseek in Cratarr (0.15, the owner, 2026-11-02: "Cratarr does Soulseek, Lidarr stays"): soularr goes; Cratarr
//! fetches whole records from Soulseek its own way. Lidarr and Soulseek are sibling fetchers, either or both (the
//! owner, 2026-11-03): Lidarr fetches its own Wanted through its own downloaders.
//!
//! - **What is wanted:** Cratarr's approved Requests, and only those (the owner, 2026-11-03: "The whole idea is that
//!   Cratarr's Soulseek doesn't need Lidarr"); read each round. One taken off the list stays off. An item whose record is had
//!   (arrived or done), or whose request left, goes unless it is placed: a fetch, a check or a placing under way is cancelled and
//!   its folder removed (Y2). A placed copy partly there lets Soulseek look again; one gone (deleted from Incoming, its folder read
//!   and its record holding no copy) makes way, and the record's request is searched again (V1); only a last copy that went to an
//!   Incoming library's trash is gone so (U7), and one kept into Music and deleted there later is never looked for again. A placing
//!   past the list's last look, just before its rename, is never cancelled: it finishes, and its item is judged once placed (V4).
//! - **Judged by its list:** each request's record is judged by its counting list (`references::tracklist`, Q31): for a record
//!   not had, its references' in order, MusicBrainz's release (or one of its release group's), Discogs', Bandcamp's album. A
//!   Soulseek folder is a candidate only when every track of the list is one of its files, by title and length (`folders_of`),
//!   never because its name looks right. A list without its tracks' lengths gives way to the next in the order that has them;
//!   where no list with lengths finds a folder in any rung of a search, the same answers are judged by it, by titles and count,
//!   each such folder said to have no lengths to check beside its tick (R33, the owner's: `untimed_folders`), and gated by
//!   that list. A record with no list to judge by is Stuck, saying why: Stuck is for what a search cannot change (no list at
//!   all, none readable). Nothing found, by either list, is a search's answer: searched again later, as ever.
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
//!   (decision 0096), as Artist/Record (Year), for the intake card. Never straight into Music. Placed whole through a hidden
//!   folder, the record's cover beside it (`cover.jpg`, its large picture: Y3); a placing that fails keeps its checked files and
//!   is tried again (X4).
//!   Trouble on Cratarr's own side (a disk full or read-only, slskd's folder refused it or gone) blames no user; free space is
//!   looked at before a fetch. Only slskd's own words that it could not store a file count as such (Y9); a name too long for its
//!   disk is the folder's, and its user is failed (V5). A part of its path or a file of it not found is slskd's own folder gone
//!   (U1): a peer's names cannot make it. Three in a row hold the record a day, and Health says Soulseek can't store files (Y9).
//! - **Kept on disk** (`soulseek.json` in the data folder), so a restart forgets nothing: a fetch under way is followed
//!   again.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::track_fetch::{NotAsked, ReleaseTrack};
use crate::lidarr::Progress;
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
/// Fetches failed on Cratarr's own side in a row after which a record holds a day (Y9): its folder is not asked of its user every
/// five minutes all day.
const LOCAL_AT_MOST: u32 = 3;
/// How long such a record holds (Y9).
const LOCAL_HOLD: i64 = 86_400;
/// Folders a record shows, best first.
const SHOWN: usize = 5;
/// The shortest wait between two rounds of searches for one record (the ladder's "at once" still waits this).
const ROUND_AT_LEAST: i64 = 15 * 60;
/// A file's length may be this far from its track's.
const LENGTH_SLACK_MS: i64 = crate::slskd::LENGTH_SLACK_MS;
/// A record held at its check is checked again after this long: each time decodes a file to ask AcoustID about it.
const CHECK_AGAIN: i64 = 15 * 60;
/// A record whose placing failed is placed again from its checked files after this long (X4).
const PLACE_AGAIN: i64 = 15 * 60;
/// Placings failed in a row after which a record holds a day (Y12): one that fails for good is not tried every fifteen minutes.
const PLACE_AT_MOST: u32 = 3;
/// How long such a record holds (Y12).
const PLACE_HOLD: i64 = 86_400;
/// The data folder's folder for folders set aside.
const SET_ASIDE: &str = "soulseek-set-aside";
/// A folder set aside is kept this long, then deleted (X4).
const SET_ASIDE_KEPT: i64 = 14 * 86_400;
/// Room kept free beyond a fetch's files, on slskd's disk and the library's (X4).
const ROOM_SPARE: u64 = 1 << 30;
/// A placed album's `cover.jpg` is no larger than this on its longer side (Y3).
const COVER_PX: u32 = 1200;
/// The most of a placed album's cover read (Y3): an original at the Cover Art Archive may be large.
const COVER_MOST: usize = 10 * 1024 * 1024;
/// The longest side of a cover decoded for `cover.jpg` (V7): a larger one is refused from its header, never decoded (an 8000-px
/// original is some 190 MB of pixels, beside the server's own memory).
const COVER_READ_PX: u32 = 6000;

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
    /// The batch whose folder holds its checked files, its transfers let go, after a placing that failed (X4): placed again from
    /// them, nothing fetched again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<String>,
    /// Fetches in a row that failed on Cratarr's own side (X4, Y9: slskd could not store a file): `LOCAL_AT_MOST` hold it a day, and
    /// Health says Soulseek can't store files. A fetch that comes through, or a placing, ends the row.
    #[serde(default)]
    pub local_failures: u32,
    /// The folder its album is being placed in, noted before the rename (Y10): a stop between the rename and the Placed write is taken
    /// as placed when the item is taken up again, never placed twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placing: Option<String>,
    /// When its placing began (epoch s, Y10): only files that came to its folder since then are its album's (`repoint`).
    #[serde(default)]
    pub placed_at: i64,
    /// Placings in a row that failed (Y12): `PLACE_AT_MOST` hold it a day, and Health says Soulseek can't place albums. A placing that
    /// comes through ends the row.
    #[serde(default)]
    pub place_failures: u32,
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
    /// Records taken off the list (X2: by record, so another request of one does not bring it back): not taken in again while a
    /// request names them.
    #[serde(default)]
    dropped_records: Vec<i64>,
    /// Requests taken off the list, as 0.16 and 0.17's previews kept them: read once, each made its record (`take_requests`).
    #[serde(default, skip_serializing)]
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
/// given its record by the first round of requests, which can come while its first fetch at the upgrade start runs. None once it is
/// off the list (Y2: its record had otherwise, or its request gone): it is placed nowhere.
fn record_now(k: &Kept, id: &str) -> Option<Record> {
    k.items.iter().find(|i| i.id == id).map(|i| i.record.clone())
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

/// Whether a folder's name is one disc's of a release (X3): "CD1", "CD 2", "Disc 1", "Disk 2 - …".
fn is_disc_folder(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    ["cd", "disc", "disk"].iter().any(|p| n.strip_prefix(p).is_some_and(|rest| rest.trim_start().starts_with(|c: char| c.is_ascii_digit())))
}

/// The disc folder a file lies in (`CD2`), where it lies in one.
fn disc_of(path: &str) -> Option<&str> {
    let dir = path.rsplit_once(['\\', '/'])?.0;
    let last = dir.rsplit_once(['\\', '/']).map_or(dir, |(_, l)| l);
    is_disc_folder(last).then_some(last)
}

/// Words that make a recording another version of its title (X3): a file's title saying one its list's title does not is not
/// that track.
const VERSIONS: [&str; 12] = ["instrumental", "live", "remix", "mix", "acoustic", "demo", "karaoke", "acapella", "cappella", "unplugged", "reprise", "edit"];

/// Whether a file's title (one way its name reads, `titles_of`) is a list track's (X3): as `renumber::same_title` has it, but an
/// "Artist - Title" list compares its title part, never its artist alone; and a version word in brackets or after " - " that the
/// list's title lacks (Instrumental, Live, Remix…) makes the file another version. Pure.
fn titled_as(file: &str, list: &str) -> bool {
    let words = |s: &str| -> Vec<String> { s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect() };
    let listed = words(list);
    let lower = file.to_lowercase();
    let marked: Vec<&str> = lower.split(['(', '[']).skip(1).chain(lower.split(" - ").skip(1)).collect();
    if marked.iter().any(|part| words(part).iter().any(|w| VERSIONS.contains(&w.as_str()) && !listed.contains(w))) {
        return false;
    }
    match list.split_once(" - ") {
        Some((_, title)) => crate::app::import::loose(file) == crate::app::import::loose(list) || super::renumber::same_title(file, title.trim()),
        None => super::renumber::same_title(file, list),
    }
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
            let dir = f.filename.rsplit_once(['\\', '/']).map_or("", |(d, _)| d);
            // A release split into disc folders (CD1, CD2…) is one candidate, its folders' parent (X3).
            let dir = match dir.rsplit_once(['\\', '/']) {
                Some((parent, last)) if is_disc_folder(last) => parent,
                _ => dir,
            };
            dirs.entry(dir.to_string()).or_default().push(f);
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
                            && titles_of(&f.filename).iter().any(|n| titled_as(n, &t.title))
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
    if f.unchecked.as_deref() == Some(NO_LENGTHS) { &it.untimed } else { &it.tracks }
}

/// The folders among a search's answers that hold every track of the list and more audio files besides (X3: a fuller release
/// for a shorter list, a single's list in an album's folder): a fallback after the folders of the list's own count, each saying
/// what it takes of it ("Takes 10 of 14 files"). Only the list's files are fetched. Pure.
fn fuller_folders(answers: &[Answer], tracks: &[Track], failed: &[String]) -> Vec<Folder> {
    folders_of(answers, tracks, failed)
        .into_iter()
        .filter(|f| f.extra > 0)
        .map(|f| Folder { unchecked: Some(format!("Takes {} of {} files", f.files.len(), f.files.len() + f.extra)), ..f })
        .collect()
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

/// What Soulseek says of a request's record (W5): its item's state, as the request reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Word {
    /// Being searched now.
    Searching,
    /// Folders found, waiting for the administrator's tick.
    Found,
    /// Ticked and fetched (how far, 0 to 100), or its files checked and placed.
    Fetching { percent: f64 },
    /// To be searched, again where a search found nothing: why, where it says.
    Waiting {
        #[serde(skip_serializing_if = "Option::is_none")]
        why: Option<String>,
    },
    /// Something it cannot get past by itself (no list to judge it by), and why.
    Stuck {
        #[serde(skip_serializing_if = "Option::is_none")]
        why: Option<String>,
    },
}

/// What Soulseek says of these requests (W5): each one's item, by the request's id.
pub fn item_words(requests: &[i64]) -> std::collections::HashMap<i64, Word> {
    kept(|k| words_in(&k.items, requests))
}

/// [`item_words`]'s: pure (V1). A request with two items (a placed copy short of its list and the one looking again, Y2) reads the
/// later.
fn words_in(items: &[Item], requests: &[i64]) -> std::collections::HashMap<i64, Word> {
    items
        .iter()
        .filter(|i| requests.contains(&i.record.request))
        .map(|i| {
            let word = match i.state {
                State::Searching => Word::Searching,
                State::Found => Word::Found,
                State::Approved | State::Fetching => Word::Fetching { percent: i.percent },
                State::Checking | State::Placed => Word::Fetching { percent: 100.0 },
                State::Waiting => Word::Waiting { why: i.why.clone() },
                State::Stuck => Word::Stuck { why: i.why.clone() },
            };
            (i.record.request, word)
        })
        .collect()
}

/// The records on the list (RT01: a record not had is never forgotten while an item of it is there); an item 0.16 kept with
/// no record yet (0) is left out: its request keeps its record. Read once the list is (`start`), as every reader of it.
pub fn item_records() -> std::collections::BTreeSet<i64> {
    kept(|k| k.items.iter().map(|i| i.record.record).filter(|record| *record != 0).collect())
}

/// The records held a day for Cratarr's own trouble (Y9: `LOCAL_AT_MOST` fetches in a row slskd could not store), the latest held
/// first, each as "Artist - Record" with why: Health's "Soulseek can't store files".
pub fn cant_store(now: i64) -> Vec<(String, String)> {
    kept(|k| cant_store_in(&k.items, now))
}

/// [`cant_store`]'s: pure.
fn cant_store_in(items: &[Item], now: i64) -> Vec<(String, String)> {
    held_in(items, now, |i| i.local_failures >= LOCAL_AT_MOST)
}

/// The records held a day for their placing failing (Y12: `PLACE_AT_MOST` in a row), as [`cant_store`] gives its own: Health's
/// "Soulseek can't place albums".
pub fn cant_place(now: i64) -> Vec<(String, String)> {
    kept(|k| cant_place_in(&k.items, now))
}

/// [`cant_place`]'s: pure.
fn cant_place_in(items: &[Item], now: i64) -> Vec<(String, String)> {
    held_in(items, now, |i| i.place_failures >= PLACE_AT_MOST)
}

/// The items `held` picks that are still held at `now`, the latest held first, each as "Artist - Record" with why. Pure.
fn held_in(items: &[Item], now: i64, held: impl Fn(&Item) -> bool) -> Vec<(String, String)> {
    let mut found: Vec<&Item> = items.iter().filter(|i| held(i) && i.due > now).collect();
    found.sort_by_key(|i| std::cmp::Reverse(i.due));
    found.into_iter().map(|i| (format!("{} - {}", i.record.artist, i.record.album), i.why.clone().unwrap_or_default())).collect()
}

/// Tests elsewhere: an item of `record` on the list as `id`, its folders found and waiting for a tick, so no worker takes it up.
#[cfg(test)]
pub fn list_for_tests(id: &str, record: i64) {
    let r = Record { record, artist: "Glass Harbour".into(), album: "Undertow".into(), year: None, request: 0 };
    let i = Item {
        id: id.into(),
        record: r,
        state: State::Found,
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
        checked: None,
        local_failures: 0,
        placing: None,
        placed_at: 0,
        place_failures: 0,
    };
    kept(|k| k.items.push(i));
}

/// Tests elsewhere: a Placed item of `record` on the list as `id`, its folder read (U3).
#[cfg(test)]
pub fn placed_for_tests(id: &str, record: i64) {
    list_for_tests(id, record);
    kept(|k| {
        if let Some(i) = k.items.iter_mut().find(|i| i.id == id) {
            i.state = State::Placed;
        }
    });
}

/// Tests elsewhere: whether an item `id` is on the list.
#[cfg(test)]
pub fn listed_for_tests(id: &str) -> bool {
    kept(|k| k.items.iter().any(|i| i.id == id))
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
        // By its record (X2); one with no record yet by its request, made its record when it has one.
        if i.record.record != 0 {
            k.dropped_records.push(i.record.record);
        } else {
            k.dropped.push(i.record.request);
        }
        // Its checked files too (X4): their folder is a batch's.
        i.batch.or(i.checked)
    })
}

/// The placed records cleared from the list.
pub fn clear_placed(app: &Arc<super::App>) -> View {
    kept(|k| k.items.retain(|i| i.state != State::Placed));
    view(app)
}

/// A record's Placed items forgotten (V1): a Delete took its copy from Incoming (any of them, U3: `IncomingService::delete_listed`),
/// so none of them stands for one any more, nor keeps the record's next request from being searched. Its items under way stay.
pub fn forget_placed(record: i64) {
    kept(|k| k.items.retain(|i| !(i.state == State::Placed && i.record.record == record)));
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
pub fn start(app: &Arc<super::App>) {
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
    let round_app = app.clone();
    tokio::spawn(async move {
        let app = round_app;
        loop {
            // Nothing searched until an Incoming library takes Soulseek's records (soularr may still be at work).
            if library().is_err() {
                let _ = tokio::time::timeout(Duration::from_secs(600), WAKE.notified()).await;
                continue;
            }
            // The approved Requests, read each round (they are Cratarr's own: no service asked).
            if let Err(e) = read_requests(&served, &app).await {
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
        // Hidden folders placings left (Y12): swept once, before this start's first fetch, so none of its own is taken.
        sweep_at_start().await;
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

/// [`sweep_hidden`] in the Soulseek library, once at the start (Y12), the items as the list holds them.
async fn sweep_at_start() {
    let Ok(lib) = library() else { return };
    let items = kept(|k| k.items.clone());
    let gone = tokio::task::spawn_blocking(move || sweep_hidden(&lib, &items)).await.unwrap_or_default();
    if !gone.is_empty() {
        tracing::info!(target: "soulseek", count = gone.len(), "hidden folders placings left behind removed");
    }
}

/// The hidden folders placings left in the library `lib` (Y12): one an attempt whose request changed left, or whose item went, is
/// named by no item. Each `.cratarr-<record>-<request>.part` at its root is removed but an item's whose placing is noted (`placing`):
/// what was removed. Blocking.
fn sweep_hidden(lib: &Path, items: &[Item]) -> Vec<PathBuf> {
    let placing: Vec<PathBuf> = items.iter().filter(|i| i.placing.is_some()).map(|i| hidden_folder(lib, &i.record)).collect();
    let hidden = |name: &str| {
        name.strip_prefix(".cratarr-")
            .and_then(|n| n.strip_suffix(".part"))
            .and_then(|n| n.split_once('-'))
            .is_some_and(|(record, request)| record.parse::<i64>().is_ok() && request.parse::<i64>().is_ok())
    };
    let mut gone = Vec::new();
    for e in std::fs::read_dir(lib).into_iter().flatten().flatten() {
        let p = e.path();
        if hidden(&*e.file_name().to_string_lossy()) && p.is_dir() && !placing.contains(&p) && std::fs::remove_dir_all(&p).is_ok() {
            gone.push(p);
        }
    }
    gone
}

/// The approved Requests taken in (decision 0099's amendment: Soulseek never reads Lidarr's Wanted), each by its record
/// (C51b, `requests::records_of`): one whose record is not on the list yet is added, waiting to be searched; one no longer
/// approved (cancelled, declined) goes unless it is placed, a fetch, a check or a placing under way cancelled (Y2).
async fn read_requests(served: &Arc<crate::serve::Serve>, app: &Arc<super::App>) -> Result<()> {
    // Folders set aside past their 14 days, looked for hourly (X4).
    static SWEPT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    if let Some(data) = served.data_dir.clone()
        && now() - SWEPT.load(std::sync::atomic::Ordering::Relaxed) >= 3600
    {
        SWEPT.store(now(), std::sync::atomic::Ordering::Relaxed);
        tokio::task::spawn_blocking(move || sweep_set_asides(&data, now())).await.ok();
    }
    let (ud, cat, kinds) = (served.userdata.clone(), served.catalogue().clone(), served.library.kinds());
    // Placed for a record retired meanwhile (X4, D17): its request follows the record its album joined or made, once its folder is
    // read (Y10), which the item then lets go of.
    let placed: Vec<(String, i64, i64, String, i64)> = kept(|k| {
        k.items.iter().filter(|i| i.state == State::Placed && i.record.record != 0).filter_map(|i| Some((i.id.clone(), i.record.request, i.record.record, i.placed.clone()?, i.placed_at))).collect()
    });
    let (approved, of, (moved, read)) = tokio::task::spawn_blocking(move || -> Result<_> {
        let moved = repoint(&ud, &cat, &placed)?;
        // Every approved request (X2), not the newest thousand: those whose record is complete give no item (`take_requests`).
        let approved = ud.requests(None, Some(super::requests::APPROVED), usize::MAX)?;
        let mut of = super::requests::records_held(&cat, &approved, &kinds)?;
        // Where each record holding no copy saw its last one go (U7, T8): only one gone from Incoming, deleted there or removed outside
        // Cratarr, is looked for again.
        super::requests::mark_gone_from_incoming(&cat, &mut of, &kinds)?;
        Ok((approved, of, moved))
    })
    .await??;
    if !moved.is_empty() || !read.is_empty() {
        kept(|k| {
            for (request, _, to) in &moved {
                for i in k.items.iter_mut().filter(|i| i.record.request == *request) {
                    i.record.record = *to;
                }
            }
            // Once (Y10): its folder read, an item lets go of it, and nothing that comes there later re-points its request.
            for i in k.items.iter_mut().filter(|i| read.contains(&i.id)) {
                i.placed = None;
            }
        });
    }
    // The owner's 6 November order (X1): Lidarr first, Soulseek five minutes after, or at once where Lidarr is not connected or did
    // not take it; an album not out yet waits for its release.
    let lidarr = super::requests::fetcher(app);
    let firsts: Vec<i64> = approved.iter().zip(&of).map(|(r, o)| first_search(r, o.as_ref().and_then(|o| o.released.as_deref()).and_then(super::requests::release_day), lidarr.is_some())).collect();
    let taken = kept(|k| take_requests(k, &approved, &of, &firsts));
    // Lidarr's grab failed or stalls, or it does not know the album, or does not want it: Soulseek's turn now, its release come.
    if let Some(f) = lidarr {
        let t = now();
        let waiting: Vec<(String, i64)> = kept(|k| k.items.iter().filter(|i| i.state == State::Waiting && i.due > t && i.misses == 0).map(|i| (i.id.clone(), i.record.request)).collect());
        let groups: HashMap<i64, String> = approved.iter().zip(&of).filter_map(|(r, o)| Some((r.id, r.release_group.clone().or_else(|| o.as_ref()?.groups.first().cloned())?))).collect();
        let unreleased: Vec<i64> = approved.iter().zip(&firsts).filter(|(r, first)| **first > r.decided_at.unwrap_or(r.at) + LIDARR_FIRST).map(|(r, _)| r.id).collect();
        let asked: Vec<String> = waiting.iter().filter(|(_, r)| !unreleased.contains(r)).filter_map(|(_, r)| groups.get(r).cloned()).collect();
        if !asked.is_empty()
            && let Ok(progress) = f.statuses(&asked).await
        {
            kept(|k| bring_forward(&mut k.items, &groups, &unreleased, &progress, t));
        }
    }
    // The first arrival stops the other (X1), and a request that left takes its item with it (Y2): a fetch, a check or a placing
    // under way is cancelled, in slskd and on disk.
    if !taken.is_empty()
        && let Ok((slskd, downloads)) = super::track_fetch::slskd_ready().await
    {
        for it in &taken {
            cancel_fetch(&slskd, &downloads, it).await;
        }
    }
    Ok(())
}

/// Requests whose record is retired re-pointed to the record their placed album joined or made (X4, D17): one on a retired record
/// (the reject workflow) is otherwise never done, and its album is fetched again after Clear placed. Once (Y10): at the first pass
/// that reads the folder, by the files that came to it since its placing alone, and for an approved request only. `placed` is
/// (item, request, record, the folder its album was placed in, when its placing began); what moved, as (request, from, to), and the
/// items whose folder was read, which let go of it. Blocking.
fn repoint(ud: &crate::userdata::UserData, cat: &crate::catalogue::Catalogue, placed: &[(String, i64, i64, String, i64)]) -> Result<(Vec<(i64, i64, i64)>, Vec<String>)> {
    let read = cat.requested(&placed.iter().map(|(_, _, record, ..)| *record).collect::<Vec<_>>())?;
    let (mut moved, mut done) = (Vec::new(), Vec::new());
    for (item, request, record, at, since) in placed {
        // Not read yet: looked at again next round.
        let Some(joined) = cat.record_placed_in(Path::new(at), *since)? else { continue };
        done.push(item.clone());
        let retired = read.get(record).is_some_and(|q| q.row.retired_at.is_some());
        let approved = ud.request(*request)?.is_some_and(|r| r.status == super::requests::APPROVED);
        if retired && approved && joined != *record {
            ud.set_request_record(*request, joined)?;
            tracing::info!(target: "soulseek", request, from = record, to = joined, "a request on a retired record follows its placed album");
            moved.push((*request, *record, joined));
        }
    }
    Ok((moved, done))
}

/// How long Soulseek leaves a request to Lidarr, which goes first (X1, the owner's 6 November order).
const LIDARR_FIRST: i64 = 5 * 60;

/// When Soulseek may first search a request's record (X1, the owner's 6 November order): five minutes after it was handed over,
/// Lidarr first; at once where Lidarr is not connected or did not take it ("Not handed over to Lidarr", as when it does not know
/// the album); never before its release (`released`, its day) where it is not out yet. Pure.
pub fn first_search(r: &crate::userdata::Request, released: Option<i64>, lidarr: bool) -> i64 {
    let handed = r.decided_at.unwrap_or(r.at);
    let refused = r.fetched.as_deref().is_some_and(|f| f.contains("Not handed over to Lidarr"));
    let after = if lidarr && !refused { handed + LIDARR_FIRST } else { handed };
    after.max(released.unwrap_or(0))
}

/// Items waiting on Lidarr brought forward to `now` where Lidarr cannot fetch their record (X1): its grab failed or stalls, or it
/// does not know the album, or does not want it. Not those whose release is still to come. Pure.
fn bring_forward(items: &mut [Item], groups: &HashMap<i64, String>, unreleased: &[i64], progress: &HashMap<String, Progress>, now: i64) {
    for i in items.iter_mut().filter(|i| i.state == State::Waiting && i.due > now && !unreleased.contains(&i.record.request)) {
        let Some(p) = groups.get(&i.record.request).and_then(|g| progress.get(g)) else { continue };
        if matches!(p, Progress::Stuck | Progress::Unknown | Progress::NotWanted) {
            i.due = now;
        }
    }
}

/// A fetch under way stopped (X1: the first arrival stops the other; Y2: a request that left, too): its transfers dropped in slskd,
/// what came of them removed from slskd's batch folder, and so the checked files a placing that failed kept (X4).
async fn cancel_fetch(slskd: &Slskd, downloads: &Path, it: &Item) {
    if let Some(batch) = &it.batch
        && let Some(user) = it.chosen.and_then(|c| it.folders.get(c)).map(|f| f.user.clone())
        && let Ok(ts) = slskd.transfers(batch).await
    {
        for t in ts {
            slskd.drop_transfer(&user, &t.id).await;
        }
    }
    for folder in it.batch.iter().chain(&it.checked) {
        std::fs::remove_dir_all(downloads.join(crate::slskd::FOLDER).join(folder)).ok();
    }
    tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), "its fetch stopped: it arrived otherwise, or its request left");
}

/// [`read_requests`]'s list kept, each request's record as `records_of` read it (`of`, in the requests' order): pure. Every
/// record is taken, with a release group or none: it is judged by its list, whosever that is (0100). An item 0.16 kept, by its
/// release group, is given its request's record (row 9.26) and goes on as it was: a fetch under way keeps its batch, its
/// folder and its tick, and is written back with its record. The items it takes off with a fetch or checked files are handed back,
/// to be cancelled (`cancel_fetch`).
fn take_requests(k: &mut Kept, approved: &[crate::userdata::Request], of: &[Option<super::requests::RecordOf>], firsts: &[i64]) -> Vec<Item> {
    let ids: std::collections::HashSet<i64> = approved.iter().map(|r| r.id).collect();
    let record_of: BTreeMap<i64, i64> = approved.iter().zip(of).filter_map(|(r, o)| Some((r.id, o.as_ref()?.record))).collect();
    // A placed copy gone (V1: deleted from Incoming, its folder read and its record holding no copy, U7, T8: its last copy gone from
    // an Incoming library) makes way: its item goes, never handed to another request of its record (X2), nor keeping that request
    // from being searched.
    let held: BTreeMap<i64, (super::requests::Holding, bool)> = of.iter().flatten().map(|o| (o.record, (o.holding, o.gone_from_incoming))).collect();
    k.items.retain(|i| !copy_gone(i, &held));
    // A request that left (cancelled, declined, rejected): its item goes to another approved request of its record, keeping its tick
    // and folders (X2); else it goes unless it is placed (Y2): one ticked, fetched, checked or held at its check is cancelled with it.
    for i in k.items.iter_mut().filter(|i| !ids.contains(&i.record.request) && i.record.record != 0) {
        if let Some(other) = record_of.iter().filter(|(_, record)| **record == i.record.record).map(|(id, _)| *id).min() {
            i.record.request = other;
        }
    }
    let mut stopped = take_off(&mut k.items, |i| !ids.contains(&i.record.request) && !stays(i));
    // Taken off by record (X2): 0.16's, by request, made their records once; one no request names any more forgotten.
    for id in std::mem::take(&mut k.dropped) {
        match record_of.get(&id) {
            Some(record) if !k.dropped_records.contains(record) => k.dropped_records.push(*record),
            Some(_) => {}
            None if ids.contains(&id) => k.dropped.push(id),
            None => {}
        }
    }
    k.dropped_records.retain(|record| record_of.values().any(|r| r == record));
    for i in k.items.iter_mut().filter(|i| i.record.record == 0) {
        if let Some((_, Some(o))) = approved.iter().zip(of).find(|(r, _)| r.id == i.record.request) {
            i.record.record = o.record;
        }
    }
    // A record already had, by its own files or a record holding them (the one had rule, R26, N9), is not searched: none is made for
    // it (Clear placed, or Lidarr's delivery, is no reason to fetch it again).
    let had: std::collections::HashSet<i64> = of.iter().flatten().filter(|o| o.had()).map(|o| o.record).collect();
    // The first arrival stops the other (X1): an item of it goes unless it is placed, which is the arrival, as nothing is seen before
    // its placing (Y2, X4); one ticked, fetched, checked or held at its check is cancelled.
    stopped.extend(take_off(&mut k.items, |i| had.contains(&i.record.record) && !stays(i)));
    for (n, (r, o)) in approved.iter().zip(of).enumerate() {
        if k.dropped.contains(&r.id) || o.as_ref().is_some_and(|o| k.dropped_records.contains(&o.record)) {
            continue;
        }
        // No record yet (0.17's step gives each one at the start): taken once it has one; a had one, never.
        let Some(o) = o.as_ref().filter(|o| !o.had()) else { continue };
        // One item a record; a placed copy partly there lets Soulseek look again, its item kept as history (Y2).
        if k.items.iter().any(|i| (i.record.request == r.id || i.record.record == o.record) && !looks_again(i, o.holding)) {
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
            checked: None,
            local_failures: 0,
            placing: None,
            placed_at: 0,
            place_failures: 0,
        });
        // Its first search as the order has it (X1, `first_search`).
        if let Some(i) = k.items.last_mut() {
            i.due = firsts.get(n).copied().unwrap_or(0);
        }
    }
    stopped.into_iter().filter(|i| i.batch.is_some() || i.checked.is_some()).collect()
}

/// The items `goes` picks taken off the list, in their order (`take_requests`).
fn take_off(items: &mut Vec<Item>, goes: impl Fn(&Item) -> bool) -> Vec<Item> {
    let (gone, stay): (Vec<Item>, Vec<Item>) = std::mem::take(items).into_iter().partition(goes);
    *items = stay;
    gone
}

/// Whether an item stays on the list whatever became of its request or its record (`take_requests`): placed (Y2), or its placing
/// noted, past the list's last look before its rename (V4): that placing finishes, and the item is judged once placed.
fn stays(i: &Item) -> bool {
    i.state == State::Placed || i.placing.is_some()
}

/// Whether a placed item lets another item of its record be made (Y2): the record reads partly there, and its own copy is short of
/// the record's list, or has been read (as many tracks there as it placed) and the record is short still. A whole copy no pass has
/// read yet waits for one, which may make its record had: till then the record is partly there by another copy. Pure.
fn looks_again(i: &Item, holding: super::requests::Holding) -> bool {
    let placed = i.chosen.and_then(|c| i.folders.get(c)).map_or(0, |f| f.files.len());
    i.state == State::Placed && matches!(holding, super::requests::Holding::Partly { has, of } if placed < of as usize || has as usize >= placed)
}

/// Whether a placed item's copy is gone (V1): a pass read its folder (Y10 lets go of it then), and its record, as `held` has the
/// records of the approved requests (how each is held, and whether its last copy went from an Incoming library), now holds no copy,
/// its last one gone from Incoming (U7, T8: deleted there, or removed outside Cratarr). One kept into a main library and deleted
/// there later is not: its request stays
/// as delivered, never looked for again. One no pass has read yet may be on its way into the catalogue. Pure.
fn copy_gone(i: &Item, held: &BTreeMap<i64, (super::requests::Holding, bool)>) -> bool {
    i.state == State::Placed && i.placed.is_none() && held.get(&i.record.record) == Some(&(super::requests::Holding::None, true))
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
        .find(|i| matches!(i.state, State::Fetching | State::Checking) && (i.batch.is_some() || i.checked.is_some()))
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

/// A record's counting list, read through `read` (`references::tracklist`, kept there once read). None is read while background
/// reading is paused (Y7): the item waits a round, never Stuck.
async fn listing<R, F>(it: &Item, cat: &Arc<crate::catalogue::Catalogue>, read: R) -> Listing
where
    R: Fn(crate::catalogue::records::Ref) -> F,
    F: std::future::Future<Output = Result<Option<super::references::Read>>>,
{
    if it.record.record == 0 {
        return Listing::None("No record to judge it by".into());
    }
    if super::switches::background_paused() {
        return Listing::Later(super::switches::PAUSED.into(), ROUND_AT_LEAST);
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
                // Folders of the list's own count first (X3): one holding more files is a fallback after every rung.
                let found: Vec<Folder> = folders_of(&a, &it.tracks, &it.failed).into_iter().filter(|f| f.extra == 0).collect();
                if !found.is_empty() {
                    tracing::info!(target: "soulseek", record = %record, search = %q, folders = found.len(), "found");
                    return with_item(id, |i| {
                        (i.folders, i.state, i.misses, i.why) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None);
                    });
                }
                heard.extend(a);
            }
            // slskd away says nothing of the record: no miss, or a week's wait would meet it when slskd came back.
            Err(e) if e.is::<Away>() => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(format!("{e:#}")), now() + ROUND_AT_LEAST)),
            Err(e) => why = Some(format!("{e:#}")),
        }
    }
    // A fuller folder for the list (X3): every track of it there, and more; offered for the tick, saying what it takes of it.
    let fuller = fuller_folders(&heard, &it.tracks, &it.failed);
    if !fuller.is_empty() {
        tracing::info!(target: "soulseek", record = %record, folders = fuller.len(), "found in fuller folders alone");
        return with_item(id, |i| {
            (i.folders, i.state, i.misses, i.why) = (fuller.into_iter().take(SHOWN).collect(), State::Found, 0, None);
        });
    }
    let found = untimed_folders(&heard, &it.untimed, &it.failed);
    if !found.is_empty() {
        tracing::info!(target: "soulseek", record = %record, folders = found.len(), "found by titles alone: no list with lengths found a folder");
        return with_item(id, |i| {
            (i.folders, i.state, i.misses, i.why) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None);
        });
    }
    // Not found: a search's answer, not a dead end, as Soulseek's users change by the hour (the coordinator, run 7b): searched
    // again later, by either list; its why says which could not find it.
    let missed = if it.untimed.is_empty() { "No folder holds every track of its release" } else { "No folder holds every track, even by titles" };
    with_item(id, |i| {
        i.misses += 1;
        (i.state, i.why, i.due) = (State::Waiting, why.or_else(|| Some(missed.into())), now() + wait_after(i.misses));
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
    // A placing a stop cut after its rename (Y10): its album is in place, so it is taken as placed, never placed again; what it came
    // from goes after.
    if let Some(to) = it.placing.as_deref().filter(|to| renamed(Path::new(to), &it.record)) {
        tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), to, "a placing cut short after its rename taken as placed");
        let to = to.to_string();
        with_item(id, |i| (i.state, i.placed, i.placing, i.batch, i.checked, i.why, i.local_failures, i.place_failures) = (State::Placed, Some(to), None, None, None, None, 0, 0));
        for b in it.batch.iter().chain(&it.checked) {
            std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(b)).ok();
        }
        return;
    }
    // One noted but cut before its rename (V4): no placing is under way now, so the round may take it off again until it is placed
    // anew, noted then in the list's last look.
    if it.placing.is_some() {
        with_item(id, |i| i.placing = None);
    }
    // Checked files whose placing failed (X4): placed again from where they were kept, nothing fetched or checked again.
    if let Some(checked) = &it.checked {
        let dir = folder.join(crate::slskd::FOLDER).join(checked);
        let files = files_under(&dir);
        if files.is_empty() {
            return with_item(id, |i| (i.state, i.checked, i.due, i.why) = (State::Approved, None, 0, Some("Its checked files were gone: fetched again".into())));
        }
        return place_from(id, &it, &dir, &files).await;
    }
    let Some(f) = it.chosen.and_then(|c| it.folders.get(c)).cloned() else {
        return with_item(id, |i| (i.state, i.chosen, i.due) = (State::Waiting, None, 0));
    };
    // Room for its files on slskd's disk and the library's before a fetch begins (X4): a disk full is no user's failure.
    if it.batch.is_none() {
        let need: u64 = f.files.iter().map(|o| o.size.max(0) as u64).sum();
        let dirs: Vec<PathBuf> = std::iter::once(folder.to_path_buf()).chain(library().ok()).collect();
        if let Some(why) = short_of_room(need, &dirs) {
            return with_item(id, |i| hold(i, why, SLSKD_AGAIN));
        }
    }
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
                // What came of it goes with it (X4): nothing else looks in a batch's folder.
                std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(&batch)).ok();
                // Cratarr's own side (X4, V5, U1: slskd's disk full or read-only, its folder refused it or gone): no user failed; the
                // same folder fetched again later.
                if e.is::<Local>() {
                    tracing::warn!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), reason = %format!("{e:#}"), "a fetch failed on Cratarr's side; held");
                    return with_item(id, |i| {
                        (i.state, i.batch, i.percent) = (State::Approved, None, 0.0);
                        // Three in a row (Y9): held a day, not asked of its user every five minutes all day; Health says why.
                        i.local_failures += 1;
                        let wait = if i.local_failures >= LOCAL_AT_MOST { LOCAL_HOLD } else { SLSKD_AGAIN };
                        hold(i, format!("{e:#}"), wait);
                    });
                }
                tracing::info!(target: "soulseek", user = %f.user, reason = %format!("{e:#}"), "a user failed; the next folder");
                return with_item(id, |i| {
                    on_failed(i, &f.user, format!("{e:#}"));
                });
            }
        }
    };
    // Its files in: a row of failures on Cratarr's side ends (Y9).
    with_item(id, |i| (i.state, i.percent, i.local_failures) = (State::Checking, 100.0, 0));
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
        Ok(files) => place_from(id, &it, &dir, &files).await,
        Err(why) => {
            let aside = set_aside(&dir, id, &f.user, &why);
            // Gone either way (X4): set aside, or where it could not be, removed.
            std::fs::remove_dir_all(&dir).ok();
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
            let said = bad.exception.as_deref().unwrap_or_default();
            // Cratarr's own side, not the user's (X4, Y9): slskd could not store it.
            if local_failure(bad) {
                return Err(Local(format!("slskd could not store {}: {said}", crate::slskd::base_name(&bad.filename))).into());
            }
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

/// A fetch that failed on Cratarr's own side (X4, V5, U1): slskd could not store a file (its disk full or read-only, its folder refused
/// it or gone). No user's failure.
#[derive(Debug)]
struct Local(String);

impl std::fmt::Display for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Local {}

/// .NET's exception types that are slskd's own disk's alone (Y9), as a transfer's exception may begin with one
/// ("System.UnauthorizedAccessException: …"): access to slskd's own folder denied, and a part of its path or a file of it not found
/// (U1). slskd makes every folder under its own download and incomplete folders, so a peer's names cannot make part of that path
/// missing: only slskd's own folder gone can, or a file taken from its incomplete folder before it was moved. A path too long
/// (`System.IO.PathTooLongException`) is the folder's own names' trouble (V5): its user is failed, and the next folder tried.
const LOCAL_KINDS: [&str; 3] = ["System.UnauthorizedAccessException", "System.IO.DirectoryNotFoundException", "System.IO.FileNotFoundException"];

/// .NET's own words for slskd's disk's trouble (Y9: no space, read-only, access denied, U1: a part of its path or a file not found),
/// as a transfer's exception begins with them, alone or after its type ("System.IO.IOException: No space left on device"): "No space
/// left on device : '/downloads/…'", "There is not enough space on the disk.", "Disk full", "Disk quota exceeded", "Read-only file
/// system", "Access to the path '/downloads/…' is denied.", "Could not find a part of the path '/downloads/…'.", "Could not find
/// file '/downloads/…'.".
const LOCAL_SAYS: [&str; 8] = [
    "No space left on device",
    "There is not enough space on the disk",
    "Disk full",
    "Disk quota exceeded",
    "Read-only file system",
    "Access to the path '",
    "Could not find a part of the path '",
    "Could not find file '",
];

/// Whether a failed transfer is slskd's own disk's trouble (X4, Y9), not its user's: errored, never refused ("Completed, Rejected"),
/// timed out or cancelled, and its exception one of .NET's local kinds (`LOCAL_KINDS`) or beginning with its words for them
/// (`LOCAL_SAYS`). Words anywhere else are its user's: a refusal ("Transfer rejected: Access to the path 'D:\…' is denied.") is
/// the user's own disk's, whatever it says.
fn local_failure(t: &crate::slskd::Transfer) -> bool {
    if !t.state.contains("Errored") {
        return false;
    }
    let said = t.exception.as_deref().unwrap_or_default().trim();
    let (kind, words) = match said.split_once(": ") {
        Some((kind, words)) if kind.starts_with("System.") && kind.ends_with("Exception") => (kind, words),
        _ => ("", said),
    };
    let words = words.to_lowercase();
    LOCAL_KINDS.contains(&kind) || LOCAL_SAYS.iter().any(|w| words.starts_with(&w.to_lowercase()))
}

/// Why a fetch of `need` bytes would not fit (X4): one of `dirs` (slskd's folder, the library) with less free than it and
/// `ROOM_SPARE`. Where its free space cannot be read, nothing is said.
fn short_of_room(need: u64, dirs: &[PathBuf]) -> Option<String> {
    dirs.iter().find_map(|d| {
        let free = crate::folders::free(d)?;
        let want = need.saturating_add(ROOM_SPARE);
        (free < want).then(|| format!("Too little free space for its files: {} MB needed, {} MB free", want / 1_000_000, free / 1_000_000))
    })
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
    // None of its files there at all (X4): slskd's folder is set wrong, or gone. No user's failure: held, as with AcoustID away.
    if on_disk.is_empty() && !f.files.is_empty() {
        return Err("Its files are not in slskd's folder: check slskd's folder in Settings › Server".into());
    }
    let mut out = Vec::new();
    let mut why = BTreeMap::new();
    for (offered, track) in f.files.iter().zip(release) {
        let name = crate::slskd::base_name(&offered.filename).to_string();
        // One disc's file in its disc folder (X3): two discs may name a file alike.
        let disc = disc_of(&offered.filename);
        let at = |p: &&PathBuf| p.file_name().is_some_and(|n| n.to_string_lossy() == name) && disc.is_none_or(|d| p.parent().and_then(Path::file_name).is_some_and(|n| n.to_string_lossy() == d));
        let Some(path) = on_disk.iter().find(at).cloned() else {
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

/// The checked files placed (X4), the record's cover beside them, and the batch's folder they came from removed. A placing that
/// fails keeps them there: the batch is let go (`checked`), no user is failed, and the record is placed again from them later. One
/// taken off the list meanwhile (Y2: its record had otherwise, or its request gone) is placed nowhere, and its files go.
async fn place_from(id: &str, it: &Item, dir: &Path, files: &[PathBuf]) {
    let lib = library();
    // Its cover only once there is somewhere to put it: it may have to be fetched.
    let cover = match &lib {
        Ok(_) => cover_for(kept(|k| record_now(k, id)).map_or(it.record.record, |r| r.record)).await,
        Err(_) => None,
    };
    // The list looked at again after the cover's wait (Y2): an item gone from it meanwhile is placed nowhere.
    let Some(r) = kept(|k| record_now(k, id)) else {
        std::fs::remove_dir_all(dir).ok();
        tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), "not placed: it left the list");
        return;
    };
    // Its target noted before the rename (Y10), with when, in the list's last look, which the rename follows (V4): an item taken off
    // since is placed nowhere, and one noted is taken off by nothing (`stays`).
    let noted = |to: &Path| kept(|k| note_placing(k, id, to, now()));
    let placed = lib.and_then(|lib| place(&lib, &r, files, cover.as_deref(), noted));
    match placed {
        Ok(to) => {
            tracing::info!(target: "soulseek", record = %format!("{} - {}", r.artist, r.album), to = %to.display(), "placed in Incoming");
            with_item(id, |i| {
                (i.state, i.placed, i.placing, i.batch, i.checked, i.why) = (State::Placed, Some(to.display().to_string()), None, None, None, None);
                (i.local_failures, i.place_failures) = (0, 0);
            });
            // What it came from goes after the Placed write (Y10): a stop between leaves it placed, never placed twice.
            std::fs::remove_dir_all(dir).ok();
        }
        // Taken off the list before its rename (V4): placed nowhere, and its files go, as at the list's first look.
        Err(e) if e.is::<LeftTheList>() => {
            std::fs::remove_dir_all(dir).ok();
            tracing::info!(target: "soulseek", record = %format!("{} - {}", r.artist, r.album), "not placed: it left the list before its rename");
        }
        Err(e) => {
            tracing::warn!(target: "soulseek", record = %format!("{} - {}", r.artist, r.album), reason = %format!("{e:#}"), "not placed; its checked files kept");
            let checked = dir.file_name().map(|n| n.to_string_lossy().into_owned());
            with_item(id, |i| {
                (i.state, i.batch, i.checked, i.placing) = (State::Checking, None, checked, None);
                // Three in a row (Y12): held a day, its files kept, not tried every fifteen minutes for good; Health says why.
                i.place_failures += 1;
                let wait = if i.place_failures >= PLACE_AT_MOST { PLACE_HOLD } else { PLACE_AGAIN };
                hold(i, format!("It could not be placed: {e:#}"), wait);
            });
        }
    }
}

/// A placing's target noted on its item, with when (Y10), in the list's last look before the rename (V4): whether the item is still on
/// the list, and so to be placed.
fn note_placing(k: &mut Kept, id: &str, to: &Path, now: i64) -> bool {
    let Some(i) = k.items.iter_mut().find(|i| i.id == id) else { return false };
    (i.placing, i.placed_at) = (Some(to.display().to_string()), now);
    true
}

/// A placing whose item left the list before its rename (V4): placed nowhere.
#[derive(Debug)]
struct LeftTheList;

impl std::fmt::Display for LeftTheList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("it left the list before its rename")
    }
}

impl std::error::Error for LeftTheList {}

/// The record's cover as JPEG, for `cover.jpg` beside its placed files (X4), from where it comes (`cover_source`), as its large
/// picture (Y3, `large_cover`), never the cover cache's thumbnail; none where it has none, or it cannot be had now.
async fn cover_for(record: i64) -> Option<Vec<u8>> {
    let cat = crate::serve::instance()?.catalogue().clone();
    let q = tokio::task::spawn_blocking(move || cat.requested(&[record])).await.ok()?.ok()?.remove(&record)?;
    let of = cover_source(q.row.cover.as_deref(), &q.groups)?;
    // The covers' hosts only, each answer read in pieces to its cap (O1).
    let http = super::reach::client(super::reach::Purpose::Covers).timeout(Duration::from_secs(30)).build().ok()?;
    let http = &http;
    large_cover(&of, |url: String| async move { super::cover_cache::image(http, &url, COVER_MOST).await }).await
}

/// An album's large picture, as `cover.jpg` takes it (Y3), each address read through `get` (the covers' client; the tests' stands
/// in): a release group's from the Cover Art Archive at 1200 px, else its original, else its Discogs master's picture as the cover
/// cache falls back to, never the archive's 250-px thumbnail; a Bandcamp album's at Bandcamp's large size (`_10`), as its page
/// names it. None where none of them gives one.
async fn large_cover<G, F>(of: &super::cover_cache::Of, get: G) -> Option<Vec<u8>>
where
    G: Fn(String) -> F,
    F: std::future::Future<Output = Result<Option<Vec<u8>>>>,
{
    use super::cover_cache::Of;
    match of {
        Of::ReleaseGroup(rg) => {
            let archive = ["front-1200", "front"].map(|size| format!("https://coverartarchive.org/release-group/{rg}/{size}"));
            if let Some(b) = first_picture(&archive, &get).await {
                return Some(b);
            }
            let other = super::discogs::release_group_cover(rg).await.ok().flatten().filter(|u| !u.contains("coverartarchive.org"))?;
            first_picture(&[other], &get).await
        }
        Of::Bandcamp(page) => {
            let large = crate::bandcamp::Bandcamp::get().album(page).await.ok()?.cover?;
            first_picture(&[large], &get).await
        }
    }
}

/// The first of `addresses` that gives a picture through `get`, as `cover.jpg` takes it (`cover_jpg`); one that cannot be read is
/// said in the log, and the next tried.
async fn first_picture<G, F>(addresses: &[String], get: &G) -> Option<Vec<u8>>
where
    G: Fn(String) -> F,
    F: std::future::Future<Output = Result<Option<Vec<u8>>>>,
{
    for url in addresses {
        match get(url.clone()).await {
            // Read on a blocking thread (V7): a large original's decode is seconds of work.
            Ok(Some(b)) => match tokio::task::spawn_blocking(move || cover_jpg(b)).await.ok().flatten() {
                Some(jpg) => return Some(jpg),
                None => tracing::warn!(target: "soulseek", url = %url, "a cover that is no picture Cratarr can read, or too large to read"),
            },
            Ok(None) => {}
            Err(e) => tracing::warn!(target: "soulseek", url = %url, reason = %format!("{e:#}"), "a cover not read"),
        }
    }
    None
}

/// A picture as `cover.jpg` takes it (Y3): a JPEG within `COVER_PX` as it came; any other made JPEG, scaled to fit. Its size is read
/// first, from its header (V7): one over `COVER_READ_PX` on its longer side is never decoded, and the decode itself is held to that
/// (`art::jpeg_within_at_most`). None where it is no picture Cratarr can read, or too large. Blocking.
fn cover_jpg(b: Vec<u8>) -> Option<Vec<u8>> {
    let r = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().ok()?;
    let jpeg = r.format() == Some(image::ImageFormat::Jpeg);
    let (w, h) = r.into_dimensions().ok()?;
    if w.max(h) > COVER_READ_PX {
        return None;
    }
    if jpeg && w.max(h) <= COVER_PX { Some(b) } else { crate::serve::art::jpeg_within_at_most(&b, COVER_PX, COVER_READ_PX).ok() }
}

/// Where a record's cover comes from (X4): its cover's own address (the Cover Art Archive's for a release group, Bandcamp's for an
/// album page), else its first release group's. Pure.
fn cover_source(cover: Option<&str>, groups: &[String]) -> Option<super::cover_cache::Of> {
    cover.and_then(super::cover_cache::Of::of_address).or_else(|| groups.first().map(|g| super::cover_cache::Of::ReleaseGroup(g.clone())))
}

/// The checked files into the Soulseek library `lib` (`place_with`); the record's folder. The catalogue is told first that the
/// album arriving there is the record (`Catalogue::expect_record`, C50), so its tracks join that record rather than make another,
/// and again once it is in place, from when the expectation waits its week (F9).
fn place(lib: &Path, r: &Record, files: &[PathBuf], cover: Option<&[u8]>, noted: impl FnOnce(&Path) -> bool) -> Result<PathBuf> {
    let served = crate::serve::instance();
    let expect = |to: &Path| match served.as_ref().filter(|_| r.record != 0).and_then(|s| Some((s, s.catalogue().place_of_path(to)?))) {
        Some((s, (folder, dir))) => s.catalogue().expect_record(folder, &dir, r.record),
        None => Ok(true),
    };
    place_with(lib, r, files, cover, expect, noted)
}

/// A record's hidden folder in the library `lib`, its album made in it before the rename (X4): its own, so a placing cut short is
/// begun again in it, not left beside it.
fn hidden_folder(lib: &Path, r: &Record) -> PathBuf {
    lib.join(format!(".cratarr-{}-{}.part", r.record, r.request))
}

/// Whether a placing noted for `to` went through its rename (Y10): its album's folder there, its hidden folder gone.
fn renamed(to: &Path, r: &Record) -> bool {
    to.is_dir() && to.parent().and_then(Path::parent).is_some_and(|lib| !hidden_folder(lib, r).exists())
}

/// How many folders a record is offered before its placing gives up: each was another record's expectation.
const FOLDERS_AT_MOST: usize = 50;

/// `place`, `expect` telling the catalogue that the album arriving in a folder is the record, and whether it took that. A
/// folder another record is still expected in is passed over for the next free one, numbered as a folder that is there already
/// (R27, N10): an album placed there would join that other record. The files are copied into a hidden folder of the library, the
/// cover beside them, and the folder renamed into place whole (X4): no catalogue pass sees half an album, and the checked files
/// stay where they are until it is in place, so a placing that fails, or is cut short, loses none. Said again once in place; a
/// failure then is said in the log and the files stay placed: they are in Incoming, and fetching them again would not help. Its
/// target is told to `noted` before the rename (Y10), and nothing is placed where `noted` says its item left the list (V4:
/// `LeftTheList`).
fn place_with(lib: &Path, r: &Record, files: &[PathBuf], cover: Option<&[u8]>, expect: impl Fn(&Path) -> Result<bool>, noted: impl FnOnce(&Path) -> bool) -> Result<PathBuf> {
    let mut to = None;
    for folder in folders_for(lib, r).take(FOLDERS_AT_MOST) {
        if expect(&folder)? {
            to = Some(folder);
            break;
        }
    }
    let to = to.context("Another record is expected in every folder it could go in")?;
    let hidden = hidden_folder(lib, r);
    std::fs::remove_dir_all(&hidden).ok();
    let made = fill(&hidden, files, cover).and_then(|()| {
        // The list's last look, the target noted in it (V4): an item gone from the list is placed nowhere.
        if !noted(&to) {
            return Err(anyhow::Error::new(LeftTheList));
        }
        let parent = to.parent().context("no folder")?;
        std::fs::create_dir_all(parent).with_context(|| format!("{} cannot be made", parent.display()))?;
        std::fs::rename(&hidden, &to).with_context(|| format!("{} cannot be made", to.display()))
    });
    if let Err(e) = made {
        std::fs::remove_dir_all(&hidden).ok();
        return Err(e);
    }
    if let Err(e) = expect(&to) {
        tracing::warn!(target: "soulseek", folder = %to.display(), reason = %format!("{e:#}"), "placed, but its record could not be told of its folder again");
    }
    Ok(to)
}

/// The files copied into `into`, a new folder (`place_with`): each disc's into its disc folder (X3), as the catalogue reads a
/// release of several, and the cover beside them.
fn fill(into: &Path, files: &[PathBuf], cover: Option<&[u8]>) -> Result<()> {
    std::fs::create_dir_all(into).with_context(|| format!("{} cannot be made", into.display()))?;
    for f in files {
        let name = f.file_name().context("no file name")?;
        let at = match f.parent().and_then(Path::file_name).map(|d| d.to_string_lossy().into_owned()).filter(|d| is_disc_folder(d)) {
            Some(disc) => {
                std::fs::create_dir_all(into.join(&disc)).with_context(|| format!("{} cannot be made", into.join(&disc).display()))?;
                into.join(disc)
            }
            None => into.to_path_buf(),
        };
        std::fs::copy(f, at.join(name)).with_context(|| format!("copying {}", f.display()))?;
    }
    if let Some(c) = cover {
        std::fs::write(into.join("cover.jpg"), c).context("its cover could not be written")?;
    }
    Ok(())
}

/// Folders set aside more than `SET_ASIDE_KEPT` ago deleted (X4), by when each was set aside (its `why.json`), and taken off their
/// items. Blocking.
fn sweep_set_asides(data: &Path, now: i64) {
    let mut gone = Vec::new();
    for e in std::fs::read_dir(data.join(SET_ASIDE)).into_iter().flatten().flatten() {
        let p = e.path();
        let at = std::fs::metadata(p.join("why.json"))
            .or_else(|_| e.metadata())
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(now, |d| d.as_secs() as i64);
        if p.is_dir() && now - at > SET_ASIDE_KEPT && std::fs::remove_dir_all(&p).is_ok() {
            gone.push(p.display().to_string());
        }
    }
    if !gone.is_empty() {
        tracing::info!(target: "soulseek", count = gone.len(), "folders set aside 14 days ago deleted");
        kept(|k| k.items.iter_mut().for_each(|i| i.set_aside.retain(|s| !gone.contains(&s.path))));
    }
}

/// A fetched folder set aside, with why each file failed beside it (`why.json`), in the data folder's
/// `soulseek-set-aside`, for 14 days (X4); where it went.
fn set_aside(dir: &Path, id: &str, user: &str, why: &BTreeMap<String, String>) -> Option<String> {
    let data = crate::serve::instance()?.data_dir.clone()?;
    let to = data.join(SET_ASIDE).join(format!("{id} {}", safe(user)));
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
            checked: None,
            local_failures: 0,
            placing: None,
            placed_at: 0,
            place_failures: 0,
        };
        assert!(on_failed(&mut i, "a", "a is offline".into()));
        assert_eq!((i.state, i.chosen, i.folders.len(), i.batch.clone(), i.failed.clone()), (State::Approved, Some(0), 1, None, vec!["a".to_string()]));
        assert_eq!(i.folders[0].user, "b", "the next folder, approved by the same tick");
        assert!(!on_failed(&mut i, "b", "queued too long".into()));
        assert_eq!((i.state, i.chosen, i.due), (State::Waiting, None, 0), "none left: searched again at once, without them");
        assert_eq!(i.failed, ["a", "b"]);
    }

    /// A record with two folders and two tracks, put on the list under `id`. Its record is none that Incoming's tests delete (their
    /// copy is record 1, and a Delete forgets its Placed items, U3).
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
            record: Record { record: 9_000, ..record("Air", "Moon Safari", Some(1998)) },
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
            checked: None,
            local_failures: 0,
            placing: None,
            placed_at: 0,
            place_failures: 0,
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
        assert_eq!((i.state, i.batch.as_deref(), i.checked.as_deref()), (State::Checking, None, Some(batch)), "passed: only the library a test has not set up stops it, its files kept (X4)");
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
        Some(super::super::requests::RecordOf { record: n, groups: Vec::new(), title: format!("Album {n}"), artist: "Artist".into(), year: Some(2000), cover: None, holder: None, holding: Default::default(), released: None, gone_from_incoming: false })
    }

    /// A record holding no copy, its last one deleted from Incoming (U7).
    fn gone(n: i64) -> Option<super::super::requests::RecordOf> {
        Some(super::super::requests::RecordOf { gone_from_incoming: true, ..of(n).unwrap() })
    }

    #[test]
    fn only_approved_requests_are_wanted_and_one_taken_off_stays_off() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, Some("rg1")), asked(2, None), asked(3, Some("rg3")), asked(4, None), asked(5, None)], &[of(101), of(102), of(103), None, of(101)], &[]);
        // Every record, with a release group or none (C51b); one with no record yet waits for one; one record taken once.
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record)).collect::<Vec<_>>(), vec![(1, 101), (2, 102), (3, 103)]);
        assert_eq!(k.items[1].record.album, "Album 102", "named as its record is");
        // Taken off (as 0.16 kept it, by request): not taken in again while it stays approved.
        k.dropped.push(3);
        k.items.retain(|i| i.record.request != 3);
        take_requests(&mut k, &[asked(1, Some("rg1")), asked(3, Some("rg3"))], &[of(101), of(103)], &[]);
        assert_eq!(k.items.len(), 1);
        // No longer approved (cancelled): it goes, unless it is placed (Y2).
        k.items[0].state = State::Found;
        take_requests(&mut k, &[], &[], &[]);
        assert!(k.items.is_empty());
        assert!(k.dropped.is_empty() && k.dropped_records.is_empty(), "a dropped one no longer approved is forgotten");
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
            checked: None,
            local_failures: 0,
            placing: None,
            placed_at: 0,
            place_failures: 0,
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

    /// R33: where even the titles find no folder (a track none holds; another count of tracks), the item says so, and is searched
    /// again later, as a miss by the list with lengths is: nothing found is no dead end.
    #[tokio::test]
    async fn with_no_list_matching_even_by_titles_the_item_says_so() {
        for (id, list) in [("ss-r33-none", vec![no_length(1, "One"), no_length(2, "Two"), no_length(3, "Three")]), ("ss-r33-count", vec![no_length(1, "One")])] {
            waiting_with(id, Vec::new(), list);
            let asked = std::sync::atomic::AtomicUsize::new(0);
            search_with(id, answering(vec![vec![titled_folder("near")]], &asked)).await;
            let i = item(id).unwrap();
            assert_eq!((i.state, i.why.as_deref(), i.misses, i.folders.len()), (State::Waiting, Some("No folder holds every track, even by titles"), 1, 0), "{id}");
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
        let to = place_with(&lib, &r, &[from.join("01 Pier.flac")], None, expect, |_| true).unwrap();
        assert_eq!(to, lib.join("Glass Harbour").join("Harbour Lights (2014) 2"));
        assert!(to.join("01 Pier.flac").exists() && !taken.exists());
        assert_eq!(*asked.lock().unwrap(), 3, "told again once placed");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: the files go into a hidden folder of the library and the folder into place whole, so no pass sees half an album; a
    /// placing that fails leaves nothing in the library, and the checked files where they were.
    #[test]
    fn a_placing_goes_through_a_hidden_folder_and_one_that_fails_keeps_its_files() {
        let d = data_folder("hidden");
        let (lib, from) = (d.join("soulseek"), d.join("fetched"));
        std::fs::create_dir_all(from.join("CD2")).unwrap();
        std::fs::write(from.join("01 Pier.flac"), b"flac").unwrap();
        std::fs::write(from.join("CD2").join("01 Quay.flac"), b"flac").unwrap();
        let r = record("Glass Harbour", "Harbour Lights", Some(2014));
        let to = lib.join("Glass Harbour").join("Harbour Lights (2014)");
        let files = [from.join("01 Pier.flac"), from.join("CD2").join("01 Quay.flac")];
        // A file gone on the way: nothing placed, nothing left half made, the checked files where they were.
        let err = place_with(&lib, &r, &[files[0].clone(), from.join("02 Gone.flac")], None, |_| Ok(true), |_| true).unwrap_err();
        assert!(format!("{err:#}").contains("02 Gone.flac"), "{err:#}");
        assert!(!to.exists(), "no half album");
        assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 0, "no hidden folder left behind");
        assert!(files.iter().all(|f| f.exists()), "the checked files kept");
        let placed = place_with(&lib, &r, &files, None, |_| Ok(true), |_| true).unwrap();
        assert_eq!(placed, to);
        assert!(to.join("01 Pier.flac").exists() && to.join("CD2").join("01 Quay.flac").exists(), "whole, each disc in its folder");
        assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 1, "the artist's folder alone");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: the record's cover goes beside its placed files as `cover.jpg`; none where it has none. It comes from the record's cover
    /// address, else its release group.
    #[test]
    fn the_placed_album_has_its_records_cover_beside_it() {
        let d = data_folder("cover");
        let (lib, from) = (d.join("soulseek"), d.join("fetched"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("01 Pier.flac"), b"flac").unwrap();
        let to = place_with(&lib, &record("Glass Harbour", "Harbour Lights", Some(2014)), &[from.join("01 Pier.flac")], Some(&b"jpeg"[..]), |_| Ok(true), |_| true).unwrap();
        assert_eq!(std::fs::read(to.join("cover.jpg")).unwrap(), b"jpeg");
        let bare = place_with(&lib, &record("Glass Harbour", "Low Tide", None), &[from.join("01 Pier.flac")], None, |_| Ok(true), |_| true).unwrap();
        assert!(bare.join("01 Pier.flac").exists() && !bare.join("cover.jpg").exists(), "none where it has none");
        use super::super::cover_cache::Of;
        let rg = "00000000-0000-4000-8000-000000000140";
        let address = format!("/api/covers/release-group/{rg}");
        assert_eq!(cover_source(Some(address.as_str()), &[]), Some(Of::ReleaseGroup(rg.into())));
        assert_eq!(cover_source(None, &[rg.to_string()]), Some(Of::ReleaseGroup(rg.into())), "its group's");
        assert_eq!(cover_source(Some("https://example.org/a.jpg"), &[]), None);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A picture `w` by `h` pixels, as `format` writes it.
    fn picture(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(w, h)).write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    /// Y3: the cover placed beside an album is the large picture, for a release group the Cover Art Archive's 1200-px one, kept as it
    /// came where it is a JPEG within 1200 px: never the 250-px thumbnail the pages' cover cache keeps.
    #[tokio::test]
    async fn the_placed_cover_is_the_large_one_not_the_thumbnail() {
        let rg = "00000000-0000-4000-8000-000000000141";
        let jpeg = picture(120, 120, image::ImageFormat::Jpeg);
        let asked = Mutex::new(Vec::new());
        let get = |url: String| {
            asked.lock().unwrap().push(url.clone());
            std::future::ready(Ok(url.ends_with("/front-1200").then(|| jpeg.clone())))
        };
        let cover = large_cover(&super::super::cover_cache::Of::ReleaseGroup(rg.into()), get).await;
        assert_eq!(cover.as_deref(), Some(jpeg.as_slice()), "as it came");
        assert_eq!(*asked.lock().unwrap(), [format!("https://coverartarchive.org/release-group/{rg}/front-1200")]);
    }

    /// Y3: where the Cover Art Archive has no 1200-px picture, its original is read, made JPEG and scaled to fit 1200 px. A picture
    /// none of its addresses gives is no cover, and the album is placed without one.
    #[tokio::test]
    async fn a_large_original_cover_is_made_jpeg_within_1200_px() {
        let rg = "00000000-0000-4000-8000-000000000142";
        let png = picture(1600, 900, image::ImageFormat::Png);
        let asked = Mutex::new(Vec::new());
        let get = |url: String| {
            asked.lock().unwrap().push(url.clone());
            std::future::ready(Ok(url.ends_with("/front").then(|| png.clone())))
        };
        let of = super::super::cover_cache::Of::ReleaseGroup(rg.into());
        let cover = large_cover(&of, get).await.expect("its original");
        let read = image::load_from_memory_with_format(&cover, image::ImageFormat::Jpeg).expect("made JPEG");
        assert_eq!((read.width(), read.height()), (1200, 675));
        assert_eq!(asked.lock().unwrap().len(), 2, "its 1200-px picture asked first");
        assert_eq!(large_cover(&of, |_| std::future::ready(Ok(Some(b"not a picture".to_vec())))).await, None);
    }

    /// A JPEG of a few pixels whose header says it is `w` by `h` (its frame's size written over): what a decoder would make room for.
    fn said_to_be(w: u16, h: u16) -> Vec<u8> {
        let mut b = picture(16, 16, image::ImageFormat::Jpeg);
        let frame = b.windows(2).position(|m| m[0] == 0xFF && matches!(m[1], 0xC0..=0xC2)).expect("its frame");
        b[frame + 5..frame + 7].copy_from_slice(&h.to_be_bytes());
        b[frame + 7..frame + 9].copy_from_slice(&w.to_be_bytes());
        b
    }

    /// V7: a cover's size is read from its header before it is decoded: one over 6000 px on its longer side is no cover, and the decode
    /// itself is held to that size, so it is refused from the header, its pixels never read. Up to 6000 px it is made JPEG within
    /// 1200 px as before. The placing's reader reads it on a blocking thread, and asks the next address.
    #[tokio::test]
    async fn an_oversized_original_is_not_decoded() {
        let huge = said_to_be(7000, 8);
        let r = image::ImageReader::new(std::io::Cursor::new(&huge)).with_guessed_format().unwrap();
        assert_eq!(r.into_dimensions().unwrap(), (7000, 8), "its header says so");
        assert_eq!(cover_jpg(huge.clone()), None, "no cover");
        let refused = crate::serve::art::jpeg_within_at_most(&huge, COVER_PX, COVER_READ_PX).unwrap_err();
        assert!(matches!(refused.downcast_ref::<image::ImageError>(), Some(image::ImageError::Limits(_))), "refused from its header: {refused:#}");
        assert_eq!(cover_jpg(picture(6001, 2, image::ImageFormat::Png)), None, "its longer side over 6000 px");
        let within = cover_jpg(picture(6000, 5, image::ImageFormat::Png)).expect("6000 px is read");
        assert_eq!(image::load_from_memory_with_format(&within, image::ImageFormat::Jpeg).unwrap().width(), COVER_PX);
        let asked = Mutex::new(0);
        let get = |_: String| {
            *asked.lock().unwrap() += 1;
            std::future::ready(Ok(Some(huge.clone())))
        };
        let addresses = ["front-1200", "front"].map(|size| format!("https://coverartarchive.org/release-group/x/{size}"));
        assert_eq!(first_picture(&addresses, &get).await, None);
        assert_eq!(*asked.lock().unwrap(), 2, "the next address asked");
    }

    /// X4: a folder set aside is kept 14 days, then deleted and taken off its item.
    #[test]
    fn a_set_aside_is_kept_fourteen_days_then_deleted() {
        let d = data_folder("aside");
        let (old, new) = (d.join(SET_ASIDE).join("ss-aside a"), d.join(SET_ASIDE).join("ss-aside b"));
        for p in [&old, &new] {
            std::fs::create_dir_all(p).unwrap();
            std::fs::write(p.join("why.json"), b"{}").unwrap();
        }
        let fifteen_days_ago = std::time::SystemTime::now() - Duration::from_secs(15 * 86_400);
        std::fs::File::options().write(true).open(old.join("why.json")).unwrap().set_modified(fifteen_days_ago).unwrap();
        listed("ss-aside", State::Waiting, None);
        with_item("ss-aside", |i| i.set_aside = [&old, &new].iter().map(|p| SetAside { user: "a".into(), path: p.display().to_string(), why: BTreeMap::new() }).collect());
        sweep_set_asides(&d, now());
        assert!(!old.exists() && new.exists(), "only the one past its 14 days");
        assert_eq!(item("ss-aside").unwrap().set_aside.iter().map(|s| s.path.clone()).collect::<Vec<_>>(), [new.display().to_string()]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4 (D17): a request whose record was retired follows the record its placed album joined or made; one on a live record stays.
    #[test]
    fn a_request_on_a_retired_record_follows_its_placed_album() {
        let cat = crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/incoming")]).unwrap();
        let folder = cat.folders().iter().next().map(|(id, _)| id).unwrap();
        let gone = cat.record_for_tests("Two Rooms", 2);
        cat.db_for_tests().execute("UPDATE records SET retired_at = 5 WHERE id = ?1", [gone]).unwrap();
        let new = cat.record_for_tests("Two Rooms", 2);
        cat.copy_for_tests(new, folder);
        let live = cat.record_for_tests("Open Water", 2);
        let ud = crate::userdata::UserData::open(None).unwrap();
        let on_gone = ud.add_request(&crate::userdata::Request { record: Some(gone), ..asked(1, None) }).unwrap();
        let on_live = ud.add_request(&crate::userdata::Request { record: Some(live), ..asked(2, None) }).unwrap();
        let at = format!("/incoming/copy-{new}");
        assert_eq!(cat.record_placed_in(Path::new(&at), 0).unwrap(), Some(new));
        let (moved, read) = repoint(&ud, &cat, &[("ss-gone".into(), on_gone, gone, at.clone(), 0), ("ss-live".into(), on_live, live, at, 0)]).unwrap();
        assert_eq!(moved, [(on_gone, gone, new)]);
        assert_eq!(read, ["ss-gone", "ss-live"], "both folders read: let go of");
        assert_eq!(ud.request(on_gone).unwrap().unwrap().record, Some(new));
        assert_eq!(ud.request(on_live).unwrap().unwrap().record, Some(live), "a live record's request stays");
    }

    /// Y10: re-pointing goes by the files that came to the folder since the placing alone, and for an approved request only: a placed
    /// path another album came to before it (its files older) re-points nothing and is not taken as read; a request no longer approved
    /// stays where it is, its folder let go of all the same.
    #[test]
    fn a_placed_path_reused_by_another_album_repoints_nothing() {
        let cat = crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/incoming")]).unwrap();
        let folder = cat.folders().iter().next().map(|(id, _)| id).unwrap();
        let (gone, other_gone) = (cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Open Water", 2));
        cat.db_for_tests().execute("UPDATE records SET retired_at = 5 WHERE id IN (?1, ?2)", [gone, other_gone]).unwrap();
        let other = cat.record_for_tests("Two Rooms", 2);
        cat.copy_for_tests(other, folder);
        cat.db_for_tests().execute_batch("UPDATE files SET came = 100").unwrap();
        let ud = crate::userdata::UserData::open(None).unwrap();
        let on_gone = ud.add_request(&crate::userdata::Request { record: Some(gone), ..asked(1, None) }).unwrap();
        let at = format!("/incoming/copy-{other}");
        let (moved, read) = repoint(&ud, &cat, &[("ss-reused".into(), on_gone, gone, at.clone(), 200)]).unwrap();
        assert!(moved.is_empty() && read.is_empty(), "its files came before the placing: another album's");
        assert_eq!(ud.request(on_gone).unwrap().unwrap().record, Some(gone));
        let declined = ud.add_request(&crate::userdata::Request { record: Some(other_gone), status: "declined".into(), ..asked(2, None) }).unwrap();
        let (moved, read) = repoint(&ud, &cat, &[("ss-declined".into(), declined, other_gone, at, 50)]).unwrap();
        assert_eq!((moved.len(), read), (0, vec!["ss-declined".to_string()]), "read and let go of, not followed");
        assert_eq!(ud.request(declined).unwrap().unwrap().record, Some(other_gone));
    }

    /// Y10: a placing a stop cut after its rename, before the Placed write, is taken as placed when its item is taken up again: never
    /// placed twice, its batch folder removed only then. One cut before its rename is checked and placed again. The target is noted
    /// before the rename.
    #[tokio::test]
    async fn a_placing_cut_after_its_rename_is_taken_as_placed() {
        let (id, batch) = ("ss-y10-cut", "b-y10-cut");
        listed(id, State::Checking, Some(batch));
        let d = fetched_to("y10-cut", batch);
        let to = d.join("soulseek").join("Air").join("Moon Safari (1998)");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("01 One.flac"), b"x").unwrap();
        with_item(id, |i| (i.placing, i.placed_at) = (Some(to.display().to_string()), now()));
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.placed.as_deref(), i.placing.as_deref(), i.batch.as_deref()), (State::Placed, Some(to.to_str().unwrap()), None, None));
        assert!(!d.join(crate::slskd::FOLDER).join(batch).exists(), "its batch folder removed, after");
        assert!(!to.with_file_name("Moon Safari (1998) 2").exists(), "never placed twice");
        // Cut before its rename: nothing there, so it is checked and placed again (no library in a test: its files kept).
        let (id, batch) = ("ss-y10-early", "b-y10-early");
        listed(id, State::Checking, Some(batch));
        fetched_to("y10-cut", batch);
        let nowhere = d.join("soulseek").join("Air").join("Premiers Symptomes");
        with_item(id, |i| i.placing = Some(nowhere.display().to_string()));
        fetch_from(id, &slskd_gone().await, &d, |_, _| std::future::ready(Ok(0.97))).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.placing.as_deref(), i.checked.as_deref()), (State::Checking, None, Some(batch)));
        // The target is told before the rename.
        let (lib, from) = (d.join("soulseek-lib"), d.join("fetched"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("01 Pier.flac"), b"flac").unwrap();
        let told = std::cell::Cell::new(false);
        let r = record("Glass Harbour", "Harbour Lights", Some(2014));
        let noted = |to: &Path| {
            told.set(!to.exists() && hidden_folder(&lib, &r).is_dir());
            true
        };
        let placed = place_with(&lib, &r, &[from.join("01 Pier.flac")], None, |_| Ok(true), noted).unwrap();
        assert!(told.get() && renamed(&placed, &r), "noted before its rename, its hidden folder there then");
        std::fs::remove_dir_all(&d).ok();
    }

    /// V4: an item whose placing is noted stays on the list whatever became of its request or its record: its placing, past the list's
    /// last look, finishes, and the item is judged once placed, as placed items are. One whose placing is not noted goes as ever, its
    /// fetch handed back to be cancelled.
    #[test]
    fn an_item_taken_off_after_its_placing_is_noted_is_judged_once_placed() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None)], &[of(101), of(102)], &[]);
        for (i, batch) in k.items.iter_mut().zip(["b-v4-1", "b-v4-2"]) {
            (i.state, i.batch, i.placing) = (State::Checking, Some(batch.to_string()), Some(format!("/soulseek/Artist/{batch}")));
        }
        // The first's request left, and the second's record arrived by Lidarr meanwhile.
        let stopped = take_requests(&mut k, &[asked(2, None)], &[had(102)], &[]);
        assert!(stopped.is_empty(), "nothing cancelled under a placing");
        assert_eq!(k.items.iter().map(|i| (i.record.record, i.state)).collect::<Vec<_>>(), [(101, State::Checking), (102, State::Checking)], "both stay");
        // Placed: judged as placed items are, kept as history.
        for i in &mut k.items {
            (i.state, i.batch, i.placing) = (State::Placed, None, None);
        }
        assert!(take_requests(&mut k, &[asked(2, None)], &[had(102)], &[]).is_empty());
        assert_eq!(k.items.iter().map(|i| (i.record.record, i.state)).collect::<Vec<_>>(), [(101, State::Placed), (102, State::Placed)]);
        // Not noted: it goes, its fetch to cancel.
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        (k.items[0].state, k.items[0].batch) = (State::Checking, Some("b-v4-3".into()));
        assert_eq!(take_requests(&mut k, &[], &[], &[]).len(), 1, "its fetch to cancel");
        assert!(k.items.is_empty());
    }

    /// V4: the target is noted in the list's last look before the rename: an item taken off the list by then is placed nowhere, the
    /// library left as it was and its checked files where they were.
    #[test]
    fn an_item_off_the_list_before_its_rename_is_placed_nowhere() {
        listed("ss-v4-noted", State::Checking, Some("b-v4-noted"));
        let to = PathBuf::from("/soulseek/Air/Moon Safari (1998)");
        assert!(kept(|k| note_placing(k, "ss-v4-noted", &to, 7)));
        let i = item("ss-v4-noted").unwrap();
        assert_eq!((i.placing, i.placed_at), (Some(to.display().to_string()), 7), "noted in the same look");
        assert!(!kept(|k| note_placing(k, "ss-v4-gone", &to, 7)), "off the list: not to be placed");
        let d = data_folder("v4-noted");
        let (lib, from) = (d.join("soulseek"), d.join("fetched"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("01 Pier.flac"), b"flac").unwrap();
        let r = record("Glass Harbour", "Harbour Lights", Some(2014));
        let err = place_with(&lib, &r, &[from.join("01 Pier.flac")], None, |_| Ok(true), |_| false).unwrap_err();
        assert!(err.is::<LeftTheList>(), "{err:#}");
        assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 0, "placed nowhere, no hidden folder left");
        assert!(from.join("01 Pier.flac").exists(), "its checked files where they were");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y12: at the start, the hidden folders placings left in the library are removed, but the one of an item whose placing is noted;
    /// nothing else of the library is touched.
    #[test]
    fn a_start_sweeps_the_hidden_folders_placings_left() {
        let lib = data_folder("y12-sweep");
        listed("ss-y12-placing", State::Checking, Some("b-y12-placing"));
        with_item("ss-y12-placing", |i| (i.record.record, i.record.request, i.placing) = (5, 9, Some(lib.join("Air").join("Moon Safari (1998)").display().to_string())));
        listed("ss-y12-other", State::Checking, Some("b-y12-other"));
        with_item("ss-y12-other", |i| (i.record.record, i.record.request) = (6, 10));
        let items = [item("ss-y12-placing").unwrap(), item("ss-y12-other").unwrap()];
        let (stray, other, placing) = (lib.join(".cratarr-1-1.part"), lib.join(".cratarr-6-10.part"), lib.join(".cratarr-5-9.part"));
        for p in [&stray, &other, &placing, &lib.join(".cratarr-trash").join("t"), &lib.join("Air").join("Moon Safari (1998)")] {
            std::fs::create_dir_all(p).unwrap();
            std::fs::write(p.join("01 One.flac"), b"x").unwrap();
        }
        let mut gone = sweep_hidden(&lib, &items);
        gone.sort();
        let mut want = vec![stray.clone(), other.clone()];
        want.sort();
        assert_eq!(gone, want, "a stray, and one whose item is placing nothing");
        assert!(!stray.exists() && !other.exists());
        assert!(placing.join("01 One.flac").exists(), "its placing noted: kept");
        assert!(lib.join(".cratarr-trash").join("t").exists() && lib.join("Air").join("Moon Safari (1998)").exists(), "nothing else");
        std::fs::remove_dir_all(&lib).ok();
    }

    /// Y12: a placing that fails three times in a row holds the record a day, not fifteen minutes, its checked files kept, and Health
    /// says Soulseek can't place albums, with why.
    #[tokio::test]
    async fn three_failed_placings_hold_the_record_a_day() {
        let (id, batch) = ("ss-y12-unplaced", "b-y12-unplaced");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("y12-unplaced", batch);
        // No library is set up in a test: each placing fails.
        fetch_from(id, &slskd_at(all_in()).await, &d, |_, _| std::future::ready(Ok(0.97))).await;
        for n in 1..=3 {
            if n > 1 {
                with_item(id, |i| i.due = 0);
                fetch_from(id, &slskd_gone().await, &d, never_gated).await;
            }
            let i = item(id).unwrap();
            assert_eq!((i.state, i.checked.as_deref(), i.place_failures), (State::Checking, Some(batch), n), "its files kept");
            let wait = i.due - now();
            if n < 3 {
                assert!(wait <= PLACE_AGAIN, "{n}: in a while");
            } else {
                assert!(wait > PLACE_AGAIN && wait <= PLACE_HOLD, "held a day");
            }
        }
        let i = item(id).unwrap();
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("It could not be placed")), "{:?}", i.why);
        let held = cant_place_in(std::slice::from_ref(&i), now());
        let f = super::super::health::soulseek_cant_place(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't place albums", i.why.as_deref(), 1));
        assert!(cant_place_in(std::slice::from_ref(&i), i.due).is_empty(), "its day over: no more");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y12: a placing that comes through ends a row of failed ones (here one a stop cut after its rename, taken as placed).
    #[tokio::test]
    async fn a_placing_that_comes_through_ends_the_row_of_failed_placings() {
        let (id, batch) = ("ss-y12-reset", "b-y12-reset");
        listed(id, State::Checking, Some(batch));
        let d = fetched_to("y12-reset", batch);
        let to = d.join("soulseek").join("Air").join("Moon Safari (1998)");
        std::fs::create_dir_all(&to).unwrap();
        with_item(id, |i| (i.placing, i.place_failures) = (Some(to.display().to_string()), 2));
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.place_failures), (State::Placed, 0));
        assert!(cant_place_in(std::slice::from_ref(&i), now()).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: too little free space for a folder's files holds the record before anything is fetched, and blames no user.
    #[tokio::test]
    async fn too_little_free_space_holds_the_record_and_blames_no_user() {
        let id = "ss-room";
        listed(id, State::Approved, None);
        with_item(id, |i| i.folders.iter_mut().flat_map(|f| f.files.iter_mut()).for_each(|o| o.size = i64::MAX / 4));
        let d = data_folder("room");
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("Too little free space")), "{:?}", i.why);
        assert_eq!((i.state, i.chosen, i.batch.clone(), i.failed.len()), (State::Approved, Some(0), None, 0));
        assert!(i.due > now(), "held");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: slskd unable to store a file (its disk full, its folder gone) is Cratarr's side: no user failed, what came of the batch
    /// removed, and the same folder fetched again later.
    #[tokio::test]
    async fn slskd_unable_to_store_a_file_blames_no_user() {
        let (id, batch) = ("ss-full", "b-full");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("full", batch);
        let full = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Errored", "percentComplete": 40.0, "exception": "System.IO.IOException: No space left on device" }] });
        let app = axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(full.clone()))));
        fetch_from(id, &slskd_at(app).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert!(i.failed.is_empty(), "no user failed");
        assert_eq!((i.state, i.chosen, i.batch.clone(), i.folders.len()), (State::Approved, Some(0), None, 2), "the same folder, later");
        assert!(i.why.as_deref().is_some_and(|w| w.contains("No space left on device")), "{:?}", i.why);
        assert!(i.due > now());
        assert!(!d.join(crate::slskd::FOLDER).join(batch).exists(), "what came of it removed");
        assert!(local_failure(&ended("Completed, Errored", "Read-only file system : '/downloads/x'")) && !local_failure(&ended("Completed, Rejected", "Transfer rejected: Queue full")));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A transfer slskd says ended so (`state`), with its exception.
    fn ended(state: &str, said: &str) -> crate::slskd::Transfer {
        crate::slskd::Transfer { id: "t1".into(), filename: r"d\01 One.flac".into(), state: state.into(), percent_complete: 0.0, exception: Some(said.into()) }
    }

    /// slskd saying, of every batch, that it could not store its file: its disk full.
    fn storing_nothing() -> axum::Router {
        let full = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Errored", "percentComplete": 40.0, "exception": "No space left on device : '/downloads/cratarr/01 One.flac'" }] });
        axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(full.clone()))))
    }

    /// Y9: only slskd's own words that it could not store a file are Cratarr's side. A refusal is its user's, whatever words it
    /// carries (the uploader's own disk's), and the next folder is tried.
    #[tokio::test]
    async fn a_remote_refusal_with_disk_words_fails_the_user() {
        assert!(local_failure(&ended("Completed, Errored", "System.IO.IOException: No space left on device")));
        assert!(local_failure(&ended("Completed, Errored", "Access to the path '/downloads/cratarr/b1/01 One.flac' is denied.")));
        assert!(local_failure(&ended("Completed, Errored", "System.UnauthorizedAccessException: Access to the path '/downloads/cratarr' is denied.")));
        assert!(!local_failure(&ended("Completed, Rejected", r"Transfer rejected: Access to the path 'D:\Music' is denied.")));
        assert!(!local_failure(&ended("Completed, Errored", r"Transfer rejected: Access to the path 'D:\Music' is denied.")), "not slskd's own words");
        assert!(!local_failure(&ended("Completed, Errored", "Remote connection closed: no space left on device")));
        assert!(!local_failure(&ended("Completed, TimedOut", "No space left on device")));
        let (id, batch) = ("ss-y9-remote", "b-y9-remote");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("y9-remote", batch);
        let refused = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Rejected", "percentComplete": 0.0, "exception": r"Access to the path 'D:\Music\01 One.flac' is denied." }] });
        let app = axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(refused.clone()))));
        fetch_from(id, &slskd_at(app).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.failed.clone(), i.folders[0].user.as_str(), i.local_failures), (State::Approved, vec!["a".to_string()], "b", 0), "the next folder");
        std::fs::remove_dir_all(&d).ok();
    }

    /// V5: a name too long for slskd's disk (a peer's folder of long names) is that folder's trouble, not the disk's: its user is failed
    /// and the next folder tried, nothing held, and Health says nothing of storing. What is the disk's is Cratarr's side: no space, a
    /// disk read-only, access denied, and slskd's own folder gone (U1).
    #[tokio::test]
    async fn a_name_too_long_fails_the_user_not_the_disk() {
        let too_long = "System.IO.PathTooLongException: The path '/downloads/cratarr/b1/01 One.flac' is too long, or a component of the specified path is too long.";
        assert!(!local_failure(&ended("Completed, Errored", too_long)));
        assert!(!local_failure(&ended("Completed, Errored", "The path '/downloads/cratarr/b1/01 One.flac' is too long, or a component of the specified path is too long.")));
        assert!(local_failure(&ended("Completed, Errored", "System.IO.IOException: Read-only file system : '/downloads/cratarr/b1'")));
        assert!(local_failure(&ended("Completed, Errored", "System.UnauthorizedAccessException: Access to the path '/downloads/cratarr/b1' is denied.")));
        let (id, batch) = ("ss-v5-long", "b-v5-long");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("v5-long", batch);
        let long = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Errored", "percentComplete": 0.0, "exception": too_long }] });
        let app = axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(long.clone()))));
        fetch_from(id, &slskd_at(app).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.failed.clone(), i.folders[0].user.as_str(), i.local_failures), (State::Approved, vec!["a".to_string()], "b", 0), "the next folder");
        assert!(i.due <= now() && cant_store_in(std::slice::from_ref(&i), now()).is_empty(), "nothing held: Soulseek can store files");
        std::fs::remove_dir_all(&d).ok();
    }

    /// U1 (the review of 8c/8d, finding 1): slskd's own folder gone ("Could not find a part of the path '…'", or a file of its
    /// incomplete folder taken before it was moved) is Cratarr's side, as a disk full is: no user is failed, and three in a row hold
    /// the record a day with Health's line. A peer's names cannot make part of slskd's own path missing.
    #[tokio::test]
    async fn a_missing_slskd_folder_blames_no_user() {
        let gone = "System.IO.DirectoryNotFoundException: Could not find a part of the path '/downloads/cratarr/b1'.";
        assert!(local_failure(&ended("Completed, Errored", gone)));
        assert!(local_failure(&ended("Completed, Errored", "Could not find a part of the path '/downloads/cratarr/b1'.")));
        assert!(local_failure(&ended("Completed, Errored", "System.IO.FileNotFoundException: Could not find file '/app/incomplete/01 One.flac'.")));
        assert!(local_failure(&ended("Completed, Errored", "Could not find file '/app/incomplete/01 One.flac'.")));
        assert!(!local_failure(&ended("Completed, Rejected", "File not shared.")), "a peer's own not found is its user's");
        assert!(!local_failure(&ended("Completed, Rejected", "Could not find file 'D:\\Music\\01 One.flac'.")), "a refusal is its user's, whatever it says");
        let (id, batch) = ("ss-u1-gone", "b-u1-gone");
        listed(id, State::Fetching, Some(batch));
        let d = data_folder("u1-gone");
        let answer = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Errored", "percentComplete": 100.0, "exception": gone }] });
        let slskd = slskd_at(axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(answer.clone()))))).await;
        for n in 1..=3 {
            with_item(id, |i| (i.state, i.batch, i.due) = (State::Fetching, Some(batch.into()), 0));
            fetch_from(id, &slskd, &d, never_gated).await;
            let i = item(id).unwrap();
            assert_eq!((i.local_failures, i.failed.len(), i.chosen, i.state), (n, 0, Some(0), State::Approved), "no user failed");
            let wait = i.due - now();
            if n < 3 {
                assert!(wait > 0 && wait <= SLSKD_AGAIN, "{n}: in a while");
            } else {
                assert!(wait > SLSKD_AGAIN && wait <= LOCAL_HOLD, "held a day");
            }
        }
        let i = item(id).unwrap();
        assert!(i.why.as_deref().is_some_and(|w| w.contains("Could not find a part of the path")), "{:?}", i.why);
        let held = cant_store_in(std::slice::from_ref(&i), now());
        let f = super::super::health::soulseek_cant_store(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't store files", i.why.as_deref(), 1));
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y9: three fetches in a row that slskd could not store hold the record a day, not five minutes, and Health says Soulseek can't
    /// store files, with why; no user is failed.
    #[tokio::test]
    async fn three_local_failures_hold_the_record_a_day() {
        let (id, batch) = ("ss-y9-full", "b-y9-full");
        listed(id, State::Fetching, Some(batch));
        let d = data_folder("y9-full");
        let slskd = slskd_at(storing_nothing()).await;
        for n in 1..=3 {
            with_item(id, |i| (i.state, i.batch, i.due) = (State::Fetching, Some(batch.into()), 0));
            fetch_from(id, &slskd, &d, never_gated).await;
            let i = item(id).unwrap();
            assert_eq!((i.local_failures, i.failed.len(), i.chosen), (n, 0, Some(0)), "no user failed");
            let wait = i.due - now();
            if n < 3 {
                assert!(wait <= SLSKD_AGAIN, "{n}: in a while");
            } else {
                assert!(wait > SLSKD_AGAIN && wait <= LOCAL_HOLD, "held a day");
            }
        }
        let i = item(id).unwrap();
        assert!(i.why.as_deref().is_some_and(|w| w.contains("No space left on device")), "{:?}", i.why);
        let held = cant_store_in(std::slice::from_ref(&i), now());
        let f = super::super::health::soulseek_cant_store(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't store files", i.why.as_deref(), 1));
        assert!(cant_store_in(std::slice::from_ref(&i), i.due).is_empty(), "its day over: no more");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y9: a fetch that comes through ends a row of failures on Cratarr's side: the next one counts from one, and holds a while only.
    #[tokio::test]
    async fn a_fetch_that_comes_through_ends_the_row_of_local_failures() {
        let (id, batch) = ("ss-y9-reset", "b-y9-reset");
        listed(id, State::Fetching, Some(batch));
        with_item(id, |i| i.local_failures = 2);
        let d = fetched_to("y9-reset", batch);
        fetch_from(id, &slskd_at(all_in()).await, &d, |_, _| std::future::ready(Ok(0.97))).await;
        assert_eq!(item(id).unwrap().local_failures, 0, "its files came");
        with_item(id, |i| (i.state, i.batch, i.checked, i.due) = (State::Fetching, Some(batch.into()), None, 0));
        fetch_from(id, &slskd_at(storing_nothing()).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!(i.local_failures, 1);
        assert!(i.due <= now() + SLSKD_AGAIN, "a while, not a day");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: a user who fails partway leaves nothing behind in slskd's folder.
    #[tokio::test]
    async fn a_failed_user_leaves_no_batch_folder_behind() {
        let (id, batch) = ("ss-partway", "b-partway");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("partway", batch);
        let refused = serde_json::json!({ "transfers": [{ "id": "t1", "filename": r"d\01 One.flac", "state": "Completed, Rejected", "percentComplete": 0.0, "exception": "Transfer rejected: Queue full" }] });
        let app = axum::Router::new().route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(move || std::future::ready(axum::Json(refused.clone()))));
        fetch_from(id, &slskd_at(app).await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.failed.clone()), (State::Approved, vec!["a".to_string()]), "the next folder");
        assert!(!d.join(crate::slskd::FOLDER).join(batch).exists(), "what the user sent is gone with them");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: none of a folder's files in slskd's folder at all (the folder set wrong) is no user's failure: the record is held.
    #[tokio::test]
    async fn files_missing_from_slskds_folder_altogether_blame_no_user() {
        let d = data_folder("nowhere");
        let f = Folder { user: "a".into(), dir: "d".into(), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 1, extra: 0, files: vec![file(r"d\01 One.flac", 100)], unchecked: None };
        let release = vec![super::super::track_fetch::track_for(1, 1, "One", "Air", None, "r")];
        let held = check(&d.join("no-such-batch"), &f, &release, &never_gated).await.unwrap_err();
        assert!(held.starts_with("Its files are not in slskd's folder"), "{held}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: a placing that fails keeps the checked files where they are and lets the batch go (no user failed, nothing fetched
    /// again); the record is placed again from them later.
    #[tokio::test]
    async fn a_failed_placing_keeps_its_checked_files_and_lets_its_batch_go() {
        let (id, batch) = ("ss-unplaced", "b-unplaced");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("unplaced", batch);
        // No library is set up in a test: its placing fails.
        fetch_from(id, &slskd_at(all_in()).await, &d, |_, _| std::future::ready(Ok(0.97))).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.as_deref(), i.checked.as_deref(), i.failed.len()), (State::Checking, None, Some(batch), 0));
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("It could not be placed")), "{:?}", i.why);
        assert!(d.join(crate::slskd::FOLDER).join(batch).join("01 One.flac").exists(), "its checked files kept");
        assert_eq!(next_of(std::slice::from_ref(&i), i.due), Some(id.to_string()), "taken up again when due");
        // Again, slskd gone meanwhile: placed again from its files, nothing fetched nor gated again.
        fetch_from(id, &slskd_gone().await, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.checked.as_deref(), i.failed.len()), (State::Checking, Some(batch), 0), "kept still: still no library");
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
        take_requests(&mut k, &[asked(7, Some("rg"))], &[of(42)], &[]);
        assert_eq!((at_start.record, record_now(&k, "ss-3").map(|r| r.record)), (0, Some(42)));
        assert_eq!(record_now(&k, "ss-9"), None, "one gone from the list: placed nowhere (Y2)");
    }

    /// A request's record as `records_of` reads it, had: a record holding its files.
    fn had(n: i64) -> Option<super::super::requests::RecordOf> {
        Some(super::super::requests::RecordOf { holder: Some(n + 1000), holding: super::super::requests::Holding::Done, ..of(n).unwrap() })
    }

    /// X3: a release split into disc folders under one parent is one candidate, each disc's files from its own folder.
    #[test]
    fn disc_folders_under_one_parent_are_one_candidate() {
        let list = [Track { disc: 1, ..track(1, "One", 100) }, Track { disc: 2, position: 1, ..track(1, "Two", 200) }];
        let found = folders_of(&[answer("u", true, vec![file(r"x\Air\Moon Safari\CD1\01 One.flac", 100), file(r"x\Air\Moon Safari\CD2\01 Two.flac", 200)])], &list, &[]);
        assert_eq!(found.iter().map(|f| (f.dir.as_str(), f.files.len())).collect::<Vec<_>>(), [(r"x\Air\Moon Safari", 2)]);
        assert_eq!(disc_of(r"x\Air\Moon Safari\CD2\01 Two.flac"), Some("CD2"));
        assert!(is_disc_folder("Disc 1") && is_disc_folder("cd2") && !is_disc_folder("CDs and more"));
    }

    /// X3: a version word in a file's title that the list's title lacks makes it another version; an "Artist - Title" list is
    /// compared by its title part, never its artist alone.
    #[test]
    fn a_bracketed_version_word_is_another_version() {
        assert!(!titled_as("01 Pier (Instrumental)", "Pier") && !titled_as("Pier - Live", "Pier") && !titled_as("Pier [Remix]", "Pier"));
        assert!(titled_as("Pier (Live)", "Pier (Live)") && titled_as("Pier (Remastered)", "Pier"));
        assert!(titled_as("Pier", "Glass Harbour - Pier") && !titled_as("Glass Harbour - Landing", "Glass Harbour - Pier"));
        assert!(!titled_as("Glass Harbour", "Glass Harbour - Pier"), "never by the artist alone");
    }

    /// X3: a folder holding more files than the list is a fallback after folders of the list's own count, saying what it takes.
    #[tokio::test]
    async fn a_fuller_folder_is_a_fallback_after_exact_ones() {
        waiting_with("ss-x3-fuller", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        let fuller = answer("big", true, vec![file(r"z\01 One.flac", 100), file(r"z\02 Two.flac", 200), file(r"z\03 Three.flac", 300)]);
        let asked = std::sync::atomic::AtomicUsize::new(0);
        search_with("ss-x3-fuller", answering(vec![vec![fuller.clone()]], &asked)).await;
        let i = item("ss-x3-fuller").unwrap();
        assert_eq!((i.folders[0].user.as_str(), i.folders[0].unchecked.as_deref()), ("big", Some("Takes 2 of 3 files")));
        assert_eq!(judged_by(&i, &i.folders[0]).len(), 2, "gated by its list with lengths");
        waiting_with("ss-x3-exact", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        search_with("ss-x3-exact", answering(vec![vec![fuller], vec![exact_folder("exact")]], &std::sync::atomic::AtomicUsize::new(0))).await;
        assert_eq!(item("ss-x3-exact").unwrap().folders.iter().map(|f| f.user.as_str()).collect::<Vec<_>>(), ["exact"], "the exact one, a rung later");
    }

    /// X2: an item whose request was taken back goes to another approved request of its record, keeping its tick and folders; with
    /// none, it goes with it, one being fetched too, handed back to be cancelled (Y2).
    #[test]
    fn a_ticked_item_goes_with_its_request_or_to_the_other() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None), asked(3, None)], &[of(101), of(101), of(103)], &[]);
        assert_eq!(k.items.len(), 2, "one item a record");
        for i in &mut k.items {
            (i.state, i.chosen) = (State::Approved, Some(0));
        }
        take_requests(&mut k, &[asked(2, None)], &[of(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record, i.state, i.chosen)).collect::<Vec<_>>(), [(2, 101, State::Approved, Some(0))], "to the other, ticked; 103's ticked one gone");
        (k.items[0].state, k.items[0].batch) = (State::Fetching, Some("b1".into()));
        let stopped = take_requests(&mut k, &[], &[], &[]);
        assert!(k.items.is_empty(), "under way: it goes too (Y2)");
        assert_eq!(stopped.len(), 1, "its fetch to cancel");
    }

    /// X2: a record taken off the list stays off, whichever request of it there is.
    #[test]
    fn out_of_the_list_is_kept_by_record() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        let i = k.items.remove(0);
        k.dropped_records.push(i.record.record);
        take_requests(&mut k, &[asked(1, None), asked(2, None)], &[of(101), of(101)], &[]);
        assert!(k.items.is_empty(), "another request of it does not bring it back");
        take_requests(&mut k, &[], &[], &[]);
        assert!(k.dropped_records.is_empty(), "named by none, forgotten");
    }

    /// X1: Soulseek's first search is five minutes after the request was handed to Lidarr, which goes first.
    #[test]
    fn soulseek_waits_five_minutes_for_lidarr() {
        let r = crate::userdata::Request { decided_at: Some(1_000), fetched: Some("Lidarr added it and is looking for it".into()), ..asked(1, None) };
        assert_eq!(first_search(&r, None, true), 1_000 + LIDARR_FIRST);
        let mut k = Kept::default();
        take_requests(&mut k, &[r.clone()], &[of(101)], &[first_search(&r, None, true)]);
        assert_eq!(k.items[0].due, 1_000 + LIDARR_FIRST);
    }

    /// X1: Soulseek starts at once where Lidarr does not know the album (its want failed), or is not connected.
    #[test]
    fn soulseek_starts_at_once_when_lidarr_does_not_know_it() {
        let refused = crate::userdata::Request { decided_at: Some(1_000), fetched: Some("Not handed over to Lidarr: Lidarr does not know Two Rooms by Glass Harbour".into()), ..asked(1, None) };
        assert_eq!(first_search(&refused, None, true), 1_000);
        let r = crate::userdata::Request { decided_at: Some(1_000), ..asked(2, None) };
        assert_eq!(first_search(&r, None, false), 1_000, "no Lidarr");
    }

    /// X1: a grab Lidarr reports failed or stalled hands the record to Soulseek at once; one it fetches still waits its turn, as
    /// does one whose release is to come.
    #[test]
    fn a_failed_grab_hands_over_to_soulseek() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None), asked(3, None)], &[of(101), of(102), of(103)], &[5_000, 5_000, 90_000]);
        let groups: HashMap<i64, String> = [(1, "g1".to_string()), (2, "g2".to_string()), (3, "g3".to_string())].into();
        let progress: HashMap<String, Progress> = [("g1".to_string(), Progress::Stuck), ("g2".to_string(), Progress::Fetching { done: 0.4 }), ("g3".to_string(), Progress::Stuck)].into();
        bring_forward(&mut k.items, &groups, &[3], &progress, 1_000);
        assert_eq!(k.items.iter().map(|i| i.due).collect::<Vec<_>>(), [1_000, 5_000, 90_000]);
    }

    /// X1: the first arrival stops the other: Soulseek's item of a record that arrived otherwise goes, a fetch under way with it
    /// (handed back to be cancelled in slskd); one placed is that arrival, and stays (Y2).
    #[test]
    fn the_first_arrival_cancels_soulseeks_fetch() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None)], &[of(101), of(102)], &[]);
        (k.items[0].state, k.items[0].batch) = (State::Fetching, Some("b1".into()));
        k.items[1].state = State::Placed;
        let stopped = take_requests(&mut k, &[asked(1, None), asked(2, None)], &[had(101), had(102)], &[]);
        assert_eq!(stopped.iter().map(|i| i.record.record).collect::<Vec<_>>(), [101]);
        assert_eq!(k.items.iter().map(|i| i.record.record).collect::<Vec<_>>(), [102]);
    }

    /// R26 (N9), X1: Soulseek takes no record already had (its own files, or a record holding them); an item of one goes, unless it
    /// is placed (that arrival itself, Y2); one being fetched or checked goes too, its fetch cancelled (X1).
    #[test]
    fn soulseek_takes_no_record_already_had() {
        let mut k = Kept::default();
        let both = [asked(1, None), asked(2, None)];
        take_requests(&mut k, &both, &[had(101), of(102)], &[]);
        assert_eq!(k.items.iter().map(|i| i.record.record).collect::<Vec<_>>(), [102]);
        k.items[0].state = State::Placed;
        take_requests(&mut k, &both, &[had(101), had(102)], &[]);
        assert_eq!(k.items.len(), 1, "placed: as it was");
        k.items[0].state = State::Found;
        take_requests(&mut k, &both, &[had(101), had(102)], &[]);
        assert!(k.items.is_empty(), "found, not fetched: gone, its album had");
    }

    /// R26 (N9): a request still approved after its album was placed and had is not searched again when the administrator clears
    /// the placed ones.
    #[test]
    fn clearing_placed_does_not_search_them_again() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        k.items[0].state = State::Placed;
        k.items.retain(|i| i.state != State::Placed);
        take_requests(&mut k, &[asked(1, None)], &[had(101)], &[]);
        assert!(k.items.is_empty());
    }

    /// A folder of the record's two tracks, as `listed` gives its items.
    fn two_tracks(user: &str) -> Folder {
        Folder {
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
        }
    }

    /// Y2: an item held at its check whose record arrived by the other fetcher goes, as one being fetched does: its transfer dropped
    /// in slskd and its batch folder removed. Only a placed one is that arrival: nothing is seen before its placing (X4).
    #[tokio::test]
    async fn a_checked_item_goes_when_the_other_fetchers_copy_arrives() {
        let batch = "b-y2-held";
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        let i = &mut k.items[0];
        (i.state, i.batch, i.folders, i.chosen) = (State::Checking, Some(batch.into()), vec![two_tracks("a")], Some(0));
        hold(i, "AcoustID could not be reached".into(), CHECK_AGAIN);
        let stopped = take_requests(&mut k, &[asked(1, None)], &[had(101)], &[]);
        assert!(k.items.is_empty(), "held at its check: gone, its record had");
        assert_eq!(stopped.iter().map(|i| i.batch.as_deref()).collect::<Vec<_>>(), [Some(batch)], "handed back to be cancelled");
        let d = fetched_to("y2-held", batch);
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = drops.clone();
        let app = all_in().route(
            "/api/v0/transfers/downloads/{user}/{id}",
            axum::routing::delete(move || {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::future::ready(axum::http::StatusCode::NO_CONTENT)
            }),
        );
        cancel_fetch(&slskd_at(app).await, &d, &stopped[0]).await;
        assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1, "its transfer dropped in slskd");
        assert!(!d.join(crate::slskd::FOLDER).join(batch).exists(), "its batch folder removed");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y2: an item whose placing failed, its checked files kept (X4), goes once its record arrived otherwise: off the list, so it is
    /// never taken up to be placed again, and its checked folder removed. A placing again already under way when it went places
    /// nothing, and its files go with it.
    #[tokio::test]
    async fn an_item_whose_placing_failed_is_never_placed_once_its_record_arrived() {
        let (id, batch) = ("ss-y2-unplaced", "b-y2-unplaced");
        listed(id, State::Fetching, Some(batch));
        let d = fetched_to("y2-unplaced", batch);
        let dir = d.join(crate::slskd::FOLDER).join(batch);
        // No library is set up in a test: its placing fails, its files kept.
        fetch_from(id, &slskd_at(all_in()).await, &d, |_, _| std::future::ready(Ok(0.97))).await;
        let held = item(id).unwrap();
        assert_eq!((held.state, held.batch.as_deref(), held.checked.as_deref()), (State::Checking, None, Some(batch)));
        let mut k = Kept { items: vec![held.clone()], ..Kept::default() };
        let stopped = take_requests(&mut k, &[asked(1, None)], &[had(held.record.record)], &[]);
        assert!(k.items.is_empty(), "off the list: never taken up to be placed again");
        assert_eq!(stopped.iter().map(|i| i.checked.as_deref()).collect::<Vec<_>>(), [Some(batch)]);
        cancel_fetch(&slskd_at(all_in()).await, &d, &stopped[0]).await;
        assert!(!dir.exists(), "its checked folder removed");
        // Taken off while a placing again was under way.
        kept(|k| k.items.retain(|i| i.id != id));
        fetched_to("y2-unplaced", batch);
        place_from(id, &held, &dir, &files_under(&dir)).await;
        assert!(item(id).is_none(), "nothing written back");
        assert!(!dir.exists(), "placed nowhere, its files gone with it");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y2: an item whose request left (cancelled, declined, rejected), no other approved request of its record there, goes unless it
    /// is placed: one held at its check, or keeping the checked files of a placing that failed, is handed back to be cancelled, so it
    /// is never placed for nobody.
    #[test]
    fn a_held_item_of_a_cancelled_request_is_not_placed() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None), asked(3, None)], &[of(101), of(102), of(103)], &[]);
        (k.items[0].state, k.items[0].batch) = (State::Checking, Some("b-y2-left".into()));
        hold(&mut k.items[0], "AcoustID could not be reached".into(), CHECK_AGAIN);
        (k.items[1].state, k.items[1].checked) = (State::Checking, Some("b-y2-checked".into()));
        k.items[2].state = State::Placed;
        let stopped = take_requests(&mut k, &[], &[], &[]);
        assert_eq!(stopped.iter().map(|i| i.record.record).collect::<Vec<_>>(), [101, 102], "handed back to be cancelled");
        assert_eq!(k.items.iter().map(|i| (i.record.record, i.state)).collect::<Vec<_>>(), [(103, State::Placed)], "a placed one stays");
    }

    /// Y2: a copy Soulseek placed that is short of its record's list, the record partly there, lets a new item of the record be made,
    /// so Soulseek looks again; the placed one stays as history. A whole copy placed waits while its record is partly there by
    /// another copy alone: no pass has read it yet, and once one has, its record is had.
    #[test]
    fn a_placed_copy_partly_there_lets_soulseek_look_again() {
        use super::super::requests::{Holding, RecordOf};
        let partly = |n: i64, has: u32, list: u32| Some(RecordOf { holder: Some(n), holding: Holding::Partly { has, of: list }, ..of(n).unwrap() });
        let placed = |k: &mut Kept| {
            let i = &mut k.items[0];
            (i.state, i.folders, i.chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
        };
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        placed(&mut k);
        // Its two tracks read, of the list's three.
        take_requests(&mut k, &[asked(1, None)], &[partly(101, 2, 3)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.record, i.state)).collect::<Vec<_>>(), [(101, State::Placed), (101, State::Waiting)]);
        take_requests(&mut k, &[asked(1, None)], &[partly(101, 2, 3)], &[]);
        assert_eq!(k.items.len(), 2, "one new item, not one a round");
        // Its two tracks the whole list, not read yet: one of them there by a copy before it.
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(2, None)], &[of(102)], &[]);
        placed(&mut k);
        take_requests(&mut k, &[asked(2, None)], &[partly(102, 1, 2)], &[]);
        assert_eq!(k.items.len(), 1, "it waits for a pass to read it");
    }

    /// V1: a copy Soulseek placed and Incoming's Delete took (its folder read, its record holding no copy) lets its Placed item make
    /// way: it goes, and the record's next request is searched again, reading Soulseek's search, never "fetching 100%".
    #[test]
    fn a_deleted_soulseek_copy_lets_the_next_request_be_searched() {
        use super::super::requests::{Holding, RecordOf, RequestState, request_state};
        let arrived = Some(RecordOf { holder: Some(101), holding: Holding::Arrived, ..of(101).unwrap() });
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        // Placed, and its folder read by a pass (Y10 let go of it): its record arrived, the item kept as history.
        (k.items[0].state, k.items[0].folders, k.items[0].chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
        take_requests(&mut k, &[asked(1, None)], &[arrived], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.state)).collect::<Vec<_>>(), [(1, State::Placed)]);
        // Deleted from Incoming: its request closed (W4), its record holds no copy, and someone asks for it again.
        take_requests(&mut k, &[asked(2, None)], &[gone(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record, i.state)).collect::<Vec<_>>(), [(2, 101, State::Waiting)], "searched again");
        let words = words_in(&k.items, &[2]);
        assert_eq!(words.get(&2), Some(&Word::Waiting { why: None }));
        let state = request_state(&asked(2, None), Holding::None, None, words.get(&2));
        assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "not fetching 100%: {state:?}");
    }

    /// V1: an item whose request left goes to another approved request of its record (X2) only while its copy may be there: a placed
    /// one no pass has read yet does (its album may be on its way into the catalogue); one whose folder was read, its record holding no
    /// copy, its last one deleted from Incoming (U7), never does, and that request is searched for.
    #[test]
    fn a_placed_item_is_never_handed_to_a_request_whose_record_it_no_longer_holds() {
        let placed = |read: bool| {
            let mut k = Kept::default();
            take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
            (k.items[0].state, k.items[0].placed) = (State::Placed, (!read).then(|| "/soulseek/Artist/Album 101 (2000)".to_string()));
            k
        };
        let mut k = placed(false);
        take_requests(&mut k, &[asked(2, None)], &[of(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.state)).collect::<Vec<_>>(), [(2, State::Placed)], "not read yet: handed over");
        let mut k = placed(true);
        take_requests(&mut k, &[asked(2, None)], &[gone(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.state)).collect::<Vec<_>>(), [(2, State::Waiting)], "never handed over: an item of its own");
    }

    /// U7 (the review of 8c/8d, finding 7): a copy Soulseek placed, kept into Music and deleted there months later: its record holds
    /// no copy and its request is still approved, but its Placed item stays and Soulseek does not look for it again. Only a last copy
    /// that went to an Incoming library's trash lets it.
    #[test]
    fn a_kept_album_deleted_from_music_is_not_searched_again() {
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        (k.items[0].state, k.items[0].folders, k.items[0].chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
        take_requests(&mut k, &[asked(1, None)], &[of(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.state)).collect::<Vec<_>>(), [(1, State::Placed)], "not looked for again");
        take_requests(&mut k, &[asked(1, None)], &[gone(101)], &[]);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.state)).collect::<Vec<_>>(), [(1, State::Waiting)], "deleted from Incoming: looked for again");
    }

    /// Seconds since the epoch, `days` ago: a time a test gives the catalogue.
    fn days_ago(days: i64) -> i64 {
        now() - days * 86_400
    }

    /// The places of a test copy's rows (`Catalogue::copy_for_tests`) of `record`, in order.
    fn rows_of(cat: &crate::catalogue::Catalogue, record: i64) -> Vec<String> {
        let db = cat.db_for_tests();
        let mut st = db.prepare("SELECT rel FROM files WHERE rel LIKE ?1 ORDER BY rel").unwrap();
        let rows: Vec<String> = st.query_map([format!("copy-{record}/%")], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        rows
    }

    /// T8: Soulseek's round as `read_requests` reads its list, on the catalogue alone (Music folder 0, Incoming 1): each request's
    /// record as held and whether its last copy went from Incoming, then the list taken.
    fn round(cat: &crate::catalogue::Catalogue, k: &mut Kept, rs: &[crate::userdata::Request]) -> Vec<(super::super::requests::Holding, bool)> {
        use crate::settings::LibraryKind;
        let kinds = [(0, LibraryKind::Main), (1, LibraryKind::Incoming)];
        let mut of = super::super::requests::records_held(cat, rs, &kinds).unwrap();
        super::super::requests::mark_gone_from_incoming(cat, &mut of, &kinds).unwrap();
        take_requests(k, rs, &of, &[]);
        of.iter().map(|o| o.as_ref().map(|o| (o.holding, o.gone_from_incoming)).unwrap()).collect()
    }

    /// T8.1, T8.2 (the review of run 10, finding 8): a copy Soulseek placed and a pass read, its files removed from Incoming on the NAS
    /// (gone, never in a trash), nothing of its record in a main library. Gone a while, they hold their record for their day (the had
    /// rule); once the catalogue retires them, the record says its last copy went from Incoming, its Placed item is let go and its
    /// approved request looked for again, never "fetching 100%". So too a copy fetched again after an older one went into Music's
    /// trash. U7's case stays: kept into Music and deleted there, into Music's trash or on the NAS; and so does one whose Incoming
    /// file went before its other file was kept into Music and deleted there, in Music's trash still.
    #[test]
    fn a_placed_copy_removed_from_incoming_by_hand_is_looked_for_again() {
        use super::super::requests::{Holding, RequestState, request_state};
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let records: Vec<i64> = ["By Hand", "Kept Then Trashed", "Kept Then Removed", "Kept After", "Fetched Again"].iter().map(|n| cat.record_for_tests(n, 2)).collect();
        let (by_hand, trashed, removed, after, again) = (records[0], records[1], records[2], records[3], records[4]);
        for r in &records {
            cat.copy_for_tests(*r, 1);
        }
        let rs: Vec<crate::userdata::Request> = records.iter().enumerate().map(|(n, r)| crate::userdata::Request { record: Some(*r), ..asked(n as i64 + 1, None) }).collect();
        // Each asked for, its item made, placed, and its folder read by a pass (Y10 let go of it).
        let mut k = Kept::default();
        take_requests(&mut k, &rs, &records.iter().map(|r| of(*r)).collect::<Vec<_>>(), &[]);
        assert_eq!(k.items.len(), 5);
        for i in k.items.iter_mut() {
            (i.state, i.folders, i.chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
        }
        let gone = |folder: i64, rel: &str, at: i64| cat.execute_for_tests(&format!("UPDATE files SET gone_at = {at} WHERE folder = {folder} AND rel = '{rel}'"));
        let dir = |r: i64| format!("copy-{r}");
        // T8.1: removed from Incoming on the NAS two days ago.
        for rel in rows_of(&cat, by_hand) {
            gone(1, &rel, days_ago(2));
        }
        // T8.2, U7's case: kept into Music (its rows moved by Cratarr), then deleted there two days ago: into Music's trash, or on the NAS.
        for r in [trashed, removed] {
            cat.move_files(1, &dir(r), 0, &dir(r)).unwrap();
        }
        cat.mark_trashed(0, &dir(trashed), &format!("{}/{}", days_ago(2), dir(trashed))).unwrap();
        for rel in rows_of(&cat, removed) {
            gone(0, &rel, days_ago(2));
        }
        // One file gone from Incoming three days ago (its other one kept into Music after the pass below).
        let after_rows = rows_of(&cat, after);
        gone(1, &after_rows[0], days_ago(3));
        // A file of it in Music's trash since three days ago; the copy fetched again into Incoming removed on the NAS two days ago.
        let rows = rows_of(&cat, again);
        cat.move_files(1, &rows[0], 0, &rows[0]).unwrap();
        cat.mark_trashed(0, &rows[0], &format!("{}/{}", days_ago(3), rows[0])).unwrap();
        gone(1, &rows[1], days_ago(2));
        // Gone a while, its files hold the record still; U7's, in Music's trash, holds none and did not go from Incoming.
        let read = round(&cat, &mut k, &rs);
        assert_eq!((read[0], read[1]), ((Holding::Arrived, false), (Holding::None, false)));
        assert!(k.items.iter().all(|i| i.state == State::Placed), "none let go while its files are gone a while");
        // The catalogue's pass retires what went (a trash emptied too: no library here keeps one).
        cat.identify().unwrap();
        // Then the other file of "Kept After" is kept into Music and deleted there: in Music's trash still, after its Incoming one went.
        cat.move_files(1, &after_rows[1], 0, &after_rows[1]).unwrap();
        cat.mark_trashed(0, &after_rows[1], &format!("{}/{}", days_ago(0), after_rows[1])).unwrap();
        let read = round(&cat, &mut k, &rs);
        assert_eq!(read, [(Holding::None, true), (Holding::None, false), (Holding::None, false), (Holding::None, false), (Holding::None, true)]);
        let states = |request: i64| k.items.iter().filter(|i| i.record.request == request).map(|i| i.state).collect::<Vec<_>>();
        assert_eq!((states(1), states(5)), (vec![State::Waiting], vec![State::Waiting]), "let go, and looked for again");
        for request in [2, 3, 4] {
            assert_eq!(states(request), [State::Placed], "kept into Music and deleted there: never looked for again ({request})");
        }
        let words = words_in(&k.items, &[1]);
        assert_eq!(words.get(&1), Some(&Word::Waiting { why: None }));
        let state = request_state(&rs[0], Holding::None, None, words.get(&1));
        assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "not fetching 100%: {state:?}");
    }

    /// T8.3: a copy Soulseek placed and a pass read, its folder read since as another record (the files there rows on that record's
    /// tracks), its own files gone: once the catalogue retires them, its item is let go and its request looked for again, as one
    /// removed by hand; the other record holds the folder.
    #[test]
    fn a_placed_copy_read_as_another_record_frees_its_item() {
        use super::super::requests::{Holding, RequestState, request_state};
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let (placed, other) = (cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Other Shore", 2));
        cat.copy_for_tests(placed, 1);
        let rs = [crate::userdata::Request { record: Some(placed), ..asked(1, None) }];
        let mut k = Kept::default();
        take_requests(&mut k, &rs, &[of(placed)], &[]);
        (k.items[0].state, k.items[0].folders, k.items[0].chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
        let dir = format!("copy-{placed}");
        cat.execute_for_tests(&format!(
            "UPDATE files SET gone_at = {gone} WHERE rel LIKE '{dir}/%';
             INSERT INTO files (folder, rel, size, mtime, track, came) SELECT 1, '{dir}/other-' || id || '.flac', 1, 1, id, {gone} FROM tracks WHERE record = {other};",
            gone = days_ago(2)
        ));
        cat.identify().unwrap();
        let read = round(&cat, &mut k, &rs);
        assert_eq!(read, [(Holding::None, true)], "its last copy went from Incoming");
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record, i.state)).collect::<Vec<_>>(), [(1, placed, State::Waiting)], "let go, and looked for again");
        let words = words_in(&k.items, &[1]);
        let state = request_state(&rs[0], Holding::None, None, words.get(&1));
        assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "not fetching 100%: {state:?}");
        let theirs = [crate::userdata::Request { record: Some(other), ..asked(2, None) }];
        assert_eq!(round(&cat, &mut Kept::default(), &theirs), [(Holding::Arrived, false)], "the other record holds the folder");
    }

    /// V1: Incoming's Delete of a copy forgets its record's Placed items, read or not: none stands for a copy any more. The record's
    /// items under way, and other records' placed ones, stay.
    #[test]
    fn incomings_delete_forgets_the_records_placed_items() {
        for (id, record, state) in [("ss-v1-placed", 9_101, State::Placed), ("ss-v1-fetching", 9_101, State::Fetching), ("ss-v1-other", 9_102, State::Placed)] {
            listed(id, state, None);
            with_item(id, |i| (i.record.record, i.placed) = (record, Some("/soulseek/Air/Moon Safari (1998)".to_string())));
        }
        forget_placed(9_101);
        assert!(item("ss-v1-placed").is_none(), "forgotten");
        assert!(item("ss-v1-fetching").is_some() && item("ss-v1-other").is_some(), "the rest stay");
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
        take_requests(&mut k, &[asked(7, Some("rg-moon"))], &[of(42)], &[]);
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

    /// A list reader that must not be asked.
    fn never_read(_: crate::catalogue::records::Ref) -> std::future::Ready<Result<Option<crate::app::references::Read>>> {
        panic!("no list is read while background reading is paused")
    }

    /// Y7: while background reading is paused Soulseek reads no list: the item waits a round, saying so, and is not Stuck. Unpaused,
    /// its list is read again.
    #[tokio::test]
    async fn soulseek_reads_no_list_while_background_reading_is_paused() {
        use super::super::switches::{BACKGROUND, PAUSED, TEST_STATE, set_for_tests};
        let _one = TEST_STATE.lock().await;
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/music")]).unwrap());
        let record = cat.record_for_tests("Two Rooms", 2);
        waiting_with("ss-y7-paused", Vec::new(), Vec::new());
        with_item("ss-y7-paused", |i| i.record.record = record);
        let it = item("ss-y7-paused").unwrap();
        let was = set_for_tests(BACKGROUND, false);
        let paused = listing(&it, &cat, never_read).await;
        set_for_tests(BACKGROUND, true);
        let unpaused = listing(&it, &cat, |_| std::future::ready(Ok(None))).await;
        set_for_tests(BACKGROUND, was);
        assert_eq!(paused, Listing::Later(PAUSED.into(), ROUND_AT_LEAST));
        assert_ne!(unpaused, paused, "unpaused, its list is read");
    }

    /// FX01's data after 0.17's step, and the Soulseek items its approved requests make, as `read_requests` takes them; the
    /// data folder, to remove once the catalogue is let go.
    fn fx01(name: &str) -> (Arc<crate::catalogue::Catalogue>, Vec<Item>, PathBuf) {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        crate::fixtures::data_0_16(&d).unwrap();
        let cat = Arc::new(crate::catalogue::Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![PathBuf::from("/music")]).unwrap());
        let ud = crate::userdata::UserData::open(Some(&d.join("userdata.sqlite"))).unwrap();
        super::super::step_017::run(&cat, &ud, &[], &crate::catalogue::Rules::default()).unwrap();
        let approved = ud.requests(None, Some(super::super::requests::APPROVED), 100).unwrap();
        let of = super::super::requests::records_of(&cat, &approved).unwrap();
        let mut k = Kept::default();
        take_requests(&mut k, &approved, &of, &[]);
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
            checked: None,
            local_failures: 0,
            placing: None,
            placed_at: 0,
            place_failures: 0,
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
