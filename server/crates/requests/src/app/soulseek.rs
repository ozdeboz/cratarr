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
//! - **Fetched with a tick, or by itself when certain:** the administrator approves many at once. A record whose best
//!   folder is certain starts by itself (`pick_certain`, decision 0099's amendment of 2026-11-19); nothing uncertain is
//!   fetched without a tick (March 2026's unattended batches cannot happen again). The folder's tracks come in one batch
//!   from its user.
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
use super::requests_host::RequestsHost;
use super::turn::Failure;
use crate::lidarr::Progress;
use crate::slskd::{Answer, Offered, Slskd};

/// The fetcher's name in a library's setting (`settings::LibrarySetting::fetcher`).
pub const FETCHER: &str = "soulseek";
/// Between two searches: gentle with Soulseek.
const PACE: Duration = Duration::from_secs(if cfg!(any(test, feature = "test-support")) { 0 } else { 5 });
/// A search is followed this long at most.
const SEARCH_AT_MOST: Duration = Duration::from_secs(120);
/// A fetch nobody's queue starts in this long is given up on that user.
const QUEUED_FOR: Duration = Duration::from_secs(30 * 60);
/// A whole fetch is given up after this long (a slow user).
const FETCH_AT_MOST: Duration = Duration::from_secs(4 * 3600);
/// Reads of slskd that may fail in a row before a fetch is given up.
const READS: u32 = 15;
/// Between two such reads.
const READ_AGAIN: Duration = Duration::from_secs(if cfg!(any(test, feature = "test-support")) { 0 } else { 4 });
/// A record held for slskd is tried again after this long.
pub const SLSKD_AGAIN: i64 = 300;
/// Fetches failed on Cratarr's own side in a row after which a record holds a day (Y9): its folder is not asked of its user every
/// five minutes all day.
const LOCAL_AT_MOST: u32 = 3;
/// How long such a record holds (Y9).
const LOCAL_HOLD: i64 = 86_400;
/// Folders a record shows, best first.
const SHOWN: usize = 5;
/// The shortest wait between two rounds of searches for one record (the ladder's "at once" still waits this).
pub const ROUND_AT_LEAST: i64 = 15 * 60;
/// A file's length may be this far from its track's.
const LENGTH_SLACK_MS: i64 = crate::slskd::LENGTH_SLACK_MS;
/// A record held at its check is checked again after this long: each time decodes a file to ask AcoustID about it.
pub const CHECK_AGAIN: i64 = 15 * 60;
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
    /// When its download ended (epoch ms, SK1.3): checks go in that order; 0, not known (a stop, 0.16's), first.
    #[serde(default)]
    pub ended_at: i64,
    /// Batches slskd forgot in a row (a restart, OQ-C3 (a)): the first fetched again with no one blamed, the second fails its user.
    #[serde(default)]
    pub forgotten: u32,
    /// Picked by Cratarr itself, its folder certain (SK3): no administrator's tick.
    #[serde(default)]
    pub auto: bool,
    /// When its folders were found (epoch s, SK3.4): a free slot counts within the hour; 0, not known (0.16's).
    #[serde(default)]
    pub found_at: i64,
    /// What its check heard in a file that passed, by the file's name (SK6.3, the owner's "and in the item"): clipping, clicks, a pop
    /// at the very end, in `Findings::notes()`'s words. Kept once its check passed, so a placing tried again keeps them; shown on the
    /// arrival's card.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, String>,
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
    /// Requests taken off the list, as 0.16 and 0.17's previews kept them: read once, each made its record (`take_requests`). Written
    /// while any is left, so a stop before the first round loses none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dropped: Vec<i64>,
    /// Batches let go whose transfers slskd could not be asked for then (a read that failed): dropped at the downloads' next look,
    /// their users' turns taken till then, so no second batch of a user goes while the first may still send.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    to_drop: Vec<ToDrop>,
    /// Whether the users 0.16.3 set aside for notes alone were asked again (SK5.6, `notes_pass`): once, at the first start.
    #[serde(default)]
    notes_passed: bool,
    /// A file that could not be read nor set aside: nothing is written over it (R24).
    #[serde(skip)]
    frozen: bool,
}

/// A batch let go to be dropped (`let_go`): the item it was of, its user, its id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToDrop {
    id: String,
    user: String,
    batch: String,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
/// Where it is kept: the data folder's `soulseek.json`, once known.
static FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // A test's own list (`tests::own_list`): kept in its folder's `soulseek.json` on the test's thread alone, read from there when
    // its memory is empty, so no other test's items mix with it and the store can be read as written.
    static OWN: std::cell::RefCell<Option<(PathBuf, Option<Kept>)>> = const { std::cell::RefCell::new(None) };
    // A test's own Soulseek library (`tests::own_list`): where its records are placed.
    static OWN_LIBRARY: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn kept<T>(f: impl FnOnce(&mut Kept) -> T) -> T {
    #[cfg(any(test, feature = "test-support"))]
    if OWN.with(|o| o.borrow().is_some()) {
        return OWN.with(|o| {
            let mut o = o.borrow_mut();
            let (p, k) = o.as_mut().expect("a test's own list");
            let k = k.get_or_insert_with(|| load(&*p));
            let out = f(k);
            keep_in(&*p, k);
            out
        });
    }
    let mut g = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let k = g.get_or_insert_with(|| FILE.get().map(|p| load(p)).unwrap_or_default());
    let out = f(k);
    if let Some(p) = FILE.get() {
        keep_in(p, k);
    }
    out
}

/// The list written whole to `p` (through a part file), unless nothing may be written to it (`frozen`, R24).
fn keep_in(p: &Path, k: &Kept) {
    if k.frozen {
        return;
    }
    let Ok(b) = serde_json::to_vec(k) else { return };
    let part = p.with_extension("json.part");
    if std::fs::write(&part, b).and_then(|_| std::fs::rename(&part, p)).is_err() {
        tracing::warn!(target: "soulseek", file = %p.display(), "the Soulseek queue could not be kept");
    }
}

/// The queue as the data folder keeps it (`soulseek.json`); none yet, an empty one (R24, N4). One 0.16 wrote, its items naming
/// their release group, is copied once to `soulseek.json.016` before 0.17 first writes over it, so a rollback finds it. One that
/// cannot be read is set aside whole (`soulseek.json.unreadable-<time>`) and said in the log as an error, and on Health while the
/// file lies there (P7), never emptied; where it cannot be set aside either, nothing is written to it (`frozen`, on Health too).
/// Records 0.16.3 held for slskd's 429 go ahead as it is read (`cleared_429s`).
fn load(p: &Path) -> Kept {
    let bytes = match std::fs::read(p) {
        Ok(bytes) => bytes,
        // A new queue: no set-aside of 0.16.3's to ask again, ever.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Kept { notes_passed: true, ..Kept::default() },
        Err(e) => return set_aside_unreadable(p, &format!("{e}")),
    };
    match serde_json::from_slice::<Kept>(&bytes) {
        Ok(mut k) => {
            let of_016 = serde_json::from_slice::<serde_json::Value>(&bytes).ok().is_some_and(|v| v["items"].as_array().is_some_and(|items| items.iter().any(|i| i["record"].get("release_group").is_some())));
            let old = p.with_extension("json.016");
            if of_016
                && !old.exists()
                && let Err(e) = std::fs::copy(p, &old)
            {
                tracing::error!(target: "soulseek", file = %old.display(), reason = %e, "0.16's Soulseek queue could not be kept aside");
            }
            let n = cleared_429s(&mut k.items);
            if n > 0 {
                tracing::info!(target: "soulseek", count = n, "records 0.16.3 held for slskd's 429 go ahead");
            }
            k
        }
        Err(e) => set_aside_unreadable(p, &format!("{e}")),
    }
}

/// Records 0.16.3 held for slskd's 429 (it asked for several at once, and slskd takes one operation at a time) go at once, their
/// words cleared (SK4.2): 0.17 asks again in slskd's lane and never keeps a 429, so this only ever meets 0.16.3's. How many. Pure.
fn cleared_429s(items: &mut [Item]) -> usize {
    let mut n = 0;
    for i in items.iter_mut().filter(|i| i.why.as_deref().is_some_and(|w| w.contains("429 Too Many Requests"))) {
        (i.why, i.due) = (None, 0);
        n += 1;
    }
    n
}

/// Once, at the first start (SK5.6, OQ-SK6 (a)), before the round's first sweep: the users 0.16.3 set aside for notes alone
/// (clipping, a pop at the very end, clicks: every file's why in the gate's note words, `flaws::only_notes`), which 0.17 no longer
/// refuses, taken out of the item's `failed`, where every set-aside of theirs on it is so. A found, waiting or stuck item that had one
/// is searched again now, as Search now does, its misses 0: a search finds those folders, and today's check judges a fresh download.
/// One under way keeps its download, only unblamed; a placed one is untouched. The set-asides stay listed till their 14 days. Marked
/// in the queue, so a later start does nothing again (a user who failed since for a real fault stays failed). Whether it ran. Pure.
fn notes_pass(k: &mut Kept) -> bool {
    if k.notes_passed {
        return false;
    }
    k.notes_passed = true;
    let mut asked = 0;
    for i in k.items.iter_mut().filter(|i| i.state != State::Placed) {
        let notes_alone = |user: &str| {
            let mut theirs = i.set_aside.iter().filter(|s| s.user == user).peekable();
            theirs.peek().is_some() && theirs.all(|s| !s.why.is_empty() && s.why.values().all(|w| crate::analysis::flaws::only_notes(w)))
        };
        let freed: Vec<String> = i.failed.iter().filter(|u| notes_alone(u)).cloned().collect();
        if freed.is_empty() {
            continue;
        }
        asked += freed.len();
        i.failed.retain(|u| !freed.contains(u));
        if matches!(i.state, State::Found | State::Waiting | State::Stuck) {
            search_again(i);
            i.misses = 0;
        }
    }
    if asked > 0 {
        tracing::info!(target: "soulseek", users = asked, "users set aside for notes alone asked again");
    }
    true
}

/// What an unreadable queue is set aside as, before its time (`set_aside_unreadable`).
const UNREADABLE: &str = "soulseek.json.unreadable-";

/// The queues set aside in the data folder `dir` (`set_aside_unreadable`), as the folder holds them now, at this start or an
/// earlier one: each file's name and when it was set aside (the time in its name; None where that was changed), the latest first.
/// Health's "Soulseek's queue was set aside" while any is there (P7, W7). Blocking.
pub fn queues_set_aside(dir: &Path) -> Vec<(String, Option<i64>)> {
    let mut out: Vec<(String, Option<i64>)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let at = name.strip_prefix(UNREADABLE)?.parse().ok();
            Some((name, at))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Whether the queue could be neither read nor set aside at this start (`load`), so nothing is written to it: Health's "Soulseek's
/// queue can't be kept" (P7). Not read yet: not.
pub fn frozen() -> bool {
    KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|k| k.frozen)
}

/// Tests elsewhere: a start's load of the queue at `p` (`load`): how many items it holds, and whether it froze.
#[cfg(any(test, feature = "test-support"))]
pub fn load_for_tests(p: &Path) -> (usize, bool) {
    let k = load(p);
    (k.items.len(), k.frozen)
}

/// An unreadable queue set aside under its time (`load`), a new one begun in its place.
fn set_aside_unreadable(p: &Path, why: &str) -> Kept {
    let aside = p.with_file_name(format!("{UNREADABLE}{}", now()));
    match std::fs::rename(p, &aside) {
        Ok(()) => {
            tracing::error!(target: "soulseek", file = %p.display(), aside = %aside.display(), reason = why, "the Soulseek queue could not be read: set aside, a new one begun");
            Kept { notes_passed: true, ..Kept::default() }
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

/// Now (epoch s), as the turns read it (`moment::now_secs`): a test moves it.
fn now() -> i64 {
    super::moment::now_secs()
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

/// The first artist credited: in `lidarr`, its first user below Soulseek (SPLIT-PLAN P7).
pub use crate::lidarr::main_artist;

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

// Whether a file's title (one way its name reads, `titles_of`) is a list track's: the one title rule, X3's, the domain's
// (PAIR01, OQ-PAIR01-1 (a)), which the holding's copies pair by too.
use cratarr_domain::identity::titled_as;

fn format_of(f: &Offered) -> String {
    f.extension.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| crate::slskd::base_name(&f.filename).rsplit('.').next().unwrap_or("").to_string()).to_lowercase()
}

/// The disc and the number a file's path says (PAIR01's Soulseek `fits`): its disc from a disc folder ("CD2" is 2), its number from
/// its name's leading digits. They only break ties (`assign`).
fn place_of(path: &str) -> (Option<u32>, Option<u32>) {
    let disc = disc_of(path).and_then(|d| d.trim_start_matches(|c: char| !c.is_ascii_digit()).split(|c: char| !c.is_ascii_digit()).next()?.parse().ok());
    let name = crate::slskd::base_name(path);
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    (disc, digits.parse().ok())
}

/// The folders among a search's answers that hold every track of the release, paired one for one by `assign` (PAIR01): the most
/// pairs, then the fewest by title alone, then the least difference in length, ties by the files' places and paths; each pair by
/// the one title rule and the one length rule (within the slack, where both are known). A folder is a candidate exactly when every
/// track pairs; its files are each track's, in the release's order. Files are FLAC or MP3 (the library's formats, which Cratarr
/// writes tags into), never locked; users in `failed` are left out. A length no file can have, or one past what milliseconds hold,
/// is never within the slack: read as unknown, its file pairs by title alone and its folder is never certain (G2). Best first:
/// lossless, the highest bitrate (G3), a free slot, a short queue, fewest extra files, the fastest.
pub fn folders_of(answers: &[Answer], tracks: &[Track], failed: &[String]) -> Vec<Folder> {
    use cratarr_domain::assign::{Fit, Side, assign, fit, ms_of_secs};
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
        for (dir, files) in dirs {
            if files.len() < tracks.len() {
                continue;
            }
            // Each file's length in whole milliseconds, by a checked multiply (T4): none where it can't be held.
            let ms: Vec<Option<i64>> = files.iter().map(|f| f.length.and_then(ms_of_secs)).collect();
            let titles: Vec<Vec<String>> = files.iter().map(|f| titles_of(&f.filename)).collect();
            let places: Vec<(Option<u32>, Option<u32>)> = files.iter().map(|f| place_of(&f.filename)).collect();
            let list: Vec<Side> = tracks.iter().map(|t| Side { disc: Some(t.disc), number: Some(t.position), key: &t.title, length_ms: t.length_ms }).collect();
            let sides: Vec<Side> = files.iter().zip(&places).zip(&ms).map(|((f, (disc, number)), ms)| Side { disc: *disc, number: *number, key: &f.filename, length_ms: *ms }).collect();
            let pairs = assign(&list, &sides, |t, f| if titles[f].iter().any(|n| titled_as(n, &tracks[t].title)) { fit(tracks[t].length_ms, ms[f]) } else { Fit::No });
            if pairs.len() < tracks.len() {
                continue;
            }
            // A length that can't be held reads as unknown from here on (the check, the ranks, `certain_folder`).
            let picked: Vec<Offered> = pairs.iter().map(|&(_, f)| Offered { length: files[f].length.filter(|_| ms[f].is_some()), ..files[f].clone() }).collect();
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
    // Lossless first, then bitrate (OQ-SK1, the owner: "lossless first (then bitrate)", G3), then a free slot and a short queue.
    let kbps = |f: &Folder| f.files.iter().map(|o| o.bit_rate.unwrap_or(0)).min().unwrap_or(0);
    out.sort_by(|a, b| {
        b.lossless
            .cmp(&a.lossless)
            .then(kbps(b).cmp(&kbps(a)))
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
        // Cratarr's own pick goes on by itself only where the next folder is certain too (SK3.7); else it waits for a pick, Found.
        if i.auto && certain(i, now()).is_none() {
            (i.state, i.chosen, i.auto) = (State::Found, None, false);
        }
        true
    }
}

/// A record held: what had to be asked could not be, which says nothing of its folder or its user. It stays as it is, its
/// tick, its folders and its batch with it, and is taken up again at `due`; `why` says what it waits for. Not Waiting,
/// which is the search's: searched again it would be Found, asking for a tick, and what it fetched would lie forgotten.
fn hold(i: &mut Item, why: String, wait: i64) {
    (i.why, i.due) = (Some(why), now() + wait);
}

/// When a record whose source could not answer for it is taken up again, by the failure's class (TN.1, TN.6): a pause at its
/// deadline (a moment on at least), no attempt used; away (down, not answering, switched off, background reading paused) after
/// `away` seconds. None where the failure says something of the user, the file or the record, or is not understood: each reader
/// does with it what it did.
fn again_at(e: &anyhow::Error, away: i64) -> Option<i64> {
    let now = now();
    match super::turn::class_at(e, now) {
        Failure::Paused { until } => Some(until.max(now + 1)),
        Failure::Away(_) => Some(now + away),
        _ => None,
    }
}

/// A record held until `due` for a source that could not answer (`again_at`), saying why: as it is, its tick, its folders and its
/// batch with it (`hold`).
fn held_to(i: &mut Item, e: &anyhow::Error, due: i64) {
    (i.why, i.due) = (Some(format!("{e:#}")), due);
}

/// What a user's fetch held by the disk floor says (AL03.2): no transfer of it begins while the server is short of space.
pub const SHORT_OF_SPACE: &str = "The server is short of space";
/// A fetch the floor held is looked at again after this long, and at once when the server is no longer short (`space_again`).
const FLOOR_AGAIN: i64 = 60;

/// The fetches the disk floor held, taken up now (AL03.2): the server is no longer short of space (`room::space`).
pub fn space_again() {
    let taken = kept(|k| {
        let held: Vec<&mut Item> = k.items.iter_mut().filter(|i| i.state == State::Approved && i.why.as_deref() == Some(SHORT_OF_SPACE)).collect();
        let n = held.len();
        for i in held {
            (i.due, i.why) = (0, None);
        }
        n
    });
    if taken > 0 {
        wake();
    }
}

/// slskd switched on again (Settings › Server): the records waiting because it was off are taken up now (TN.12), not a round later.
pub fn switched_on() {
    let off = super::references::SwitchedOff("slskd").to_string();
    let now = now();
    let taken = kept(|k| {
        let waiting: Vec<&mut Item> = k.items.iter_mut().filter(|i| i.why.as_deref() == Some(off.as_str()) && i.due > now).collect();
        let n = waiting.len();
        for i in waiting {
            i.due = now;
        }
        n
    });
    if taken > 0 {
        wake();
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

pub fn view<H: RequestsHost>(app: &Arc<H>) -> View {
    start(app);
    View { items: kept(|k| k.items.clone()), library: library().ok().map(|p| p.display().to_string()) }
}

/// A tick (RQ01.8 (b)): an item and its folder chosen by its place (none: the best), and, from a request's story, the source it names
/// there (its user, its folder's last part), so a tick sent before a new search changed the folders fetches from no other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Pick {
    pub id: String,
    #[serde(default)]
    pub folder: Option<usize>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub folder_name: Option<String>,
}

/// What a tick naming a source no longer at its place says (RQ01.8 (b)).
pub const SOURCE_GONE: &str = "That source is gone; look again";

/// These records approved, each with the folder given (else the best); the queue as it is now. A tick naming a source no longer at
/// its place is refused (`SOURCE_GONE`), and nothing is ticked.
pub fn approve<H: RequestsHost>(app: &Arc<H>, picked: &[Pick]) -> Result<View> {
    kept(|k| approve_in(&mut k.items, picked))?;
    wake();
    Ok(view(app))
}

/// [`approve`]'s, on the list's items: each Found item ticked approved with its folder (the first tick wins: one approved since is
/// left as it is, RQ01.8 (a)); none where a tick names a source not at its place now. Pure.
fn approve_in(items: &mut [Item], picked: &[Pick]) -> Result<()> {
    // Gone: no item of that id holds the source the tick names at its place (a new search moved it, or the item went).
    let gone = |p: &Pick| {
        (p.user.is_some() || p.folder_name.is_some())
            && !items.iter().find(|i| i.id == p.id).and_then(|i| i.folders.get(p.folder.unwrap_or(0))).is_some_and(|f| {
                p.user.as_ref().is_none_or(|u| *u == f.user) && p.folder_name.as_deref().is_none_or(|n| n == folder_name(&f.dir))
            })
    };
    if picked.iter().any(gone) {
        bail!(SOURCE_GONE);
    }
    for p in picked {
        if let Some(i) = items.iter_mut().find(|i| i.id == p.id && i.state == State::Found) {
            let f = p.folder.unwrap_or(0);
            if f < i.folders.len() {
                (i.state, i.chosen, i.why) = (State::Approved, Some(f), None);
            }
        }
    }
    Ok(())
}

/// A record searched now, whatever its wait (and from the ladder's first rung).
pub fn search_now<H: RequestsHost>(app: &Arc<H>, id: &str) -> View {
    with_item(id, search_again);
    wake();
    view(app)
}

/// An item searched again by hand (Search now): due now, its word gone, and its list read again, as its record counts now (R25,
/// N7: a list chosen by hand since reaches it; the folders found by the old one go with it). One in flight is left as it is.
fn search_again(i: &mut Item) {
    if matches!(i.state, State::Waiting | State::Found | State::Stuck) {
        (i.state, i.due, i.why, i.chosen) = (State::Waiting, 0, None, None);
        i.notes.clear();
        i.tracks.clear();
        i.untimed.clear();
        i.folders.clear();
    }
}

/// What Soulseek says of a request's record: in base, as a request's state holds it (`soulseek_base`).
pub use super::soulseek_base::Word;

/// What Soulseek says of these requests (W5), each given with its record: its item's word (`item_of`), by the request's id.
pub fn item_words(requests: &[(i64, Option<i64>)]) -> std::collections::HashMap<i64, Word> {
    kept(|k| words_in(&k.items, requests))
}

/// [`item_words`]'s: pure (V1).
fn words_in(items: &[Item], requests: &[(i64, Option<i64>)]) -> std::collections::HashMap<i64, Word> {
    requests.iter().filter_map(|&(request, record)| Some((request, word_in(items, item_of(items, request, record)?)))).collect()
}

/// An item's word as the list tells it (W5): a picked record whose user's turn another record of theirs holds (SK1.2: never two of
/// one user at once) waits for it, saying so (`waiting_for`), worked out from the list and never kept in its why; else its own word
/// (`word_of`). Pure.
fn word_in(items: &[Item], i: &Item) -> Word {
    if i.state == State::Approved
        && i.batch.is_none()
        && i.why.as_deref() != Some(SHORT_OF_SPACE)
        && let Some(user) = chosen_user(i)
        && items.iter().any(|o| o.id != i.id && downloading(o) && chosen_user(o) == Some(user))
    {
        return Word::Waiting { why: Some(waiting_for(user)) };
    }
    word_of(i)
}

/// What a picked record says while its user's other record downloads (SK1.2).
fn waiting_for(user: &str) -> String {
    format!("Waiting for {user}'s other record")
}

/// A request's item (RQ01.5): its own, else its record's, so every open request of a record reads its one item (one item a record,
/// `take_requests`); of two (a placed copy short of its list and the one looking again, Y2), the later. Pure.
fn item_of(items: &[Item], request: i64, record: Option<i64>) -> Option<&Item> {
    items.iter().rev().find(|i| i.record.request == request).or_else(|| record.filter(|r| *r != 0).and_then(|r| items.iter().rev().find(|i| i.record.record == r)))
}

/// An item's word (W5), as the item alone tells it.
fn word_of(i: &Item) -> Word {
    match i.state {
        State::Searching => Word::Searching,
        // Found, and held for its asker's room or the server's space (SK3.5): waiting, in those words.
        State::Found if matches!(i.why.as_deref(), Some(super::requests::WAITS_FOR_ROOM | super::requests::WAITS_FOR_SPACE)) => Word::Waiting { why: i.why.clone() },
        State::Found => Word::Found,
        // Held by the disk floor (AL03.2): no transfer begun, so not fetching.
        State::Approved if i.why.as_deref() == Some(SHORT_OF_SPACE) => Word::Waiting { why: i.why.clone() },
        State::Approved | State::Fetching => Word::Fetching { percent: i.percent },
        State::Checking | State::Placed => Word::Fetching { percent: 100.0 },
        State::Waiting => Word::Waiting { why: i.why.clone() },
        State::Stuck => Word::Stuck { why: i.why.clone() },
    }
}

/// A request's record's item as its story tells it (RQ01.4), whole, for its asker too (the owner: Soulseek's user names included):
/// its state, how far while fetched, the sources found (by Soulseek user, in its order; never a path on the server), the one ticked,
/// the users who failed it, what holds it and till when, why it is stuck, its misses and next search, what was set aside and why per
/// file, and whether slskd is switched off (nothing moves). Its whys whole: a user's story cuts them (`requests::story`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    pub id: String,
    pub state: State,
    /// 0 to 100, while fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    pub sources: Vec<Source>,
    /// Of `sources`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<usize>,
    pub failed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held: Option<Held>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stuck: Option<String>,
    pub misses: u32,
    /// Epoch seconds, while waiting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_search: Option<i64>,
    pub set_aside: Vec<Aside>,
    pub off: bool,
}

/// A folder found, as a story shows it (RQ01.4): whose, its own name (its path's last part), how good and how soon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    pub user: String,
    pub folder: String,
    pub format: String,
    pub lossless: bool,
    pub free_slot: bool,
    pub queue: i64,
    pub speed: i64,
    pub extra: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unchecked: Option<String>,
}

/// What holds an item, and till when (epoch seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Held {
    pub why: String,
    pub until: i64,
}

/// A folder set aside, as a story shows it: whose, and why each file failed; never where it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Aside {
    pub user: String,
    pub files: Vec<AsideFile>,
}

/// A file set aside, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AsideFile {
    pub file: String,
    pub why: String,
}

/// The item of a request's record as its story tells it at `now` (RQ01.4, RQ01.5): the request's own item, else its record's
/// (`item_of`). None where its record has none.
pub fn part_of(request: i64, record: Option<i64>, now: i64) -> Option<Part> {
    let off = !super::switches::is_on(super::switches::SLSKD);
    kept(|k| item_of(&k.items, request, record).map(|i| part(i, now, off)))
}

/// [`part_of`]'s: an item as a story tells it at `now`, `off` where slskd is switched off. Held: ticked, fetched or checked, and
/// waiting for its time with why (`hold`, the disk floor). Pure.
fn part(i: &Item, now: i64, off: bool) -> Part {
    let held = (matches!(i.state, State::Approved | State::Fetching | State::Checking) && i.due > now).then(|| i.why.clone()).flatten().map(|why| Held { why, until: i.due });
    let source = |f: &Folder| Source {
        user: f.user.clone(),
        folder: folder_name(&f.dir).to_string(),
        format: f.format.clone(),
        lossless: f.lossless,
        free_slot: f.free_slot,
        queue: f.queue,
        speed: f.speed,
        extra: f.extra,
        unchecked: f.unchecked.clone(),
    };
    Part {
        id: i.id.clone(),
        state: i.state,
        percent: (i.state == State::Fetching).then_some(i.percent),
        sources: i.folders.iter().map(source).collect(),
        chosen: i.chosen,
        failed: i.failed.clone(),
        held,
        stuck: (i.state == State::Stuck).then(|| i.why.clone()).flatten(),
        misses: i.misses,
        next_search: (i.state == State::Waiting && i.due > now).then_some(i.due),
        set_aside: i.set_aside.iter().map(|a| Aside { user: a.user.clone(), files: a.why.iter().map(|(file, why)| AsideFile { file: file.clone(), why: why.clone() }).collect() }).collect(),
        off,
    }
}

/// A folder's own name: the last part of its path as its user shares it.
fn folder_name(dir: &str) -> &str {
    dir.rsplit(['\\', '/']).find(|part| !part.is_empty()).unwrap_or(dir)
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
#[cfg(any(test, feature = "test-support"))]
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
        ended_at: 0,
        forgotten: 0,
        auto: false,
        found_at: 0,
        notes: BTreeMap::new(),
    };
    kept(|k| k.items.push(i));
}

/// Tests elsewhere: a Placed item of `record` on the list as `id`, its folder read (U3).
#[cfg(any(test, feature = "test-support"))]
pub fn placed_for_tests(id: &str, record: i64) {
    list_for_tests(id, record);
    kept(|k| {
        if let Some(i) = k.items.iter_mut().find(|i| i.id == id) {
            i.state = State::Placed;
        }
    });
}

/// Tests elsewhere: whether an item `id` is on the list.
#[cfg(any(test, feature = "test-support"))]
pub fn listed_for_tests(id: &str) -> bool {
    kept(|k| k.items.iter().any(|i| i.id == id))
}

/// Tests elsewhere: item `id` on the list as `f` leaves it (`list_for_tests` puts one there).
#[cfg(any(test, feature = "test-support"))]
pub fn change_for_tests(id: &str, f: impl FnOnce(&mut Item)) {
    with_item(id, f);
}

/// Tests elsewhere: an item of `record` on the list as `id`, picked for `user`'s folder of one file ("01 Slack Water.flac").
#[cfg(any(test, feature = "test-support"))]
pub fn picked_for_tests(id: &str, record: i64, user: &str) {
    list_for_tests(id, record);
    let file = Offered { filename: r"d\01 Slack Water.flac".into(), size: 1000, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: Some(100), is_locked: false };
    let folder = Folder { user: user.into(), dir: "d".into(), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 1, extra: 0, files: vec![file], unchecked: None };
    with_item(id, |i| (i.state, i.folders, i.chosen) = (State::Approved, vec![folder], Some(0)));
}

/// Tests elsewhere: a picked item's batch asked for, as the downloads' worker does.
#[cfg(any(test, feature = "test-support"))]
pub async fn start_for_tests(id: &str, slskd: &Slskd, folder: &Path) -> bool {
    start_downloads(&|i: &Item| i.id == id, slskd, folder, &Look::roomy()).await;
    item(id).is_some_and(|i| i.state == State::Fetching)
}

/// Tests elsewhere: item `id` as the list holds it.
#[cfg(any(test, feature = "test-support"))]
pub fn item_for_tests(id: &str) -> Option<Item> {
    item(id)
}

/// Tests elsewhere: item `id`'s fetch failed on `user` (refused, offline): the next folder chosen, as a fetch does (`on_failed`).
#[cfg(any(test, feature = "test-support"))]
pub fn failed_for_tests(id: &str, user: &str) {
    with_item(id, |i| {
        on_failed(i, user, format!("{user} is offline"));
    });
}

/// A record taken off the list: its Request is not taken in again, and nobody is blamed. One under way stops (OQ-SK2 (a), the owner:
/// "Out of the list" also stops a transfer under way): its transfers dropped in slskd, what it fetched or checked removed, and any
/// record waiting for its user goes.
pub async fn drop_item<H: RequestsHost>(app: &Arc<H>, id: &str) -> View {
    if let Some(it) = dropped(id)
        && let Ok((slskd, folder)) = super::track_fetch::slskd_ready().await
    {
        cancel_fetch(&slskd, &folder, &it).await;
        wake_starts();
        room_again();
    }
    view(app)
}

/// The record off the list, as it was, for its fetch to be stopped (OQ-SK2 (a)).
fn dropped(id: &str) -> Option<Item> {
    kept(|k| {
        let at = k.items.iter().position(|i| i.id == id)?;
        let i = k.items.remove(at);
        // By its record (X2); one with no record yet by its request, made its record when it has one.
        if i.record.record != 0 {
            k.dropped_records.push(i.record.record);
        } else {
            k.dropped.push(i.record.request);
        }
        Some(i)
    })
}

/// The placed records cleared from the list.
pub fn clear_placed<H: RequestsHost>(app: &Arc<H>) -> View {
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
    #[cfg(any(test, feature = "test-support"))]
    if let Some(lib) = OWN_LIBRARY.with(|l| l.borrow().clone()) {
        return Ok(lib);
    }
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let dir = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    let saved = crate::settings::load(&dir.join("settings.json"), &|k: &str| std::env::var(k).ok())?.context("Nothing is set up yet")?;
    taking(&saved)
}

/// `library_in`, or why none: no Incoming library takes Soulseek's records, which only an administrator can change (TN.1: Stuck).
pub(crate) fn taking(saved: &crate::settings::Saved) -> Result<PathBuf> {
    library_in(saved).ok_or_else(|| super::turn::Stuck("No Incoming library takes Soulseek's records: choose one in its settings".into()).into())
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
    wake_starts();
}

/// Whether Soulseek fetches here: slskd set up and switched on (Settings › Server), as single-track intake's.
pub async fn connected() -> bool {
    super::track_fetch::slskd_ready().await.is_ok()
}

/// The workers started, once: one searches (one search at a time); one starts downloads, every picked record at once (never two of
/// one user), each followed by its own follower; one checks and places them, one at a time (SK1).
pub fn start<H: RequestsHost>(app: &Arc<H>) {
    start_on(app.clone())
}

/// [`start`], on its host.
fn start_on(app: Arc<dyn RequestsHost>) {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let Some(served) = crate::serve::instance() else { return };
    let Some(dir) = served.data_dir.clone() else { return };
    let _ = FILE.set(dir.join("soulseek.json"));
    if STARTED.set(()).is_err() {
        return;
    }
    restarted();
    let round_app = app.clone();
    crate::run::spawn(async move {
        let app = round_app;
        loop {
            // Nothing searched until an Incoming library takes Soulseek's records (soularr may still be at work).
            if library().is_err() {
                let _ = tokio::time::timeout(Duration::from_secs(600), WAKE.notified()).await;
                continue;
            }
            // The approved Requests, read each round (they are Cratarr's own: no service asked).
            if let Err(e) = read_requests(&served, &*app).await {
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
    crate::run::spawn(async move {
        // Hidden folders placings left (Y12): swept once, before this start's first fetch, so none of its own is taken.
        sweep_at_start().await;
        crate::run::spawn(checks());
        downloads().await;
    });
}

/// What a restart left half done, taken up as the list is first read at the start (`start`): a search starts again, a fetch is
/// followed again. One held (at its check, or for its source) waits for its time as it was kept, its batch and its files where they
/// are (TN.9): nothing of it is sent back, and the start's sweep (`sweep_at_start`) is of the library's hidden folders alone. And once,
/// before the round's first sweep of the set-asides, 0.16.3's users set aside for notes alone are asked again (SK5.6), in this first
/// write.
fn restarted() {
    kept(|k| {
        for i in k.items.iter_mut().filter(|i| i.state == State::Searching) {
            i.state = State::Waiting;
        }
        notes_pass(k);
    });
}

/// [`sweep_hidden`] in the Soulseek library, once at the start (Y12), the items as the list holds them.
async fn sweep_at_start() {
    let Ok(lib) = library() else { return };
    let items = kept(|k| k.items.clone());
    let gone = crate::run::spawn_blocking(move || sweep_hidden(&lib, &items)).await.unwrap_or_default();
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
async fn read_requests(served: &Arc<crate::serve::Serve>, app: &dyn RequestsHost) -> Result<()> {
    // Folders set aside past their 14 days, looked for hourly (X4).
    static SWEPT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    if let Some(data) = served.data_dir.clone()
        && now() - SWEPT.load(std::sync::atomic::Ordering::Relaxed) >= 3600
    {
        SWEPT.store(now(), std::sync::atomic::Ordering::Relaxed);
        crate::run::spawn_blocking(move || sweep_set_asides(&data, now())).await.ok();
    }
    let (ud, cat, kinds) = (served.userdata.clone(), served.catalogue().clone(), served.library.kinds());
    // Placed for a record retired meanwhile (X4, D17): its request follows the record its album joined or made, once its folder is
    // read (Y10), which the item then lets go of.
    let placed: Vec<(String, i64, i64, String, i64)> = kept(|k| {
        k.items.iter().filter(|i| i.state == State::Placed && i.record.record != 0).filter_map(|i| Some((i.id.clone(), i.record.request, i.record.record, i.placed.clone()?, i.placed_at))).collect()
    });
    let (approved, of, (moved, read)) = crate::run::spawn_blocking(move || -> Result<_> {
        let moved = repoint(&ud, &cat, &placed)?;
        let (approved, of) = requests_taken(&ud, &cat, &kinds)?;
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
    let firsts = firsts_of(&approved, &of, lidarr.is_some());
    *NOT_BEFORE.lock().unwrap_or_else(|e| e.into_inner()) = approved.iter().map(|r| r.id).zip(firsts.iter().copied()).collect();
    // The first arrival stops the other (X1), and a request that left takes its item with it (Y2): a fetch, a check or a placing
    // under way is cancelled, in slskd and on disk, at once.
    take_in(&approved, &of, &firsts, super::track_fetch::slskd_ready()).await;
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
    Ok(())
}

/// When each of these requests' records may first be searched (`first_search`, X1), Lidarr connected or not. Pure.
fn firsts_of(approved: &[crate::userdata::Request], of: &[Option<super::requests::RecordOf>], lidarr: bool) -> Vec<i64> {
    approved.iter().zip(of).map(|(r, o)| first_search(r, o.as_ref().and_then(|o| o.released.as_deref()).and_then(super::requests::release_day), lidarr)).collect()
}

/// The list taken by the approved requests (`take_requests`, each record's first search in `firsts`), and the fetches of the items it
/// took off stopped (X1: the first arrival stops the other; Y2: a request that left takes its item with it), slskd and its folder as
/// `ready` gives them: a fetch, a check or a placing under way cancelled, in slskd and on disk (`cancel_fetch`), at once.
async fn take_in(
    approved: &[crate::userdata::Request],
    of: &[Option<super::requests::RecordOf>],
    firsts: &[i64],
    ready: impl std::future::Future<Output = Result<(Slskd, PathBuf)>>,
) {
    let taken = kept(|k| take_requests(k, approved, of, firsts));
    if !taken.is_empty()
        && let Ok((slskd, downloads)) = ready.await
    {
        for it in &taken {
            cancel_fetch(&slskd, &downloads, it).await;
        }
    }
}

/// The requests Soulseek's list is kept by (`take_requests`), each with its record as `records_held` reads it: every approved
/// request (X2), not the newest thousand, but one waiting for its asker's room (AL02.2: no item of it till it goes; one waiting for
/// space keeps the item it had, AL03.2, and is given none, `take_requests`); a delivered one
/// is not approved, so never searched again ((c), AL02.8). Where each record holding no copy saw its last one go (U7, T8): only
/// one gone from Incoming, deleted there or removed outside Cratarr, is looked for again. Blocking.
pub(crate) fn requests_taken(
    ud: &crate::userdata::UserData,
    cat: &crate::catalogue::Catalogue,
    kinds: &[(i64, crate::settings::LibraryKind)],
) -> Result<(Vec<crate::userdata::Request>, Vec<Option<super::requests::RecordOf>>)> {
    let approved: Vec<crate::userdata::Request> = ud.requests(None, Some(super::requests::APPROVED), usize::MAX)?.into_iter().filter(|r| !super::requests::waits_for_room(r)).collect();
    let mut of = super::requests::records_held(cat, &approved, kinds)?;
    super::requests::mark_gone_from_incoming(cat, &mut of, kinds)?;
    Ok((approved, of))
}

/// The records `take_requests` makes items of from these requests, on a list of their own (the tests', AL02).
#[cfg(test)]
pub(crate) fn items_of(approved: &[crate::userdata::Request], of: &[Option<super::requests::RecordOf>]) -> Vec<i64> {
    let mut k = Kept::default();
    take_requests(&mut k, approved, of, &vec![0; approved.len()]);
    k.items.iter().map(|i| i.record.record).collect()
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

/// Each approved request's first search as the last round read it (`first_search`): Cratarr picks nothing of one before (G6).
static NOT_BEFORE: Mutex<BTreeMap<i64, i64>> = Mutex::new(BTreeMap::new());

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
        if matches!(p, Progress::Stuck { .. } | Progress::Unknown | Progress::NotWanted) {
            i.due = now;
        }
    }
}

/// A fetch under way stopped (X1: the first arrival stops the other; Y2: a request that left, too): its transfers dropped in slskd,
/// what came of them removed from slskd's batch folder, and so the checked files a placing that failed kept (X4).
async fn cancel_fetch(slskd: &Slskd, downloads: &Path, it: &Item) {
    // Let go as any download is: one slskd could not be asked for is dropped at the next look, its user's turn taken till then.
    if let (Some(batch), Some(user)) = (&it.batch, chosen_user(it)) {
        let_go(slskd, downloads, &it.id, user, batch).await;
    }
    for folder in it.batch.iter().chain(&it.checked) {
        std::fs::remove_dir_all(downloads.join(crate::slskd::FOLDER).join(folder)).ok();
    }
    tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), "its fetch stopped: it arrived otherwise, or its request left");
}

/// [`read_requests`]'s list kept, each request's record as `records_of` read it (`of`, in the requests' order): pure. Every
/// record is taken, with a release group or none: it is judged by its list, whosever that is (0100). An item 0.16 kept, by its
/// release group, is given its request's record (row 9.26) and goes on as it was: a fetch under way keeps its batch, its
/// folder and its tick, and is written back with its record; one Found by 0.16 is searched again (SK4.4). The items it takes off
/// with a fetch or checked files are handed back, to be cancelled (`cancel_fetch`).
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
            // Found by 0.16 (OQ-SK3 (a), SK4.4): its folders from 0.16's list and an answer of unknown age, so it is searched again
            // now, as Search now does, with 0.17's counting list, before anything may start from them.
            if i.state == State::Found {
                search_again(i);
                (i.found_at, i.auto) = (0, false);
            }
        }
    }

    // A record merged since into another (0.17's fold, F2.11; a copy joined, F11.1): its item names the record its request's is now,
    // as the request reads it (merges followed), so it is searched, placed and found had as that one.
    for i in k.items.iter_mut().filter(|i| i.record.record != 0) {
        if let Some(record) = record_of.get(&i.record.request).filter(|record| **record != i.record.record) {
            i.record.record = *record;
        }
    }
    // F15.6: two items of one record after a merge (by a master, a group, a fold): one searches it, the first made or the one under
    // way; the other is taken off where nothing of it is under way (ticked, fetched, checked, placed): one under way stays, the other
    // waits. Each request reads the record's item.
    let mut first: BTreeMap<i64, bool> = BTreeMap::new();
    for i in k.items.iter().filter(|i| i.record.record != 0) {
        let under_way = under_way(i);
        first.entry(i.record.record).and_modify(|u| *u |= under_way).or_insert(under_way);
    }
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let doubles: std::collections::HashSet<String> = k
        .items
        .iter()
        .filter(|i| i.record.record != 0)
        .filter(|i| {
            let again = !seen.insert(i.record.record) || (first.get(&i.record.record) == Some(&true) && !under_way(i));
            again && !under_way(i)
        })
        .map(|i| i.id.clone())
        .collect();
    stopped.extend(take_off(&mut k.items, |i| doubles.contains(&i.id)));
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
        // A Want waiting for space (AL03.1): no item of it till it goes; an item it has stays, its transfer held by the floor.
        if super::requests::waits_for_space(r) {
            continue;
        }
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
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

/// Whether something of an item is under way (F15.6): ticked, fetched, checked, or placed.
fn under_way(i: &Item) -> bool {
    stays(i) || i.chosen.is_some() || matches!(i.state, State::Approved | State::Fetching | State::Checking | State::Placed)
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
pub fn next_search() -> Option<String> {
    let t = now();
    kept(|k| next_to_search(k, t))
}

/// The next item to search at `t` (A1.4): a waiting one whose time came, and a Stuck one whose time came, its list read again (a
/// reference gained since, MusicBrainz back): the longest due first. Pure.
fn next_to_search(k: &Kept, t: i64) -> Option<String> {
    k.items.iter().filter(|i| matches!(i.state, State::Waiting | State::Stuck) && i.due <= t).min_by_key(|i| (i.due, i.id.clone())).map(|i| i.id.clone())
}

/// The next item a worker takes up at `now`, as the tests read it: a follower or the checks one whose fetch a restart left under way,
/// then the downloads an approved one; one held waits for its time. Pure.
#[cfg(any(test, feature = "test-support"))]
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

/// How long an item with no record yet waits for the round to give it one.
const RECORD_AGAIN: i64 = 60;

/// How long an item waits after Bandcamp answered with its bot check (R23, N3): at most once a day, nothing done to get past it.
const CHECKED_AGAIN_SECS: i64 = 86_400;

/// A record's counting list, read through `read` (`references::tracklist`, kept there once read), else what not reading it comes to
/// (`not_listed`). None is read while background reading is paused (Y7): the item waits a round, never Stuck.
async fn listing<R, F>(it: &Item, cat: &Arc<crate::catalogue::Catalogue>, read: R) -> Listing
where
    R: Fn(crate::catalogue::records::Ref) -> F,
    F: std::future::Future<Output = Result<Option<super::references::Read>>>,
{
    // An item 0.16 kept is given its record by the round: till then it waits a while, never Stuck a day for want of one.
    if it.record.record == 0 {
        return Listing::Later("Waits for its record".into(), RECORD_AGAIN);
    }
    let e = if super::switches::background_paused() {
        anyhow::anyhow!(super::switches::PAUSED)
    } else {
        match super::references::tracklist(cat, it.record.record, read).await {
            Ok(Ok(lists)) => return Listing::Ready(lists),
            // No list to judge by: what someone must act on (TN.1).
            Ok(Err(why)) => super::turn::Stuck(why).into(),
            Err(e) => e,
        }
    };
    not_listed(&e, now())
}

/// A record's list not read, by its failure's class at `now` (TN.1, TN.13): no list to judge by, Stuck; Bandcamp's bot check, a day
/// (R23); a source paused (MusicBrainz's 503), held to its deadline, no try counted; away or not understood, a round. Pure.
fn not_listed(e: &anyhow::Error, now: i64) -> Listing {
    match super::turn::class_at(e, now) {
        Failure::Stuck => Listing::None(format!("{e:#}")),
        _ if crate::bandcamp::not_read(e) => Listing::Later("Bandcamp didn't answer".into(), CHECKED_AGAIN_SECS),
        Failure::Paused { until } => Listing::Later(format!("{e:#}"), (until - now).max(1)),
        _ => Listing::Later(format!("{e:#}"), ROUND_AT_LEAST),
    }
}

/// One record searched: its list first (once), then the ladder's rungs in turn until one finds folders.
async fn search(id: &str) {
    let Some(served) = crate::serve::instance() else { return };
    let ready = async { super::track_fetch::slskd_ready().await.map(|(s, _)| s) };
    search_in(id, served.catalogue(), super::references::read_reference, ready).await;
}

/// [`search`], its list read through `read`, slskd as `ready` gives it (the tests' stand in).
pub async fn search_in<R, F>(id: &str, cat: &Arc<crate::catalogue::Catalogue>, read: R, ready: impl std::future::Future<Output = Result<Slskd>>)
where
    R: Fn(crate::catalogue::records::Ref) -> F,
    F: std::future::Future<Output = Result<Option<super::references::Read>>>,
{
    let Some(it) = item(id) else { return };
    if !read_its_list(id, &it, cat, read).await {
        return;
    }
    let slskd = match ready.await {
        Ok(s) => s,
        Err(e) => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(format!("{e:#}")), now() + ROUND_AT_LEAST)),
    };
    search_through(id, &slskd).await;
}

/// A record taken to be searched (`search`): Searching, its list read first where it has none (once; a Stuck one's again, A1.4).
/// Whether it goes on to its search: else it waits (`Later`, R23) or is Stuck a day (no list).
async fn read_its_list<R, F>(id: &str, it: &Item, cat: &Arc<crate::catalogue::Catalogue>, read: R) -> bool
where
    R: Fn(crate::catalogue::records::Ref) -> F,
    F: std::future::Future<Output = Result<Option<super::references::Read>>>,
{
    with_item(id, |i| i.state = State::Searching);
    if it.tracks.is_empty() && it.untimed.is_empty() {
        match listing(it, cat, read).await {
            Listing::Ready(lists) => {
                let source = |l: &Option<(String, Vec<Track>)>| l.as_ref().map_or_else(String::new, |(s, _)| s.clone());
                tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), list = %source(&lists.timed), untimed = %source(&lists.untimed), "judged by its list");
                with_item(id, |i| (i.tracks, i.untimed) = (lists.timed.map(|(_, l)| l).unwrap_or_default(), lists.untimed.map(|(_, l)| l).unwrap_or_default()));
            }
            Listing::None(why) => {
                with_item(id, |i| (i.state, i.why, i.due) = (State::Stuck, Some(why), now() + wait_after(3)));
                return false;
            }
            Listing::Later(why, wait) => {
                with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(why), now() + wait));
                return false;
            }
        }
    }
    true
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
                        (i.folders, i.state, i.misses, i.why, i.found_at) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None, now());
                wake_starts();
                    });
                }
                heard.extend(a);
            }
            // slskd away or paused says nothing of the record: no miss (TN.6), or a week's wait would meet it when slskd came back; a
            // pause holds it to its source's deadline.
            Err(e) => match again_at(&e, ROUND_AT_LEAST) {
                Some(due) => return with_item(id, |i| (i.state, i.why, i.due) = (State::Waiting, Some(format!("{e:#}")), due)),
                None => why = Some(format!("{e:#}")),
            },
        }
    }
    // A fuller folder for the list (X3): every track of it there, and more; offered for the tick, saying what it takes of it.
    let fuller = fuller_folders(&heard, &it.tracks, &it.failed);
    if !fuller.is_empty() {
        tracing::info!(target: "soulseek", record = %record, folders = fuller.len(), "found in fuller folders alone");
        return with_item(id, |i| {
            (i.folders, i.state, i.misses, i.why, i.found_at) = (fuller.into_iter().take(SHOWN).collect(), State::Found, 0, None, now());
                wake_starts();
        });
    }
    let found = untimed_folders(&heard, &it.untimed, &it.failed);
    if !found.is_empty() {
        tracing::info!(target: "soulseek", record = %record, folders = found.len(), "found by titles alone: no list with lengths found a folder");
        return with_item(id, |i| {
            (i.folders, i.state, i.misses, i.why, i.found_at) = (found.into_iter().take(SHOWN).collect(), State::Found, 0, None, now());
                wake_starts();
        });
    }
    // Not found: a search's answer, not a dead end, as Soulseek's users change by the hour: searched
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

// ---- Downloads at once, checks one at a time (0.17's round 1.3, SK1)

/// What a look of the downloads' worker reads beside the list (SK1, SK3): the time, whether the server is below its disk floor and
/// whose requests that holds (a user's, AL03.2), the free space of a folder's disk, and the library records are placed in. Read anew
/// at each look (`Look::read`); the tests' stand-ins (`Look::roomy`).
#[derive(Clone)]
pub(crate) struct Look {
    pub now: i64,
    /// The server below its disk floor.
    pub short: bool,
    /// The requests whose asker is a user, not an administrator: the floor holds theirs.
    pub users: std::collections::HashSet<i64>,
    pub library: Option<PathBuf>,
    free: Arc<dyn Fn(&Path) -> Option<u64> + Send + Sync>,
    /// Requests whose asker is over their room (AL02): Cratarr's own picks of theirs wait (SK3.5).
    pub full: std::collections::HashSet<i64>,
    /// Records had by the had rule, and the requests still open (none read: every one), read at the look itself (SK3.6).
    pub had: std::collections::HashSet<i64>,
    pub open: Option<std::collections::HashSet<i64>>,
    /// Soulseek switched on (`switches::SLSKD`).
    pub on: bool,
    /// Requests Lidarr goes first for, and till when (the 6 November order, `first_search`): no automatic pick before (G6).
    pub not_before: HashMap<i64, i64>,
    /// Whether the server was read whole for this look (its requests, the had rule, the floor, the rooms): Cratarr's own picks
    /// fail closed without it (the review of part 3, H2).
    pub read: bool,
}

impl Look {
    /// The disks as they are, nothing held by the floor (as a test of today's fetcher read them).
    pub(crate) fn of_disks() -> Self {
        Self { now: now(), short: false, users: Default::default(), library: library().ok(), free: Arc::new(crate::folders::free), full: Default::default(), had: Default::default(), open: None, on: true, not_before: Default::default(), read: false }
    }

    /// The look of the server now: its disks, the floor as `room` reads it, each item's asker and whether they are over their room,
    /// the had rule and the open requests as the round reads them, and Soulseek's switch.
    async fn read() -> Self {
        let mut look = Self::of_disks();
        look.on = super::switches::is_on(super::switches::SLSKD);
        look.not_before = NOT_BEFORE.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(r, t)| (*r, *t)).collect();
        let Some(serve) = crate::serve::instance() else { return look };
        let requests: Vec<i64> = kept(|k| k.items.iter().map(|i| i.record.request).collect());
        let s = serve.clone();
        let read = crate::run::spawn_blocking(move || {
            let short = super::room::short_on(&s, &super::room::TheDisks);
            let askers: HashMap<i64, String> = requests.into_iter().filter_map(|r| Some((r, s.userdata.request(r).ok().flatten()?.user))).collect();
            let users = askers.iter().filter(|(_, a)| s.users.get(a).is_some_and(|u| !u.admin)).map(|(r, _)| *r).collect();
            let held = if s.has_catalogue() { requests_taken(&s.userdata, s.catalogue(), &s.library.kinds()).ok() } else { None };
            (short, users, askers, held)
        })
        .await;
        if let Ok((short, users, askers, held)) = read {
            (look.short, look.users) = (short, users);
            if let Some((approved, of)) = held {
                look.open = Some(approved.iter().map(|r| r.id).collect());
                look.had = of.iter().flatten().filter(|o| o.had()).map(|o| o.record).collect();
                look.read = true;
            }
            let mut accounts: Vec<String> = askers.values().cloned().collect();
            accounts.sort();
            accounts.dedup();
            let full = super::requests::full_askers(&serve, &accounts).await;
            look.full = askers.into_iter().filter(|(_, a)| full.contains(a)).map(|(r, _)| r).collect();
        }
        look
    }

    pub(crate) fn free(&self, dir: &Path) -> Option<u64> {
        (self.free)(dir)
    }
}

#[cfg(any(test, feature = "test-support"))]
// Shared with the tests of the crates above (test-support): the stand-ins only this crate's tests call are idle there.
#[cfg_attr(not(test), allow(dead_code))]
impl Look {
    /// Room on every disk, the floor far below, nothing held: the tests' stand-in.
    pub(crate) fn roomy() -> Self {
        Self { now: now(), short: false, users: Default::default(), library: None, free: Arc::new(|_| Some(u64::MAX / 4)), full: Default::default(), had: Default::default(), open: None, on: true, not_before: Default::default(), read: true }
    }
    /// Every disk `bytes` free.
    pub(crate) fn free_bytes(mut self, bytes: u64) -> Self {
        self.free = Arc::new(move |_| Some(bytes));
        self
    }
    /// Below the disk floor, these requests a user's.
    pub(crate) fn below_floor(mut self, users: &[i64]) -> Self {
        (self.short, self.users) = (true, users.iter().copied().collect());
        self
    }
    /// These requests' askers over their room.
    pub(crate) fn full(mut self, requests: &[i64]) -> Self {
        self.full = requests.iter().copied().collect();
        self
    }
    /// These records had.
    pub(crate) fn had(mut self, records: &[i64]) -> Self {
        self.had = records.iter().copied().collect();
        self
    }
    /// Only these requests open.
    pub(crate) fn open(mut self, requests: &[i64]) -> Self {
        self.open = Some(requests.iter().copied().collect());
        self
    }
    /// Soulseek switched off.
    pub(crate) fn off(mut self) -> Self {
        self.on = false;
        self
    }
    /// A look that could not read the server.
    pub(crate) fn unread(mut self) -> Self {
        self.read = false;
        self
    }
    /// Lidarr first for `request` till `until`.
    pub(crate) fn not_before(mut self, request: i64, until: i64) -> Self {
        self.not_before.insert(request, until);
        self
    }
    /// The look at `now`.
    pub(crate) fn at(mut self, now: i64) -> Self {
        self.now = now;
        self
    }
}

/// Which items a worker's look takes: every one on the server; a test's own (its ids).
pub(crate) type Within<'a> = &'a (dyn Fn(&Item) -> bool + Sync);

/// Every item of the list.
fn every(_: &Item) -> bool {
    true
}

/// Where a check's records go (the tests' own folders stand in): the Incoming library they are placed in, and the data folder their
/// set-asides are kept in.
pub(crate) struct Places {
    pub library: std::result::Result<PathBuf, String>,
    pub data: Option<PathBuf>,
}

impl Places {
    fn of_server() -> Self {
        Self { library: library().map_err(|e| format!("{e:#}")), data: crate::serve::instance().and_then(|s| s.data_dir.clone()) }
    }
}

/// The user an item's chosen folder is of.
fn chosen_user(i: &Item) -> Option<&str> {
    i.chosen.and_then(|c| i.folders.get(c)).map(|f| f.user.as_str())
}

/// Whether an item's batch is asked for or downloading (SK1.2): its user's turn is taken until every transfer of it is done.
fn downloading(i: &Item) -> bool {
    (i.state == State::Approved && i.batch.is_some()) || i.state == State::Fetching
}

/// Whether an item's files are on their way or on this server's disks (SK1.4): its batch asked for or downloading, waiting for its
/// check or being checked, or being placed.
fn under_way_here(i: &Item) -> bool {
    downloading(i) || (i.state == State::Checking && (i.batch.is_some() || i.checked.is_some())) || i.placing.is_some()
}

/// The bytes of an item's chosen folder.
fn bytes_of(i: &Item) -> u64 {
    i.chosen.and_then(|c| i.folders.get(c)).map_or(0, |f| f.files.iter().map(|o| o.size.max(0) as u64).sum())
}

/// What a start held for too little free space says, before its numbers (SK1.4): taken up again when a placing frees some.
const NO_ROOM: &str = "Too little free space for its files";

/// The next picked item of `within` a look starts (SK1): due, in the list's order, `tried` passed over, its user's turn free (never
/// two records of one user at once, SK1.2). One the floor holds (a user's, below it: AL03.2) or the free space holds (every record
/// under way weighed with it, SK1.4) is held on the way, saying so, blaming no one. Its batch's id kept on it before it is sent
/// (SK2.3), and whether that id is new (else kept from before, never answered: read back first). Pure but for the new id.
fn next_start(k: &mut Kept, within: Within, folder: &Path, look: &Look, tried: &std::collections::HashSet<String>) -> Option<Start> {
    let mine: Vec<usize> = (0..k.items.len()).filter(|&n| within(&k.items[n])).collect();
    // A user's turn is taken by a record of theirs downloading, and by a batch let go that is not dropped yet.
    let busy: Vec<String> = mine
        .iter()
        .map(|&n| &k.items[n])
        .filter(|i| downloading(i))
        .filter_map(|i| chosen_user(i).map(str::to_string))
        .chain(to_drop_of(k, within).map(|d| d.user.clone()))
        .collect();
    let weighed: u64 = mine.iter().map(|&n| &k.items[n]).filter(|i| under_way_here(i)).map(bytes_of).sum();
    for n in mine {
        let i = &mut k.items[n];
        if i.state != State::Approved || i.due > look.now || tried.contains(&i.id) {
            continue;
        }
        let Some(user) = chosen_user(i).map(str::to_string) else {
            (i.state, i.chosen, i.due, i.batch) = (State::Waiting, None, 0, None);
            continue;
        };
        // An id kept and never answered holds its user's turn itself.
        let kept_id = i.batch.is_some();
        // An automatic pick not yet sent is held at its start as at its pick (G9): below the floor anyone's, over its asker's room.
        if i.auto && !kept_id {
            // A look that could not read the server starts no pick of Cratarr's (H2): it waits for one that could.
            if !look.read {
                continue;
            }
            let wait = if look.short {
                Some(super::requests::WAITS_FOR_SPACE)
            } else if look.full.contains(&i.record.request) {
                Some(super::requests::WAITS_FOR_ROOM)
            } else {
                None
            };
            if let Some(why) = wait {
                (i.state, i.chosen, i.auto, i.why) = (State::Found, None, false, Some(why.into()));
                continue;
            }
        }
        if !kept_id && busy.contains(&user) {
            continue;
        }
        // One step for the floor and the room, for a new id and for a kept one sent again (the review of part 2, M1).
        let own = bytes_of(i);
        let held = if look.short && look.users.contains(&i.record.request) {
            Some((SHORT_OF_SPACE.to_string(), FLOOR_AGAIN))
        } else {
            let want = own.saturating_add(weighed - if kept_id { own } else { 0 }).saturating_add(ROOM_SPARE);
            let dirs: Vec<PathBuf> = std::iter::once(folder.to_path_buf()).chain(look.library.clone()).collect();
            dirs.iter().filter_map(|d| look.free(d)).find(|free| *free < want).map(|free| (format!("{NO_ROOM}: {} MB needed, {} MB free", want / 1_000_000, free / 1_000_000), SLSKD_AGAIN))
        };
        if kept_id {
            return Some(Start { id: i.id.clone(), fresh: false, held });
        }
        if let Some((why, wait)) = held {
            hold(i, why, wait);
            continue;
        }
        i.batch = Some(crate::slskd::new_batch());
        // A new folder's download: none of an earlier one's notes (the review's L7).
        i.notes.clear();
        return Some(Start { id: i.id.clone(), fresh: true, held: None });
    }
    None
}

/// How long a free slot counts after the search that found it (OQ-SK3): an older answer is searched again before Cratarr picks by it.
const FRESH: i64 = 3_600;

/// Whether a found item's best folder (`folders[0]`, the one an empty tick takes: OQ-SK1) is certain by today's judging alone (SK3):
/// it holds every track of the list the item is judged by, each paired with one file (`folders_of`), and no other audio file
/// (`extra` 0, nothing unchecked: not found by titles alone, no fuller folder); its user had a free slot when it was found, and has not
/// failed this record. A judging of its own it has none: it reads only what the search kept. Pure.
fn certain_folder(i: &Item) -> bool {
    let Some(f) = i.folders.first() else { return false };
    !i.tracks.is_empty()
        && f.unchecked.is_none()
        && f.extra == 0
        && f.files.len() == i.tracks.len()
        // Paired by title alone (slskd gave no length) is no certainty (G2).
        && f.files.iter().all(|o| o.length.is_some())
        && f.free_slot
        && !i.failed.contains(&f.user)
}

/// Cratarr's own pick of a found item at `now` (SK3.3): folder 0 where it is certain (`certain_folder`) and its free slot fresh, from a
/// search within the hour (OQ-SK3). Pure.
fn certain(i: &Item, now: i64) -> Option<usize> {
    (certain_folder(i) && i.found_at > 0 && now - i.found_at <= FRESH).then_some(0)
}

/// Cratarr's own picks at a look (SK3, the owner's "Automatic when certain"): each found item of `within` whose best folder is certain
/// and fresh, its record not had and its request still open (both read at the look itself), its asker within their room and the server
/// above its disk floor, picked as an empty tick picks it, marked as Cratarr's (`auto`). One certain but found over an hour ago, and
/// one 0.16 found (no time it was found), is searched again first (OQ-SK3); one waiting for room or space says so, its folders kept and
/// tickable. Soulseek switched off, nothing. The ids picked.
fn pick_certain(k: &mut Kept, within: Within, look: &Look) -> Vec<String> {
    let mut picked = Vec::new();
    // Fail closed (the review of part 3, H2): a look that could not read the server picks nothing by itself.
    if !look.on || !look.read {
        return picked;
    }
    for i in k.items.iter_mut().filter(|i| within(i) && i.state == State::Found && i.record.record != 0) {
        if look.had.contains(&i.record.record) || look.open.as_ref().is_some_and(|open| !open.contains(&i.record.request)) {
            continue;
        }
        // Lidarr first (the 6 November order): no automatic pick before its five minutes, even found early by Search now (G6).
        if look.not_before.get(&i.record.request).is_some_and(|until| look.now < *until) {
            continue;
        }
        let waiting = |i: &Item| matches!(i.why.as_deref(), Some(super::requests::WAITS_FOR_ROOM | super::requests::WAITS_FOR_SPACE));
        if i.found_at == 0 {
            search_again(i);
            continue;
        }
        if !certain_folder(i) {
            if waiting(i) {
                i.why = None;
            }
            continue;
        }
        if look.now - i.found_at > FRESH {
            search_again(i);
            continue;
        }
        // Below the floor nothing starts by itself, anyone's; over their room, a user's waits (Q2: room holds no tick).
        if look.short {
            i.why = Some(super::requests::WAITS_FOR_SPACE.into());
            continue;
        }
        if look.full.contains(&i.record.request) {
            i.why = Some(super::requests::WAITS_FOR_ROOM.into());
            continue;
        }
        (i.state, i.chosen, i.auto, i.why, i.due) = (State::Approved, Some(0), true, None, 0);
        tracing::info!(target: "soulseek", record = %format!("{} - {}", i.record.artist, i.record.album), user = %i.folders[0].user, "started by itself: its folder is certain");
        picked.push(i.id.clone());
    }
    picked
}

/// A start `next_start` chose: its item, whether its batch's id is new (else kept from before, never answered: read back first), and
/// what holds it where that kept id must be sent again (the floor, the room: M1).
struct Start {
    id: String,
    fresh: bool,
    held: Option<(String, i64)>,
}

/// One look of the downloads' worker (SK1): every picked record of `within` that may start, in the list's order, its batch asked
/// for, one after another through slskd's lane: they wait for each other only, never for a download or a check. No cap of Cratarr's
/// own: slskd's limits are the only brake (the owner's "No limit").
pub(crate) async fn start_downloads(within: Within<'_>, slskd: &Slskd, folder: &Path, look: &Look) {
    drop_again(within, slskd, folder).await;
    // Cratarr's own picks first (SK3): they then start as any picked record.
    kept(|k| pick_certain(k, within, look));
    let mut tried = std::collections::HashSet::new();
    while let Some(Start { id, fresh, held }) = kept(|k| next_start(k, within, folder, look, &tried)) {
        tried.insert(id.clone());
        start_one(&id, slskd, folder, fresh, held).await;
    }
}

/// Free space found again (SK1.4): the starts held for it go at the next look, once a placing ended or a fetch let its files go. Their
/// words stay till then: the look starts them, or holds them again saying so.
fn room_again() {
    let n = kept(|k| {
        let held: Vec<&mut Item> = k.items.iter_mut().filter(|i| i.state == State::Approved && i.due > 0 && i.why.as_deref().is_some_and(|w| w.starts_with(NO_ROOM))).collect();
        let n = held.len();
        for i in held {
            i.due = 0;
        }
        n
    });
    if n > 0 {
        wake_starts();
    }
}

/// A picked record's batch asked for (SK2): its id kept on the item before (`next_start`), the batch sent through slskd's lane, the
/// list looked at inside it just before each try (an item gone from the list, or no longer picked, sends nothing). An answer lost
/// is read back by that id: slskd has it, it is under way; slskd does not know it, it is sent again under that id, once. An id kept
/// and never answered (`fresh` false: a stop between) is read back first. slskd busy never reaches the item. Whether it is under
/// way now (Fetching, its batch kept).
async fn start_one(id: &str, slskd: &Slskd, folder: &Path, fresh: bool, held: Option<(String, i64)>) -> bool {
    let Some(it) = item(id).filter(|i| i.state == State::Approved) else { return false };
    let Some(batch) = it.batch.clone() else { return false };
    let Some(f) = it.chosen.and_then(|c| it.folders.get(c)).cloned() else {
        with_item(id, |i| (i.state, i.chosen, i.due, i.batch) = (State::Waiting, None, 0, None));
        return false;
    };
    let mut again = fresh;
    if !fresh {
        match slskd.transfers(&batch).await {
            Ok(_) => return taken(id, slskd, folder, &f.user, &batch).await,
            // Not known: sent again only where a new one would be (M1).
            Err(e) if e.is::<crate::slskd::NotKnown>() => {
                if let Some((why, wait)) = held {
                    with_item(id, |i| {
                        i.batch = None;
                        hold(i, why, wait);
                    });
                    return false;
                }
            }
            // slskd away: read back when due, by the failure's class (TN.6); busy (a read's 429, SK2.2), or anything else: at the
            // next look.
            Err(e) => {
                if let Some(due) = again_at(&e, SLSKD_AGAIN).filter(|_| !e.is::<crate::slskd::Busy>()) {
                    with_item(id, |i| held_to(i, &e, due));
                }
                return false;
            }
        }
    }
    loop {
        let still = || kept(|k| k.items.iter().any(|i| i.id == id && i.state == State::Approved && i.batch.as_deref() == Some(batch.as_str())));
        match slskd.fetch_many(&batch, &f.user, &f.files, still).await {
            Ok(()) => return taken(id, slskd, folder, &f.user, &batch).await,
            Err(e) if e.is::<crate::slskd::Gone>() => return false,
            Err(e) if e.is::<crate::slskd::Lost>() => match slskd.transfers(&batch).await {
                Ok(_) => return taken(id, slskd, folder, &f.user, &batch).await,
                Err(r) if r.is::<crate::slskd::NotKnown>() && again => again = false,
                Err(r) => {
                    // Not known after its second sending: a new id next time. Not read: its id kept, read back when due.
                    let forget = r.is::<crate::slskd::NotKnown>();
                    with_item(id, |i| {
                        if forget {
                            i.batch = None;
                        }
                        hold(i, format!("{e:#}"), SLSKD_AGAIN);
                    });
                    return false;
                }
            },
            Err(e) => {
                match again_at(&e, SLSKD_AGAIN) {
                    // slskd itself could not take it (not reached, its key, a proxy): never sent, held, no attempt used, no user
                    // blamed (TN.6).
                    Some(due) => with_item(id, |i| {
                        i.batch = None;
                        held_to(i, &e, due);
                    }),
                    None => {
                        with_item(id, |i| {
                            on_failed(i, &f.user, format!("{e:#}"));
                        });
                        wake_starts();
                    }
                }
                return false;
            }
        }
    }
}

/// A picked item's batch taken by slskd: under way, followed from now. Its item gone from the list while slskd took it (SK1.6, the
/// review of part 2's note), the batch is let go at once, never left running unfollowed. Whether it is under way.
async fn taken(id: &str, slskd: &Slskd, folder: &Path, user: &str, batch: &str) -> bool {
    let on = kept(|k| {
        let i = k.items.iter_mut().find(|i| i.id == id && i.state == State::Approved && i.batch.as_deref() == Some(batch))?;
        (i.state, i.percent, i.why) = (State::Fetching, 0.0, None);
        Some(())
    });
    if on.is_none() {
        tracing::info!(target: "soulseek", batch, "a batch taken as its item left the list: let go");
        let_go(slskd, folder, id, user, batch).await;
        wake_starts();
    }
    on.is_some()
}

/// The downloads followed now, by their items' ids: one follower each.
static FOLLOWED: Mutex<std::collections::BTreeSet<String>> = Mutex::new(std::collections::BTreeSet::new());

/// The downloads due to be followed that no follower follows yet, marked followed (`downloads`).
fn to_follow(items: &[Item], now: i64) -> Vec<String> {
    let mut followed = FOLLOWED.lock().unwrap_or_else(|e| e.into_inner());
    items.iter().filter(|i| i.state == State::Fetching && i.batch.is_some() && i.due <= now).filter(|i| followed.insert(i.id.clone())).map(|i| i.id.clone()).collect()
}

/// The downloads' worker (SK1): at each look every picked record that may start is asked for, and every download gets its own
/// follower; woken when a record is picked, a user's turn frees or room comes back, and once a minute. slskd not ready: what would
/// start or be followed now is held, saying why.
async fn downloads() {
    loop {
        match super::track_fetch::slskd_ready().await {
            Ok((slskd, folder)) => {
                let look = Look::read().await;
                start_downloads(&every, &slskd, &folder, &look).await;
                for id in kept(|k| to_follow(&k.items, now())) {
                    let (slskd, folder) = (slskd.clone(), folder.clone());
                    crate::run::spawn(async move {
                        follow_one(&id, &slskd, &folder).await;
                        FOLLOWED.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                    });
                }
            }
            Err(e) => not_ready(&every, &e),
        }
        let _ = tokio::time::timeout(Duration::from_secs(60), STARTS.notified()).await;
    }
}

/// slskd not ready (switched off, not set up): what of `within` would start or be followed now is held a while, saying why.
fn not_ready(within: Within, e: &anyhow::Error) {
    let (why, t) = (format!("{e:#}"), now());
    kept(|k| {
        for i in k.items.iter_mut().filter(|i| within(i) && i.due <= t && matches!(i.state, State::Approved | State::Fetching)) {
            hold(i, why.clone(), SLSKD_AGAIN);
        }
    });
}

/// The checks' worker (SK1.3): one record at a time, in the order its download ended, checked and then placed; woken when a download
/// ends, and once a minute.
async fn checks() {
    loop {
        if let Ok((slskd, folder)) = super::track_fetch::slskd_ready().await {
            let places = Places::of_server();
            let gate = |file: PathBuf, track: ReleaseTrack| async move {
                let (score, notes) = super::track_fetch::gate_track_and_notes(&file, &track).await?;
                note_heard(&file, notes);
                Ok(score)
            };
            for _ in 0..1_000 {
                if !check_next(&every, &slskd, &folder, &places, &gate).await {
                    break;
                }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(60), CHECKS.notified()).await;
    }
}

/// Why a record whose checked files went while it waited is fetched again (X4, TN.9): no user's failure.
const FILES_GONE: &str = "Its checked files were gone: fetched again";

/// One record checked and placed (SK1.3): a placing a stop cut after its rename taken as placed (Y10); checked files whose placing
/// failed placed again from them (X4); one held at its check whose files went meanwhile fetched again (X4, TN.9); else its downloaded
/// files gated one at a time, each on the one decode turn (`HEAVY`, the light rule), then placed. A file that fails sets its folder
/// aside and fails its user, the next folder after. One whose item left the list meanwhile (SK1.6) is checked no further, and placed
/// and set aside nowhere.
async fn check_one<G, Fut>(id: &str, slskd: &Slskd, folder: &Path, places: &Places, gate: &G)
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    let Some(it) = item(id).filter(|i| i.state == State::Checking) else { return };
    // A placing a stop cut after its rename (Y10): its album is in place, so it is taken as placed, never placed again; what it came
    // from goes after.
    if let Some(to) = it.placing.as_deref().filter(|to| renamed(Path::new(to), &it.record)) {
        tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), to, "a placing cut short after its rename taken as placed");
        let to = to.to_string();
        with_item(id, |i| (i.state, i.placed, i.placing, i.batch, i.checked, i.why, i.local_failures, i.place_failures) = (State::Placed, Some(to), None, None, None, None, 0, 0));
        for b in it.batch.iter().chain(&it.checked) {
            std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(b)).ok();
        }
        return room_again();
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
            return with_item(id, |i| (i.state, i.checked, i.due, i.why) = (State::Approved, None, 0, Some(FILES_GONE.into())));
        }
        return place_from(id, &it, &dir, &files, places).await;
    }
    let Some(f) = it.chosen.and_then(|c| it.folders.get(c)).cloned() else {
        return with_item(id, |i| (i.state, i.chosen, i.due) = (State::Waiting, None, 0));
    };
    let Some(batch) = it.batch.clone() else { return };
    let dir = folder.join(crate::slskd::FOLDER).join(&batch);
    // Its files gone while it was held at its check (X4, TN.9: its folder emptied while the server was down): fetched again from the
    // same folder under its tick, its transfers let go, no user failed. One just downloaded with none there is slskd's folder set
    // wrong (`check_while`).
    if it.why.is_some() && !f.files.is_empty() && files_under(&dir).is_empty() {
        tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), "its checked files were gone: fetched again");
        let_go(slskd, folder, id, &f.user, &batch).await;
        with_item(id, |i| (i.state, i.batch, i.percent, i.due, i.why) = (State::Approved, None, 0.0, 0, Some(FILES_GONE.into())));
        return wake_starts();
    }
    let release: Vec<ReleaseTrack> =
        judged_by(&it, &f).iter().map(|t| super::track_fetch::track_for(t.disc, t.position, &t.title, &it.record.artist, t.length_ms, t.recording.as_deref().unwrap_or_default())).collect();
    let still = || kept(|k| k.items.iter().any(|i| i.id == id && i.batch.as_deref() == Some(batch.as_str())));
    let checked = match check_while(&dir, &f, &release, gate, &still).await {
        Ok(Some(verdict)) => verdict,
        Ok(None) => {
            std::fs::remove_dir_all(&dir).ok();
            return tracing::info!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), "not checked further: it left the list");
        }
        // No verdict: held at its check, its files kept, until AcoustID's deadline where it asked to be left alone (TN.6), else a while.
        Err(e) => {
            tracing::warn!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), reason = %format!("{e:#}"), "held at its check");
            let due = again_at(&e, CHECK_AGAIN).unwrap_or_else(|| now() + CHECK_AGAIN);
            return with_item(id, |i| held_to(i, &e, due));
        }
    };
    // At its check its files are in: slskd's list decides nothing, and its transfers are let go.
    for t in slskd.transfers(&batch).await.unwrap_or_default() {
        slskd.drop_transfer(&f.user, &t.id).await;
    }
    match checked {
        Ok(files) => {
            // Its notes kept once its check passed (SK6.3): a placing tried again keeps them, and its arrival's card says them.
            let notes = notes_of(&files);
            with_item(id, |i| i.notes = notes);
            place_from(id, &it, &dir, &files, places).await
        }
        Err(why) => {
            notes_of(&files_under(&dir));
            let aside = set_aside(&dir, id, &f.user, &why, places.data.as_deref());
            // Gone either way (X4): set aside, or where it could not be, removed.
            std::fs::remove_dir_all(&dir).ok();
            with_item(id, |i| {
                if let Some(path) = aside {
                    i.set_aside.push(SetAside { user: f.user.clone(), path, why: why.clone() });
                }
                let n = why.len();
                on_failed(i, &f.user, format!("{n} of its files failed the check; set aside"));
            });
            wake_starts();
            room_again();
        }
    }
}

/// Between two reads of a download's transfers.
const FOLLOW_EVERY: Duration = Duration::from_millis(if cfg!(any(test, feature = "test-support")) { 10 } else { 3_000 });

/// What one read of a download's follower came to (`follow_step`).
enum Followed {
    /// Still going: read again.
    Going,
    /// The read failed, saying why (counted against `READS`).
    Unread(anyhow::Error),
    /// Its follower ends: the download ended, failed, was held, or its item left the list or the download.
    Over,
}

/// Whether item `id` is still downloading batch `batch` (SK1.6: a follower whose item left stops at its next read).
fn still_fetching(id: &str, batch: &str) -> bool {
    kept(|k| k.items.iter().any(|i| i.id == id && i.state == State::Fetching && i.batch.as_deref() == Some(batch)))
}

/// A download's transfers dropped in slskd (through its lane) and what came of them removed: its user failed it, slskd did, or its
/// item left the list. Where slskd could not be asked for them, the batch is kept to drop (`to_drop`), its user's turn taken till the
/// downloads' next look drops it (`drop_again`): a transfer still queued at its user may send yet.
async fn let_go(slskd: &Slskd, folder: &Path, id: &str, user: &str, batch: &str) {
    if !drop_batch(slskd, user, batch).await {
        tracing::info!(target: "soulseek", batch, "a batch let go could not be read: dropped at the next look");
        kept(|k| {
            if !k.to_drop.iter().any(|d| d.batch == batch) {
                k.to_drop.push(ToDrop { id: id.to_string(), user: user.to_string(), batch: batch.to_string() });
            }
        });
    }
    std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(batch)).ok();
}

/// A batch's transfers dropped in slskd, each through its lane: whether that is done (slskd answered for them, or knows no such batch).
async fn drop_batch(slskd: &Slskd, user: &str, batch: &str) -> bool {
    match slskd.transfers(batch).await {
        Ok(ts) => {
            for t in ts {
                slskd.drop_transfer(user, &t.id).await;
            }
            true
        }
        Err(e) => e.is::<crate::slskd::NotKnown>(),
    }
}

/// The batches let go and kept to drop that `within` takes (its item's, or one whose item left the list).
fn to_drop_of<'a>(k: &'a Kept, within: Within<'a>) -> impl Iterator<Item = &'a ToDrop> + 'a {
    k.to_drop.iter().filter(move |d| k.items.iter().find(|i| i.id == d.id).is_none_or(within))
}

/// The batches kept to drop (`let_go`) dropped now where slskd answers for them, what came of them removed; their users' turns free.
async fn drop_again(within: Within<'_>, slskd: &Slskd, folder: &Path) {
    for d in kept(|k| to_drop_of(k, within).cloned().collect::<Vec<_>>()) {
        if drop_batch(slskd, &d.user, &d.batch).await {
            std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(&d.batch)).ok();
            kept(|k| k.to_drop.retain(|x| x.batch != d.batch));
        }
    }
}

/// One read of a download (SK1): its percent kept, its own transfers' alone; every transfer done, it goes to its check, when it
/// ended kept (`ended_at`), its user's turn free. A user's failure (refused, offline, an error, queued too long, too slow) fails that
/// user for this record alone, the next folder after (`on_failed`); Cratarr's own side (slskd could not store a file) blames no one
/// and holds (Y9); slskd shut down, or a batch it forgot (a restart, OQ-C3 (a)) is fetched again from the same folder once, no one
/// blamed; forgotten twice in a row, the user is failed as ever. slskd busy (a 429) is read again, and no failed read.
async fn follow_step(id: &str, slskd: &Slskd, folder: &Path, followed_for: Duration) -> Followed {
    let Some(it) = item(id).filter(|i| i.state == State::Fetching) else { return Followed::Over };
    let (Some(batch), Some(user)) = (it.batch.clone(), chosen_user(&it).map(str::to_string)) else { return Followed::Over };
    let record = format!("{} - {}", it.record.artist, it.record.album);
    let read = slskd.transfers(&batch).await;
    if !still_fetching(id, &batch) {
        return Followed::Over;
    }
    let ts = match read {
        Ok(ts) if !ts.is_empty() => ts,
        Err(e) if e.is::<crate::slskd::Busy>() => return Followed::Going,
        Err(e) if e.is::<crate::slskd::NotKnown>() => {
            std::fs::remove_dir_all(folder.join(crate::slskd::FOLDER).join(&batch)).ok();
            let again = it.forgotten == 0;
            tracing::warn!(target: "soulseek", record = %record, again, "slskd does not know its batch (a restart)");
            with_item(id, |i| {
                if again {
                    (i.state, i.batch, i.percent, i.why, i.due) = (State::Approved, None, 0.0, None, 0);
                    i.forgotten += 1;
                    i.local_failures += 1;
                } else {
                    i.forgotten = 0;
                    on_failed(i, &user, format!("slskd could not be read: {e:#}"));
                }
            });
            wake_starts();
            return Followed::Over;
        }
        Ok(_) => return Followed::Unread(anyhow::anyhow!("slskd could not be read: no transfers")),
        Err(e) => return Followed::Unread(e.context("slskd could not be read")),
    };
    if let Some(bad) = ts.iter().find(|t| t.done() && !t.succeeded()) {
        let said = bad.exception.clone().unwrap_or_default();
        let file = crate::slskd::base_name(&bad.filename).to_string();
        let_go(slskd, folder, id, &user, &batch).await;
        if local_failure(bad) {
            tracing::warn!(target: "soulseek", record = %record, reason = %said, "a fetch failed on Cratarr's side; held");
            with_item(id, |i| {
                (i.state, i.batch, i.percent) = (State::Approved, None, 0.0);
                // Three in a row (Y9): held a day, not asked of its user every five minutes all day; Health says why.
                i.local_failures += 1;
                let wait = if i.local_failures >= LOCAL_AT_MOST { LOCAL_HOLD } else { SLSKD_AGAIN };
                hold(i, format!("slskd could not store {file}: {said}"), wait);
            });
        } else if said.contains("Application shut down") {
            // slskd itself stopped (a restart, what the NAS saw on 2026-10-17): no user's doing; the same folder fetched again.
            tracing::warn!(target: "soulseek", record = %record, "slskd shut down under a fetch: fetched again");
            with_item(id, |i| {
                (i.state, i.batch, i.percent, i.why, i.due) = (State::Approved, None, 0.0, None, 0);
                i.local_failures += 1;
            });
        } else {
            tracing::info!(target: "soulseek", user = %user, reason = %said, "a user failed; the next folder");
            with_item(id, |i| {
                on_failed(i, &user, format!("{file} was not sent{}", if said.is_empty() { String::new() } else { format!(": {said}") }));
            });
        }
        wake_starts();
        return Followed::Over;
    }
    if ts.iter().all(|t| t.done()) {
        // Its files in: a row of failures on Cratarr's side ends (Y9), and its user's turn is free.
        let ended = now_ms();
        with_item(id, |i| (i.state, i.percent, i.local_failures, i.forgotten, i.ended_at, i.why) = (State::Checking, 100.0, 0, 0, ended, None));
        wake_starts();
        CHECKS.notify_one();
        return Followed::Over;
    }
    let percent = ts.iter().map(|t| t.percent_complete).sum::<f64>() / ts.len() as f64;
    let gave_up = if percent <= 0.0 && followed_for > QUEUED_FOR {
        Some(format!("It stayed in the user's queue over {} minutes", QUEUED_FOR.as_secs() / 60))
    } else if followed_for > FETCH_AT_MOST {
        Some(format!("It took over {} hours", FETCH_AT_MOST.as_secs() / 3600))
    } else {
        None
    };
    if let Some(why) = gave_up {
        let_go(slskd, folder, id, &user, &batch).await;
        with_item(id, |i| {
            on_failed(i, &user, why);
        });
        wake_starts();
        return Followed::Over;
    }
    with_item(id, |i| i.percent = (percent * 10.0).round() / 10.0);
    Followed::Going
}

/// A download followed until it ends (SK1): one read every `FOLLOW_EVERY`; `READS` failed reads in a row and slskd away hold it,
/// its batch kept (slskd may well finish it), followed again when due; failed otherwise ("no transfers"), its user's.
async fn follow_one(id: &str, slskd: &Slskd, folder: &Path) {
    let since = std::time::Instant::now();
    let mut unread = 0;
    loop {
        match follow_step(id, slskd, folder, since.elapsed()).await {
            Followed::Over => return,
            Followed::Going => {
                unread = 0;
                tokio::time::sleep(FOLLOW_EVERY).await;
            }
            Followed::Unread(e) => {
                unread += 1;
                if unread < READS {
                    tokio::time::sleep(READ_AGAIN).await;
                    continue;
                }
                let Some(it) = item(id).filter(|i| i.state == State::Fetching) else { return };
                let (Some(batch), Some(user)) = (it.batch.clone(), chosen_user(&it).map(str::to_string)) else { return };
                // slskd away: its batch stays, for slskd may well finish it, and it is followed again when due (TN.6).
                if let Some(due) = again_at(&e, SLSKD_AGAIN) {
                    tracing::warn!(target: "soulseek", record = %format!("{} - {}", it.record.artist, it.record.album), reason = %format!("{e:#}"), "held for slskd");
                    return with_item(id, |i| held_to(i, &e, due));
                }
                let_go(slskd, folder, id, &user, &batch).await;
                with_item(id, |i| {
                    on_failed(i, &user, format!("{e:#}"));
                });
                wake_starts();
                return;
            }
        }
    }
}

/// Notes Soulseek's gate heard in a file that passed (SK6.3), by its path, till its check takes them (`check_one`).
static HEARD: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());

/// What Soulseek's gate heard in `file`, kept for its check.
pub(crate) fn note_heard(file: &Path, notes: Option<String>) {
    if let Some(n) = notes {
        HEARD.lock().unwrap_or_else(|e| e.into_inner()).insert(file.to_path_buf(), n);
    }
}

/// The notes heard in these files, by name, taken.
fn notes_of(files: &[PathBuf]) -> BTreeMap<String, String> {
    let mut heard = HEARD.lock().unwrap_or_else(|e| e.into_inner());
    files.iter().filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), heard.remove(p)?))).collect()
}

/// The checks' worker woken: a download ended.
static CHECKS: tokio::sync::Notify = tokio::sync::Notify::const_new();
/// The downloads' worker woken: a record picked, a user's turn free, room again. Kept till it looks, unlike `WAKE`.
static STARTS: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// The downloads' worker woken for its next look.
fn wake_starts() {
    STARTS.notify_one();
}

/// Milliseconds since the epoch: when a download ended (`ended_at`).
fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// The next record of `within` to check at `now` (SK1.3): one downloaded (or a check or a placing a stop cut, or checked files whose
/// placing failed), due, in the order its download ended, one whose end is not known (a stop, 0.16's) first; ended together, the
/// list's order. Pure.
fn next_check(items: &[Item], within: Within, now: i64) -> Option<String> {
    items
        .iter()
        .enumerate()
        .filter(|(_, i)| within(i) && i.state == State::Checking && i.due <= now && (i.batch.is_some() || i.checked.is_some() || i.placing.is_some()))
        .min_by_key(|(n, i)| (i.ended_at, *n))
        .map(|(_, i)| i.id.clone())
}

/// One look of the checks' worker (SK1.3): the next downloaded record of `within` checked, then placed; whether there was one. One
/// at a time, whatever downloads meanwhile; the next check begins once this one's placing ended.
pub(crate) async fn check_next<G, Fut>(within: Within<'_>, slskd: &Slskd, folder: &Path, places: &Places, gate: &G) -> bool
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    let Some(id) = kept(|k| next_check(&k.items, within, now())) else { return false };
    check_one(&id, slskd, folder, places, gate).await;
    true
}

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

/// What the check of a fetched folder found: its files in the release's order, or why each that failed did (by its name).
type Verdict = std::result::Result<Vec<PathBuf>, BTreeMap<String, String>>;

/// Every file of the fetched folder gated against its track (by `gate`), one at a time, each on the one decode turn (`HEAVY`, taken
/// for the file and let go after: the light rule): the verdict, or, where AcoustID could not be asked and no file has failed so far,
/// none: why (`NotAsked`, AcoustID's deadline with it where it asked to be left alone), as nothing is known yet. `still` is asked
/// before each file and after the last: where it says no (its item left the list, SK1.6), the check ends there, with no verdict
/// (`None`).
async fn check_while<G, Fut>(dir: &Path, f: &Folder, release: &[ReleaseTrack], gate: &G, still: &(dyn Fn() -> bool + Sync)) -> Result<Option<Verdict>>
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    let on_disk = files_under(dir);
    // None of its files there at all (X4): slskd's folder is set wrong, or gone. No user's failure: held, as with AcoustID away.
    if on_disk.is_empty() && !f.files.is_empty() {
        bail!("Its files are not in slskd's folder: check slskd's folder in Settings › Server");
    }
    let mut out = Vec::new();
    let mut why = BTreeMap::new();
    for (offered, track) in f.files.iter().zip(release) {
        if !still() {
            return Ok(None);
        }
        let name = crate::slskd::base_name(&offered.filename).to_string();
        // One disc's file in its disc folder (X3): two discs may name a file alike.
        let disc = disc_of(&offered.filename);
        let at = |p: &&PathBuf| p.file_name().is_some_and(|n| n.to_string_lossy() == name) && disc.is_none_or(|d| p.parent().and_then(Path::file_name).is_some_and(|n| n.to_string_lossy() == d));
        let Some(path) = on_disk.iter().find(at).cloned() else {
            why.insert(name, "It is not in slskd's folder".into());
            continue;
        };
        let turn = super::heavy_base::HEAVY.acquire().await.expect("the turn is never closed");
        let heard = gate(path.clone(), track.clone()).await;
        drop(turn);
        match heard {
            Ok(_) => out.push(path),
            // None after it is asked about: AcoustID would not answer them either, and each would be decoded whole to find
            // out. A file failed already settles the folder.
            Err(e) if e.is::<NotAsked>() && why.is_empty() => return Err(e),
            Err(e) if e.is::<NotAsked>() => break,
            Err(e) => {
                why.insert(name, format!("{e:#}"));
            }
        }
    }
    if !still() {
        return Ok(None);
    }
    Ok(Some(if why.is_empty() { Ok(out) } else { Err(why) }))
}

/// [`check_while`] of a folder whose item stays.
#[cfg(test)]
async fn check<G, Fut>(dir: &Path, f: &Folder, release: &[ReleaseTrack], gate: &G) -> Result<Verdict>
where
    G: Fn(PathBuf, ReleaseTrack) -> Fut,
    Fut: std::future::Future<Output = Result<f32>>,
{
    Ok(check_while(dir, f, release, gate, &|| true).await?.expect("it stays"))
}

/// The files anywhere under `dir`.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
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
async fn place_from(id: &str, it: &Item, dir: &Path, files: &[PathBuf], places: &Places) {
    let lib = places.library.clone().map_err(|e| anyhow::anyhow!(e));
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
    // Its files gone from slskd's folder, or kept: room for the next start either way (SK1.4).
    room_again();
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
    let q = crate::run::spawn_blocking(move || cat.requested(&[record])).await.ok()?.ok()?.remove(&record)?;
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
            Ok(Some(b)) => match crate::run::spawn_blocking(move || cover_jpg(b)).await.ok().flatten() {
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
fn set_aside(dir: &Path, id: &str, user: &str, why: &BTreeMap<String, String>, data: Option<&Path>) -> Option<String> {
    let data = data?.to_path_buf();
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

#[cfg(any(test, feature = "test-support"))]
pub mod tests {
    // Shared with the tests of the crates above (test-support): idle where no test calls it.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
    use super::*;

    /// Whose turn it is (round 1.3's review: M3's room and space arm, L3, L4).
    mod turns;
    /// The upgrade from 0.16.3's queue (SK4, SK5.6).
    mod upgrade;

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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
        };
        kept(|k| k.items.push(i));
    }

    /// slskd at a local address, answering as `app` does.
    async fn slskd_at(app: axum::Router) -> Slskd {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
        NotAsked("AcoustID could not be reached".into(), None).into()
    }

    #[tokio::test]
    async fn a_check_acoustid_cannot_answer_holds_the_record_and_holds_it_against_nobody() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        assert_eq!(check(&dir, &f, &release, &second).await.unwrap_err().to_string(), "AcoustID could not be reached");
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let (id, batch) = ("ss-gone", "b-gone");
        listed(id, State::Fetching, Some(batch));
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-gone-{}", std::process::id()));
        // Changed by round 1.3's plan (OQ-C3 (a), SK1.7): the first time slskd forgets a batch (a restart), the same folder is
        // fetched again, under a new batch, with no user failed; it is never held for nothing.
        let forgot = slskd_at(axum::Router::new()).await;
        fetch_from(id, &forgot, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.clone(), i.chosen, i.failed.clone(), i.local_failures), (State::Approved, None, Some(0), Vec::<String>::new(), 1), "the same folder again, no user failed");
        assert!(i.due <= now(), "not held: fetched again at the next look");
        // The second time in a row: its user failed as ever, the next folder.
        with_item(id, |i| (i.state, i.batch) = (State::Fetching, Some("b-gone-again".into())));
        fetch_from(id, &forgot, &d, never_gated).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.batch.clone(), i.failed.clone()), (State::Approved, None, vec!["a".to_string()]), "the next folder: holding it would wait for ever");
    }

    #[tokio::test]
    async fn a_search_slskd_could_not_make_is_no_miss() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let id = "ss-search";
        listed(id, State::Searching, None);
        with_item(id, |i| i.misses = 3);
        search_through(id, &slskd_gone().await).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.misses), (State::Waiting, 3), "the ladder's waits are for records not found");
        assert!(i.due <= now() + ROUND_AT_LEAST, "searched again soon");
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("slskd could not be reached")), "{:?}", i.why);
    }

    /// Changed by round 1.3's plan (OQ-SK2 (a), the owner: "Out of the list" also stops a transfer under way): was
    /// `a_record_held_at_its_check_can_leave_the_list_and_one_at_work_cannot`. A record held at its check leaves, its batch with it,
    /// as before; so does one being checked, and one whose files are coming: its transfers dropped in slskd (through its lane) and
    /// what came of them removed. Nobody blamed; the record stays off the list.
    #[tokio::test]
    async fn a_record_at_work_can_leave_the_list_and_its_transfers_stop() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        listed("ss-work", State::Checking, Some("b-work"));
        with_item("ss-work", |i| hold(i, "AcoustID is not set up".into(), CHECK_AGAIN));
        assert_eq!(dropped("ss-work").and_then(|i| i.batch).as_deref(), Some("b-work"), "its batch, for its folder to go");
        assert!(item("ss-work").is_none());
        listed("ss-checked-now", State::Checking, Some("b-checked-now"));
        assert_eq!(dropped("ss-checked-now").and_then(|i| i.batch).as_deref(), Some("b-checked-now"), "being checked: it leaves too");
        let slskd = Scripted::new().hold_all(30.0);
        let d = data_folder("oq-sk2");
        picked("ss-coming", &["peer-a"]);
        assert!(start_it("ss-coming", &slskd.client(), &d).await);
        let batch = batch_of("ss-coming");
        std::fs::create_dir_all(d.join(crate::slskd::FOLDER).join(&batch)).unwrap();
        std::fs::write(d.join(crate::slskd::FOLDER).join(&batch).join("01 One.flac.part"), b"x").unwrap();
        let it = dropped("ss-coming").expect("its files coming: it leaves too");
        cancel_fetch(&slskd.client(), &d, &it).await;
        assert!(item("ss-coming").is_none() && slskd.deleted(&batch), "its transfers dropped");
        assert!(!d.join(crate::slskd::FOLDER).join(&batch).exists(), "its partial files removed");
        assert!(it.failed.is_empty() && slskd.violations().is_empty());
        std::fs::remove_dir_all(&d).ok();
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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

    // ---- SK5 (the owner, 2026-11-19): clipping, a pop at the very end and clicks are notes, not refusals. Glass Harbour's "Tide
    // Tables" (2012), record 11, from peer-a's folder, peer-b's and peer-c's after it.

    const TIDE_TABLES: [&str; 3] = ["01 Slack Water.flac", "02 Neap.flac", "03 Spring Tide.flac"];

    /// "Tide Tables" on the list under `id`, a folder of each of `users`' holding its three tracks, the first ticked.
    fn tide_tables(id: &str, state: State, batch: Option<&str>, users: &[&str]) {
        let lengths = [250, 240, 260];
        let folder = |user: &&str| Folder {
            user: user.to_string(),
            dir: "Tide Tables".into(),
            format: "flac".into(),
            lossless: true,
            free_slot: true,
            queue: 0,
            speed: 1,
            extra: 0,
            files: TIDE_TABLES.iter().zip(lengths).map(|(f, secs)| file(&format!(r"Tide Tables\{f}"), secs)).collect(),
            unchecked: None,
        };
        let i = Item {
            id: id.into(),
            record: Record { record: 11, artist: "Glass Harbour".into(), album: "Tide Tables".into(), year: Some(2012), request: 11 },
            state,
            tracks: vec![track(1, "Slack Water", 250), track(2, "Neap", 240), track(3, "Spring Tide", 260)],
            untimed: Vec::new(),
            folders: users.iter().map(folder).collect(),
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
        };
        kept(|k| k.items.push(i));
    }

    /// "Tide Tables" downloaded into `at`, each file's bytes its own name.
    fn tide_tables_into(at: &Path) {
        std::fs::create_dir_all(at).unwrap();
        for f in TIDE_TABLES {
            std::fs::write(at.join(f), f.as_bytes()).unwrap();
        }
    }

    /// slskd's completed-downloads folder, `batch`'s "Tide Tables" in it.
    fn tide_tables_fetched(name: &str, batch: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-{name}-{}", std::process::id()));
        tide_tables_into(&d.join(crate::slskd::FOLDER).join(batch));
        d
    }

    /// The release a fetch of `it` checks its folder's files against, as `fetch_from` makes it.
    fn release_to_check(it: &Item) -> Vec<ReleaseTrack> {
        it.tracks.iter().map(|t| super::super::track_fetch::track_for(t.disc, t.position, &t.title, &it.record.artist, t.length_ms, t.recording.as_deref().unwrap_or_default())).collect()
    }

    /// What one file's sound was heard to have (SK5's heard gate): its findings, frames that would not decode, and how much longer
    /// than its recording it is.
    #[derive(Default, Clone)]
    struct Sound {
        flaws: crate::analysis::flaws::Findings,
        damaged: u32,
        longer: f64,
    }

    /// The gate, heard (SK5): what each file's sound was heard to have, by its name, judged by Soulseek's own gate
    /// (`track_fetch::judge_track`), AcoustID knowing it as its track's recording unless told another; each file AcoustID is asked
    /// about noted. A file told to be undecodable goes through the real gate, which decodes it.
    #[derive(Default)]
    struct Heard {
        sounds: HashMap<String, Sound>,
        other: Vec<String>,
        undecodable: Vec<String>,
        asked: Mutex<Vec<String>>,
    }

    impl Heard {
        fn sound(mut self, name: &str, f: impl FnOnce(&mut Sound)) -> Self {
            f(self.sounds.entry(name.into()).or_default());
            self
        }

        fn clips(self, name: &str, n: u32, first: f64) -> Self {
            self.sound(name, |s| (s.flaws.clips, s.flaws.first_clip) = (n, Some(first)))
        }

        fn clips_everywhere(self, n: u32, first: f64) -> Self {
            TIDE_TABLES.iter().fold(self, |h, f| h.clips(f, n, first))
        }

        fn click(self, name: &str, at: f64) -> Self {
            self.sound(name, |s| s.flaws.clicks.push(at))
        }

        fn pop(self, name: &str) -> Self {
            self.sound(name, |s| s.flaws.pop_at_end = true)
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        async fn gate(&self, file: PathBuf, track: ReleaseTrack) -> Result<f32> {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            if self.undecodable.contains(&name) {
                return super::super::track_fetch::gate_track(&file, &track).await;
            }
            let s = self.sounds.get(&name).cloned().unwrap_or_default();
            let print = crate::acoustid::Print { fingerprint: String::new(), duration_secs: track.length_ms.unwrap_or_default() as f64 / 1000.0 + s.longer, damaged: s.damaged, flaws: s.flaws };
            let recording = if self.other.contains(&name) { "another recording".to_string() } else { track.recording.clone() };
            let lookup = |_: crate::acoustid::Print| {
                self.asked.lock().unwrap().push(name.clone());
                std::future::ready(Ok(vec![crate::acoustid::Found { score: 0.97, recordings: vec![crate::acoustid::Recording { id: recording }] }]))
            };
            super::super::track_fetch::judge_track(print, &track, lookup).await
        }
    }

    /// SK5.1 (OQ-SK4): a folder whose tracks clip, click and pop passes Soulseek's check: AcoustID asked about every file, no user
    /// failed, nothing set aside, and on to its placing with its files as they were fetched. A unit test has no library, so its
    /// placing stops there, its checked files kept (as `a_failed_placing_keeps_its_checked_files_and_lets_its_batch_go`); the
    /// end-to-end SK6.3 places it.
    #[tokio::test]
    async fn clipping_a_pop_and_clicks_are_notes_not_refusals() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let (id, batch) = ("ss-n1", "b-n1");
        tide_tables(id, State::Fetching, Some(batch), &["peer-a", "peer-b"]);
        let d = tide_tables_fetched("n1", batch);
        let dir = d.join(crate::slskd::FOLDER).join(batch);
        let heard = &Heard::default().clips("02 Neap.flac", 5, 106.0).pop("03 Spring Tide.flac").click("03 Spring Tide.flac", 130.0);
        fetch_from(id, &slskd_at(all_in()).await, &d, |file, track| heard.gate(file, track)).await;
        assert_eq!(heard.asked(), TIDE_TABLES, "AcoustID asked about every file, the clipped one and the popped one too");
        let i = item(id).unwrap();
        assert!(i.failed.is_empty() && i.set_aside.is_empty(), "no user failed, nothing set aside: {:?}", i.why);
        assert_eq!((i.state, i.chosen, i.folders.iter().map(|f| f.user.as_str()).collect::<Vec<_>>()), (State::Checking, Some(0), vec!["peer-a", "peer-b"]));
        assert_eq!(i.checked.as_deref(), Some(batch), "passed: on to its placing, which only the test's missing library stops");
        assert!(i.why.as_deref().is_some_and(|w| w.starts_with("It could not be placed")), "{:?}", i.why);
        for f in TIDE_TABLES {
            assert_eq!(std::fs::read(dir.join(f)).unwrap(), f.as_bytes(), "{f} as it was fetched");
        }
        // The review of part 5, H1: taken up again with an Incoming library, it is placed, its files as fetched, nothing set aside.
        with_item(id, |i| i.due = 0);
        check_one(id, &slskd_gone().await, &d, &places_in(&d), &|file, track| heard.gate(file, track)).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.failed.len(), i.set_aside.len()), (State::Placed, 0, 0));
        for f in TIDE_TABLES {
            assert_eq!(std::fs::read(d.join("incoming").join("Glass Harbour").join("Tide Tables (2012)").join(f)).unwrap(), f.as_bytes(), "{f} placed as it was fetched");
        }
        assert!(!holds_anything(&d.join("data").join(SET_ASIDE)), "nothing set aside");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK5.2: a real fault still sets its folder aside and fails its user for the next folder, the file's why the fault's own words,
    /// with clipping besides or not: highs that stop at 16 kHz in a FLAC or a 320 kbit/s MP3, AcoustID's other recording, 30 s
    /// longer than the recording, 2 frames that would not decode, no decode at all, not in slskd's folder.
    #[tokio::test]
    async fn real_faults_still_set_a_folder_aside() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        use crate::analysis::flaws::Sold;
        let neap = "02 Neap.flac";
        let walled = |sold: Sold| move |s: &mut Sound| (s.flaws.sold, s.flaws.cutoff_hz) = (sold, Some(16_000.0));
        let faults: [(&str, Box<dyn Fn(Heard) -> Heard>, String); 7] = [
            ("a FLAC walled", Box::new(move |h: Heard| h.sound(neap, walled(Sold::Lossless))), "Not lossless: it stops at 16 kHz".into()),
            ("a 320 walled", Box::new(move |h: Heard| h.sound(neap, walled(Sold::Mp3 { kbps: 320 }))), "Re-encoded from a lower bitrate: it stops at 16 kHz".into()),
            ("another recording", Box::new(move |mut h: Heard| (h.other.push(neap.into()), h).1), "AcoustID does not know this file as that recording".into()),
            ("30 s longer", Box::new(move |h: Heard| h.sound(neap, |s| s.longer = 30.0)), "It is 30 s longer than the recording".into()),
            ("2 frames", Box::new(move |h: Heard| h.sound(neap, |s| s.damaged = 2)), "2 frames would not decode".into()),
            // Its words the decoder's own, read from the file as it lies (below).
            ("no decode", Box::new(move |mut h: Heard| (h.undecodable.push(neap.into()), h).1), String::new()),
            ("missing", Box::new(|h: Heard| h), "It is not in slskd's folder".into()),
        ];
        for (n, (fault, heard_so, words)) in faults.iter().enumerate() {
            for clipped in [false, true] {
                let base = if clipped { Heard::default().clips_everywhere(5, 106.0) } else { Heard::default() };
                let heard = &heard_so(base);
                let (id, batch) = (format!("ss-sk52-{n}-{clipped}"), format!("b-sk52-{n}-{clipped}"));
                tide_tables(&id, State::Fetching, Some(&batch), &["peer-a", "peer-b"]);
                let d = tide_tables_fetched(&format!("sk52-{n}-{clipped}"), &batch);
                let dir = d.join(crate::slskd::FOLDER).join(&batch);
                if *fault == "missing" {
                    std::fs::remove_file(dir.join(neap)).unwrap();
                }
                let words = if *fault == "no decode" { format!("{:#}", crate::acoustid::print(&dir.join(neap)).unwrap_err()) } else { words.clone() };
                let gate = move |file, track| heard.gate(file, track);
                // The check's verdict, which `set_aside` writes beside the folder (why.json): the fault alone, in its own words.
                let it = item(&id).unwrap();
                let why = check(&dir, &it.folders[0], &release_to_check(&it), &gate).await.unwrap().unwrap_err();
                assert_eq!(why.into_iter().collect::<Vec<_>>(), [(neap.to_string(), words.clone())], "{fault}, clipped besides: {clipped}");
                // Through the fetch: its user failed, the next folder ticked, the folder gone from slskd's (set aside where there is a
                // data folder, which a unit test has none of).
                // The review of part 5, H1: through the workers, with a data folder, the folder set aside on disk.
                let slskd = slskd_at(all_in()).await;
                follow_one(&id, &slskd, &d).await;
                check_one(&id, &slskd, &d, &places_in(&d), &gate).await;
                let i = item(&id).unwrap();
                assert_eq!((i.state, i.failed.clone(), i.folders[0].user.as_str(), i.chosen, i.batch.clone()), (State::Approved, vec!["peer-a".to_string()], "peer-b", Some(0), None), "{fault}, clipped besides: {clipped}");
                assert_eq!(i.why.as_deref(), Some("1 of its files failed the check; set aside"), "{fault}, clipped besides: {clipped}");
                assert!(!dir.exists(), "{fault}, clipped besides: {clipped}");
                assert_eq!(i.set_aside.len(), 1, "{fault}, clipped besides: {clipped}");
                let aside = Path::new(&i.set_aside[0].path);
                let kept_why: BTreeMap<String, String> = serde_json::from_slice(&std::fs::read(aside.join("why.json")).unwrap()).unwrap();
                assert_eq!(kept_why.into_iter().collect::<Vec<_>>(), [(neap.to_string(), words.clone())], "{fault}: why.json beside the folder");
                assert_eq!(aside.join(neap).exists(), *fault != "missing", "{fault}: the folder's files kept aside");
                std::fs::remove_dir_all(&d).ok();
            }
        }
    }

    /// slskd taking every batch (SK5.4): the user each POST names noted, the batch's files written into slskd's folder under `d` as
    /// it is taken, and every batch read back done.
    fn taking_batches(d: &Path, posts: Arc<Mutex<Vec<String>>>) -> axum::Router {
        let d = d.to_path_buf();
        all_in().route(
            "/api/v0/transfers/downloads/batches",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                posts.lock().unwrap().push(body["username"].as_str().unwrap_or_default().to_string());
                tide_tables_into(&d.join(crate::slskd::FOLDER).join(body["id"].as_str().unwrap_or_default()));
                std::future::ready(axum::Json(serde_json::json!({ "batch": {}, "failures": [] })))
            }),
        )
    }

    /// SK5.4 (kind 6): a record whose every copy clips at the same moments (its master's) is placed from its first folder: no second
    /// user's folder fetched, nobody failed, never back to found for a pick. Ticked here as today; Part 3's own pick (SK3.1) starts it
    /// once built, and its placing needs a library a unit test has none of, so it stops at its placing, its files kept.
    #[tokio::test]
    async fn a_clipping_master_is_placed_from_its_first_certain_folder() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let id = "ss-m";
        // The review of part 5, H1: three exact folders with free slots, fresh; Cratarr picks the first itself (Part 3).
        tide_tables(id, State::Found, None, &["peer-a", "peer-b", "peer-c"]);
        with_item(id, |i| (i.chosen, i.found_at) = (None, now()));
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-sk54-{}", std::process::id()));
        let posts = Arc::new(Mutex::new(Vec::new()));
        let heard = &Heard::default().clips_everywhere(5, 106.0);
        let slskd = slskd_at(taking_batches(&d, posts.clone())).await;
        start_downloads(&|i: &Item| i.id == id, &slskd, &d, &Look::roomy()).await;
        assert!(item(id).is_some_and(|i| i.auto && i.chosen == Some(0)), "Cratarr's own pick: no administrator's asked for");
        fetch_from(id, &slskd, &d, |file, track| heard.gate(file, track)).await;
        assert_eq!(*posts.lock().unwrap(), ["peer-a"], "the first folder fetched");
        assert_eq!(heard.asked(), TIDE_TABLES, "every file passed to AcoustID");
        let i = item(id).unwrap();
        let batch = i.checked.clone().expect("passed: on to its placing, its files kept");
        assert!(i.failed.is_empty() && i.set_aside.is_empty(), "{:?}", i.why);
        assert_eq!((i.state, i.chosen, i.folders.len()), (State::Checking, Some(0), 3));
        // Taken up again when due: placed from the same files, nothing fetched again, no other folder.
        with_item(id, |i| i.due = 0);
        fetch_from(id, &slskd, &d, |file, track| heard.gate(file, track)).await;
        assert_eq!(*posts.lock().unwrap(), ["peer-a"], "no second user's folder fetched");
        assert_eq!(heard.asked().len(), 3, "nor checked again");
        let i = item(id).unwrap();
        assert_eq!((i.state, i.checked.as_deref(), i.failed.len()), (State::Checking, Some(batch.as_str()), 0), "never back to found");
        for f in TIDE_TABLES {
            assert_eq!(std::fs::read(d.join(crate::slskd::FOLDER).join(&batch).join(f)).unwrap(), f.as_bytes(), "{f} as it was fetched");
        }
        // With an Incoming library: placed from the first folder, Cratarr's pick, never back to found.
        with_item(id, |i| i.due = 0);
        check_one(id, &slskd, &d, &places_in(&d), &|file, track| heard.gate(file, track)).await;
        let i = item(id).unwrap();
        assert_eq!((i.state, i.auto, i.failed.len()), (State::Placed, true, 0));
        assert_eq!(*posts.lock().unwrap(), ["peer-a"], "no second user's folder fetched");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4 (D17): a request whose record was retired follows the record its placed album joined or made; one on a live record stays.
    #[test]
    fn a_request_on_a_retired_record_follows_its_placed_album() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let f = super::super::health_requests::soulseek_cant_place(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't place albums", i.why.as_deref(), 1));
        assert!(cant_place_in(std::slice::from_ref(&i), i.due).is_empty(), "its day over: no more");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y12: a placing that comes through ends a row of failed ones (here one a stop cut after its rename, taken as placed).
    #[tokio::test]
    async fn a_placing_that_comes_through_ends_the_row_of_failed_placings() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let f = super::super::health_requests::soulseek_cant_store(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't store files", i.why.as_deref(), 1));
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y9: three fetches in a row that slskd could not store hold the record a day, not five minutes, and Health says Soulseek can't
    /// store files, with why; no user is failed.
    #[tokio::test]
    async fn three_local_failures_hold_the_record_a_day() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let f = super::super::health_requests::soulseek_cant_store(&held).expect("Health says so");
        assert_eq!((f.title.as_str(), f.detail.as_deref(), f.items.len()), ("Soulseek can't store files", i.why.as_deref(), 1));
        assert!(cant_store_in(std::slice::from_ref(&i), i.due).is_empty(), "its day over: no more");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y9: a fetch that comes through ends a row of failures on Cratarr's side: the next one counts from one, and holds a while only.
    #[tokio::test]
    async fn a_fetch_that_comes_through_ends_the_row_of_local_failures() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let held = check(&d.join("no-such-batch"), &f, &release, &never_gated).await.unwrap_err().to_string();
        assert!(held.starts_with("Its files are not in slskd's folder"), "{held}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// X4: a placing that fails keeps the checked files where they are and lets the batch go (no user failed, nothing fetched
    /// again); the record is placed again from them later.
    #[tokio::test]
    async fn a_failed_placing_keeps_its_checked_files_and_lets_its_batch_go() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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

    /// PAIR01.1 through the caller (the first failing test): a folder every track pairs with is found, each track its file in the
    /// list's order, in either listing order. Red first: today's greedy gives track 1 the first listed of two at 1 000 and leaves
    /// track 2 3 000 off (the twin: 4 000), so no folder.
    #[test]
    fn a_crossing_folder_is_found() {
        let q = |first: (&str, i64), second: (&str, i64)| {
            answer("peer-q", true, vec![file(&format!(r"q\Kite Theory\Quiet Hours\{}", first.0), first.1), file(&format!(r"q\Kite Theory\Quiet Hours\{}", second.0), second.1)])
        };
        for (list, files) in [
            // The crossing case: 181 000 and 179 000 against 180 s (listed first) and 182 s.
            ([track(1, "Untitled", 181), track(2, "Untitled", 179)], [("01 Untitled.flac", 180), ("02 Untitled.flac", 182)]),
            // Its twin, with no tie: 180 000 and 178 000 against 179 s and 182 s.
            ([track(1, "Untitled", 180), track(2, "Untitled", 178)], [("01 Untitled.flac", 179), ("02 Untitled.flac", 182)]),
        ] {
            for answers in [vec![q(files[0], files[1])], vec![q(files[1], files[0])]] {
                let found = folders_of(&answers, &list, &[]);
                assert_eq!(found.len(), 1, "a folder every track pairs with is found: {list:?}");
                let names: Vec<&str> = found[0].files.iter().map(|f| crate::slskd::base_name(&f.filename)).collect();
                assert_eq!(names, ["02 Untitled.flac", "01 Untitled.flac"], "each track its file, in the list's order");
                assert_eq!(found[0].extra, 0);
            }
        }
    }

    /// PAIR01.5 (c) through the caller: a peer's impossible length pairs nothing by length, never panics nor wraps. Red first:
    /// `secs * 1000` panics in a test build for `i64::MAX` and 2^61 + 250 (a release build wraps the second to Pier's 250 000).
    #[test]
    fn a_peers_impossible_length_pairs_nothing() {
        let pier = [track(1, "Pier", 250)];
        let wraps = 2_305_843_009_213_694_202_i64;
        assert_eq!(wraps.wrapping_mul(1_000), 250_000);
        for secs in [-5, 0, i64::MAX, wraps] {
            let found = folders_of(&[answer("peer-p", true, vec![file(r"p\Glass Harbour\Sea Wall\01 Pier.flac", secs)])], &pier, &[]);
            // Never found "within the slack": no folder, or one paired by title alone whose length reads as unknown, so never certain
            // (G2).
            if let Some(f) = found.first() {
                assert!(f.files.iter().all(|o| o.length.is_none()), "{secs} s: paired by title alone, its length unknown: {f:?}");
            }
        }
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let mut k = Kept::default();
        take_requests(&mut k, &[asked(1, None), asked(2, None), asked(3, None)], &[of(101), of(102), of(103)], &[5_000, 5_000, 90_000]);
        let groups: HashMap<i64, String> = [(1, "g1".to_string()), (2, "g2".to_string()), (3, "g3".to_string())].into();
        let progress: HashMap<String, Progress> = [("g1".to_string(), Progress::Stuck { said: None }), ("g2".to_string(), Progress::Fetching { done: 0.4 }), ("g3".to_string(), Progress::Stuck { said: None })].into();
        bring_forward(&mut k.items, &groups, &[3], &progress, 1_000);
        assert_eq!(k.items.iter().map(|i| i.due).collect::<Vec<_>>(), [1_000, 5_000, 90_000]);
    }

    /// X1: the first arrival stops the other: Soulseek's item of a record that arrived otherwise goes, a fetch under way with it
    /// (handed back to be cancelled in slskd); one placed is that arrival, and stays (Y2).
    #[test]
    fn the_first_arrival_cancels_soulseeks_fetch() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        place_from(id, &held, &dir, &files_under(&dir), &Places::of_server()).await;
        assert!(item(id).is_none(), "nothing written back");
        assert!(!dir.exists(), "placed nowhere, its files gone with it");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Y2: an item whose request left (cancelled, declined, rejected), no other approved request of its record there, goes unless it
    /// is placed: one held at its check, or keeping the checked files of a placing that failed, is handed back to be cancelled, so it
    /// is never placed for nobody.
    #[test]
    fn a_held_item_of_a_cancelled_request_is_not_placed() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let words = words_in(&k.items, &[(2, None)]);
        assert_eq!(words.get(&2), Some(&Word::Waiting { why: None }));
        let state = request_state(&asked(2, None), Holding::None, None, words.get(&2));
        assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "not fetching 100%: {state:?}");
    }

    /// V1: an item whose request left goes to another approved request of its record (X2) only while its copy may be there: a placed
    /// one no pass has read yet does (its album may be on its way into the catalogue); one whose folder was read, its record holding no
    /// copy, its last one deleted from Incoming (U7), never does, and that request is searched for.
    #[test]
    fn a_placed_item_is_never_handed_to_a_request_whose_record_it_no_longer_holds() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
        // Gone, its files hold the record no more (A3.1: only a present copy is had): its last copy went from Incoming, its item let go
        // at once; U7's, in Music's trash, holds none and did not go from Incoming.
        let read = round(&cat, &mut k, &rs);
        assert_eq!((read[0], read[1]), ((Holding::None, true), (Holding::None, false)));
        let placed = |request: i64| k.items.iter().filter(|i| i.record.request == request).all(|i| i.state == State::Placed);
        assert!(!placed(1) && placed(2) && placed(3) && placed(4), "gone from Incoming: let go; kept into Music and deleted there: never");
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
        let words = words_in(&k.items, &[(1, None)]);
        assert_eq!(words.get(&1), Some(&Word::Waiting { why: None }));
        let state = request_state(&rs[0], Holding::None, None, words.get(&1));
        assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "not fetching 100%: {state:?}");
    }

    /// T8.3, rebuilt by RF4 (the review of runs 11 and 12, RB4): a copy Soulseek placed in Incoming and a pass read, then read as
    /// another record's: retagged, its file rows keep their places and their tracks, and those tracks move on to the record their
    /// album now is (`assign_records`: a track whose only file sits in another record's album). The placed record, every track of it
    /// moved on, retires keeping where they lay: its item is let go and its request looked for again, never "fetching 100%"; the
    /// other record holds the folder. Kept into Music first and retagged there, it is not looked for again (U7's rule). T8.3's first
    /// shape stays: its own files gone, and new rows on the other record's tracks in the folder, once the catalogue retires them.
    #[test]
    fn a_placed_copy_read_as_another_record_frees_its_item() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        use super::super::requests::{Holding, RequestState, request_state};
        use crate::catalogue::{AlbumCopy, identity};
        for (case, kept) in [("retagged in Incoming", false), ("kept into Music, retagged there", true)] {
            let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
            let (placed, other) = (cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Other Shore", 3));
            // Where the copy lies when it is retagged: Incoming (1), or Music (0) after a Keep; the other record's copy beside it.
            let folder = if kept { 0 } else { 1 };
            cat.copy_for_tests(placed, 1);
            cat.copy_for_tests(other, folder);
            let rs = [crate::userdata::Request { record: Some(placed), ..asked(1, None) }];
            let mut k = Kept::default();
            take_requests(&mut k, &rs, &[of(placed)], &[]);
            (k.items[0].state, k.items[0].folders, k.items[0].chosen) = (State::Placed, vec![two_tracks("a")], Some(0));
            let dir = format!("copy-{placed}");
            if kept {
                cat.move_files(1, &dir, 0, &dir).unwrap();
            }
            // A copy's rows as they lie: each one's path, track, and the track's record.
            let rows = |record: i64| -> Vec<(String, i64, Option<i64>)> {
                let db = cat.db_for_tests();
                let mut st = db.prepare("SELECT f.rel, f.track, t.record FROM files f JOIN tracks t ON t.id = f.track WHERE f.rel LIKE ?1 ORDER BY f.rel").unwrap();
                let rows: Vec<(String, i64, Option<i64>)> = st.query_map([format!("copy-{record}/%")], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
                rows
            };
            let before = rows(placed);
            assert!(before.len() == 2 && before.iter().all(|(_, _, r)| *r == Some(placed)), "{case}: {before:?}");
            // Retagged as the other record's album: one album of its files and the placed copy's, where they lie.
            let lying: Vec<(String, i64)> = rows(other).into_iter().chain(before.clone()).map(|(rel, track, _)| (rel, track)).collect();
            let album = AlbumCopy {
                facts: identity::RecordFacts {
                    release: None,
                    artist: identity::fold("Glass Harbour"),
                    title: identity::fold("Other Shore"),
                    tracks: (1..=lying.len()).map(|n| identity::Listed::of(&format!("Track {n}"), 200_000)).collect(),
                },
                title: "Other Shore".into(),
                artist: "Glass Harbour".into(),
                tracks: lying.iter().enumerate().map(|(n, (_, track))| (*track, Some(1), Some(n as u32 + 1))).collect(),
                files: lying.iter().map(|(rel, _)| (folder, rel.clone())).collect(),
                folder,
                ..Default::default()
            };
            cat.assign_records(&[album]).unwrap();
            let after = rows(placed);
            let places = |rows: &[(String, i64, Option<i64>)]| rows.iter().map(|(rel, track, _)| (rel.clone(), *track)).collect::<Vec<_>>();
            assert_eq!(places(&after), places(&before), "{case}: the same rows, on the same tracks");
            assert!(after.iter().all(|(_, _, r)| *r == Some(other)), "{case}: their tracks the other record's now: {after:?}");
            let read = round(&cat, &mut k, &rs);
            if kept {
                assert_eq!(read, [(Holding::None, false)], "{case}: its last copy went from Music");
                assert_eq!(k.items.iter().map(|i| i.state).collect::<Vec<_>>(), [State::Placed], "{case}: never looked for again");
                continue;
            }
            assert_eq!(read, [(Holding::None, true)], "{case}: its last copy went from Incoming");
            assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record, i.state)).collect::<Vec<_>>(), [(1, placed, State::Waiting)], "{case}: let go, and looked for again");
            let words = words_in(&k.items, &[(1, None)]);
            let state = request_state(&rs[0], Holding::None, None, words.get(&1));
            assert!(matches!(state, RequestState::Wanted { soulseek: Some(Word::Waiting { .. }), .. }), "{case}: not fetching 100%: {state:?}");
            let theirs = [crate::userdata::Request { record: Some(other), ..asked(2, None) }];
            assert_eq!(round(&cat, &mut Kept::default(), &theirs), [(Holding::Arrived, false)], "{case}: the other record holds the folder");
        }
        // T8.3's first shape.
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
        let words = words_in(&k.items, &[(1, None)]);
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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

    /// F2.11 (kind 6): a record merged into another (0.17's fold) takes its Soulseek item along: the next round names the record its
    /// request reads now, one item still.
    #[test]
    fn a_merged_records_soulseek_item_names_the_record_it_went_into() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let (older, newer) = (cat.record_for_tests("Stone Sky", 0), cat.record_for_tests("Stone Sky", 0));
        let asked = crate::userdata::Request {
            id: 9701,
            user: "alice".into(),
            album_key: "glassharbour|stonesky".into(),
            artist_key: "glass harbour".into(),
            release_group: None,
            album: "Stone Sky".into(),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at: 1,
            status: super::super::requests::APPROVED.into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
            record: Some(newer),
        };
        let mut k = Kept::default();
        let round = |k: &mut Kept| {
            let of = super::super::requests::records_of(&cat, std::slice::from_ref(&asked)).unwrap();
            take_requests(k, std::slice::from_ref(&asked), &of, &[]);
        };
        round(&mut k);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record)).collect::<Vec<_>>(), [(9701, newer)]);
        cat.merge_records(newer, older).unwrap();
        round(&mut k);
        assert_eq!(k.items.iter().map(|i| (i.record.request, i.record.record)).collect::<Vec<_>>(), [(9701, older)], "the next round names the record it went into");
    }

    /// F15.6: alice's open request on one record of a master and bob's on the other, a Soulseek item for each, none ticked; the two
    /// records joined by their master (the newer into the older): both requests read the one record, one item searches it (alice's, the
    /// first made) and bob's is taken off, his request reading the record's item. Where bob's is under way (fetching), it stays and
    /// alice's is taken off, her request waiting on his: never two searching, never one under way taken off.
    #[test]
    fn a_merge_by_a_master_leaves_one_item_searching() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        for (case, bobs_fetching) in [("none ticked", false), ("bob's fetching", true)] {
            let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
            let (older, newer) = (cat.record_for_tests("Two Rooms", 0), cat.record_for_tests("Two Rooms", 0));
            let request = |id: i64, user: &str, record: i64| crate::userdata::Request {
                id,
                user: user.into(),
                album_key: "glassharbour|tworooms".into(),
                artist_key: "glass harbour".into(),
                release_group: None,
                album: "Two Rooms".into(),
                artist: "Glass Harbour".into(),
                year: None,
                cover: None,
                at: 1,
                status: super::super::requests::APPROVED.into(),
                reason: None,
                decided_by: None,
                decided_at: None,
                fetched: None,
                record: Some(record),
            };
            let asked = [request(9801, "alice", older), request(9802, "bob", newer)];
            let mut k = Kept::default();
            let round = |k: &mut Kept| {
                let of = super::super::requests::records_of(&cat, &asked).unwrap();
                take_requests(k, &asked, &of, &[])
            };
            round(&mut k);
            assert_eq!(k.items.len(), 2, "{case}: an item each");
            if bobs_fetching {
                let bobs = k.items.iter_mut().find(|i| i.record.request == 9802).unwrap();
                bobs.state = State::Fetching;
                bobs.chosen = Some(0);
            }
            for (record, master) in [(older, "5000031"), (newer, "5000031")] {
                cat.learn_from_read(record, &crate::catalogue::records::Ref::new("discogs", "release", &format!("36000{record}")), &[crate::catalogue::records::Ref::new("discogs", "master", master)], &[]).unwrap();
            }
            assert_eq!(cat.record(newer).unwrap().unwrap().id, older, "{case}: one record");
            round(&mut k);
            let records: Vec<i64> = super::super::requests::records_of(&cat, &asked).unwrap().into_iter().map(|o| o.unwrap().record).collect();
            assert_eq!(records, [older, older], "{case}: both requests read the one record");
            let searching: Vec<(i64, State)> = k.items.iter().map(|i| (i.record.request, i.state.clone())).collect();
            if bobs_fetching {
                assert_eq!(searching, [(9802, State::Fetching)], "{case}: the one under way stays, never taken off; alice's waits on it, taken off");
            } else {
                assert_eq!(searching, [(9801, State::Waiting)], "{case}: one item searches it, the first made; bob's taken off");
            }
        }
    }

    /// A1.4.1: an item Stuck with "No list to judge by", due in the past, whose record has since gained a MusicBrainz reference whose
    /// list has lengths: the round takes it (the longest due first; one not due yet, one fetching or placed never), its list read again:
    /// Searching, judged by it. With still no list: Stuck again, a day on.
    #[tokio::test]
    async fn a_stuck_item_is_searched_again_once_due() {
        let _state = crate::app::references::TestState::set().await;
        let t = now();
        let stuck = |id: &str, due: i64| Item { id: id.into(), state: State::Stuck, why: Some("No list to judge by".into()), due, folders: Vec::new(), ..found_twice(id) };
        let mut k = Kept::default();
        k.items = vec![stuck("ss-a141-1", t - 10), stuck("ss-a141-2", t - 500), stuck("ss-a141-3", t + 3600), Item { state: State::Fetching, ..found_twice("ss-a141-4") }, Item { state: State::Placed, ..found_twice("ss-a141-5") }];
        assert_eq!(next_to_search(&k, t).as_deref(), Some("ss-a141-2"), "the longest due first");
        k.items.retain(|i| i.id != "ss-a141-2");
        assert_eq!(next_to_search(&k, t).as_deref(), Some("ss-a141-1"));
        k.items.retain(|i| i.id != "ss-a141-1");
        assert_eq!(next_to_search(&k, t), None, "never one not due yet, fetching or placed");
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/music")]).unwrap());
        for (id, found) in [("ss-a141-found", true), ("ss-a141-none", false)] {
            let record = cat.record_for_tests("Two Rooms", 0);
            assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("musicbrainz", "release", &format!("r141-{id}")), "link").unwrap());
            let it = Item { record: Record { record, ..found_twice(id).record }, ..stuck(id, t - 10) };
            kept(|q| q.items.push(it.clone()));
            let read = move |_: crate::catalogue::records::Ref| {
                let list: Vec<Track> = [("Lamplight", 200_000), ("Morning", 180_000)].into_iter().zip(1..).map(|((title, ms), n)| Track { disc: 1, position: n, title: title.into(), length_ms: Some(ms), recording: None }).collect();
                std::future::ready(Ok::<_, anyhow::Error>(found.then(|| (list, None::<crate::catalogue::records::Ref>, crate::app::references::ReadMore::default()))))
            };
            let goes = read_its_list(id, &it, &cat, read).await;
            let now_it = item(id).unwrap();
            kept(|q| q.items.retain(|i| i.id != id));
            if found {
                assert!(goes, "judged by its list: on to its search");
                assert_eq!((now_it.state, now_it.tracks.len()), (State::Searching, 2));
            } else {
                assert!(!goes);
                assert_eq!(now_it.state, State::Stuck, "still no list: Stuck again");
                assert!((now_it.due - (t + 86_400)).abs() <= 5, "a day on: {}", now_it.due - t);
            }
        }
    }

    /// A1.4.2: 0.16.3's queue with items Stuck on "503 Service Temporarily Unavailable" and on "No list to judge by", due long since:
    /// the first round takes each in turn (no step part needed: their due is past), its list read again; MusicBrainz busy now reads
    /// `Later` (Waiting, R23), never Stuck again for that.
    #[tokio::test]
    async fn the_stuck_items_of_0_16_are_looked_at_in_the_first_round() {
        let _state = crate::app::references::TestState::set().await;
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-a142-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("soulseek.json");
        let of_016 = |id: &str, n: i64, why: &str, due: i64| {
            serde_json::json!({ "id": id, "record": { "artist": "Kite Theory", "album": format!("Record {n}"), "release_group": format!("00000000-0000-4000-8000-00000000014{n}"), "year": 2001, "request": n },
                "state": "stuck", "tracks": [], "folders": [], "failed": [], "misses": 0, "due": due, "why": why, "percent": 0.0, "set_aside": [] })
        };
        let queue = serde_json::json!({ "next": 3, "items": [of_016("ss-a142-1", 1, "503 Service Temporarily Unavailable", 1_790_000_000), of_016("ss-a142-2", 2, "No list to judge by", 1_790_100_000)], "dropped": [] });
        std::fs::write(&p, serde_json::to_vec(&queue).unwrap()).unwrap();
        let mut k = load(&p);
        assert_eq!(k.items.len(), 2, "0.16.3's queue read as it is");
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/music")]).unwrap());
        let busy = |_: crate::catalogue::records::Ref| std::future::ready(Err::<Option<crate::app::references::Read>, _>(anyhow::anyhow!("MusicBrainz answered 503 Service Temporarily Unavailable")));
        let mut taken = Vec::new();
        while let Some(id) = next_to_search(&k, now()) {
            let mut it = k.items.iter().find(|i| i.id == id).cloned().unwrap();
            k.items.retain(|i| i.id != id);
            // Its request's record, as the requests' read gives it (`take_requests`): a MusicBrainz reference.
            it.record.record = cat.record_for_tests(&it.record.album, 0);
            cat.set_reference(&crate::serve::ids::record(it.record.record), &crate::catalogue::records::Ref::new("musicbrainz", "release", &format!("r142-{id}")), "link").unwrap();
            kept(|q| q.items.push(it.clone()));
            let goes = read_its_list(&id, &it, &cat, busy).await;
            let now_it = item(&id).unwrap();
            kept(|q| q.items.retain(|i| i.id != id));
            assert!(!goes, "{id}: waits");
            assert_eq!(now_it.state, State::Waiting, "{id}: Later (R23), never Stuck again for MusicBrainz being busy");
            assert!(now_it.why.as_deref().is_some_and(|w| w.contains("503")), "{id}: {:?}", now_it.why);
            taken.push(id);
        }
        assert_eq!(taken, ["ss-a142-1", "ss-a142-2"], "each in turn, the longest due first");
        std::fs::remove_dir_all(&d).ok();
    }

    /// RG1 (the owner, 2026-11-18: "When the type agrees"), at 0.17's fold (PA1): alice's open Want "Kite EP", by its names alone (a
    /// record of its own, as 0.16's step or preview 1 made it), being fetched by Soulseek (its batch 60% done), beside the house's had
    /// "Kite". Where that "Kite" is an EP by the release type rule (four tracks), or a ten-track one set to EP by hand, the fold joins the
    /// Want's record to it: the next round takes her item off with its fetch, to be cancelled (no second copy), and the Want is
    /// delivered as had. Where it is a ten-track album, the two stay two: the Want stays open, its item keeps its record, its batch and
    /// its share, nothing taken off. The same for "Kite LP": an album joins, an EP does not.
    #[test]
    fn the_fold_joins_a_wants_record_to_a_had_one_only_where_the_type_agrees() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        for (case, title, tracks, typed, joins) in [
            ("EP, a had EP", "Kite EP", 4, None, true),
            ("EP, a had album set to EP by hand", "Kite EP", 10, Some("ep"), true),
            ("EP, a had album", "Kite EP", 10, None, false),
            ("LP, a had album", "Kite LP", 10, None, true),
            ("LP, a had EP", "Kite LP", 4, None, false),
        ] {
            let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
            let ud = crate::userdata::UserData::open(None).unwrap();
            let wanted = crate::catalogue::records::Wanted { artist: "Kite Theory".into(), title: title.into(), ..Default::default() };
            let own = cat.record_for_request(&wanted).unwrap();
            let artist = cat.artist_for("Kite Theory").unwrap();
            let kite = cat.record_for_tests("Kite", tracks);
            cat.execute_for_tests(&format!("UPDATE records SET artist = 'Kite Theory' WHERE id = {kite}; INSERT INTO record_artists (record, artist, position, how) VALUES ({kite}, {artist}, 0, 'files');"));
            if let Some(t) = typed {
                cat.keep_types(&[(kite, t.to_string())]).unwrap();
            }
            let id = ud
                .add_request(&crate::userdata::Request {
                    id: 0,
                    user: "alice".into(),
                    album_key: format!("kitetheory|{}", crate::catalogue::identity::fold(title)),
                    artist_key: "kite theory".into(),
                    release_group: None,
                    album: title.into(),
                    artist: "Kite Theory".into(),
                    year: None,
                    cover: None,
                    at: 1,
                    status: super::super::requests::APPROVED.into(),
                    reason: None,
                    decided_by: None,
                    decided_at: None,
                    fetched: None,
                    record: Some(own),
                })
                .unwrap();
            let asked = ud.request(id).unwrap().unwrap();
            let mut k = Kept::default();
            let round = |k: &mut Kept| -> Vec<Item> {
                let of = super::super::requests::records_of(&cat, std::slice::from_ref(&asked)).unwrap();
                take_requests(k, std::slice::from_ref(&asked), &of, &[])
            };
            assert!(round(&mut k).is_empty(), "{case}: nothing taken off at first");
            let item = k.items.iter_mut().find(|i| i.record.request == id).expect("an item of her Want");
            item.state = State::Fetching;
            item.batch = Some("batch-60".into());
            item.percent = 60.0;
            let fetching = item.clone();
            super::super::step_017::fold(&cat, &ud, &[], &|| Ok(())).unwrap();
            let taken = round(&mut k);
            if joins {
                assert_eq!(cat.record(own).unwrap().unwrap().id, kite, "{case}: the Want's record joined the had one");
                assert!(k.items.iter().all(|i| i.record.request != id), "{case}: her item taken off");
                assert_eq!(taken.iter().map(|i| i.batch.clone()).collect::<Vec<_>>(), [Some("batch-60".to_string())], "{case}: its fetch handed back to be cancelled");
                assert_eq!(super::super::requests::deliver_in(&ud, &cat, &[], None).len(), 1, "{case}: the Want delivered as had");
                assert_ne!(ud.request(id).unwrap().unwrap().status, super::super::requests::APPROVED, "{case}: closed");
            } else {
                assert_eq!(cat.record(own).unwrap().unwrap().id, own, "{case}: two records");
                assert!(taken.is_empty(), "{case}: nothing taken off");
                assert_eq!(k.items.iter().find(|i| i.record.request == id), Some(&fetching), "{case}: its item, its record, its batch and its share untouched");
                assert!(super::super::requests::deliver_in(&ud, &cat, &[], None).is_empty(), "{case}: nothing delivered");
                assert_eq!(ud.request(id).unwrap().unwrap().status, super::super::requests::APPROVED, "{case}: the Want stays open");
            }
        }
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
        let _one = TEST_STATE.write().await;
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
        crate::catalogue::records::data_0_16(&d).unwrap();
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
        std::future::ready(Ok(Some((list, None, Default::default()))))
    }

    /// R25 (N7): Search now reads the record's counting list again, so a list chosen by hand since reaches the item; one in
    /// flight keeps what it has.
    #[test]
    fn search_now_reads_the_counting_list_again() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
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
        // Its list is read in the background, whose switch other tests here move (TS01 item 9: they raced).
        let _one = crate::app::switches::TEST_STATE.read().await;
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
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let raw = r#"{"next":3,"items":[{"id":"ss-3","record":{"artist":"Air","album":"Moon Safari","release_group":"rg","request":7},"state":"waiting"}]}"#;
        let k: Kept = serde_json::from_str(raw).unwrap();
        assert_eq!((k.items[0].due, k.items[0].misses, k.items[0].record.request), (0, 0, 7), "no time kept: due");
    }

    /// AL02.8 ((c); INTERACTIONS top 12, I09): ann's request of R2, delivered into Music, closes at the requests' round. Months later
    /// root deletes R2 from Music and the trash is emptied: R2 holds no copy, and no Placed item stands in the way. Soulseek's list,
    /// kept by the requests it reads (`requests_taken`, `take_requests`), makes no item for R2: a delivered request is never
    /// searched again, and never reads wanted.
    #[tokio::test]
    async fn a_delivered_album_deleted_from_a_kept_library_is_not_searched_again() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        use crate::app::requests::tests::{Copies, Strict, with_accounts};
        use crate::app::requests::{DELIVERED, Hands, RequestState, round_with};
        use crate::app::room::tests::{MUSIC, Roomy, room_shape};
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let r2 = cat.record_for_tests("Quiet Lanes", 2);
        cat.copy_for_tests(r2, MUSIC);
        cat.came_for_tests(MUSIC, 250);
        let asked = crate::userdata::Request {
            id: 0,
            user: "ann".into(),
            album_key: "k-r2".into(),
            artist_key: "glass harbour".into(),
            release_group: None,
            album: "Quiet Lanes".into(),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at: 150,
            status: super::super::requests::APPROVED.into(),
            reason: None,
            decided_by: Some("ann".into()),
            decided_at: Some(150),
            fetched: Some(super::super::requests::SOULSEEK_SEARCHES.into()),
            record: Some(r2),
        };
        let id = serve.userdata.add_request(&asked).unwrap();
        let (strict, house) = (Strict::expecting_nothing(), Copies::none());
        round_with(&Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy }, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(serve.userdata.request(id).unwrap().unwrap().status, DELIVERED, "delivered into Music");
        cat.mark_trashed(MUSIC, &format!("copy-{r2}"), "300/Quiet Lanes").unwrap();
        cat.execute_for_tests(&format!("DELETE FROM files WHERE folder = {MUSIC}"));
        let (approved, of) = requests_taken(&serve.userdata, &cat, &serve.library.kinds()).unwrap();
        assert!(!approved.iter().any(|r| r.id == id), "not among the requests Soulseek reads");
        assert!(!items_of(&approved, &of).contains(&r2), "no item for R2");
        let now = serve.userdata.request(id).unwrap().unwrap();
        let held = super::super::requests::records_held(&cat, std::slice::from_ref(&now), &serve.library.kinds()).unwrap().pop().flatten().map_or_else(Default::default, |o| o.holding);
        assert_eq!(super::super::requests::request_state(&now, held, None, None), RequestState::Delivered, "never wanted");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// An item of record 9700 found with peer-a's and peer-b's folders, waiting for a tick (RQ01.8).
    fn found_twice(id: &str) -> Item {
        let folder = |user: &str| Folder {
            user: user.into(),
            dir: format!("@@{user}\\Music\\Night Ferry"),
            format: "flac".into(),
            lossless: true,
            free_slot: true,
            queue: 0,
            speed: 1_000_000,
            extra: 0,
            files: Vec::new(),
            unchecked: None,
        };
        Item {
            id: id.into(),
            record: Record { record: 9_700, artist: "Glass Harbour".into(), album: "Night Ferry".into(), year: None, request: 1 },
            state: State::Found,
            tracks: Vec::new(),
            untimed: Vec::new(),
            folders: vec![folder("peer-a"), folder("peer-b")],
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
        }
    }

    use crate::slskd::scripted::Scripted;

    /// An item started as the downloads' worker's look starts it (its batch's id kept, sent through the lane); whether it is under way.
    async fn start_it(id: &str, slskd: &Slskd, folder: &Path) -> bool {
        start_downloads(&|i: &Item| i.id == id, slskd, folder, &Look::roomy()).await;
        item(id).is_some_and(|i| i.state == State::Fetching)
    }

    /// The workers' seams in turn for one item, as 0.17's one fetcher took it up (`fetch_from`, which the tests before round 1.3
    /// drove; they assert what they did): its start (the disks as they are), its download followed to its end, its check and placing.
    pub async fn fetch_from<G, Fut>(id: &str, slskd: &Slskd, folder: &Path, gate: G)
    where
        G: Fn(PathBuf, ReleaseTrack) -> Fut,
        Fut: std::future::Future<Output = Result<f32>>,
    {
        start_downloads(&|i: &Item| i.id == id, slskd, folder, &Look::of_disks()).await;
        follow_one(id, slskd, folder).await;
        check_one(id, slskd, folder, &Places::of_server(), &gate).await;
    }

    /// Every one of `futures` run together, on this task; their outputs in order.
    async fn all<'a, T>(mut futures: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a>>>) -> Vec<T> {
        let mut out: Vec<Option<T>> = futures.iter().map(|_| None).collect();
        std::future::poll_fn(|cx| {
            let mut pending = false;
            for (f, o) in futures.iter_mut().zip(out.iter_mut()) {
                if o.is_none() {
                    match f.as_mut().poll(cx) {
                        std::task::Poll::Ready(v) => *o = Some(v),
                        std::task::Poll::Pending => pending = true,
                    }
                }
            }
            if pending { std::task::Poll::Pending } else { std::task::Poll::Ready(()) }
        })
        .await;
        out.into_iter().map(Option::unwrap).collect()
    }

    /// An item picked for `users`' folders in turn (record 9_000's two tracks), its first folder chosen, on the list as `id`.
    fn picked(id: &str, users: &[&str]) {
        listed(id, State::Approved, None);
        with_item(id, |i| i.folders = users.iter().map(|u| two_tracks(u)).collect());
    }

    /// SK2.1: slskd's operations go one at a time whoever asks (Soulseek's records, its searcher, the bulk queue's POST, a fetch
    /// taken off the list), its reads beside them, and the six downloads' transfers run together once started.
    #[tokio::test]
    async fn slskds_operations_go_one_at_a_time_whoever_asks() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let slskd = Scripted::new().slow(200).hold_all(20.0);
        let client = slskd.client();
        let d = data_folder("sk21");
        let ids: Vec<String> = (0..6).map(|n| format!("ss-sk21-{n}")).collect();
        for (n, id) in ids.iter().enumerate() {
            picked(id, &[&format!("peer-{}", (b'a' + n as u8) as char)]);
        }
        // A seventh under way, to be taken off (its transfers to DELETE).
        picked("ss-sk21-off", &["peer-g"]);
        assert!(start_it("ss-sk21-off", &client, &d).await);
        let leaving = item("ss-sk21-off").unwrap();
        waiting_with("ss-sk21-s", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        let ti1 = Offered { filename: r"z\07 Track.flac".into(), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false };
        let done = std::sync::atomic::AtomicBool::new(false);
        let work = async {
            let starts = all(ids.iter().map(|id| Box::pin(start_it(id, &client, &d)) as std::pin::Pin<Box<dyn std::future::Future<Output = bool>>>).collect());
            let (started, _, bulk, _) = tokio::join!(starts, search_through("ss-sk21-s", &client), client.fetch("peer-z", &ti1), cancel_fetch(&client, &d, &leaving));
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            (started, bulk)
        };
        let reads = async {
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                client.transfers(leaving.batch.as_deref().unwrap()).await.ok();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let ((started, bulk), ()) = tokio::join!(work, reads);
        assert!(slskd.never_overlapped(), "{:?}", slskd.violations());
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        assert!(slskd.reads_during_operations() > 0, "reads went on meanwhile");
        assert!(started.iter().all(|s| *s) && bulk.is_ok(), "{started:?} {bulk:?}");
        assert!(ids.iter().all(|id| item(id).is_some_and(|i| i.state == State::Fetching && i.why.is_none())));
        let going = slskd.going();
        assert!(ids.iter().all(|id| going.contains(&item(id).unwrap().batch.unwrap())), "the six transfers run together");
        assert!(slskd.deleted(leaving.batch.as_deref().unwrap()), "the one taken off: its transfers dropped through the lane");
        let kinds: Vec<_> = slskd.ops().into_iter().filter(|o| o.status == 200).map(|o| o.kind).collect();
        for k in [crate::slskd::scripted::Kind::Batch, crate::slskd::scripted::Kind::Search, crate::slskd::scripted::Kind::ForgetSearch, crate::slskd::scripted::Kind::DropTransfer] {
            assert!(kinds.contains(&k), "{k:?} went through the lane");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK2.2: a 429 is slskd busy: the batch is asked again after 1, 2, 4 s (or its `Retry-After` where longer), in the lane, until
    /// taken; the item meanwhile approved with no why and due 0, its request reading Fetching; nobody failed, nothing counted, never
    /// a five-minute hold. A search's POST, a DELETE and a read meeting one alike; ten minutes of them: every 30 s, the log saying
    /// so once a minute.
    #[tokio::test]
    async fn a_busy_slskd_is_asked_again_shortly_and_blames_no_one() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk22");
        let seen: Arc<Mutex<Vec<Item>>> = Arc::default();
        let kept_seen = seen.clone();
        let slskd = Scripted::new().busy(3).on_post(move |_| kept_seen.lock().unwrap().extend(item("ss-sk22")));
        picked("ss-sk22", &["peer-a"]);
        assert!(start_it("ss-sk22", &slskd.client(), &d).await);
        assert_eq!(slskd.post_offsets_secs(), [0, 1, 3, 7]);
        let i = item("ss-sk22").unwrap();
        assert!(i.state == State::Fetching && i.why.is_none() && i.failed.is_empty() && i.local_failures == 0 && i.misses == 0 && i.place_failures == 0, "{i:?}");
        assert_eq!(slskd.batches_of("peer-a"), [i.batch.clone().unwrap()], "one batch");
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 4);
        for s in &seen {
            assert!(s.state == State::Approved && s.why.is_none() && s.due == 0, "meanwhile: {s:?}");
            assert!(matches!(word_of(s), Word::Fetching { .. }), "its request reads Fetching");
        }
        // (a) Retry-After: 20.
        let slskd = Scripted::new().busy(3).retry_after(20);
        picked("ss-sk22-a", &["peer-a"]);
        assert!(start_it("ss-sk22-a", &slskd.client(), &d).await);
        assert_eq!(slskd.post_offsets_secs()[1], 20, "the second try 20 s after the first");
        // (b) A search's POST: it goes once taken, no miss, no why.
        let found = serde_json::json!([{ "username": "peer-s", "hasFreeUploadSlot": true, "queueLength": 0, "uploadSpeed": 100, "files": [
            { "filename": r"x\Air\Moon Safari\01 One.flac", "size": 1000, "length": 100 }, { "filename": r"x\Air\Moon Safari\02 Two.flac", "size": 1000, "length": 200 } ] }]);
        let slskd = Scripted::new().busy(3).answers(found.as_array().unwrap().clone());
        waiting_with("ss-sk22-b", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        search_through("ss-sk22-b", &slskd.client()).await;
        let i = item("ss-sk22-b").unwrap();
        assert_eq!((i.state, i.misses, i.why.clone()), (State::Found, 0, None), "{i:?}");
        let searches: Vec<u16> = slskd.ops().into_iter().filter(|o| o.kind == crate::slskd::scripted::Kind::Search).map(|o| o.status).collect();
        assert_eq!(searches, [429, 429, 429, 200]);
        // (c) A DELETE: it goes once taken, and the transfer is gone.
        let slskd = Scripted::new().hold_all(5.0);
        picked("ss-sk22-c", &["peer-c"]);
        assert!(start_it("ss-sk22-c", &slskd.client(), &d).await);
        let batch = item("ss-sk22-c").unwrap().batch.unwrap();
        slskd.then_busy(3);
        cancel_fetch(&slskd.client(), &d, &item("ss-sk22-c").unwrap()).await;
        assert!(slskd.deleted(&batch), "dropped once slskd took it");
        // (d) A read answered 429 (a proxy before slskd): read again at its next look, no failed read of `READS`.
        let slskd = Scripted::new().read_busy(READS + 5);
        picked("ss-sk22-d", &["peer-d"]);
        assert!(start_it("ss-sk22-d", &slskd.client(), &d).await);
        fetch_from("ss-sk22-d", &slskd.client(), &d, |_, _| std::future::ready(Err(acoustid_away()))).await;
        let i = item("ss-sk22-d").unwrap();
        assert_eq!((i.state, i.failed.len(), i.batch.is_some()), (State::Checking, 0, true), "followed to its end: {i:?}");
        // (e) Ten minutes of 429s: tried every 30 s at most, the log saying so once a minute, taken the moment slskd takes it.
        let slskd = Scripted::new().busy_for(600_000);
        picked("ss-sk22-e", &["peer-e"]);
        let client = slskd.client();
        assert!(start_it("ss-sk22-e", &client, &d).await);
        let at = slskd.post_offsets_secs();
        assert_eq!(&at[..7], [0, 1, 3, 7, 15, 31, 61]);
        assert!(at.windows(2).skip(6).all(|w| w[1] - w[0] == 30), "{at:?}");
        assert!(*at.last().unwrap() >= 600 && *at.last().unwrap() < 630, "the moment slskd takes it: {at:?}");
        assert_eq!(client.busy_said(), 10, "once a minute");
        let i = item("ss-sk22-e").unwrap();
        assert!(i.state == State::Fetching && i.why.is_none() && i.failed.is_empty() && i.due == 0);
        for v in ["ss-sk22", "ss-sk22-a", "ss-sk22-b", "ss-sk22-c", "ss-sk22-d", "ss-sk22-e"] {
            let i = item(v).unwrap();
            assert!(!i.why.as_deref().unwrap_or_default().contains("429") && !i.why.as_deref().unwrap_or_default().contains("Too Many Requests"), "{v}: {:?}", i.why);
        }
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK2.3 (kind 4): a batch's id is on its item before its POST is sent; an answer lost is read back by that id (slskd has it:
    /// fetched, followed); a POST meeting slskd's own unfinished work goes again a second later; where slskd never took it (the
    /// connection cut first, or a stop between the id kept and the POST), it is sent again under the same id, once. Never two
    /// batches of one item, never a hold, nobody failed.
    #[tokio::test]
    async fn a_lost_answer_is_read_back_by_its_batch_and_never_fetched_twice() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk23");
        let noted: Arc<Mutex<Vec<(String, Option<String>)>>> = Arc::default();
        let kept_noted = noted.clone();
        let slskd = Scripted::new().lose_answer_of_first_post().on_post(move |op| {
            if op.user == "peer-a" {
                kept_noted.lock().unwrap().push((op.batch.clone(), item("ss-sk23-1").and_then(|i| i.batch)));
            }
        });
        picked("ss-sk23-1", &["peer-a"]);
        picked("ss-sk23-2", &["peer-b"]);
        let client = slskd.client();
        start_it("ss-sk23-1", &client, &d).await;
        start_it("ss-sk23-2", &client, &d).await;
        let noted = noted.lock().unwrap().clone();
        assert_eq!(noted.len(), 1);
        assert_eq!(Some(noted[0].0.clone()), noted[0].1, "its batch id on the item before its POST was sent");
        let one = item("ss-sk23-1").unwrap();
        assert_eq!(slskd.batches_of("peer-a"), [one.batch.clone().unwrap()], "one batch of ss-1");
        assert_eq!(slskd.reads_of(&one.batch.clone().unwrap()), 1, "read back by its id");
        let b: Vec<(u16, u64)> = slskd.posts().into_iter().filter(|o| o.user == "peer-b").map(|o| (o.status, o.began_ms)).collect();
        assert_eq!(b.iter().map(|x| x.0).collect::<Vec<_>>(), [429, 200], "slskd's own unfinished work, then taken");
        assert_eq!(b[1].1 - b[0].1, 1000, "a second later");
        for id in ["ss-sk23-1", "ss-sk23-2"] {
            let i = item(id).unwrap();
            assert!(i.state == State::Fetching && i.why.is_none() && i.failed.is_empty() && i.due == 0, "{i:?}");
        }
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        // (a) The connection cut before slskd took it: read back, not known, sent again under the same id, once.
        let slskd = Scripted::new().cut_before_taking();
        picked("ss-sk23-a", &["peer-a"]);
        start_it("ss-sk23-a", &slskd.client(), &d).await;
        let i = item("ss-sk23-a").unwrap();
        let tries: Vec<String> = slskd.posts().into_iter().filter(|o| o.status != 429).map(|o| o.batch).collect();
        assert_eq!(tries, [i.batch.clone().unwrap(), i.batch.clone().unwrap()], "the same id, twice in all");
        assert_eq!(slskd.batches_of("peer-a").len(), 1);
        assert!(i.state == State::Fetching && i.failed.is_empty() && i.why.is_none(), "{i:?}");
        // (b) A stop between the id kept and its POST: at the next start, read back, not known, sent under that id.
        let slskd = Scripted::new();
        picked("ss-sk23-b", &["peer-a"]);
        with_item("ss-sk23-b", |i| i.batch = Some("00000000-0000-4000-8000-0000000002b3".into()));
        start_it("ss-sk23-b", &slskd.client(), &d).await;
        let i = item("ss-sk23-b").unwrap();
        assert_eq!(slskd.batches_of("peer-a"), ["00000000-0000-4000-8000-0000000002b3"]);
        assert!(i.state == State::Fetching && i.failed.is_empty() && i.why.is_none(), "{i:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Items whose ids begin `prefix`: a test's own on the shared list.
    fn ours(prefix: &str) -> impl Fn(&Item) -> bool + Sync + '_ {
        move |i: &Item| i.id.starts_with(prefix)
    }

    /// One read of every download of `within`, as each follower makes one.
    async fn follow_round(within: Within<'_>, slskd: &Slskd, folder: &Path) {
        let ids: Vec<String> = kept(|k| k.items.iter().filter(|i| within(i) && i.state == State::Fetching).map(|i| i.id.clone()).collect());
        for id in ids {
            follow_step(&id, slskd, folder, Duration::ZERO).await;
        }
    }

    /// The downloads' worker's looks and every follower's reads, again and again until nothing changes but how far.
    async fn settle(within: Within<'_>, slskd: &Slskd, folder: &Path, look: &Look) {
        let shape = || kept(|k| k.items.iter().filter(|i| within(i)).map(|i| (i.id.clone(), i.state, i.batch.clone(), i.chosen, i.failed.clone())).collect::<Vec<_>>());
        for _ in 0..50 {
            let before = shape();
            start_downloads(within, slskd, folder, look).await;
            follow_round(within, slskd, folder).await;
            if shape() == before {
                return;
            }
        }
        panic!("never settled");
    }

    /// Every check of `within` taken until none is left.
    async fn check_until_idle<G, Fut>(within: Within<'_>, slskd: &Slskd, folder: &Path, places: &Places, gate: &G)
    where
        G: Fn(PathBuf, ReleaseTrack) -> Fut,
        Fut: std::future::Future<Output = Result<f32>>,
    {
        for _ in 0..50 {
            if !check_next(within, slskd, folder, places, gate).await {
                return;
            }
        }
        panic!("checks never ended");
    }

    /// The batch an item downloads.
    fn batch_of(id: &str) -> String {
        item(id).and_then(|i| i.batch).unwrap_or_else(|| panic!("{id} has no batch"))
    }

    /// Where a test's checks place and set aside: its own Incoming library and data folder, under `d`.
    fn places_in(d: &Path) -> Places {
        Places { library: Ok(d.join("incoming")), data: Some(d.join("data")) }
    }

    /// Whether a folder holds anything (one not there holds nothing).
    fn holds_anything(dir: &Path) -> bool {
        std::fs::read_dir(dir).is_ok_and(|mut r| r.next().is_some())
    }

    /// One call of the heard gate: the batch its file came in, its name, whether the one decode turn was held meanwhile, and the
    /// watched items' states then.
    type Call = (String, String, bool, Vec<(String, State)>);

    /// The gate, heard in its turn (SCENARIOS-SK17, "Shapes and fakes", for SK1.3 and SK1.6; SK5's `Heard` judges sounds): each call recorded; two at once a violation; `block` stops each call
    /// until `let_all_go`.
    #[derive(Clone)]
    struct HeardInTurn {
        calls: Arc<Mutex<Vec<Call>>>,
        at_once: Arc<std::sync::atomic::AtomicUsize>,
        violations: Arc<Mutex<Vec<String>>>,
        gate: Arc<tokio::sync::Semaphore>,
        blocking: Arc<std::sync::atomic::AtomicBool>,
        watch: Arc<Mutex<Vec<String>>>,
    }

    impl HeardInTurn {
        fn new() -> Self {
            Self {
                calls: Arc::default(),
                at_once: Arc::default(),
                violations: Arc::default(),
                gate: Arc::new(tokio::sync::Semaphore::new(0)),
                blocking: Arc::default(),
                watch: Arc::default(),
            }
        }
        /// Each call waits for `let_all_go`.
        fn block(self) -> Self {
            self.blocking.store(true, std::sync::atomic::Ordering::SeqCst);
            self
        }
        /// These items' states recorded with each call.
        fn watching(self, ids: &[&str]) -> Self {
            *self.watch.lock().unwrap() = ids.iter().map(|i| i.to_string()).collect();
            self
        }
        fn let_all_go(&self) {
            self.blocking.store(false, std::sync::atomic::Ordering::SeqCst);
            self.gate.add_permits(1000);
        }
        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
        /// The batches heard, in turn, each once.
        fn batches_in_order(&self) -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for (b, ..) in self.calls() {
                if out.last() != Some(&b) {
                    out.push(b);
                }
            }
            out
        }
        /// The watched items' states when `batch`'s first file was heard.
        fn states_at(&self, batch: &str) -> Vec<(String, State)> {
            self.calls().into_iter().find(|c| c.0 == batch).map(|c| c.3).unwrap_or_default()
        }
        /// The gate as Soulseek's check takes it.
        fn gate(&self) -> impl Fn(PathBuf, ReleaseTrack) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<f32>> + Send>> + use<> {
            let me = self.clone();
            move |file: PathBuf, _| {
                let me = me.clone();
                Box::pin(async move {
                    let batch = file.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let held = super::super::heavy_base::HEAVY.try_acquire().map(drop).is_err();
                    if me.at_once.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
                        me.violations.lock().unwrap().push(format!("two files heard at once: {name}"));
                    }
                    let watched = me.watch.lock().unwrap().clone();
                    let states = watched.iter().filter_map(|id| item(id).map(|i| (id.clone(), i.state))).collect();
                    me.calls.lock().unwrap().push((batch, name, held, states));
                    if me.blocking.load(std::sync::atomic::Ordering::SeqCst) {
                        me.gate.acquire().await.unwrap().forget();
                    }
                    me.at_once.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(0.97)
                })
            }
        }
    }

    /// SK1.1: every picked record starts at once, no cap of Cratarr's own: 25 batch POSTs one after another, all before any batch
    /// ends; each item downloading its own batch, its percent its own batch's alone.
    #[tokio::test]
    async fn picked_records_download_at_once() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let slskd = Scripted::new().slow(50).hold_all(10.0);
        let d = data_folder("sk11");
        let ids: Vec<String> = (1..=25).map(|n| format!("ss-sk11-{n:02}")).collect();
        for (n, id) in ids.iter().enumerate() {
            picked(id, &[&format!("peer-{:02}", n + 1)]);
        }
        let ours = ours("ss-sk11-");
        let client = slskd.client();
        start_downloads(&ours, &client, &d, &Look::roomy()).await;
        assert_eq!(slskd.posts().len(), 25);
        assert!(slskd.never_overlapped() && slskd.violations().is_empty(), "{:?}", slskd.violations());
        assert_eq!(slskd.going().len(), 25, "all 25 before any batch ended");
        let batches: Vec<String> = ids.iter().map(|id| batch_of(id)).collect();
        assert!(slskd.posts().iter().all(|p| batches.contains(&p.batch)), "a POST for a picked item only, each its own");
        follow_round(&ours, &client, &d).await;
        assert!(ids.iter().all(|id| item(id).is_some_and(|i| i.state == State::Fetching && i.percent == 10.0)));
        slskd.at(&batches[0], 55.0);
        follow_round(&ours, &client, &d).await;
        assert_eq!(item(&ids[0]).unwrap().percent, 55.0);
        assert!(ids[1..].iter().all(|id| item(id).unwrap().percent == 10.0), "each moves with its own transfers alone");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.2: never two records of one user at once. ss-2 waits for peer-a's other record, approved, no why, no deadline; ss-4,
    /// refused by peer-c, moves to its next folder (peer-a) and waits too, after ss-2; each goes the moment the one before it is
    /// done downloading (gone to its check). Variants: ss-1's failure, or its request leaving, lets ss-2 go at once.
    #[tokio::test]
    async fn no_two_records_from_one_user_at_once() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk12");
        let slskd = Scripted::new().hold_all(5.0).offline("peer-c").writes_to(&d, None);
        picked("ss-sk12-1", &["peer-a"]);
        picked("ss-sk12-2", &["peer-a"]);
        picked("ss-sk12-3", &["peer-b"]);
        picked("ss-sk12-4", &["peer-c", "peer-a"]);
        let (ours, client, look) = (ours("ss-sk12-"), slskd.client(), Look::roomy());
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-b", "peer-c"]);
        for id in ["ss-sk12-2", "ss-sk12-4"] {
            let i = item(id).unwrap();
            assert!(i.state == State::Approved && i.due == 0 && i.batch.is_none(), "{id} waits, no deadline: {i:?}");
            assert!(!i.why.as_deref().unwrap_or_default().contains("Waiting"), "the wait's words are worked out from the list, never kept: {i:?}");
        }
        assert_eq!(item("ss-sk12-2").unwrap().why, None);
        assert_eq!(item("ss-sk12-4").unwrap().why.as_deref(), Some("peer-c is offline"), "its failure's words, as today");
        let four = item("ss-sk12-4").unwrap();
        assert_eq!((four.failed.clone(), four.folders[0].user.as_str()), (vec!["peer-c".to_string()], "peer-a"), "its next folder, as today");
        // Their line, as their requests read it (the review of part 1, M3): worked out from the list, never kept.
        let line = |id: &str| word_in(&kept(|k| k.items.clone()), &item(id).unwrap());
        for id in ["ss-sk12-2", "ss-sk12-4"] {
            assert_eq!(line(id), Word::Waiting { why: Some("Waiting for peer-a's other record".into()) }, "{id}");
        }
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().len(), 3, "peer-a's turn taken: nothing more");
        // ss-1's download ends: ss-2's POST at once, while ss-1 waits for its check; ss-4 after ss-2's.
        slskd.release(&batch_of("ss-sk12-1"));
        follow_round(&ours, &client, &d).await;
        assert_eq!(item("ss-sk12-1").unwrap().state, State::Checking);
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().last().map(|p| (p.user.clone(), p.batch.clone())), Some(("peer-a".to_string(), batch_of("ss-sk12-2"))));
        assert_eq!(item("ss-sk12-4").unwrap().state, State::Approved, "ss-4 waits for ss-2");
        assert!(matches!(line("ss-sk12-2"), Word::Fetching { .. }), "its line goes once its POST goes: {:?}", line("ss-sk12-2"));
        assert_eq!(line("ss-sk12-4"), Word::Waiting { why: Some("Waiting for peer-a's other record".into()) });
        slskd.release(&batch_of("ss-sk12-2"));
        follow_round(&ours, &client, &d).await;
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().last().map(|p| (p.user.clone(), p.batch.clone())), Some(("peer-a".to_string(), batch_of("ss-sk12-4"))));
        for id in ["ss-sk12-1", "ss-sk12-2", "ss-sk12-3"] {
            assert!(item(id).unwrap().failed.is_empty(), "{id}: nobody failed for waiting");
        }
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        // ss-1's failure lets ss-2 go; peer-a failed for ss-1 alone.
        let slskd = Scripted::new().hold_all(5.0);
        picked("ss-sk12v-1", &["peer-a"]);
        picked("ss-sk12v-2", &["peer-a"]);
        let (ours, client) = (self::ours("ss-sk12v-"), slskd.client());
        start_downloads(&ours, &client, &d, &look).await;
        slskd.error(&batch_of("ss-sk12v-1"), "Remote connection closed");
        follow_round(&ours, &client, &d).await;
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-a"]);
        assert_eq!((item("ss-sk12v-1").unwrap().failed, item("ss-sk12v-2").unwrap().failed), (vec!["peer-a".to_string()], vec![]));
        assert_eq!(item("ss-sk12v-2").unwrap().state, State::Fetching);
        // ss-1's request leaving (the round takes it off, its fetch stopped) lets ss-2 go.
        let slskd = Scripted::new().hold_all(5.0);
        picked("ss-sk12w-1", &["peer-a"]);
        picked("ss-sk12w-2", &["peer-a"]);
        let (ours, client) = (self::ours("ss-sk12w-"), slskd.client());
        start_downloads(&ours, &client, &d, &look).await;
        let gone = kept(|k| take_off(&mut k.items, |i| i.id == "ss-sk12w-1"));
        cancel_fetch(&client, &d, &gone[0]).await;
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-a"]);
        assert!(item("ss-sk12w-2").unwrap().failed.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.3: checks one at a time, in the order downloads ended (ended together, the list's order), each record placed before the
    /// next check begins; each file heard on the one decode turn (`HEAVY`): while a test holds it, nothing is heard, and downloads
    /// go on meanwhile.
    #[tokio::test]
    async fn checks_go_one_at_a_time_while_downloads_run() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk13");
        let places = places_in(&d);
        let slskd = Scripted::new().hold_all(20.0).writes_to(&d, None);
        let ids = ["ss-sk13-1", "ss-sk13-2", "ss-sk13-3", "ss-sk13-4"];
        for (id, peer) in ids.iter().zip(["peer-a", "peer-b", "peer-c", "peer-d"]) {
            picked(id, &[peer]);
        }
        let (ours, client) = (ours("ss-sk13-"), slskd.client());
        start_downloads(&ours, &client, &d, &Look::roomy()).await;
        let (b1, b2, b3, b4) = (batch_of(ids[0]), batch_of(ids[1]), batch_of(ids[2]), batch_of(ids[3]));
        let heavy = super::super::heavy_base::HEAVY.acquire().await.unwrap();
        slskd.release(&b2);
        follow_round(&ours, &client, &d).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        slskd.release(&b1);
        slskd.release(&b3);
        follow_round(&ours, &client, &d).await;
        let ended = |id: &str| item(id).unwrap().ended_at;
        assert!(ended(ids[1]) > 0 && ended(ids[1]) < ended(ids[0]) && ended(ids[0]) <= ended(ids[2]), "when each ended is kept");
        let gate = HeardInTurn::new().watching(&ids);
        let g = gate.gate();
        let checking = check_until_idle(&ours, &client, &d, &places, &g);
        let meanwhile = async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(gate.calls().is_empty(), "nothing heard while the turn is held");
            slskd.at(&b4, 60.0);
            follow_round(&ours, &client, &d).await;
            assert_eq!(item(ids[3]).unwrap().percent, 60.0, "ss-4's download read and kept meanwhile");
            drop(heavy);
        };
        tokio::join!(checking, meanwhile);
        assert_eq!(gate.batches_in_order(), [b2.clone(), b1.clone(), b3.clone()]);
        assert_eq!(gate.calls().len(), 6, "every file of each, then the next");
        assert!(gate.calls().iter().all(|c| c.2), "each file heard on the one turn");
        assert!(gate.violations.lock().unwrap().is_empty(), "{:?}", gate.violations.lock().unwrap());
        assert!(gate.states_at(&b1).contains(&(ids[1].to_string(), State::Placed)), "ss-2 placed before ss-1's check began");
        assert!(gate.states_at(&b3).contains(&(ids[0].to_string(), State::Placed)), "ss-1 placed before ss-3's check began");
        assert!(ids[..3].iter().all(|id| item(id).unwrap().state == State::Placed));
        assert_eq!(item(ids[3]).unwrap().state, State::Fetching);
        assert!(slskd.violations().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.4 (a): every start weighs the files of every record downloading, waiting for its check, being checked or being placed:
    /// on a disk with 5 GB free, two records of 2 GB start, the others hold saying so, blaming no one; once one is placed, the next
    /// starts at the next look.
    #[tokio::test]
    async fn downloads_under_way_count_against_the_free_space() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        const GB: u64 = 1 << 30;
        let d = data_folder("sk14");
        let slskd = Scripted::new().hold_all(1.0).writes_to(&d, None);
        let ids = ["ss-sk14-1", "ss-sk14-2", "ss-sk14-3", "ss-sk14-4"];
        for (n, id) in ids.iter().enumerate() {
            picked(id, &[&format!("peer-{}", n + 1)]);
            with_item(id, |i| i.folders[0].files.iter_mut().for_each(|f| f.size = GB as i64));
        }
        let (ours, client, look) = (ours("ss-sk14-"), slskd.client(), Look::roomy().free_bytes(5 * GB));
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().len(), 2);
        for id in &ids[2..] {
            let i = item(id).unwrap();
            assert!(i.state == State::Approved && i.batch.is_none() && i.failed.is_empty(), "{i:?}");
            assert!(i.why.as_deref().is_some_and(|w| w.starts_with("Too little free space")), "{:?}", i.why);
        }
        // Downloaded, waiting for its check: still weighed.
        slskd.release(&batch_of(ids[0]));
        follow_round(&ours, &client, &d).await;
        assert_eq!(item(ids[0]).unwrap().state, State::Checking);
        for id in &ids[2..] {
            with_item(id, |i| i.due = 0);
        }
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().len(), 2, "its files still on the disk");
        // Placed: the next starts at the next look, and the fourth still waits.
        check_until_idle(&ours, &client, &d, &places_in(&d), &|_, _| std::future::ready(Ok(0.97))).await;
        assert_eq!(item(ids[0]).unwrap().state, State::Placed);
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users().len(), 3);
        assert_eq!(item(ids[2]).unwrap().state, State::Fetching);
        assert!(item(ids[3]).unwrap().why.as_deref().is_some_and(|w| w.starts_with("Too little free space")));
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.4 (b): below the disk floor no user's download starts (AL03.2), each saying the server is short of space; an
    /// administrator's tick goes; space again, all of them go at once, no cap.
    #[tokio::test]
    async fn below_the_floor_no_users_download_starts() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk14f");
        let slskd = Scripted::new().hold_all(1.0);
        let alices = ["ss-sk14f-a1", "ss-sk14f-a2", "ss-sk14f-a3"];
        for (n, id) in alices.iter().chain(&["ss-sk14f-r1", "ss-sk14f-r2"]).enumerate() {
            picked(id, &[&format!("peer-f{n}")]);
            with_item(id, |i| i.record.request = if n < 3 { 101 + n as i64 } else { 198 + n as i64 });
        }
        let (ours, client) = (ours("ss-sk14f-"), slskd.client());
        start_downloads(&ours, &client, &d, &Look::roomy().below_floor(&[101, 102, 103])).await;
        assert_eq!(slskd.post_users(), ["peer-f3", "peer-f4"], "root's two");
        for id in alices {
            let i = item(id).unwrap();
            assert!(i.state == State::Approved && i.why.as_deref() == Some(SHORT_OF_SPACE) && i.failed.is_empty() && i.batch.is_none(), "{i:?}");
        }
        space_again();
        start_downloads(&ours, &client, &d, &Look::roomy()).await;
        assert_eq!(slskd.posts().len(), 5, "all three at once");
        assert!(alices.iter().all(|id| item(id).unwrap().state == State::Fetching));
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.5 (kind 3): a user gone offline mid-transfer fails that record alone: its transfers dropped, its folder removed, the
    /// record to its next folder at once; the user's turn free, so their other record goes by its own try; another download untouched.
    #[tokio::test]
    async fn a_user_gone_offline_fails_that_record_alone() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk15");
        let slskd = Scripted::new().hold_all(5.0).writes_to(&d, None);
        picked("ss-sk15-1", &["peer-a", "peer-b"]);
        picked("ss-sk15-2", &["peer-c"]);
        picked("ss-sk15-3", &["peer-a"]);
        let (ours, client, look) = (ours("ss-sk15-"), slskd.client(), Look::roomy());
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-c"]);
        let (b1, b2) = (batch_of("ss-sk15-1"), batch_of("ss-sk15-2"));
        std::fs::create_dir_all(d.join(crate::slskd::FOLDER).join(&b1)).unwrap();
        slskd.at(&b1, 40.0);
        follow_round(&ours, &client, &d).await;
        slskd.error(&b1, "Remote connection closed");
        settle(&ours, &client, &d, &look).await;
        let one = item("ss-sk15-1").unwrap();
        assert_eq!((one.failed.clone(), one.state, one.folders[0].user.as_str()), (vec!["peer-a".to_string()], State::Fetching, "peer-b"));
        assert!(slskd.deleted(&b1) && !d.join(crate::slskd::FOLDER).join(&b1).exists(), "its transfers dropped, its folder removed");
        assert_eq!(slskd.post_users(), ["peer-a", "peer-c", "peer-b", "peer-a"], "ss-1 to peer-b at once, ss-3's turn on peer-a");
        let three = item("ss-sk15-3").unwrap();
        assert!(three.failed.is_empty() && three.state == State::Fetching);
        slskd.at(&b2, 70.0);
        follow_round(&ours, &client, &d).await;
        let two = item("ss-sk15-2").unwrap();
        assert_eq!((two.state, two.percent, two.batch.as_deref()), (State::Fetching, 70.0, Some(b2.as_str())), "untouched");
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        // peer-a still offline: ss-3's own try is refused, and peer-a failed for it by that try alone.
        let slskd = Scripted::new().hold_all(5.0);
        picked("ss-sk15v-1", &["peer-a", "peer-b"]);
        picked("ss-sk15v-3", &["peer-a"]);
        let (ours, client) = (self::ours("ss-sk15v-"), slskd.client());
        start_downloads(&ours, &client, &d, &look).await;
        slskd.error(&batch_of("ss-sk15v-1"), "Remote connection closed");
        slskd.then_offline("peer-a");
        follow_round(&ours, &client, &d).await;
        assert!(item("ss-sk15v-3").unwrap().failed.is_empty(), "not failed before it asked");
        settle(&ours, &client, &d, &look).await;
        assert_eq!(item("ss-sk15v-3").unwrap().failed, ["peer-a"], "failed by its own try");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.6 (kind 3), one way of leaving: alice's four requests leave while ss-q1's POST is being tried again in the lane, ss-q2
    /// downloads, ss-q3 waits for its check and ss-q4 is being checked (its first file under way); `arrived`: each record comes into
    /// Incoming through Lidarr instead (X1).
    async fn four_stages_leave(tag: &str, arrived: bool) {
        let d = data_folder(tag);
        let places = places_in(&d);
        let slskd = Scripted::new().hold_all(10.0).writes_to(&d, None).real_clock();
        let client = slskd.client();
        let id = |q: &str| format!("ss-{tag}-{q}");
        for (n, q) in ["q2", "q3", "q4"].iter().enumerate() {
            picked(&id(q), &[&format!("peer-{tag}-{q}")]);
            with_item(&id(q), |i| (i.record.request, i.record.record) = (302 + n as i64, 9_302 + n as i64));
        }
        // bob's record, waiting for peer-q2's other one.
        picked(&id("q5"), &[&format!("peer-{tag}-q2")]);
        with_item(&id("q5"), |i| (i.record.request, i.record.record) = (399, 9_399));
        let prefix = format!("ss-{tag}-");
        let ours = ours(&prefix);
        start_downloads(&ours, &client, &d, &Look::roomy()).await;
        let (b2, b3, b4) = (batch_of(&id("q2")), batch_of(&id("q3")), batch_of(&id("q4")));
        slskd.release(&b4);
        follow_round(&ours, &client, &d).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        slskd.release(&b3);
        follow_round(&ours, &client, &d).await;
        picked(&id("q1"), &[&format!("peer-{tag}-q1")]);
        with_item(&id("q1"), |i| (i.record.request, i.record.record) = (301, 9_301));
        slskd.then_busy(3);
        let gate = HeardInTurn::new().block();
        let g = gate.gate();
        let q1 = id("q1");
        let only_q1 = |i: &Item| i.id == q1;
        let leave = async {
            while gate.calls().is_empty() || !slskd.posts().iter().any(|p| p.user.ends_with("-q1")) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            // The round: the requests taken back (or their records arrived), their items off the list, their fetches stopped.
            let mine: Vec<Item> = ["q1", "q2", "q3", "q4"].iter().map(|q| item(&id(q)).unwrap()).collect();
            let mut k = Kept { items: mine.clone(), ..Kept::default() };
            let approved: Vec<crate::userdata::Request> = if arrived { (301..=304).map(|r| asked(r, None)).collect() } else { Vec::new() };
            let of: Vec<_> = if arrived { (9_301..=9_304).map(had).collect() } else { Vec::new() };
            let stopped = take_requests(&mut k, &approved, &of, &[]);
            assert!(k.items.is_empty(), "all four off the list");
            kept(|q| q.items.retain(|i| !mine.iter().any(|m| m.id == i.id)));
            for it in &stopped {
                cancel_fetch(&client, &d, it).await;
            }
            gate.let_all_go();
            stopped
        };
        let roomy = Look::roomy();
        let (stopped, (), ()) = tokio::join!(leave, check_until_idle(&ours, &client, &d, &places, &g), start_downloads(&only_q1, &client, &d, &roomy));
        assert_eq!(stopped.len(), 4, "each one's fetch stopped");
        assert!(stopped.iter().all(|i| i.failed.is_empty() && i.local_failures == 0), "nobody failed");
        // ss-q1: no batch taken; the start looked inside the lane and found it gone.
        assert!(slskd.batches_of(&format!("peer-{tag}-q1")).is_empty() && slskd.posts().iter().filter(|p| p.user.ends_with("-q1")).all(|p| p.status == 429));
        // ss-q2: its transfers dropped, its folder gone; its follower stops at its next read.
        assert!(slskd.deleted(&b2) && !d.join(crate::slskd::FOLDER).join(&b2).exists());
        let reads = slskd.reads_of(&b2);
        assert!(matches!(follow_step(&id("q2"), &client, &d, Duration::ZERO).await, Followed::Over));
        assert!(slskd.reads_of(&b2) <= reads + 1, "no more reads after the next");
        // ss-q3: its files removed, never heard; ss-q4: the file under way finished, nothing more heard, nothing placed or set aside.
        assert!(!d.join(crate::slskd::FOLDER).join(&b3).exists());
        let heard = gate.calls();
        assert!(!heard.iter().any(|c| c.0 == b3), "ss-q3's files never heard");
        assert_eq!(heard.iter().filter(|c| c.0 == b4).count(), 1, "ss-q4's file under way finished, no other heard");
        assert!(!holds_anything(&d.join("incoming")) && !holds_anything(&d.join("data").join(SET_ASIDE)), "nothing placed, nothing set aside");
        assert!(["q1", "q2", "q3", "q4"].iter().all(|q| item(&id(q)).is_none()));
        // Any record waiting for those users goes at once.
        start_downloads(&ours, &client, &d, &Look::roomy()).await;
        assert_eq!(item(&id("q5")).unwrap().state, State::Fetching);
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.6 (kind 3): a request taken back, or its record arriving, meanwhile stops its record at every stage (OQ-SK2 (a)).
    #[tokio::test]
    async fn a_request_that_leaves_stops_its_record_at_every_stage() {
        four_stages_leave("sk16", false).await;
        four_stages_leave("sk16x", true).await;
    }

    /// SK1.7 (kind 3): slskd restarts with many under way. (a) It answers nothing a while: every download holds, its batch kept,
    /// nobody failed, all followed again once it answers. (b) It forgot b1; (c) b2's transfers ended "Application shut down": each
    /// fetched again from the same folder under a new batch, through the lane, nobody failed, `local_failures` 1; ss-5 still waits for
    /// peer-a. (d) Forgotten a second time in a row: peer-a failed for ss-1 as ever, its next folder fetched.
    #[tokio::test]
    async fn a_slskd_restart_blames_no_user_and_fetches_each_again_once() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk17");
        let slskd = Scripted::new().hold_all(30.0);
        picked("ss-sk17-1", &["peer-a", "peer-d"]);
        picked("ss-sk17-2", &["peer-b"]);
        picked("ss-sk17-3", &["peer-c"]);
        picked("ss-sk17-5", &["peer-a"]);
        let (ours, client, look) = (ours("ss-sk17-"), slskd.client(), Look::roomy());
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-b", "peer-c"]);
        let (b1, b2, b3) = (batch_of("ss-sk17-1"), batch_of("ss-sk17-2"), batch_of("ss-sk17-3"));
        // (a)
        let away = slskd_gone().await;
        for id in ["ss-sk17-1", "ss-sk17-2", "ss-sk17-3"] {
            follow_one(id, &away, &d).await;
            let i = item(id).unwrap();
            assert!(i.state == State::Fetching && i.batch.is_some() && i.failed.is_empty() && i.due > now(), "{id} held, its batch kept: {i:?}");
        }
        for id in ["ss-sk17-1", "ss-sk17-2", "ss-sk17-3"] {
            with_item(id, |i| i.due = 0);
        }
        follow_round(&ours, &client, &d).await;
        assert!(["ss-sk17-1", "ss-sk17-2", "ss-sk17-3"].iter().all(|id| item(id).unwrap().percent == 30.0), "followed again");
        // (b) and (c)
        slskd.forget(&b1);
        slskd.error(&b2, "Application shut down");
        settle(&ours, &client, &d, &look).await;
        for (id, was) in [("ss-sk17-1", &b1), ("ss-sk17-2", &b2)] {
            let i = item(id).unwrap();
            assert!(i.failed.is_empty() && i.local_failures == 1 && i.state == State::Fetching && i.batch.as_ref() != Some(was) && i.chosen == Some(0), "{id}: {i:?}");
        }
        assert_eq!(slskd.post_users()[3..], ["peer-a", "peer-b"], "again, one after another");
        assert_eq!(batch_of("ss-sk17-3"), b3);
        assert_eq!(item("ss-sk17-5").unwrap().state, State::Approved, "ss-5 waits for ss-1's new batch");
        // (d)
        slskd.forget(&batch_of("ss-sk17-1"));
        settle(&ours, &client, &d, &look).await;
        let one = item("ss-sk17-1").unwrap();
        assert_eq!((one.failed.clone(), one.state, one.folders[0].user.as_str()), (vec!["peer-a".to_string()], State::Fetching, "peer-d"));
        assert_eq!(item("ss-sk17-5").unwrap().state, State::Fetching, "peer-a free: ss-5 goes");
        assert!(slskd.never_overlapped() && slskd.violations().is_empty(), "{:?}", slskd.violations());
        std::fs::remove_dir_all(&d).ok();
    }

    /// The review of part 2, M1: an id kept and never answered (a stop between keeping it and its POST) is sent again only where a
    /// new one would be: below the floor a user's waits (AL03.2), short of room it holds (X4); one slskd has is followed as ever.
    #[tokio::test]
    async fn a_kept_batch_sent_again_waits_below_the_floor_and_for_room() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("m1");
        let slskd = Scripted::new().hold_all(5.0);
        let client = slskd.client();
        picked("ss-m1-floor", &["peer-a"]);
        with_item("ss-m1-floor", |i| (i.record.request, i.batch) = (501, Some(crate::slskd::new_batch())));
        start_downloads(&ours("ss-m1-floor"), &client, &d, &Look::roomy().below_floor(&[501])).await;
        let i = item("ss-m1-floor").unwrap();
        assert!(slskd.posts().is_empty(), "no transfer of a user's below the floor");
        assert_eq!((i.state, i.why.as_deref()), (State::Approved, Some(SHORT_OF_SPACE)));
        picked("ss-m1-room", &["peer-b"]);
        with_item("ss-m1-room", |i| i.batch = Some(crate::slskd::new_batch()));
        start_downloads(&ours("ss-m1-room"), &client, &d, &Look::roomy().free_bytes(1)).await;
        assert!(slskd.posts().is_empty(), "nothing sent short of room");
        assert!(item("ss-m1-room").unwrap().why.as_deref().is_some_and(|w| w.starts_with("Too little free space")));
        // One slskd took before the stop: read back and followed, the floor holding nothing under way.
        picked("ss-m1-going", &["peer-c"]);
        with_item("ss-m1-going", |i| i.record.request = 503);
        assert!(start_it("ss-m1-going", &client, &d).await);
        with_item("ss-m1-going", |i| i.state = State::Approved);
        start_downloads(&ours("ss-m1-going"), &client, &d, &Look::roomy().below_floor(&[503])).await;
        assert_eq!(item("ss-m1-going").unwrap().state, State::Fetching);
        assert_eq!(slskd.posts().len(), 1, "never sent twice");
        std::fs::remove_dir_all(&d).ok();
    }

    /// The review of part 2's note for SK1.6: an item taken off the list while its POST is in flight (slskd taking it, its answer not
    /// back yet): its batch is let go once slskd answers, never left running unfollowed.
    #[tokio::test]
    async fn a_batch_in_flight_when_its_item_leaves_is_let_go() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk16-flight");
        let arrived = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = arrived.clone();
        let slskd = Scripted::new().slow(300).hold_all(5.0).on_post(move |_| seen.store(true, std::sync::atomic::Ordering::SeqCst));
        let client = slskd.client();
        picked("ss-sk16f-q0", &["peer-q0"]);
        let leave = async {
            while !arrived.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let gone = kept(|k| take_off(&mut k.items, |i| i.id == "ss-sk16f-q0"));
            cancel_fetch(&client, &d, &gone[0]).await;
            gone[0].batch.clone().unwrap()
        };
        let (roomy, mine) = (Look::roomy(), ours("ss-sk16f-"));
        let (batch, ()) = tokio::join!(leave, start_downloads(&mine, &client, &d, &roomy));
        assert!(item("ss-sk16f-q0").is_none());
        assert_eq!(slskd.batches_of("peer-q0"), [batch.clone()], "slskd took it");
        assert!(slskd.deleted(&batch), "and it was let go, not left running");
        std::fs::remove_dir_all(&d).ok();
    }

    /// The review of part 2, M2 (SK2.2 (d)'s reads beside the follow): a 429 answering a search's state or its answers is slskd busy:
    /// read again, never a miss, never "429" in a why.
    #[tokio::test]
    async fn a_busy_read_of_a_search_is_no_miss() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let found = serde_json::json!([{ "username": "peer-s", "hasFreeUploadSlot": true, "queueLength": 0, "uploadSpeed": 100, "files": [
            { "filename": r"x\Air\Moon Safari\01 One.flac", "size": 1000, "length": 100 }, { "filename": r"x\Air\Moon Safari\02 Two.flac", "size": 1000, "length": 200 } ] }]);
        // Every rung of the ladder meets them (three rungs here).
        let slskd = Scripted::new().answers(found.as_array().unwrap().clone()).responses_busy(5);
        waiting_with("ss-m2-search", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
        search_through("ss-m2-search", &slskd.client()).await;
        let i = item("ss-m2-search").unwrap();
        assert_eq!((i.state, i.misses, i.why.clone()), (State::Found, 0, None), "{i:?}");
    }

    /// An item found at `found_at` by request `request` (record 9_000 + `request`), with folders of `users`, each of record 9_000's
    /// two tracks, exact, a free slot where told; on the list as `id`, waiting for a pick.
    fn found(id: &str, users: &[(&str, bool)], found_at: i64, request: i64) {
        listed(id, State::Found, None);
        with_item(id, |i| {
            i.folders = users.iter().map(|(u, slot)| Folder { free_slot: *slot, ..two_tracks(u) }).collect();
            (i.chosen, i.found_at, i.record.request, i.record.record) = (None, found_at, request, 9_000 + request);
        });
    }

    /// An administrator's tick of `id`'s folder `folder`, as the page sends it.
    fn tick(id: &str, folder: usize) {
        kept(|k| approve_in(&mut k.items, &[Pick { id: id.into(), folder: Some(folder), ..Default::default() }])).unwrap();
    }

    /// SK3.1: a found record whose best folder is certain starts by itself at the look its search wakes: one POST, for that folder's
    /// files; the item fetching from folder 0, marked Cratarr's pick, no tick asked for; then checked and placed in Incoming.
    #[tokio::test]
    async fn a_certain_record_starts_by_itself() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk31");
        let slskd = Scripted::new().hold_all(5.0).writes_to(&d, None);
        found("ss-sk31", &[("peer-a", true), ("peer-b", true)], now(), 3101);
        with_item("ss-sk31", |i| (i.record.artist, i.record.album, i.record.year) = ("Glass Harbour".into(), "Tide Tables".into(), Some(2012)));
        let (mine, client) = (ours("ss-sk31"), slskd.client());
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        let i = item("ss-sk31").unwrap();
        assert_eq!((i.state, i.chosen, i.auto), (State::Fetching, Some(0), true));
        assert_eq!(slskd.post_users(), ["peer-a"]);
        assert_eq!(slskd.posts()[0].files, [r"d\01 One.flac", r"d\02 Two.flac"], "peer-a's files, in the list's order");
        slskd.release(&batch_of("ss-sk31"));
        follow_round(&mine, &client, &d).await;
        check_until_idle(&mine, &client, &d, &places_in(&d), &|_, _| std::future::ready(Ok(0.97))).await;
        let i = item("ss-sk31").unwrap();
        assert_eq!((i.state, i.auto), (State::Placed, true));
        assert!(d.join("incoming").join("Glass Harbour").join("Tide Tables (2012)").join("01 One.flac").exists(), "in Incoming");
        assert!(slskd.violations().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.2: the uncertain ones wait for an administrator's pick: only fuller folders, folders found by titles alone, a best FLAC with
    /// no free slot (an exact MP3 with one second), the best folder's user failed, no timed list. None starts; each stays found,
    /// tickable; root's tick of (a) is fetched as today, not Cratarr's pick.
    #[tokio::test]
    async fn uncertain_records_wait_for_a_pick() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk32");
        let slskd = Scripted::new().hold_all(5.0);
        let t = now();
        found("ss-sk32-a", &[("peer-a", true)], t, 3201);
        with_item("ss-sk32-a", |i| (i.folders[0].extra, i.folders[0].unchecked) = (2, Some("Takes 2 of 4 files".into())));
        found("ss-sk32-b", &[("peer-b", true)], t, 3202);
        with_item("ss-sk32-b", |i| i.folders[0].unchecked = Some(NO_LENGTHS.into()));
        found("ss-sk32-c", &[("peer-c", false), ("peer-c2", true)], t, 3203);
        with_item("ss-sk32-c", |i| (i.folders[0].queue, i.folders[1].format, i.folders[1].lossless) = (12, "mp3".into(), false));
        found("ss-sk32-d", &[("peer-d", true), ("peer-d2", true)], t, 3204);
        with_item("ss-sk32-d", |i| i.failed = vec!["peer-d".into()]);
        found("ss-sk32-e", &[("peer-e", true)], t, 3205);
        with_item("ss-sk32-e", |i| i.tracks.clear());
        let (mine, client) = (ours("ss-sk32-"), slskd.client());
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        assert!(slskd.posts().is_empty(), "none starts");
        for x in ["a", "b", "c", "d", "e"] {
            let i = item(&format!("ss-sk32-{x}")).unwrap();
            assert!(i.state == State::Found && !i.auto && i.chosen.is_none() && !i.folders.is_empty(), "{x}: {i:?}");
        }
        tick("ss-sk32-a", 0);
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        let a = item("ss-sk32-a").unwrap();
        assert_eq!((a.state, a.auto, slskd.post_users()), (State::Fetching, false, vec!["peer-a".to_string()]));
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.3 (kind 6): "certain" is today's judging and nothing new: (d), PAIR01's case, first: two tracks titled "Untitled", 180 s and
    /// 181 s, and two such files: each paired once.
    #[test]
    fn certain_is_todays_judging() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let t0 = 1_800_000_000;
        let judged = |tracks: Vec<Track>, folders: Vec<Folder>| Item { tracks, folders, found_at: t0, state: State::Found, ..found_twice("ss-sk33") };
        // (d)
        let untitled = vec![track(1, "Untitled", 180), track(2, "Untitled", 181)];
        let paired = folders_of(&[answer("peer-u", true, vec![file(r"u\01 Untitled.flac", 180), file(r"u\02 Untitled.flac", 181)])], &untitled, &[]);
        assert_eq!(paired.len(), 1);
        assert_eq!(paired[0].files.iter().map(|f| f.filename.as_str()).collect::<Vec<_>>(), [r"u\01 Untitled.flac", r"u\02 Untitled.flac"], "each track its own file, each file once");
        assert_eq!(certain(&judged(untitled, paired), t0), Some(0));
        // (a) an exact folder.
        let two = vec![track(1, "One", 100), track(2, "Two", 200)];
        assert_eq!(certain(&judged(two.clone(), folders_of(&[exact_folder("peer-a")], &two, &[])), t0), Some(0));
        // (b) a 14-track counting list (the shortest list kept for the record has 10): whole by the list it is judged by.
        let fourteen: Vec<Track> = (1..=14).map(|n| track(n, &format!("Song {n}"), 100 + n as i64)).collect();
        let whole = answer("peer-b", true, (1..=14).map(|n| file(&format!(r"b\{n:02} Song {n}.flac"), 100 + n as i64)).collect());
        assert_eq!(certain(&judged(fourteen.clone(), folders_of(&[whole], &fourteen, &[])), t0), Some(0));
        // (c) track 3 2.5 s off: never a candidate.
        let three = vec![track(1, "One", 100), track(2, "Two", 200), Track { length_ms: Some(302_500), ..track(3, "Three", 0) }];
        let off = answer("peer-c", true, vec![file(r"c\01 One.flac", 100), file(r"c\02 Two.flac", 200), file(r"c\03 Three.flac", 300)]);
        assert!(folders_of(&[off], &three, &[]).is_empty());
        // (e) a 0.16 folder: no unchecked words, `extra` 2.
        let mut kept_016 = folders_of(&[exact_folder("peer-e")], &two, &[]);
        kept_016[0].extra = 2;
        assert_eq!(certain(&judged(two.clone(), kept_016), t0), None);
        // G2 (the owner's rules): a folder paired by title alone, slskd giving its files no length, is never certain.
        let unlengthed = answer("peer-n", true, vec![Offered { length: None, ..file(r"n\01 One.flac", 100) }, Offered { length: None, ..file(r"n\02 Two.flac", 200) }]);
        let by_title = folders_of(&[unlengthed], &two, &[]);
        assert_eq!(by_title.len(), 1);
        assert_eq!(certain(&judged(two.clone(), by_title), t0), None);
        // Never by titles alone; never with fewer files than tracks.
        let titled = untimed_folders(&[titled_folder("peer-t")], &[no_length(1, "One"), no_length(2, "Two")], &[]);
        assert_eq!(certain(&judged(two.clone(), titled), t0), None);
        let mut short = folders_of(&[exact_folder("peer-s")], &two, &[]);
        short[0].files.pop();
        assert_eq!(certain(&judged(two, short), t0), None);
    }

    /// SK3.4 (OQ-SK3): a free slot counts from a search within the hour. Of four found items with certain best folders, the one found
    /// ten minutes ago starts; the one found three hours ago and 0.16's (no found time) are searched again, Search now's path; the
    /// one with no free slot waits for its pick, not searched again by this rule.
    #[tokio::test]
    async fn a_stale_free_slot_is_looked_at_again_before_a_start() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk34");
        let slskd = Scripted::new().hold_all(5.0);
        let t0 = now();
        found("ss-sk34-1", &[("peer-1", true)], t0 - 3 * 3600, 3401);
        found("ss-sk34-2", &[("peer-2", true)], t0 - 600, 3402);
        found("ss-sk34-3", &[("peer-3", true)], 0, 3403);
        found("ss-sk34-4", &[("peer-4", false)], t0 - 3 * 3600, 3404);
        start_downloads(&ours("ss-sk34-"), &slskd.client(), &d, &Look::roomy().at(t0)).await;
        assert_eq!(slskd.post_users(), ["peer-2"]);
        for id in ["ss-sk34-1", "ss-sk34-3"] {
            let i = item(id).unwrap();
            assert!(i.state == State::Waiting && i.due == 0 && i.folders.is_empty() && i.tracks.is_empty() && !i.auto, "{id} searched again: {i:?}");
        }
        let four = item("ss-sk34-4").unwrap();
        assert_eq!((four.state, four.folders.len()), (State::Found, 1));
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.5 (kind 6): room and the disk floor hold automatic starts, never a tick and never a download under way. Over her room,
    /// alice's waits saying "Waits for room", and starts once she is under; below the floor nothing starts by itself, root's included,
    /// each saying "Waits for space"; root's ticks meanwhile go as today; space again, every one waiting starts.
    #[tokio::test]
    async fn room_and_the_floor_hold_automatic_starts() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        use super::super::requests::{WAITS_FOR_ROOM, WAITS_FOR_SPACE};
        let d = data_folder("sk35");
        let slskd = Scripted::new().hold_all(5.0);
        let t = now();
        found("ss-sk35-alice", &[("peer-al", true)], t, 3501);
        found("ss-sk35-bob", &[("peer-b", true)], t, 3502);
        found("ss-sk35-root", &[("peer-r", true)], t, 3503);
        let (mine, client) = (ours("ss-sk35-"), slskd.client());
        start_downloads(&mine, &client, &d, &Look::roomy().full(&[3501])).await;
        assert_eq!(slskd.post_users(), ["peer-b", "peer-r"]);
        let alice = item("ss-sk35-alice").unwrap();
        assert_eq!((alice.state, alice.why.as_deref(), alice.failed.len(), alice.misses), (State::Found, Some(WAITS_FOR_ROOM), 0, 0));
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        assert_eq!(item("ss-sk35-alice").unwrap().state, State::Fetching, "under her room again: at the next look");
        // Below the floor: second certain items of each.
        found("ss-sk35-alice2", &[("peer-al2", true)], t, 3504);
        found("ss-sk35-bob2", &[("peer-b2", true)], t, 3505);
        found("ss-sk35-root2", &[("peer-r2", true)], t, 3506);
        let below = Look::roomy().below_floor(&[3501, 3502, 3504, 3505]);
        start_downloads(&mine, &client, &d, &below).await;
        assert_eq!(slskd.posts().len(), 3, "nothing starts by itself below the floor");
        for id in ["ss-sk35-alice2", "ss-sk35-bob2", "ss-sk35-root2"] {
            let i = item(id).unwrap();
            assert_eq!((i.state, i.why.as_deref()), (State::Found, Some(WAITS_FOR_SPACE)), "{id}");
        }
        assert!(["ss-sk35-alice", "ss-sk35-bob", "ss-sk35-root"].iter().all(|id| item(id).unwrap().state == State::Fetching), "under way: never stopped");
        // Root ticks alice's and his own meanwhile: his goes, hers is held by the floor (AL03.2).
        tick("ss-sk35-alice2", 0);
        tick("ss-sk35-root2", 0);
        start_downloads(&mine, &client, &d, &below).await;
        assert_eq!(item("ss-sk35-root2").unwrap().state, State::Fetching);
        assert_eq!(item("ss-sk35-alice2").unwrap().why.as_deref(), Some(SHORT_OF_SPACE));
        // Space again: every one waiting starts, no cap; room never holds a tick.
        space_again();
        start_downloads(&mine, &client, &d, &Look::roomy().full(&[3504])).await;
        assert!(["ss-sk35-alice2", "ss-sk35-bob2"].iter().all(|id| item(id).unwrap().state == State::Fetching));
        assert_eq!(slskd.posts().len(), 6);
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.6 (kind 6): Lidarr goes first, five minutes ahead: no search, so no automatic start, before then (`first_search`); none for
    /// a record had or a request no longer open, read at the start itself; once neither, it starts.
    #[tokio::test]
    async fn automatic_starts_keep_lidarrs_five_minutes() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let t0 = 1_800_000_000;
        let want = crate::userdata::Request { decided_at: Some(t0), fetched: Some("Lidarr added it and is looking for it".into()), ..asked(1, None) };
        assert_eq!([first_search(&want, None, true), first_search(&want, Some(t0 + 7 * 86_400), true)], [t0 + 300, t0 + 7 * 86_400]);
        let mut k = Kept::default();
        take_requests(&mut k, std::slice::from_ref(&want), &[of(3601)], &[first_search(&want, None, true)]);
        assert_eq!((k.items[0].state, k.items[0].due), (State::Waiting, t0 + 300), "not searched before Lidarr's five minutes");
        let d = data_folder("sk36");
        let slskd = Scripted::new().hold_all(5.0);
        found("ss-sk36", &[("peer-a", true)], now(), 3602);
        let (mine, client) = (ours("ss-sk36"), slskd.client());
        start_downloads(&mine, &client, &d, &Look::roomy().had(&[9_000 + 3602])).await;
        start_downloads(&mine, &client, &d, &Look::roomy().open(&[])).await;
        assert!(slskd.posts().is_empty(), "had, or its request gone: none");
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        assert_eq!(slskd.post_users(), ["peer-a"]);
        // G6 (the owner's rules): found early (an administrator's Search now), no automatic pick before Lidarr's five minutes are up.
        let t = now();
        found("ss-sk36-early", &[("peer-e", true)], t, 3603);
        let early = ours("ss-sk36-early");
        start_downloads(&early, &client, &d, &Look::roomy().not_before(3603, t + 300)).await;
        assert_eq!(item("ss-sk36-early").unwrap().state, State::Found, "not before Lidarr's five minutes");
        start_downloads(&early, &client, &d, &Look::roomy().not_before(3603, t + 300).at(t + 301)).await;
        assert_eq!(item("ss-sk36-early").unwrap().state, State::Fetching);
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.7: after a user fails, Cratarr's pick goes on by itself only where the next folder is certain; else back to found, waiting
    /// for a pick, no longer Cratarr's. An administrator's tick goes to its next folder as today.
    #[test]
    fn after_a_failure_cratarrs_pick_goes_on_only_where_certain() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let t = now();
        let picked_as = |id: &str, users: &[(&str, bool)], auto: bool| {
            found(id, users, t, 3700);
            with_item(id, |i| (i.state, i.chosen, i.auto) = (State::Approved, Some(0), auto));
            item(id).unwrap()
        };
        let mut one = picked_as("ss-sk37-1", &[("peer-a", true), ("peer-b", true)], true);
        on_failed(&mut one, "peer-a", "peer-a is offline".into());
        assert_eq!((one.state, one.auto, one.chosen, one.folders[0].user.as_str()), (State::Approved, true, Some(0), "peer-b"));
        let mut two = picked_as("ss-sk37-2", &[("peer-c", true), ("peer-d", false)], true);
        on_failed(&mut two, "peer-c", "1 of its files failed the check; set aside".into());
        assert_eq!((two.state, two.auto, two.chosen, two.folders[0].user.as_str()), (State::Found, false, None, "peer-d"));
        let mut three = picked_as("ss-sk37-3", &[("peer-e", true), ("peer-f", false)], false);
        on_failed(&mut three, "peer-e", "peer-e is offline".into());
        assert_eq!((three.state, three.auto, three.chosen, three.folders[0].user.as_str()), (State::Approved, false, Some(0), "peer-f"));
    }

    /// SK3.8: March 2026's lesson holds. Switched off, or its request not open, nothing starts by itself; an automatic download's
    /// every file is gated, AcoustID asked where the list has recordings; a wrong recording sets its folder aside and fails its user;
    /// nothing goes anywhere but the Incoming library given.
    #[tokio::test]
    async fn automatic_starts_keep_march_2026s_lesson() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk38");
        let slskd = Scripted::new().hold_all(5.0).writes_to(&d, None);
        found("ss-sk38", &[("peer-a", true)], now(), 3801);
        let (mine, client) = (ours("ss-sk38"), slskd.client());
        start_downloads(&mine, &client, &d, &Look::roomy().off()).await;
        start_downloads(&mine, &client, &d, &Look::roomy().open(&[3800])).await;
        assert!(slskd.posts().is_empty(), "switched off, or its request not open: nothing");
        start_downloads(&mine, &client, &d, &Look::roomy()).await;
        assert!(item("ss-sk38").unwrap().auto);
        slskd.release(&batch_of("ss-sk38"));
        follow_round(&mine, &client, &d).await;
        let heard = Heard { other: vec!["02 Two.flac".into()], ..Default::default() };
        check_until_idle(&mine, &client, &d, &places_in(&d), &|f, t| heard.gate(f, t)).await;
        assert_eq!(heard.asked().len(), 2, "every file gated, AcoustID asked of each");
        let i = item("ss-sk38").unwrap();
        assert_eq!((i.failed.clone(), i.set_aside.len(), i.state), (vec!["peer-a".to_string()], 1, State::Waiting));
        assert!(!holds_anything(&d.join("incoming")), "nothing placed");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.9 (kind 4): Cratarr's pick meets a person's once. (a) The first pick wins, one batch, whichever came first; (b) Search now
    /// leaves one picked as it is, and searches one found again, nothing starting from its old folders; (c) off the list first, no
    /// POST. (d) is `two_ticks_at_once_the_first_wins_and_a_gone_source_is_refused`.
    #[tokio::test]
    async fn cratarrs_pick_meets_a_persons_once() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk39");
        let slskd = Scripted::new().hold_all(5.0);
        let (client, look, t) = (slskd.client(), Look::roomy(), now());
        found("ss-sk39-a", &[("peer-a", true), ("peer-b", true)], t, 3901);
        kept(|k| pick_certain(k, &|i: &Item| i.id == "ss-sk39-a", &look));
        tick("ss-sk39-a", 1);
        found("ss-sk39-a2", &[("peer-c", true), ("peer-d", true)], t, 3902);
        tick("ss-sk39-a2", 1);
        start_downloads(&ours("ss-sk39-a"), &client, &d, &look).await;
        let (a, a2) = (item("ss-sk39-a").unwrap(), item("ss-sk39-a2").unwrap());
        assert_eq!(((a.chosen, a.auto), (a2.chosen, a2.auto)), ((Some(0), true), (Some(1), false)), "the first pick wins");
        assert_eq!(slskd.post_users(), ["peer-a", "peer-d"], "one batch each");
        found("ss-sk39-b", &[("peer-e", true)], t, 3903);
        kept(|k| pick_certain(k, &ours("ss-sk39-b"), &look));
        with_item("ss-sk39-b", search_again);
        assert_eq!(item("ss-sk39-b").unwrap().state, State::Approved, "picked: as it is");
        found("ss-sk39-b2", &[("peer-f", true)], t, 3904);
        with_item("ss-sk39-b2", search_again);
        found("ss-sk39-c", &[("peer-g", true)], t, 3905);
        assert!(dropped("ss-sk39-c").is_some());
        start_downloads(&ours("ss-sk39-"), &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-d", "peer-e"], "nothing from old folders, nothing off the list");
        assert_eq!(item("ss-sk39-b2").unwrap().state, State::Waiting);
        std::fs::remove_dir_all(&d).ok();
    }

    /// G3 (OQ-SK1's own words, "lossless first (then bitrate)"): a folder's bitrate ranks right after lossless, before a free slot and
    /// a queue, for an empty tick and Cratarr's pick alike.
    #[test]
    fn a_folders_bitrate_ranks_right_after_lossless() {
        let two = vec![track(1, "One", 100), track(2, "Two", 200)];
        let at = |user: &str, kbps: i64, slot: bool| {
            let f = |n: u32, title: &str, secs: i64| Offered { bit_rate: Some(kbps), ..file(&format!(r"{user}\0{n} {title}.mp3"), secs) };
            answer(user, slot, vec![f(1, "One", 100), f(2, "Two", 200)])
        };
        let found = folders_of(&[at("free-192", 192, true), at("queued-320", 320, false)], &two, &[]);
        assert_eq!(found.iter().map(|f| f.user.as_str()).collect::<Vec<_>>(), ["queued-320", "free-192"]);
        let flac = answer("flac", false, vec![file(r"f\01 One.flac", 100), file(r"f\02 Two.flac", 200)]);
        assert_eq!(folders_of(&[at("free-320", 320, true), flac], &two, &[])[0].user, "flac", "lossless still first");
    }

    /// G9 (the owner's rules): an automatic pick not yet sent (its user's turn taken) is held at its start as at its pick, by the
    /// floor (anyone's) and its asker's room: back to found, saying why, no longer Cratarr's pick.
    #[tokio::test]
    async fn an_automatic_pick_not_yet_sent_waits_for_the_floor_and_room() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        use super::super::requests::{WAITS_FOR_ROOM, WAITS_FOR_SPACE};
        let d = data_folder("g9");
        let slskd = Scripted::new().hold_all(5.0);
        let client = slskd.client();
        let t = now();
        found("ss-g9-busy", &[("peer-a", true)], t, 4901);
        tick("ss-g9-busy", 0);
        found("ss-g9-auto", &[("peer-a", true)], t, 4902);
        found("ss-g9-room", &[("peer-a", true)], t, 4903);
        start_downloads(&ours("ss-g9-"), &client, &d, &Look::roomy()).await;
        assert!(["ss-g9-auto", "ss-g9-room"].iter().all(|id| item(id).is_some_and(|i| i.state == State::Approved && i.auto)), "picked, waiting for peer-a");
        start_downloads(&ours("ss-g9-"), &client, &d, &Look::roomy().below_floor(&[])).await;
        let a = item("ss-g9-auto").unwrap();
        assert_eq!((a.state, a.auto, a.why.as_deref()), (State::Found, false, Some(WAITS_FOR_SPACE)));
        start_downloads(&ours("ss-g9-"), &client, &d, &Look::roomy().full(&[4903])).await;
        let r = item("ss-g9-room").unwrap();
        assert_eq!((r.state, r.auto, r.why.as_deref()), (State::Found, false, Some(WAITS_FOR_ROOM)));
        assert_eq!(slskd.posts().len(), 1);
        std::fs::remove_dir_all(&d).ok();
    }

    /// The record-0 guard (Part 4's map): an item 0.16 kept with no record yet, taken up before the round gives it its record, waits
    /// for it a while, never Stuck a day for want of it; nor is it picked by Cratarr.
    #[tokio::test]
    async fn an_item_with_no_record_yet_is_neither_stuck_by_a_search_nor_picked() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let it = Item { state: State::Waiting, due: 0, record: Record { record: 0, ..found_twice("ss-r0").record }, ..found_twice("ss-r0") };
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        kept(|k| k.items.push(it.clone()));
        assert!(!read_its_list("ss-r0", &it, &cat, never_read).await);
        let after = item("ss-r0").unwrap();
        assert!(after.state == State::Waiting && after.due <= now() + 60, "it waits for its record, never Stuck a day: {after:?}");
        let mut k = Kept::default();
        k.items.push(Item { state: State::Found, found_at: now(), ..it });
        assert!(pick_certain(&mut k, &|_: &Item| true, &Look::roomy()).is_empty());
    }

    /// The review of part 3, H2: a look that could not read the server (its requests, the had rule, the floor) picks nothing by itself,
    /// and starts no pick of Cratarr's not yet sent: its own picks fail closed. A person's tick still goes.
    #[tokio::test]
    async fn a_look_that_could_not_read_the_server_picks_nothing() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("h2");
        let slskd = Scripted::new().hold_all(5.0);
        let client = slskd.client();
        let t = now();
        found("ss-h2-a", &[("peer-a", true)], t, 5101);
        found("ss-h2-b", &[("peer-b", true)], t, 5102);
        tick("ss-h2-b", 0);
        found("ss-h2-c", &[("peer-c", true)], t, 5103);
        with_item("ss-h2-c", |i| (i.state, i.chosen, i.auto) = (State::Approved, Some(0), true));
        start_downloads(&ours("ss-h2-"), &client, &d, &Look::roomy().unread()).await;
        assert_eq!(item("ss-h2-a").unwrap().state, State::Found, "no automatic pick");
        assert_eq!(item("ss-h2-c").unwrap().state, State::Approved, "Cratarr's pick not sent");
        assert_eq!(slskd.post_users(), ["peer-b"], "the tick goes");
        assert!(!Look::of_disks().read, "a look of the disks alone has read nothing of the server");
        std::fs::remove_dir_all(&d).ok();
    }

    /// The record whose real audio the gate cases hold: its artist, title, year and request 11, its list with recordings r-1 … r-3.
    fn case_item(id: &str, batch: &str, users: &[&str]) {
        use crate::fixtures::gate::TIDE_TABLES as REC;
        tide_tables(id, State::Fetching, Some(batch), users);
        with_item(id, |i| i.tracks = REC.tracks.iter().enumerate().map(|(n, (title, secs))| Track { disc: 1, position: n as u32 + 1, title: title.to_string(), length_ms: Some(*secs as i64 * 1000), recording: Some(format!("r-{}", n + 1)) }).collect());
    }

    /// slskd's folder holding a gate case's real files under `batch`, as `fetched_to` holds stand-ins.
    fn fetched_from(case: &str, batch: &str) -> PathBuf {
        let from = crate::fixtures::gate::gate_cases_once().join(case).join(crate::fixtures::gate::TIDE_TABLES.folder());
        let d = data_folder(&format!("case-{batch}"));
        let at = d.join(crate::slskd::FOLDER).join(batch);
        std::fs::create_dir_all(&at).unwrap();
        for e in std::fs::read_dir(&from).unwrap().flatten() {
            std::fs::copy(e.path(), at.join(e.file_name())).unwrap();
        }
        d
    }

    /// Soulseek's own gate (faults refusing, notes passing, its notes kept for the check), AcoustID stood in: it knows each file of
    /// `names` as its recording, and each question is noted in `asked`.
    fn knowing(asked: &Arc<Mutex<Vec<String>>>, names: &[(&str, &str)]) -> impl Fn(PathBuf, ReleaseTrack) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<f32>> + Send>> + use<> {
        let asked = asked.clone();
        let names: BTreeMap<String, String> = names.iter().map(|(f, r)| (f.to_string(), r.to_string())).collect();
        move |file: PathBuf, track: ReleaseTrack| {
            let (asked, names) = (asked.clone(), names.clone());
            Box::pin(async move {
                let name = file.file_name().unwrap().to_string_lossy().into_owned();
                let recording = names.get(&name).cloned().unwrap_or_default();
                let lookup = move |_: crate::acoustid::Print| {
                    asked.lock().unwrap().push(name);
                    std::future::ready(Ok(vec![crate::acoustid::Found { score: 0.97, recordings: vec![crate::acoustid::Recording { id: recording }] }]))
                };
                let (score, notes) = super::super::track_fetch::gate_track_noted(&file, &track, lookup).await?;
                note_heard(&file, notes);
                Ok(score)
            })
        }
    }

    const CASE_NAMES: [(&str, &str); 3] = [("01 Slack Water.flac", "r-1"), ("02 Neap.flac", "r-2"), ("03 Spring Tide.flac", "r-3")];

    /// SK6.1's AcoustID's half: a certain folder's real files pass every check, AcoustID asked of each; passed (its checked files
    /// kept, as only the library a unit test has not set up stops it); nobody failed, nothing set aside, no notes.
    #[tokio::test]
    async fn a_certain_folders_files_pass_every_check_and_acoustid_is_asked_of_each() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let (id, batch) = ("ss-g1", "b-g1");
        case_item(id, batch, &["peer-a"]);
        let d = fetched_from(crate::fixtures::gate::CLEAN, batch);
        let asked = Arc::new(Mutex::new(Vec::new()));
        fetch_from(id, &slskd_at(all_in()).await, &d, knowing(&asked, &CASE_NAMES)).await;
        let i = item(id).unwrap();
        assert_eq!(asked.lock().unwrap().len(), 3, "AcoustID asked of each file");
        assert!(i.failed.is_empty() && i.set_aside.is_empty() && i.checked.as_deref() == Some(batch) && i.notes.is_empty(), "{i:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK6.2's AcoustID's half: a FLAC walled at 16 kHz is refused, each file in the fault's words, before AcoustID is asked; through
    /// the workers its user is failed and the item goes back to its search.
    #[tokio::test]
    async fn a_transcode_is_refused_before_acoustid_is_asked() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let (id, batch) = ("ss-g2", "b-g2");
        case_item(id, batch, &["peer-a"]);
        let d = fetched_from(crate::fixtures::gate::WALLED, batch);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let gate = knowing(&asked, &CASE_NAMES);
        let it = item(id).unwrap();
        let why = check(&d.join(crate::slskd::FOLDER).join(batch), &it.folders[0], &release_to_check(&it), &gate).await.unwrap().unwrap_err();
        assert_eq!(why.values().collect::<Vec<_>>(), ["Not lossless: it stops at 16 kHz"; 3], "{why:?}");
        assert!(asked.lock().unwrap().is_empty(), "AcoustID asked nothing");
        fetch_from(id, &slskd_at(all_in()).await, &d, gate).await;
        let i = item(id).unwrap();
        assert_eq!((i.failed.clone(), i.state, i.folders.len()), (vec!["peer-a".to_string()], State::Waiting, 0), "back to its search");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK6.3's AcoustID's half: clipping, a click and a pop at the very end pass every check, AcoustID still asked of every file;
    /// passed, nobody failed, nothing set aside; the item keeps its notes, by file.
    #[tokio::test]
    async fn notes_pass_every_check_and_acoustid_is_still_asked() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let (id, batch) = ("ss-g3", "b-g3");
        case_item(id, batch, &["peer-a"]);
        let d = fetched_from(crate::fixtures::gate::NOTES, batch);
        let asked = Arc::new(Mutex::new(Vec::new()));
        fetch_from(id, &slskd_at(all_in()).await, &d, knowing(&asked, &CASE_NAMES)).await;
        let i = item(id).unwrap();
        assert_eq!(asked.lock().unwrap().len(), 3, "AcoustID asked of every file, notes or not");
        assert!(i.failed.is_empty() && i.set_aside.is_empty() && i.checked.as_deref() == Some(batch), "{i:?}");
        assert_eq!(i.notes.get("02 Neap.flac").map(String::as_str), Some("Clipped at 0:01"));
        assert_eq!(i.notes.get("03 Spring Tide.flac").map(String::as_str), Some("A click at 0:01; a pop at the very end"));
        assert!(!i.notes.contains_key("01 Slack Water.flac"));
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK3.10, the request's word (`words_in`): a found item waiting for room or space reads Waiting with that why for its asker; one
    /// Cratarr picked reads Fetching, as any picked one.
    #[test]
    fn a_certain_item_waiting_for_room_is_waiting_for_its_asker() {
        let mut waits = found_twice("ss-sk310-a");
        (waits.record.request, waits.why) = (3101, Some(super::super::requests::WAITS_FOR_ROOM.into()));
        let mut picked = found_twice("ss-sk310-b");
        (picked.record.request, picked.record.record, picked.state, picked.chosen, picked.auto) = (3102, 9_701, State::Approved, Some(0), true);
        let words = words_in(&[waits, picked], &[(3101, None), (3102, None)]);
        assert_eq!(words.get(&3101), Some(&Word::Waiting { why: Some("Waits for room".into()) }));
        assert!(matches!(words.get(&3102), Some(Word::Fetching { .. })), "{words:?}");
    }

    /// The review's L7: an item's notes are its last checked folder's: a new batch, or a search again, leaves none of an earlier
    /// folder's.
    #[tokio::test]
    async fn notes_go_when_a_new_batch_starts_or_the_record_is_searched_again() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("l7");
        let slskd = Scripted::new().hold_all(5.0);
        let earlier = || [("02 Neap.flac".to_string(), "Clipped at 0:01".to_string())].into_iter().collect::<BTreeMap<_, _>>();
        picked("ss-l7-a", &["peer-a"]);
        with_item("ss-l7-a", |i| i.notes = earlier());
        start_downloads(&ours("ss-l7-a"), &slskd.client(), &d, &Look::roomy()).await;
        assert!(item("ss-l7-a").unwrap().notes.is_empty(), "a new batch: none of the earlier folder's notes");
        found("ss-l7-b", &[("peer-b", true)], now(), 5201);
        with_item("ss-l7-b", |i| i.notes = earlier());
        with_item("ss-l7-b", search_again);
        assert!(item("ss-l7-b").unwrap().notes.is_empty(), "searched again: none either");
        std::fs::remove_dir_all(&d).ok();
    }

    /// SK1.8 (kind 4): two picks of one user at the same moment, root's tick of ss-2 and Cratarr's pick of ss-1, start one download:
    /// the other waits for that user's other record, and goes once its transfers are done. Variant: root ticks ss-1 itself as Cratarr
    /// picks it: picked once, one batch.
    #[tokio::test]
    async fn two_picks_of_one_user_at_once_start_one_download() {
        let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let d = data_folder("sk18");
        let slskd = Scripted::new().hold_all(5.0);
        let (client, look, t) = (slskd.client(), Look::roomy(), now());
        found("ss-sk18-1", &[("peer-a", true)], t, 1801);
        found("ss-sk18-2", &[("peer-a", false)], t, 1802);
        tick("ss-sk18-2", 0);
        let mine = ours("ss-sk18-");
        start_downloads(&mine, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a"]);
        let two = item("ss-sk18-2").unwrap();
        assert_eq!((two.state, two.batch.is_none()), (State::Approved, true), "waits for peer-a's other record");
        slskd.release(&batch_of("ss-sk18-1"));
        follow_round(&mine, &client, &d).await;
        start_downloads(&mine, &client, &d, &look).await;
        assert_eq!(slskd.post_users(), ["peer-a", "peer-a"]);
        found("ss-sk18v", &[("peer-b", true)], t, 1803);
        kept(|k| {
            pick_certain(k, &ours("ss-sk18v"), &look);
            approve_in(&mut k.items, &[Pick { id: "ss-sk18v".into(), ..Default::default() }]).unwrap();
        });
        start_downloads(&ours("ss-sk18v"), &client, &d, &look).await;
        assert_eq!((slskd.batches_of("peer-b").len(), item("ss-sk18v").unwrap().auto), (1, true), "picked once");
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
        std::fs::remove_dir_all(&d).ok();
    }

    /// RQ01.8 (kind 4). (a) Two administrators tick different sources of one Found item at once: the first wins, as a tick acts on a
    /// Found item alone, and the second changes nothing, from the Soulseek list (by place) as from two stories' sheets (naming their
    /// sources). (b) A tick from a sheet read before a new search changed the folders names its source: one no longer at its place
    /// (another user's folder there, another folder of its user's, or none) is refused, "That source is gone; look again", and nothing
    /// is ticked, nor the other ticks sent with it; one still there is ticked. (c) Search now while it fetches changes nothing: its
    /// fetch keeps its batch and its tick. Never a fetch from another peer than the one ticked.
    #[test]
    fn two_ticks_at_once_the_first_wins_and_a_gone_source_is_refused() {
        let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
        let pick = |id: &str, folder: usize, named: Option<(&str, &str)>| Pick { id: id.into(), folder: Some(folder), user: named.map(|n| n.0.to_string()), folder_name: named.map(|n| n.1.to_string()) };
        // (a)
        for named in [false, true] {
            let mut items = vec![found_twice("ss-a")];
            let at = |user: &'static str| named.then_some((user, "Night Ferry"));
            approve_in(&mut items, &[pick("ss-a", 0, at("peer-a"))]).unwrap();
            approve_in(&mut items, &[pick("ss-a", 1, at("peer-b"))]).unwrap();
            assert_eq!((items[0].state, items[0].chosen), (State::Approved, Some(0)), "the first tick wins (named: {named})");
        }
        // (b) A new search put peer-c's folder where peer-b's was.
        let mut items = vec![found_twice("ss-b"), found_twice("ss-c")];
        items[0].folders[1].user = "peer-c".into();
        let before = items.clone();
        let e = approve_in(&mut items, &[pick("ss-c", 0, Some(("peer-a", "Night Ferry"))), pick("ss-b", 1, Some(("peer-b", "Night Ferry")))]).unwrap_err();
        assert_eq!(e.to_string(), SOURCE_GONE);
        assert_eq!(items, before, "nothing ticked, the other tick neither");
        assert_eq!(approve_in(&mut items, &[pick("ss-b", 0, Some(("peer-a", "Night Ferry (Live)")))]).unwrap_err().to_string(), SOURCE_GONE, "another folder of its user's");
        assert_eq!(approve_in(&mut items, &[pick("ss-b", 5, Some(("peer-a", "Night Ferry")))]).unwrap_err().to_string(), SOURCE_GONE, "none at that place");
        assert_eq!(items, before);
        approve_in(&mut items, &[pick("ss-b", 1, Some(("peer-c", "Night Ferry")))]).unwrap();
        assert_eq!((items[0].state, items[0].chosen, items[0].folders[1].user.as_str()), (State::Approved, Some(1), "peer-c"), "the source still there, ticked");
        // (c) Search now while it fetches.
        let mut i = found_twice("ss-d");
        (i.state, i.chosen, i.batch, i.percent) = (State::Fetching, Some(1), Some("b-rq01-8".into()), 40.0);
        let was = i.clone();
        search_again(&mut i);
        assert_eq!(i, was, "nothing changes: its fetch goes on");
    }

    // ---- TN: Soulseek's searches, fetches and checks read the class (TN.5, TN.6, TN.12, TN.13): a pause holds to its source's
    // deadline and uses no attempt, through the bulk queue's slskd and AcoustID fakes (`track_intake::tests`) on tokio's paused clock.
    // slskd has no pause of its own in 0.17 (round 1.3's lane: a 429 is asked again there, SK2.6): where TN's tests held slskd by its
    // 429, slskd is away (503) for that while instead (`FakeSlskd::away_until`), and its records hold as away holds them.

    use std::collections::VecDeque;

    use crate::app::track_intake_base::testing as bulk;
    use crate::app::turn::{Paused, Turn};
    use crate::catalogue::records::Ref;
    use bulk::{Clock, FakeAcoustid, FakeSlskd, T0};

    /// A folder of the test's own with its list in it (`items`, its `soulseek.json`) and its Soulseek library, on this thread.
    pub fn own_list(name: &str, items: Vec<Item>) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-soulseek-own-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let k = Kept { next: items.len() as u64, items, ..Kept::default() };
        std::fs::write(d.join("soulseek.json"), serde_json::to_vec(&k).unwrap()).unwrap();
        OWN.with(|o| *o.borrow_mut() = Some((d.join("soulseek.json"), None)));
        OWN_LIBRARY.with(|l| *l.borrow_mut() = Some(d.join("incoming")));
        d
    }

    /// The test done: its list and its library kept nowhere, its folder gone.
    pub fn own_done(d: &Path) {
        OWN.with(|o| *o.borrow_mut() = None);
        OWN_LIBRARY.with(|l| *l.borrow_mut() = None);
        std::fs::remove_dir_all(d).ok();
    }

    /// `soulseek.json` as written, its items by id.
    pub fn on_disk(d: &Path) -> HashMap<String, Item> {
        let k: Kept = serde_json::from_slice(&std::fs::read(d.join("soulseek.json")).unwrap()).unwrap();
        k.items.into_iter().map(|i| (i.id.clone(), i)).collect()
    }

    /// "Tide Tables"' list as a record's, each track ten seconds long, its recording as AcoustID's fake knows it.
    pub fn tide_list() -> Vec<Track> {
        [(1, "Slack Water", "rec-slack"), (2, "Neap", "rec-neap"), (3, "Spring Tide", "rec-spring")]
            .into_iter()
            .map(|(n, title, rec)| Track { disc: 1, position: n, title: title.into(), length_ms: Some(10_000), recording: Some(rec.into()) })
            .collect()
    }

    /// `user`'s folder of Glass Harbour's `album`, holding the list's three files.
    fn folder_of(user: &str, album: &str) -> Folder {
        let file = |t: &Track| Offered {
            filename: format!("Glass Harbour\\{album}\\0{} {}.flac", t.position, t.title),
            size: 1000,
            extension: Some("flac".into()),
            bit_rate: None,
            bit_depth: None,
            sample_rate: None,
            length: Some(10),
            is_locked: false,
        };
        let files = tide_list().iter().map(file).collect();
        Folder { user: user.into(), dir: format!("Glass Harbour\\{album}"), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 100, extra: 0, files, unchecked: None }
    }

    /// slskd's answers to a search for `album`: `user`'s folder of it.
    pub fn answers_of(user: &str, album: &str) -> serde_json::Value {
        serde_json::json!([{ "username": user, "files": folder_of(user, album).files, "hasFreeUploadSlot": true, "queueLength": 0, "uploadSpeed": 100 }])
    }

    /// Glass Harbour's `album` on the list as `id`, at `state`, its list read; its record and its request both `record`.
    pub fn tn_item(id: &str, record: i64, album: &str, state: State) -> Item {
        Item {
            id: id.into(),
            record: Record { record, artist: "Glass Harbour".into(), album: album.into(), year: None, request: record },
            state,
            tracks: tide_list(),
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
            ended_at: 0,
            forgotten: 0,
            auto: false,
            found_at: 0,
            notes: BTreeMap::new(),
        }
    }

    /// `i` ticked: its folders `users`' (the first chosen), its batch `batch`.
    pub fn ticked(i: Item, users: &[&str], batch: Option<&str>) -> Item {
        let folders = users.iter().map(|u| folder_of(u, &i.record.album)).collect();
        Item { folders, chosen: Some(0), batch: batch.map(Into::into), ..i }
    }

    /// `album`'s three files as slskd lands a batch of them (`<slskd>/cratarr/<batch>`), each a copy of `tone`.
    pub fn landed(slskd_dir: &Path, batch: &str, album: &str, tone: &Path) {
        let at = slskd_dir.join(crate::slskd::FOLDER).join(batch);
        std::fs::create_dir_all(&at).unwrap();
        for f in folder_of("peer-a", album).files {
            std::fs::copy(tone, at.join(crate::slskd::base_name(&f.filename))).unwrap();
        }
    }

    /// slskd as Soulseek's rounds have it (`track_fetch::slskd_on`): `slskd` while it is switched on, else why not.
    pub async fn ready(slskd: &Slskd) -> Result<Slskd> {
        super::super::track_fetch::slskd_on()?;
        Ok(slskd.clone())
    }

    /// The next item the downloads' or the checks' worker takes up now (`next_of`): one held waits for its time.
    pub fn next_fetch() -> Option<String> {
        let t = now();
        kept(|k| next_of(&k.items, t))
    }

    /// [`fetch_from`] as the downloads' worker takes it, slskd and its completed-downloads folder as `ready` gives them: not ready
    /// (switched off, not set up), the item held as the worker holds it (`not_ready`).
    async fn fetch_in<G, Fut>(id: &str, ready: impl std::future::Future<Output = Result<(Slskd, PathBuf)>>, gate: G)
    where
        G: Fn(PathBuf, ReleaseTrack) -> Fut,
        Fut: std::future::Future<Output = Result<f32>>,
    {
        match ready.await {
            Ok((slskd, folder)) => fetch_from(id, &slskd, &folder, gate).await,
            Err(e) => not_ready(&|i: &Item| i.id == id, &e),
        }
    }

    /// The gate as the server's (`gate_track`, background reading's switch with it), AcoustID asked through `heard` in `turn`.
    pub fn gate_through(turn: Arc<Turn>, heard: Arc<FakeAcoustid>) -> impl Fn(PathBuf, ReleaseTrack) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<f32>>>> {
        move |file, track| {
            let (turn, heard) = (turn.clone(), heard.clone());
            Box::pin(async move {
                let lookup = |print: crate::acoustid::Print| async move { crate::acoustid::lookup_in(&turn, |form| heard.answer(form), "key", &print).await };
                super::super::track_fetch::gate_track_with(&file, &track, lookup).await
            })
        }
    }

    /// The 0.17-shaped list (`soulseek.json`), one item in each state as production's store holds them (Placed, Found, Waiting,
    /// Approved, Fetching, Checking, Stuck), with the users that failed them, folders set aside and failures in a row as such a store
    /// has them.
    fn soulseek_store() -> Vec<Item> {
        let aside = |user: &str| SetAside { user: user.into(), path: format!("soulseek-set-aside/ss-5 {user}"), why: BTreeMap::from([("02 Neap.flac".to_string(), "2 frames would not decode".to_string())]) };
        vec![
            Item { placed: Some("Glass Harbour/Tide Tables".into()), ..ticked(tn_item("ss-1", 9201, "Tide Tables", State::Placed), &["peer-a"], None) },
            Item { chosen: None, ..ticked(tn_item("ss-2", 9202, "Neap Season", State::Found), &["peer-a", "peer-b"], None) },
            Item { misses: 2, failed: vec!["peer-c".into()], ..tn_item("ss-3", 9203, "Harbour Lights", State::Waiting) },
            Item { failed: vec!["peer-d".into()], local_failures: 1, ..ticked(tn_item("ss-4", 9204, "Undertow", State::Approved), &["peer-a", "peer-c"], None) },
            Item { percent: 40.0, set_aside: vec![aside("peer-e")], local_failures: 2, ..ticked(tn_item("ss-5", 9205, "Quiet Lanes", State::Fetching), &["peer-b"], Some("b5")) },
            Item { percent: 100.0, place_failures: 1, ..ticked(tn_item("ss-6", 9206, "Two Rooms", State::Checking), &["peer-a"], Some("b6")) },
            Item { tracks: Vec::new(), why: Some("No list to judge by".into()), due: T0 + 86_400, ..tn_item("ss-7", 9207, "Spring Tide", State::Stuck) },
        ]
    }

    /// TN.6: slskd away until t0+300 (503 to ss-3's search, ss-4's batch and ss-5's reads: slskd has no pause of its own in 0.17), and
    /// AcoustID's code 14 (Retry-After 120) at ss-6's check. In `soulseek.json` as written: ss-3 Waiting with its misses, due a round
    /// on, saying slskd would not search; ss-4 Approved with its folder chosen and every folder, due t0+300; ss-5 Fetching with its
    /// batch, due t0+300; ss-6 at its check with b6's files where they were, due t0+120; the users that failed each, its folders set aside
    /// and its failures in a row as they were; the others untouched. Then ss-6 is gated at AcoustID's deadline, not before, and ss-5
    /// followed again once slskd is back. Never a user failed, a folder set aside, a miss counted, an item Stuck or Waiting that was not,
    /// a DELETE sent or b6's folder removed while slskd is away or AcoustID paused.
    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_uses_no_attempt_and_fails_nothing() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (_pinned, clock) = Clock::at_t0();
        let d = own_list("rate-limit", soulseek_store());
        let slskd_dir = d.join("slskd");
        let tone = bulk::tone(&d);
        landed(&slskd_dir, "b6", "Two Rooms", &tone);
        let fake = Arc::new(FakeSlskd { away_until: T0 + 300, folder: slskd_dir.clone(), file: tone.clone(), ..Default::default() });
        let slskd = bulk::slskd_at("http://192.168.1.20:5053", &fake);
        let heard = Arc::new(FakeAcoustid { limits: Mutex::new(VecDeque::from([true])), said: Some(120), ..Default::default() });
        let gate = gate_through(Arc::new(Turn::new(crate::app::turn::ACOUSTID)), heard.clone());
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        let before = on_disk(&d);

        // When: ss-3 searched, ss-4 fetched, ss-5 followed, ss-6 checked.
        search_in("ss-3", &cat, never_read, ready(&slskd)).await;
        for id in ["ss-4", "ss-5", "ss-6"] {
            fetch_from(id, &slskd, &slskd_dir, &gate).await;
        }

        // Then, in soulseek.json as written.
        let after = on_disk(&d);
        let (ss3, ss4, ss5, ss6) = (&after["ss-3"], &after["ss-4"], &after["ss-5"], &after["ss-6"]);
        assert_eq!((ss3.state, ss3.misses, ss3.due), (State::Waiting, 2, T0 + ROUND_AT_LEAST), "ss-3");
        assert!(ss3.why.as_deref().is_some_and(|w| w.starts_with("slskd would not search: 503")), "{:?}", ss3.why);
        assert_eq!((ss4.state, ss4.chosen, &ss4.folders, ss4.due, ss4.batch.as_deref()), (State::Approved, Some(0), &before["ss-4"].folders, T0 + 300, None), "ss-4");
        assert_eq!((ss5.state, ss5.batch.as_deref(), ss5.due, ss5.percent, ss5.chosen), (State::Fetching, Some("b5"), T0 + 300, 40.0, Some(0)), "ss-5");
        let acoustid_pause = Paused { source: "acoustid", until: T0 + 120 }.to_string();
        assert_eq!((ss6.state, ss6.batch.as_deref(), ss6.due, ss6.why.as_deref()), (State::Checking, Some("b6"), T0 + 120, Some(acoustid_pause.as_str())), "ss-6");
        for id in ["ss-3", "ss-4", "ss-5", "ss-6"] {
            let (a, b) = (&after[id], &before[id]);
            assert_eq!((&a.failed, &a.set_aside, a.local_failures, a.place_failures, a.misses), (&b.failed, &b.set_aside, b.local_failures, b.place_failures, b.misses), "{id}");
        }
        for id in ["ss-1", "ss-2", "ss-7"] {
            assert_eq!(after[id], before[id], "{id} untouched");
        }
        let b6 = slskd_dir.join(crate::slskd::FOLDER).join("b6");
        assert_eq!(files_under(&b6).len(), 3, "b6's files where they were");
        assert_eq!(fake.asked.lock().unwrap().len(), 2 + READS as usize, "slskd asked once for each, away: ss-3's search, ss-4's batch, ss-5's reads in a row; no DELETE");
        assert_eq!(heard.asked.lock().unwrap().len(), 1, "AcoustID asked once: ss-6's first file");

        // AcoustID's deadline: ss-6 gated then, not before, on the files it had; slskd still away, b6's transfers left in its list.
        clock.to(119).await;
        assert_eq!(next_fetch(), None, "nothing taken up before t0+120");
        clock.to(120).await;
        assert_eq!(next_fetch().as_deref(), Some("ss-6"));
        fetch_from("ss-6", &slskd, &slskd_dir, &gate).await;
        assert_eq!(on_disk(&d)["ss-6"].state, State::Placed, "gated after");
        assert_eq!(heard.asked.lock().unwrap().iter().map(|(at, _)| *at).collect::<Vec<_>>(), [T0, T0 + 120, T0 + 120, T0 + 120]);
        assert_eq!(fake.asked.lock().unwrap().len(), 3 + READS as usize, "slskd asked nothing more before t0+300 but b6's transfers, answered 503");

        // slskd back: ss-5 followed again (its batch done meanwhile), on to its check, and placed.
        landed(&slskd_dir, "b5", "Quiet Lanes", &tone);
        clock.to(299).await;
        assert_eq!(next_fetch(), None, "nothing taken up before t0+300");
        clock.to(300).await;
        assert_eq!(next_fetch().as_deref(), Some("ss-5"));
        fetch_from("ss-5", &slskd, &slskd_dir, &gate).await;
        let last = on_disk(&d);
        assert_eq!((last["ss-5"].state, last["ss-5"].failed.len()), (State::Placed, 0), "followed again");
        let asked = fake.asked.lock().unwrap().clone();
        assert!(asked.iter().any(|(m, p, at, _)| m == "GET" && p.ends_with("/batches/b5") && *at == T0 + 300), "{asked:?}");
        assert!(asked.iter().filter(|(m, ..)| m == "DELETE").all(|(_, _, at, _)| *at >= T0 + 300), "no DELETE while slskd is away: {asked:?}");
        assert!(last.values().all(|i| i.failed == before[&i.id].failed), "no user failed");
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        assert!(heard.violations.lock().unwrap().is_empty(), "{:?}", heard.violations.lock().unwrap());
        own_done(&d);
    }

    /// TN.12, Soulseek's half: slskd away until t0+300 (503 to all it is asked: slskd has no pause of its own in 0.17); then, each alone.
    /// Background reading paused: the gate's AcoustID is not asked, the record holds at its check a while (`CHECK_AGAIN`) saying so, no
    /// deadline; a record's list is not read, it waits a round. MusicBrainz switched off: a record's list passes to its next source
    /// (Discogs'); one with MusicBrainz's alone waits a round, away, not Stuck; slskd still away holds the other a round. slskd switched off
    /// (at t0+10): records wait "slskd is switched off" a round or a while, away with no deadline; switched on at t0+100, while slskd is
    /// still away, they are taken up at once and held for slskd away; switched off again and on at t0+301, slskd back, the one waiting on
    /// it is asked at once. None of it moves a miss or fails a user.
    #[tokio::test(start_paused = true)]
    async fn the_switches_are_not_pauses() {
        use crate::app::switches::{BACKGROUND, MUSICBRAINZ as MUSICBRAINZ_ON, PAUSED, SLSKD as SLSKD_ON, TEST_STATE, set_for_tests};
        let _one = TEST_STATE.write().await;
        let (_pinned, clock) = Clock::at_t0();
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        let (both, alone) = (cat.record_for_tests("Harbour Lights", 0), cat.record_for_tests("Neap Season", 0));
        for (record, source, value) in [(both, "musicbrainz", "r-harbour"), (both, "discogs", "2000013"), (alone, "musicbrainz", "r-neap")] {
            assert!(cat.set_reference(&crate::serve::ids::record(record), &Ref::new(source, "release", value), "link").unwrap());
        }
        let items = vec![
            Item { percent: 100.0, ..ticked(tn_item("ss-c", 9301, "Two Rooms", State::Checking), &["peer-a"], Some("b-c")) },
            Item { tracks: Vec::new(), misses: 1, ..tn_item("ss-b", both, "Harbour Lights", State::Waiting) },
            Item { tracks: Vec::new(), ..tn_item("ss-n", alone, "Neap Season", State::Waiting) },
            Item { misses: 2, ..tn_item("ss-s", 9304, "Low Water", State::Waiting) },
            ticked(tn_item("ss-a", 9305, "Undertow", State::Approved), &["peer-a"], None),
            Item { due: T0 + 7 * 86_400, ..tn_item("ss-x", 9306, "Spring Tide", State::Waiting) },
        ];
        let d = own_list("switches", items);
        let slskd_dir = d.join("slskd");
        let tone = bulk::tone(&d);
        landed(&slskd_dir, "b-c", "Two Rooms", &tone);
        let fake = Arc::new(FakeSlskd {
            away_until: T0 + 300,
            found: HashMap::from([("Glass Harbour Spring Tide", answers_of("peer-b", "Spring Tide"))]),
            folder: slskd_dir.clone(),
            file: tone.clone(),
            ..Default::default()
        });
        let slskd = bulk::slskd_at("http://192.168.1.20:5054", &fake);
        let given = slskd.search("Glass Harbour").await.unwrap_err();
        assert!(matches!(crate::app::turn::class(&given), Failure::Away(_)), "the given: slskd away, its 503: {given:#}");
        let heard = Arc::new(FakeAcoustid::default());
        let gate = gate_through(Arc::new(Turn::new(crate::app::turn::ACOUSTID)), heard.clone());
        let slskd_away = |w: Option<&str>| w.is_some_and(|w| w.starts_with("slskd would not search: 503"));
        let away = |e: &anyhow::Error| matches!(crate::app::turn::class(e), Failure::Away(_));
        let with_folder = |slskd: Result<Slskd>| slskd.map(|s| (s, slskd_dir.clone()));

        // Background reading paused: AcoustID not asked at the check, a list not read; each waits a while saying so, no deadline.
        let was = set_for_tests(BACKGROUND, false);
        fetch_from("ss-c", &slskd, &slskd_dir, &gate).await;
        search_in("ss-b", &cat, never_read, ready(&slskd)).await;
        set_for_tests(BACKGROUND, was);
        let held = on_disk(&d);
        let (c, b) = (&held["ss-c"], &held["ss-b"]);
        assert_eq!((c.state, c.batch.as_deref(), c.due, c.why.as_deref()), (State::Checking, Some("b-c"), T0 + CHECK_AGAIN, Some(PAUSED)), "held at its check");
        assert!(heard.asked.lock().unwrap().is_empty(), "AcoustID not asked");
        assert_eq!(files_under(&slskd_dir.join(crate::slskd::FOLDER).join("b-c")).len(), 3, "its files kept");
        assert_eq!((b.state, b.due, b.why.as_deref(), b.tracks.len()), (State::Waiting, T0 + ROUND_AT_LEAST, Some(PAUSED), 0), "no list read");

        // MusicBrainz switched off: Harbour Lights judged by Discogs' list, then held a round for slskd away; Neap Season, with
        // MusicBrainz's list alone, waits a round, away, not Stuck.
        let read = |r: Ref| async move { if r.source == "musicbrainz" { crate::app::references::read_reference(r).await } else { Ok(Some((tide_list(), None, Default::default()))) } };
        let was = set_for_tests(MUSICBRAINZ_ON, false);
        search_in("ss-b", &cat, &read, ready(&slskd)).await;
        search_in("ss-n", &cat, &read, ready(&slskd)).await;
        let off = crate::app::references::read_reference(Ref::new("musicbrainz", "release", "r-neap")).await.unwrap_err();
        set_for_tests(MUSICBRAINZ_ON, was);
        assert!(away(&off), "switched off: away, no deadline");
        let held = on_disk(&d);
        let (b, n) = (&held["ss-b"], &held["ss-n"]);
        assert_eq!((b.state, b.tracks.len(), b.due), (State::Waiting, 3, T0 + ROUND_AT_LEAST), "Discogs' list; held a round for slskd away");
        assert!(slskd_away(b.why.as_deref()), "{:?}", b.why);
        assert_eq!((n.state, n.due, n.why.as_deref()), (State::Waiting, T0 + ROUND_AT_LEAST, Some("MusicBrainz is switched off")), "a round, not Stuck");

        // slskd switched off at t0+10: Low Water and Undertow wait for it, a round and a while, away with no deadline.
        clock.to(10).await;
        let slskd_was = set_for_tests(SLSKD_ON, false);
        let Err(off) = ready(&slskd).await else { panic!("slskd switched off is not ready") };
        assert!(off.is::<crate::app::references::SwitchedOff>() && away(&off), "switched off: away, no deadline");
        search_in("ss-s", &cat, never_read, ready(&slskd)).await;
        fetch_in("ss-a", async { with_folder(ready(&slskd).await) }, &gate).await;
        let held = on_disk(&d);
        let (s, a) = (&held["ss-s"], &held["ss-a"]);
        let switched_off = off.to_string();
        assert_eq!(switched_off, "slskd is switched off");
        assert_eq!((s.state, s.due, s.why.as_deref()), (State::Waiting, T0 + 10 + ROUND_AT_LEAST, Some(switched_off.as_str())), "Low Water");
        assert_eq!((a.state, a.chosen, a.due, a.why.as_deref()), (State::Approved, Some(0), T0 + 10 + SLSKD_AGAIN, Some(switched_off.as_str())), "Undertow");

        // Switched on at t0+100, slskd still away: both taken up at once, and held for slskd away (a round, a while), asked once each.
        clock.to(100).await;
        set_for_tests(SLSKD_ON, true);
        switched_on();
        assert_eq!((next_search().as_deref(), next_fetch().as_deref()), (Some("ss-s"), Some("ss-a")), "taken up at once");
        search_in("ss-s", &cat, never_read, ready(&slskd)).await;
        fetch_in("ss-a", async { with_folder(ready(&slskd).await) }, &gate).await;
        let held = on_disk(&d);
        assert_eq!((held["ss-s"].due, held["ss-a"].due, held["ss-a"].chosen), (T0 + 100 + ROUND_AT_LEAST, T0 + 100 + SLSKD_AGAIN, Some(0)), "held for slskd away");
        assert_eq!(fake.asked.lock().unwrap().len(), 4, "slskd asked once for each while away: the given, ss-b's and ss-s's searches, ss-a's batch");
        for (id, misses) in [("ss-c", 0), ("ss-b", 1), ("ss-n", 0), ("ss-s", 2), ("ss-a", 0)] {
            assert_eq!((held[id].misses, held[id].failed.len(), held[id].set_aside.len()), (misses, 0, 0), "{id}: no miss, no user failed");
        }

        // Switched off again, Spring Tide waiting on it; switched on at t0+301, slskd back: asked at once.
        set_for_tests(SLSKD_ON, false);
        search_in("ss-x", &cat, never_read, ready(&slskd)).await;
        assert_eq!(on_disk(&d)["ss-x"].why.as_deref(), Some(switched_off.as_str()));
        clock.to(301).await;
        set_for_tests(SLSKD_ON, true);
        switched_on();
        assert_eq!(item("ss-x").map(|i| i.due), Some(T0 + 301), "taken up at once");
        search_in("ss-x", &cat, never_read, ready(&slskd)).await;
        set_for_tests(SLSKD_ON, slskd_was);
        assert_eq!(on_disk(&d)["ss-x"].state, State::Found);
        assert_eq!(fake.searches_asked().last().cloned(), Some(("Glass Harbour Spring Tide".to_string(), T0 + 301)));
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        own_done(&d);
    }

    /// TN.13, Soulseek's list: MusicBrainz held a minute (its 503 saying Retry-After 60). At t0+1 Neap Season's list cannot be read:
    /// the record waits to MusicBrainz's deadline, saying so, its misses as they were, not Stuck, and slskd is asked nothing; a record
    /// with no list at all is Stuck (TN.1's class: someone must act). At t0+59 nothing is taken up; at t0+60 the list is read and the
    /// record searched.
    #[tokio::test(start_paused = true)]
    async fn a_paused_musicbrainz_counts_no_try_against_a_list() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let (_pinned, clock) = Clock::at_t0();
        let musicbrainz = Turn::new(crate::app::turn::MUSICBRAINZ);
        assert_eq!(musicbrainz.heard(true, Some(Duration::from_secs(60))), Some(Duration::from_secs(60)), "MusicBrainz's 503, Retry-After 60");
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        let (neap, quiet) = (cat.record_for_tests("Neap Season", 0), cat.record_for_tests("Quiet Lanes", 0));
        assert!(cat.set_reference(&crate::serve::ids::record(neap), &Ref::new("musicbrainz", "release", "r-neap"), "link").unwrap());
        let items = vec![Item { tracks: Vec::new(), misses: 2, ..tn_item("ss-p", neap, "Neap Season", State::Waiting) }, Item { tracks: Vec::new(), ..tn_item("ss-q", quiet, "Quiet Lanes", State::Waiting) }];
        let d = own_list("musicbrainz-paused", items);
        let fake = Arc::new(FakeSlskd { found: HashMap::from([("Glass Harbour Neap Season", answers_of("peer-a", "Neap Season"))]), ..Default::default() });
        let slskd = bulk::slskd_at("http://192.168.1.20:5055", &fake);
        let asked = Mutex::new(Vec::new());
        let read = |r: Ref| {
            let (turn, asked) = (&musicbrainz, &asked);
            async move {
                // As the background asks MusicBrainz, behind the pages, within its patience (OQ-T1 (a)).
                turn.take_within(true, crate::app::musicbrainz::PATIENCE).await?;
                asked.lock().unwrap().push((r.value, crate::app::moment::now_secs()));
                Ok::<Option<crate::app::references::Read>, anyhow::Error>(Some((tide_list(), None, Default::default())))
            }
        };

        clock.to(1).await;
        search_in("ss-p", &cat, &read, ready(&slskd)).await;
        search_in("ss-q", &cat, &read, ready(&slskd)).await;
        let held = on_disk(&d);
        let (p, q) = (&held["ss-p"], &held["ss-q"]);
        let pause = Paused { source: "musicbrainz", until: T0 + 60 }.to_string();
        assert_eq!((p.state, p.misses, p.due, p.why.as_deref(), p.tracks.len()), (State::Waiting, 2, T0 + 60, Some(pause.as_str()), 0), "held to MusicBrainz's deadline, no try");
        assert_eq!((q.state, q.why.as_deref()), (State::Stuck, Some("No list to judge by")), "no list at all");
        assert!(asked.lock().unwrap().is_empty() && fake.asked.lock().unwrap().is_empty(), "nothing asked");
        clock.to(59).await;
        assert_eq!(next_search(), None, "not before its deadline");
        clock.to(60).await;
        assert_eq!(next_search().as_deref(), Some("ss-p"));
        search_in("ss-p", &cat, &read, ready(&slskd)).await;
        assert_eq!(*asked.lock().unwrap(), [("r-neap".to_string(), T0 + 60)]);
        assert_eq!((on_disk(&d)["ss-p"].state, on_disk(&d)["ss-p"].misses), (State::Found, 0));
        assert_eq!(fake.searches_asked(), [("Glass Harbour Neap Season".to_string(), T0 + 60)]);
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        own_done(&d);
    }

    // ---- TN, Soulseek's part two (TN.9, TN.10, TN.16): a restart mid-pause, two accounts' items waiting on one source, and the
    // queues as they were kept. slskd has no pause of its own in 0.17 (round 1.3's lane): where TN held slskd by its 429, slskd is
    // away (503) for that while instead (`FakeSlskd::away_until`), nothing of it kept across a restart (SK2.6)

    /// TN.9's world for one start, slskd at `url`, the server's holds kept in its user data (OQ-T3). Before the stop, at t0: slskd
    /// away until t0+300 (503 to all it is asked; no hold of its own, so none kept), and AcoustID's code 14 to ss-6's first file at its
    /// check (Retry-After 600): ss-6 held at its check to t0+600, b6's files in slskd's folder; ss-w waiting, its round's wait over at
    /// t0+100. The server stops at t0+60 (b6's folder emptied meanwhile where `emptied`) and starts at t0+120, AcoustID's turn made anew
    /// taking up its hold.
    async fn restart_mid_pause(name: &str, url: &str, emptied: bool) {
        use crate::app::turn::{ACOUSTID, class, held_choice};
        let (_pinned, clock) = Clock::at_t0();
        let ud = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let kept_until = |source: &str| ud.import_choice(&held_choice(source)).unwrap().and_then(|v| v.parse::<i64>().ok());
        let items = vec![
            Item { percent: 100.0, ..ticked(tn_item("ss-6", 9406, "Two Rooms", State::Checking), &["peer-a"], Some("b6")) },
            Item { misses: 1, due: T0 + 100, why: Some("No folder holds every track of its release".into()), ..tn_item("ss-w", 9408, "Spring Tide", State::Waiting) },
        ];
        let d = own_list(name, items);
        let slskd_dir = d.join("slskd");
        let tone = bulk::tone(&d);
        landed(&slskd_dir, "b6", "Two Rooms", &tone);
        let b6 = slskd_dir.join(crate::slskd::FOLDER).join("b6");
        let fake = Arc::new(FakeSlskd {
            away_until: T0 + 300,
            found: HashMap::from([("Glass Harbour Spring Tide", answers_of("peer-b", "Spring Tide"))]),
            folder: slskd_dir.clone(),
            file: tone.clone(),
            ..Default::default()
        });
        let heard = Arc::new(FakeAcoustid { limits: Mutex::new(VecDeque::from([true])), said: Some(600), ..Default::default() });
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        // The turns as a start makes them: AcoustID's keeping its hold in the user data, as `turn::keep_all` keeps the server's own;
        // slskd has none (round 1.3's lane).
        let turns = || {
            let acoustid = Arc::new(Turn::new(ACOUSTID));
            acoustid.keep(ud.clone());
            (acoustid, bulk::slskd_at(url, &fake))
        };

        // Before the stop, at t0.
        let (acoustid, slskd) = turns();
        let given = slskd.search("Glass Harbour").await.unwrap_err();
        assert!(matches!(class(&given), Failure::Away(_)), "the given: slskd away, its 503: {given:#}");
        fetch_from("ss-6", &slskd, &slskd_dir, gate_through(acoustid.clone(), heard.clone())).await;
        let before = on_disk(&d);
        let acoustid_pause = Paused { source: "acoustid", until: T0 + 600 }.to_string();
        let ss6 = &before["ss-6"];
        assert_eq!((ss6.state, ss6.batch.as_deref(), ss6.due, ss6.why.as_deref()), (State::Checking, Some("b6"), T0 + 600, Some(acoustid_pause.as_str())), "held at its check");
        assert_eq!([kept_until("slskd"), kept_until("acoustid")], [None, Some(T0 + 600)], "AcoustID's hold kept as it is set; none of slskd's (SK2.6)");

        // The stop at t0+60: nothing of the server kept but its stores. The start at t0+120.
        clock.to(60).await;
        drop((acoustid, slskd));
        OWN.with(|o| o.borrow_mut().as_mut().expect("a test's own list").1 = None);
        if emptied {
            for f in files_under(&b6) {
                std::fs::remove_file(f).unwrap();
            }
        }
        clock.to(120).await;
        restarted();
        sweep_at_start().await;
        let (acoustid, slskd) = turns();
        let gate = gate_through(acoustid.clone(), heard.clone());
        assert_eq!(on_disk(&d), before, "the list read back as it was written: ss-6 still at its check, nothing failed");
        assert_eq!(files_under(&b6).len(), if emptied { 0 } else { 3 }, "b6 and its files where they were: the start took nothing");
        assert_eq!(acoustid.paused_until(), Some(T0 + 600), "AcoustID's hold taken up, never lengthened");
        assert_eq!([kept_until("slskd"), kept_until("acoustid")], [None, Some(T0 + 600)]);

        // t0+120: ss-w's wait over, it meets slskd away and holds a round, no miss. ss-6 waits its time.
        assert_eq!((next_search().as_deref(), next_fetch()), (Some("ss-w"), None));
        search_in("ss-w", &cat, never_read, ready(&slskd)).await;
        let w = &on_disk(&d)["ss-w"];
        assert_eq!((w.state, w.misses, w.due), (State::Waiting, 1, T0 + 120 + ROUND_AT_LEAST), "held a round for slskd away");
        assert!(w.why.as_deref().is_some_and(|w| w.starts_with("slskd would not search: 503")), "{:?}", w.why);
        for at in [200, 299] {
            clock.to(at).await;
            assert_eq!((next_search(), next_fetch()), (None, None), "nothing taken up at t0+{at}");
        }
        assert_eq!(fake.asked.lock().unwrap().len(), 2, "slskd asked nothing before t0+300 but the given and ss-w's search");

        // ss-6 still waits for AcoustID.
        clock.to(599).await;
        assert_eq!(next_fetch(), None, "ss-6 not before AcoustID's deadline");
        assert_eq!(heard.asked.lock().unwrap().iter().map(|(at, _)| *at).collect::<Vec<_>>(), [T0], "AcoustID asked nothing since its code 14");

        // t0+600: ss-6 gated on its files; where they went while the server was down, fetched again from the same folder first.
        clock.to(600).await;
        assert_eq!(next_fetch().as_deref(), Some("ss-6"));
        fetch_from("ss-6", &slskd, &slskd_dir, &gate).await;
        if emptied {
            let gone = &on_disk(&d)["ss-6"];
            assert_eq!((gone.state, gone.batch.as_deref(), gone.chosen, gone.due, gone.why.as_deref()), (State::Approved, None, Some(0), 0, Some(FILES_GONE)), "its checked files were gone: fetched again under its tick");
            assert_eq!(next_fetch().as_deref(), Some("ss-6"));
            fetch_from("ss-6", &slskd, &slskd_dir, &gate).await;
            let asked = fake.asked.lock().unwrap().clone();
            let batches: Vec<(String, i64)> = asked.iter().filter(|(m, p, ..)| m == "POST" && p.ends_with("/batches")).map(|(_, _, at, b)| (b["username"].as_str().unwrap_or_default().to_string(), *at)).collect();
            assert_eq!(batches, [("peer-a".to_string(), T0 + 600)], "the same folder, once");
        }
        let last = on_disk(&d);
        let ss6 = &last["ss-6"];
        assert_eq!((ss6.state, &ss6.failed, &ss6.set_aside), (State::Placed, &before["ss-6"].failed, &before["ss-6"].set_aside), "gated and placed, no user failed");
        assert_eq!(heard.asked.lock().unwrap().iter().map(|(at, _)| *at).collect::<Vec<_>>(), [T0, T0 + 600, T0 + 600, T0 + 600]);
        let asked = fake.asked.lock().unwrap().clone();
        assert_eq!(asked.iter().filter(|(_, _, at, _)| *at < T0 + 300).count(), 2, "while away, slskd asked the given and ss-w's search alone: {asked:?}");

        // ss-w's round over, slskd back: searched.
        clock.to(120 + ROUND_AT_LEAST).await;
        assert_eq!(next_search().as_deref(), Some("ss-w"));
        search_in("ss-w", &cat, never_read, ready(&slskd)).await;
        assert_eq!(on_disk(&d)["ss-w"].state, State::Found);
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        assert!(heard.violations.lock().unwrap().is_empty(), "{:?}", heard.violations.lock().unwrap());
        own_done(&d);
    }

    /// TN.9 (OQ-T3), Soulseek's start: AcoustID held until t0+600, kept in the server's user data; slskd away until t0+300 (no hold of
    /// its own in 0.17, so none kept, SK2.6); ss-6 held at its check to t0+600 with b6's files in slskd's folder. The server stops at
    /// t0+60 and starts at t0+120: AcoustID's hold is taken up as its turn is made again, so nothing is asked of it before t0+600; ss-w,
    /// due meanwhile, meets slskd away and holds a round, no miss; ss-6 is still at its check, b6 and its files kept, nothing failed,
    /// and the start's sweep does not take b6; at t0+600 ss-6 is gated and placed. The variant, b6's folder emptied while the server
    /// was down: at t0+600 ss-6 finds its files gone ("Its checked files were gone", X4) and is fetched again from the same folder, no
    /// user failed. Never a question asked early, a held item failed by the restart, or a kept deadline lengthened.
    #[tokio::test(start_paused = true)]
    async fn a_restart_mid_pause_asks_nothing_early() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        restart_mid_pause("restart-kept", "http://192.168.1.20:5056", false).await;
        restart_mid_pause("restart-emptied", "http://192.168.1.20:5057", true).await;
    }

    /// `user`'s request of `record` (its album `album`, by Glass Harbour), approved at `at`, kept in `ud`: its id.
    fn asked_by(ud: &crate::userdata::UserData, user: &str, record: i64, album: &str, at: i64) -> i64 {
        let r = crate::userdata::Request {
            id: 0,
            user: user.into(),
            album_key: format!("k-{record}"),
            artist_key: "glass harbour".into(),
            release_group: None,
            album: album.into(),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at,
            status: super::super::requests::APPROVED.into(),
            reason: None,
            decided_by: Some("root".into()),
            decided_at: Some(at),
            fetched: None,
            record: Some(record),
        };
        ud.add_request(&r).unwrap()
    }

    /// Glass Harbour's `album` on the list as `id`, at `state`, its list read: of `record`, for `request`.
    fn asked_item(id: &str, record: i64, request: i64, album: &str, state: State) -> Item {
        let mut i = tn_item(id, record, album, state);
        i.record.request = request;
        i
    }

    /// Soulseek's round reading the approved requests as `read_requests` does, Lidarr not connected: each one's record as the
    /// catalogue holds it (Music folder 0, Incoming 1), the list taken by them, and the fetches of the items it took off stopped
    /// through `slskd` and its folder (`take_in`).
    async fn requests_read(ud: &crate::userdata::UserData, cat: &crate::catalogue::Catalogue, slskd: &Slskd, slskd_dir: &Path) {
        use crate::settings::LibraryKind;
        let (approved, of) = requests_taken(ud, cat, &[(0, LibraryKind::Main), (1, LibraryKind::Incoming)]).unwrap();
        let firsts = firsts_of(&approved, &of, false);
        take_in(&approved, &of, &firsts, async { Ok::<_, anyhow::Error>((slskd.clone(), slskd_dir.to_path_buf())) }).await;
    }

    /// TN.10: ann's approved request of "Tide Tables" (record 11) and bob's of "Neap Season" (record 12), each a Soulseek item waiting;
    /// bob's second request, of record 11 too (one item a record, X2); slskd away until t0+120 (503 to all it is asked: no pause of its
    /// own in 0.17). The rounds read the requests and the searcher takes its work: each item meets slskd away and holds a round with
    /// its misses; after it, one search at a time, the longest waiting first. The variant, ann cancelling at t0+60: record 11's item
    /// passes to bob's request with its `due` (a round on) and its misses (X2), record 12's untouched. Never ann's cancel failing nor
    /// bob's item searched early, two items of record 11, or the wait kept against a request (the requests as their accounts left
    /// them).
    #[tokio::test(start_paused = true)]
    async fn two_accounts_items_wait_on_one_pause() {
        use super::super::requests::{APPROVED, CANCELLED};
        let _one = crate::app::switches::TEST_STATE.write().await;
        for (cancels, url) in [(false, "http://192.168.1.20:5058"), (true, "http://192.168.1.20:5059")] {
            let (_pinned, clock) = Clock::at_t0();
            let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
            // Not had: no tracks (a test catalogue's track with no file reads as present).
            let (tide, neap) = (cat.record_for_tests("Tide Tables", 0), cat.record_for_tests("Neap Season", 0));
            let ud = crate::userdata::UserData::open(None).unwrap();
            let ann = asked_by(&ud, "ann", tide, "Tide Tables", T0 - 900);
            let bob = asked_by(&ud, "bob", neap, "Neap Season", T0 - 600);
            let bob_too = asked_by(&ud, "bob", tide, "Tide Tables", T0 - 300);
            let items = vec![
                Item { misses: 1, ..asked_item("ss-11", tide, ann, "Tide Tables", State::Waiting) },
                Item { misses: 2, ..asked_item("ss-12", neap, bob, "Neap Season", State::Waiting) },
            ];
            let d = own_list(&format!("two-accounts-{cancels}"), items);
            let slskd_dir = d.join("slskd");
            let fake = Arc::new(FakeSlskd {
                away_until: T0 + 120,
                found: HashMap::from([("Glass Harbour Tide Tables", answers_of("peer-a", "Tide Tables")), ("Glass Harbour Neap Season", answers_of("peer-b", "Neap Season"))]),
                folder: slskd_dir.clone(),
                ..Default::default()
            });
            let slskd = bulk::slskd_at(url, &fake);
            let given = slskd.search("Glass Harbour").await.unwrap_err();
            assert!(matches!(crate::app::turn::class(&given), Failure::Away(_)), "the given: slskd away, its 503: {given:#}");

            // t0: the round reads the three requests (one item a record: none for bob's second), and the searcher meets slskd away.
            requests_read(&ud, &cat, &slskd, &slskd_dir).await;
            for id in ["ss-11", "ss-12"] {
                assert_eq!(next_search().as_deref(), Some(id), "by due, then id");
                search_in(id, &cat, never_read, ready(&slskd)).await;
            }
            assert_eq!(next_search(), None);
            let held = on_disk(&d);
            assert_eq!(held.len(), 2, "one item a record");
            for (id, record, request, misses) in [("ss-11", tide, ann, 1), ("ss-12", neap, bob, 2)] {
                let i = &held[id];
                assert_eq!((i.record.record, i.record.request, i.state, i.misses, i.due), (record, request, State::Waiting, misses, T0 + ROUND_AT_LEAST), "{id}");
                assert!(i.why.as_deref().is_some_and(|w| w.starts_with("slskd would not search: 503")), "{id}: {:?}", i.why);
            }

            // The variant: ann cancels at t0+60, and the next round reads it.
            if cancels {
                clock.to(60).await;
                assert!(ud.set_request_status(ann, APPROVED, CANCELLED, None, Some("ann")).unwrap(), "ann's cancel goes through");
                requests_read(&ud, &cat, &slskd, &slskd_dir).await;
                let passed = on_disk(&d);
                assert_eq!(passed.len(), 2, "one item a record still");
                let to_bob = Item { record: Record { request: bob_too, ..held["ss-11"].record.clone() }, ..held["ss-11"].clone() };
                assert_eq!(passed["ss-11"], to_bob, "record 11's item passed to bob's request, its due and its misses with it (X2)");
                assert_eq!(passed["ss-12"], held["ss-12"], "record 12's untouched");
                assert_eq!(next_search(), None, "nothing searched early");
            }
            clock.to(ROUND_AT_LEAST - 1).await;
            assert_eq!(next_search(), None, "nothing before their round is over");
            assert_eq!(fake.asked.lock().unwrap().len(), 3, "slskd asked once for each item while away, and the given");

            // A round on, slskd back: one search at a time, the longest waiting first.
            clock.to(ROUND_AT_LEAST).await;
            for id in ["ss-11", "ss-12"] {
                assert_eq!(next_search().as_deref(), Some(id));
                search_in(id, &cat, never_read, ready(&slskd)).await;
            }
            let words = |w: &str, at: i64| (w.to_string(), at);
            assert_eq!(
                fake.searches_asked(),
                [
                    words("Glass Harbour", T0),
                    words("Glass Harbour Tide Tables", T0),
                    words("Glass Harbour Neap Season", T0),
                    words("Glass Harbour Tide Tables", T0 + ROUND_AT_LEAST),
                    words("Glass Harbour Neap Season", T0 + ROUND_AT_LEAST)
                ]
            );
            let asked = fake.asked.lock().unwrap().clone();
            let posted: Vec<usize> = asked.iter().enumerate().filter(|(_, (m, p, ..))| m == "POST" && p.ends_with("/searches")).map(|(n, _)| n).collect();
            let forgot = asked.iter().position(|(m, p, ..)| m == "DELETE" && p.contains("/searches/")).expect("record 11's search forgotten");
            assert!(posted[3] < forgot && forgot < posted[4], "record 11's search over before record 12's begins: {asked:?}");
            let last = on_disk(&d);
            let owner = if cancels { bob_too } else { ann };
            assert_eq!((last["ss-11"].state, last["ss-11"].record.request, last["ss-12"].state), (State::Found, owner, State::Found));
            // The wait on the items alone: each request as its account left it.
            let status = |id: i64| ud.request(id).unwrap().map(|r| (r.status, r.reason, r.fetched));
            assert_eq!(status(ann), Some(((if cancels { CANCELLED } else { APPROVED }).to_string(), None, None)), "ann's");
            for id in [bob, bob_too] {
                assert_eq!(status(id), Some((APPROVED.to_string(), None, None)), "bob's");
            }
            assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
            own_done(&d);
        }
    }

    /// Soulseek's list as production's held it at 0.17's previews (13 items: 7 found, 3 waiting, 3 stuck; 88 tracks; each tied to a
    /// request), built here, none of production's data: the found with their folders, waiting for a tick; the waiting not found yet,
    /// with their misses and their time; the stuck saying why.
    fn production_store(now: i64) -> Vec<Item> {
        let listed = |n: u32| -> Vec<Track> {
            (1..=n).map(|p| Track { disc: 1, position: p, title: format!("Part {p}"), length_ms: Some(180_000 + i64::from(p) * 7_000), recording: Some(format!("rec-{p}")) }).collect()
        };
        let folder = |user: &str, album: &str, tracks: &[Track]| {
            let dir = format!("Music\\Glass Harbour\\{album}");
            let file = |t: &Track| Offered {
                filename: format!("{dir}\\{:02} {}.flac", t.position, t.title),
                size: 30_000_000,
                extension: Some("flac".into()),
                bit_rate: None,
                bit_depth: Some(16),
                sample_rate: Some(44_100),
                length: t.length_ms.map(|ms| ms / 1000),
                is_locked: false,
            };
            let files = tracks.iter().map(file).collect();
            Folder { user: user.into(), dir: dir.clone(), format: "flac".into(), lossless: true, free_slot: true, queue: 0, speed: 1_000_000, extra: 0, files, unchecked: None }
        };
        (0..13_i64)
            .map(|n| {
                let album = format!("Record {}", n + 1);
                let i = tn_item(&format!("ss-{}", n + 1), 9401 + n, &album, State::Found);
                match n {
                    0..=6 => {
                        let tracks = listed(10);
                        Item { folders: vec![folder("peer-a", &album, &tracks), folder("peer-b", &album, &tracks)], tracks, ..i }
                    }
                    7..=9 => {
                        let misses = n as u32 - 5;
                        Item { state: State::Waiting, tracks: listed(6), misses, due: now + wait_after(misses), why: Some("No folder holds every track of its release".into()), ..i }
                    }
                    _ => Item { state: State::Stuck, tracks: Vec::new(), due: now + wait_after(3), why: Some("No list to judge by".into()), ..i },
                }
            })
            .collect()
    }

    /// TN.16, Soulseek's half: FX01's 0.16 `soulseek.json` (5 items: waiting with misses 2 and its time, placed, fetching with its
    /// batch, found, stuck) and a 0.17-shaped store as production's (13 items) read after the turns as they did: each item's state,
    /// misses, due, failed, folders, batch, set_aside and why as they were. A round with every source answering leaves them be (the
    /// found waiting for a tick, the waiting for their time, 0.16's stuck one due to have its list read again as trunk's A1.4 has it,
    /// the 0.17 store's stuck stuck), but for 0.16's fetching item, followed again as today;
    /// 0.16's file is kept aside once as `soulseek.json.016`. The store written again holds every field it had, as it had it, and
    /// none for a pause (the deadline is `due`, `paused_until` a view). Never an item searched, failed or dropped by the change, nor
    /// a file refused.
    #[tokio::test]
    async fn the_queues_read_back_unchanged_after_the_turns() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        // Before the 0.16 waiting item's time: a round as one could find it.
        let at = 1_794_000_000;
        let _pinned = crate::app::turn::Pinned(at);
        crate::app::moment::PINNED.store(at, std::sync::atomic::Ordering::Relaxed);
        const FIELDS: [&str; 25] = [
            "id", "record", "state", "tracks", "untimed", "folders", "chosen", "failed", "misses", "due", "why", "batch", "percent", "placed", "set_aside", "checked",
            "local_failures", "placing", "placed_at", "place_failures", "ended_at", "forgotten", "auto", "found_at", "notes",
        ];
        let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<String>>();
        let stored = |d: &Path| serde_json::from_slice::<serde_json::Value>(&std::fs::read(d.join("soulseek.json")).unwrap()).unwrap();

        // FX01's 0.16 file, as 0.16 left it, read whole.
        let raw = include_str!("../../../../tests/fixtures/0.16/soulseek.json");
        let d = own_list("tn16-016", Vec::new());
        std::fs::write(d.join("soulseek.json"), raw).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(raw).unwrap();
        let had = fixture["items"].as_array().unwrap();
        let read = kept(|k| k.items.clone());
        assert_eq!(read.len(), had.len(), "a 0.16 file read whole");
        assert!(!kept(|k| k.frozen) && queues_set_aside(&d).is_empty(), "not refused");
        assert_eq!(std::fs::read_to_string(d.join("soulseek.json.016")).unwrap(), raw, "kept aside, as today");
        for (i, f) in read.iter().zip(had) {
            let was = (
                f["id"].as_str().unwrap(),
                serde_json::from_value::<State>(f["state"].clone()).unwrap(),
                f["misses"].as_u64().unwrap() as u32,
                f["due"].as_i64().unwrap(),
                serde_json::from_value::<Vec<String>>(f["failed"].clone()).unwrap(),
                serde_json::from_value::<Vec<Folder>>(f["folders"].clone()).unwrap(),
                f["batch"].as_str(),
                serde_json::from_value::<Vec<SetAside>>(f["set_aside"].clone()).unwrap(),
                f["why"].as_str(),
            );
            assert_eq!((i.id.as_str(), i.state, i.misses, i.due, i.failed.clone(), i.folders.clone(), i.batch.as_deref(), i.set_aside.clone(), i.why.as_deref()), was);
        }

        // One round, every source answering: nothing searched (the waiting one's time to come, the found waiting for a tick) but the
        // stuck one, whose time came (A1.4: its list read again); the fetching one followed again, as today, on to its check and its
        // placing.
        let slskd_dir = d.join("slskd");
        let b3 = "00000000-0000-4000-8000-00000000b003";
        let b3_dir = slskd_dir.join(crate::slskd::FOLDER).join(b3);
        std::fs::create_dir_all(&b3_dir).unwrap();
        for name in ["01 - Paper.mp3", "02 - Kites.mp3"] {
            std::fs::write(b3_dir.join(name), b"mp3").unwrap();
        }
        let fake = Arc::new(FakeSlskd { folder: slskd_dir.clone(), ..Default::default() });
        let slskd = bulk::slskd_at("http://192.168.1.20:5061", &fake);
        assert_eq!(next_search().as_deref(), Some("ss-5"), "nothing searched but the stuck one, due (A1.4)");
        assert_eq!(next_fetch().as_deref(), Some("ss-3"), "the fetching one followed again");
        fetch_from("ss-3", &slskd, &slskd_dir, |_, _| std::future::ready(Ok(0.97))).await;
        assert_eq!((next_search().as_deref(), next_fetch()), (Some("ss-5"), None));
        let asked = fake.asked.lock().unwrap().clone();
        assert!(asked.iter().any(|(m, p, ..)| m == "GET" && p.ends_with(&format!("/batches/{b3}"))), "{asked:?}");
        assert!(asked.iter().all(|(m, ..)| m != "POST"), "nothing searched nor fetched anew: {asked:?}");
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        let back = on_disk(&d);
        for i in &read {
            if i.id == "ss-3" {
                let followed = &back["ss-3"];
                assert_eq!((followed.state, &followed.failed, &followed.set_aside), (State::Placed, &i.failed, &i.set_aside), "followed to its end, no user failed");
            } else {
                assert_eq!(&back[&i.id], i, "{} as it was", i.id);
            }
        }
        // Written again: every field it had (ss-3's batch gone with its placing), none for the pause.
        let written = stored(&d);
        assert_eq!(written["next"], fixture["next"]);
        for (w, f) in written["items"].as_array().unwrap().iter().zip(had) {
            let mut was = keys(f);
            if f["id"] == "ss-3" {
                was.remove("batch");
            }
            assert!(was.is_subset(&keys(w)), "{}: every field it had: {:?}", f["id"], was.difference(&keys(w)).collect::<Vec<_>>());
            let other: Vec<String> = keys(w).into_iter().filter(|k| !FIELDS.contains(&k.as_str())).collect();
            assert!(other.is_empty(), "{}: no field for the pause: {other:?}", f["id"]);
        }
        // Kept aside once: a start that reads 0.17's own copies nothing over it.
        load(&d.join("soulseek.json"));
        assert_eq!(std::fs::read_to_string(d.join("soulseek.json.016")).unwrap(), raw, "once");
        own_done(&d);

        // A 0.17-shaped store as production's: read back as written, a round takes nothing of it, and it is written again as it was.
        let store = production_store(at);
        let count = |s: State| store.iter().filter(|i| i.state == s).count();
        assert_eq!((count(State::Found), count(State::Waiting), count(State::Stuck), store.iter().map(|i| i.tracks.len()).sum::<usize>()), (7, 3, 3, 88));
        let d = own_list("tn16-017", store.clone());
        let before = stored(&d);
        assert_eq!(kept(|k| k.items.clone()), store, "read back as written");
        assert_eq!((next_search(), next_fetch()), (None, None), "the found wait for a tick, the waiting for their time, the stuck stay");
        assert_eq!(stored(&d), before, "every field as it had it, none for a pause");
        assert!(!d.join("soulseek.json.016").exists() && queues_set_aside(&d).is_empty(), "0.17's own: nothing kept aside, nothing refused");
        own_done(&d);
    }
}
